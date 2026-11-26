use crate::{app_log, local_agent_service};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAgentRequest {
    pub host: String,
    pub port: u16,
    #[serde(flatten)]
    pub message: serde_json::Map<String, serde_json::Value>,
}

#[tauri::command]
pub async fn invoke_remote_agent(request: RemoteAgentRequest) -> Result<serde_json::Value, String> {
    // Admit before dispatching so a slow peer cannot fill the blocking task queue.
    let permit = local_agent_service::admit_agent_request()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        local_agent_service::invoke_remote_agent(
            &request.host,
            request.port,
            serde_json::Value::Object(request.message),
        )
    })
    .await
    .map_err(|_| "agent_request_worker_failed".to_string())?
}

#[tauri::command]
pub async fn upload_remote_agent_file(
    app: AppHandle,
    request: local_agent_service::RemoteAgentFileUploadRequest,
) -> Result<local_agent_service::AgentFileUploadResult, String> {
    let logger = app
        .try_state::<app_log::AppLogger>()
        .map(|state| state.inner().clone());
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let mut transfer_started: Option<Instant> = None;
        let mut verification_started: Option<Instant> = None;
        let mut first_chunk_reported = false;
        let mut initial_offset = 0;
        let mut total_bytes = 0;
        let mut transfer_id = None;
        let result = local_agent_service::upload_agent_file_with_progress(
            local_agent_service::AgentFileUploadRequest {
                host: request.host,
                port: request.port,
                session_id: request.session_id,
                file_path: request.file_path,
                relative_path: request.relative_path,
                transfer_id: request.transfer_id,
                target_directory: request.target_directory,
                rename_policy: request.rename_policy,
            },
            |progress| {
                total_bytes = progress.total_bytes;
                if transfer_id.is_none() {
                    transfer_id = Some(progress.transfer_id.clone());
                }
                match progress.phase {
                    local_agent_service::AgentFileUploadPhase::Preparing => {}
                    local_agent_service::AgentFileUploadPhase::Transferring => {
                        if let Some(transfer_started_at) = transfer_started {
                            if !first_chunk_reported && progress.transferred_bytes > initial_offset
                            {
                                log_upload_timing(
                                    &logger,
                                    &progress.transfer_id,
                                    "first_chunk_accepted",
                                    transfer_started_at.elapsed(),
                                    progress.total_bytes,
                                );
                                first_chunk_reported = true;
                            }
                        } else {
                            log_upload_timing(
                                &logger,
                                &progress.transfer_id,
                                "preparation_finished",
                                started.elapsed(),
                                progress.total_bytes,
                            );
                            initial_offset = progress.transferred_bytes;
                            transfer_started = Some(Instant::now());
                        }
                    }
                    local_agent_service::AgentFileUploadPhase::Verifying => {
                        if let Some(transfer_started_at) = transfer_started {
                            log_upload_timing(
                                &logger,
                                &progress.transfer_id,
                                "data_sent",
                                transfer_started_at.elapsed(),
                                progress.total_bytes,
                            );
                        }
                        verification_started = Some(Instant::now());
                    }
                }
                let _ = app.emit("agent-file-upload-progress", progress);
            },
        );
        if let Some(verification_started_at) = verification_started {
            if let Some(logger) = &logger {
                logger.submit(
                    app_log::LogLevel::Debug,
                    "file_transfer",
                    "upload_phase_timing",
                    "Upload verification finished.",
                    serde_json::json!({
                        "stage": "verification_finished",
                        "elapsed_ms": verification_started_at.elapsed().as_millis(),
                        "transfer_id": transfer_id,
                        "total_bytes": total_bytes,
                        "success": result.is_ok(),
                    }),
                );
            }
        }
        result
    })
    .await
    .map_err(|error| format!("agent_file_upload_worker_failed: {error}"))?
}

fn log_upload_timing(
    logger: &Option<app_log::AppLogger>,
    transfer_id: &str,
    stage: &str,
    elapsed: std::time::Duration,
    total_bytes: u64,
) {
    if let Some(logger) = logger {
        logger.submit(
            app_log::LogLevel::Debug,
            "file_transfer",
            "upload_phase_timing",
            "Upload phase completed.",
            serde_json::json!({ "transfer_id": transfer_id, "stage": stage, "elapsed_ms": elapsed.as_millis(), "total_bytes": total_bytes }),
        );
    }
}
