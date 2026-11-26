//! Successful OS checks remember the signed host version, never an assumed grant.
use super::paths;
use crate::local_vnc_server::{
    LocalVncServerPermission, LocalVncServerPermissionKind as Kind,
    LocalVncServerPermissionStatus as Status,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionIdentity {
    pub identifier: String,
    pub cdhash: String,
}

pub(super) fn current() -> Option<PermissionIdentity> {
    unsafe extern "C" {
        fn vnc_sharing_host_identity(
            identifier: *mut u8,
            identifier_size: usize,
            hash: *mut u8,
            hash_size: usize,
        ) -> bool;
    }
    let mut identifier = [0_u8; 256];
    let mut hash = [0_u8; 129];
    if !unsafe {
        vnc_sharing_host_identity(
            identifier.as_mut_ptr(),
            identifier.len(),
            hash.as_mut_ptr(),
            hash.len(),
        )
    } {
        return None;
    }
    let string = |bytes: &[u8]| {
        std::ffi::CStr::from_bytes_until_nul(bytes)
            .ok()?
            .to_str()
            .ok()
            .map(str::to_owned)
    };
    Some(PermissionIdentity {
        identifier: string(&identifier)?,
        cdhash: string(&hash)?,
    })
}

#[derive(Default, Serialize, Deserialize)]
struct Grants {
    screen_recording: Option<PermissionIdentity>,
    accessibility: Option<PermissionIdentity>,
}

pub(super) struct PermissionHistory {
    path: Option<PathBuf>,
    grants: Grants,
}

impl PermissionHistory {
    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            path: None,
            grants: Grants::default(),
        }
    }
    pub fn load() -> Self {
        let path = paths::data_directory()
            .ok()
            .map(|directory| directory.join("permission-identities.json"));
        let grants = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, grants }
    }

    pub fn signature_changed(
        &self,
        identity: Option<&PermissionIdentity>,
        permissions: &[LocalVncServerPermission],
    ) -> Vec<Kind> {
        let Some(identity) = identity else {
            return vec![];
        };
        permissions
            .iter()
            .filter(|permission| permission.status == Status::Missing)
            .filter(|permission| {
                self.grant(&permission.kind)
                    .is_some_and(|previous| previous != identity)
            })
            .map(|permission| permission.kind.clone())
            .collect()
    }

    fn grant(&self, kind: &Kind) -> Option<&PermissionIdentity> {
        match kind {
            Kind::ScreenRecording => self.grants.screen_recording.as_ref(),
            Kind::Accessibility => self.grants.accessibility.as_ref(),
        }
    }

    /// Missing/revoked results preserve history. Only a fresh successful OS check advances it.
    pub fn record(
        &mut self,
        identity: Option<&PermissionIdentity>,
        permissions: &[LocalVncServerPermission],
    ) {
        let Some(identity) = identity else {
            return;
        };
        let mut changed = false;
        for permission in permissions
            .iter()
            .filter(|permission| permission.status == Status::Granted)
        {
            let slot = match permission.kind {
                Kind::ScreenRecording => &mut self.grants.screen_recording,
                Kind::Accessibility => &mut self.grants.accessibility,
            };
            if slot.as_ref() != Some(identity) {
                *slot = Some(identity.clone());
                changed = true;
            }
        }
        if changed {
            let _ = self.save();
        } // History is advisory; failure never changes the OS result.
    }

    fn save(&self) -> Result<(), String> {
        let path = self
            .path
            .as_ref()
            .ok_or("background_share_config_unavailable")?;
        paths::ensure_private_directory(
            path.parent().ok_or("background_share_config_unavailable")?,
        )?;
        let bytes =
            serde_json::to_vec(&self.grants).map_err(|_| "background_share_config_unavailable")?;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let next = path.with_extension("json.next");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(0x100)
            .open(&next)
            .map_err(|_| "background_share_config_unavailable")?; // Darwin O_NOFOLLOW.
        file.write_all(&bytes)
            .map_err(|_| "background_share_config_unavailable")?;
        paths::replace_file(&next, path)
    }
}

