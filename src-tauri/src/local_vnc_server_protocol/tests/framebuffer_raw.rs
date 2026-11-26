#[test]
fn framebuffer_rect_writer_streams_rows_with_client_pixel_format() {
    let raw_framebuffer = vec![
        30, 20, 10, 0, // server B,G,R,0
        70, 60, 50, 0,
    ];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
    let client_pixel_format = core_bgra32_pixel_format();
    let mut wire = Vec::new();
    let mut zlib_encoder = RfbZlibEncoder::new();

    write_framebuffer_rect(
        &mut wire,
        &raw_framebuffer,
        framebuffer,
        &FramebufferRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        client_pixel_format,
        RfbFramebufferEncoding::Raw,
        &mut zlib_encoder,
    )
    .expect("write framebuffer rect");

    assert_eq!(
        wire,
        vec![
            0, 0, // x
            0, 0, // y
            0, 2, // width
            0, 1, // height
            0, 0, 0, 0, // raw encoding
            10, 20, 30, 0, // client R,G,B,0
            50, 60, 70, 0,
        ],
    );
}

#[test]
fn framebuffer_rect_writer_rejects_short_raw_framebuffer_before_streaming_pixels() {
    let raw_framebuffer = vec![30, 20, 10, 0];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
    let mut wire = Vec::new();
    let mut zlib_encoder = RfbZlibEncoder::new();

    let error = write_framebuffer_rect(
        &mut wire,
        &raw_framebuffer,
        framebuffer,
        &FramebufferRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Raw,
        &mut zlib_encoder,
    )
    .expect_err("short raw framebuffer must not produce a partial rect");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("framebuffer_row_out_of_bounds"));
    assert!(wire.is_empty(), "invalid frames must be rejected before writing rectangle bytes");
}

#[test]
fn framebuffer_rect_writer_honors_8bpp_client_pixel_format() {
    let raw_framebuffer = vec![
        0, 0, 255, 0, // red
        0, 255, 0, 0, // green
    ];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
    let client_pixel_format = RfbPixelFormat {
        bits_per_pixel: 8,
        depth: 8,
        big_endian: false,
        true_color: true,
        red_max: 3,
        green_max: 3,
        blue_max: 3,
        red_shift: 0,
        green_shift: 2,
        blue_shift: 4,
    };
    let mut wire = Vec::new();
    let mut zlib_encoder = RfbZlibEncoder::new();

    let bytes_written = write_framebuffer_rect(
        &mut wire,
        &raw_framebuffer,
        framebuffer,
        &FramebufferRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        client_pixel_format,
        RfbFramebufferEncoding::Raw,
        &mut zlib_encoder,
    )
    .expect("write 8bpp framebuffer rect");

    assert_eq!(
        wire,
        vec![
            0, 0, // x
            0, 0, // y
            0, 2, // width
            0, 1, // height
            0, 0, 0, 0,    // raw encoding
            0x03, // red maxed in low two bits
            0x0c, // green maxed in bits 2-3
        ],
    );
    assert_eq!(bytes_written, wire.len() as u64);
    assert_eq!(
        raw_framebuffer_update_bytes_written(
            &[FramebufferRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            }],
            client_pixel_format
        ),
        wire.len() as u64 - 12 + 16,
    );
}

#[test]
fn raw_framebuffer_rect_uses_bits_per_pixel_for_32bpp_depth24_clients() {
    let width = 1671_u32;
    let height = 1_u32;
    let mut raw_framebuffer = Vec::with_capacity(width as usize * 4);
    for index in 0..width {
        raw_framebuffer.extend_from_slice(&[
            (index % 251) as u8,
            (index % 239) as u8,
            (index % 227) as u8,
            0,
        ]);
    }
    let framebuffer = DisplaySize { width, height };
    let rect = FramebufferRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut wire = Vec::new();
    let mut zlib_encoder = RfbZlibEncoder::new();

    let bytes_written = write_framebuffer_rect(
        &mut wire,
        &raw_framebuffer,
        framebuffer,
        &rect,
        core_bgra32_pixel_format(),
        RfbFramebufferEncoding::Raw,
        &mut zlib_encoder,
    )
    .expect("write Core BGRA32 raw framebuffer rect");

    assert_eq!(
        i32::from_be_bytes([wire[8], wire[9], wire[10], wire[11]]),
        RAW_ENCODING
    );
    assert_eq!(wire.len(), 12 + width as usize * height as usize * 4);
    assert_eq!(bytes_written, wire.len() as u64);
}

#[test]
fn only_prewrite_framebuffer_length_mismatches_are_recoverable() {
    let mismatch = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "framebuffer_length_mismatch: framebuffer=2x1 raw_len=4 expected_raw_len=8",
    );
    let row_bounds = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "framebuffer_row_out_of_bounds: framebuffer=2x1 row=0 x=0 width=2 raw_len=4",
    );
    let socket_error = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "socket closed");

    assert!(is_recoverable_framebuffer_source_error(&mismatch));
    assert!(!is_recoverable_framebuffer_source_error(&row_bounds));
    assert!(!is_recoverable_framebuffer_source_error(&socket_error));
}
