use std::io::Read;
use std::net::TcpStream;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::{RfbDeadlineIo, RFB_IO_POLL};
use crate::remote_desktop_core::{FrameInterest, FramebufferRect};

const MAX_CLIENT_ENCODING_COUNT: usize = 1024;
pub(super) const MAX_CLIENT_CUT_TEXT_BYTES: usize = 1024 * 1024 + 4;
const CLIENT_MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) enum ClientMessage {
    PixelFormat([u8; 19]),
    Encodings(Vec<u8>),
    FrameRequest(FrameInterest),
    Key {
        down: bool,
        keysym: u32,
    },
    Pointer {
        buttons: u8,
        x: u16,
        y: u16,
    },
    CutText {
        signed_length: i32,
        payload: Vec<u8>,
    },
    Continuous {
        enabled: bool,
        region: FramebufferRect,
    },
    Fence {
        flags: u32,
        payload: Vec<u8>,
    },
}

impl ClientMessage {
    pub(super) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Encodings(payload)
                | Self::CutText { payload, .. }
                | Self::Fence { payload, .. } => payload.capacity(),
                _ => 0,
            }
    }
}

// Once a message type arrives, every field shares a deadline.
pub(super) fn read_client_message(
    stream: &mut TcpStream,
    stop_requested: &AtomicBool,
) -> Result<Option<ClientMessage>, std::io::Error> {
    let mut message_type = [0];
    stream.set_read_timeout(Some(RFB_IO_POLL))?;
    match stream.read(&mut message_type) {
        Ok(0) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed",
            ))
        }
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::Interrupted
            ) =>
        {
            return Ok(None)
        }
        Err(error) => return Err(error),
    }
    let mut io = RfbDeadlineIo {
        stream,
        stop_requested,
        deadline: Instant::now() + CLIENT_MESSAGE_TIMEOUT,
    };
    let message = match message_type[0] {
        0 => {
            let mut payload = [0; 19];
            io.read_exact(&mut payload)?;
            ClientMessage::PixelFormat(payload)
        }
        2 => {
            let mut header = [0; 3];
            io.read_exact(&mut header)?;
            let count = usize::from(u16::from_be_bytes([header[1], header[2]]));
            if count > MAX_CLIENT_ENCODING_COUNT {
                return Err(resource_limit("client_encodings"));
            }
            let mut encodings = vec![0; count * 4];
            io.read_exact(&mut encodings)?;
            ClientMessage::Encodings(encodings)
        }
        3 => {
            let mut payload = [0; 9];
            io.read_exact(&mut payload)?;
            ClientMessage::FrameRequest(FrameInterest {
                incremental: payload[0] != 0,
                region: Some(parse_update_region(&payload)),
            })
        }
        4 => {
            let mut payload = [0; 7];
            io.read_exact(&mut payload)?;
            ClientMessage::Key {
                down: payload[0] != 0,
                keysym: u32::from_be_bytes(payload[3..7].try_into().unwrap()),
            }
        }
        5 => {
            let mut payload = [0; 5];
            io.read_exact(&mut payload)?;
            ClientMessage::Pointer {
                buttons: payload[0],
                x: u16::from_be_bytes([payload[1], payload[2]]),
                y: u16::from_be_bytes([payload[3], payload[4]]),
            }
        }
        6 => {
            let mut header = [0; 7];
            io.read_exact(&mut header)?;
            let signed_length = i32::from_be_bytes(header[3..7].try_into().unwrap());
            let length = signed_length.unsigned_abs() as usize;
            if length > MAX_CLIENT_CUT_TEXT_BYTES {
                return Err(resource_limit("client_clipboard"));
            }
            let mut payload = vec![0; length];
            io.read_exact(&mut payload)?;
            ClientMessage::CutText {
                signed_length,
                payload,
            }
        }
        150 => {
            let mut payload = [0; 9];
            io.read_exact(&mut payload)?;
            ClientMessage::Continuous {
                enabled: payload[0] != 0,
                region: parse_update_region(&payload),
            }
        }
        248 => {
            let mut header = [0; 8];
            io.read_exact(&mut header)?;
            if usize::from(header[7]) > crate::remote_desktop_core::rfb_fence::MAX_FENCE_PAYLOAD {
                return Err(resource_limit("fence_payload"));
            }
            let mut payload = vec![0; usize::from(header[7])];
            io.read_exact(&mut payload)?;
            ClientMessage::Fence {
                flags: u32::from_be_bytes(header[3..7].try_into().unwrap()),
                payload,
            }
        }
        250 => {
            io.read_exact(&mut [0; 3])?;
            eprintln!("local VNC server: ignored unsupported XVP client message");
            return Ok(None);
        }
        251 => {
            let mut header = [0; 7];
            io.read_exact(&mut header)?;
            let mut screens = vec![0; usize::from(header[5]) * 16];
            io.read_exact(&mut screens)?;
            eprintln!("local VNC server: ignored unsupported SetDesktopSize client message");
            return Ok(None);
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("unsupported_client_message: type={}", message_type[0]),
            ))
        }
    };
    Ok(Some(message))
}

fn resource_limit(area: &'static str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("rfb_resource_limit:{area}"),
    )
}

pub(super) fn parse_update_region(payload: &[u8; 9]) -> FramebufferRect {
    FramebufferRect {
        x: u32::from(u16::from_be_bytes([payload[1], payload[2]])),
        y: u32::from(u16::from_be_bytes([payload[3], payload[4]])),
        width: u32::from(u16::from_be_bytes([payload[5], payload[6]])),
        height: u32::from(u16::from_be_bytes([payload[7], payload[8]])),
    }
}
