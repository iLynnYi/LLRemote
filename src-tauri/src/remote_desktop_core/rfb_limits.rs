// Budgets apply before trusting peer lengths. 128 MiB permits 8K and dual 4K
// canonical frames; a wire message may also contain compressed/tile headers.
pub(super) const MAX_FRAMEBUFFER_BYTES: usize = 128 * 1024 * 1024;
pub(super) const MAX_SERVER_MESSAGE_BYTES: usize = 192 * 1024 * 1024;
pub(super) const MAX_DESKTOP_NAME_BYTES: usize = 64 * 1024;
pub(super) const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;
pub(super) const MAX_CURSOR_BYTES: usize = 1024 * 1024;
pub(super) const RECEIVE_CHUNK_BYTES: usize = 16 * 1024;

pub(super) fn framebuffer_bytes(width: usize, height: usize) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err("rfb_invalid_framebuffer_geometry".into());
    }
    let length = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4));
    match length {
        Some(length) if length <= MAX_FRAMEBUFFER_BYTES => Ok(length),
        _ => Err("rfb_resource_limit:framebuffer".into()),
    }
}

pub(super) fn cursor_bytes(width: usize, height: usize) -> Result<usize, String> {
    let bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= MAX_CURSOR_BYTES)
        .ok_or("rfb_resource_limit:cursor")?;
    Ok(bytes)
}

pub(super) fn message_end(offset: usize, length: usize) -> Result<usize, String> {
    match offset.checked_add(length) {
        Some(end) if end <= MAX_SERVER_MESSAGE_BYTES => Ok(end),
        _ => Err("rfb_resource_limit:server_message".into()),
    }
}

pub(super) fn check_receive_capacity(length: usize, additional: usize) -> Result<(), String> {
    match length.checked_add(additional) {
        Some(total) if total <= MAX_SERVER_MESSAGE_BYTES + RECEIVE_CHUNK_BYTES => Ok(()),
        _ => Err("rfb_resource_limit:receive_buffer".into()),
    }
}
