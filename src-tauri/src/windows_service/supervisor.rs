use super::{
    config::{self, ServiceConfig},
    credentials,
    ipc::{self, Peer, Request, Response, WorkerDiagnostics},
    native::{self, Handle},
    sas, user_agent, WindowsServiceOverview,
};
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, DuplicateTokenEx, LookupPrivilegeValueW, SecurityImpersonation,
    SetTokenInformation, TokenPrimary, TokenSessionId, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken};
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::System::Threading::{
    CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, ResumeThread, WaitForSingleObject,
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION,
    STARTUPINFOW,
};

static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
static SERVICE_HANDLE: AtomicUsize = AtomicUsize::new(0);
static CONFIGURE_ONLY: AtomicBool = AtomicBool::new(false);

struct WorkerProcess {
    process: Handle,
    job: Handle,
    pid: u32,
    session_id: u32,
    diagnostics: WorkerDiagnostics,
    ready: bool,
    last_heartbeat: Instant,
    server_id: String,
    desktop_password: zeroize::Zeroizing<String>,
}

impl WorkerProcess {
    fn alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.process.0, 0) != WAIT_OBJECT_0 }
    }
    fn stop(self) {
        // The IPC heartbeat tells a responsive worker to stop first. Native calls
        // that cannot return are contained by the exclusively owned Job.
        if unsafe { WaitForSingleObject(self.process.0, 1500) } != WAIT_OBJECT_0 {
            unsafe {
                TerminateJobObject(self.job.0, 0);
                WaitForSingleObject(self.process.0, 1500);
            }
        }
    }
}

struct Supervisor {
    config: Option<ServiceConfig>,
    worker: Option<WorkerProcess>,
    last_error: Option<String>,
    sas_error_code: Option<String>,
    sas_request_count: u64,
    restart_after: Instant,
    reconfigure: bool,
    user_agent: Option<WorkerProcess>,
    user_agent_port: Option<u16>,
    user_agent_view: Option<crate::local_vnc_server::LocalVncServerViewRequest>,
    user_agent_error: Option<String>,
    agent_restart_after: Instant,
    agent_check_after: Instant,
    user_agent_revision: u64,
    agent_state_key: u64,
    user_agent_capabilities: Vec<String>,
    broker: crate::local_vnc_server::LocalVncServerManager,
    relay: std::sync::Arc<crate::local_vnc_server::desktop_relay::DesktopRelay>,
    server_id: String,
    relay_worker_pid: Option<u32>,
    relay_retry_after: Instant,
    broker_retry_after: Instant,
    broker_view: Option<crate::local_vnc_server::LocalVncServerViewRequest>,
    broker_error: Option<String>,
    relay_error: Option<String>,
    startup_attempt: Option<crate::startup_attempt::StartupAttempt>,
}

impl Supervisor {
    fn new() -> Self {
        let (config, last_error) = match config::read() {
            Ok(config) => (config, None),
            Err(code) => (None, Some(code)),
        };
        Self {
            config,
            last_error,
            worker: None,
            sas_error_code: None,
            sas_request_count: 0,
            restart_after: Instant::now(),
            reconfigure: false,
            user_agent: None,
            user_agent_port: None,
            user_agent_view: None,
            user_agent_error: None,
            agent_restart_after: Instant::now(),
            agent_check_after: Instant::now(),
            user_agent_revision: 0,
            agent_state_key: 0,
            user_agent_capabilities: vec![],
            broker: Default::default(),
            relay: Default::default(),
            server_id: uuid::Uuid::new_v4().to_string(),
            relay_worker_pid: None,
            relay_retry_after: Instant::now(),
            broker_retry_after: Instant::now(),
            broker_view: None,
            broker_error: None,
            relay_error: None,
            startup_attempt: None,
        }
    }

