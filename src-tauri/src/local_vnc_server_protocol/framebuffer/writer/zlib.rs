fn write_zlib_framebuffer_rect_payload<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
    zlib_encoder: &mut RfbZlibEncoder,
) -> Result<u64, std::io::Error> {
    let mut rect_pixels = Vec::with_capacity(rect.width as usize * rect.height as usize * 4);
    for y in rect.y..(rect.y + rect.height) {
        write_framebuffer_row(
            &mut rect_pixels,
            raw_framebuffer,
            framebuffer,
            rect.x,
            y,
            rect.width,
            pixel_format,
        )?;
    }
    let compressed = zlib_encoder.encode_zlib_rect(&rect_pixels)?;
    stream.write_all(&(compressed.len() as u32).to_be_bytes())?;
    stream.write_all(&compressed)?;
    Ok(4 + compressed.len() as u64)
}
