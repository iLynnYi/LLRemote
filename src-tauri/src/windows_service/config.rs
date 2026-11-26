use super::{native, WindowsServiceConfigureRequest, WindowsServiceOverview};
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ServiceConfig {
    pub bind_address: String,
    pub port: u16,
    pub view_only: bool,
    pub credential_ref: String,
}

impl ServiceConfig {
    pub fn from_request(request: &WindowsServiceConfigureRequest) -> Self {
        Self {
            bind_address: request.bind_address.clone(),
            port: request.port,
            view_only: request.view_only,
            credential_ref: super::credentials::CREDENTIAL_TARGET.into(),
        }
    }
    pub fn apply(&self, overview: &mut WindowsServiceOverview) {
        overview.bind_address = Some(self.bind_address.clone());
        overview.port = Some(self.port);
        overview.view_only = self.view_only;
    }
}

pub(super) fn read() -> Result<Option<ServiceConfig>, String> {
    let path = native::data_directory()?.join("server.json");
    native::reject_reparse(&path)?;
    let data = match std::fs::read(path) {
        Ok(data) if data.len() <= 4096 => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        _ => return Err("windows_service_invalid_config".into()),
    };
    let config: ServiceConfig =
        serde_json::from_slice(&data).map_err(|_| "windows_service_invalid_config".to_string())?;
    if config.bind_address.parse::<std::net::IpAddr>().is_err()
        || config.port == 0
        || config.credential_ref != super::credentials::CREDENTIAL_TARGET
    {
        return Err("windows_service_invalid_config".into());
    }
    Ok(Some(config))
}

pub(super) fn write(config: &ServiceConfig) -> Result<(), String> {
    let directory = native::data_directory()?;
    native::reject_reparse(&directory)?;
    let path = directory.join("server.json");
    let temporary = directory.join(format!("server-{}.next.json", uuid::Uuid::new_v4()));
    native::reject_reparse(&path)?;
    native::reject_reparse(&temporary)?;
    let bytes =
        serde_json::to_vec(config).map_err(|_| "windows_service_configure_failed".to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "windows_service_configure_failed".to_string())?;
    let written = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err("windows_service_configure_failed".into());
    }
    native::protect_file(&temporary, true)?;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    if unsafe {
        MoveFileExW(
            native::wide(&temporary).as_ptr(),
            native::wide(&path).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        let _ = std::fs::remove_file(temporary);
        return Err("windows_service_configure_failed".into());
    }
    Ok(())
}

pub(super) fn remove() -> Result<(), String> {
    let path = native::data_directory()?.join("server.json");
    native::reject_reparse(&path)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("windows_service_uninstall_failed".into()),
    }
}
