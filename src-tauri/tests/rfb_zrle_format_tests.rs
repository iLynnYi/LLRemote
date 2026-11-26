use std::io::Write;
use LLRemoteLib::remote_desktop_core::{
    ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
    RfbClientFramebufferDecoder,
};

fn update(width: u16, height: u16, encoding: i32, payload: &[u8]) -> Vec<u8> {
    let mut wire = vec![0, 0, 0, 1, 0, 0, 0, 0];
    wire.extend(width.to_be_bytes());
    wire.extend(height.to_be_bytes());
    wire.extend(encoding.to_be_bytes());
    wire.extend(payload);
    wire
}

fn compressed_payload(tiles: &[u8]) -> Vec<u8> {
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    zlib.write_all(tiles).unwrap();
    let compressed = zlib.finish().unwrap();
    [
        (compressed.len() as u32).to_be_bytes().as_slice(),
        &compressed,
    ]
    .concat()
}

#[test]
fn zrle_decodes_an_independent_complete_zlib_wire_golden() {
    // RFC CPIXEL B=0x56,G=0x34,R=0x12, compressed independently with Python zlib.
    const COMPRESSED: [u8; 12] = [120, 156, 99, 8, 51, 17, 2, 0, 1, 128, 0, 157];
    let mut wire = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 16, 0, 0, 0, 12];
    wire.extend_from_slice(&COMPRESSED);
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    decoder
        .decode_framebuffer_update(
            &wire,
            FramebufferGeometry {
                width: 1,
                height: 1,
                pixel_format: PixelFormat::Bgra8888,
            },
            &mut core,
        )
        .unwrap();
    assert_eq!(core.pixels(), [0x56, 0x34, 0x12, 255]);
}

fn decode(
    width: u16,
    height: u16,
    format: ClientRfbPixelFormat,
    tiles: &[u8],
) -> Result<Vec<u8>, String> {
    // The uncompressed vectors are written from RFC 6143, independently of our Server encoder.
    let wire = update(width, height, 16, &compressed_payload(tiles));
    let mut core = ClientFramebufferCore::default();
    RfbClientFramebufferDecoder::new(format)?.decode_framebuffer_update(
        &wire,
        FramebufferGeometry {
            width: width.into(),
            height: height.into(),
            pixel_format: PixelFormat::Bgra8888,
        },
        &mut core,
    )?;
    Ok(core.pixels().to_vec())
}

#[test]
fn zrle_cpixel_follows_negotiated_byte_order_and_omitted_byte() {
    let canonical = ClientRfbPixelFormat::core_bgra32();
    for (big_endian, high_bytes, pixel) in [
        (false, false, [0x56, 0x34, 0x12]),
        (true, false, [0x12, 0x34, 0x56]),
        (false, true, [0x56, 0x34, 0x12]),
        (true, true, [0x12, 0x34, 0x56]),
    ] {
        let mut format = canonical;
        format.big_endian = big_endian;
        if high_bytes {
            format.red_shift = 24;
            format.green_shift = 16;
            format.blue_shift = 8;
        }
        let tiles = [0, pixel[0], pixel[1], pixel[2]];
        assert_eq!(
            decode(1, 1, format, &tiles).unwrap(),
            [0x56, 0x34, 0x12, 255],
            "big_endian={big_endian}, high_bytes={high_bytes}"
        );
    }
    let format = ClientRfbPixelFormat {
        red_shift: 0,
        blue_shift: 16,
        ..canonical
    };
    assert_eq!(
        decode(1, 1, format, &[0, 0x12, 0x34, 0x56]).unwrap(),
        [0x56, 0x34, 0x12, 255]
    );
}

#[test]
fn zrle_solid_tiles_have_no_palette_index_bytes() {
    assert_eq!(
        decode(
            3,
            2,
            ClientRfbPixelFormat::core_bgra32(),
            &[1, 0x56, 0x34, 0x12]
        )
        .unwrap(),
        [0x56, 0x34, 0x12, 255].repeat(6)
    );
    let tiles = [1, 0x56, 0x34, 0x12, 1, 0x98, 0x76, 0x54];
    let pixels = decode(65, 1, ClientRfbPixelFormat::core_bgra32(), &tiles).unwrap();
    assert_eq!(&pixels[..256], [0x56, 0x34, 0x12, 255].repeat(64));
    assert_eq!(&pixels[256..], [0x98, 0x76, 0x54, 255]);
}

