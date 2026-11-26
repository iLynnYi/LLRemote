//! The service supervises this process, but all network Agent actions execute
//! with the logged-on user's non-elevated token and profile.
use super::{
    clipboard_transfer::{append_clipboard_chunk, ClipboardTransfer},
    install, ipc, native,
    types::ServiceAgentEndpoint,
};
use crate::local_vnc_server::LocalVncServerViewRequest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::System::Pipes::{
    DisconnectNamedPipe, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UserAgentConfig {
    pub bind_address: String,
    pub server_id: String,
    pub view_only: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UserAgentStatus {
    pub port: u16,
    pub view: Option<LocalVncServerViewRequest>,
    pub capabilities: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    Status {
        interactive: bool,
    },
    Discover,
    SessionEnded,
    ClipboardChunk {
        transfer_id: String,
        offset: u32,
        total: u32,
        data: String,
    },
    Stop,
}

// No Debug: Discover replies carry a one-use authentication ticket.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Reply {
    pub status: Option<UserAgentStatus>,
    pub endpoint: Option<ServiceAgentEndpoint>,
    pub error_code: Option<String>,
}

fn pipe_name(session_id: u32, pid: u32) -> String {
    format!(r"\\.\pipe\LLRemoteUserAgent-{session_id}-{pid}")
}

pub(super) fn call(pid: u32, session_id: u32, command: &Command) -> Result<Reply, String> {
    let timeout = Duration::from_millis(300);
    let deadline = Instant::now() + timeout;
    let pipe = ipc::open_client(&pipe_name(session_id, pid), timeout)?;
    let mut actual_pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut actual_pid) } == 0 || actual_pid != pid {
        return Err("windows_service_access_denied".into());
    }
    ipc::write_message(pipe.0, command, deadline)?;
    let reply: Reply = ipc::read_message(pipe.0, deadline)?;
    ipc::write_message(pipe.0, &true, deadline)?;
    match &reply.error_code {
        Some(code) => Err(code.clone()),
        None => Ok(reply),
    }
}

pub(super) fn run(session_id: u32) -> Result<(), String> {
    let token = native::process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY)?;
    super::user_token::validate(token.0, session_id)?;
    if native::current_session()? != session_id || std::env::var_os("USERPROFILE").is_none() {
        return Err("windows_service_user_agent_identity_failed".into());
    }
    native::initialize_worker_dpi_awareness()?;
    let mut bootstrap =
        ipc::call(&ipc::Request::UserAgentBootstrap { session_id })?.into_result()?;
    let config = bootstrap
        .user_agent_config
        .take()
        .ok_or_else(|| "windows_service_user_agent_unavailable".to_string())?;
    let interactive = Arc::new(AtomicBool::new(false));
    let shared_display = crate::local_agent_service::user_agent_display();
    let mut agent = None;
    for port in 6200..=u16::MAX {
        match crate::local_agent_service::start_local_agent(
            &config.bind_address,
            port,
            config.server_id.clone(),
            Arc::clone(&shared_display),
            config.view_only,
            Some(Arc::clone(&interactive)),
        ) {
            Ok(runtime) => {
                agent = Some(runtime);
                break;
            }
            Err(code) if code.starts_with("agent_listener_port_conflict:") => continue,
            Err(_) => return Err("windows_service_user_agent_unavailable".into()),
        }
    }
    let mut agent = agent.ok_or_else(|| "windows_service_user_agent_unavailable".to_string())?;
    let pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
    let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{})", native::current_user_sid()?);
    let pipe = ipc::create_server(&pipe_name(session_id, pid), &sddl)?;
    let mut clipboard_transfers: HashMap<String, ClipboardTransfer> = HashMap::new();
    let mut stopping = false;
    while !stopping {
        if !ipc::connected(pipe.0)? {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        let deadline = Instant::now() + Duration::from_millis(300);
        let result = (|| {
            let mut caller = 0;
            if unsafe { GetNamedPipeClientProcessId(pipe.0, &mut caller) } == 0
                || install::service_process_id()? != Some(caller)
            {
                return Err("windows_service_access_denied".to_string());
            }
            let command: Command = ipc::read_message(pipe.0, deadline)?;
            let mut reply = Reply {
                status: None,
                endpoint: None,
                error_code: None,
            };
            match command {
                Command::Status {
                    interactive: active,
                } => {
                    interactive.store(active, Ordering::SeqCst);
                    let display = shared_display
                        .read()
                        .map_err(|_| "windows_service_user_agent_unavailable".to_string())?;
                    reply.status = Some(UserAgentStatus {
                        port: agent.port,
                        capabilities: agent.overview().capabilities,
                        view: (display.revision > 0).then(|| LocalVncServerViewRequest {
                            display_id: Some(display.id.clone()),
                            resolution_mode: Some(display.resolution_mode),
                            refresh_mode: Some(display.refresh_mode),
                        }),
                    });
                }
                Command::Discover => {
                    let (bootstrap_key, expires_at) =
                        crate::local_agent_service::issue_agent_ticket(&agent.ticket_store())?;
                    reply.endpoint = Some(ServiceAgentEndpoint {
                        port: agent.port,
                        bootstrap_key,
                        expires_at,
                    });
                }
                Command::SessionEnded => {
                    crate::background_share::disconnect_lock::session_ended()?;
                }
                Command::ClipboardChunk {
                    transfer_id,
                    offset,
                    total,
                    data,
                } => {
                    if config.view_only || !interactive.load(Ordering::SeqCst) {
                        return Err("agent_user_desktop_unavailable".into());
                    }
                    if let Some(text) = append_clipboard_chunk(
                        &mut clipboard_transfers,
                        transfer_id,
                        offset,
                        total,
                        &data,
                    )? {
                        crate::local_vnc_server::set_clipboard_text_from_vnc(&text)?;
                    }
                }
                Command::Stop => stopping = true,
            }
            Ok(reply)
        })();
        let reply = result.unwrap_or_else(|code| Reply {
            status: None,
            endpoint: None,
            error_code: Some(code),
        });
        if ipc::write_message(pipe.0, &reply, deadline).is_ok() {
            let _ = ipc::read_message::<bool>(pipe.0, deadline);
        }
        unsafe { DisconnectNamedPipe(pipe.0) };
    }
    agent.stop();
    Ok(())
}