/// Reset only one named resource for one bundle. Never reset All or all applications.
fn reset_command(kind: &Kind, identifier: &str) -> Result<std::process::Command, String> {
    let service = match kind {
        Kind::ScreenRecording => "ScreenCapture",
        Kind::Accessibility => "Accessibility",
    };
    let mut command = std::process::Command::new("/usr/bin/tccutil");
    command
        .args(["reset", service, identifier])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    Ok(command)
}

fn reset_identifiers<'a>(kind: &Kind, helper: &'a str, owner: Option<&'a str>) -> Vec<&'a str> {
    let mut identifiers = vec![helper];
    // macOS may record ScreenCapture under the containing app while the helper
    // performs the capture. Clear both possible owners of the stale grant.
    if *kind == Kind::ScreenRecording {
        if let Some(owner) = owner.filter(|owner| *owner != helper) {
            identifiers.push(owner);
        }
    }
    identifiers
}

pub(super) fn reset_for_host(kind: &Kind, helper: &str, owner: Option<&str>) -> Result<(), String> {
    reset_for_host_with(kind, helper, owner, reset)
}

fn reset_for_host_with(
    kind: &Kind,
    helper: &str,
    owner: Option<&str>,
    mut reset_one: impl FnMut(&Kind, &str) -> Result<(), String>,
) -> Result<(), String> {
    for identifier in reset_identifiers(kind, helper, owner) {
        reset_one(kind, identifier)?;
    }
    Ok(())
}

fn reset(kind: &Kind, identifier: &str) -> Result<(), String> {
    let failed = || "background_share_permission_reset_failed".to_string();
    let mut child = reset_command(kind, identifier)?
        .spawn()
        .map_err(|_| failed())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(failed())
                }
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failed());
            }
        }
    }
}

pub(super) fn register_bundle(executable: &std::path::Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn vnc_sharing_host_register_bundle(path: *const std::ffi::c_char) -> bool;
    }
    if !super::bundle_macos::is_app_executable(executable) {
        return Err("background_share_permission_identity_unavailable".into());
    }
    let bundle = executable
        .ancestors()
        .nth(3)
        .ok_or("background_share_permission_identity_unavailable")?;
    let path = std::ffi::CString::new(bundle.as_os_str().as_bytes())
        .map_err(|_| "background_share_permission_identity_unavailable")?;
    if unsafe { vnc_sharing_host_register_bundle(path.as_ptr()) } {
        Ok(())
    } else {
        Err("background_share_permission_reset_failed".into())
    }
}

/// The caller holds host control. Recheck first, stop an idle host, reset one
/// resource, and restore the host even on reset failure. This never starts sharing.
pub(super) fn repair_with_actions(
    overview: &super::types::BackgroundShareOverview,
    kind: &Kind,
    expected_identifier: &str,
    stop: impl FnOnce() -> Result<(), String>,
    reset: impl FnOnce(&Kind, &str) -> Result<(), String>,
    restore: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if overview
        .server
        .permissions
        .iter()
        .any(|permission| permission.kind == *kind && permission.status == Status::Granted)
    {
        return Ok(());
    }
    if overview.server.status != crate::local_vnc_server::LocalVncServerStatus::Stopped {
        return Err("background_share_permission_repair_busy".into());
    }
    if !overview
        .server
        .permissions
        .iter()
        .any(|permission| permission.kind == *kind && permission.status == Status::Missing)
    {
        return Err("background_share_permission_identity_unavailable".into());
    }
    let identity = overview
        .permission_identity
        .as_ref()
        .filter(|identity| identity.identifier == expected_identifier)
        .ok_or("background_share_permission_identity_unavailable")?;
    stop()?;
    let result = reset(kind, &identity.identifier);
    let restored = restore();
    result?;
    restored
}

#[cfg(test)]
#[path = "../../tests/support/background_share_permission_identity_tests.rs"]
mod tests;
