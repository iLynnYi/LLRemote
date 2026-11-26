// Protocol vectors intentionally do not use our Client's pixel conversion.
#[test]
fn tight_server_fill_and_small_copy_follow_the_negotiated_pixel_format() {
    let rgb565 = RfbPixelFormat {
        bits_per_pixel: 16,
        depth: 16,
        red_max: 31,
        green_max: 63,
        blue_max: 31,
        red_shift: 11,
        green_shift: 5,
        blue_shift: 0,
        ..default_rfb_pixel_format()
    };
    for (format, first, second) in [
        (
            RfbPixelFormat {
                bits_per_pixel: 8,
                depth: 8,
                red_max: 7,
                green_max: 7,
                blue_max: 3,
                red_shift: 5,
                green_shift: 2,
                blue_shift: 0,
                ..default_rfb_pixel_format()
            },
            vec![5],
            vec![0xff],
        ),
        (rgb565, vec![0x8a, 0x11], vec![0xff, 0xff]),
        (
            RfbPixelFormat {
                big_endian: true,
                ..rgb565
            },
            vec![0x11, 0x8a],
            vec![0xff, 0xff],
        ),
        // Depth 32 disables Tight's special RGB24 representation.
        (
            RfbPixelFormat {
                depth: 32,
                ..default_rfb_pixel_format()
            },
            vec![0x56, 0x34, 0x12, 0],
            vec![0xff, 0xff, 0xff, 0],
        ),
        (
            RfbPixelFormat {
                big_endian: true,
                depth: 32,
                ..default_rfb_pixel_format()
            },
            vec![0, 0x12, 0x34, 0x56],
            vec![0, 0xff, 0xff, 0xff],
        ),
        // RGB24 ignores wire endian and channel shifts when its conditions hold.
        (
            RfbPixelFormat {
                big_endian: true,
                red_shift: 24,
                green_shift: 16,
                blue_shift: 8,
                ..default_rfb_pixel_format()
            },
            vec![0x12, 0x34, 0x56],
            vec![0xff, 0xff, 0xff],
        ),
    ] {
        for solid in [true, false] {
            let mut source = vec![0x56, 0x34, 0x12, 255];
            source.extend_from_slice(if solid {
                &[0x56, 0x34, 0x12, 255]
            } else {
                &[255; 4]
            });
            let mut wire = Vec::new();
            write_framebuffer_rect(
                &mut wire,
                &source,
                DisplaySize {
                    width: 2,
                    height: 1,
                },
                &FramebufferRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                format,
                RfbFramebufferEncoding::Tight,
                &mut RfbZlibEncoder::new(),
            )
            .unwrap();
            let mut expected = vec![if solid { 0x80 } else { 0 }];
            expected.extend_from_slice(&first);
            if !solid {
                expected.extend_from_slice(&second);
            }
            assert_eq!(&wire[12..], expected, "format={format:?}, solid={solid}");
        }
    }
}

#[test]
fn tight_server_rgb565_compresses_at_the_twelve_byte_boundary() {
    let format = RfbPixelFormat {
        bits_per_pixel: 16,
        depth: 16,
        red_max: 31,
        green_max: 63,
        blue_max: 31,
        red_shift: 11,
        green_shift: 5,
        blue_shift: 0,
        ..default_rfb_pixel_format()
    };
    let mut encoder = RfbZlibEncoder::new();
    let mut decoders = std::array::from_fn::<_, 4, _>(|_| Decompress::new(true));
    for pass in 0..8 {
        let mut source = Vec::new();
        let mut expected = Vec::new();
        for pixel in 0..6 {
            let red = (pixel + pass) % 2 == 0;
            source.extend_from_slice(if red {
                &[0, 0, 255, 255]
            } else {
                &[0, 255, 0, 255]
            });
            expected.extend_from_slice(if red { &[0, 0xf8] } else { &[0xe0, 7] });
        }
        let mut wire = Vec::new();
        write_framebuffer_rect(
            &mut wire,
            &source,
            DisplaySize {
                width: 6,
                height: 1,
            },
            &FramebufferRect {
                x: 0,
                y: 0,
                width: 6,
                height: 1,
            },
            format,
            RfbFramebufferEncoding::Tight,
            &mut encoder,
        )
        .unwrap();
        let mut offset = 13;
        let length = read_tight_test_length(&wire, &mut offset);
        assert_eq!(offset + length, wire.len());
        let decoder = &mut decoders[usize::from(wire[12] >> 4)];
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let mut decoded = vec![0; 13];
        decoder
            .decompress(&wire[offset..], &mut decoded, FlushDecompress::Sync)
            .unwrap();
        assert_eq!(decoder.total_in() - before_in, length as u64);
        assert_eq!(decoder.total_out() - before_out, 12);
        assert_eq!(&decoded[..12], expected);
    }
}

