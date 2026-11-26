fn write_tight_framebuffer_rect_payload<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
    state: &mut RfbZlibEncoder,
) -> Result<u64, std::io::Error> {
    let bytes_per_pixel = tight_pixel_bytes_per_pixel(pixel_format)?;
    let pixels = collect_tight_pixels(raw_framebuffer, framebuffer, rect, pixel_format)?;
    let first = &pixels[..bytes_per_pixel];
    if pixels
        .chunks_exact(bytes_per_pixel)
        .all(|pixel| pixel == first)
    {
        stream.write_all(&[0x80])?;
        stream.write_all(first)?;
        return Ok(1 + bytes_per_pixel as u64);
    }

    let stream_id = state.next_tight_stream;
    state.next_tight_stream = (state.next_tight_stream + 1) % state.tight_encoders.len();
    if pixels.len() < 12 {
        stream.write_all(&[(stream_id as u8) << 4])?;
        stream.write_all(&pixels)?;
        return Ok(1 + pixels.len() as u64);
    }
    let compressed = compress_framebuffer_rect(&mut state.tight_encoders[stream_id], &pixels)?;
    let prefix = tight_compact_length(compressed.len())?;
    stream.write_all(&[(stream_id as u8) << 4])?;
    stream.write_all(&prefix)?;
    stream.write_all(&compressed)?;
    Ok(1 + prefix.len() as u64 + compressed.len() as u64)
}

// TPIXEL is RGB24 only for this exact profile; other true-colour formats use
// negotiated PIXEL bytes, including their endian and channel placement.
fn tight_uses_rgb24(pixel_format: RfbPixelFormat) -> bool {
    pixel_format.true_color
        && pixel_format.bits_per_pixel == 32
        && pixel_format.depth == 24
        && pixel_format.red_max == 255
        && pixel_format.green_max == 255
        && pixel_format.blue_max == 255
}

fn tight_pixel_bytes_per_pixel(pixel_format: RfbPixelFormat) -> Result<usize, std::io::Error> {
    let bytes = pixel_format_bytes_per_pixel(pixel_format)?;
    Ok(if tight_uses_rgb24(pixel_format) {
        3
    } else {
        bytes as usize
    })
}

fn collect_tight_pixels(
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
) -> Result<Vec<u8>, std::io::Error> {
    validate_framebuffer_rect_source(raw_framebuffer, framebuffer, rect)?;
    let bytes_per_pixel = tight_pixel_bytes_per_pixel(pixel_format)?;
    let mut output =
        Vec::with_capacity(rect.width as usize * rect.height as usize * bytes_per_pixel);
    for y in rect.y..rect.y + rect.height {
        if !tight_uses_rgb24(pixel_format) {
            write_framebuffer_row(
                &mut output,
                raw_framebuffer,
                framebuffer,
                rect.x,
                y,
                rect.width,
                pixel_format,
            )?;
            continue;
        }
        let start = (y as usize * framebuffer.width as usize + rect.x as usize) * 4;
        let end = start + rect.width as usize * 4;
        for bgra in raw_framebuffer[start..end].as_chunks::<4>().0 {
            output.extend_from_slice(&[bgra[2], bgra[1], bgra[0]]);
        }
    }
    Ok(output)
}

fn tight_compact_length(length: usize) -> Result<Vec<u8>, std::io::Error> {
    if length > 0x3f_ffff {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "tight_payload_too_large",
        ));
    }
    let mut value = length;
    let mut bytes = vec![(value & 0x7f) as u8];
    value >>= 7;
    if value > 0 {
        bytes[0] |= 0x80;
        bytes.push((value & 0x7f) as u8);
        value >>= 7;
    }
    if value > 0 {
        bytes[1] |= 0x80;
        bytes.push(value as u8);
    }
    Ok(bytes)
}
