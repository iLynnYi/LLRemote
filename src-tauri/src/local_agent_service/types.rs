use crate::local_vnc_server::SharedDisplay;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentStatus {
    Stopped,
    Running,
    Failed,
}

impl LocalAgentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Running => "running",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAgentOverview {
    pub status: LocalAgentStatus,
    pub bind_address: String,
    pub port: Option<u16>,
    pub capabilities: Vec<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug)]
pub(crate) struct LocalAgentRuntime {
    pub(super) bind_address: String,
    pub(crate) port: u16,
    pub(super) stop_requested: Arc<AtomicBool>,
    pub(super) worker: Option<JoinHandle<()>>,
    pub(super) ticket_store: AgentTicketStore,
    pub(super) context: AgentRuntimeContext,
}

pub(crate) type AgentTicketStore = Arc<Mutex<HashMap<String, Instant>>>;

#[derive(Debug, Clone)]
pub(super) struct AgentTransferState {
    pub(super) file_name: String,
    pub(super) relative_path: Option<String>,
    pub(super) paused: bool,
    pub(super) committing: bool,
    pub(super) part_path: PathBuf,
    pub(super) final_path: PathBuf,
    pub(super) rename_policy: String,
    pub(super) offset: u64,
    pub(super) expected_size: u64,
}

pub(super) type AgentTransferMap = Arc<Mutex<HashMap<String, AgentTransferState>>>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFileUploadRequest {
    pub host: String,
    pub port: u16,
    pub session_id: String,
    pub file_path: String,
    pub relative_path: Option<String>,
    pub transfer_id: Option<String>,
    pub target_directory: Option<String>,
    pub rename_policy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFileUploadResult {
    pub file_name: String,
    pub remote_path: String,
    pub bytes_written: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFileUploadProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub phase: AgentFileUploadPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentFileUploadPhase {
    Preparing,
    Transferring,
    Verifying,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentResolvedDisplay {
    pub id: String,
    pub x: i32,
    pub width: u32,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissionDiagnosticItem {
    pub kind: String,
    pub status: String,
    pub required_for: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDisplayInventoryItem {
    pub id: String,
    pub label: String,
    pub is_primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub scale_factor: f64,
}

#[derive(Debug, Clone)]
pub(super) struct AgentRuntimeContext {
    pub(super) shared_display: Arc<RwLock<SharedDisplay>>,
    pub(super) view_only: bool,
    pub(super) interactive: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSecureInputRecoveryResult {
    pub target: String,
    pub checked_at: String,
    pub status: String,
    pub action: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRuntimeAgentDescription {
    pub server_id: String,
    pub view_only: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAgentFileUploadRequest {
    pub host: String,
    pub port: u16,
    pub session_id: String,
    pub file_path: String,
    pub relative_path: Option<String>,
    pub transfer_id: Option<String>,
    pub target_directory: Option<String>,
    pub rename_policy: Option<String>,
}
