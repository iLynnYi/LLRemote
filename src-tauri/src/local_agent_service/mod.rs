use crate::local_vnc_server::{
    control_remote_desktop, get_clipboard_text_for_vnc, get_local_vnc_server_permissions,
    list_local_vnc_server_displays, set_clipboard_text_from_vnc, wake_remote_display, DisplayFrame,
    DisplaySize, LocalVncServerDisplay, LocalVncServerPermissionKind, LocalVncServerRefreshMode,
    LocalVncServerResolutionMode, SharedDisplay,
};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const CLIPBOARD_TEXT_WRITE: &str = "clipboard.text.write";
const CLIPBOARD_TEXT_READ: &str = "clipboard.text.read";
const MESSAGE_SHOW: &str = "message.show";
const FILE_TRANSFER_CREATE: &str = "file.transfer.create";
const FILE_TRANSFER_CHUNK: &str = "file.transfer.chunk";
const FILE_TRANSFER_COMMIT: &str = "file.transfer.commit";
const FILE_TRANSFER_CANCEL: &str = "file.transfer.cancel";
const FILE_TRANSFER_PAUSE: &str = "file.transfer.pause";
const FILE_TRANSFER_STATUS: &str = "file.transfer.status";
const FILE_TRANSFER_RESUME: &str = "file.transfer.resume";
const FILE_TRANSFER_UPLOAD: &str = "file.transfer.upload";
const FILE_TRANSFER_RELATIVE_PATH: &str = "file.transfer.relative-path";
const FILE_TRANSFER_CANCEL_CAPABILITY: &str = "file.transfer.cancel";
const FILE_TRANSFER_SHA256: &str = "file.transfer.checksum.sha256";
const FILE_OPEN: &str = "file.open";
const FILE_REVEAL: &str = "file.reveal";
const AGENT_PERMISSIONS_DIAGNOSE: &str = "agent.permissions.diagnose";
const AGENT_DISPLAY_ENUMERATE: &str = "agent.display.enumerate";
const AGENT_DISPLAY_SELECT: &str = "agent.display.select";
const DISPLAY_WAKE: &str = "display.wake";
const SESSION_END: &str = "remote.session.end";
const REMOTE_DESKTOP_CONTROL: &str = "remote.desktop.control";
const AGENT_SECURE_INPUT_RECOVER: &str = "agent.secure_input.recover";
const DEFAULT_FILE_CHUNK_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_AGENT_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const AGENT_REQUEST_TIMEOUT_SECONDS: u64 = 5;
const AGENT_FILE_COMMIT_MIN_TIMEOUT_SECONDS: u64 = 30;
const AGENT_FILE_COMMIT_MAX_TIMEOUT_SECONDS: u64 = 300;
const AGENT_FILE_COMMIT_BYTES_PER_TIMEOUT_SECOND: u64 = 8 * 1024 * 1024;
const AGENT_SESSION_TTL: Duration = Duration::from_secs(60);
const AGENT_TICKET_TTL: Duration = Duration::from_secs(30);

mod types;

pub use types::*;

include!("protocol.rs");
include!("request_io.rs");
include!("request_limit.rs");
include!("capabilities.rs");
include!("transfers.rs");
include!("paths.rs");
include!("display.rs");

