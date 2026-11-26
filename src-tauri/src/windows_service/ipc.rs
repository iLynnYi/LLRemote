use super::{
    native::{self, Handle, SecurityDescriptor},
    WindowsServiceConfigureRequest, WindowsServiceOverview, WindowsServicePasswordRequest,
};
use serde::{Deserialize, Serialize};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    ERROR_NO_DATA, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, GENERIC_READ, GENERIC_WRITE, HANDLE,
};
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    GetNamedPipeServerProcessId, SetNamedPipeHandleState, PIPE_NOWAIT, PIPE_READMODE_MESSAGE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use zeroize::Zeroizing;

const PIPE_NAME: &str = r"\\.\pipe\LLRemoteServer-v1";
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const IPC_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "action",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum Request {
    Status,
    RepairUserAgent,
    DisconnectConnection {
        connection_id: String,
    },
    Configure(WindowsServiceConfigureRequest),
    SetPassword {
        expected_pid: u32,
        request: WindowsServicePasswordRequest,
    },
    StopConfigurationOnly {
        expected_pid: u32,
    },
    WorkerBootstrap {
        session_id: u32,
    },
    WorkerHeartbeat(Box<WorkerDiagnostics>),
    SecureAttention {
        session_id: u32,
    },
    UserAgentBootstrap {
        session_id: u32,
    },
    DiscoverUserAgent {
        session_id: u32,
        server_id: String,
    },
    BrokerClipboardChunk {
        transfer_id: String,
        offset: u32,
        total: u32,
        data: String,
    },
    Reset,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WorkerDiagnostics {
    pub desktop: Option<String>,
    pub desktop_epoch: u64,
    pub input_status: String,
    pub input_failures: u64,
    pub last_error_code: Option<String>,
    pub runtime_error: Option<String>,
    #[serde(default)]
    pub server: Option<crate::local_vnc_server::LocalVncServerOverview>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Response {
    pub overview: Option<WindowsServiceOverview>,
    pub worker_config: Option<WindowsServiceConfigureRequest>,
    pub stop_worker: bool,
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent_config: Option<super::user_agent::UserAgentConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_endpoint: Option<super::types::ServiceAgentEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_view: Option<crate::local_vnc_server::LocalVncServerViewRequest>,
}

impl Response {
    pub fn ack() -> Self {
        Self {
            overview: None,
            worker_config: None,
            stop_worker: false,
            error_code: None,
            server_id: None,
            user_agent_config: None,
            agent_endpoint: None,
            user_view: None,
        }
    }
    pub fn overview(overview: WindowsServiceOverview) -> Self {
        Self {
            overview: Some(overview),
            worker_config: None,
            stop_worker: false,
            error_code: None,
            server_id: None,
            user_agent_config: None,
            agent_endpoint: None,
            user_view: None,
        }
    }
    pub fn error(code: String) -> Self {
        Self {
            overview: None,
            worker_config: None,
            stop_worker: false,
            error_code: Some(code),
            server_id: None,
            user_agent_config: None,
            agent_endpoint: None,
            user_view: None,
        }
    }
    pub fn into_result(self) -> Result<Self, String> {
        if let Some(code) = &self.error_code {
            return Err(code.clone());
        }
        if self
            .overview
            .as_ref()
            .is_some_and(|view| view.ipc_version < super::MIN_SUPPORTED_IPC_VERSION)
        {
            return Err("windows_service_protocol_incompatible".into());
        }
        Ok(self)
    }
}

pub(super) struct Peer {
    pub pid: u32,
    pub session_id: u32,
    pub system: bool,
    pub elevated: bool,
    pub user_sid: String,
}

fn peer_for_pid(pid: u32, token_access: u32) -> Result<Peer, String> {
    let process = Handle::new(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) },
        "windows_service_access_denied",
    )?;
    let token = native::process_token(process.0, token_access)?;
    let mut session_id = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut session_id) } == 0 {
        return Err("windows_service_access_denied".into());
    }
    Ok(Peer {
        pid,
        session_id,
        system: native::token_is_system(token.0),
        elevated: native::token_is_elevated(token.0),
        user_sid: native::token_user_sid(token.0)?,
    })
}

pub(crate) fn create_server(name: &str, sddl: &str) -> Result<Handle, String> {
    let descriptor = SecurityDescriptor::from_sddl(sddl)?;
    let attributes = descriptor.attributes();
    Handle::new(
        unsafe {
            CreateNamedPipeW(
                native::wide(name).as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE
                    | PIPE_READMODE_MESSAGE
                    | PIPE_NOWAIT
                    | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_MESSAGE_BYTES as u32,
                MAX_MESSAGE_BYTES as u32,
                1000,
                &attributes,
            )
        },
        "windows_service_ipc_unavailable",
    )
}

