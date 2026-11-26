#[test]
fn framebuffer_rect_writer_uses_zrle_when_client_supports_it() {
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
        RfbFramebufferEncoding::Zrle,
        &mut zlib_encoder,
    )
    .expect("write zrle framebuffer rect");

    assert_eq!(&wire[..8], &[0, 0, 0, 0, 0, 2, 0, 1]);
    assert_eq!(
        i32::from_be_bytes([wire[8], wire[9], wire[10], wire[11]]),
        ZRLE_ENCODING,
    );
    let compressed_len = u32::from_be_bytes([wire[12], wire[13], wire[14], wire[15]]) as usize;
    assert_eq!(wire.len(), 16 + compressed_len);

    let mut decoder = Decompress::new(true);
    let decoded = inflate_zlib_rect_chunk(&mut decoder, &wire[16..], 1 + 2 * 3);
    assert_eq!(
        decoded,
        vec![
            0, // raw ZRLE tile
            30, 20, 10, // negotiated little-endian CPIXEL: B,G,R
            70, 60, 50,
        ],
    );
}

#[test]
fn framebuffer_rect_writer_reuses_zrle_stream_across_rectangles() {
    let raw_framebuffer = vec![
        30, 20, 10, 0, // server B,G,R,0
        70, 60, 50, 0,
    ];
    let framebuffer = DisplaySize {
        width: 2,
        height: 1,
    };
    let rect = FramebufferRect {
        x: 0,
        y: 0,
        width: 2,
        height: 1,
    };
    let mut zlib_encoder = RfbZlibEncoder::new();
    let mut first_wire = Vec::new();
    let mut second_wire = Vec::new();

    write_framebuffer_rect(
        &mut first_wire,
        &raw_framebuffer,
        framebuffer,
        &rect,
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zrle,
        &mut zlib_encoder,
    )
    .expect("write first zrle framebuffer rect");
    write_framebuffer_rect(
        &mut second_wire,
        &raw_framebuffer,
        framebuffer,
        &rect,
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Zrle,
        &mut zlib_encoder,
    )
    .expect("write second zrle framebuffer rect");

    let mut decoder = Decompress::new(true);
    assert_eq!(
        inflate_zrle_rect_chunk_with_sync_flush(&mut decoder, &first_wire),
        vec![0, 30, 20, 10, 70, 60, 50]
    );
    assert_eq!(
        inflate_zrle_rect_chunk_with_sync_flush(&mut decoder, &second_wire),
        vec![0, 30, 20, 10, 70, 60, 50]
    );
}
