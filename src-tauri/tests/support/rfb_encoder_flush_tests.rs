#[test]
fn server_tight_partial_entropy_rectangles_are_complete_before_publication() {
    use crate::remote_desktop_core::{
        ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
        RfbClientFramebufferDecoder,
    };
    let size = DisplaySize {
        width: 664,
        height: 32,
    };
    let rect = FramebufferRect {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
    };
    let geometry = FramebufferGeometry {
        width: size.width,
        height: size.height,
        pixel_format: PixelFormat::Bgra8888,
    };
    let repeat_every = 4;
    let mut random = 0x9876abcd_u32;
    let mut encoder = RfbZlibEncoder::new();
    let mut independent: [_; 4] = std::array::from_fn(|_| Decompress::new(true));
    let mut client = RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut framebuffer = ClientFramebufferCore::default();
    for update in 0..16 {
        let mut source = vec![255; size.width as usize * size.height as usize * 4];
        for (index, pixel) in source.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if index % repeat_every == 0 {
                pixel[..3].copy_from_slice(&[2, 5, 127]);
            } else {
                for channel in &mut pixel[..3] {
                    random ^= random << 13;
                    random ^= random >> 17;
                    random ^= random << 5;
                    *channel = random as u8;
                }
            }
        }
        let mut wire = vec![0, 0, 0, 1];
        write_framebuffer_rect(
            &mut wire,
            &source,
            size,
            &rect,
            default_rfb_pixel_format(),
            RfbFramebufferEncoding::Tight,
            &mut encoder,
        )
        .unwrap();
        let mut offset = 17;
        for index in 0..3 {
            let byte = wire[offset];
            offset += 1;
            if index == 2 || byte & 0x80 == 0 {
                break;
            }
        }
        let compressed = &wire[offset..];
        let inflate = &mut independent[update % 4];
        let before_in = inflate.total_in();
        let before_out = inflate.total_out();
        let mut rgb = vec![0; source.len() / 4 * 3 + 1];
        inflate
            .decompress(compressed, &mut rgb, FlushDecompress::Sync)
            .unwrap();
        assert_eq!(inflate.total_in() - before_in, compressed.len() as u64);
        assert_eq!(
            inflate.total_out() - before_out,
            (source.len() / 4 * 3) as u64,
            "repeat_every={repeat_every} update={update} payload={} tail={:?}",
            compressed.len(),
            &compressed[compressed.len().saturating_sub(8)..]
        );
        for (actual, expected) in rgb[..source.len() / 4 * 3]
            .as_chunks::<3>()
            .0
            .iter()
            .zip(source.as_chunks::<4>().0)
        {
            assert_eq!(*actual, [expected[2], expected[1], expected[0]]);
        }
        client
            .decode_framebuffer_update(&wire, geometry, &mut framebuffer)
            .unwrap();
        assert_eq!(framebuffer.pixels(), source);
    }
}

#[test]
fn server_keeps_zlib_and_zrle_histories_independent_across_encoding_changes() {
    for encodings in [[6_i32, 16, 0, 7, 16, 6], [16, 6, 0, 7, 6, 16]] {
        let size = DisplaySize {
            width: 8,
            height: 2,
        };
        let rect = FramebufferRect {
            x: 0,
            y: 0,
            width: 8,
            height: 2,
        };
        let mut encoder = RfbZlibEncoder::new();
        let mut zlib = Decompress::new(true);
        let mut zrle = Decompress::new(true);
        for (update, encoding) in encodings.into_iter().enumerate() {
            let source: Vec<u8> = (0..16_u8)
                .flat_map(|pixel| [pixel + update as u8, pixel * 3, 200 - pixel, 0])
                .collect();
            // Follow production SetEncodings selection, retaining one connection's state.
            let selected =
                parse_client_encoding_support(&encoding.to_be_bytes()).framebuffer_encoding;
            let mut wire = Vec::new();
            write_framebuffer_rect(
                &mut wire,
                &source,
                size,
                &rect,
                default_rfb_pixel_format(),
                selected,
                &mut encoder,
            )
            .unwrap();
            assert_eq!(&wire[8..12], &encoding.to_be_bytes());
            if !matches!(encoding, 6 | 16) {
                continue;
            }
            let (decoder, expected) = if encoding == 6 {
                (&mut zlib, source)
            } else {
                let mut tiles = vec![0]; // One independent RFC raw tile of CPIXEL BGR.
                tiles.extend(
                    source
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|pixel| pixel[..3].iter().copied()),
                );
                (&mut zrle, tiles)
            };
            let compressed = &wire[16..];
            assert_eq!(
                u32::from_be_bytes(wire[12..16].try_into().unwrap()) as usize,
                compressed.len()
            );
            let before_in = decoder.total_in();
            let before_out = decoder.total_out();
            let mut actual = vec![0; expected.len() + 1];
            decoder
                .decompress(compressed, &mut actual, FlushDecompress::Sync)
                .unwrap_or_else(|error| panic!("encoding={encoding}, update={update}: {error}"));
            assert_eq!(decoder.total_in() - before_in, compressed.len() as u64);
            assert_eq!(decoder.total_out() - before_out, expected.len() as u64);
            assert_eq!(&actual[..expected.len()], expected);
        }
    }
}

