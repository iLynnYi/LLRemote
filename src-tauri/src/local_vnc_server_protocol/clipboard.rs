use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use crate::local_vnc_server::{get_clipboard_text_for_vnc, set_clipboard_text_from_vnc};

include!("clipboard/types.rs");
include!("clipboard/client.rs");
include!("clipboard/publish.rs");
include!("clipboard/decode.rs");
include!("clipboard/wire.rs");

#[cfg(target_os = "windows")]
pub(super) fn forward_service_clipboard(signed_length: i32, payload: &[u8], view_only: bool) {
    if view_only || signed_length < 0 {
        return;
    }
    let decoded = decode_legacy_cut_text(payload);
    if let Err(code) = crate::windows_service::write_clipboard_from_broker(&decoded.text) {
        eprintln!("local VNC clipboard: service forwarding failed: {code}");
    }
}
