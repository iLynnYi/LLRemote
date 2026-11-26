pub mod agent;
pub mod background_share;
pub mod files;
pub mod hosts;
pub mod remote_session;
pub mod windows_firewall;
pub mod windows_service;

use crate::{
    app_log::{AppLogger, FrontendLogEvent, LogLevel},
    tray,
};
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn log_connection_model(
    logger: tauri::State<'_, AppLogger>,
    message: String,
    target: Option<String>,
    attempt_id: Option<String>,
) {
    logger.submit(
        LogLevel::Debug,
        "connection.model",
        "connection_model_connected",
        &message,
        serde_json::json!({ "target": target, "attempt_id": attempt_id }),
    );
}

#[tauri::command]
pub fn append_frontend_log_batch(
    logger: tauri::State<'_, AppLogger>,
    events: Vec<FrontendLogEvent>,
) {
    logger.frontend_batch(events);
}

#[tauri::command]
pub fn open_log_directory(logger: tauri::State<'_, AppLogger>) -> Result<(), String> {
    logger.open_log_directory()
}

#[tauri::command]
pub fn update_tray_state(app: AppHandle, snapshot: tray::TrayStateSnapshot) -> Result<(), String> {
    tray::update_tray(&app, snapshot)
}

#[tauri::command]
pub fn focus_session_external_window(app: AppHandle, label: String) -> Result<(), String> {
    let Some(window) = app.get_webview_window(&label) else {
        return Err(format!("session_external_window_not_found:{label}"));
    };

    window
        .show()
        .map_err(|error| format!("session_external_window_show_failed:{error}"))?;
    window
        .unminimize()
        .map_err(|error| format!("session_external_window_unminimize_failed:{error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("session_external_window_focus_failed:{error}"))?;
    Ok(())
}

#[tauri::command]
pub fn report_frontend_startup_failure(app: AppHandle) {
    use tauri_plugin_dialog::DialogExt;

    if let Some(logger) = app.try_state::<AppLogger>() {
        logger.submit(
            LogLevel::Error,
            "app",
            "frontend_startup_failed",
            "The application interface could not start.",
            serde_json::json!({}),
        );
    }
    let exit = app.clone();
    app.dialog()
        .message("无法启动应用界面。请重新打开 LL Remote，若仍失败请检查应用日志。")
        .title("LL Remote 无法启动")
        .kind(tauri_plugin_dialog::MessageDialogKind::Error)
        .show(move |_| exit.exit(1));
}
