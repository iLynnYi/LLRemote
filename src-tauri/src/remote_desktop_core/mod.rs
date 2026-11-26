//! Runtime primitives for the replacement remote-desktop pipeline.
//!
//! This module intentionally has no Tauri, socket, or platform-capture dependency.
//! A capture adapter publishes canonical frames into `ServerFramebufferCore`; an RFB
//! adapter consumes client updates and feeds decoded rectangles into `ClientFramebufferCore`.

mod client;
mod client_framebuffer_hub;
mod fence;
mod framebuffer;
mod memory;
mod runtime_status;
pub use runtime_status::{
    DesktopAccessState, SecureAttentionResult, ServerRuntimeMessage, RUNTIME_STATUS_PSEUDO_ENCODING,
};
mod rfb_authentication;
mod rfb_client;
mod rfb_client_handshake;
mod rfb_client_session;
pub(crate) mod rfb_fence;
mod rfb_framebuffer;
mod rfb_hextile;
mod rfb_limits;
mod rfb_metrics;
mod rfb_pixel;
mod rfb_receive;
mod rfb_tight;
mod server;
mod server_history;
mod server_signal;
pub(crate) use framebuffer::coalesce_dirty_rectangles;
mod surface;

pub use client::{
    ClientFramebufferCore, ClientFramebufferSnapshot, DecodedFramebufferRect,
    DecodedFramebufferUpdate, SurfaceFrameUpdate, SurfaceId, SurfaceSnapshot,
};
pub use client_framebuffer_hub::ClientFramebufferHub;
pub use fence::{
    assemble_discovery_frames, decode_discovery_frame, encode_discovery_frames, DiscoveryFrame,
    DiscoveryFrameType,
};
pub use framebuffer::{
    diff_dirty_rectangles, FrameId, Framebuffer, FramebufferGeometry, FramebufferRect, PixelFormat,
};
pub(crate) use memory::{ClientMemoryBudget, MemoryReservation};
pub(crate) use rfb_authentication::RfbAuthentication;
pub use rfb_client::{
    ClientRfbPixelFormat, RfbClientFramebufferDecoder, RfbCursorUpdate, RfbDesktopScreen,
    RfbFramebufferUpdateOutcome,
};
pub use rfb_client_handshake::{RfbClientHandshake, RfbClientHandshakeEvent};
pub use rfb_client_session::{
    run_rfb_client_tcp_loop, run_rfb_client_tcp_loop_until, run_rfb_client_tcp_loop_with_commands,
    run_rfb_client_tcp_loop_with_commands_and_ready,
    run_rfb_client_tcp_loop_with_commands_ready_and_events, RfbClientCommand, RfbClientSessionCore,
    RfbClientSessionEvent,
};
pub(crate) use rfb_client_session::{
    run_rfb_client_tcp_loop_with_command_source, RfbCommandSource,
};
pub use rfb_metrics::{RfbEncodingCounts, RfbPerformanceMetrics};
pub(crate) use rfb_pixel::cpixel_byte_range;
pub use server::{ClientFrameUpdate, FrameInterest, ServerFramebufferCore};
pub(crate) use server_signal::ServerUpdateSignal;
pub use surface::SurfaceFramePacket;
