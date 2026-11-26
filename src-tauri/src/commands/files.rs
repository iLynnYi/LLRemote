use crate::{file_access, file_transfer, session_artifacts};

#[tauri::command]
pub async fn start_session_recording_file(
    app: tauri::AppHandle,
    request: session_artifacts::RecordingStartRequest,
) -> Result<session_artifacts::RecordingFileHandle, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        file_access::prepare_builtin_file_access_directory(
            &app.state::<file_access::FileAccessStore>(),
            &request.directory,
        )?;
        app.state::<session_artifacts::SessionRecordingFiles>()
            .start(request)
    })
    .await
    .map_err(|_| "recording_worker_failed".to_string())?
}
#[tauri::command]
pub async fn append_session_recording_chunk(
    app: tauri::AppHandle,
    request: session_artifacts::RecordingChunkRequest,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        app.state::<session_artifacts::SessionRecordingFiles>()
            .append(request)
    })
    .await
    .map_err(|_| "recording_worker_failed".to_string())?
}
#[tauri::command]
pub async fn finish_session_recording_file(
    app: tauri::AppHandle,
    request: session_artifacts::RecordingFinishRequest,
) -> Result<session_artifacts::RecordingSaveResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        app.state::<session_artifacts::SessionRecordingFiles>()
            .finish(request)
    })
    .await
    .map_err(|_| "recording_worker_failed".to_string())?
}
#[tauri::command]
pub async fn inspect_session_recording_space(
    app: tauri::AppHandle,
    request: session_artifacts::RecordingSpaceRequest,
) -> Result<session_artifacts::space::RecordingSpaceStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        app.state::<session_artifacts::SessionRecordingFiles>()
            .space(request)
    })
    .await
    .map_err(|_| "recording_worker_failed".to_string())?
}
#[tauri::command]
pub fn list_session_recording_jobs(
    state: State<'_, session_artifacts::SessionRecordingFiles>,
) -> Result<Vec<session_artifacts::RecordingJob>, String> {
    state.jobs()
}
#[tauri::command]
pub async fn recover_session_recordings(
    app: tauri::AppHandle,
    directory: String,
) -> Result<Vec<session_artifacts::RecordingJob>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<session_artifacts::SessionRecordingFiles>();
        let store = app.state::<file_access::FileAccessStore>();
        let mut directories = state.recovery_directories();
        if !directory.is_empty() {
            directories.push(directory.into());
        }
        directories.sort();
        directories.dedup();
        for directory in directories {
            let path = directory.to_string_lossy();
            if file_access::prepare_builtin_file_access_directory(&store, &path).is_ok() {
                let _ = state.recover_directory(&directory);
            }
        }
        state.jobs()
    })
    .await
    .map_err(|_| "recording_worker_failed".to_string())?
}
#[tauri::command]
pub async fn retry_session_recording_save(
    app: tauri::AppHandle,
    request: session_artifacts::RecordingRetryRequest,
) -> Result<session_artifacts::RecordingSaveResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        if let Some(directory) = request.directory.as_ref() {
            file_access::prepare_builtin_file_access_directory(
                &app.state::<file_access::FileAccessStore>(),
                directory,
            )?;
        }
        app.state::<session_artifacts::SessionRecordingFiles>()
            .retry(request)
    })
    .await
    .map_err(|_| "recording_worker_failed".to_string())?
}
#[tauri::command]
pub async fn relocate_session_recording_file(
    app: tauri::AppHandle,
    request: session_artifacts::RecordingRetryRequest,
) -> Result<session_artifacts::RecordingFileHandle, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        if let Some(directory) = request.directory.as_ref() {
            file_access::prepare_builtin_file_access_directory(
                &app.state::<file_access::FileAccessStore>(),
                directory,
            )?;
        }
        app.state::<session_artifacts::SessionRecordingFiles>()
            .relocate_file(request)
    })
    .await
    .map_err(|_| "recording_worker_failed".to_string())?
}
#[tauri::command]
pub fn release_session_recording_file(
    state: State<'_, session_artifacts::SessionRecordingFiles>,
    recording_id: String,
) -> Result<(), String> {
    state.release(&recording_id)
}
use tauri::State;

