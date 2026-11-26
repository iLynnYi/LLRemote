use crate::remote_session;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn start_remote_session(
    app: AppHandle,
    manager: State<'_, remote_session::RemoteSessionManager>,
    request: remote_session::RemoteSessionStartRequest,
) -> Result<remote_session::RemoteSessionSnapshot, String> {
    let snapshot = manager.start(request)?;
    emit_remote_session_status(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn update_remote_session(
    app: AppHandle,
    manager: State<'_, remote_session::RemoteSessionManager>,
    request: remote_session::RemoteSessionUpdateRequest,
) -> Result<remote_session::RemoteSessionSnapshot, String> {
    let snapshot = manager.update(request)?;
    emit_remote_session_status(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn connect_remote_session_rfb(
    app: AppHandle,
    request: remote_session::RemoteSessionRfbConnectRequest,
) -> Result<(), remote_session::RfbFailure> {
    let session_id = request.session_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let event_app = app.clone();
        app.state::<remote_session::RemoteSessionManager>()
            .connect_rfb(request, move |event| {
                let _ = event_app.emit("remote-session-rfb-event", &event);
                let _ = event_app.emit(&format!("remote-session-rfb-event:{session_id}"), &event);
            })
    })
    .await
    .map_err(|_| remote_session::RfbFailure::from_internal("rfb_connect_dispatch_failed"))?
    .map_err(|error| remote_session::RfbFailure::from_internal(&error))
}

#[tauri::command]
pub fn send_remote_session_rfb_input(
    manager: State<'_, remote_session::RemoteSessionManager>,
    request: remote_session::RemoteSessionRfbInputRequest,
) -> Result<(), remote_session::RfbFailure> {
    manager
        .send_rfb_input(request)
        .map_err(|error| remote_session::RfbFailure::from_internal(&error))
}

#[tauri::command]
pub async fn disconnect_remote_session_rfb(
    app: AppHandle,
    session_id: String,
) -> Result<(), remote_session::RfbFailure> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<remote_session::RemoteSessionManager>()
            .disconnect_rfb(&session_id)
    })
    .await
    .map_err(|_| remote_session::RfbFailure::from_internal("rfb_disconnect_dispatch_failed"))?
    .map_err(|error| remote_session::RfbFailure::from_internal(&error))
}

#[tauri::command]
pub async fn stop_remote_session(
    app: AppHandle,
    session_id: String,
) -> Result<(), remote_session::RfbFailure> {
    let worker_app = app.clone();
    let worker_session_id = session_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        worker_app
            .state::<remote_session::RemoteSessionManager>()
            .stop(&worker_session_id)
    })
    .await
    .map_err(|_| remote_session::RfbFailure::from_internal("rfb_stop_dispatch_failed"))?
    .map_err(|error| remote_session::RfbFailure::from_internal(&error))?;
    let _ = app.emit("remote-session-status", session_id.clone());
    let _ = app.emit(&format!("remote-session-status:{session_id}"), "stopped");
    Ok(())
}

#[tauri::command]
pub fn attach_session_surface(
    app: AppHandle,
    manager: State<'_, remote_session::RemoteSessionManager>,
    request: remote_session::SessionSurfaceAttachRequest,
) -> Result<remote_session::SessionSurfaceSnapshot, String> {
    let session_id = request.session_id.clone();
    let surface = manager.attach_surface(request)?;
    if let Ok(Some(snapshot)) = manager.get_snapshot(&session_id) {
        emit_remote_session_surface(&app, &snapshot);
    }
    Ok(surface)
}

#[tauri::command]
pub fn detach_session_surface(
    app: AppHandle,
    manager: State<'_, remote_session::RemoteSessionManager>,
    session_id: String,
    surface_id: String,
) -> Result<remote_session::RemoteSessionSnapshot, String> {
    let snapshot = manager.detach_surface(&session_id, &surface_id)?;
    emit_remote_session_surface(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn set_active_session_surface(
    app: AppHandle,
    manager: State<'_, remote_session::RemoteSessionManager>,
    session_id: String,
    surface_id: String,
) -> Result<remote_session::RemoteSessionSnapshot, String> {
    let snapshot = manager.set_active_surface(&session_id, &surface_id)?;
    emit_remote_session_surface(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn get_remote_session_snapshot(
    manager: State<'_, remote_session::RemoteSessionManager>,
    session_id: String,
) -> Result<Option<remote_session::RemoteSessionSnapshot>, String> {
    manager.get_snapshot(&session_id)
}

#[tauri::command]
pub fn list_remote_session_snapshots(
    manager: State<'_, remote_session::RemoteSessionManager>,
) -> Result<Vec<remote_session::RemoteSessionSnapshot>, String> {
    manager.list_snapshots()
}

fn emit_remote_session_status(app: &AppHandle, snapshot: &remote_session::RemoteSessionSnapshot) {
    let _ = app.emit("remote-session-status", snapshot);
    let _ = app.emit(
        &format!("remote-session-status:{}", snapshot.session_id),
        snapshot,
    );
}

fn emit_remote_session_surface(app: &AppHandle, snapshot: &remote_session::RemoteSessionSnapshot) {
    let _ = app.emit("remote-session-surface", snapshot);
    let _ = app.emit(
        &format!("remote-session-surface:{}", snapshot.session_id),
        snapshot,
    );
}
