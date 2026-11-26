use crate::windows_service::{
    self, WindowsServiceConfigureRequest, WindowsServiceOverview, WindowsServicePasswordRequest,
    WindowsServiceRepairReport,
};

async fn dispatch(
    action: fn() -> Result<WindowsServiceOverview, String>,
) -> Result<WindowsServiceOverview, String> {
    tauri::async_runtime::spawn_blocking(action)
        .await
        .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn get_windows_service_status() -> Result<WindowsServiceOverview, String> {
    dispatch(windows_service::status).await
}

#[tauri::command]
pub async fn get_windows_service_server_status(
) -> Result<crate::local_vnc_server::LocalVncServerOverview, String> {
    tauri::async_runtime::spawn_blocking(windows_service::server_status)
        .await
        .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn disconnect_windows_service_connection(
    connection_id: String,
) -> Result<crate::local_vnc_server::LocalVncServerOverview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        windows_service::disconnect_connection(connection_id)
    })
    .await
    .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn install_windows_service(
    request: Option<WindowsServiceConfigureRequest>,
    use_background_password: Option<bool>,
) -> Result<WindowsServiceOverview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut request = request;
        if use_background_password.unwrap_or(false) {
            let request = request.as_mut().ok_or("windows_service_invalid_config")?;
            request.password = windows_service::setup_password(&request.password)?;
        }
        windows_service::install(request)
    })
    .await
    .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn uninstall_windows_service() -> Result<WindowsServiceOverview, String> {
    dispatch(windows_service::uninstall).await
}

#[tauri::command]
pub async fn start_windows_service() -> Result<WindowsServiceOverview, String> {
    dispatch(windows_service::start).await
}

#[tauri::command]
pub async fn start_windows_service_with_fallback(
    request: crate::background_share::BackgroundShareStartRequest,
    use_background_password: Option<bool>,
    logger: tauri::State<'_, crate::app_log::AppLogger>,
) -> Result<windows_service::ServiceStartOutcome, String> {
    let logger = logger.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut request = request;
        if use_background_password.unwrap_or(false) {
            request.password = windows_service::setup_password(&request.password)?;
            // Read once for this start and align the service with exactly the
            // same short-lived secret used by a possible temporary handoff.
            windows_service::set_password(WindowsServicePasswordRequest {
                password: request.password.clone(),
            })?;
        }
        windows_service::start_with_fallback(request, logger)
    })
    .await
    .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn stop_windows_service() -> Result<WindowsServiceOverview, String> {
    dispatch(windows_service::stop).await
}

#[tauri::command]
pub async fn enable_windows_service_sas() -> Result<WindowsServiceOverview, String> {
    dispatch(windows_service::enable_sas).await
}

#[tauri::command]
pub async fn repair_windows_service() -> Result<WindowsServiceRepairReport, String> {
    tauri::async_runtime::spawn_blocking(windows_service::repair_sharing)
        .await
        .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn set_windows_service_auto_start(
    enabled: bool,
) -> Result<WindowsServiceOverview, String> {
    tauri::async_runtime::spawn_blocking(move || windows_service::set_auto_start(enabled))
        .await
        .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn configure_windows_service(
    request: WindowsServiceConfigureRequest,
) -> Result<WindowsServiceOverview, String> {
    tauri::async_runtime::spawn_blocking(move || windows_service::configure(request))
        .await
        .map_err(|_| "windows_service_dispatch_failed".to_string())?
}

#[tauri::command]
pub async fn authorize_windows_service_password_management(
) -> Result<WindowsServiceOverview, String> {
    dispatch(windows_service::authorize_password_management).await
}

#[tauri::command]
pub async fn set_windows_service_password(
    password: String,
) -> Result<WindowsServiceOverview, String> {
    let request = WindowsServicePasswordRequest { password };
    tauri::async_runtime::spawn_blocking(move || windows_service::set_password(request))
        .await
        .map_err(|_| "windows_service_dispatch_failed".to_string())?
}
