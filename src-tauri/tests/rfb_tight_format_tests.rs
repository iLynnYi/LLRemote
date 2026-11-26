use std::io::Write;
use LLRemoteLib::remote_desktop_core::{
    ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
    RfbClientFramebufferDecoder,
};

fn update(width: u16, height: u16, encoding: i32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 1, 0, 0, 0, 0];
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend(encoding.to_be_bytes());
    bytes.extend(payload);
    bytes
}
fn decode(
    width: u16,
    height: u16,
    encoding: i32,
    format: ClientRfbPixelFormat,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let mut core = ClientFramebufferCore::default();
    RfbClientFramebufferDecoder::new(format)?.decode_framebuffer_update(
        &update(width, height, encoding, payload),
        FramebufferGeometry {
            width: width.into(),
            height: height.into(),
            pixel_format: PixelFormat::Bgra8888,
        },
        &mut core,
    )?;
    Ok(core.pixels().to_vec())
}
fn compressed(bytes: &[u8]) -> Vec<u8> {
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    zlib.write_all(bytes).unwrap();
    let body = zlib.finish().unwrap();
    assert!(body.len() < 128);
    let mut bytes = vec![body.len() as u8];
    bytes.extend(body);
    bytes
}

#[test]
fn tight_32_bit_fill_copy_and_palette_use_tpixel_instead_of_fixed_rgb() {
    for big_endian in [false, true] {
        let format = ClientRfbPixelFormat {
            depth: 32,
            big_endian,
            ..ClientRfbPixelFormat::core_bgra32()
        };
        let pixel = if big_endian {
            [0, 0x12, 0x34, 0x56]
        } else {
            [0x56, 0x34, 0x12, 0]
        };
        for encoding in [7, -260] {
            assert_eq!(
                decode(
                    2,
                    1,
                    encoding,
                    format,
                    &[&[0x80], pixel.as_slice()].concat()
                )
                .unwrap(),
                [0x56, 0x34, 0x12, 255].repeat(2)
            );
        }
        assert_eq!(
            decode(1, 1, 7, format, &[&[0], pixel.as_slice()].concat()).unwrap(),
            [0x56, 0x34, 0x12, 255]
        );
        let mut palette = vec![0x40, 1, 1];
        palette.extend(pixel);
        palette.extend([0; 4]);
        palette.push(0x40);
        assert_eq!(
            decode(2, 1, 7, format, &palette).unwrap(),
            [0x56, 0x34, 0x12, 255, 0, 0, 0, 255]
        );
        let mut payload = vec![0];
        payload.extend(compressed(&pixel.repeat(3)));
        assert_eq!(
            decode(3, 1, 7, format, &payload).unwrap(),
            [0x56, 0x34, 0x12, 255].repeat(3)
        );
    }
    let shifted = ClientRfbPixelFormat {
        big_endian: true,
        red_shift: 24,
        green_shift: 16,
        blue_shift: 8,
        ..ClientRfbPixelFormat::core_bgra32()
    };
    assert_eq!(
        decode(1, 1, 7, shifted, &[0x80, 0x12, 0x34, 0x56]).unwrap(),
        [0x56, 0x34, 0x12, 255]
    );
}

#[test]
fn tight_fill_preserves_stream_reset_bits_and_transaction_order() {
    fn basic(
        encoder: &mut flate2::write::ZlibEncoder<Vec<u8>>,
        stream: u8,
        rgb: [u8; 3],
    ) -> Vec<u8> {
        let offset = encoder.get_ref().len();
        encoder.write_all(&rgb.repeat(4)).unwrap();
        encoder.flush().unwrap();
        let body = &encoder.get_ref()[offset..];
        assert!(body.len() < 128);
        let mut payload = vec![stream << 4, body.len() as u8];
        payload.extend(body);
        update(4, 1, 7, &payload)
    }

    for fill_encoding in [7, -260] {
        let geometry = FramebufferGeometry {
            width: 4,
            height: 1,
            pixel_format: PixelFormat::Bgra8888,
        };
        let mut decoder =
            RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
        let mut core = ClientFramebufferCore::default();
        let mut streams: [_; 4] = std::array::from_fn(|_| {
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast())
        });
        for (stream, encoder) in streams.iter_mut().enumerate() {
            decoder
                .decode_framebuffer_update(
                    &basic(encoder, stream as u8, [1, 2, 3]),
                    geometry,
                    &mut core,
                )
                .unwrap();
        }
        // Reset streams 0 and 2 via Fill; the other two retain their history.
        decoder
            .decode_framebuffer_update(
                &update(4, 1, fill_encoding, &[0x85, 4, 5, 6]),
                geometry,
                &mut core,
            )
            .unwrap();
        for (stream, encoder) in streams.iter_mut().enumerate() {
            if stream % 2 == 0 {
                *encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            }
            decoder
                .decode_framebuffer_update(
                    &basic(encoder, stream as u8, [7, 8, 9]),
                    geometry,
                    &mut core,
                )
                .unwrap();
            assert_eq!(core.pixels(), [9, 8, 7, 255].repeat(4));
        }
        decoder
            .decode_framebuffer_update(
                &update(4, 1, fill_encoding, &[0x8f, 10, 20, 30]),
                geometry,
                &mut core,
            )
            .unwrap();
        for stream in 0..4 {
            let mut payload = vec![stream << 4];
            payload.extend(compressed(&[40, 50, 60].repeat(4)));
            decoder
                .decode_framebuffer_update(&update(4, 1, 7, &payload), geometry, &mut core)
                .unwrap();
            assert_eq!(core.pixels(), [60, 50, 40, 255].repeat(4));
        }

        // CopyRect observes the preceding Fill in the same private update.
        let mut ordered = update(1, 1, fill_encoding, &[0x80, 70, 80, 90]);
        ordered[3] = 2;
        ordered.extend([0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0]);
        let mut failed = ordered.clone();
        failed[3] = 3;
        failed.extend([0, 0, 0, 0, 0, 1, 0, 1]);
        failed.extend(987_i32.to_be_bytes());
        let pixels = core.pixels().to_vec();
        let frame_id = core.latest_frame_id();
        assert_eq!(
            decoder
                .decode_framebuffer_update(&failed, geometry, &mut core)
                .unwrap_err(),
            "rfb_unsupported_framebuffer_encoding:987"
        );
        assert_eq!(core.pixels(), pixels);
        assert_eq!(core.latest_frame_id(), frame_id);
        let outcome = decoder
            .decode_framebuffer_update(&ordered, geometry, &mut core)
            .unwrap();
        assert_eq!(
            core.pixels(),
            [[90, 80, 70, 255].repeat(2), [60, 50, 40, 255].repeat(2)].concat()
        );
        assert_eq!(outcome.frame_id, frame_id.map(|frame| frame + 1));
        assert_eq!(outcome.encodings.rectangles, 2);
        assert_eq!(outcome.encodings.tight, 1);
        assert_eq!(outcome.encodings.copy_rect, 1);
        assert_eq!(outcome.encodings.jpeg, 0);
    }
}

