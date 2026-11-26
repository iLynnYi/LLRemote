use crate::background_share::{self, BackgroundShareOverview, BackgroundShareStartRequest};

#[tauri::command]
pub async fn get_local_share_disconnect_lock() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(background_share::disconnect_lock::enabled)
        .await
        .map_err(|_| "disconnect_lock_settings_unavailable".to_string())?
}

#[tauri::command]
pub async fn set_local_share_disconnect_lock(enabled: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        background_share::disconnect_lock::set_enabled(enabled)
    })
    .await
    .map_err(|_| "disconnect_lock_settings_unavailable".to_string())?
}

#[tauri::command]
pub async fn prepare_background_share(recover: bool) -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(move || background_share::prepare(recover))
        .await
        .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn get_background_share_status(
    inspect_permissions: Option<bool>,
) -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if cfg!(target_os = "macos") && !inspect_permissions.unwrap_or(false) {
            background_share::runtime_status()
        } else {
            background_share::status()
        }
    })
    .await
    .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn start_background_share(
    request: BackgroundShareStartRequest,
) -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(move || background_share::start(request))
        .await
        .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn stop_background_share() -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(background_share::stop)
        .await
        .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn set_background_share_auto_start(
    enabled: bool,
) -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(move || background_share::set_auto_start(enabled))
        .await
        .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn restart_background_share() -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(background_share::restart)
        .await
        .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn update_background_share_view(
    request: crate::local_vnc_server::LocalVncServerViewRequest,
) -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(move || background_share::update_view(request))
        .await
        .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn disconnect_background_share_connection(
    connection_id: String,
) -> Result<BackgroundShareOverview, String> {
    tauri::async_runtime::spawn_blocking(move || background_share::disconnect(connection_id))
        .await
        .map_err(|_| "background_share_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn request_background_share_permission(
    kind: crate::local_vnc_server::LocalVncServerPermissionKind,
    open_settings: Option<bool>,
) -> Result<crate::local_vnc_server::LocalVncServerPermissionRequestResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if open_settings.unwrap_or(false) {
            crate::local_vnc_server::open_local_vnc_server_permission_settings(kind)
        } else {
            background_share::request_permission(kind)
        }
    })
    .await
    .map_err(|_| "background_share_dispatch_failed".to_string())?
}
