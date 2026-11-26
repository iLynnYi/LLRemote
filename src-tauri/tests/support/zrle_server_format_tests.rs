// Expected CPIXEL bytes are RFC 6143 vectors, not output from our Client decoder.
#[test]
fn zrle_server_obeys_compact_pixel_byte_order_and_channel_placement() {
    for (big_endian, high_bytes, expected) in [
        (false, false, vec![0, 0x56, 0x34, 0x12]),
        (true, false, vec![0, 0x12, 0x34, 0x56]),
        (false, true, vec![0, 0x56, 0x34, 0x12]),
        (true, true, vec![0, 0x12, 0x34, 0x56]),
    ] {
        let mut format = RfbPixelFormat {
            red_shift: 16,
            blue_shift: 0,
            ..default_rfb_pixel_format()
        };
        format.big_endian = big_endian;
        if high_bytes {
            format.red_shift = 24;
            format.green_shift = 16;
            format.blue_shift = 8;
        }
        let mut wire = Vec::new();
        write_framebuffer_rect(
            &mut wire,
            &[0x56, 0x34, 0x12, 0],
            DisplaySize {
                width: 1,
                height: 1,
            },
            &FramebufferRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            format,
            RfbFramebufferEncoding::Zrle,
            &mut RfbZlibEncoder::new(),
        )
        .unwrap();
        assert_eq!(inflate_server_zrle_vector(&wire), expected);
    }
}

#[test]
fn zrle_server_uses_pixel_width_for_8_and_16_bit_formats() {
    for (format, expected) in [
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
            vec![0, 5],
        ),
        (
            RfbPixelFormat {
                bits_per_pixel: 16,
                depth: 16,
                red_max: 31,
                green_max: 63,
                blue_max: 31,
                red_shift: 11,
                green_shift: 5,
                blue_shift: 0,
                ..default_rfb_pixel_format()
            },
            vec![0, 0x8a, 0x11],
        ),
        (
            RfbPixelFormat {
                bits_per_pixel: 16,
                depth: 16,
                big_endian: true,
                red_max: 31,
                green_max: 63,
                blue_max: 31,
                red_shift: 11,
                green_shift: 5,
                blue_shift: 0,
                ..default_rfb_pixel_format()
            },
            vec![0, 0x11, 0x8a],
        ),
    ] {
        let mut wire = Vec::new();
        write_framebuffer_rect(
            &mut wire,
            &[0x56, 0x34, 0x12, 0],
            DisplaySize {
                width: 1,
                height: 1,
            },
            &FramebufferRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            format,
            RfbFramebufferEncoding::Zrle,
            &mut RfbZlibEncoder::new(),
        )
        .unwrap();
        assert_eq!(inflate_server_zrle_vector(&wire), expected);
    }
}

fn inflate_server_zrle_vector(wire: &[u8]) -> Vec<u8> {
    let mut decoder = Decompress::new(true);
    let mut pixels = vec![0; 32];
    decoder
        .decompress(&wire[16..], &mut pixels, FlushDecompress::Sync)
        .unwrap();
    assert_eq!(decoder.total_in(), (wire.len() - 16) as u64);
    pixels.truncate(decoder.total_out() as usize);
    pixels
}

#[test]
fn zrle_server_rejects_invalid_channel_layout_before_writing_headers() {
    for format in [
        RfbPixelFormat {
            red_shift: 255,
            ..default_rfb_pixel_format()
        },
        RfbPixelFormat {
            red_shift: 8,
            ..default_rfb_pixel_format()
        },
        RfbPixelFormat {
            red_max: 100,
            ..default_rfb_pixel_format()
        },
    ] {
        let mut wire = Vec::new();
        assert!(write_framebuffer_rect(
            &mut wire,
            &[0; 4],
            DisplaySize {
                width: 1,
                height: 1
            },
            &FramebufferRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1
            },
            format,
            RfbFramebufferEncoding::Zrle,
            &mut RfbZlibEncoder::new()
        )
        .is_err());
        assert!(wire.is_empty());
    }
}

#[test]
fn zrle_server_crops_and_orders_partial_tiles_across_rows() {
    let mut source = Vec::new();
    for y in 0..68_u8 {
        for x in 0..68_u8 {
            source.extend_from_slice(&[x, x ^ y, y, 0]);
        }
    }
    let mut wire = Vec::new();
    write_framebuffer_rect(
        &mut wire,
        &source,
        DisplaySize {
            width: 68,
            height: 68,
        },
        &FramebufferRect {
            x: 1,
            y: 1,
            width: 65,
            height: 65,
        },
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zrle,
        &mut RfbZlibEncoder::new(),
    )
    .unwrap();
    assert_eq!(&wire[..8], &[0, 1, 0, 1, 0, 65, 0, 65]);
    let mut expected = Vec::new();
    // Four RFC tiles, including the one-column/one-row/corner edge tiles.
    for (x, y, w, h) in [
        (1, 1, 64, 64),
        (65, 1, 1, 64),
        (1, 65, 64, 1),
        (65, 65, 1, 1),
    ] {
        expected.push(0);
        for row in y..y + h {
            for column in x..x + w {
                expected.extend_from_slice(&[column, column ^ row, row]);
            }
        }
    }
    let mut decoder = Decompress::new(true);
    let mut decoded = vec![0; expected.len()];
    decoder
        .decompress(&wire[16..], &mut decoded, FlushDecompress::Sync)
        .unwrap();
    assert_eq!(decoder.total_in(), (wire.len() - 16) as u64);
    assert_eq!(decoder.total_out(), expected.len() as u64);
    assert_eq!(decoded, expected);
}
