fn write_hextile_framebuffer_rect_payload<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
) -> Result<u64, std::io::Error> {
    let y_end = rect.y + rect.height;
    let x_end = rect.x + rect.width;
    let mut bytes_written = 0_u64;
    let mut tile_y = rect.y;
    while tile_y < y_end {
        let tile_height = HEXTILE_TILE_SIZE.min(y_end - tile_y);
        let mut tile_x = rect.x;
        while tile_x < x_end {
            let tile_width = HEXTILE_TILE_SIZE.min(x_end - tile_x);
            stream.write_all(&[1])?;
            bytes_written += 1;
            for row_y in tile_y..(tile_y + tile_height) {
                write_framebuffer_row(
                    stream,
                    raw_framebuffer,
                    framebuffer,
                    tile_x,
                    row_y,
                    tile_width,
                    pixel_format,
                )?;
                bytes_written +=
                    u64::from(tile_width) * u64::from(pixel_format_bytes_per_pixel(pixel_format)?);
            }
            tile_x += HEXTILE_TILE_SIZE;
        }
        tile_y += HEXTILE_TILE_SIZE;
    }
    Ok(bytes_written)
}
