//! Authenticated Workbench runtime notices over opaque RFB Fence payloads.
//! Separate from Discovery declarations; no framebuffer, secrets or platform APIs.
use serde::{Deserialize, Serialize};

pub const RUNTIME_STATUS_PSEUDO_ENCODING: i32 = -0x5657_5253;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum DesktopAccessState {
    Waiting,
    Available,
    Unavailable,
    CaptureUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum SecureAttentionResult {
    Submitted = 1,
    Unsupported,
    Denied,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ServerRuntimeMessage {
    Desktop {
        desktop: DesktopAccessState,
        #[serde(rename = "secureAttentionAvailable")]
        secure_attention_available: bool,
    },
    SecureAttentionRequest {
        #[serde(rename = "requestId")]
        request_id: u32,
    },
    SecureAttentionResponse {
        #[serde(rename = "requestId")]
        request_id: u32,
        result: SecureAttentionResult,
    },
}

impl ServerRuntimeMessage {
    pub fn encode(self) -> Vec<u8> {
        let (kind, value, flags, id) = match self {
            Self::Desktop {
                desktop,
                secure_attention_available,
            } => (1, desktop as u8, u8::from(secure_attention_available), 0),
            Self::SecureAttentionRequest { request_id } => (2, 0, 0, request_id),
            Self::SecureAttentionResponse { request_id, result } => {
                (3, result as u8, 0, request_id)
            }
        };
        let mut bytes = vec![b'V', b'W', b'R', b'S', 1, kind, value, flags];
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 12 || &bytes[..5] != b"VWRS\x01" {
            return None;
        }
        let id = u32::from_be_bytes(bytes[8..12].try_into().ok()?);
        match bytes[5] {
            1 if id == 0 && bytes[7] <= 1 => Some(Self::Desktop {
                desktop: match bytes[6] {
                    0 => DesktopAccessState::Waiting,
                    1 => DesktopAccessState::Available,
                    2 => DesktopAccessState::Unavailable,
                    3 => DesktopAccessState::CaptureUnavailable,
                    _ => return None,
                },
                secure_attention_available: bytes[7] == 1,
            }),
            2 if id != 0 && bytes[6] == 0 && bytes[7] == 0 => {
                Some(Self::SecureAttentionRequest { request_id: id })
            }
            3 if id != 0 && bytes[7] == 0 => Some(Self::SecureAttentionResponse {
                request_id: id,
                result: match bytes[6] {
                    1 => SecureAttentionResult::Submitted,
                    2 => SecureAttentionResult::Unsupported,
                    3 => SecureAttentionResult::Denied,
                    4 => SecureAttentionResult::Failed,
                    _ => return None,
                },
            }),
            _ => None,
        }
    }
}
