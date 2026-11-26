use super::rfb_limits::{check_receive_capacity, framebuffer_bytes, MAX_DESKTOP_NAME_BYTES};
use super::RfbAuthentication;
use super::{ClientRfbPixelFormat, FramebufferGeometry, PixelFormat};

#[cfg(test)]
#[path = "../../tests/support/rfb_auth_lifetime_tests.rs"]
mod auth_lifetime_tests;

const RFB_3_3: &[u8] = b"RFB 003.003\n";
const RFB_3_7: &[u8] = b"RFB 003.007\n";
const RFB_3_8: &[u8] = b"RFB 003.008\n";
const SECURITY_NONE: u8 = 1;
const SECURITY_VNC_AUTH: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RfbClientHandshakeEvent {
    Ready {
        geometry: FramebufferGeometry,
        desktop_name: String,
        pixel_format: ClientRfbPixelFormat,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandshakeState {
    AwaitingVersion,
    AwaitingSecurity,
    AwaitingSecurityResult,
    AwaitingVncChallenge,
    AwaitingServerInit,
    Ready,
    Failed,
}

/// Stateful RFB 3.3/3.7/3.8 initial handshake. It owns protocol bytes and bounded
/// short-lived authentication material, without retaining a plaintext password;
/// TCP/WebSocket IO and authentication prompts remain at the connection boundary.
pub struct RfbClientHandshake {
    state: HandshakeState,
    selected_version: Option<(u8, u8)>,
    buffered: Vec<u8>,
    events: Vec<RfbClientHandshakeEvent>,
    pub(super) authentication: RfbAuthentication,
}

impl Default for RfbClientHandshake {
    fn default() -> Self {
        Self {
            state: HandshakeState::AwaitingVersion,
            selected_version: None,
            buffered: Vec::new(),
            events: Vec::new(),
            authentication: RfbAuthentication::default(),
        }
    }
}

impl RfbClientHandshake {
    pub fn with_password(password: Option<String>) -> Self {
        let mut handshake = Self::default();
        handshake.authentication = RfbAuthentication::from_password(password);
        handshake
    }
    /// Accepts arbitrary stream fragments and returns client-originated bytes in
    /// protocol order. A caller writes each returned buffer unchanged to RFB TCP.
    pub fn consume(&mut self, input: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        if self.state == HandshakeState::Failed {
            return Err("rfb_handshake_failed".into());
        }
        let result = self.consume_inner(input);
        if result.is_err() {
            self.authentication.clear();
            self.buffered.clear();
            self.events.clear();
            self.state = HandshakeState::Failed;
        }
        result
    }

    fn consume_inner(&mut self, input: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        check_receive_capacity(self.buffered.len(), input.len())?;
        self.buffered.extend_from_slice(input);
        let mut writes = Vec::new();
        loop {
            match self.state {
                HandshakeState::AwaitingVersion => {
                    let Some(banner) = self.take_exact(12) else {
                        break;
                    };
                    let (major, minor) = parse_version_banner(&banner)?;
                    let (selected, response) = select_version(major, minor)?;
                    self.selected_version = Some(selected);
                    self.state = HandshakeState::AwaitingSecurity;
                    writes.push(response.to_vec());
                }
                HandshakeState::AwaitingSecurity => {
                    let (_, minor) = self
                        .selected_version
                        .expect("version selected before security");
                    if minor == 3 {
                        let Some(kind) = self.take_u32() else { break };
                        match kind {
                            kind if kind == u32::from(SECURITY_NONE) => {
                                self.authentication.clear();
                                self.state = HandshakeState::AwaitingServerInit;
                                writes.push(vec![1]); // ClientInit: shared
                            }
                            kind if kind == u32::from(SECURITY_VNC_AUTH) => {
                                if !self.authentication.has_key()? {
                                    return Err("rfb_credentials_required".into());
                                }
                                self.state = HandshakeState::AwaitingVncChallenge;
                            }
                            _ => {
                                return Err(format!("rfb_unsupported_security_type:{kind}"));
                            }
                        }
                    } else {
                        let Some(count) = self.buffered.first().copied() else {
                            break;
                        };
                        if count == 0 {
                            return Err("rfb_server_offered_no_security_types".into());
                        }
                        let Some(types) = self.take_exact(usize::from(count) + 1) else {
                            break;
                        };
                        if types[1..].contains(&SECURITY_NONE) {
                            self.authentication.clear();
                            writes.push(vec![SECURITY_NONE]);
                            if minor == 7 {
                                self.state = HandshakeState::AwaitingServerInit;
                                writes.push(vec![1]); // RFB 3.7 None has no SecurityResult.
                            } else {
                                self.state = HandshakeState::AwaitingSecurityResult;
                            }
                        } else if types[1..].contains(&SECURITY_VNC_AUTH) {
                            if !self.authentication.has_key()? {
                                return Err("rfb_credentials_required".into());
                            }
                            self.state = HandshakeState::AwaitingVncChallenge;
                            writes.push(vec![SECURITY_VNC_AUTH]);
                        } else {
                            return Err("rfb_unsupported_security_types".into());
                        }
                    }
                }
                HandshakeState::AwaitingVncChallenge => {
                    let Some(challenge) = self.take_exact(16) else {
                        break;
                    };
                    let challenge: [u8; 16] = challenge.try_into().expect("exact VNC challenge");
                    writes.push(self.authentication.respond(&challenge)?.to_vec());
                    self.state = HandshakeState::AwaitingSecurityResult;
                }
                HandshakeState::AwaitingSecurityResult => {
                    let Some(result) = self.take_u32() else { break };
                    if result != 0 {
                        return Err(format!("rfb_security_failed:{result}"));
                    }
                    self.state = HandshakeState::AwaitingServerInit;
                    writes.push(vec![1]); // ClientInit: shared
                }
                HandshakeState::AwaitingServerInit => {
                    let Some(header) = self.buffered.get(..24) else {
                        break;
                    };
                    let name_length =
                        u32::from_be_bytes(header[20..24].try_into().expect("exact name length"));
                    framebuffer_bytes(
                        usize::from(u16::from_be_bytes([header[0], header[1]])),
                        usize::from(u16::from_be_bytes([header[2], header[3]])),
                    )?;
                    if name_length as u64 > MAX_DESKTOP_NAME_BYTES as u64 {
                        return Err("rfb_resource_limit:desktop_name".into());
                    }
                    let total = 24usize
                        .checked_add(
                            usize::try_from(name_length)
                                .map_err(|_| "rfb_desktop_name_length_overflow")?,
                        )
                        .ok_or_else(|| "rfb_desktop_name_length_overflow".to_string())?;
                    let Some(server_init) = self.take_exact(total) else {
                        break;
                    };
                    let event = parse_server_init(&server_init)?;
                    let geometry = match &event {
                        RfbClientHandshakeEvent::Ready { geometry, .. } => *geometry,
                    };
                    self.events.push(event);
                    self.state = HandshakeState::Ready;
                    writes.push(set_pixel_format_message(ClientRfbPixelFormat::core_bgra32()));
                    writes.push(set_encodings_message());
                    writes.push(framebuffer_update_request(geometry, false));
                }
                HandshakeState::Ready => break,
                HandshakeState::Failed => return Err("rfb_handshake_failed".into()),
            }
        }
        Ok(writes)
    }

    pub fn take_events(&mut self) -> Vec<RfbClientHandshakeEvent> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn take_pending_bytes(&mut self) -> Vec<u8> {
        if self.state == HandshakeState::Ready {
            std::mem::take(&mut self.buffered)
        } else {
            Vec::new()
        }
    }

    fn take_u32(&mut self) -> Option<u32> {
        self.take_exact(4)
            .map(|bytes| u32::from_be_bytes(bytes.try_into().expect("exact u32")))
    }

    fn take_exact(&mut self, length: usize) -> Option<Vec<u8>> {
        (self.buffered.len() >= length).then(|| self.buffered.drain(..length).collect())
    }
}

impl Drop for RfbClientHandshake {
    fn drop(&mut self) {
        self.authentication.clear();
    }
}

fn parse_version_banner(banner: &[u8]) -> Result<(u8, u8), String> {
    if banner.len() != 12 || &banner[..4] != b"RFB " || banner[11] != b'\n' || banner[7] != b'.' {
        return Err("rfb_invalid_version_banner".into());
    }
    let digits = |bytes: &[u8]| -> Result<u8, String> {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| "rfb_invalid_version_banner".into())
    };
    Ok((digits(&banner[4..7])?, digits(&banner[8..11])?))
}

fn select_version(major: u8, minor: u8) -> Result<((u8, u8), &'static [u8]), String> {
    if major != 3 {
        return Err("rfb_unsupported_protocol_version".into());
    }
    if minor >= 8 {
        Ok(((3, 8), RFB_3_8))
    } else if minor >= 7 {
        Ok(((3, 7), RFB_3_7))
    } else if minor == 3 {
        Ok(((3, 3), RFB_3_3))
    } else {
        Err("rfb_unsupported_protocol_version".into())
    }
}

fn parse_server_init(bytes: &[u8]) -> Result<RfbClientHandshakeEvent, String> {
    let width = u16::from_be_bytes(bytes[0..2].try_into().expect("server init width")) as u32;
    let height = u16::from_be_bytes(bytes[2..4].try_into().expect("server init height")) as u32;
    let pixel_format = ClientRfbPixelFormat {
        bits_per_pixel: bytes[4],
        depth: bytes[5],
        big_endian: bytes[6] != 0,
        true_color: bytes[7] != 0,
        red_max: u16::from_be_bytes(bytes[8..10].try_into().expect("red max")),
        green_max: u16::from_be_bytes(bytes[10..12].try_into().expect("green max")),
        blue_max: u16::from_be_bytes(bytes[12..14].try_into().expect("blue max")),
        red_shift: bytes[14],
        green_shift: bytes[15],
        blue_shift: bytes[16],
    };
    let desktop_name =
        String::from_utf8(bytes[24..].to_vec()).map_err(|_| "rfb_desktop_name_not_utf8")?;
    Ok(RfbClientHandshakeEvent::Ready {
        geometry: FramebufferGeometry {
            width,
            height,
            pixel_format: PixelFormat::Bgra8888,
        },
        desktop_name,
        pixel_format,
    })
}

fn set_pixel_format_message(format: ClientRfbPixelFormat) -> Vec<u8> {
    let mut message = vec![
        0,
        0,
        0,
        0,
        format.bits_per_pixel,
        format.depth,
        u8::from(format.big_endian),
        u8::from(format.true_color),
    ];
    message.extend_from_slice(&format.red_max.to_be_bytes());
    message.extend_from_slice(&format.green_max.to_be_bytes());
    message.extend_from_slice(&format.blue_max.to_be_bytes());
    message.extend_from_slice(&[
        format.red_shift,
        format.green_shift,
        format.blue_shift,
        0,
        0,
        0,
    ]);
    message
}

fn set_encodings_message() -> Vec<u8> {
    set_encodings_message_with_quality(6, 2).expect("static RFB quality profile")
}

pub(crate) fn set_encodings_message_with_quality(
    quality_level: u8,
    compression_level: u8,
) -> Result<Vec<u8>, String> {
    if quality_level > 9 || compression_level > 9 {
        return Err("rfb_quality_profile_out_of_range".into());
    }
    let encodings: [i32; 20] = [
        7,
        -260,
        -317, // Tight BasicCompression without zlib; distinct from TightPNG.
        16,
        5,
        6,
        1,
        0,
        -256 + i32::from(compression_level),
        -32 + i32::from(quality_level),
        -239,
        -240,
        -232,
        -308,
        -312, // Fence enables optional Server Discovery.
        super::RUNTIME_STATUS_PSEUDO_ENCODING,
        -313,
        0xc0a1_e5ce_u32 as i32,
        -223,
        -224,
    ]; // Tight/TightPNG, ZRLE, Hextile, Zlib, CopyRect, Raw and negotiated pseudo encodings
    let mut message = vec![2, 0];
    message.extend_from_slice(&(encodings.len() as u16).to_be_bytes());
    for encoding in encodings {
        message.extend_from_slice(&encoding.to_be_bytes());
    }
    Ok(message)
}

pub(crate) fn framebuffer_update_request(
    geometry: FramebufferGeometry,
    incremental: bool,
) -> Vec<u8> {
    let width = u16::try_from(geometry.width).unwrap_or(u16::MAX);
    let height = u16::try_from(geometry.height).unwrap_or(u16::MAX);
    let mut message = vec![3, u8::from(incremental)];
    message.extend_from_slice(&[0, 0, 0, 0]);
    message.extend_from_slice(&width.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message
}
