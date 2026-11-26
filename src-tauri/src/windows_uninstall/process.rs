//! Close only the main executable belonging to the verified installation.

pub(super) fn background_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".exe") && name.starts_with("llremoteservice-")
}

fn candidate(
    name: &str,
    sid: Option<&str>,
    plan: &super::identity::Plan,
    operator: &str,
) -> Result<bool, String> {
    if !name.eq_ignore_ascii_case("LLRemote.exe") {
        return Ok(false);
    }
    if !plan.machine && sid.is_none() && operator != plan.user_sid {
        // An unresolved owner is not evidence of an unrelated installation.
        // Stop before cleanup if cross-account enumeration is still incomplete.
        return Err("windows_uninstall_process_owner_unknown".into());
    }
    Ok(plan.machine || sid == Some(plan.user_sid.as_str()))
}

fn open_candidate<T>(
    mut open: impl FnMut() -> Result<T, u32>,
    enable_debug: impl FnOnce() -> Result<(), String>,
) -> Result<Option<T>, String> {
    let result = match open() {
        // A runas administrator normally has SeDebugPrivilege disabled. Retry
        // only a denied application candidate, never arbitrary system processes.
        Err(5) => {
            enable_debug()?;
            open()
        }
        result => result,
    };
    match result {
        Ok(handle) => Ok(Some(handle)),
        // A process enumerated earlier may already have exited.
        Err(87) => Ok(None),
        Err(code) => Err(format!("windows_uninstall_process_access_denied:{code}")),
    }
}

#[cfg(target_os = "windows")]
pub(super) fn background_running(user: &super::user::User) -> Result<bool, String> {
    use crate::windows_service::native;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let _debug = if native::current_elevation()? {
        Some(super::user::PrivilegeGuard::enable("SeDebugPrivilege")?)
    } else {
        None
    };
    for entry in super::user::Processes::enumerate()?.entries() {
        if entry.pProcessName.is_null() {
            continue;
        }
        let mut length = 0;
        while unsafe { *entry.pProcessName.add(length) } != 0 {
            length += 1;
        }
        let name = unsafe {
            String::from_utf16_lossy(std::slice::from_raw_parts(entry.pProcessName, length))
        };
        if !background_name(&name) {
            continue;
        }
        let sid = super::user::process_sid(entry);
        if sid.as_deref() != Some(user.sid.as_str()) {
            if sid.is_none() && native::current_user_sid()? != user.sid {
                return Err("windows_uninstall_background_stop_failed".into());
            }
            continue;
        }
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.ProcessId) };
        if raw.is_null() && native::last_error() == 87 {
            continue;
        }
        let process = native::Handle::new(raw, "windows_uninstall_background_stop_failed")?;
        let mut image = [0u16; 32768];
        let mut length = image.len() as u32;
        if unsafe { QueryFullProcessImageNameW(process.0, 0, image.as_mut_ptr(), &mut length) } == 0
        {
            return Err("windows_uninstall_background_stop_failed".into());
        }
        let path = std::path::PathBuf::from(String::from_utf16_lossy(&image[..length as usize]));
        if super::shortcuts::background_target(&path, user) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "windows")]
