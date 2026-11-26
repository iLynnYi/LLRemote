#[test]
fn framebuffer_rect_writer_uses_zlib_when_client_supports_it() {
    let raw_framebuffer = vec![
        30, 20, 10, 0, // server B,G,R,0
        70, 60, 50, 0,
    ];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
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
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zlib,
        &mut zlib_encoder,
    )
    .expect("write framebuffer rect");

    assert_eq!(&wire[..8], &[0, 0, 0, 0, 0, 2, 0, 1]);
    assert_eq!(
        i32::from_be_bytes([wire[8], wire[9], wire[10], wire[11]]),
        6
    );
    let compressed_len = u32::from_be_bytes([wire[12], wire[13], wire[14], wire[15]]) as usize;
    assert_eq!(wire.len(), 16 + compressed_len);

    let mut decoder = Decompress::new(true);
    let decoded = inflate_zlib_rect_chunk(&mut decoder, &wire[16..], raw_framebuffer.len());
    assert_eq!(decoded, raw_framebuffer);
}

#[test]
fn framebuffer_rect_writer_reuses_zlib_stream_across_rectangles() {
    let raw_framebuffer = vec![
        30, 20, 10, 0, // server B,G,R,0
        70, 60, 50, 0,
    ];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
    let mut zlib_encoder = RfbZlibEncoder::new();
    let mut first_wire = Vec::new();
    let mut second_wire = Vec::new();
    let rect = FramebufferRect {
        x: 0,
        y: 0,
        width: 2,
        height: 1,
    };

    write_framebuffer_rect(
        &mut first_wire,
        &raw_framebuffer,
        framebuffer,
        &rect,
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zlib,
        &mut zlib_encoder,
    )
    .expect("write first zlib framebuffer rect");
    write_framebuffer_rect(
        &mut second_wire,
        &raw_framebuffer,
        framebuffer,
        &rect,
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zlib,
        &mut zlib_encoder,
    )
    .expect("write second zlib framebuffer rect");

    let mut decoder = Decompress::new(true);
    let first_len = u32::from_be_bytes([
        first_wire[12],
        first_wire[13],
        first_wire[14],
        first_wire[15],
    ]) as usize;
    let second_len = u32::from_be_bytes([
        second_wire[12],
        second_wire[13],
        second_wire[14],
        second_wire[15],
    ]) as usize;

    assert_eq!(
        inflate_zlib_rect_chunk(&mut decoder, &first_wire[16..16 + first_len], 8),
        raw_framebuffer
    );
    assert_eq!(
        inflate_zlib_rect_chunk(&mut decoder, &second_wire[16..16 + second_len], 8),
        raw_framebuffer
    );
}

#[test]
fn framebuffer_rect_writer_zlib_chunks_inflate_with_sync_flush() {
    let raw_framebuffer = vec![
        30, 20, 10, 0, // server B,G,R,0
        70, 60, 50, 0,
    ];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
    let mut zlib_encoder = RfbZlibEncoder::new();
    let mut wire = Vec::new();

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
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zlib,
        &mut zlib_encoder,
    )
    .expect("write zlib framebuffer rect");

    let compressed_len = u32::from_be_bytes([wire[12], wire[13], wire[14], wire[15]]) as usize;
    let mut decoder = Decompress::new(true);
    let mut decoded = vec![0_u8; raw_framebuffer.len()];
    decoder
        .decompress(
            &wire[16..16 + compressed_len],
            &mut decoded,
            FlushDecompress::None,
        )
        .expect("decode zlib rect chunk with sync flush mode");

    assert_eq!(decoder.total_out(), decoded.len() as u64);
    assert_eq!(decoded, raw_framebuffer);
}

#[test]
fn framebuffer_rect_writer_zlib_honors_core_bgra32_pixel_format() {
    let raw_framebuffer = vec![
        0, 0, 255, 0, // red
        0, 255, 0, 0, // green
    ];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
    let client_pixel_format = core_bgra32_pixel_format();
    let mut zlib_encoder = RfbZlibEncoder::new();
    let mut wire = Vec::new();

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
        RfbFramebufferEncoding::Zlib,
        &mut zlib_encoder,
    )
    .expect("write zlib framebuffer rect");

    let compressed_len = u32::from_be_bytes([wire[12], wire[13], wire[14], wire[15]]) as usize;
    let mut decoder = Decompress::new(true);
    let mut decoded = vec![0_u8; raw_framebuffer.len()];
    decoder
        .decompress(
            &wire[16..16 + compressed_len],
            &mut decoded,
            FlushDecompress::None,
        )
        .expect("decode zlib rect chunk with sync flush mode");

    assert_eq!(decoder.total_out(), decoded.len() as u64);
    assert_eq!(
        decoded,
        vec![
            255, 0, 0, 0, // requested R,G,B,padding layout
            0, 255, 0, 0,
        ],
    );
}

#[test]
fn framebuffer_rect_writer_large_zlib_band_inflates_with_sync_flush() {
    let framebuffer = DisplaySize {
        width: 1920,
        height: 64,
    };
    let mut raw_framebuffer =
        Vec::with_capacity(framebuffer.width as usize * framebuffer.height as usize * 4);
    for _ in 0..(framebuffer.width as usize * framebuffer.height as usize) {
        raw_framebuffer.extend_from_slice(&[42, 42, 42, 0]);
    }
    let mut zlib_encoder = RfbZlibEncoder::new();
    let mut wire = Vec::new();

    write_framebuffer_rect(
        &mut wire,
        &raw_framebuffer,
        framebuffer,
        &FramebufferRect {
            x: 0,
            y: 0,
            width: framebuffer.width,
            height: framebuffer.height,
        },
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zlib,
        &mut zlib_encoder,
    )
    .expect("write large zlib framebuffer rect");

    let compressed_len = u32::from_be_bytes([wire[12], wire[13], wire[14], wire[15]]) as usize;
    let mut decoder = Decompress::new(true);
    let mut decoded = vec![0_u8; raw_framebuffer.len()];
    decoder
        .decompress(
            &wire[16..16 + compressed_len],
            &mut decoded,
            FlushDecompress::None,
        )
        .expect("decode large zlib rect chunk with sync flush mode");

    assert_eq!(decoder.total_out(), raw_framebuffer.len() as u64);
    assert_eq!(decoded, raw_framebuffer);
}