#[cfg(test)]
#[path = "../../tests/support/server_agent_lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "../../tests/support/server_agent_policy_tests.rs"]
mod policy_tests;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRequest {
    #[serde(rename = "type")]
    message_type: String,
    bootstrap_key: Option<String>,
    session_id: Option<String>,
    text: Option<String>,
    transfer_id: Option<String>,
    file_name: Option<String>,
    relative_path: Option<String>,
    size: Option<u64>,
    offset: Option<u64>,
    data_base64: Option<String>,
    sha256: Option<String>,
    target_directory: Option<String>,
    rename_policy: Option<String>,
    path: Option<String>,
    reason: Option<String>,
    display_id: Option<String>,
    action_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentResponse {
    ok: bool,
    error: Option<String>,
    transfer_id: Option<String>,
    chunk_size: Option<usize>,
    offset: Option<u64>,
    file_name: Option<String>,
    remote_path: Option<String>,
    bytes_written: Option<u64>,
    sha256: Option<String>,
    permissions: Option<Vec<AgentPermissionDiagnosticItem>>,
    displays: Option<Vec<AgentDisplayInventoryItem>>,
    secure_input: Option<AgentSecureInputRecoveryResult>,
    session_id: Option<String>,
    description: Option<LocalRuntimeAgentDescription>,
    text: Option<String>,
    revision: Option<String>,
}

pub(crate) fn start_local_agent(
    bind_address: &str,
    preferred_port: u16,
    server_id: String,
    shared_display: Arc<RwLock<SharedDisplay>>,
    view_only: bool,
    interactive: Option<Arc<AtomicBool>>,
) -> Result<LocalAgentRuntime, String> {
    let listener = TcpListener::bind((bind_address, preferred_port)).map_err(|error| {
        format!(
            "agent_listener_port_conflict: unable to bind local Agent on {}:{}: {}",
            bind_address, preferred_port, error
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("agent_listener_setup_failed: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("agent_listener_addr_failed: {error}"))?
        .port();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let ticket_store: AgentTicketStore = Arc::new(Mutex::new(HashMap::new()));
    let worker_stop = Arc::clone(&stop_requested);
    let worker_bootstrap_key = Uuid::new_v4().to_string();
    let worker_server_id = server_id;
    let worker_ticket_store = Arc::clone(&ticket_store);
    let transfers = Arc::new(Mutex::new(HashMap::new()));
    let worker_transfers = Arc::clone(&transfers);
    let inbox_dir = agent_inbox_dir()?;
    let worker_inbox = inbox_dir.clone();
    let context = AgentRuntimeContext {
        shared_display,
        view_only,
        interactive,
    };
    let worker_context = context.clone();
    let worker = thread::spawn(move || {
        run_agent_listener(
            listener,
            worker_stop,
            AgentBootstrap {
                key: worker_bootstrap_key,
                tickets: worker_ticket_store,
            },
            worker_server_id,
            worker_transfers,
            worker_inbox,
            worker_context,
        )
    });

    Ok(LocalAgentRuntime {
        bind_address: bind_address.to_string(),
        port,
        stop_requested,
        worker: Some(worker),
        ticket_store,
        context,
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn user_agent_display() -> Arc<RwLock<SharedDisplay>> {
    Arc::new(RwLock::new(
        shared_display_for_agent_request_from_inventory(
            list_local_vnc_server_displays(),
            None,
            None,
        ),
    ))
}

pub(crate) fn issue_agent_ticket(store: &AgentTicketStore) -> Result<(String, u64), String> {
    let ticket = Uuid::new_v4().to_string();
    let expires_at = Instant::now() + AGENT_TICKET_TTL;
    store
        .lock()
        .map_err(|_| "agent_ticket_state_poisoned".to_string())?
        .insert(ticket.clone(), expires_at);
    let expires_unix = SystemTime::now()
        .checked_add(AGENT_TICKET_TTL)
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or_default();
    Ok((ticket, expires_unix))
}

pub(crate) fn consume_agent_ticket(store: &AgentTicketStore, ticket: &str) -> bool {
    let Ok(mut tickets) = store.lock() else {
        return false;
    };
    let now = Instant::now();
    tickets.retain(|_, expires_at| *expires_at > now);
    tickets.remove(ticket).is_some()
}

pub(crate) fn stopped_agent_overview() -> LocalAgentOverview {
    LocalAgentOverview {
        status: LocalAgentStatus::Stopped,
        bind_address: "0.0.0.0".to_string(),
        port: None,
        capabilities: vec![],
        last_error: None,
        updated_at: now_unix_seconds(),
    }
}

impl LocalAgentRuntime {
    pub(crate) fn ticket_store(&self) -> AgentTicketStore {
        Arc::clone(&self.ticket_store)
    }
    pub(crate) fn overview(&self) -> LocalAgentOverview {
        LocalAgentOverview {
            status: LocalAgentStatus::Running,
            bind_address: self.bind_address.clone(),
            port: Some(self.port),
            capabilities: agent_capabilities_for_context(&self.context),
            last_error: None,
            updated_at: now_unix_seconds(),
        }
    }

    pub(crate) fn stop(&mut self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for LocalAgentRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn failed_agent_overview(bind_address: String, error: String) -> LocalAgentOverview {
    LocalAgentOverview {
        status: LocalAgentStatus::Failed,
        bind_address,
        port: None,
        capabilities: vec![],
        last_error: Some(error),
        updated_at: now_unix_seconds(),
    }
}

fn now_unix_seconds() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
#[path = "../../tests/support/agent_client_tests.rs"]
mod client_tests;

#[cfg(test)]
#[path = "../../tests/support/agent_file_commit_tests.rs"]
mod file_commit_tests;

#[cfg(test)]
#[path = "../../tests/support/agent_response_tests.rs"]
mod response_tests;