    fn overview(&self) -> WindowsServiceOverview {
        let worker = self.worker.as_ref().filter(|worker| worker.alive());
        let mut overview = WindowsServiceOverview {
            supported: true,
            installed: true,
            ipc_version: super::IPC_VERSION,
            auto_start: super::install::service_auto_start(),
            last_service_attempt: super::native::data_directory()
                .ok()
                .and_then(|directory| crate::startup_attempt::read(&directory)),
            status: "running".into(),
            worker_session_id: worker.map(|worker| worker.session_id),
            worker_running: worker.is_some_and(|worker| worker.ready),
            desktop_recovering: self.broker.is_running() && !self.relay.ready(),
            bind_address: None,
            port: None,
            view_only: false,
            credential_configured: credentials::read().is_ok(),
            password_management_authorized: false,
            password_management_supported: true,
            last_error_code: self
                .broker_error
                .clone()
                .or_else(|| self.relay_error.clone())
                .or_else(|| {
                    self.last_error.clone().or_else(|| {
                        worker.and_then(|worker| worker.diagnostics.last_error_code.clone())
                    })
                }),
            sas_policy: sas::policy(),
            sas_error_code: self.sas_error_code.clone(),
            sas_request_count: self.sas_request_count,
            desktop: worker.and_then(|worker| worker.diagnostics.desktop.clone()),
            input_status: worker.map(|worker| worker.diagnostics.input_status.clone()),
            input_failures: worker
                .map(|worker| worker.diagnostics.input_failures)
                .unwrap_or(0),
            user_agent_running: self
                .user_agent
                .as_ref()
                .is_some_and(|agent| agent.alive() && agent.ready),
            user_agent_port: self.user_agent_port,
            user_agent_error_code: self.user_agent_error.clone(),
            user_agent_repair_supported: true,
            user_agent_revision: self.user_agent_revision,
            server: self.broker.status().ok().filter(|server| {
                server.status == crate::local_vnc_server::LocalVncServerStatus::Running
            }),
        };
        if let Some(config) = &self.config {
            config.apply(&mut overview);
        }
        if let Some(server) = overview.server.as_mut() {
            if overview.user_agent_running {
                server.agent = crate::local_agent_service::LocalAgentOverview {
                    status: crate::local_agent_service::LocalAgentStatus::Running,
                    bind_address: server.bind_address.clone(),
                    port: self.user_agent_port,
                    capabilities: self.user_agent_capabilities.clone(),
                    last_error: None,
                    updated_at: server.updated_at.clone(),
                };
            }
        }
        overview
    }

    fn valid_worker(&self, peer: &Peer, requested_session: Option<u32>) -> bool {
        peer.system
            && self.worker.as_ref().is_some_and(|worker| {
                worker.alive()
                    && worker.pid == peer.pid
                    && worker.session_id == peer.session_id
                    && requested_session.is_none_or(|session| session == worker.session_id)
            })
    }

    fn overview_for_peer(&self, peer: &Peer) -> WindowsServiceOverview {
        let mut overview = self.overview();
        match super::password_management::authorized(peer.elevated, &peer.user_sid) {
            Ok(authorized) => overview.password_management_authorized = authorized,
            Err(code) => overview.last_error_code = Some(code),
        }
        overview
    }

    fn valid_user_agent(&self, peer: &Peer, session_id: u32) -> bool {
        !peer.system
            && !peer.elevated
            && self.user_agent.as_ref().is_some_and(|agent| {
                agent.alive()
                    && agent.pid == peer.pid
                    && agent.session_id == peer.session_id
                    && agent.session_id == session_id
            })
    }