#[test]
fn zrle_cpixel_is_used_by_packed_palette_and_both_rle_modes() {
    let format = ClientRfbPixelFormat::core_bgra32();
    assert_eq!(
        decode(2, 1, format, &[2, 0x56, 0x34, 0x12, 0x98, 0x76, 0x54, 0x40]).unwrap(),
        [0x56, 0x34, 0x12, 255, 0x98, 0x76, 0x54, 255]
    );
    assert_eq!(
        decode(2, 1, format, &[128, 0x56, 0x34, 0x12, 1]).unwrap(),
        [0x56, 0x34, 0x12, 255].repeat(2)
    );
    assert_eq!(
        decode(
            2,
            1,
            format,
            &[130, 0x56, 0x34, 0x12, 0x98, 0x76, 0x54, 0x81, 1]
        )
        .unwrap(),
        [0x98, 0x76, 0x54, 255].repeat(2)
    );
}

#[test]
fn zrle_reserved_subencodings_return_errors_without_panicking() {
    for subencoding in [17, 126, 127, 129] {
        let mut tiles = vec![subencoding];
        tiles.extend([0; 127 * 4]);
        assert_eq!(
            decode(1, 1, ClientRfbPixelFormat::core_bgra32(), &tiles).unwrap_err(),
            "rfb_zrle_invalid_subencoding"
        );
    }
}

#[test]
fn zrle_keeps_four_bytes_when_channels_span_the_whole_pixel() {
    let format = ClientRfbPixelFormat {
        depth: 30,
        red_max: 1023,
        green_max: 1023,
        blue_max: 1023,
        red_shift: 20,
        green_shift: 10,
        blue_shift: 0,
        ..ClientRfbPixelFormat::core_bgra32()
    };
    assert_eq!(
        decode(1, 1, format, &[0, 0xff, 0x03, 0xf0, 0x3f]).unwrap(),
        [255, 0, 255, 255]
    );
}

#[test]
fn zrle_persistent_history_survives_other_framebuffer_encodings() {
    let geometry = FramebufferGeometry {
        width: 17,
        height: 3,
        pixel_format: PixelFormat::Bgra8888,
    };
    for big_endian in [false, true] {
        let format = ClientRfbPixelFormat {
            big_endian,
            ..ClientRfbPixelFormat::core_bgra32()
        };
        let mut tiles = vec![0];
        let mut expected = Vec::new();
        let mut random = 0x12345678_u32;
        for _ in 0..51 {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            let [blue, green, red, _] = random.to_le_bytes();
            expected.extend([blue, green, red, 255]);
            tiles.extend(if big_endian {
                [red, green, blue]
            } else {
                [blue, green, red]
            });
        }
        // Generic sync-flush chunks share history independently of our Server writer.
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut chunks = Vec::new();
        for _ in 0..2 {
            let offset = zlib.get_ref().len();
            zlib.write_all(&tiles).unwrap();
            zlib.flush().unwrap();
            chunks.push(zlib.get_ref()[offset..].to_vec());
        }
        assert!(
            chunks[1].len() < tiles.len() / 2,
            "second chunk should reuse prior data"
        );
        let zrle = |body: &[u8]| {
            update(
                17,
                3,
                16,
                &[(body.len() as u32).to_be_bytes().as_slice(), body].concat(),
            )
        };
        let mut decoder = RfbClientFramebufferDecoder::new(format).unwrap();
        let mut core = ClientFramebufferCore::default();
        decoder
            .decode_framebuffer_update(&zrle(&chunks[0]), geometry, &mut core)
            .unwrap();
        assert_eq!(core.pixels(), expected);
        let wire_pixel = if big_endian {
            [0, 90, 80, 70]
        } else {
            [70, 80, 90, 0]
        };
        for (frame, message, expected_pixels) in [
            (
                2,
                update(17, 3, 0, &wire_pixel.repeat(51)),
                [70, 80, 90, 255].repeat(51),
            ),
            (
                3,
                update(17, 3, 6, &compressed_payload(&wire_pixel.repeat(51))),
                [70, 80, 90, 255].repeat(51),
            ),
            (
                4,
                update(17, 3, 7, &[0x80, 120, 130, 140]),
                [140, 130, 120, 255].repeat(51),
            ),
        ] {
            decoder
                .decode_framebuffer_update(&message, geometry, &mut core)
                .unwrap();
            assert_eq!(core.pixels(), expected_pixels);
            assert_eq!(core.latest_frame_id(), Some(frame));
        }
        let outcome = decoder
            .decode_framebuffer_update(&zrle(&chunks[1]), geometry, &mut core)
            .unwrap();
        assert_eq!(core.pixels(), expected);
        assert_eq!(outcome.frame_id, Some(5));
        assert_eq!(outcome.encodings.rectangles, 1);
        let mut fresh = RfbClientFramebufferDecoder::new(format).unwrap();
        let mut empty = ClientFramebufferCore::default();
        assert!(fresh
            .decode_framebuffer_update(&zrle(&chunks[1]), geometry, &mut empty)
            .is_err());
        assert!(
            empty.snapshot().is_none(),
            "second chunk is not a standalone zlib stream"
        );
    }
}

