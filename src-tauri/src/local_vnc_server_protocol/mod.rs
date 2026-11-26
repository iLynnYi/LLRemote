mod client_messages;
mod clipboard;
mod framebuffer;

use crate::local_vnc_server::auth::{create_auth_challenge, create_vnc_auth_response_from_key};
use crate::local_vnc_server::types::{DisplaySize, LocalVncServerConnectionKind};
use crate::local_vnc_server::LocalServerClientWorkers;
use crate::local_vnc_server::{
    default_rfb_pixel_format, inject_key_event, inject_pointer_event,
    send_secure_attention_sequence, LocalVncServerConnectionRegistry, LocalVncServerFrameCache,
    RfbPixelFormat, SharedDisplay, RFB_VERSION,
};
use crate::server_discovery::write_hello as write_server_discovery_hello;
use client_messages::{read_client_message, ClientMessage};
#[cfg(target_os = "windows")]
use clipboard::forward_service_clipboard;
use clipboard::{
    handle_client_cut_text, publish_changed_platform_clipboard, write_extended_clipboard_caps,
    ClipboardPublishState,
};
use framebuffer::{
    is_recoverable_framebuffer_source_error, parse_client_encoding_support, parse_set_pixel_format,
    write_core_framebuffer_update, write_desktop_size_update, write_end_of_continuous_updates,
    write_server_init, RfbFramebufferEncoding, RfbZlibEncoder,
};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

include!("secure_attention.rs");
include!("listener.rs");
include!("handshake_io.rs");
include!("handshake.rs");
include!("client_inbox.rs");
include!("client_connection.rs");
include!("client_loop.rs");
include!("secure_attention_handler.rs");
include!("runtime_status.rs");

#[cfg(test)]
mod tests;
