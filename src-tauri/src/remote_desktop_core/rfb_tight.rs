use super::rfb_client::ClientRfbPixelFormat;

/// The encoding number changes the meaning of control 0xA. Framing and decoding
/// must use the same interpretation before reading any variable-length body.
#[derive(Clone, Copy)]
pub(super) enum TightMode {
    Fill,
    Jpeg,
    Png,
    Basic {
        stream: usize,
        explicit_filter: bool,
        no_zlib: bool,
    },
}

impl TightMode {
    pub(super) fn parse(control: u8, encoding: i32) -> Result<Self, String> {
        let mode = control >> 4;
        match mode {
            8 => Ok(Self::Fill),
            9 => Ok(Self::Jpeg),
            10 if encoding == -260 => Ok(Self::Png),
            0..=7 if encoding == 7 => Ok(Self::Basic {
                stream: usize::from(mode & 3),
                explicit_filter: mode & 4 != 0,
                no_zlib: false,
            }),
            10 | 14 if encoding == 7 => Ok(Self::Basic {
                stream: 0,
                explicit_filter: mode == 14,
                no_zlib: true,
            }),
            _ => Err("rfb_unsupported_tight_control".into()),
        }
    }
    pub(super) fn is_image(self) -> bool {
        matches!(self, Self::Jpeg | Self::Png)
    }
}

// Tight senders must split at 2048 pixels. Legacy servers also send wider
// rectangles: receive uses the shared framebuffer/update/session budgets and
// checks framebuffer bounds before decode, rather than a fixed row buffer.
pub(super) fn filtered_length(
    filter: u8,
    width: usize,
    height: usize,
    pixel_bytes: usize,
    palette_size: usize,
) -> Result<usize, String> {
    let pixel_count = width.checked_mul(height).ok_or("rfb_tight_size_overflow")?;
    match filter {
        0 | 2 => pixel_count
            .checked_mul(pixel_bytes)
            .ok_or_else(|| "rfb_tight_size_overflow".into()),
        1 if !(2..=256).contains(&palette_size) => Err("rfb_tight_invalid_palette".into()),
        1 if palette_size == 2 => width
            .div_ceil(8)
            .checked_mul(height)
            .ok_or_else(|| "rfb_tight_size_overflow".into()),
        1 => Ok(pixel_count),
        _ => Err("rfb_unsupported_tight_filter".into()),
    }
}

/// Returns the announced length as soon as its fields are available. The outer
/// framer checks the aggregate message budget before waiting for the body.
pub(super) fn wire_length(
    bytes: &[u8],
    width: usize,
    height: usize,
    encoding: i32,
    format: ClientRfbPixelFormat,
) -> Result<Option<usize>, String> {
    let Some(&control) = bytes.first() else {
        return Ok(None);
    };
    let mode = TightMode::parse(control, encoding)?;
    let pixel_bytes = format.tight_pixel_bytes();
    let mut offset = 1usize;
    let explicit_filter = match mode {
        TightMode::Fill => return Ok(Some(1 + pixel_bytes)),
        TightMode::Jpeg | TightMode::Png => {
            let Some((length, prefix)) = compact_length_at(&bytes[offset..])? else {
                return Ok(None);
            };
            offset += prefix;
            return Ok(Some(offset + length));
        }
        TightMode::Basic {
            explicit_filter, ..
        } => explicit_filter,
    };
    let filter = if explicit_filter {
        let Some(&filter) = bytes.get(offset) else {
            return Ok(None);
        };
        offset += 1;
        filter
    } else {
        0
    };
    let mut palette_size = 0;
    if filter == 1 {
        let Some(&count) = bytes.get(offset) else {
            return Ok(None);
        };
        palette_size = usize::from(count) + 1;
        offset += 1;
        // Validate the palette before waiting for its body.
        filtered_length(filter, width, height, pixel_bytes, palette_size)?;
        offset += palette_size * pixel_bytes;
    }
    let expected = filtered_length(filter, width, height, pixel_bytes, palette_size)?;
    if expected < 12 {
        return Ok(Some(offset + expected));
    }
    if bytes.len() < offset {
        return Ok(None);
    }
    let Some((length, prefix)) = compact_length_at(&bytes[offset..])? else {
        return Ok(None);
    };
    if matches!(mode, TightMode::Basic { no_zlib: true, .. }) && length != expected {
        return Err(format!(
            "rfb_tight_decoded_length_mismatch:framing,0,{width},{height},{length},0,{expected},{length}",
        ));
    }
    offset += prefix;
    Ok(Some(offset + length))
}

fn compact_length_at(bytes: &[u8]) -> Result<Option<(usize, usize)>, String> {
    let Some(first) = bytes.first().copied() else {
        return Ok(None);
    };
    let mut length = usize::from(first & 0x7f);
    if first & 0x80 == 0 {
        return Ok(Some((length, 1)));
    }
    let Some(second) = bytes.get(1).copied() else {
        return Ok(None);
    };
    length |= usize::from(second & 0x7f) << 7;
    if second & 0x80 == 0 {
        return Ok(Some((length, 2)));
    }
    let Some(third) = bytes.get(2).copied() else {
        return Ok(None);
    };
    length |= usize::from(third) << 14;
    Ok(Some((length, 3)))
}
