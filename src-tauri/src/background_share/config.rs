use super::{
    paths,
    types::{BackgroundShareLaunchMode, BackgroundShareStartRequest},
};
use crate::local_vnc_server::{LocalVncServerStartRequest, LocalVncServerViewRequest};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const CREDENTIAL_SERVICE: &str = "LLRemote Background Sharing";
const CREDENTIAL_ACCOUNT: &str = "user-server";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Config {
    pub bind_address: String,
    pub port: u16,
    pub view_only: bool,
    pub auto_start: bool,
    pub credential_ref: Option<String>,
    pub view: Option<LocalVncServerViewRequest>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        Self::load_from(&paths::data_directory()?)
    }

    fn load_from(directory: &std::path::Path) -> Result<Self, String> {
        let path = directory.join("server.json");
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "background_share_config_invalid".to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("background_share_config_unavailable".into()),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_in(&paths::data_directory()?)
    }

    fn save_in(&self, directory: &std::path::Path) -> Result<(), String> {
        paths::ensure_private_directory(directory)?;
        let bytes =
            serde_json::to_vec(self).map_err(|_| "background_share_config_invalid".to_string())?;
        let temporary = directory.join("server.json.next");
        std::fs::write(&temporary, bytes)
            .map_err(|_| "background_share_config_unavailable".to_string())?;
        paths::replace_file(&temporary, &directory.join("server.json"))
    }

    pub fn start_request(&self, password: String) -> LocalVncServerStartRequest {
        LocalVncServerStartRequest {
            bind_address: self.bind_address.clone(),
            port: self.port,
            view_only: self.view_only,
            password,
        }
    }

    pub fn apply_start(&mut self, request: &BackgroundShareStartRequest) {
        self.bind_address = request.bind_address.clone();
        self.port = request.port;
        self.view_only = request.view_only;
    }

    pub fn restores_listener(&self, mode: BackgroundShareLaunchMode) -> bool {
        mode == BackgroundShareLaunchMode::Login && self.auto_start
    }

    pub fn validate_listener(&self, password: &str) -> Result<(), String> {
        if password.is_empty()
            || password.len() > 8
            || self.port == 0
            || self.bind_address.parse::<std::net::IpAddr>().is_err()
        {
            return Err("background_share_config_invalid".into());
        }
        Ok(())
    }
}

fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT)
        .map_err(|_| "background_share_credential_unavailable".into())
}

pub(super) fn read_password(config: &Config) -> Result<Zeroizing<String>, String> {
    if config.credential_ref.as_deref() != Some("system://background-sharing/user-server") {
        return Err("background_share_credential_missing".into());
    }
    credential()?
        .get_password()
        .map(Zeroizing::new)
        .map_err(|error| match error {
            keyring::Error::NoEntry => "background_share_credential_missing".into(),
            _ => "background_share_credential_unavailable".into(),
        })
}

pub(super) struct StartPasswords {
    pub active: Zeroizing<String>,
    pub previous: Option<Zeroizing<String>>,
}

/// Temporary starts require an explicit secret and never inspect the credential store.
pub(super) fn passwords_for_start(
    config: &Config,
    request: &mut BackgroundShareStartRequest,
) -> Result<StartPasswords, String> {
    passwords_for_start_with(config, request, read_password)
}

fn passwords_for_start_with(
    config: &Config,
    request: &mut BackgroundShareStartRequest,
    mut read: impl FnMut(&Config) -> Result<Zeroizing<String>, String>,
) -> Result<StartPasswords, String> {
    let has_new_password = !request.password.is_empty();
    if request.temporary_password && !has_new_password {
        return Err("background_share_config_invalid".into());
    }
    let active = if has_new_password {
        Zeroizing::new(std::mem::take(&mut request.password))
    } else {
        read(config)?
    };
    let mut requested = config.clone();
    requested.apply_start(request);
    requested.validate_listener(&active)?;
    let previous = if has_new_password && !request.temporary_password {
        previous_password_with(config, read)?
    } else {
        None
    };
    Ok(StartPasswords { active, previous })
}

fn previous_password_with(
    config: &Config,
    read: impl FnOnce(&Config) -> Result<Zeroizing<String>, String>,
) -> Result<Option<Zeroizing<String>>, String> {
    if config.credential_ref.is_none() {
        return Ok(None);
    }
    match read(config) {
        Ok(password) => Ok(Some(password)),
        // A deleted entry has no old secret to restore. A new password can
        // repair it; denied/unavailable storage must preserve the old value.
        Err(code) if code == "background_share_credential_missing" => Ok(None),
        Err(code) => Err(code),
    }
}

