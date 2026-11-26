use super::*;

struct Fake {
    failure: Option<&'static str>,
    unsafe_stop: bool,
    fallback_failure: bool,
    events: Vec<String>,
    requests: Vec<(String, u16, bool, bool, String)>,
}
impl Runtime for Fake {
    fn start(&mut self) -> Result<WindowsServiceOverview, String> {
        self.failure.map_or_else(
            || Ok(WindowsServiceOverview::unsupported()),
            |error| Err(error.into()),
        )
    }
    fn quiesce(&mut self) -> Result<(), String> {
        self.events.push("quiesce".into());
        if self.unsafe_stop {
            Err("windows_service_stop_failed".into())
        } else {
            Ok(())
        }
    }
    fn fallback(
        &mut self,
        request: BackgroundShareStartRequest,
    ) -> Result<BackgroundShareOverview, String> {
        self.requests.push((
            request.bind_address.clone(),
            request.port,
            request.view_only,
            request.temporary_password,
            request.password.clone(),
        ));
        if self.fallback_failure {
            return Err("background_share_port_conflict".into());
        }
        Ok(BackgroundShareOverview {
            server: crate::local_vnc_server::pending_local_vnc_server_overview(),
            service_fallback: true,
            temporary_share: true,
            configured: true,
            credential_configured: false,
            auto_start: false,
            startup_registration: Default::default(),
            last_login_attempt: None,
            process_running: true,
            last_error_code: None,
            runtime: None,
            current_build_id: "test".into(),
            permission_source: Default::default(),
            restart_supported: false,
            ipc_version: 9,
            #[cfg(target_os = "macos")]
            permission_identity: None,
            signature_changed_permissions: vec![],
        })
    }
    fn log(&mut self, event: &str, error: Option<&str>) {
        self.events.push(format!("{event}:{}", error.unwrap_or("")));
    }
}
fn fake(failure: Option<&'static str>) -> Fake {
    Fake {
        failure,
        unsafe_stop: false,
        fallback_failure: false,
        events: vec![],
        requests: vec![],
    }
}
fn request(password: &str) -> BackgroundShareStartRequest {
    BackgroundShareStartRequest {
        bind_address: "127.0.0.1".into(),
        port: 5910,
        view_only: true,
        temporary_password: false,
        password: password.into(),
    }
}
#[test]
fn records_original_native_failure_before_successful_temporary_handoff() {
    let error = "windows_service_start_failed:stage=StartServiceW;win32_error=1067";
    let mut runtime = fake(Some(error));
    let outcome = run(&mut runtime, request("fixture")).unwrap();
    assert!(outcome.background.as_ref().unwrap().service_fallback);
    assert_eq!(outcome.service_error.as_deref(), Some(error));
    assert_eq!(
        runtime.requests,
        vec![("127.0.0.1".into(), 5910, true, true, "fixture".into())]
    );
    assert!(runtime.events[1].contains(error));
    assert_eq!(runtime.events[2], "quiesce");
    assert_eq!(
        runtime.events.last().unwrap(),
        "service_fallback_succeeded:"
    );
    assert!(!runtime.events.iter().any(|event| event.contains("fixture")));
    // A later attempt always calls the installed service again.
    runtime.failure = None;
    assert!(run(&mut runtime, request("")).unwrap().service.is_some());
    assert_eq!(runtime.requests.len(), 1);
}
#[test]
fn cancellation_and_authorization_failures_never_start_ordinary_sharing() {
    for error in [
        "windows_service_uac_cancelled",
        "windows_service_access_denied",
        "windows_service_invalid_config",
        "windows_service_elevation_denied",
    ] {
        let mut runtime = fake(Some(error));
        assert!(run(&mut runtime, request("fixture")).is_err());
        assert!(runtime.requests.is_empty());
        assert!(!runtime.events.iter().any(|event| event == "quiesce"));
    }
}
#[test]
fn missing_password_does_not_reuse_an_ordinary_saved_credential() {
    let mut runtime = fake(Some("windows_service_worker_exited"));
    let result = run(&mut runtime, request("")).unwrap();
    assert!(result.password_required);
    assert!(runtime.requests.is_empty());
}
#[test]
fn failure_to_stop_blocks_fallback_and_preserves_both_errors() {
    let mut runtime = fake(Some("windows_service_worker_exited"));
    runtime.unsafe_stop = true;
    let result = run(&mut runtime, request("fixture")).unwrap();
    assert_eq!(
        result.service_error.as_deref(),
        Some("windows_service_worker_exited")
    );
    assert_eq!(
        result.fallback_error.as_deref(),
        Some("windows_service_stop_failed")
    );
    assert!(runtime.requests.is_empty());
}
#[test]
fn fallback_failure_preserves_service_error_and_fallback_error() {
    let mut runtime = fake(Some("windows_service_worker_exited"));
    runtime.fallback_failure = true;
    let result = run(&mut runtime, request("fixture")).unwrap();
    assert_eq!(
        result.service_error.as_deref(),
        Some("windows_service_worker_exited")
    );
    assert_eq!(
        result.fallback_error.as_deref(),
        Some("background_share_port_conflict")
    );
}
#[test]
fn handoff_requires_installed_stopped_service_and_no_live_worker_or_listener() {
    let mut service = WindowsServiceOverview::unsupported();
    service.installed = true;
    for status in ["running", "unknown", "start_pending", "stop_pending"] {
        service.status = status.into();
        assert!(validate_stopped(&service).is_err());
    }
    service.status = "stopped".into();
    assert!(validate_stopped(&service).is_ok());
    service.worker_running = true;
    assert!(validate_stopped(&service).is_err());
    service.worker_running = false;
    let mut server = crate::local_vnc_server::pending_local_vnc_server_overview();
    server.status = crate::local_vnc_server::LocalVncServerStatus::Running;
    service.server = Some(server);
    assert!(validate_stopped(&service).is_err());
}