#[tauri::command]
pub async fn load_file_transfer_tasks(
    app: tauri::AppHandle,
) -> Result<Vec<serde_json::Value>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::files::transfer_task_store::TransferTaskRepository>()
            .load()
    })
    .await
    .map_err(|error| format!("file_transfer_store_worker_failed: {error}"))?
}

#[tauri::command]
pub async fn apply_file_transfer_task_changes(
    app: tauri::AppHandle,
    upserts: Vec<serde_json::Value>,
    removes: Vec<String>,
) -> Result<(), String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::files::transfer_task_store::TransferTaskRepository>()
            .apply(upserts, removes)
    })
    .await
    .map_err(|error| format!("file_transfer_store_worker_failed: {error}"))?
}

#[tauri::command]
pub fn start_file_transfer_source_scan(
    scans: tauri::State<'_, file_transfer::FileTransferScans>,
    paths: Vec<String>,
) -> Result<String, String> {
    scans.start(paths)
}

#[tauri::command]
pub async fn next_file_transfer_source_page(
    app: tauri::AppHandle,
    scan_id: String,
) -> Result<file_transfer::FileTransferSourcePage, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<file_transfer::FileTransferScans>()
            .next(&scan_id, 200)
    })
    .await
    .map_err(|error| format!("file_transfer_scan_worker_failed: {error}"))?
}

#[tauri::command]
pub fn cancel_file_transfer_source_scan(
    scans: tauri::State<'_, file_transfer::FileTransferScans>,
    scan_id: String,
) -> Result<(), String> {
    scans.cancel(&scan_id)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn select_file_transfer_sources(
    window: tauri::WebviewWindow,
) -> Result<Vec<String>, crate::files::picker_windows::SourcePickerError> {
    crate::files::picker_windows::select(window).await
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn select_file_transfer_sources() -> Result<Vec<String>, String> {
    file_transfer::select_file_transfer_sources()
}

#[tauri::command]
pub fn inspect_session_artifact_directory(
    store: State<'_, file_access::FileAccessStore>,
    directory: String,
) -> session_artifacts::SessionArtifactDirectoryStatus {
    let _ = file_access::prepare_builtin_file_access_directory(&store, &directory);
    session_artifacts::inspect_session_artifact_directory(directory)
}

#[tauri::command]
pub fn write_session_artifact_file(
    store: State<'_, file_access::FileAccessStore>,
    request: session_artifacts::SessionArtifactWriteRequest,
) -> Result<session_artifacts::SessionArtifactWriteResult, String> {
    file_access::prepare_builtin_file_access_directory(&store, &request.directory)?;
    session_artifacts::write_session_artifact_file(request)
}

#[tauri::command]
pub fn list_file_access_directories(
    store: State<'_, file_access::FileAccessStore>,
) -> Result<Vec<file_access::FileAccessDirectoryStatus>, String> {
    store.ensure_available()?;
    Ok(file_access::list_file_access_directory_statuses(&store))
}

#[tauri::command]
pub fn grant_file_access_directory(
    store: State<'_, file_access::FileAccessStore>,
    request: file_access::FileAccessGrantRequest,
) -> Result<file_access::FileAccessDirectory, String> {
    file_access::grant_file_access_directory(&store, request)
}

#[tauri::command]
pub fn revoke_file_access_directory(
    store: State<'_, file_access::FileAccessStore>,
    request: file_access::FileAccessGrantRequest,
) -> Result<bool, String> {
    file_access::revoke_file_access_directory(&store, request)
}

#[tauri::command]
pub fn set_builtin_file_access_directory(
    store: State<'_, file_access::FileAccessStore>,
    request: file_access::FileAccessGrantRequest,
) -> Result<file_access::FileAccessDirectory, String> {
    file_access::set_builtin_file_access_directory(&store, request)
}

#[tauri::command]
pub fn reveal_authorized_path(
    store: State<'_, file_access::FileAccessStore>,
    request: file_access::FileAccessPathRequest,
) -> Result<file_access::FileAccessActionResult, String> {
    file_access::reveal_authorized_path(&store, request)
}

#[tauri::command]
pub fn open_authorized_path(
    store: State<'_, file_access::FileAccessStore>,
    request: file_access::FileAccessPathRequest,
) -> Result<file_access::FileAccessActionResult, String> {
    file_access::open_authorized_path(&store, request)
}