#[test]
fn server_compression_flushes_high_entropy_rectangles_before_returning() {
    let mut state = 0x12345678_u32;
    let mut source = Vec::new();
    for _ in 0..65 * 65 {
        let mut pixel = [0; 4];
        for channel in &mut pixel[..3] {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *channel = state as u8;
        }
        source.extend_from_slice(&pixel);
    }
    let mut encoder = RfbZlibEncoder::new();
    let mut decoder = Decompress::new(true);
    for _ in 0..2 {
        let mut wire = Vec::new();
        write_framebuffer_rect(
            &mut wire,
            &source,
            DisplaySize {
                width: 65,
                height: 65,
            },
            &FramebufferRect {
                x: 0,
                y: 0,
                width: 65,
                height: 65,
            },
            default_rfb_pixel_format(),
            RfbFramebufferEncoding::Zlib,
            &mut encoder,
        )
        .unwrap();
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let mut decoded = vec![0; source.len() + 1];
        decoder
            .decompress(&wire[16..], &mut decoded, FlushDecompress::Sync)
            .unwrap();
        assert_eq!(decoder.total_in() - before_in, (wire.len() - 16) as u64);
        assert_eq!(
            decoder.total_out() - before_out,
            source.len() as u64,
            "each compressed rectangle must be complete before the next arrives"
        );
        assert_eq!(&decoded[..source.len()], source);
    }
}

#[test]
fn server_tight_high_entropy_updates_decode_across_persistent_streams() {
    use crate::remote_desktop_core::{
        ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
        RfbClientFramebufferDecoder,
    };
    let size = DisplaySize {
        width: 1920,
        height: 32,
    };
    let geometry = FramebufferGeometry {
        width: size.width,
        height: size.height,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut random = 0x12345678_u32;
    let mut encoder = RfbZlibEncoder::new();
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut framebuffer = ClientFramebufferCore::default();
    for update in 0..20 {
        let mut source = vec![255; size.width as usize * size.height as usize * 4];
        for pixel in source.as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                *channel = random as u8;
            }
        }
        let mut wire = vec![0, 0, 0, 1];
        write_framebuffer_rect(
            &mut wire,
            &source,
            size,
            &FramebufferRect {
                x: 0,
                y: 0,
                width: size.width,
                height: size.height,
            },
            default_rfb_pixel_format(),
            RfbFramebufferEncoding::Tight,
            &mut encoder,
        )
        .unwrap();
        if update == 0 {
            // Independently prove the encoder delivered the entire compressed
            // rectangle, including its flush trailer, before testing our decoder.
            let mut payload_start = 17; // update + rectangle + Tight control
            for index in 0..3 {
                let byte = wire[payload_start];
                payload_start += 1;
                if index == 2 || byte & 0x80 == 0 {
                    break;
                }
            }
            let compressed = &wire[payload_start..];
            let expected = source.len() / 4 * 3;
            let mut independent = Decompress::new(true);
            let mut decoded = vec![0; expected + 1];
            independent
                .decompress(compressed, &mut decoded, FlushDecompress::Sync)
                .unwrap();
            assert_eq!(independent.total_in(), compressed.len() as u64);
            assert_eq!(independent.total_out(), expected as u64);
            for (rgb, bgra) in decoded[..expected]
                .as_chunks::<3>()
                .0
                .iter()
                .zip(source.as_chunks::<4>().0)
            {
                assert_eq!(*rgb, [bgra[2], bgra[1], bgra[0]]);
            }
        }
        decoder
            .decode_framebuffer_update(&wire, geometry, &mut framebuffer)
            .unwrap_or_else(|error| panic!("update {update}: {error}"));
        assert_eq!(
            framebuffer.snapshot().unwrap().pixels(),
            source,
            "update {update}"
        );
    }
}