pub(super) fn close_applications(
    directory: &std::path::Path,
    plan: &super::identity::Plan,
) -> Result<(), String> {
    use super::user::{self, PrivilegeGuard, Processes};
    use crate::windows_service::native;
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        PROCESS_TERMINATE,
    };

    let current = unsafe { GetCurrentProcessId() };
    let operator = native::current_user_sid()?;
    // Restore the exact original privilege state before any data cleanup or
    // target-user task starts. No privilege is enabled when it is unnecessary.
    let mut debug = None;
    let mut open = |pid, access| {
        open_candidate(
            || {
                let raw = unsafe { OpenProcess(access, 0, pid) };
                if raw.is_null() {
                    Err(native::last_error())
                } else {
                    Ok(native::Handle(raw))
                }
            },
            || {
                if debug.is_none() {
                    debug = Some(PrivilegeGuard::enable("SeDebugPrivilege").map_err(|_| {
                        "windows_uninstall_process_access_denied:debug_privilege".to_string()
                    })?);
                }
                Ok(())
            },
        )
    };
    for entry in Processes::enumerate()?.entries() {
        if entry.ProcessId == current || entry.ProcessId == 0 || entry.pProcessName.is_null() {
            continue;
        }
        let mut length = 0;
        while unsafe { *entry.pProcessName.add(length) } != 0 {
            length += 1;
        }
        let name = unsafe {
            String::from_utf16_lossy(std::slice::from_raw_parts(entry.pProcessName, length))
        };
        if !candidate(&name, user::process_sid(entry).as_deref(), plan, &operator)? {
            continue;
        }
        let Some(process) = open(entry.ProcessId, PROCESS_QUERY_LIMITED_INFORMATION)? else {
            continue;
        };
        let image_path = |handle: &native::Handle| -> Result<std::path::PathBuf, String> {
            let mut image = [0u16; 32768];
            let mut length = image.len() as u32;
            if unsafe { QueryFullProcessImageNameW(handle.0, 0, image.as_mut_ptr(), &mut length) }
                == 0
            {
                return Err(format!(
                    "windows_uninstall_process_query_failed:{}",
                    native::last_error()
                ));
            }
            Ok(std::path::PathBuf::from(String::from_utf16_lossy(
                &image[..length as usize],
            )))
        };
        let path = image_path(&process)?;
        if !user::same_path(&path, &directory.join("LLRemote.exe")) {
            continue;
        }
        // Request terminate access only after matching the full image path,
        // then verify again on the new handle to protect against PID reuse.
        let Some(process) = open(
            entry.ProcessId,
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
        )?
        else {
            continue;
        };
        if !user::same_path(&image_path(&process)?, &path) {
            return Err("windows_uninstall_process_identity_changed".into());
        }
        if unsafe { TerminateProcess(process.0, 0) } == 0 {
            let error = native::last_error();
            // Exiting between opening and terminating is harmless.
            if unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0 {
                continue;
            }
            return Err(format!("windows_uninstall_process_stop_failed:{error}"));
        }
        if unsafe { WaitForSingleObject(process.0, 8000) } != WAIT_OBJECT_0 {
            return Err("windows_uninstall_process_stop_failed:timeout".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_names_follow_windows_case_rules_and_exclude_the_system_service() {
        assert!(background_name("LLREMOTESERVICE-build.EXE"));
        assert!(!background_name("LLRemoteService.exe"));
        assert!(!background_name("LLRemote.exe"));
    }

    #[test]
    fn denied_candidate_is_retried_with_debug_and_never_silently_skipped() {
        let mut calls = 0;
        let mut enabled = false;
        assert_eq!(
            open_candidate(
                || {
                    calls += 1;
                    if calls == 1 {
                        Err(5)
                    } else {
                        Ok(42)
                    }
                },
                || {
                    enabled = true;
                    Ok(())
                }
            )
            .unwrap(),
            Some(42)
        );
        assert!(enabled);
        assert_eq!(calls, 2);
        assert!(open_candidate::<()>(|| Err(5), || Ok(())).is_err());
        assert!(open_candidate::<()>(|| Err(5), || Err("debug denied".into())).is_err());
    }

    #[test]
    fn only_a_process_that_has_gone_can_be_ignored() {
        assert_eq!(
            open_candidate::<()>(|| Err(87), || panic!("no elevation needed")).unwrap(),
            None
        );
        assert_eq!(
            open_candidate(|| Ok(42), || panic!("no elevation needed")).unwrap(),
            Some(42)
        );
        assert!(open_candidate::<()>(|| Err(6), || panic!("not access denied")).is_err());
    }

    #[test]
    fn candidates_exclude_helpers_and_other_personal_installations() {
        let personal = super::super::identity::Plan {
            user_sid: "owner".into(),
            machine: false,
            needs_elevation: false,
        };
        let machine = super::super::identity::Plan {
            user_sid: "owner".into(),
            machine: true,
            needs_elevation: false,
        };
        assert!(candidate("llremote.EXE", Some("owner"), &personal, "admin").unwrap());
        assert!(!candidate("LLRemote.exe", Some("admin"), &personal, "admin").unwrap());
        // Ordinary owners can still uninstall their own installation when an
        // unrelated account's inaccessible application is visible by name.
        assert!(!candidate("LLRemote.exe", None, &personal, "owner").unwrap());
        assert!(!candidate(
            ".llremote-uninstall-helper.exe",
            Some("owner"),
            &machine,
            "admin"
        )
        .unwrap());
        assert!(!candidate("LLRemoteService.exe", Some("owner"), &machine, "admin").unwrap());
        assert!(candidate("LLRemote.exe", Some("admin"), &machine, "admin").unwrap());
    }

    #[test]
    fn missing_cross_account_sid_cannot_silently_skip_a_running_application() {
        let plan = super::super::identity::Plan {
            user_sid: "owner".into(),
            machine: false,
            needs_elevation: false,
        };
        assert_eq!(
            candidate("LLRemote.exe", None, &plan, "admin").unwrap_err(),
            "windows_uninstall_process_owner_unknown"
        );
        let error = super::super::prepare_uninstall_with_actions(
            || Ok(()),
            || candidate("LLRemote.exe", None, &plan, "admin").map(|_| ()),
            || panic!("an unresolved process owner must prevent service removal"),
            || panic!("an unresolved process owner must prevent personal cleanup"),
        )
        .unwrap_err();
        assert_eq!(super::super::error_exit_code(&error), 124);
        assert!(candidate("LLRemote.exe", Some("owner"), &plan, "admin").unwrap());
    }
}
