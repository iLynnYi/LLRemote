const TIGHT_MAX_RECTANGLE_WIDTH: u32 = 2048;
// Leave ample room for deflate overhead below Tight's 22-bit wire length. This
// also bounds the per-rectangle source/compressed buffers of the sole writer.
const TIGHT_MAX_RECTANGLE_PIXEL_BYTES: usize = 1024 * 1024;

/// Plan all wire rectangles before writing the update count. Splitting must not
/// change the selected Core delivery or advance any persistent zlib stream.
fn plan_framebuffer_rectangles(
    rectangles: Vec<FramebufferRect>,
    framebuffer: DisplaySize,
    pixel_format: RfbPixelFormat,
    encoding: RfbFramebufferEncoding,
) -> Result<Vec<FramebufferRect>, std::io::Error> {
    pixel_format_bytes_per_pixel(pixel_format)?;
    if framebuffer.width == 0
        || framebuffer.height == 0
        || framebuffer.width > u16::MAX as u32
        || framebuffer.height > u16::MAX as u32
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "rfb_invalid_framebuffer_geometry",
        ));
    }
    for rect in &rectangles {
        if rect.width == 0
            || rect.height == 0
            || rect
                .x
                .checked_add(rect.width)
                .is_none_or(|end| end > framebuffer.width)
            || rect
                .y
                .checked_add(rect.height)
                .is_none_or(|end| end > framebuffer.height)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "framebuffer_rect_out_of_bounds",
            ));
        }
    }
    if !matches!(encoding, RfbFramebufferEncoding::Tight) {
        if rectangles.len() > u16::MAX as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "rfb_rectangle_count_limit",
            ));
        }
        return Ok(rectangles);
    }
    let bytes_per_pixel = tight_pixel_bytes_per_pixel(pixel_format)?;
    let mut output = Vec::new();
    for rect in &rectangles {
        let mut x = rect.x;
        while x < rect.x + rect.width {
            let width = (rect.x + rect.width - x).min(TIGHT_MAX_RECTANGLE_WIDTH);
            let rows =
                (TIGHT_MAX_RECTANGLE_PIXEL_BYTES / (width as usize * bytes_per_pixel)) as u32;
            let mut y = rect.y;
            while y < rect.y + rect.height {
                let height = (rect.y + rect.height - y).min(rows);
                if output.len() == u16::MAX as usize {
                    // A dense selection may exceed the count after splitting.
                    // Replan its bounds, rather than truncating the u16 count.
                    let first = rectangles[0];
                    let bounds = rectangles.iter().skip(1).fold(first, |bounds, rect| {
                        let x = bounds.x.min(rect.x);
                        let y = bounds.y.min(rect.y);
                        FramebufferRect {
                            x,
                            y,
                            width: (bounds.x + bounds.width).max(rect.x + rect.width) - x,
                            height: (bounds.y + bounds.height).max(rect.y + rect.height) - y,
                        }
                    });
                    return plan_framebuffer_rectangles(
                        vec![bounds],
                        framebuffer,
                        pixel_format,
                        encoding,
                    );
                }
                output.push(FramebufferRect {
                    x,
                    y,
                    width,
                    height,
                });
                y += height;
            }
            x += width;
        }
    }
    Ok(output)
}

fn validate_tight_rectangle(
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
) -> Result<(), std::io::Error> {
    let bytes_per_pixel = tight_pixel_bytes_per_pixel(pixel_format)?;
    let pixel_bytes = (rect.width as usize)
        .checked_mul(rect.height as usize)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel));
    if rect.width > TIGHT_MAX_RECTANGLE_WIDTH
        || pixel_bytes.is_none_or(|bytes| bytes > TIGHT_MAX_RECTANGLE_PIXEL_BYTES)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "tight_rectangle_requires_split",
        ));
    }
    Ok(())
}
