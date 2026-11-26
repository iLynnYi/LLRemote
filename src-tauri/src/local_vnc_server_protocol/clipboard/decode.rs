#[derive(Debug, PartialEq, Eq)]
pub(super) struct DecodedCutText {
    pub(super) text: String,
    pub(super) encoding: &'static str,
}

pub(super) fn decode_legacy_cut_text(payload: &[u8]) -> DecodedCutText {
    if let Ok(text) = std::str::from_utf8(payload) {
        return DecodedCutText {
            text: text.to_string(),
            encoding: "utf-8",
        };
    }

    DecodedCutText {
        text: payload.iter().map(|byte| char::from(*byte)).collect(),
        encoding: "latin-1",
    }
}

pub(super) fn decode_extended_clipboard_provide(payload: &[u8]) -> Result<Option<String>, String> {
    if payload.len() < 4 {
        return Err("payload_too_short".to_string());
    }

    let flags = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let formats = flags & EXTENDED_CLIPBOARD_FORMAT_MASK;
    if (formats & EXTENDED_CLIPBOARD_FORMAT_TEXT) == 0 {
        return Ok(None);
    }

    let mut inflated = Vec::new();
    ZlibDecoder::new(&payload[4..])
        .take((super::client_messages::MAX_CLIENT_CUT_TEXT_BYTES + 1) as u64)
        .read_to_end(&mut inflated)
        .map_err(|error| format!("zlib_inflate_failed: {error}"))?;
    if inflated.len() > super::client_messages::MAX_CLIENT_CUT_TEXT_BYTES {
        return Err("clipboard_resource_limit".into());
    }

    let mut offset = 0_usize;
    for index in 0..16 {
        let format = 1_u32 << index;
        if (formats & format) == 0 {
            continue;
        }

        if inflated.len().saturating_sub(offset) < 4 {
            return Err("format_size_missing".to_string());
        }
        let size = u32::from_be_bytes([
            inflated[offset],
            inflated[offset + 1],
            inflated[offset + 2],
            inflated[offset + 3],
        ]) as usize;
        offset += 4;

        if inflated.len().saturating_sub(offset) < size {
            return Err("format_payload_truncated".to_string());
        }
        let data = &inflated[offset..offset + size];
        offset += size;

        if format == EXTENDED_CLIPBOARD_FORMAT_TEXT {
            let mut text = String::from_utf8(data.to_vec())
                .map_err(|error| format!("utf8_decode_failed: {error}"))?;
            if text.ends_with('\0') {
                text.pop();
            }
            return Ok(Some(text.replace("\r\n", "\n")));
        }
    }

    Ok(None)
}
