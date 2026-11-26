use super::rfb_limits::{
    cursor_bytes, framebuffer_bytes, message_end, MAX_CLIPBOARD_BYTES, MAX_FRAMEBUFFER_BYTES,
};

/// Resumes framing at the current rectangle/tile instead of rescanning an
/// incomplete update. It consumes no bytes and never owns decoded pixels.
pub(super) struct RfbServerMessageFramer {
    framebuffer: Option<FramebufferScan>,
    pixel_format: super::ClientRfbPixelFormat,
}

struct FramebufferScan {
    offset: usize,
    remaining: usize,
    decoded_bytes: usize,
    rectangle: Option<RectangleScan>,
}

struct RectangleScan {
    start: usize,
    width: usize,
    height: usize,
    encoding: i32,
    hextile: HextileScan,
}

#[derive(Default)]
struct HextileScan {
    tile: usize,
    offset: usize,
    colors: super::rfb_hextile::HextileColorState,
}

impl RfbServerMessageFramer {
    pub(super) fn new(pixel_format: super::ClientRfbPixelFormat) -> Self {
        Self {
            framebuffer: None,
            pixel_format,
        }
    }
    pub(super) fn next_message_length(&mut self, bytes: &[u8]) -> Result<Option<usize>, String> {
        let Some(&kind) = bytes.first() else {
            return Ok(None);
        };
        if kind != 0 {
            let length = match kind {
                1 if bytes.len() >= 6 => {
                    6 + usize::from(u16::from_be_bytes([bytes[4], bytes[5]])) * 6
                }
                1 | 3 | 248
                    if bytes.len()
                        < if kind == 1 {
                            6
                        } else if kind == 3 {
                            8
                        } else {
                            9
                        } =>
                {
                    return Ok(None)
                }
                2 | 150 => 1,
                3 => {
                    let length =
                        i32::from_be_bytes(bytes[4..8].try_into().expect("clipboard length"))
                            .unsigned_abs() as usize;
                    if length > MAX_CLIPBOARD_BYTES {
                        return Err("rfb_resource_limit:clipboard".into());
                    }
                    message_end(8, length)?
                }
                248 => {
                    let length = usize::from(bytes[8]);
                    if length > super::rfb_fence::MAX_FENCE_PAYLOAD {
                        return Err("rfb_resource_limit:fence_payload".into());
                    }
                    9 + length
                }
                _ => return Err(format!("rfb_unsupported_server_message_type:{kind}")),
            };
            return Ok((bytes.len() >= length).then_some(length));
        }
        if self.framebuffer.is_none() {
            if bytes.len() < 4 {
                return Ok(None);
            }
            self.framebuffer = Some(FramebufferScan {
                offset: 4,
                remaining: usize::from(u16::from_be_bytes([bytes[2], bytes[3]])),
                decoded_bytes: 0,
                rectangle: None,
            });
        }
        let scan = self.framebuffer.as_mut().expect("framebuffer header");
        while scan.remaining != 0 {
            if scan.rectangle.is_none() {
                let header_end = message_end(scan.offset, 12)?;
                if bytes.len() < header_end {
                    return Ok(None);
                }
                let width = usize::from(u16::from_be_bytes([
                    bytes[scan.offset + 4],
                    bytes[scan.offset + 5],
                ]));
                let height = usize::from(u16::from_be_bytes([
                    bytes[scan.offset + 6],
                    bytes[scan.offset + 7],
                ]));
                let encoding = i32::from_be_bytes(
                    bytes[scan.offset + 8..header_end]
                        .try_into()
                        .expect("rectangle encoding"),
                );
                if encoding == -224 {
                    self.framebuffer = None;
                    return Ok(Some(header_end));
                }
                // A rejected/forwarded SetDesktopSize response has undefined
                // dimensions. Consume its layout structure without allocating a frame.
                let failed_resize = encoding == -308
                    && bytes[scan.offset..scan.offset + 2] == [0, 1]
                    && bytes[scan.offset + 2..scan.offset + 4] != [0, 0];
                if matches!(encoding, 0 | 1 | 5 | 6 | 7 | 16 | -260 | -223 | -308) && !failed_resize
                {
                    let pixels = framebuffer_bytes(width, height)?;
                    if !matches!(encoding, -223 | -308) {
                        scan.decoded_bytes = scan
                            .decoded_bytes
                            .checked_add(pixels)
                            .ok_or("rfb_resource_limit:decoded_update")?;
                        if scan.decoded_bytes > MAX_FRAMEBUFFER_BYTES {
                            return Err("rfb_resource_limit:decoded_update".into());
                        }
                    }
                } else if matches!(encoding, -239 | -240) {
                    let bytes = cursor_bytes(width, height)?;
                    scan.decoded_bytes = scan
                        .decoded_bytes
                        .checked_add(bytes)
                        .filter(|total| *total <= MAX_FRAMEBUFFER_BYTES)
                        .ok_or("rfb_resource_limit:decoded_update")?;
                }
                scan.rectangle = Some(RectangleScan {
                    start: header_end,
                    width,
                    height,
                    encoding,
                    hextile: HextileScan::default(),
                });
            }
            let rectangle = scan.rectangle.as_mut().expect("rectangle header");
            let payload = &bytes[rectangle.start..];
            let length = match rectangle.encoding {
                0 => Some(framebuffer_bytes(rectangle.width, rectangle.height)?),
                1 => Some(4),
                6 | 16 => {
                    if payload.len() < 4 {
                        return Ok(None);
                    }
                    let compressed =
                        u32::from_be_bytes(payload[..4].try_into().expect("compressed length"))
                            as usize;
                    // Check the announced end now, without waiting for the body.
                    message_end(rectangle.start, message_end(4, compressed)?)?;
                    Some(4 + compressed)
                }
                5 => rectangle
                    .hextile
                    .scan(payload, rectangle.width, rectangle.height)?,
                7 | -260 => super::rfb_tight::wire_length(
                    payload,
                    rectangle.width,
                    rectangle.height,
                    rectangle.encoding,
                    self.pixel_format,
                )?,
                -223 | -232 | -313 => Some(0),
                -239 => Some(
                    rectangle.width * rectangle.height * 4
                        + rectangle.width.div_ceil(8) * rectangle.height,
                ),
                -240 if rectangle.width == 0 || rectangle.height == 0 => Some(0),
                -240 => Some(6 + rectangle.width.div_ceil(8) * rectangle.height * 2),
                -308 => {
                    if payload.len() < 4 {
                        return Ok(None);
                    }
                    Some(4 + usize::from(payload[0]) * 16)
                }
                encoding => return Err(format!("rfb_unsupported_framebuffer_encoding:{encoding}")),
            };
            let Some(length) = length else {
                return Ok(None);
            };
            let end = message_end(rectangle.start, length)?;
            if bytes.len() < end {
                return Ok(None);
            }
            scan.offset = end;
            scan.remaining -= 1;
            scan.rectangle = None;
        }
        let length = scan.offset;
        self.framebuffer = None;
        Ok(Some(length))
    }
}

