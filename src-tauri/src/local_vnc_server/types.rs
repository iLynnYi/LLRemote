use crate::local_agent_service::LocalAgentOverview;
use crate::server::LocalAddress;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturedPixelFormat {
    Rgba8888,
    Bgra8888,
    Argb8888,
    Abgr8888,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfbPixelFormat {
    pub bits_per_pixel: u8,
    pub depth: u8,
    pub big_endian: bool,
    pub true_color: bool,
    pub red_max: u16,
    pub green_max: u16,
    pub blue_max: u16,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalVncServerResolutionMode {
    Fit,
    Native,
    Hd,
    Fhd,
    HdLandscape,
    HdPortrait,
    FhdLandscape,
    FhdPortrait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalVncServerRefreshMode {
    Responsive,
    Balanced,
    LowPower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalVncServerDisplayKind {
    Main,
    Display,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerDisplay {
    pub id: String,
    pub label: String,
    pub kind: LocalVncServerDisplayKind,
    pub is_primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayFrame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalVncServerStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    PermissionMissing,
    PortConflict,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalVncServerConnectionKind {
    Rfb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalVncServerConnectionStatus {
    Online,
    Closing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerActiveConnection {
    pub id: String,
    pub kind: LocalVncServerConnectionKind,
    pub device_name: Option<String>,
    pub platform: Option<String>,
    pub ip_address: Option<String>,
    pub connected_at: String,
    pub status: LocalVncServerConnectionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerStartRequest {
    pub port: u16,
    pub bind_address: String,
    pub password: String,
    pub view_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerViewRequest {
    pub display_id: Option<String>,
    pub resolution_mode: Option<LocalVncServerResolutionMode>,
    pub refresh_mode: Option<LocalVncServerRefreshMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalVncServerPermissionKind {
    ScreenRecording,
    Accessibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalVncServerPermissionStatus {
    Granted,
    Missing,
    Unknown,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerPermission {
    pub kind: LocalVncServerPermissionKind,
    pub status: LocalVncServerPermissionStatus,
    pub required_for: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerPermissionRequestResult {
    pub opened: bool,
    pub kind: LocalVncServerPermissionKind,
    pub platform: String,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalVncServerCaptureStatus {
    #[default]
    Waiting,
    Ready,
    PermissionDenied,
    Unavailable,
    InvalidFrame,
    Unsupported,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerFrameDiagnostics {
    pub capture_status: LocalVncServerCaptureStatus,
    pub capture_frames: u64,
    pub capture_failures: u64,
    pub framebuffer_update_requests: u64,
    pub framebuffer_updates: u64,
    pub last_capture_duration_ms: u64,
    pub last_frame_age_ms: Option<u64>,
    pub last_diff_duration_ms: u64,
    pub last_write_duration_ms: u64,
    pub last_dirty_rects: u64,
    pub last_framebuffer_encoding: String,
    pub last_raw_bytes_written: u64,
    pub last_encoded_bytes_written: u64,
    pub last_bytes_written: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVncServerOverview {
    pub status: LocalVncServerStatus,
    pub bind_address: String,
    pub port: u16,
    pub addresses: Vec<LocalAddress>,
    pub displays: Vec<LocalVncServerDisplay>,
    pub selected_display_id: String,
    pub selected_display_label: String,
    pub resolution_mode: LocalVncServerResolutionMode,
    pub refresh_mode: LocalVncServerRefreshMode,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub security_type: String,
    pub view_only: bool,
    pub active_connections: usize,
    pub active_connection_devices: Vec<LocalVncServerActiveConnection>,
    pub permissions: Vec<LocalVncServerPermission>,
    pub agent: LocalAgentOverview,
    pub frame_diagnostics: LocalVncServerFrameDiagnostics,
    pub started_at: Option<String>,
    pub updated_at: String,
}