#[test]
fn zrle_two_dimensional_edge_tiles_keep_packed_rows_and_long_runs_aligned() {
    const A: [u8; 3] = [10, 20, 30];
    const B: [u8; 3] = [40, 50, 60];
    const C: [u8; 3] = [70, 80, 90];
    for palette_rle in [false, true] {
        for row_codes in [
            &[0x3f_u8, 0x7f, 0xbf][..],
            &[0x0f, 0x1f, 0x2f, 0x3f, 0x4f][..],
            &[
                0x0f, 0x1f, 0x2f, 0x3f, 0x4f, 0x5f, 0x6f, 0x7f, 0x8f, 0x9f, 0xaf, 0xbf, 0xcf, 0xdf,
                0xef, 0xff,
            ][..],
        ] {
            let mut tiles = Vec::new();
            if palette_rle {
                tiles.push(130);
                tiles.extend(A);
                tiles.extend(B);
                tiles.push(0x81);
            } else {
                tiles.push(128);
                tiles.extend(A);
            }
            // 4096 = 1 + 16 * 255 + 15, crossing many run-length continuation bytes.
            tiles.extend([255; 16]);
            tiles.push(15);
            tiles.push(row_codes.len() as u8);
            for index in 0..row_codes.len() as u8 {
                tiles.extend([index + 10, index + 50, index + 100]);
            }
            // Right edge: one pixel per row, unused low bits deliberately set.
            for y in 0..64 {
                tiles.push(row_codes[y % row_codes.len()]);
            }
            // Bottom edge: one literal primary pixel followed by a 63-pixel secondary run.
            tiles.push(130);
            tiles.extend(A);
            tiles.extend(B);
            tiles.extend([0, 0x81, 62]);
            tiles.push(1);
            tiles.extend(C);
            let mut expected = Vec::new();
            for y in 0..65 {
                for x in 0..65 {
                    let [blue, green, red] = if y == 64 {
                        if x == 64 {
                            C
                        } else if x == 0 {
                            A
                        } else {
                            B
                        }
                    } else if x == 64 {
                        let index = (y % row_codes.len()) as u8;
                        [index + 10, index + 50, index + 100]
                    } else if palette_rle {
                        B
                    } else {
                        A
                    };
                    expected.extend([blue, green, red, 255]);
                }
            }
            assert_eq!(
                decode(65, 65, ClientRfbPixelFormat::core_bgra32(), &tiles).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn malformed_zrle_tiles_do_not_publish_preceding_rectangles() {
    let geometry = FramebufferGeometry {
        width: 2,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    for (tiles, code) in [
        (
            vec![3, 10, 20, 30, 40, 50, 60, 70, 80, 90, 0xc0],
            "rfb_zrle_palette_index_out_of_bounds",
        ),
        (
            vec![130, 10, 20, 30, 40, 50, 60, 2],
            "rfb_zrle_palette_index_out_of_bounds",
        ),
        (vec![128, 10, 20, 30, 1], "rfb_zrle_run_exceeds_tile"),
        (
            vec![130, 10, 20, 30, 40, 50, 60, 0x80, 1],
            "rfb_zrle_run_exceeds_tile",
        ),
        (vec![0, 10, 20], "rfb_framebuffer_update_truncated"),
        (vec![1, 10, 20, 30, 0], "rfb_zrle_decoded_trailing_bytes"),
    ] {
        let mut decoder =
            RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
        let mut core = ClientFramebufferCore::default();
        decoder
            .decode_framebuffer_update(
                &update(2, 1, 0, &[1, 2, 3, 0, 4, 5, 6, 0]),
                geometry,
                &mut core,
            )
            .unwrap();
        let baseline = core.pixels().to_vec();
        let mut bad = update(1, 1, 0, &[9, 8, 7, 0]);
        bad[3] = 2;
        bad.extend(&update(1, 1, 16, &compressed_payload(&tiles))[4..]);
        assert_eq!(
            decoder
                .decode_framebuffer_update(&bad, geometry, &mut core)
                .unwrap_err(),
            code
        );
        assert_eq!(core.pixels(), baseline);
        assert_eq!(core.latest_frame_id(), Some(1));
        decoder
            .decode_framebuffer_update(&update(1, 1, 0, &[20, 30, 40, 0]), geometry, &mut core)
            .unwrap();
        assert_eq!(core.pixels(), [20, 30, 40, 255, 4, 5, 6, 255]);
        assert_eq!(core.latest_frame_id(), Some(2));
    }
}