pub(super) fn save_password(config: &mut Config, password: &str) -> Result<(), String> {
    credential()?
        .set_password(password)
        .map_err(|_| "background_share_credential_unavailable".to_string())?;
    config.credential_ref = Some("system://background-sharing/user-server".into());
    Ok(())
}

pub(super) fn restore_password(previous: Option<&str>) -> Result<(), String> {
    match previous {
        Some(password) => credential()?.set_password(password),
        None => match credential()?.delete_credential() {
            Err(keyring::Error::NoEntry) => Ok(()),
            result => result,
        },
    }
    .map_err(|_| "background_share_credential_unavailable".into())
}

/// Starting with a new password saves it as part of the same settings change.
/// Refresh the existing login registration to the current executable without
/// accepting a stale policy snapshot from the start request.
pub(super) fn persist_start(
    next: &mut Config,
    temporary: bool,
    password: Option<&str>,
    previous_password: Option<&str>,
) -> Result<(), String> {
    persist_start_with(
        next,
        temporary,
        PasswordChange {
            new: password,
            previous: previous_password,
        },
        save_password,
        super::startup::configure,
        Config::save,
        restore_password,
    )
}

struct PasswordChange<'a> {
    new: Option<&'a str>,
    previous: Option<&'a str>,
}

fn persist_start_with(
    next: &mut Config,
    temporary: bool,
    passwords: PasswordChange<'_>,
    save_secret: impl FnOnce(&mut Config, &str) -> Result<(), String>,
    mut configure_startup: impl FnMut(bool) -> Result<(), String>,
    save_config: impl FnOnce(&Config) -> Result<(), String>,
    restore_secret: impl FnOnce(Option<&str>) -> Result<(), String>,
) -> Result<(), String> {
    let PasswordChange {
        new: password,
        previous: previous_password,
    } = passwords;
    if temporary {
        return Ok(());
    }
    let first_credential = password.is_some() && next.credential_ref.is_none();
    let mut secret_saved = false;
    let mut registration_attempted = false;
    let result = (|| {
        if let Some(password) = password {
            save_secret(next, password)?;
            secret_saved = true;
        }
        registration_attempted = true;
        configure_startup(next.auto_start)?;
        save_config(next)
    })();
    if let Err(code) = result {
        let secret_rollback_failed =
            password.is_some() && restore_secret(previous_password).is_err();
        let registration_rollback_failed =
            registration_attempted && configure_startup(next.auto_start).is_err();
        if secret_rollback_failed {
            // A denied first write may also deny cleanup of an entry that was
            // never created. No saved config or login policy was committed.
            if first_credential
                && !secret_saved
                && code == "background_share_credential_unavailable"
            {
                return Err("background_share_initial_credential_rollback_failed".into());
            }
            return Err("background_share_credential_rollback_failed".into());
        }
        if registration_rollback_failed {
            return Err("background_share_startup_rollback_failed".into());
        }
        return Err(code);
    }
    Ok(())
}

pub(super) fn validate_auto_start(config: &Config, enabled: bool) -> Result<(), String> {
    validate_auto_start_with(config, enabled, read_password)
}

fn validate_auto_start_with(
    config: &Config,
    enabled: bool,
    read: impl FnOnce(&Config) -> Result<Zeroizing<String>, String>,
) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }
    let password = read(config).map_err(|code| {
        if code == "background_share_credential_missing" {
            "background_share_auto_start_credential_required".into()
        } else {
            code
        }
    })?;
    config.validate_listener(&password)
}

pub(super) fn persist_auto_start(previous: &Config, next: &Config) -> Result<(), String> {
    persist_auto_start_with(previous, next, super::startup::configure, Config::save)
}

fn persist_auto_start_with(
    previous: &Config,
    next: &Config,
    mut configure_startup: impl FnMut(bool) -> Result<(), String>,
    save_config: impl FnOnce(&Config) -> Result<(), String>,
) -> Result<(), String> {
    let result = configure_startup(next.auto_start).and_then(|_| save_config(next));
    if let Err(code) = result {
        if configure_startup(previous.auto_start).is_err() {
            return Err("background_share_startup_rollback_failed".into());
        }
        return Err(code);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/background_share_config_tests.rs"]
mod tests;
