//! Keeps delegated password changes separate from administrator service configuration.

const MAX_MANAGERS: usize = 64;

pub(super) fn valid_user_sid(value: &str) -> bool {
    let mut parts = value.split('-');
    if parts.next() != Some("S") || parts.next() != Some("1") {
        return false;
    }
    let Some(authority) = parts.next() else {
        return false;
    };
    if !canonical_decimal(authority)
        || authority
            .parse::<u64>()
            .map_or(true, |value| value >= 1 << 48)
    {
        return false;
    }
    let subauthorities = parts.collect::<Vec<_>>();
    !subauthorities.is_empty()
        && subauthorities.len() <= 15
        && subauthorities
            .iter()
            .all(|part| canonical_decimal(part) && part.parse::<u32>().is_ok())
}

fn canonical_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

pub(super) fn validate_roles(roles: &[String]) -> Result<(), String> {
    if roles.len() > MAX_MANAGERS
        || roles
            .iter()
            .enumerate()
            .any(|(index, sid)| !valid_user_sid(sid) || roles[..index].contains(sid))
    {
        return Err("windows_service_password_management_authorization_failed".into());
    }
    Ok(())
}

pub(super) fn add_user(roles: &mut Vec<String>, sid: &str) -> Result<(), String> {
    validate_roles(roles)?;
    if !valid_user_sid(sid)
        || (roles.len() == MAX_MANAGERS && !roles.iter().any(|role| role == sid))
    {
        return Err("windows_service_password_management_authorization_failed".into());
    }
    if !roles.iter().any(|role| role == sid) {
        roles.push(sid.into());
    }
    Ok(())
}

pub(super) fn can_manage_password(elevated: bool, trusted_sid: &str, roles: &[String]) -> bool {
    elevated
        || (valid_user_sid(trusted_sid)
            && validate_roles(roles).is_ok()
            && roles.iter().any(|sid| sid == trusted_sid))
}

pub(super) fn validate_change_context(
    configuration_only: bool,
    configured: bool,
) -> Result<(), String> {
    if !configuration_only {
        return Err("windows_service_invalid_config".into());
    }
    if !configured {
        return Err("windows_service_credential_missing".into());
    }
    Ok(())
}

pub(super) fn validate_configuration_instance(
    expected_pid: u32,
    current_pid: u32,
    configuration_only: bool,
) -> Result<(), String> {
    if expected_pid == current_pid && configuration_only {
        Ok(())
    } else {
        Err("windows_service_stop_failed".into())
    }
}

pub(super) trait PasswordChangeOperations {
    fn ensure_authorized(&mut self) -> Result<(), String>;
    fn is_running(&mut self) -> Result<bool, String>;
    fn start_configuration_only(&mut self) -> Result<(), String>;
    fn password_management_supported(&mut self) -> Result<bool, String>;
    fn change_password(&mut self) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
}

pub(super) fn change_password(
    operations: &mut impl PasswordChangeOperations,
) -> Result<(), String> {
    operations.ensure_authorized()?;
    if operations.is_running()? {
        return Err("windows_service_invalid_config".into());
    }
    let changed = operations
        .start_configuration_only()
        .and_then(|()| operations.password_management_supported())
        .and_then(ensure_supported)
        .and_then(|()| operations.change_password());
    operations.stop()?;
    changed
}

pub(super) fn owns_live_service(
    started: bool,
    owned_pid: Option<u32>,
    live_pid: Option<u32>,
) -> Result<bool, String> {
    if !started {
        return Ok(false);
    }
    if live_pid.is_some() && live_pid != owned_pid {
        return Err("windows_service_stop_failed".into());
    }
    Ok(true)
}

pub(super) fn bind_running_instance(started: bool, running: bool, pid: u32) -> Result<u32, String> {
    if started && running && pid != 0 {
        Ok(pid)
    } else {
        Err("windows_service_start_failed".into())
    }
}

pub(super) fn ensure_supported(supported: bool) -> Result<(), String> {
    if supported {
        Ok(())
    } else {
        Err("windows_service_password_management_unsupported".into())
    }
}

#[cfg(target_os = "windows")]
pub(super) fn authorized(elevated: bool, trusted_sid: &str) -> Result<bool, String> {
    if elevated {
        return Ok(true);
    }
    Ok(can_manage_password(false, trusted_sid, &storage::read()?))
}

#[cfg(target_os = "windows")]
pub(super) fn current_user_authorized() -> Result<bool, String> {
    authorized(
        super::native::is_elevated(),
        &super::native::current_user_sid()?,
    )
}

#[cfg(target_os = "windows")]
pub(super) use storage::{grant, remove};

#[cfg(target_os = "windows")]
pub(super) fn read_roles() -> Result<Vec<String>, String> {
    storage::read()
}

#[cfg(target_os = "windows")]
mod storage {
    use super::*;
    use crate::windows_service::native;
    use serde::{Deserialize, Serialize};
    use std::io::Write;

    const ROLE_FILE: &str = "password-managers.json";

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Roles {
        user_sids: Vec<String>,
    }

    fn failure() -> String {
        "windows_service_password_management_authorization_failed".into()
    }

    pub(super) fn read() -> Result<Vec<String>, String> {
        let directory = native::data_directory()?;
        native::reject_reparse_chain(&directory).map_err(|_| failure())?;
        native::verify_protected_metadata(&directory).map_err(|_| failure())?;
        let path = directory.join(ROLE_FILE);
        native::reject_reparse_chain(&path).map_err(|_| failure())?;
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Ok(metadata) if metadata.is_file() && metadata.len() <= 16 * 1024 => {}
            _ => return Err(failure()),
        }
        // Do not repair and then trust an attacker-precreated child or its contents.
        native::verify_protected_metadata(&path).map_err(|_| failure())?;
        let bytes = std::fs::read(path).map_err(|_| failure())?;
        if bytes.len() > 16 * 1024 {
            return Err(failure());
        }
        let roles: Roles = serde_json::from_slice(&bytes).map_err(|_| failure())?;
        validate_roles(&roles.user_sids)?;
        Ok(roles.user_sids)
    }

    pub(crate) fn grant(user_sid: &str) -> Result<(), String> {
        if !native::is_elevated() {
            return Err("windows_service_admin_required".into());
        }
        let _role_gate = native::role_lock()?;
        let mut roles = read()?;
        add_user(&mut roles, user_sid)?;
        let directory = native::data_directory()?;
        let path = directory.join(ROLE_FILE);
        let temporary = directory.join(format!(
            "password-managers-{}.next.json",
            uuid::Uuid::new_v4()
        ));
        native::reject_reparse_chain(&temporary).map_err(|_| failure())?;
        let bytes = serde_json::to_vec(&Roles { user_sids: roles }).map_err(|_| failure())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| failure())?;
        let result = (|| {
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|_| failure())?;
            drop(file);
            native::protect_file(&temporary, true).map_err(|_| failure())?;
            native::verify_protected_metadata(&temporary).map_err(|_| failure())?;
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
                return Err(failure());
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }

    pub(crate) fn remove() -> Result<(), String> {
        let _role_gate = native::role_lock()?;
        let path = native::data_directory()?.join(ROLE_FILE);
        native::reject_reparse_chain(&path)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("windows_service_uninstall_failed".into()),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_password_management_tests.rs"]
mod tests;