#[test]
fn tight_gradient_reconstructs_channel_values_before_scaling_to_bgra() {
    // Independent component differences for a 2x2 image. The bottom-right
    // predictors clamp above redMax and below zero for green/blue.
    for big_endian in [false, true] {
        let format = ClientRfbPixelFormat {
            bits_per_pixel: 32,
            depth: 16,
            big_endian,
            red_max: 31,
            green_max: 63,
            blue_max: 31,
            red_shift: 11,
            green_shift: 5,
            blue_shift: 0,
            ..ClientRfbPixelFormat::core_bgra32()
        };
        let mut differences = Vec::new();
        for [r, g, b] in [[10_u32, 60, 20], [21, 4, 14], [21, 5, 13], [1, 63, 31]] {
            let value = (r << 11) | (g << 5) | b;
            differences.extend(if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            });
        }
        let mut payload = vec![0x40, 2];
        payload.extend(compressed(&differences));
        assert_eq!(
            decode(2, 2, 7, format, &payload).unwrap(),
            [164, 242, 82, 255, 16, 0, 255, 255, 8, 4, 255, 255, 255, 255, 0, 255]
        );
    }
}

#[test]
fn tight_and_tight_png_interpret_the_same_control_byte_differently() {
    use image::ImageEncoder;
    let format = ClientRfbPixelFormat::core_bgra32();
    assert_eq!(
        decode(1, 1, 7, format, &[0xa0, 10, 20, 30]).unwrap(),
        [30, 20, 10, 255]
    );
    let rgb = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let payload = [&[0xa0, 12][..], &rgb].concat();
    assert_eq!(
        decode(2, 2, 7, format, &payload).unwrap(),
        [3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
    );
    assert_eq!(
        decode(2, 1, 7, format, &[0xe0, 1, 1, 1, 2, 3, 10, 20, 30, 0x40]).unwrap(),
        [3, 2, 1, 255, 30, 20, 10, 255]
    );
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[10, 20, 30, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    assert!(png.len() < 128);
    let mut payload = vec![0xa0, png.len() as u8];
    payload.extend(png);
    assert_eq!(
        decode(1, 1, -260, format, &payload).unwrap(),
        [30, 20, 10, 255]
    );
    assert_eq!(
        decode(1, 1, -260, format, &[0, 10, 20, 30]).unwrap_err(),
        "rfb_unsupported_tight_control"
    );
}

#[test]
fn tight_out_of_bounds_palette_and_no_zlib_length_do_not_publish_pixels() {
    let format = ClientRfbPixelFormat::core_bgra32();
    let mut decoder = RfbClientFramebufferDecoder::new(format).unwrap();
    let mut core = ClientFramebufferCore::default();
    let geometry = FramebufferGeometry {
        width: 2049,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    decoder
        .decode_framebuffer_update(&update(1, 1, 7, &[0x80, 10, 20, 30]), geometry, &mut core)
        .unwrap();
    let pixels = core.pixels().to_vec();
    let frame_id = core.latest_frame_id();
    let mut excess = vec![0];
    excess.extend(compressed(&[0; 13])); // Four RGB pixels require exactly 12 bytes.
    for bytes in [
        update(2050, 1, 7, &[0x80, 10, 20, 30]),
        update(1, 1, 7, &[0x40, 1, 0, 10, 20, 30, 0]),
        update(4, 1, 7, &[0xa0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        update(4, 1, 7, &excess),
    ] {
        assert!(decoder
            .decode_framebuffer_update(&bytes, geometry, &mut core)
            .is_err());
        assert_eq!(core.pixels(), pixels);
        assert_eq!(core.latest_frame_id(), frame_id);
    }
}
