use super::user::{self, User};
use crate::windows_service::native;
use std::path::{Path, PathBuf};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};
use windows_core::{Interface, PCWSTR};

pub(super) fn background_target(target: &Path, user: &User) -> bool {
    let Some(parent) = target.parent() else {
        return false;
    };
    let name = target
        .file_name()
        .map(|v| v.to_string_lossy())
        .unwrap_or_default();
    user::same_path(parent, &user.local.join("LLRemote/Sharing/bin"))
        && super::process::background_name(&name)
}

pub(super) fn disable_background_startup(user: &User) -> Result<(), String> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
        .map_err(|_| "windows_uninstall_startup_cleanup_failed")?;
    let _apartment = Apartment;
    let path = user.startup.join("LLRemote Sharing.lnk");
    if target(&path)?.is_some_and(|target| background_target(&target, user)) {
        remove_file(&path)?;
    }
    Ok(())
}

/// App shortcuts are cosmetic after the executable is gone; keep each residue.
pub(super) fn remove(user: &User, directory: &Path, machine: bool) -> super::report::CleanupReport {
    let mut report = super::report::CleanupReport::default();
    if machine {
        return report;
    }
    if unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }.is_err() {
        report.warning("windows_uninstall_shortcut_cleanup_failed", "Windows Shell");
        return report;
    }
    let _apartment = Apartment;
    if let Some(desktop) = &user.desktop {
        let desktop = desktop.join("LL Remote.lnk");
        report.capture(remove_app_shortcut(&desktop, directory), &desktop);
    } else {
        report.warning(
            "windows_uninstall_shortcut_cleanup_failed",
            "User Shell Folders / Desktop",
        );
    }
    let mut pending = if let Some(programs) = &user.programs {
        vec![programs.clone()]
    } else {
        report.warning(
            "windows_uninstall_shortcut_cleanup_failed",
            "User Shell Folders / Programs",
        );
        Vec::new()
    };
    let mut visited = 0;
    while let Some(folder) = pending.pop() {
        visited += 1;
        if visited > 4096 {
            report.warning("windows_uninstall_shortcut_cleanup_failed", &folder);
            break;
        }
        if native::reject_reparse_chain(&folder).is_err() {
            report.warning("windows_uninstall_shortcut_cleanup_failed", &folder);
            continue;
        }
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                report.warning("windows_uninstall_shortcut_cleanup_failed", &folder);
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    report.warning("windows_uninstall_shortcut_cleanup_failed", &folder);
                    continue;
                }
            };
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => {
                    report.warning("windows_uninstall_shortcut_cleanup_failed", entry.path());
                    continue;
                }
            };
            if kind.is_dir() {
                if native::reject_reparse_chain(&entry.path()).is_ok() {
                    pending.push(entry.path());
                }
            } else if entry.file_name() == "LL Remote.lnk" {
                report.capture(remove_app_shortcut(&entry.path(), directory), entry.path());
            }
        }
    }
    report
}

fn remove_app_shortcut(path: &Path, directory: &Path) -> Result<(), String> {
    if target(path)?.is_some_and(|target| user::same_path(&target, &directory.join("LLRemote.exe")))
    {
        remove_file(path)?;
    }
    Ok(())
}

fn target(path: &Path) -> Result<Option<PathBuf>, String> {
    native::reject_reparse_chain(path)?;
    if !path
        .try_exists()
        .map_err(|_| "windows_uninstall_shortcut_cleanup_failed")?
    {
        return Ok(None);
    }
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|_| "windows_uninstall_shortcut_cleanup_failed")?;
    let file: IPersistFile = link
        .cast()
        .map_err(|_| "windows_uninstall_shortcut_cleanup_failed")?;
    unsafe { file.Load(PCWSTR(native::wide(path).as_ptr()), STGM_READ) }
        .map_err(|_| "windows_uninstall_shortcut_cleanup_failed")?;
    let mut value = vec![0u16; 32768];
    unsafe { link.GetPath(&mut value, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32) }
        .map_err(|_| "windows_uninstall_shortcut_cleanup_failed")?;
    let size = value
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(value.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &value[..size],
    ))))
}

fn remove_file(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|_| "windows_uninstall_shortcut_cleanup_failed".into())
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
