use crate::windows_firewall::{self, FirewallOverview};

#[tauri::command]
pub async fn inspect_windows_firewall() -> Result<FirewallOverview, String> {
    tauri::async_runtime::spawn_blocking(windows_firewall::inspect)
        .await
        .map_err(|_| "windows_firewall_inspect_failed".to_string())?
}

#[tauri::command]
pub async fn open_windows_firewall_settings() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(windows_firewall::open_settings)
        .await
        .map_err(|_| "windows_firewall_settings_failed".to_string())?
}