    fn handle(&mut self, request: Request, peer: Peer) -> Response {
        let result = (|| match request {
            Request::Status => Ok(Response::overview(self.overview_for_peer(&peer))),
            Request::RepairUserAgent => {
                if !peer.elevated {
                    return Err("windows_service_admin_required".into());
                }
                if !self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.alive() && worker.ready)
                {
                    return Err("windows_service_user_agent_unavailable".into());
                }
                // Rebuild only the optional user channel; public RFB sockets
                // and the SYSTEM capture/input worker continue running.
                self.stop_user_agent();
                self.agent_restart_after = Instant::now();
                self.agent_check_after = Instant::now();
                Ok(Response::overview(self.overview()))
            }
            Request::DisconnectConnection { connection_id } => {
                // Local pipe access is sufficient for disconnecting a device.
                // This operation does not manage passwords or service settings.
                self.broker.disconnect_connection(connection_id)?;
                if let Some(agent) = self
                    .user_agent
                    .as_ref()
                    .filter(|agent| agent.alive() && agent.ready)
                {
                    user_agent::call(
                        agent.pid,
                        agent.session_id,
                        &user_agent::Command::SessionEnded,
                    )
                    .map_err(|_| "windows_service_screen_lock_failed".to_string())?;
                }
                Ok(Response::overview(self.overview()))
            }
            Request::Configure(request) => {
                if !peer.elevated {
                    return Err("windows_service_admin_required".into());
                }
                request.validate()?;
                let previous_password = credentials::read().ok();
                if request.password.is_empty() && previous_password.is_none() {
                    return Err("windows_service_credential_missing".into());
                }
                if !request.password.is_empty() {
                    credentials::write(&request.password)?;
                }
                let next = ServiceConfig::from_request(&request);
                if let Err(code) = config::write(&next) {
                    if !request.password.is_empty() {
                        if let Some(previous) = previous_password {
                            let _ = credentials::write(&previous);
                        } else {
                            let _ = credentials::remove();
                        }
                    }
                    return Err(code);
                }
                self.config = Some(next);
                self.reconfigure = true;
                self.last_error = None;
                Ok(Response::overview(self.overview()))
            }
            Request::SetPassword {
                expected_pid,
                request,
            } => {
                if !super::password_management::authorized(peer.elevated, &peer.user_sid)? {
                    return Err("windows_service_password_management_authorization_required".into());
                }
                super::password_management::validate_change_context(
                    CONFIGURE_ONLY.load(Ordering::SeqCst),
                    self.config.is_some(),
                )?;
                super::password_management::validate_configuration_instance(
                    expected_pid,
                    std::process::id(),
                    CONFIGURE_ONLY.load(Ordering::SeqCst),
                )
                .map_err(|_| "windows_service_invalid_config".to_string())?;
                request.validate()?;
                credentials::write(&request.password)?;
                self.last_error = None;
                Ok(Response::overview(self.overview_for_peer(&peer)))
            }
            Request::StopConfigurationOnly { expected_pid } => {
                if !super::password_management::authorized(peer.elevated, &peer.user_sid)? {
                    return Err("windows_service_password_management_authorization_required".into());
                }
                super::password_management::validate_configuration_instance(
                    expected_pid,
                    std::process::id(),
                    CONFIGURE_ONLY.load(Ordering::SeqCst),
                )?;
                STOP_REQUESTED.store(true, Ordering::SeqCst);
                report_state(SERVICE_STOP_PENDING, 0);
                Ok(Response::overview(self.overview_for_peer(&peer)))
            }
            Request::WorkerBootstrap { session_id } => {
                if !self.valid_worker(&peer, Some(session_id)) {
                    return Err("windows_service_access_denied".into());
                }
                let config = self
                    .config
                    .as_ref()
                    .ok_or_else(|| "windows_service_credential_missing".to_string())?;
                let password = self
                    .worker
                    .as_ref()
                    .ok_or("windows_service_access_denied")?
                    .desktop_password
                    .as_str();
                let mut response = Response::overview(self.overview());
                response.server_id = self.worker.as_ref().map(|worker| worker.server_id.clone());
                response.worker_config = Some(super::WindowsServiceConfigureRequest {
                    bind_address: config.bind_address.clone(),
                    port: config.port,
                    view_only: config.view_only,
                    password: password.to_string(),
                });
                Ok(response)
            }
            Request::WorkerHeartbeat(diagnostics) => {
                if !self.valid_worker(&peer, None) {
                    return Err("windows_service_access_denied".into());
                }
                let worker = self
                    .worker
                    .as_mut()
                    .ok_or_else(|| "windows_service_access_denied".to_string())?;
                if let Some(code) = &diagnostics.runtime_error {
                    self.last_error = Some(code.clone());
                }
                worker.ready = diagnostics.runtime_error.is_none();
                worker.last_heartbeat = Instant::now();
                worker.diagnostics = *diagnostics;
                let mut response = Response::overview(self.overview());
                response.stop_worker = STOP_REQUESTED.load(Ordering::SeqCst) || self.reconfigure;
                // Keep capture selection/rate in the desktop process. Resolution
                // scaling belongs to the public broker, avoiding two resizes.
                response.user_view = self.user_agent_view.clone().map(|mut view| {
                    view.resolution_mode =
                        Some(crate::local_vnc_server::LocalVncServerResolutionMode::Native);
                    view
                });
                Ok(response)
            }
            Request::UserAgentBootstrap { session_id } => {
                if !self.valid_user_agent(&peer, session_id) {
                    return Err("windows_service_access_denied".into());
                }
                let config = self
                    .config
                    .as_ref()
                    .ok_or_else(|| "windows_service_user_agent_unavailable".to_string())?;
                let worker = self
                    .worker
                    .as_ref()
                    .filter(|worker| worker.session_id == session_id && worker.alive())
                    .ok_or_else(|| "windows_service_access_denied".to_string())?;
                let mut response = Response::overview(self.overview());
                response.user_agent_config = Some(user_agent::UserAgentConfig {
                    bind_address: config.bind_address.clone(),
                    server_id: worker.server_id.clone(),
                    view_only: config.view_only,
                });
                Ok(response)
            }
            Request::DiscoverUserAgent {
                session_id,
                server_id,
            } => {
                let broker = peer.system && peer.pid == std::process::id() && peer.session_id == 0;
                let allowed_caller = broker || self.valid_worker(&peer, Some(session_id));
                if !allowed_caller
                    || self
                        .worker
                        .as_ref()
                        .is_none_or(|worker| worker.server_id != server_id)
                {
                    return Err("windows_service_access_denied".into());
                }
                let mut response = Response::overview(self.overview());
                if let Some(agent) = self
                    .user_agent
                    .as_ref()
                    .filter(|agent| agent.alive() && agent.ready)
                {
                    response.agent_endpoint = user_agent::call(
                        agent.pid,
                        agent.session_id,
                        &user_agent::Command::Discover,
                    )?
                    .endpoint;
                }
                Ok(response)
            }
            Request::BrokerClipboardChunk {
                transfer_id,
                offset,
                total,
                data,
            } => {
                if !peer.system
                    || peer.pid != std::process::id()
                    || peer.session_id != 0
                    || self.config.as_ref().is_none_or(|config| config.view_only)
                {
                    return Err("windows_service_access_denied".into());
                }
                if !self
                    .user_agent_capabilities
                    .iter()
                    .any(|capability| capability == "clipboard.text.write")
                {
                    return Err("windows_service_user_agent_unavailable".into());
                }
                let agent = self
                    .user_agent
                    .as_ref()
                    .filter(|agent| agent.alive() && agent.ready)
                    .ok_or("windows_service_user_agent_unavailable")?;
                user_agent::call(
                    agent.pid,
                    agent.session_id,
                    &user_agent::Command::ClipboardChunk {
                        transfer_id,
                        offset,
                        total,
                        data,
                    },
                )?;
                Ok(Response::ack())
            }
            Request::SecureAttention { session_id } => {
                if !self.valid_worker(&peer, Some(session_id)) {
                    return Err("windows_service_access_denied".into());
                }
                self.sas_request_count = self.sas_request_count.saturating_add(1);
                let result = sas::send_for_peer(&peer);
                self.sas_error_code = result.as_ref().err().cloned();
                result?;
                Ok(Response::overview(self.overview()))
            }
            Request::Reset => {
                if !peer.elevated {
                    return Err("windows_service_admin_required".into());
                }
                credentials::remove()?;
                config::remove()?;
                self.config = None;
                self.reconfigure = true;
                Ok(Response::overview(self.overview()))
            }
        })();
        result.unwrap_or_else(Response::error)
    }

    fn tick(&mut self) {
        self.tick_broker();
        self.tick_worker();
        self.tick_relay();
        self.tick_user_agent();
        self.broker
            .update_service_agent_revision(self.user_agent_revision);
        if let Some(view) = &self.user_agent_view {
            if self.broker_view.as_ref() != Some(view)
                && self.broker.update_view(view.clone()).is_ok()
            {
                self.broker_view = Some(view.clone());
            }
        }
    }

    fn tick_broker(&mut self) {
        if self.reconfigure || self.config.is_none() {
            self.relay.detach();
            self.relay_worker_pid = None;
            let _ = self.broker.stop();
            self.broker_view = None;
            self.broker_error = None;
            self.relay_error = None;
            self.broker_retry_after = Instant::now();
        }
        if self.broker.is_running() || Instant::now() < self.broker_retry_after {
            return;
        }
        let Some(config) = &self.config else {
            let code = self
                .last_error
                .clone()
                .unwrap_or_else(|| "windows_service_invalid_config".into());
            self.finish_startup_attempt(Err(code));
            self.startup_attempt = None;
            return;
        };
        let result = credentials::read().and_then(|password| {
            self.broker.start_service_broker(
                crate::local_vnc_server::LocalVncServerStartRequest {
                    bind_address: config.bind_address.clone(),
                    port: config.port,
                    password: password.to_string(),
                    view_only: config.view_only,
                },
                self.server_id.clone(),
                self.relay.clone(),
            )
        });
        if let Err(code) = result {
            self.finish_startup_attempt(Err(code.clone()));
            self.broker_error = Some(if code.starts_with("port_conflict:") {
                "windows_service_port_conflict".into()
            } else {
                code
            });
            self.broker_retry_after = Instant::now() + Duration::from_secs(2);
        } else {
            self.finish_startup_attempt(Ok(()));
            self.broker_error = None;
        }
    }

    fn finish_startup_attempt(&mut self, result: Result<(), String>) {
        let succeeded = result.is_ok();
        if let Some(attempt) = self.startup_attempt.as_mut() {
            attempt.finish(result);
            if let Ok(directory) = super::native::data_directory() {
                let _ = crate::startup_attempt::write(&directory, attempt);
                let _ = super::native::protect_file(&directory.join("startup-attempt.json"), true);
            }
        }
        if succeeded {
            self.startup_attempt = None;
        }
    }

    fn tick_relay(&mut self) {
        let Some(worker) = self
            .worker
            .as_ref()
            .filter(|worker| worker.alive() && worker.ready)
        else {
            return;
        };
        if self.relay_worker_pid == Some(worker.pid) && self.relay.attached() {
            if self.relay.ready() {
                self.relay_error = None;
            }
            return;
        }
        if self.relay_worker_pid == Some(worker.pid) {
            self.relay_error = Some("windows_service_desktop_relay_unavailable".into());
        }
        if Instant::now() < self.relay_retry_after {
            return;
        }
        let Some(server) = &worker.diagnostics.server else {
            return;
        };
        let pid = worker.pid;
        let result = self
            .relay
            .attach(server.port, worker.desktop_password.to_string());
        self.relay_retry_after = Instant::now() + Duration::from_secs(2);
        match result {
            Ok(()) => {
                self.relay_worker_pid = Some(pid);
            }
            Err(code) => {
                self.relay_error = Some(code);
            }
        }
    }

    fn tick_worker(&mut self) {
        let session = unsafe { WTSGetActiveConsoleSessionId() };
        let changing_session = self
            .worker
            .as_ref()
            .is_some_and(|worker| session != u32::MAX && session != worker.session_id);
        let expired = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.last_heartbeat.elapsed() > Duration::from_secs(20));
        let exited = self.worker.as_ref().is_some_and(|worker| !worker.alive());
        if changing_session || self.reconfigure || expired || exited {
            self.relay.detach();
            self.relay_worker_pid = None;
            self.relay_retry_after = Instant::now();
            self.relay_error = None;
            self.stop_user_agent();
            if exited || expired {
                let failure = if exited {
                    let mut exit = 0;
                    match self.worker.as_ref() {
                        Some(worker) if unsafe { GetExitCodeProcess(worker.process.0, &mut exit) } != 0 =>
                            format!("windows_service_worker_exited:stage=worker_process;exit_code={exit}"),
                        _ => "windows_service_worker_exited:stage=worker_process;exit_code_unavailable".into(),
                    }
                } else {
                    "windows_service_worker_exited:stage=heartbeat;timeout_ms=20000".into()
                };
                // Preserve an earlier capture error and append native process
                // diagnostics instead of replacing the original cause.
                self.last_error = Some(match self.last_error.take() {
                    Some(original)
                        if original.contains("stage=worker_process")
                            || original.contains("stage=heartbeat") =>
                    {
                        original
                    }
                    Some(original) => format!("{original};{failure}"),
                    None => failure,
                });
                self.restart_after = Instant::now() + Duration::from_secs(2);
            }
            if let Some(worker) = self.worker.take() {
                worker.stop();
            }
            self.reconfigure = false;
        }
        if self.worker.is_some()
            || session == u32::MAX
            || self.config.is_none()
            || Instant::now() < self.restart_after
        {
            return;
        }
        if credentials::read().is_err() {
            self.last_error = Some("windows_service_credential_missing".into());
            self.restart_after = Instant::now() + Duration::from_secs(2);
            return;
        }
        match start_worker(session, self.server_id.clone()) {
            Ok(worker) => {
                self.worker = Some(worker);
                self.last_error = None;
            }
            Err(code) => {
                self.last_error = Some(code);
                self.restart_after = Instant::now() + Duration::from_secs(2);
            }
        }
    }

    fn stop_user_agent(&mut self) {
        if let Some(agent) = self.user_agent.take() {
            let _ = user_agent::call(agent.pid, agent.session_id, &user_agent::Command::Stop);
            agent.stop();
        }
        self.user_agent_port = None;
        self.user_agent_view = None;
        self.user_agent_capabilities.clear();
        self.user_agent_error = None;
        self.update_agent_revision(0);
    }

    fn update_agent_revision(&mut self, state_key: u64) {
        if self.agent_state_key != state_key {
            self.agent_state_key = state_key;
            self.user_agent_revision = self.user_agent_revision.saturating_add(1);
        }
    }

    fn tick_user_agent(&mut self) {
        let worker = self
            .worker
            .as_ref()
            .filter(|worker| worker.alive() && worker.ready);
        let worker_identity = worker.map(|worker| (worker.session_id, worker.server_id.clone()));
        if self.user_agent.as_ref().is_some_and(|agent| {
            !agent.alive()
                || agent.last_heartbeat.elapsed() > Duration::from_secs(20)
                || worker_identity.as_ref().is_none_or(|(session, server_id)| {
                    agent.session_id != *session || agent.server_id != *server_id
                })
        }) {
            self.stop_user_agent();
            self.agent_restart_after = Instant::now() + Duration::from_secs(2);
        }
        let Some((session_id, server_id)) = worker_identity else {
            return;
        };
        if self.user_agent.is_none() && Instant::now() >= self.agent_restart_after {
            match start_user_agent(session_id, server_id) {
                Ok(agent) => {
                    self.user_agent = Some(agent);
                    self.user_agent_error = None;
                    self.agent_check_after = Instant::now() + Duration::from_millis(500);
                }
                Err(code) => {
                    self.user_agent_error = Some(code);
                    self.agent_restart_after = Instant::now() + Duration::from_secs(2);
                }
            }
        }
        if Instant::now() < self.agent_check_after {
            return;
        }
        self.agent_check_after = Instant::now() + Duration::from_millis(500);
        let Some(agent) = self.user_agent.as_mut() else {
            return;
        };
        let interactive = unsafe { WTSGetActiveConsoleSessionId() } == session_id
            && self
                .worker
                .as_ref()
                .is_some_and(|worker| worker.diagnostics.desktop.as_deref() == Some("Default"));
        match user_agent::call(
            agent.pid,
            session_id,
            &user_agent::Command::Status { interactive },
        ) {
            Ok(reply) => {
                if let Some(status) = reply.status {
                    agent.ready = true;
                    agent.last_heartbeat = Instant::now();
                    self.user_agent_port = Some(status.port);
                    self.user_agent_view = status.view;
                    self.user_agent_capabilities = status.capabilities;
                    self.user_agent_error = None;
                    let key = (u64::from(agent.pid) << 32)
                        | (u64::from(status.port) << 1)
                        | u64::from(interactive);
                    self.update_agent_revision(key);
                }
            }
            Err(_) => {
                agent.ready = false;
                self.user_agent_port = None;
                self.user_agent_error = Some("windows_service_user_agent_unavailable".into());
                self.update_agent_revision(0);
            }
        }
    }
}

