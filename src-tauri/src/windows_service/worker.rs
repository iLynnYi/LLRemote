use super::{
    ipc::{self, Request, WorkerDiagnostics},
    native,
};
use std::time::{Duration, Instant};
use zeroize::Zeroize;

pub(super) fn run(session_id: u32) -> Result<(), String> {
    native::initialize_worker_dpi_awareness()?;
    if native::current_session()? != session_id {
        return Err("windows_service_worker_identity_failed".into());
    }
    let mut response = ipc::call(&Request::WorkerBootstrap { session_id })?.into_result()?;
    let mut config = response
        .worker_config
        .take()
        .ok_or_else(|| "windows_service_credential_missing".to_string())?;
    let server_id = response
        .server_id
        .take()
        .ok_or_else(|| "windows_service_worker_identity_failed".to_string())?;
    crate::local_vnc_server_platform::windows_desktop::enable_service_worker()?;
    let manager = crate::local_vnc_server::LocalVncServerManager::default();
    let request = crate::local_vnc_server::LocalVncServerStartRequest {
        // Only the broker owns the configured public endpoint. Each replaceable
        // desktop source uses a private, ephemeral loopback listener and secret.
        port: 0,
        bind_address: "127.0.0.1".into(),
        password: std::mem::take(&mut config.password),
        view_only: config.view_only,
    };
    if let Err(error) = manager.start_service_worker(request, server_id) {
        let code = if error.starts_with("port_conflict:") {
            "windows_service_port_conflict"
        } else {
            "windows_service_worker_start_failed"
        };
        let _ = ipc::call(&Request::WorkerHeartbeat(Box::new(WorkerDiagnostics {
            runtime_error: Some(code.into()),
            ..WorkerDiagnostics::default()
        })));
        return Err(code.into());
    }
    config.password.zeroize();
    let mut last_heartbeat = Instant::now();
    let mut applied_user_view = None;
    let outcome = loop {
        let diagnostics = crate::local_vnc_server_platform::windows_desktop::diagnostics();
        let heartbeat = WorkerDiagnostics {
            desktop: diagnostics.desktop,
            desktop_epoch: diagnostics.desktop_epoch,
            input_status: diagnostics.input_status,
            input_failures: diagnostics.input_failures,
            last_error_code: diagnostics.last_error_code,
            runtime_error: None,
            server: manager.status().ok(),
        };
        match ipc::call(&Request::WorkerHeartbeat(Box::new(heartbeat)))
            .and_then(ipc::Response::into_result)
        {
            Ok(response) if response.stop_worker => break Ok(()),
            Ok(response) => {
                last_heartbeat = Instant::now();
                if let Some(overview) = response.overview.as_ref() {
                    manager.update_service_agent_revision(overview.user_agent_revision);
                }
                if let Some(view) = response.user_view {
                    if applied_user_view.as_ref() != Some(&view)
                        && manager.update_view(view.clone()).is_ok()
                    {
                        applied_user_view = Some(view);
                    }
                }
            }
            Err(code)
                if matches!(
                    code.as_str(),
                    "windows_service_ipc_timeout" | "windows_service_ipc_unavailable"
                ) && last_heartbeat.elapsed() < Duration::from_secs(20) => {}
            Err(code) => break Err(code),
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    // The supervisor owns an outer process deadline if native cleanup blocks.
    let _ = manager.stop();
    outcome
}
