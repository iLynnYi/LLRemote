//! Standard Fence framing, independent of the private Discovery payload format.

pub(crate) const FENCE_MESSAGE: u8 = 248;
pub(crate) const FENCE_FLAG_REQUEST: u32 = 1 << 31;
pub(crate) const MAX_FENCE_PAYLOAD: usize = 64;

/// Serializes either direction of the bounded RFB Fence message. Callers choose
/// only the synchronization flags their processing model can actually honor.
pub(crate) fn fence_message(flags: u32, payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() > MAX_FENCE_PAYLOAD {
        return Err("rfb_resource_limit:fence_payload".into());
    }
    let mut bytes = vec![FENCE_MESSAGE, 0, 0, 0];
    bytes.extend_from_slice(&flags.to_be_bytes());
    bytes.push(payload.len() as u8);
    bytes.extend_from_slice(payload);
    Ok(bytes)
}
