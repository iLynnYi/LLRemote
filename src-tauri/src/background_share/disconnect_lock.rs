//! The controlled user's policy is shared by their App, background host and Agent.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Settings {
    enabled: bool,
}

pub(crate) fn enabled() -> Result<bool, String> {
    read_from(&super::paths::data_directory()?)
}

fn read_from(directory: &Path) -> Result<bool, String> {
    let path = directory.join("disconnect-lock.json");
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() <= 4096 => serde_json::from_slice::<Settings>(&bytes)
            .map(|settings| settings.enabled)
            .map_err(|_| "disconnect_lock_settings_invalid".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        _ => Err("disconnect_lock_settings_unavailable".into()),
    }
}

pub(crate) fn set_enabled(enabled: bool) -> Result<bool, String> {
    save_in(&super::paths::data_directory()?, enabled)?;
    Ok(enabled)
}

fn save_in(directory: &Path, enabled: bool) -> Result<(), String> {
    super::paths::ensure_private_directory(directory)?;
    let temporary = directory.join(format!("disconnect-lock-{}.next", uuid::Uuid::new_v4()));
    let result = (|| {
        let bytes = serde_json::to_vec(&Settings { enabled })
            .map_err(|_| "disconnect_lock_settings_invalid".to_string())?;
        std::fs::write(&temporary, bytes)
            .map_err(|_| "disconnect_lock_settings_unavailable".to_string())?;
        super::paths::replace_file(&temporary, &directory.join("disconnect-lock.json"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

/// Only explicit end notifications and local Disconnect actions call this.
/// Transport errors, listener shutdown and connection guard drops do not.
pub(crate) fn session_ended() -> Result<(), String> {
    apply_policy(enabled()?, crate::local_vnc_server_platform::lock_screen)
}

fn apply_policy(enabled: bool, lock: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    if enabled {
        lock()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(std::path::PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("llremote-disconnect-lock-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn preference_is_persisted_for_other_user_processes_and_defaults_off() {
        let directory = TestDirectory::new();
        assert!(!read_from(&directory.0).unwrap());
        save_in(&directory.0, true).unwrap();
        assert!(read_from(&directory.0).unwrap());
        save_in(&directory.0, false).unwrap();
        assert!(!read_from(&directory.0).unwrap());
    }

    #[test]
    fn invalid_saved_policy_is_reported_instead_of_silently_disabled() {
        let directory = TestDirectory::new();
        std::fs::write(
            directory.0.join("disconnect-lock.json"),
            b"{\"enabled\":\"false\"}",
        )
        .unwrap();
        assert_eq!(
            read_from(&directory.0).unwrap_err(),
            "disconnect_lock_settings_invalid"
        );
    }

    #[test]
    fn controlled_user_policy_is_the_only_decision_to_lock() {
        apply_policy(false, || panic!("disabled policy must not lock")).unwrap();
        let mut calls = 0;
        apply_policy(true, || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(
            apply_policy(true, || Err("permission_missing".into())).unwrap_err(),
            "permission_missing"
        );
    }
}
