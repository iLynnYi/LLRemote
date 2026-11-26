include!("writer/raw.rs");
include!("writer/hextile.rs");
include!("writer/zlib.rs");
include!("writer/zrle.rs");
include!("writer/tight.rs");

pub(super) fn write_server_init<W: Write>(
    stream: &mut W,
    framebuffer: DisplaySize,
) -> Result<(), std::io::Error> {
    stream.write_all(&(framebuffer.width as u16).to_be_bytes())?;
    stream.write_all(&(framebuffer.height as u16).to_be_bytes())?;
    stream.write_all(&[
        32, 24, 0, 1, // bits-per-pixel, depth, little-endian, true-color
        0, 255, 0, 255, 0, 255, // color max values
        16, 8, 0, // shifts
        0, 0, 0, // padding
    ])?;
    stream.write_all(&(DESKTOP_NAME.len() as u32).to_be_bytes())?;
    stream.write_all(DESKTOP_NAME.as_bytes())?;
    Ok(())
}

pub(super) fn write_end_of_continuous_updates<W: Write>(
    stream: &mut W,
) -> Result<(), std::io::Error> {
    stream.write_all(&[150])
}

/// Encodes the dirty regions selected by `ServerFramebufferCore`.
pub(super) fn write_core_framebuffer_update<W: Write>(
    stream: &mut W,
    frame: &crate::remote_desktop_core::ClientFrameUpdate,
    frame_cache: &LocalVncServerFrameCache,
    pixel_format: RfbPixelFormat,
    framebuffer_encoding: RfbFramebufferEncoding,
    zlib_encoder: &mut RfbZlibEncoder,
) -> Result<(), std::io::Error> {
    let geometry = frame.framebuffer.geometry();
    let framebuffer = DisplaySize {
        width: geometry.width,
        height: geometry.height,
    };
    let raw_framebuffer = frame.framebuffer.shared_pixels();
    let dirty_rectangles = crate::remote_desktop_core::coalesce_dirty_rectangles(
        frame.dirty_rectangles.clone(),
        geometry,
        u16::MAX as usize,
    );
    let rectangles = dirty_rectangles
        .iter()
        .map(|rect| FramebufferRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        })
        .collect::<Vec<_>>();
    let expected = framebuffer.width as usize * framebuffer.height as usize * 4;
    if raw_framebuffer.len() != expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "framebuffer_length_mismatch",
        ));
    }
    let rectangles =
        plan_framebuffer_rectangles(rectangles, framebuffer, pixel_format, framebuffer_encoding)?;
    let raw_bytes_written = raw_framebuffer_update_bytes_written(&rectangles, pixel_format);
    let write_started_at = Instant::now();
    let mut writer = BufferedFramebufferWrite::new(stream);
    let mut encoded_bytes_written = 4_u64;
    writer.write_all(&[0, 0])?;
    writer.write_all(&(rectangles.len() as u16).to_be_bytes())?;
    for rect in &rectangles {
        encoded_bytes_written += write_framebuffer_rect(
            &mut writer,
            raw_framebuffer.as_slice(),
            framebuffer,
            rect,
            pixel_format,
            framebuffer_encoding,
            zlib_encoder,
        )?;
    }
    writer.flush()?;
    frame_cache.record_send(
        Duration::ZERO,
        write_started_at.elapsed(),
        rectangles.len(),
        framebuffer_encoding.label(),
        raw_bytes_written,
        encoded_bytes_written,
    );
    Ok(())
}

pub(super) fn write_framebuffer_rect<W: Write>(
    stream: &mut W,
    raw_framebuffer: &[u8],
    framebuffer: DisplaySize,
    rect: &FramebufferRect,
    pixel_format: RfbPixelFormat,
    framebuffer_encoding: RfbFramebufferEncoding,
    zlib_encoder: &mut RfbZlibEncoder,
) -> Result<u64, std::io::Error> {
    validate_framebuffer_rect_source(raw_framebuffer, framebuffer, rect)?;
    pixel_format_bytes_per_pixel(pixel_format)?;
    if matches!(framebuffer_encoding, RfbFramebufferEncoding::Tight) {
        validate_tight_rectangle(rect, pixel_format)?;
    }

    stream.write_all(&(rect.x as u16).to_be_bytes())?;
    stream.write_all(&(rect.y as u16).to_be_bytes())?;
    stream.write_all(&(rect.width as u16).to_be_bytes())?;
    stream.write_all(&(rect.height as u16).to_be_bytes())?;
    match framebuffer_encoding {
        RfbFramebufferEncoding::Raw => {
            stream.write_all(&RAW_ENCODING.to_be_bytes())?;
        }
        RfbFramebufferEncoding::Hextile => {
            stream.write_all(&HEXTILE_ENCODING.to_be_bytes())?;
            let payload_bytes = write_hextile_framebuffer_rect_payload(
                stream,
                raw_framebuffer,
                framebuffer,
                rect,
                pixel_format,
            )?;
            return Ok(12 + payload_bytes);
        }
        RfbFramebufferEncoding::Zlib => {
            stream.write_all(&ZLIB_ENCODING.to_be_bytes())?;
            let payload_bytes = write_zlib_framebuffer_rect_payload(
                stream,
                raw_framebuffer,
                framebuffer,
                rect,
                pixel_format,
                zlib_encoder,
            )?;
            return Ok(12 + payload_bytes);
        }
        RfbFramebufferEncoding::Zrle => {
            stream.write_all(&ZRLE_ENCODING.to_be_bytes())?;
            let payload_bytes = write_zrle_framebuffer_rect_payload(
                stream,
                raw_framebuffer,
                framebuffer,
                rect,
                pixel_format,
                zlib_encoder,
            )?;
            return Ok(12 + payload_bytes);
        }
        RfbFramebufferEncoding::Tight => {
            stream.write_all(&TIGHT_ENCODING.to_be_bytes())?;
            let payload_bytes = write_tight_framebuffer_rect_payload(
                stream,
                raw_framebuffer,
                framebuffer,
                rect,
                pixel_format,
                zlib_encoder,
            )?;
            return Ok(12 + payload_bytes);
        }
    }
    let payload_bytes = write_raw_framebuffer_rect_payload(
        stream,
        raw_framebuffer,
        framebuffer,
        rect,
        pixel_format,
    )?;
    Ok(12 + payload_bytes)
}

pub(super) fn raw_framebuffer_update_bytes_written(
    rects: &[FramebufferRect],
    pixel_format: RfbPixelFormat,
) -> u64 {
    let bytes_per_pixel = pixel_format_bytes_per_pixel(pixel_format).unwrap_or(4) as u64;

    4 + rects
        .iter()
        .map(|rect| 12 + u64::from(rect.width) * u64::from(rect.height) * bytes_per_pixel)
        .sum::<u64>()
}
