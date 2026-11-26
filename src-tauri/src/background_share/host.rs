use super::{
    config::{self, Config},
    ipc,
    types::{
        BackgroundShareLaunchMode, BackgroundShareOverview, BackgroundSharePermissionSource,
        BackgroundShareRuntime, Reply, Request,
    },
};
use crate::local_vnc_server::{LocalVncServerManager, LocalVncServerStatus};

// Recheck at the listener boundary: a grant may be revoked after frontend preflight.
pub(super) fn validate_start_permissions(
    permissions: &[crate::local_vnc_server::LocalVncServerPermission],
    view_only: bool,
) -> Result<(), String> {
    use crate::local_vnc_server::{
        LocalVncServerPermissionKind as Kind, LocalVncServerPermissionStatus as Status,
    };
    crate::local_vnc_server::validate_required_capture_permissions(permissions)?;
    if cfg!(target_os = "macos") {
        for kind in [Kind::ScreenRecording, Kind::Accessibility] {
            if kind == Kind::Accessibility && view_only {
                continue;
            }
            if !permissions
                .iter()
                .any(|permission| permission.kind == kind && permission.status == Status::Granted)
            {
                return Err(
                    "permission_missing: required background permission is not granted".into(),
                );
            }
        }
    }
    Ok(())
}

struct Host {
    manager: LocalVncServerManager,
    service_fallback: bool,
    temporary_share: bool,
    config: Config,
    error: Option<String>,
    stopped: bool,
    permissions: Vec<crate::local_vnc_server::LocalVncServerPermission>,
    #[cfg(target_os = "macos")]
    permission_identity: Option<super::permission_identity::PermissionIdentity>,
    #[cfg(target_os = "macos")]
    permission_history: super::permission_identity::PermissionHistory,
}

impl Host {
    fn overview(&self) -> Result<BackgroundShareOverview, String> {
        let server = self
            .manager
            .status_with_permissions(self.permissions.clone())?;
        Ok(BackgroundShareOverview {
            service_fallback: self.service_fallback,
            temporary_share: self.temporary_share && server.status == LocalVncServerStatus::Running,
            server,
            configured: self.config.port > 0,
            credential_configured: self.config.credential_ref.is_some(),
            auto_start: self.config.auto_start,
            startup_registration: super::types::StartupRegistration::Unknown,
            last_login_attempt: None,
            process_running: !self.stopped,
            last_error_code: self.error.clone(),
            runtime: Some(BackgroundShareRuntime {
                pid: std::process::id(),
                build_id: super::BUILD_ID.into(),
            }),
            current_build_id: super::BUILD_ID.into(),
            permission_source: BackgroundSharePermissionSource::BackgroundCached,
            restart_supported: cfg!(target_os = "macos"),
            ipc_version: super::IPC_VERSION,
            #[cfg(target_os = "macos")]
            permission_identity: self.permission_identity.clone(),
            signature_changed_permissions: {
                #[cfg(target_os = "macos")]
                {
                    self.permission_history
                        .signature_changed(self.permission_identity.as_ref(), &self.permissions)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    vec![]
                }
            },
        })
    }

    fn handle(&mut self, request: Request) -> Reply {
        self.handle_with_permission_probe(
            request,
            crate::local_vnc_server::get_local_vnc_server_permissions,
        )
    }