pub(crate) fn connected(pipe: HANDLE) -> Result<bool, String> {
    if unsafe { ConnectNamedPipe(pipe, null_mut()) } != 0 {
        // PIPE_NOWAIT reports success when a disconnected instance begins
        // listening. Only ERROR_PIPE_CONNECTED means a client is present.
        return Ok(false);
    }
    match native::last_error() {
        ERROR_PIPE_CONNECTED => Ok(true),
        ERROR_PIPE_LISTENING => Ok(false),
        ERROR_NO_DATA => {
            // A client may close before the next poll. Recycle that connection
            // so one abandoned request cannot block status and heartbeats.
            unsafe { DisconnectNamedPipe(pipe) };
            Ok(false)
        }
        _ => Err("windows_service_ipc_unavailable".into()),
    }
}

pub(crate) fn open_client(name: &str, timeout: Duration) -> Result<Handle, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let raw = unsafe {
            CreateFileW(
                native::wide(name).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if let Ok(pipe) = Handle::new(raw, "windows_service_ipc_unavailable") {
            let mode = PIPE_READMODE_MESSAGE | PIPE_NOWAIT;
            if unsafe { SetNamedPipeHandleState(pipe.0, &mode, null(), null()) } == 0 {
                return Err("windows_service_ipc_unavailable".into());
            }
            return Ok(pipe);
        }
        if Instant::now() >= deadline {
            return Err("windows_service_ipc_timeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn write_message<T: Serialize>(
    pipe: HANDLE,
    value: &T,
    deadline: Instant,
) -> Result<(), String> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(value).map_err(|_| "windows_service_internal".to_string())?,
    );
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("windows_service_invalid_config".into());
    }
    loop {
        let mut written = 0;
        let result = unsafe {
            WriteFile(
                pipe,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                null_mut(),
            )
        };
        if result != 0 && written == bytes.len() as u32 {
            return Ok(());
        }
        if written != 0 || (result == 0 && !matches!(native::last_error(), 232 | 233)) {
            return Err("windows_service_ipc_unavailable".into());
        }
        if Instant::now() >= deadline {
            return Err("windows_service_ipc_timeout".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn read_message<T: serde::de::DeserializeOwned>(
    pipe: HANDLE,
    deadline: Instant,
) -> Result<T, String> {
    read_message_while(pipe, deadline, || Ok(()))
}

fn read_message_while<T: serde::de::DeserializeOwned>(
    pipe: HANDLE,
    deadline: Instant,
    mut check_helper: impl FnMut() -> Result<(), String>,
) -> Result<T, String> {
    let mut bytes = Zeroizing::new(vec![0_u8; MAX_MESSAGE_BYTES]);
    loop {
        let mut read = 0;
        if unsafe {
            ReadFile(
                pipe,
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                &mut read,
                null_mut(),
            )
        } != 0
            && read > 0
        {
            return serde_json::from_slice(&bytes[..read as usize])
                .map_err(|_| "windows_service_invalid_config".into());
        }
        let error = native::last_error();
        check_helper()?;
        if !matches!(error, 232 | 233 | 0) {
            return Err("windows_service_ipc_unavailable".into());
        }
        if Instant::now() >= deadline {
            return Err("windows_service_ipc_timeout".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn call(request: &Request) -> Result<Response, String> {
    let deadline = Instant::now() + IPC_TIMEOUT;
    let pipe = open_client(PIPE_NAME, IPC_TIMEOUT)?;
    let mut server_pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut server_pid) } == 0 {
        return Err("windows_service_access_denied".into());
    }
    // A forged pipe cannot harvest passwords: match both SCM's live process and
    // the service's PID before any request is written. Ordinary users need no
    // access to a privileged process token merely to query its status.
    if super::install::service_process_id()? != Some(server_pid) {
        return Err("windows_service_access_denied".into());
    }
    write_message(pipe.0, request, deadline)?;
    let response = read_message(pipe.0, deadline)?;
    write_message(pipe.0, &true, deadline)?;
    Ok(response)
}

pub(super) struct Server {
    pipe: Handle,
}
impl Server {
    pub fn new() -> Result<Self, String> {
        // Authenticated local users can query and disconnect devices. Configuration
        // verbs retain their authorization checks; worker verbs match PID/session.
        Ok(Self {
            pipe: create_server(PIPE_NAME, "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)")?,
        })
    }
    pub fn poll(&mut self, handler: impl FnOnce(Request, Peer) -> Response) -> Result<(), String> {
        let pipe = &self.pipe;
        if !connected(pipe.0)? {
            return Ok(());
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        let response = (|| {
            let mut pid = 0;
            if unsafe { GetNamedPipeClientProcessId(pipe.0, &mut pid) } == 0 {
                return Err("windows_service_access_denied".into());
            }
            let peer = peer_for_pid(pid, TOKEN_QUERY)?;
            let request = read_message(pipe.0, deadline)?;
            Ok(handler(request, peer))
        })()
        .unwrap_or_else(Response::error);
        if write_message(pipe.0, &response, deadline).is_ok() {
            // A bounded client ACK replaces an unbounded FlushFileBuffers call.
            let _ = read_message::<bool>(pipe.0, deadline);
        }
        unsafe {
            DisconnectNamedPipe(pipe.0);
        }
        Ok(())
    }
}

pub(super) fn relay_name(id: &str) -> Result<String, String> {
    if id.len() != 36 || uuid::Uuid::parse_str(id).is_err() {
        return Err("windows_service_access_denied".into());
    }
    Ok(format!(r"\\.\pipe\LLRemoteConfigure-{id}"))
}

pub(super) fn receive_relay(
    id: &str,
    expected_server_pid: u32,
) -> Result<WindowsServiceConfigureRequest, String> {
    let pipe = open_client(&relay_name(id)?, Duration::from_secs(10))?;
    let mut server_pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut server_pid) } == 0
        || server_pid != expected_server_pid
    {
        return Err("windows_service_access_denied".into());
    }
    let deadline = Instant::now() + IPC_TIMEOUT;
    let request = read_message(pipe.0, deadline)?;
    write_message(pipe.0, &true, deadline)?;
    Ok(request)
}

fn wait_for_helper_pipe(
    pipe: HANDLE,
    mut check_helper: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !connected(pipe)? {
        check_helper()?;
        if Instant::now() >= deadline {
            return Err("windows_service_ipc_timeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

pub(super) fn relay_to_helper(
    pipe: HANDLE,
    expected_pid: u32,
    request: &WindowsServiceConfigureRequest,
    mut check_helper: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    wait_for_helper_pipe(pipe, &mut check_helper)?;
    let mut pid = 0;
    if unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) } == 0
        || pid != expected_pid
        || !peer_for_pid(pid, TOKEN_QUERY)?.elevated
    {
        return Err("windows_service_access_denied".into());
    }
    let deadline = Instant::now() + IPC_TIMEOUT;
    write_message(pipe, request, deadline)?;
    if !read_message_while::<bool>(pipe, deadline, check_helper)? {
        return Err("windows_service_access_denied".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_pipe_tests.rs"]
mod tests;

pub(super) fn receive_repair_report(
    pipe: HANDLE,
    expected_pid: u32,
    mut check_helper: impl FnMut() -> Result<(), String>,
) -> Result<super::WindowsServiceRepairReport, String> {
    wait_for_helper_pipe(pipe, &mut check_helper)?;
    let mut pid = 0;
    if unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) } == 0
        || pid != expected_pid
        || !peer_for_pid(pid, TOKEN_QUERY)?.elevated
    {
        return Err("windows_service_access_denied".into());
    }
    write_message(pipe, &true, Instant::now() + IPC_TIMEOUT)?;
    let result: Result<super::WindowsServiceRepairReport, String> =
        read_message_while(pipe, Instant::now() + Duration::from_secs(60), check_helper)?;
    write_message(pipe, &true, Instant::now() + IPC_TIMEOUT)?;
    result
}

pub(super) fn send_repair_report(
    id: &str,
    expected_server_pid: u32,
    repair: impl FnOnce() -> Result<super::WindowsServiceRepairReport, String>,
) -> Result<(), String> {
    let pipe = open_client(&relay_name(id)?, Duration::from_secs(10))?;
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut pid) } == 0 || pid != expected_server_pid {
        return Err("windows_service_access_denied".into());
    }
    if !read_message::<bool>(pipe.0, Instant::now() + IPC_TIMEOUT)? {
        return Err("windows_service_access_denied".into());
    }
    // Mutations begin only after the App and elevated helper authenticate one
    // another. Reports contain state/categories, never passwords or tickets.
    let report = repair();
    write_message(pipe.0, &report, Instant::now() + IPC_TIMEOUT)?;
    let _ = read_message::<bool>(pipe.0, Instant::now() + IPC_TIMEOUT)?;
    Ok(())
}