fn enable_privilege(name: &str) -> Result<(), String> {
    let token = native::process_token(
        unsafe { GetCurrentProcess() },
        TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
    )?;
    let mut privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        ..Default::default()
    };
    let mut luid = windows_sys::Win32::Foundation::LUID::default();
    if unsafe { LookupPrivilegeValueW(null(), native::wide(name).as_ptr(), &mut luid) } == 0 {
        return Err(format!(
            "windows_service_worker_start_failed:stage=LookupPrivilegeValueW;win32_error={}",
            native::last_error()
        ));
    }
    privileges.Privileges[0] = LUID_AND_ATTRIBUTES {
        Luid: luid,
        Attributes: SE_PRIVILEGE_ENABLED,
    };
    if unsafe { AdjustTokenPrivileges(token.0, 0, &privileges, 0, null_mut(), null_mut()) } == 0
        || native::last_error() == 1300
    {
        return Err(format!(
            "windows_service_worker_start_failed:stage=AdjustTokenPrivileges;win32_error={}",
            native::last_error()
        ));
    }
    Ok(())
}

fn start_worker(session_id: u32, server_id: String) -> Result<WorkerProcess, String> {
    for name in [
        "SeTcbPrivilege",
        "SeAssignPrimaryTokenPrivilege",
        "SeIncreaseQuotaPrivilege",
    ] {
        enable_privilege(name)?;
    }
    let source = native::process_token(
        unsafe { GetCurrentProcess() },
        TOKEN_QUERY | TOKEN_DUPLICATE,
    )?;
    if !native::token_is_system(source.0) {
        return Err("windows_service_access_denied".into());
    }
    let mut token = null_mut();
    if unsafe {
        DuplicateTokenEx(
            source.0,
            TOKEN_ALL_ACCESS,
            null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut token,
        )
    } == 0
    {
        return Err(format!(
            "windows_service_worker_start_failed:stage=DuplicateTokenEx;win32_error={}",
            native::last_error()
        ));
    }
    let token = Handle::new(token, "windows_service_worker_start_failed")?;
    if unsafe {
        SetTokenInformation(
            token.0,
            TokenSessionId,
            (&session_id as *const u32).cast(),
            size_of::<u32>() as u32,
        )
    } == 0
    {
        return Err(format!(
            "windows_service_worker_start_failed:stage=SetTokenInformation;win32_error={}",
            native::last_error()
        ));
    }
    start_process(&token, session_id, "--worker", server_id, null())
}

