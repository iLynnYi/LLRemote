fn write_zrle_framebuffer_rect_payload<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
    zlib_encoder: &mut RfbZlibEncoder,
) -> Result<u64, std::io::Error> {
    let range = crate::remote_desktop_core::cpixel_byte_range(
        pixel_format.bits_per_pixel,
        pixel_format.depth,
        pixel_format.big_endian,
        [
            (pixel_format.red_max, pixel_format.red_shift),
            (pixel_format.green_max, pixel_format.green_shift),
            (pixel_format.blue_max, pixel_format.blue_shift),
        ],
    )
    .map_err(|code| std::io::Error::new(std::io::ErrorKind::InvalidData, code))?;
    let mut zrle_tiles = Vec::with_capacity(
        rect.width as usize * rect.height as usize * range.len()
            + rect.width.div_ceil(64) as usize * rect.height.div_ceil(64) as usize,
    );
    let y_end = rect.y + rect.height;
    let x_end = rect.x + rect.width;
    let mut tile_y = rect.y;
    while tile_y < y_end {
        let tile_height = ZRLE_TILE_SIZE.min(y_end - tile_y);
        let mut tile_x = rect.x;
        while tile_x < x_end {
            let tile_width = ZRLE_TILE_SIZE.min(x_end - tile_x);
            zrle_tiles.push(0);
            write_zrle_raw_tile(
                &mut zrle_tiles,
                raw_framebuffer,
                framebuffer,
                &FramebufferRect {
                    x: tile_x,
                    y: tile_y,
                    width: tile_width,
                    height: tile_height,
                },
                pixel_format,
                &range,
            )?;
            tile_x += ZRLE_TILE_SIZE;
        }
        tile_y += ZRLE_TILE_SIZE;
    }

    let compressed = zlib_encoder.encode_zrle_rect(&zrle_tiles)?;
    stream.write_all(&(compressed.len() as u32).to_be_bytes())?;
    stream.write_all(&compressed)?;
    Ok(4 + compressed.len() as u64)
}

fn write_zrle_raw_tile<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
    range: &std::ops::Range<usize>,
) -> Result<(), std::io::Error> {
    let (x, y, width, height) = (rect.x, rect.y, rect.width, rect.height);
    let source_width = framebuffer.width as usize;
    for row_y in y..(y + height) {
        let start = (row_y as usize * source_width + x as usize) * 4;
        let end = start + width as usize * 4;
        if end > raw_framebuffer.len() {
            return Err(framebuffer_row_out_of_bounds_error(
                framebuffer,
                x,
                row_y,
                width,
                raw_framebuffer.len(),
            ));
        }
        for pixel in raw_framebuffer[start..end].as_chunks::<4>().0 {
            if pixel_format == crate::local_vnc_server::default_rfb_pixel_format() {
                stream.write_all(&pixel[..3])?;
            } else {
                let mut wire = [0; 4];
                write_pixel(&mut &mut wire[..], *pixel, pixel_format)?;
                stream.write_all(&wire[range.clone()])?;
            }
        }
    }
    Ok(())
}
