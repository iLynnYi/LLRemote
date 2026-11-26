//! LL Remote discovery frames transported inside RFB Fence payloads.
//!
//! This is deliberately independent from the local-server implementation: a
//! native client must be able to validate and reassemble frames from any peer
//! without giving a UI or WebSocket layer access to raw RFB bytes.

use serde_json::Value;

pub const FENCE_WIRE_MAX_PAYLOAD: usize = 64;
pub const FENCE_WIRE_HEADER_LENGTH: usize = 10;
pub const FENCE_WIRE_MAX_BODY_BYTES: usize = 16 * 1024;

const MAGIC: &[u8; 4] = b"VWSD";
const VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryFrameType {
    Hello,
    Discover,
    Manifest,
    Error,
}

impl DiscoveryFrameType {
    fn wire(self) -> u8 {
        match self {
            Self::Hello => 1,
            Self::Discover => 2,
            Self::Manifest => 3,
            Self::Error => 4,
        }
    }

    fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Hello),
            2 => Some(Self::Discover),
            3 => Some(Self::Manifest),
            4 => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryFrame {
    pub frame_type: DiscoveryFrameType,
    pub request_id: u8,
    pub sequence: u8,
    pub total: u8,
    pub more: bool,
    pub body: Vec<u8>,
}

pub fn encode_discovery_frames(
    frame_type: DiscoveryFrameType,
    request_id: u8,
    body: &Value,
) -> Result<Vec<Vec<u8>>, String> {
    let body =
        serde_json::to_vec(body).map_err(|error| format!("fence_json_encode_failed:{error}"))?;
    if body.len() > FENCE_WIRE_MAX_BODY_BYTES {
        return Err("fence_body_too_large".into());
    }
    let chunk_size = FENCE_WIRE_MAX_PAYLOAD - FENCE_WIRE_HEADER_LENGTH;
    let total = body.len().div_ceil(chunk_size).max(1);
    if total > usize::from(u8::MAX) {
        return Err("fence_body_too_fragmented".into());
    }
    Ok((0..total)
        .map(|sequence| {
            let start = sequence * chunk_size;
            let end = (start + chunk_size).min(body.len());
            let mut frame = Vec::with_capacity(FENCE_WIRE_HEADER_LENGTH + end - start);
            frame.extend_from_slice(MAGIC);
            frame.extend_from_slice(&[
                VERSION,
                frame_type.wire(),
                request_id,
                u8::from(sequence + 1 < total),
                sequence as u8,
                total as u8,
            ]);
            frame.extend_from_slice(&body[start..end]);
            frame
        })
        .collect())
}

pub fn decode_discovery_frame(payload: &[u8]) -> Option<DiscoveryFrame> {
    if !(FENCE_WIRE_HEADER_LENGTH..=FENCE_WIRE_MAX_PAYLOAD).contains(&payload.len())
        || payload.get(..4) != Some(MAGIC)
        || payload[4] != VERSION
    {
        return None;
    }
    let frame_type = DiscoveryFrameType::from_wire(payload[5])?;
    let request_id = payload[6];
    let more = payload[7] == 1;
    let sequence = payload[8];
    let total = payload[9];
    if total == 0 || sequence >= total || (more && sequence + 1 == total) {
        return None;
    }
    Some(DiscoveryFrame {
        frame_type,
        request_id,
        sequence,
        total,
        more,
        body: payload[FENCE_WIRE_HEADER_LENGTH..].to_vec(),
    })
}

pub fn assemble_discovery_frames(frames: &[DiscoveryFrame]) -> Option<Value> {
    let first = frames.first()?;
    if frames.len() != usize::from(first.total)
        || frames.iter().enumerate().any(|(index, frame)| {
            frame.frame_type != first.frame_type
                || frame.request_id != first.request_id
                || frame.total != first.total
                || frame.sequence != index as u8
        })
    {
        return None;
    }
    let body_len = frames.iter().map(|frame| frame.body.len()).sum::<usize>();
    if body_len > FENCE_WIRE_MAX_BODY_BYTES {
        return None;
    }
    let mut body = Vec::with_capacity(body_len);
    for frame in frames {
        body.extend_from_slice(&frame.body);
    }
    serde_json::from_slice(&body).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_json_round_trips() {
        let body = serde_json::json!({ "text": "x".repeat(128) });
        let encoded = encode_discovery_frames(DiscoveryFrameType::Manifest, 7, &body).unwrap();
        assert!(encoded.len() > 1);
        let decoded = encoded
            .iter()
            .map(|frame| decode_discovery_frame(frame).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(assemble_discovery_frames(&decoded), Some(body));
    }
}
