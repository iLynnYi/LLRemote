pub(crate) mod auth;
pub(crate) mod framebuffer;
mod permissions;
pub(crate) mod types;

mod client_workers;
mod connections;
pub(crate) mod desktop_relay;
mod frame_cache;

pub(crate) use client_workers::LocalServerClientWorkers;
pub(crate) use connections::LocalVncServerConnectionRegistry;
pub(crate) use frame_cache::LocalVncServerFrameCache;

use crate::local_agent_service::{
    failed_agent_overview, start_local_agent, stopped_agent_overview, LocalAgentOverview,
    LocalAgentRuntime,
};
pub(crate) use crate::local_vnc_server_platform::{
    control_remote_desktop, get_clipboard_text_for_vnc, inject_key_event, inject_pointer_event,
    send_secure_attention_sequence, set_clipboard_text_from_vnc, wake_remote_display,
    CaptureFailure, PlatformFrameCapture,
};
pub use crate::local_vnc_server_platform::{
    list_local_vnc_server_displays, mac_keycode_for_keyboard_input, mac_keycode_for_keysym,
    map_vnc_point_to_display, map_vnc_point_to_screen, scroll_delta_for_pointer_transition,
};
use crate::local_vnc_server_protocol::{run_listener, RfbServerContext};
use crate::server::local_address_candidates;
use auth::create_vnc_auth_key;
pub use auth::create_vnc_auth_response;
pub use framebuffer::{
    constrained_framebuffer_size, convert_rfb_framebuffer_to_pixel_format,
    default_rfb_pixel_format, framebuffer_size_for_resolution, refresh_interval,
    scale_captured_framebuffer_to_rfb, scale_rgba_to_rfb_framebuffer,
};
use permissions::collect_local_vnc_server_permissions;
pub(crate) use permissions::pending_local_vnc_server_permissions;
pub use permissions::{
    get_local_vnc_server_permissions, open_local_vnc_server_permission_settings,
    request_local_vnc_server_permission,
};
use std::net::TcpListener;
#[cfg(test)]
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
pub use types::*;
use uuid::Uuid;

pub(crate) const RFB_VERSION: &[u8; 12] = b"RFB 003.008\n";
pub(crate) const DEFAULT_FRAMEBUFFER_WIDTH: u16 = 1280;
pub(crate) const DEFAULT_FRAMEBUFFER_HEIGHT: u16 = 800;
const DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE: LocalVncServerRefreshMode =
    LocalVncServerRefreshMode::Responsive;
pub(crate) const DESKTOP_NAME: &str = "LL Remote Local Server";

include!("display.rs");
include!("runtime.rs");
include!("capture.rs");
include!("manager.rs");
include!("overview.rs");
include!("validation.rs");
include!("status.rs");
include!("tests.rs");

#[cfg(test)]
#[path = "../../tests/support/server_manager_lifecycle_tests.rs"]
mod manager_lifecycle_tests;
