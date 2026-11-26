use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub(super) struct ClipboardTransfer {
    data: Zeroizing<Vec<u8>>,
    total: usize,
    last_update: Instant,
}

/// Reassemble bounded clipboard bytes before the user process writes them.
pub(super) fn append_clipboard_chunk(
    transfers: &mut HashMap<String, ClipboardTransfer>,
    transfer_id: String,
    offset: u32,
    total: u32,
    data: &str,
) -> Result<Option<Zeroizing<String>>, String> {
    let total = total as usize;
    if total > 1024 * 1024 || transfer_id.len() > 64 {
        return Err("clipboard_resource_limit".into());
    }
    let chunk = Zeroizing::new(
        STANDARD
            .decode(data)
            .map_err(|_| "clipboard_invalid_chunk")?,
    );
    if chunk.len() > 10 * 1024 {
        return Err("clipboard_resource_limit".into());
    }
    transfers.retain(|_, transfer| transfer.last_update.elapsed() < Duration::from_secs(10));
    if offset == 0 {
        if transfers.len() >= 4 && !transfers.contains_key(&transfer_id) {
            return Err("clipboard_resource_limit".into());
        }
        transfers.insert(
            transfer_id.clone(),
            ClipboardTransfer {
                data: Zeroizing::new(Vec::with_capacity(total)),
                total,
                last_update: Instant::now(),
            },
        );
    }
    let transfer = transfers
        .get_mut(&transfer_id)
        .ok_or("clipboard_transfer_missing")?;
    if transfer.total != total
        || transfer.data.len() != offset as usize
        || transfer.data.len().saturating_add(chunk.len()) > total
    {
        transfers.remove(&transfer_id);
        return Err("clipboard_invalid_chunk".into());
    }
    transfer.data.extend_from_slice(&chunk);
    transfer.last_update = Instant::now();
    if transfer.data.len() != total {
        return Ok(None);
    }
    let complete = transfers
        .remove(&transfer_id)
        .ok_or("clipboard_transfer_missing")?;
    let text = std::str::from_utf8(&complete.data).map_err(|_| "clipboard_invalid_utf8")?;
    Ok(Some(Zeroizing::new(text.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_chunks_complete_utf8_only_after_all_bytes_arrive() {
        let mut transfers = HashMap::new();
        let text = "明天你好";
        let bytes = text.as_bytes();
        assert!(append_clipboard_chunk(
            &mut transfers,
            "one".into(),
            0,
            bytes.len() as u32,
            &STANDARD.encode(&bytes[..4])
        )
        .unwrap()
        .is_none());
        assert_eq!(
            append_clipboard_chunk(
                &mut transfers,
                "one".into(),
                4,
                bytes.len() as u32,
                &STANDARD.encode(&bytes[4..])
            )
            .unwrap()
            .as_deref()
            .map(String::as_str),
            Some(text)
        );
        assert!(transfers.is_empty());
    }

    #[test]
    fn clipboard_chunks_reject_out_of_order_and_oversized_data() {
        let mut transfers = HashMap::new();
        assert!(append_clipboard_chunk(
            &mut transfers,
            "one".into(),
            0,
            8,
            &STANDARD.encode(b"abc")
        )
        .unwrap()
        .is_none());
        assert_eq!(
            append_clipboard_chunk(&mut transfers, "one".into(), 4, 8, &STANDARD.encode(b"def"))
                .unwrap_err(),
            "clipboard_invalid_chunk"
        );
        assert!(transfers.is_empty());
        assert_eq!(
            append_clipboard_chunk(&mut transfers, "two".into(), 0, 1024 * 1024 + 1, "")
                .unwrap_err(),
            "clipboard_resource_limit"
        );
    }
}
