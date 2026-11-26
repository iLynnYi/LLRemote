use super::identity::{Installation, Plan};
use super::user::{self, User};
use crate::windows_service::native;
use std::io::Write;
use std::path::{Path, PathBuf};
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

pub(super) const OWNER_FILE: &str = ".llremote-install-owner";

pub(super) fn directory(requested: Option<&str>) -> Result<PathBuf, String> {
    let directory = match requested {
        Some(path) => PathBuf::from(path),
        None => std::env::current_exe()
            .map_err(|_| "windows_uninstall_binary_missing")?
            .parent()
            .ok_or("windows_uninstall_binary_missing")?
            .to_owned(),
    };
    if !directory.is_absolute()
        || directory.file_name().is_none()
        || directory
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("windows_uninstall_invalid_arguments".into());
    }
    native::reject_reparse_chain(&directory)?;
    Ok(directory)
}

pub(super) fn record(scope: &str) -> Result<(), String> {
    let directory = directory(None)?;
    let install = match scope {
        "user" => Installation::user(native::current_user_sid()?),
        "all" if native::current_elevation()? => Installation::machine(),
        _ => return Err("windows_uninstall_invalid_arguments".into()),
    };
    let path = directory.join(OWNER_FILE);
    native::reject_reparse_chain(&path)?;
    if path.exists() && read_marker(&directory)? != install {
        return Err("windows_uninstall_owner_mismatch".into());
    }
    // Write before service registration so a partial installation has ownership
    // too. Never silently adopt a different user's installation during repair.
    let pending = directory.join(format!("{OWNER_FILE}.next"));
    native::reject_reparse_chain(&pending)?;
    match std::fs::remove_file(&pending) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("windows_uninstall_owner_write_failed".into()),
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|_| "windows_uninstall_owner_write_failed")?;
    let result = file
        .write_all(install.encode().as_bytes())
        .and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| {
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        if unsafe {
            MoveFileExW(
                native::wide(&pending).as_ptr(),
                native::wide(&path).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    });
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result.map_err(|_| "windows_uninstall_owner_write_failed".to_string())
}

fn read_marker(directory: &Path) -> Result<Installation, String> {
    let path = directory.join(OWNER_FILE);
    native::reject_reparse_chain(&path)?;
    let metadata = std::fs::metadata(&path).map_err(|_| "windows_uninstall_owner_unknown")?;
    if !metadata.is_file() || metadata.len() > 512 {
        return Err("windows_uninstall_owner_unknown".into());
    }
    Installation::parse(
        &std::fs::read_to_string(path).map_err(|_| "windows_uninstall_owner_unknown")?,
    )
}

pub(super) fn plan(directory: &Path, requested: Option<&str>) -> Result<Plan, String> {
    let machine_registered = user::registered_at(HKEY_LOCAL_MACHINE, directory)?;
    let installation = read_marker(directory)?;
    // Machine ownership must be backed by the protected machine registry.
    if installation.user_sid.is_none() != machine_registered {
        return Err("windows_uninstall_owner_mismatch".into());
    }
    installation.plan(
        &native::current_user_sid()?,
        native::current_elevation()?,
        requested,
    )
}

pub(super) fn inspect(
    directory: &Path,
    report: &Path,
    requested: Option<&str>,
) -> Result<(), String> {
    let plan = plan(directory, requested)?;
    let mut service = false;
    // Validate the actual owner's record before mutating anything. If the
    // caller needs elevation, the elevated instance performs that validation.
    if !plan.needs_elevation {
        let user = User::resolve(&plan.user_sid)?;
        validate_registration(directory, &plan, &user)?;
        service = crate::windows_service::validate_app_uninstall(directory)
            .map_err(super::errors::service_error)?;
        user.close()?;
    }
    let label = account_name(&plan.user_sid).replace(['\r', '\n', '[', ']'], " ");
    let contents = format!(
        "[uninstall]\r\nscope={}\r\nsid={}\r\nuser={}\r\nelevate={}\r\nservice={}\r\nservice_elevate={}\r\n",
        if plan.machine { "all" } else { "user" },
        plan.user_sid,
        label,
        u8::from(plan.needs_elevation),
        u8::from(service),
        u8::from(plan.service_needs_elevation(service, native::current_elevation()?))
    );
    write_report(&contents, report)
}

// NSIS's INI reader expects a Unicode BOM for non-ASCII account names.
fn write_report(value: &str, path: &Path) -> Result<(), String> {
    native::reject_reparse_chain(path)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "windows_uninstall_report_failed")?;
    let bytes: Vec<u8> = [0xfeffu16]
        .into_iter()
        .chain(value.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    file.write_all(&bytes)
        .map_err(|_| "windows_uninstall_report_failed".into())
}

pub(super) fn validate_registration(
    directory: &Path,
    plan: &Plan,
    user: &User,
) -> Result<(), String> {
    let root = if plan.machine {
        HKEY_LOCAL_MACHINE
    } else {
        user.hive.0
    };
    if !user::registered_at(root, directory)? {
        return Err("windows_uninstall_owner_mismatch".into());
    }
    Ok(())
}

fn account_name(sid: &str) -> String {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
    use windows_sys::Win32::Security::LookupAccountSidW;
    let mut raw = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(native::wide(sid).as_ptr(), &mut raw) } == 0 {
        return sid.into();
    }
    let mut name = [0u16; 256];
    let mut domain = [0u16; 256];
    let mut n = name.len() as u32;
    let mut d = domain.len() as u32;
    let mut kind = 0;
    let found = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            raw,
            name.as_mut_ptr(),
            &mut n,
            domain.as_mut_ptr(),
            &mut d,
            &mut kind,
        )
    } != 0;
    unsafe { LocalFree(raw) };
    if found {
        format!(
            "{}\\{}",
            String::from_utf16_lossy(&domain[..d as usize]).trim_end_matches('\0'),
            String::from_utf16_lossy(&name[..n as usize]).trim_end_matches('\0')
        )
    } else {
        sid.into()
    }
}
