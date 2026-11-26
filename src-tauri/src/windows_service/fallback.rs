//! A failed service start may borrow the user runtime for this attempt only.
use super::{ServiceStartOutcome, WindowsServiceOverview};
use crate::background_share::{BackgroundShareOverview, BackgroundShareStartRequest};

fn eligible(error: &str) -> bool {
    matches!(
        error.split(':').next().unwrap_or(error),
        "windows_service_start_failed"
            | "windows_service_worker_start_failed"
            | "windows_service_worker_exited"
            | "windows_service_ipc_timeout"
            | "windows_service_worker_identity_failed"
            | "windows_service_ipc_unavailable"
            | "windows_service_port_conflict"
    )
}

pub(super) trait Runtime {
    fn start(&mut self) -> Result<WindowsServiceOverview, String>;
    /// Stop and independently verify SCM/worker/listener ownership before handoff.
    fn quiesce(&mut self) -> Result<(), String>;
    fn fallback(
        &mut self,
        request: BackgroundShareStartRequest,
    ) -> Result<BackgroundShareOverview, String>;
    fn log(&mut self, event: &str, error: Option<&str>);
}

pub(super) fn run(
    runtime: &mut impl Runtime,
    mut request: BackgroundShareStartRequest,
) -> Result<ServiceStartOutcome, String> {
    runtime.log("service_start_requested", None);
    let error = match runtime.start() {
        Ok(service) => {
            runtime.log("service_start_succeeded", None);
            return Ok(ServiceStartOutcome {
                service: Some(service),
                background: None,
                service_error: None,
                fallback_error: None,
                password_required: false,
            });
        }
        Err(error) => error,
    };
    // Always record the original failure, including when handoff later succeeds.
    runtime.log("service_start_failed", Some(&error));
    if !eligible(&error) {
        return Err(error);
    }
    let mut outcome = ServiceStartOutcome {
        service: None,
        background: None,
        service_error: Some(error),
        fallback_error: None,
        password_required: false,
    };
    if let Err(error) = runtime.quiesce() {
        runtime.log("service_fallback_blocked", Some(&error));
        outcome.fallback_error = Some(error);
        return Ok(outcome);
    }
    // Never substitute another ordinary-sharing saved credential for SYSTEM's.
    if request.password.is_empty() {
        runtime.log("service_fallback_password_required", None);
        outcome.password_required = true;
        return Ok(outcome);
    }
    request.temporary_password = true;
    runtime.log("service_fallback_started", None);
    match runtime.fallback(request) {
        Ok(background) => {
            runtime.log("service_fallback_succeeded", None);
            outcome.background = Some(background);
        }
        Err(error) => {
            runtime.log("service_fallback_failed", Some(&error));
            outcome.fallback_error = Some(error);
        }
    }
    Ok(outcome)
}

#[cfg(target_os = "windows")]
pub(super) fn start(
    request: BackgroundShareStartRequest,
    logger: crate::app_log::AppLogger,
) -> Result<ServiceStartOutcome, String> {
    struct Native {
        logger: crate::app_log::AppLogger,
        attempt: String,
        started: std::time::Instant,
    }
    impl Runtime for Native {
        fn start(&mut self) -> Result<WindowsServiceOverview, String> {
            super::start()?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                let status = super::status()?;
                if status.worker_running
                    && status.server.as_ref().is_some_and(|server| {
                        server.status == crate::local_vnc_server::LocalVncServerStatus::Running
                    })
                {
                    return Ok(status);
                }
                if let Some(error) = status.last_error_code {
                    return Err(error);
                }
                if status.status == "stopped" {
                    return Err(status
                        .last_service_attempt
                        .and_then(|attempt| attempt.error_code)
                        .unwrap_or_else(|| "windows_service_worker_exited".into()));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(
                        "windows_service_start_failed:stage=listener_ready;timeout_ms=15000".into(),
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        fn quiesce(&mut self) -> Result<(), String> {
            let status = super::status()?;
            if status.status != "stopped" {
                super::stop()?;
            }
            let status = super::status()?;
            validate_stopped(&status)
        }
        fn fallback(
            &mut self,
            request: BackgroundShareStartRequest,
        ) -> Result<BackgroundShareOverview, String> {
            crate::background_share::start_service_fallback(request)
        }
        fn log(&mut self, event: &str, error: Option<&str>) {
            self.logger.submit(
                if error.is_some() {
                    crate::app_log::LogLevel::Error
                } else {
                    crate::app_log::LogLevel::Info
                },
                "windows_service",
                event,
                error.unwrap_or(event),
                serde_json::json!({
                    "attempt_id": self.attempt, "elapsed_ms": self.started.elapsed().as_millis(),
                    "error": error,
                    "diagnostics": error.map(|_| super::install::failure_diagnostics()),
                }),
            );
        }
    }
    run(
        &mut Native {
            logger,
            attempt: uuid::Uuid::new_v4().to_string(),
            started: std::time::Instant::now(),
        },
        request,
    )
}

pub(crate) fn validate_stopped(status: &WindowsServiceOverview) -> Result<(), String> {
    if !status.installed
        || status.status != "stopped"
        || status.worker_running
        || status.server.as_ref().is_some_and(|server| {
            server.status != crate::local_vnc_server::LocalVncServerStatus::Stopped
        })
    {
        return Err(
            "windows_service_start_failed:stage=fallback_safety;service_not_stopped".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_fallback_tests.rs"]
mod tests;
