use crate::{app_log, remote_session::RemoteSessionManager};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{AppHandle, Manager, RunEvent};

type ShutdownResult = Result<(), &'static str>;

#[derive(Default)]
pub(crate) struct AppShutdown {
    state: Mutex<ShutdownState>,
}

#[derive(Default)]
enum ShutdownState {
    #[default]
    Idle,
    Running {
        worker: JoinHandle<ShutdownResult>,
        exit_code: i32,
    },
    Complete,
}

impl AppShutdown {
    fn is_complete(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| matches!(*state, ShutdownState::Complete))
    }

    fn is_finished(&self) -> bool {
        self.state.lock().is_ok_and(|state| match &*state {
            ShutdownState::Running { worker, .. } => worker.is_finished(),
            _ => false,
        })
    }

    fn start(
        &self,
        exit_code: i32,
        shutdown: impl FnOnce() -> ShutdownResult + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) -> ShutdownResult {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "app_shutdown_state_poisoned")?;
        if !matches!(*state, ShutdownState::Idle) {
            return Ok(());
        }
        let worker = thread::Builder::new()
            .name("vnc-app-shutdown".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(shutdown))
                    .unwrap_or(Err("app_shutdown_worker_panicked"));
                wake();
                result
            })
            .map_err(|_| "app_shutdown_spawn_failed")?;
        *state = ShutdownState::Running { worker, exit_code };
        Ok(())
    }

    // The main-thread completion callback runs after service cleanup. Joining
    // also proves the coordinator itself has returned before allowing exit.
    fn finish(&self) -> Result<Option<i32>, &'static str> {
        let running = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "app_shutdown_state_poisoned")?;
            if !matches!(*state, ShutdownState::Running { .. }) {
                return Ok(None);
            }
            std::mem::take(&mut *state)
        };
        let ShutdownState::Running { worker, exit_code } = running else {
            unreachable!()
        };
        worker
            .join()
            .map_err(|_| "app_shutdown_worker_panicked")??;
        *self
            .state
            .lock()
            .map_err(|_| "app_shutdown_state_poisoned")? = ShutdownState::Complete;
        Ok(Some(exit_code))
    }
}

fn begin_shutdown(app: &AppHandle) {
    app.state::<RemoteSessionManager>().begin_shutdown();
}

fn shutdown_services(app: &AppHandle) -> ShutdownResult {
    let clients = app.state::<RemoteSessionManager>();
    // Background/service sharing has independent ownership; App exit closes viewers.
    clients
        .request_stop_all()
        .map_err(|_| "app_shutdown_client_cancel_failed")?;
    loop {
        let client_result = clients.shutdown();
        if client_result.is_ok() {
            return Ok(());
        }
        if client_result
            .as_ref()
            .is_err_and(|error| error != "rfb_shutdown_pending")
        {
            return Err("app_shutdown_client_failed");
        }
        // OS DNS cannot be interrupted by std. Retain its owner and the exit
        // gate until the existing worker finishes; do not overlap a new job.
        thread::sleep(Duration::from_millis(50));
    }
}

fn report_failure(app: &AppHandle, code: &'static str) {
    if let Some(logger) = app.try_state::<app_log::AppLogger>() {
        logger.submit(
            app_log::LogLevel::Error,
            "app",
            "app_shutdown_failed",
            "Application shutdown did not complete.",
            serde_json::json!({ "code": code }),
        );
    }
}

fn finish_requested_shutdown(app: &AppHandle) {
    match app.state::<AppShutdown>().finish() {
        Ok(Some(code)) => app.exit(code),
        Ok(None) => {}
        Err(code) => report_failure(app, code),
    }
}

/// Handles normal quit without blocking the UI on socket/capture worker joins.
pub(crate) fn handle_event(app: &AppHandle, event: RunEvent) {
    let shutdown = app.state::<AppShutdown>();
    match event {
        RunEvent::ExitRequested { api, code, .. } => {
            begin_shutdown(app);
            if shutdown.is_complete() {
                return;
            }
            // Tauri cannot prevent restart. Its Exit event uses the fallback.
            if code == Some(tauri::RESTART_EXIT_CODE) {
                return;
            }
            api.prevent_exit();
            let worker_app = app.clone();
            let wake_app = app.clone();
            if let Err(code) = shutdown.start(
                code.unwrap_or(0),
                move || shutdown_services(&worker_app),
                move || {
                    let completion_app = wake_app.clone();
                    let _ = wake_app
                        .run_on_main_thread(move || finish_requested_shutdown(&completion_app));
                },
            ) {
                report_failure(app, code);
            }
        }
        RunEvent::MainEventsCleared if shutdown.is_finished() => finish_requested_shutdown(app),
        RunEvent::Exit if !shutdown.is_complete() => {
            begin_shutdown(app);
            // Some platform/system exits cannot be prevented. Reap any current
            // coordinator before performing the final service cleanup fallback.
            let _ = shutdown.finish();
            if !shutdown.is_complete() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    shutdown_services(app)
                }))
                .unwrap_or(Err("app_shutdown_worker_panicked"));
                if let Err(code) = result {
                    report_failure(app, code);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "../tests/support/app_shutdown_tests.rs"]
mod tests;