struct UserEnvironment(*mut std::ffi::c_void);
impl Drop for UserEnvironment {
    fn drop(&mut self) {
        unsafe { DestroyEnvironmentBlock(self.0) };
    }
}

fn start_user_agent(session_id: u32, server_id: String) -> Result<WorkerProcess, String> {
    let mut source = null_mut();
    if unsafe { WTSQueryUserToken(session_id, &mut source) } == 0 {
        return Err("windows_service_user_agent_login_required".into());
    }
    let source = Handle::new(source, "windows_service_user_agent_unavailable")?;
    let token = super::user_token::for_session(source, session_id)?;
    let mut environment = null_mut();
    if unsafe { CreateEnvironmentBlock(&mut environment, token.0, 0) } == 0 {
        return Err("windows_service_user_agent_unavailable".into());
    }
    let environment = UserEnvironment(environment);
    start_process(&token, session_id, "--user-agent", server_id, environment.0)
        .map_err(|_| "windows_service_user_agent_unavailable".into())
}

fn start_process(
    token: &Handle,
    session_id: u32,
    role: &str,
    server_id: String,
    environment: *const std::ffi::c_void,
) -> Result<WorkerProcess, String> {
    let job = Handle::new(
        unsafe { CreateJobObjectW(null(), null()) },
        "windows_service_worker_start_failed",
    )?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | if role == "--user-agent" {
            windows_sys::Win32::System::JobObjects::JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK
        } else {
            0
        };
    if unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(format!(
            "windows_service_worker_start_failed:stage=SetInformationJobObject;win32_error={}",
            native::last_error()
        ));
    }
    let binary = native::installed_binary()?;
    native::reject_reparse(&binary)?;
    let mut command = native::wide(format!(
        "\"{}\" {role} --session-id {session_id}",
        binary.display()
    ));
    let mut desktop = native::wide("WinSta0\\Default");
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        lpDesktop: desktop.as_mut_ptr(),
        ..Default::default()
    };
    // The ordinary user Agent must retain access to its own process/threads;
    // SYSTEM desktop workers keep their existing object security.
    let object_security = if role == "--user-agent" {
        Some(super::user_token::object_security(token.0)?)
    } else {
        None
    };
    let object_attributes = object_security
        .as_ref()
        .map(|security| security.attributes());
    let object_attributes = object_attributes
        .as_ref()
        .map_or(null(), |attributes| attributes as *const _);
    let mut process = PROCESS_INFORMATION::default();
    if unsafe {
        CreateProcessAsUserW(
            token.0,
            native::wide(&binary).as_ptr(),
            command.as_mut_ptr(),
            object_attributes,
            object_attributes,
            0,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | if environment.is_null() {
                    0
                } else {
                    CREATE_UNICODE_ENVIRONMENT
                },
            environment,
            native::wide(native::install_directory()?).as_ptr(),
            &startup,
            &mut process,
        )
    } == 0
    {
        return Err(format!(
            "windows_service_worker_start_failed:stage=CreateProcessAsUserW;win32_error={}",
            native::last_error()
        ));
    }
    let process_handle = Handle::new(process.hProcess, "windows_service_worker_start_failed")?;
    let thread = Handle::new(process.hThread, "windows_service_worker_start_failed")?;
    if unsafe { AssignProcessToJobObject(job.0, process_handle.0) } == 0 {
        let error = native::last_error();
        unsafe {
            windows_sys::Win32::System::Threading::TerminateProcess(process_handle.0, 1);
        }
        return Err(format!("windows_service_worker_start_failed:stage=AssignProcessToJobObject;win32_error={error}"));
    }
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        return Err(format!(
            "windows_service_worker_start_failed:stage=ResumeThread;win32_error={}",
            native::last_error()
        ));
    }
    Ok(WorkerProcess {
        process: process_handle,
        job,
        pid: process.dwProcessId,
        session_id,
        diagnostics: WorkerDiagnostics::default(),
        ready: false,
        last_heartbeat: Instant::now(),
        server_id,
        desktop_password: zeroize::Zeroizing::new({
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(uuid::Uuid::new_v4().as_bytes())[..8]
                .to_string()
        }),
    })
}

