fn write_raw_framebuffer_rect_payload<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
) -> Result<u64, std::io::Error> {
    for y in rect.y..(rect.y + rect.height) {
        write_framebuffer_row(
            stream,
            raw_framebuffer,
            framebuffer,
            rect.x,
            y,
            rect.width,
            pixel_format,
        )?;
    }
    Ok(u64::from(rect.width)
        * u64::from(rect.height)
        * u64::from(pixel_format_bytes_per_pixel(pixel_format)?))
}