    // Tests replace the OS probe; ordinary status must never invoke it.
    fn handle_with_permission_probe(
        &mut self,
        request: Request,
        mut probe: impl FnMut() -> Vec<crate::local_vnc_server::LocalVncServerPermission>,
    ) -> Reply {
        if let Request::Permission(kind) = request {
            return match crate::local_vnc_server::request_local_vnc_server_permission(kind) {
                Ok(permission) => Reply {
                    overview: None,
                    error_code: None,
                    permission: Some(permission),
                },
                Err(code) => Reply {
                    overview: None,
                    error_code: Some(code),
                    permission: None,
                },
            };
        }
        let fallback = matches!(&request, Request::StartServiceFallback(_));
        let inspected_permissions =
            matches!(
                &request,
                Request::Status | Request::Start(_) | Request::StartServiceFallback(_)
            ) || matches!(&request, Request::SetAutoStart { enabled: true })
                && cfg!(target_os = "macos");
        let result = (|| {
            match request {
                Request::Status => {
                    self.permissions = probe();
                }
                Request::RuntimeStatus => {}
                Request::SetAutoStart { enabled } => {
                    if enabled {
                        super::validate_background_runtime()?;
                        #[cfg(target_os = "macos")]
                        {
                            self.permissions = probe();
                            validate_start_permissions(&self.permissions, self.config.view_only)
                                .map_err(|_| {
                                    "background_share_auto_start_permission_required".to_string()
                                })?;
                        }
                    }
                    config::validate_auto_start(&self.config, enabled)?;
                    let mut next = self.config.clone();
                    next.auto_start = enabled;
                    config::persist_auto_start(&self.config, &next)?;
                    self.config = next;
                    self.error = None;
                }
                Request::Start(mut request) | Request::StartServiceFallback(mut request) => {
                    if self.manager.status()?.status == LocalVncServerStatus::Running {
                        return Err("background_share_already_running".into());
                    }
                    // Serialize handoff against install/configure/start/stop and
                    // recheck at the actual listener boundary, after host upgrade.
                    #[cfg(target_os = "windows")]
                    let _service_gate = if fallback {
                        let gate = crate::windows_service::native::lifecycle_lock()?;
                        let service = crate::windows_service::status()?;
                        crate::windows_service::fallback::validate_stopped(&service)?;
                        if service.port != Some(request.port)
                            || service.bind_address.as_deref()
                                != Some(request.bind_address.as_str())
                            || service.view_only != request.view_only
                        {
                            return Err("windows_service_invalid_config".into());
                        }
                        if !request.temporary_password || request.password.is_empty() {
                            return Err("background_share_credential_missing".into());
                        }
                        Some(gate)
                    } else {
                        super::validate_background_runtime()?;
                        None
                    };
                    #[cfg(not(target_os = "windows"))]
                    {
                        if fallback {
                            return Err("background_share_unsupported".into());
                        }
                        super::validate_background_runtime()?;
                    }
                    let has_new_password = !request.password.is_empty();
                    let mut next = self.config.clone();
                    next.apply_start(&request);
                    let permissions = crate::local_vnc_server::get_local_vnc_server_permissions();
                    self.permissions = permissions.clone();
                    validate_start_permissions(&permissions, request.view_only)?;
                    let passwords = config::passwords_for_start(&self.config, &mut request)?;
                    // Validate the listener first. Failed starts preserve the
                    // previous login registration, settings and saved secret.
                    self.manager
                        .start(next.start_request(passwords.active.to_string()))?;
                    if let Some(view) = next.view.clone() {
                        let _ = self.manager.update_view(view);
                    }
                    let persist = config::persist_start(
                        &mut next,
                        request.temporary_password,
                        has_new_password.then_some(passwords.active.as_str()),
                        passwords.previous.as_deref().map(String::as_str),
                    );
                    if let Err(code) = persist {
                        let _ = self.manager.stop();
                        return Err(code);
                    }
                    if !request.temporary_password {
                        self.config = next;
                    }
                    self.service_fallback = fallback;
                    self.temporary_share = request.temporary_password;
                    self.error = None;
                }
                Request::Stop | Request::Recover => {
                    if matches!(request, Request::Recover)
                        && self.manager.status()?.status == LocalVncServerStatus::Running
                    {
                        return Err("background_share_already_running".into());
                    }
                    self.manager.stop()?;
                    self.temporary_share = false;
                    self.stopped = true;
                }
                Request::View(view) => {
                    self.manager.update_view(view.clone())?;
                    self.config.view = Some(view);
                    self.config.save()?;
                }
                Request::Disconnect { connection_id } => {
                    self.manager.disconnect_connection(connection_id)?;
                    super::disconnect_lock::session_ended()
                        .map_err(|_| "background_share_screen_lock_failed".to_string())?;
                }
                Request::Permission(_) => unreachable!(),
            }
            let mut overview = self.overview()?;
            if inspected_permissions {
                #[cfg(target_os = "macos")]
                self.permission_history
                    .record(self.permission_identity.as_ref(), &self.permissions);
                overview.permission_source = BackgroundSharePermissionSource::Background;
            }
            Ok(overview)
        })();
        match result {
            Ok(overview) => Reply {
                overview: Some(overview),
                error_code: None,
                permission: None,
            },
            Err(code) => Reply {
                overview: None,
                error_code: Some(code),
                permission: None,
            },
        }
    }
}

pub(super) fn run(mode: BackgroundShareLaunchMode) -> Result<(), String> {
    let mut login_attempt = (mode == BackgroundShareLaunchMode::Login)
        .then(crate::startup_attempt::StartupAttempt::starting);
    let attempt_directory = super::paths::data_directory()
        .ok()
        .filter(|directory| super::paths::ensure_private_directory(directory).is_ok());
    if let (Some(attempt), Some(directory)) = (&login_attempt, &attempt_directory) {
        let _ = crate::startup_attempt::write(directory, attempt);
    }
    let mut finish_attempt = |result: Result<(), String>| {
        if let (Some(attempt), Some(directory)) = (&mut login_attempt, &attempt_directory) {
            attempt.finish(result);
            let _ = crate::startup_attempt::write(directory, attempt);
        }
    };
    let server = ipc::Server::new().inspect_err(|code| {
        finish_attempt(Err(code.clone()));
    })?;
    let config = Config::load().inspect_err(|code| {
        finish_attempt(Err(code.clone()));
    })?;
    let mut host = Host {
        manager: LocalVncServerManager::default(),
        service_fallback: false,
        temporary_share: false,
        config,
        error: None,
        stopped: false,
        permissions: crate::local_vnc_server::pending_local_vnc_server_permissions(),
        #[cfg(target_os = "macos")]
        permission_identity: super::permission_identity::current(),
        #[cfg(target_os = "macos")]
        permission_history: super::permission_identity::PermissionHistory::load(),
    };
    if host.config.restores_listener(mode) {
        let startup = (|| {
            super::validate_background_runtime()?;
            let password = config::read_password(&host.config)?;
            host.config.validate_listener(&password)?;
            let permissions = crate::local_vnc_server::get_local_vnc_server_permissions();
            host.permissions = permissions.clone();
            #[cfg(target_os = "macos")]
            host.permission_history
                .record(host.permission_identity.as_ref(), &permissions);
            validate_start_permissions(&permissions, host.config.view_only)?;
            host.manager
                .start(host.config.start_request(password.to_string()))?;
            if let Some(view) = host.config.view.clone() {
                let _ = host.manager.update_view(view);
            }
            Ok::<(), String>(())
        })();
        finish_attempt(startup.clone());
        host.error = startup.err();
    } else if mode == BackgroundShareLaunchMode::Login {
        // A stale OS registration may still launch us after policy was disabled.
        finish_attempt(Err("background_share_auto_start_disabled".into()));
    }
    while !host.stopped {
        server.poll(|request| host.handle(request))?;
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../../tests/support/background_share_status_tests.rs"]
mod status_tests;
