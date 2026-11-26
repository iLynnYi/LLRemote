#[test]
fn framebuffer_rect_writer_uses_hextile_raw_tiles_when_client_supports_it() {
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
        core_bgra32_pixel_format(),
        RfbFramebufferEncoding::Hextile,
        &mut zlib_encoder,
    )
    .expect("write hextile framebuffer rect");

    assert_eq!(&wire[..8], &[0, 0, 0, 0, 0, 2, 0, 1]);
    assert_eq!(
        i32::from_be_bytes([wire[8], wire[9], wire[10], wire[11]]),
        HEXTILE_ENCODING,
    );
    assert_eq!(wire[12], 1);
    assert_eq!(&wire[13..], &[10, 20, 30, 0, 50, 60, 70, 0]);
    assert_eq!(bytes_written, wire.len() as u64);
}

#[test]
fn framebuffer_rect_writer_keeps_hextile_tiles_raw_for_stable_boundaries() {
    let raw_framebuffer = vec![30, 20, 10, 0, 30, 20, 10, 0, 30, 20, 10, 0, 30, 20, 10, 0];
    let framebuffer = DisplaySize {
        width: 2,
        height: 2,
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
            height: 2,
        },
        core_bgra32_pixel_format(),
        RfbFramebufferEncoding::Hextile,
        &mut zlib_encoder,
    )
    .expect("write hextile solid tile");

    assert_eq!(
        i32::from_be_bytes([wire[8], wire[9], wire[10], wire[11]]),
        HEXTILE_ENCODING,
    );
    assert_eq!(
        &wire[12..],
        &[
            1, // Hextile raw tile
            10, 20, 30, 0, 10, 20, 30, 0, 10, 20, 30, 0, 10, 20, 30, 0,
        ],
    );
    assert_eq!(bytes_written, wire.len() as u64);
}

#[test]
fn framebuffer_rect_writer_keeps_large_unaligned_hextile_payload_synchronized() {
    let framebuffer = DisplaySize {
        width: 1722,
        height: 1080,
    };
    let raw_framebuffer =
        vec![255_u8; framebuffer.width as usize * framebuffer.height as usize * 4];
    let mut wire = Vec::new();
    let mut zlib_encoder = RfbZlibEncoder::new();

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
        core_bgra32_pixel_format(),
        RfbFramebufferEncoding::Hextile,
        &mut zlib_encoder,
    )
    .expect("write large hextile framebuffer rect");

    let mut offset = 12;
    let mut tile_y = 0_u32;
    while tile_y < framebuffer.height {
        let tile_height = HEXTILE_TILE_SIZE.min(framebuffer.height - tile_y);
        let mut tile_x = 0_u32;
        while tile_x < framebuffer.width {
            let tile_width = HEXTILE_TILE_SIZE.min(framebuffer.width - tile_x);
            assert_eq!(
                wire[offset], 1,
                "tile {},{} must start with Hextile raw subencoding",
                tile_x, tile_y
            );
            offset += 1 + tile_width as usize * tile_height as usize * 4;
            tile_x += HEXTILE_TILE_SIZE;
        }
        tile_y += HEXTILE_TILE_SIZE;
    }
    assert_eq!(offset, wire.len());
}