impl HextileScan {
    fn scan(&mut self, bytes: &[u8], width: usize, height: usize) -> Result<Option<usize>, String> {
        let columns = width.div_ceil(16);
        let tiles = columns * height.div_ceil(16);
        while self.tile < tiles {
            let mut offset = self.offset;
            let Some(&flags) = bytes.get(offset) else {
                return Ok(None);
            };
            let mut colors = self.colors;
            colors.begin_tile(flags)?;
            offset += 1;
            let tile_width = (width - (self.tile % columns) * 16).min(16);
            let tile_height = (height - (self.tile / columns) * 16).min(16);
            if flags & 1 != 0 {
                offset = message_end(offset, tile_width * tile_height * 4)?;
            } else {
                if flags & 2 != 0 {
                    offset = message_end(offset, 4)?
                }
                if flags & 4 != 0 {
                    offset = message_end(offset, 4)?
                }
                if flags & 8 != 0 {
                    let Some(&count) = bytes.get(offset) else {
                        return Ok(None);
                    };
                    colors.check_subrectangles(flags, count)?;
                    offset = message_end(
                        offset + 1,
                        usize::from(count) * if flags & 16 != 0 { 6 } else { 2 },
                    )?;
                }
            }
            if bytes.len() < offset {
                return Ok(None);
            }
            self.offset = offset;
            self.colors = colors;
            self.tile += 1;
        }
        Ok(Some(self.offset))
    }
}

#[cfg(test)]
mod tight_framing_tests {
    use super::*;
    include!("../../tests/support/rfb_tight_framing_tests.rs");
}