#[test]
fn tight_server_splits_a_high_entropy_4k_core_update_before_writing() {
    use crate::remote_desktop_core::{
        ClientFramebufferCore, ClientRfbPixelFormat, FrameInterest, Framebuffer,
        FramebufferGeometry, PixelFormat, RfbClientFramebufferDecoder, ServerFramebufferCore,
    };
    let geometry = FramebufferGeometry {
        width: 3840,
        height: 2160,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut random = 0x12345678_u32;
    let mut source = vec![255; 3840 * 2160 * 4];
    for pixel in source.as_chunks_mut::<4>().0 {
        for channel in &mut pixel[..3] {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            *channel = random as u8;
        }
    }
    let core = ServerFramebufferCore::default();
    core.attach_client("4k").unwrap();
    core.publish(Framebuffer::new(geometry, source).unwrap())
        .unwrap();
    let frame = core
        .next_update("4k", FrameInterest::full())
        .unwrap()
        .unwrap();
    let mut wire = Vec::new();
    write_core_framebuffer_update(
        &mut wire,
        &frame,
        &LocalVncServerFrameCache::default(),
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Tight,
        &mut RfbZlibEncoder::new(),
    )
    .unwrap();
    let count = u16::from_be_bytes([wire[2], wire[3]]);
    assert!(count > 1);
    let mut offset = 4;
    let mut covered = vec![false; 3840 * 2160];
    let mut streams = std::array::from_fn::<_, 4, _>(|_| Decompress::new(true));
    let pixels = frame.framebuffer.shared_pixels();
    for _ in 0..count {
        let field = |i| usize::from(u16::from_be_bytes([wire[offset + i], wire[offset + i + 1]]));
        let (x, y, width, height) = (field(0), field(2), field(4), field(6));
        assert!(width <= 2048 && width > 0 && height > 0);
        assert!(x + width <= 3840 && y + height <= 2160);
        assert_eq!(&wire[offset + 8..offset + 12], &7_i32.to_be_bytes());
        offset += 12;
        let control = wire[offset];
        offset += 1;
        assert_eq!(control & 0xcf, 0); // CopyFilter, no stream resets.
        let length = read_tight_test_length(&wire, &mut offset);
        assert!(length <= 0x3f_ffff);
        let expected = width * height * 3;
        let mut decoded = vec![0; expected + 1];
        let decoder = &mut streams[usize::from(control >> 4)];
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        decoder
            .decompress(
                &wire[offset..offset + length],
                &mut decoded,
                FlushDecompress::Sync,
            )
            .unwrap();
        assert_eq!(decoder.total_in() - before_in, length as u64);
        assert_eq!(decoder.total_out() - before_out, expected as u64);
        for row in 0..height {
            for column in 0..width {
                let index = (y + row) * 3840 + x + column;
                assert!(!std::mem::replace(&mut covered[index], true));
                let source = &pixels[index * 4..index * 4 + 4];
                let rgb = &decoded[(row * width + column) * 3..(row * width + column + 1) * 3];
                assert_eq!(rgb, [source[2], source[1], source[0]]);
            }
        }
        offset += length;
    }
    assert_eq!(offset, wire.len());
    assert!(covered.into_iter().all(|pixel| pixel));
    let mut client = ClientFramebufferCore::default();
    RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32())
        .unwrap()
        .decode_framebuffer_update(&wire, geometry, &mut client)
        .unwrap();
    assert_eq!(client.snapshot().unwrap().pixels(), pixels.as_slice());
}

#[test]
fn tight_server_rejects_an_unsplit_rectangle_before_its_header() {
    let mut wire = Vec::new();
    assert!(write_framebuffer_rect(
        &mut wire,
        &vec![0; 2049 * 4],
        DisplaySize {
            width: 2049,
            height: 1
        },
        &FramebufferRect {
            x: 0,
            y: 0,
            width: 2049,
            height: 1
        },
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Tight,
        &mut RfbZlibEncoder::new()
    )
    .is_err());
    assert!(wire.is_empty());
}

#[test]
fn tight_server_replans_bounds_when_splitting_exceeds_the_wire_rectangle_count() {
    use crate::remote_desktop_core::{
        FrameInterest, Framebuffer, FramebufferGeometry, FramebufferRect as CoreRect, PixelFormat,
        ServerFramebufferCore,
    };
    let geometry = FramebufferGeometry {
        width: 4096,
        height: 600,
        pixel_format: PixelFormat::Bgra8888,
    };
    let core = ServerFramebufferCore::default();
    core.attach_client("count").unwrap();
    core.publish(Framebuffer::new(geometry, vec![0; 4096 * 600 * 4]).unwrap())
        .unwrap();
    let mut frame = core
        .next_update("count", FrameInterest::full())
        .unwrap()
        .unwrap();
    frame.dirty_rectangles = vec![
        CoreRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1
        };
        64_000
    ];
    frame.dirty_rectangles.extend(vec![
        CoreRect {
            x: 0,
            y: 0,
            width: 2049,
            height: 1
        };
        1000
    ]);
    let mut wire = Vec::new();
    write_core_framebuffer_update(
        &mut wire,
        &frame,
        &LocalVncServerFrameCache::default(),
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Tight,
        &mut RfbZlibEncoder::new(),
    )
    .unwrap();
    // 65,000 selected rectangles become 66,000 after width splitting. Bounds
    // must be replanned before writing the count; both resulting fills fit.
    assert_eq!(&wire[..4], &[0, 0, 0, 2]);
    assert_eq!(&wire[4..12], &[0, 0, 0, 0, 8, 0, 0, 1]);
    assert_eq!(&wire[20..28], &[8, 0, 0, 0, 0, 1, 0, 1]);
    assert_eq!(wire.len(), 36);
}

fn read_tight_test_length(bytes: &[u8], offset: &mut usize) -> usize {
    let mut length = 0;
    for index in 0..3 {
        let byte = bytes[*offset];
        *offset += 1;
        length |= usize::from(if index == 2 { byte } else { byte & 0x7f }) << (index * 7);
        if index == 2 || byte & 0x80 == 0 {
            break;
        }
    }
    length
}
