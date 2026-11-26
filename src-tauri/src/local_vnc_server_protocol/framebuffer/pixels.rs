fn write_framebuffer_row<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    x: u32,
    y: u32,
    width: u32,
    pixel_format: RfbPixelFormat,
) -> Result<(), std::io::Error> {
    let source_width = framebuffer.width as usize;
    let row_width = width as usize;
    let start = (y as usize * source_width + x as usize) * 4;
    let end = start + row_width * 4;
    if end > raw_framebuffer.len() {
        return Err(framebuffer_row_out_of_bounds_error(
            framebuffer,
            x,
            y,
            width,
            raw_framebuffer.len(),
        ));
    }

    let row = &raw_framebuffer[start..end];
    for pixel in row.as_chunks::<4>().0 {
        write_pixel(
            stream,
            [pixel[0], pixel[1], pixel[2], pixel[3]],
            pixel_format,
        )?;
    }
    Ok(())
}

fn validate_framebuffer_rect_source(
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
) -> Result<(), std::io::Error> {
    let rect_x_end = rect.x.saturating_add(rect.width);
    let rect_y_end = rect.y.saturating_add(rect.height);
    if rect.width == 0
        || rect.height == 0
        || rect_x_end > framebuffer.width
        || rect_y_end > framebuffer.height
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "framebuffer_rect_out_of_bounds: framebuffer={}x{} rect={}x{}+{},{}",
                framebuffer.width, framebuffer.height, rect.width, rect.height, rect.x, rect.y
            ),
        ));
    }

    let expected_len = framebuffer.width as usize * framebuffer.height as usize * 4;
    if raw_framebuffer.len() < expected_len {
        return Err(framebuffer_row_out_of_bounds_error(
            framebuffer,
            rect.x,
            rect.y,
            rect.width,
            raw_framebuffer.len(),
        ));
    }

    Ok(())
}

fn framebuffer_row_out_of_bounds_error(
    framebuffer: DisplaySize,
    x: u32,
    y: u32,
    width: u32,
    raw_len: usize,
) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
            "framebuffer_row_out_of_bounds: framebuffer={}x{} row={} x={} width={} raw_len={}",
            framebuffer.width, framebuffer.height, y, x, width, raw_len
        ),
    )
}

fn pixel_format_bytes_per_pixel(pixel_format: RfbPixelFormat) -> Result<u8, std::io::Error> {
    if !pixel_format.true_color {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported_pixel_format: color-map pixel formats are not supported",
        ));
    }

    crate::remote_desktop_core::cpixel_byte_range(
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

    match pixel_format.bits_per_pixel {
        8 => Ok(1),
        16 => Ok(2),
        32 => Ok(4),
        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported_pixel_format: bits_per_pixel={other}"),
        )),
    }
}

fn write_pixel<W: Write>(
    stream: &mut W,
    pixel: [u8; 4],
    pixel_format: RfbPixelFormat,
) -> Result<(), std::io::Error> {
    let bytes_per_pixel = pixel_format_bytes_per_pixel(pixel_format)?;
    let red = scale_pixel_channel(pixel[2], pixel_format.red_max);
    let green = scale_pixel_channel(pixel[1], pixel_format.green_max);
    let blue = scale_pixel_channel(pixel[0], pixel_format.blue_max);
    let value = (red << pixel_format.red_shift)
        | (green << pixel_format.green_shift)
        | (blue << pixel_format.blue_shift);

    match bytes_per_pixel {
        1 => stream.write_all(&[value as u8])?,
        2 if pixel_format.big_endian => stream.write_all(&(value as u16).to_be_bytes())?,
        2 => stream.write_all(&(value as u16).to_le_bytes())?,
        4 if pixel_format.big_endian => stream.write_all(&value.to_be_bytes())?,
        4 => stream.write_all(&value.to_le_bytes())?,
        _ => unreachable!("pixel format bytes per pixel is validated"),
    }
    Ok(())
}

fn scale_pixel_channel(value: u8, max: u16) -> u32 {
    if max == 255 {
        value as u32
    } else {
        ((value as u32 * max as u32) / 255).min(max as u32)
    }
}