fn report_state(state: u32, exit_code: u32) {
    let handle = SERVICE_HANDLE.load(Ordering::SeqCst) as SERVICE_STATUS_HANDLE;
    if handle.is_null() {
        return;
    }
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN | SERVICE_ACCEPT_SESSIONCHANGE
        } else {
            0
        },
        dwWin32ExitCode: exit_code,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: if state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING {
            1
        } else {
            0
        },
        dwWaitHint: if state == SERVICE_STOP_PENDING {
            5000
        } else {
            0
        },
    };
    unsafe {
        SetServiceStatus(handle, &status);
    }
}

unsafe extern "system" fn control(
    code: u32,
    _event: u32,
    _data: *mut std::ffi::c_void,
    _context: *mut std::ffi::c_void,
) -> u32 {
    match code {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            STOP_REQUESTED.store(true, Ordering::SeqCst);
            report_state(SERVICE_STOP_PENDING, 0);
            0
        }
        SERVICE_CONTROL_SESSIONCHANGE | SERVICE_CONTROL_INTERROGATE => 0,
        _ => 120,
    }
}

unsafe extern "system" fn service_main(argc: u32, argv: *mut *mut u16) {
    let result = std::panic::catch_unwind(|| {
        let handle = unsafe {
            RegisterServiceCtrlHandlerExW(
                native::wide(native::SERVICE_NAME).as_ptr(),
                Some(control),
                null(),
            )
        };
        if handle.is_null() {
            return Err("windows_service_start_failed".to_string());
        }
        SERVICE_HANDLE.store(handle as usize, Ordering::SeqCst);
        STOP_REQUESTED.store(false, Ordering::SeqCst);
        let configure_only = if argc == 2 && !argv.is_null() {
            let value = unsafe { *argv.add(1) };
            if value.is_null() {
                false
            } else {
                let expected = native::wide("--configure-only");
                expected
                    .iter()
                    .enumerate()
                    .all(|(index, unit)| unsafe { *value.add(index) == *unit })
            }
        } else {
            false
        };
        CONFIGURE_ONLY.store(configure_only, Ordering::SeqCst);
        report_state(SERVICE_START_PENDING, 0);
        let mut startup_attempt = (!configure_only
            && super::install::service_auto_start() == Some(true))
        .then(crate::startup_attempt::StartupAttempt::starting);
        if let Some(attempt) = &startup_attempt {
            if let Ok(directory) = super::native::data_directory() {
                let _ = crate::startup_attempt::write(&directory, attempt);
                let _ = super::native::protect_file(&directory.join("startup-attempt.json"), true);
            }
        }
        let mut server = ipc::Server::new().inspect_err(|code| {
            if let Some(attempt) = &mut startup_attempt {
                attempt.finish(Err(code.clone()));
                if let Ok(directory) = super::native::data_directory() {
                    let _ = crate::startup_attempt::write(&directory, attempt);
                    let _ =
                        super::native::protect_file(&directory.join("startup-attempt.json"), true);
                }
            }
        })?;
        let mut supervisor = Supervisor::new();
        supervisor.startup_attempt = startup_attempt;
        report_state(SERVICE_RUNNING, 0);
        while !STOP_REQUESTED.load(Ordering::SeqCst) {
            if let Err(code) = server.poll(|request, peer| supervisor.handle(request, peer)) {
                supervisor.finish_startup_attempt(Err(code.clone()));
                return Err(code);
            }
            if !CONFIGURE_ONLY.load(Ordering::SeqCst) {
                supervisor.tick();
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        // Continue serving the bounded heartbeat stop response before forcing
        // the owned Job, so a normal RFB disconnect/resource release can finish.
        let grace = Instant::now() + Duration::from_millis(1000);
        while supervisor.worker.as_ref().is_some_and(WorkerProcess::alive) && Instant::now() < grace
        {
            let _ = server.poll(|request, peer| supervisor.handle(request, peer));
            std::thread::sleep(Duration::from_millis(20));
        }
        supervisor.relay.detach();
        let _ = supervisor.broker.stop();
        if let Some(worker) = supervisor.worker.take() {
            worker.stop();
        }
        supervisor.stop_user_agent();
        Ok::<(), String>(())
    });
    report_state(
        SERVICE_STOPPED,
        if matches!(result, Ok(Ok(()))) {
            0
        } else {
            1064
        },
    );
}

pub(super) fn run_service() -> Result<(), String> {
    let mut name = native::wide(native::SERVICE_NAME);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: null_mut(),
            lpServiceProc: None,
        },
    ];
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        Err("windows_service_start_failed".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_agent_repair_tests.rs"]
mod agent_repair_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_local_peer_can_dispatch_disconnect_without_password_management_role() {
        let mut supervisor = Supervisor::new();
        let response = supervisor.handle(
            Request::DisconnectConnection {
                connection_id: "ip:192.168.85.1".into(),
            },
            Peer {
                pid: 1,
                session_id: 1,
                system: false,
                elevated: false,
                user_sid: String::new(),
            },
        );
        // Reaching the stopped broker proves this request passed dispatch without
        // consulting password roles, even for a peer with no delegated role.
        assert!(response
            .error_code
            .as_deref()
            .is_some_and(|code| code.starts_with("not_running:")));
    }
}
