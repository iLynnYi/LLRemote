//! Installer-only, headless cleanup. This entry runs before Tauri or a WebView.
#[cfg(any(target_os = "windows", test))]
pub(crate) mod data;
mod errors;
#[cfg(any(target_os = "windows", test))]
mod identity;
#[cfg(target_os = "windows")]
mod installation;
#[cfg(target_os = "windows")]
mod native;
#[cfg(any(target_os = "windows", test))]
mod process;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod report;
#[cfg(target_os = "windows")]
mod shortcuts;
#[cfg(target_os = "windows")]
mod user;
#[cfg(any(target_os = "windows", test))]
mod user_task;

#[cfg(target_os = "windows")]
pub(crate) fn same_install_directory(left: &std::path::Path, right: &std::path::Path) -> bool {
    user::same_path(left, right)
}

#[cfg(any(target_os = "windows", test))]
fn is_host_password_target(target: &str) -> bool {
    // keyring's Windows backend uses "{account}.{service}" as the target.
    target.starts_with("host:")
        && target.ends_with(&format!(".{}", crate::credential_vault::SERVICE_NAME))
}

#[cfg(any(target_os = "windows", test))]
fn prepare_uninstall_with_actions(
    authorize_service: impl FnOnce() -> Result<(), String>,
    close_applications: impl FnOnce() -> Result<(), String>,
    remove_service: impl FnOnce() -> Result<(), String>,
    clean_personal: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // Refusing service authorization must leave the application and its data
    // available for a later retry with an administrator.
    authorize_service()?;
    close_applications()?;
    remove_service()?;
    clean_personal()
}

pub fn run_if_requested() -> Result<Option<i32>, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if !matches!(
        args.first().map(String::as_str),
        Some(
            "--prepare-uninstall"
                | "--finish-uninstall"
                | "--uninstall-inspect"
                | "--uninstall-user-stop"
                | "--uninstall-user-check"
                | "--uninstall-user-clean"
        )
    ) {
        return Ok(None);
    }
    let output = if args.len() >= 3 && args[args.len() - 2] == "--uninstall-result" {
        let path = std::path::PathBuf::from(args.pop().unwrap());
        args.pop();
        Some(path)
    } else {
        None
    };
    #[cfg(target_os = "windows")]
    {
        // The installed executable used to supply its directory implicitly.
        // A package-supplied helper carries the original directory as its last option.
        let requested_directory =
            if args.len() >= 3 && args[args.len() - 2] == "--uninstall-directory" {
                let directory = args.pop();
                args.pop();
                directory
            } else {
                None
            };
        let directory = installation::directory(requested_directory.as_deref())?;
        let cleanup = match args.as_slice() {
            [verb, flag, sid] if verb == "--uninstall-user-stop" && flag == "--target-user" => {
                Some(native::user_action(
                    &directory,
                    sid,
                    user_task::UserAction::Stop,
                )?)
            }

            [verb, flag, sid] if verb == "--uninstall-user-check" && flag == "--target-user" => {
                Some(native::user_action(
                    &directory,
                    sid,
                    user_task::UserAction::Check,
                )?)
            }
            [verb, flag, sid] if verb == "--uninstall-user-clean" && flag == "--target-user" => {
                Some(native::user_action(
                    &directory,
                    sid,
                    user_task::UserAction::Clean { delete_data: false },
                )?)
            }
            [verb, flag, sid, purge]
                if verb == "--uninstall-user-clean"
                    && flag == "--target-user"
                    && purge == "--delete-data" =>
            {
                Some(native::user_action(
                    &directory,
                    sid,
                    user_task::UserAction::Clean { delete_data: true },
                )?)
            }
            [verb, flag, sid] if verb == "--finish-uninstall" && flag == "--target-user" => {
                Some(native::finish(&directory, false, sid)?)
            }
            [verb, flag, sid, purge]
                if verb == "--finish-uninstall"
                    && flag == "--target-user"
                    && purge == "--delete-data" =>
            {
                Some(native::finish(&directory, true, sid)?)
            }
            [verb, report] if verb == "--uninstall-inspect" => {
                installation::inspect(&directory, std::path::Path::new(report), None)?;
                None
            }
            [verb, report, sid] if verb == "--uninstall-inspect" => {
                installation::inspect(&directory, std::path::Path::new(report), Some(sid))?;
                None
            }
            [verb] if verb == "--prepare-uninstall" => {
                Some(native::prepare(&directory, false, None)?)
            }
            [verb, flag] if verb == "--prepare-uninstall" && flag == "--delete-data" => {
                Some(native::prepare(&directory, true, None)?)
            }
            [verb, flag, sid] if verb == "--prepare-uninstall" && flag == "--target-user" => {
                Some(native::prepare(&directory, false, Some(sid))?)
            }
            [verb, flag, sid, purge]
                if verb == "--prepare-uninstall"
                    && flag == "--target-user"
                    && purge == "--delete-data" =>
            {
                Some(native::prepare(&directory, true, Some(sid))?)
            }
            _ => return Err("windows_uninstall_invalid_arguments".into()),
        };
        let Some(mut cleanup) = cleanup else {
            return Ok(Some(0));
        };
        if let Some(path) = output {
            if cleanup
                .write(
                    &path,
                    matches!(
                        args[0].as_str(),
                        "--uninstall-user-stop" | "--uninstall-user-clean"
                    ),
                )
                .is_err()
            {
                cleanup.warning("windows_uninstall_report_failed", path);
            }
        }
        Ok(Some(cleanup.exit_code()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = output;
        Err("windows_uninstall_unsupported".into())
    }
}

pub fn error_exit_code(error: &str) -> i32 {
    errors::exit_code(error)
}

#[cfg(target_os = "windows")]
pub(crate) fn error_from_exit(code: u32) -> String {
    errors::from_exit(code)
}

/// Installer commands run before Tauri, stores or the GUI single-instance gate.
/// Stable exit codes survive the Windows GUI subsystem (which has no stderr).
pub fn run_update_if_requested() -> Result<Option<i32>, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !matches!(
        args.first().map(String::as_str),
        Some(
            "--installer-check"
                | "--installer-state"
                | "--installer-verify"
                | "--installer-service"
                | "--prepare-update"
                | "--finish-update"
                | "--installer-owner-user"
                | "--installer-owner-all"
        )
    ) {
        return Ok(None);
    }
    if args.len() != 1 && !(args.len() == 3 && args[1] == "--installer-report") {
        return Ok(Some(80));
    }
    #[cfg(target_os = "windows")]
    {
        let result = match args[0].as_str() {
            // Loading the real packaged EXE exercises its imports and manifest.
            // These checks never load stores, migrate data or start listeners.
            "--installer-check" => Ok(0),
            "--installer-state" => installer_state(),
            "--installer-verify" => verify_webview_runtime(),
            "--installer-owner-user" => installation::record("user")
                .map(|_| 0)
                .map_err(|error| (95, error)),
            "--installer-owner-all" => installation::record("all")
                .map(|_| 0)
                .map_err(|error| (95, error)),
            "--installer-service" => crate::windows_service::install_for_installer()
                .map_err(|error| (90, error))
                .and_then(|state| {
                    if state.installed && state.status == "stopped" {
                        Ok(0)
                    } else {
                        Err((90, "windows_service_postinstall_verification_failed".into()))
                    }
                }),
            "--finish-update" => crate::background_share::finish_update()
                .map(|_| 0)
                .map_err(|error| (84, error)),
            _ => prepare_update_with_actions(
                crate::windows_service::prepare_app_update,
                crate::background_share::prepare_update,
            )
            .map(|had_service| if had_service { 10 } else { 0 }),
        };
        let (exit, detail) = match result {
            Ok(exit) => (exit, String::new()),
            Err((stage, error)) => (installer_failure_code(stage, &error), error),
        };
        if let Some(path) = args.get(2) {
            let path = std::path::Path::new(path);
            let report = (|| -> Result<(), String> {
                crate::windows_service::native::reject_reparse_chain(
                    path.parent().ok_or("installer_report_invalid")?,
                )?;
                write_installer_report(path, exit, &detail).map_err(|error| error.to_string())
            })();
            if report.is_err() {
                return Ok(Some(89));
            }
        }
        Ok(Some(exit))
    }
    #[cfg(not(target_os = "windows"))]
    Err("windows_install_unsupported".into())
}

#[cfg(target_os = "windows")]
fn verify_webview_runtime() -> Result<i32, (i32, String)> {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows_core_webview::{PCWSTR, PWSTR};
    let mut version = PWSTR::null();
    let result =
        unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) };
    let available = !version.is_null();
    if available {
        unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(version.0.cast()) };
    }
    result.map_err(|error| {
        (
            94,
            format!("webview2_runtime_unavailable:{:08x}", error.code().0),
        )
    })?;
    if !available {
        return Err((94, "webview2_runtime_unavailable".into()));
    }
    Ok(0)
}

#[cfg(target_os = "windows")]
fn installer_state() -> Result<i32, (i32, String)> {
    let service = crate::windows_service::installer_state().map_err(|error| (81, error))?;
    let background =
        crate::background_share::installer_has_runtime().map_err(|error| (83, error))?;
    Ok(match (service, background) {
        (None, false) => 0,
        (Some(false), false) => 10,
        (Some(true), false) => 11,
        (None, true) => 12,
        (Some(_), true) => 13,
    })
}

#[cfg(any(target_os = "windows", test))]
fn write_installer_report(path: &std::path::Path, exit: i32, detail: &str) -> std::io::Result<()> {
    use std::io::Write;
    // Diagnostics are fixed machine codes, never arbitrary user/configuration
    // contents. Refuse overwrites, including a pre-existing symlink.
    let detail: String = detail
        .chars()
        .take(256)
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-' | '=' | ';'))
        .collect();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    write!(
        file,
        "[maintenance]\r\nprotocol=1\r\nresult={exit}\r\ndetail={detail}\r\n"
    )?;
    file.sync_all()
}

#[cfg(any(target_os = "windows", test))]
fn installer_failure_code(stage: i32, error: &str) -> i32 {
    match error.split(':').next().unwrap_or(error) {
        "windows_service_uac_cancelled" => 85,
        "windows_service_elevation_denied" => 86,
        "windows_service_access_denied" | "windows_service_admin_required" => 87,
        "windows_service_ipc_timeout" => 88,
        "windows_service_install_copy_failed" => 91,
        "windows_service_install_register_failed" => 92,
        "windows_service_install_authorization_failed" => 93,
        _ => stage,
    }
}

#[cfg(any(target_os = "windows", test))]
fn prepare_update_with_actions(
    stop_service: impl FnOnce() -> Result<bool, String>,
    stop_background: impl FnOnce() -> Result<(), String>,
) -> Result<bool, (i32, String)> {
    let had_service = stop_service().map_err(|error| (82, error))?;
    stop_background().map_err(|error| (83, error))?;
    // Only stop here. Service replacement happens after the installed payload
    // has been verified, so a copy failure cannot leave a newer service behind.
    Ok(had_service)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleanup_reasons_must_not_collapse_into_one_generic_exit() {
        for (reason, expected) in [
            ("windows_uninstall_background_stop_failed", 109),
            ("windows_uninstall_shortcut_cleanup_failed", 110),
            ("windows_uninstall_startup_cleanup_failed", 111),
            ("windows_uninstall_registry_cleanup_failed", 112),
            ("windows_uninstall_credential_cleanup_failed", 113),
            ("windows_uninstall_data_cleanup_failed", 114),
            ("windows_uninstall_data_in_use:32", 115),
            ("windows_uninstall_data_access_denied:5", 116),
            ("windows_uninstall_data_unsafe_path", 117),
        ] {
            assert_eq!(super::error_exit_code(reason), expected, "{reason}");
        }
    }

    #[test]
    fn service_authorization_precedes_all_uninstall_changes() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        super::prepare_uninstall_with_actions(
            || {
                calls.borrow_mut().push("authorize");
                Ok(())
            },
            || {
                calls.borrow_mut().push("close");
                Ok(())
            },
            || {
                calls.borrow_mut().push("service");
                Ok(())
            },
            || {
                calls.borrow_mut().push("personal");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            *calls.borrow(),
            ["authorize", "close", "service", "personal"]
        );

        let error = "windows_uninstall_service_admin_required";
        assert_eq!(
            super::prepare_uninstall_with_actions(
                || Err(error.into()),
                || panic!("no authorization: keep the application running"),
                || panic!("no authorization: keep the service intact"),
                || panic!("no authorization: keep personal data and shortcuts"),
            )
            .unwrap_err(),
            error
        );
        assert_eq!(super::error_exit_code(error), 108);
    }

    #[test]
    fn failed_service_removal_does_not_continue_personal_cleanup() {
        assert_eq!(
            super::prepare_uninstall_with_actions(
                || Ok(()),
                || Ok(()),
                || Err("windows_service_uninstall_failed".into()),
                || panic!("keep personal data after service removal failed"),
            )
            .unwrap_err(),
            "windows_service_uninstall_failed"
        );
    }

    #[test]
    fn preparation_only_stops_runtimes_and_preserves_failure_stage() {
        let calls = std::cell::RefCell::new(Vec::new());
        let service = super::prepare_update_with_actions(
            || {
                calls.borrow_mut().push("service");
                Ok(true)
            },
            || {
                calls.borrow_mut().push("background");
                Ok(())
            },
        )
        .unwrap();
        assert!(service);
        assert_eq!(*calls.borrow(), ["service", "background"]);
        assert_eq!(
            super::prepare_update_with_actions(
                || Err("windows_service_uac_cancelled".into()),
                || panic!("must not stop the background after cancellation"),
            )
            .unwrap_err(),
            (82, "windows_service_uac_cancelled".into())
        );
        assert_eq!(
            super::prepare_update_with_actions(|| Ok(false), || Err("busy".into()),).unwrap_err(),
            (83, "busy".into())
        );
        assert!(!super::prepare_update_with_actions(|| Ok(false), || Ok(())).unwrap());
    }

    #[test]
    fn user_session_and_profile_failures_have_distinct_uninstall_exit_codes() {
        assert_eq!(
            super::error_exit_code("windows_uninstall_process_access_denied:5"),
            125
        );
        assert_eq!(
            super::error_exit_code("windows_uninstall_process_stop_failed:timeout"),
            127
        );
        assert_eq!(
            super::error_exit_code("windows_uninstall_user_task_failed:8007052e"),
            104
        );
        assert_eq!(
            super::error_exit_code("windows_uninstall_profile_unavailable"),
            106
        );
        assert_eq!(
            super::error_exit_code("windows_uninstall_user_login_required"),
            103
        );
        assert_eq!(
            super::error_exit_code("windows_uninstall_owner_mismatch"),
            102
        );
    }

    #[test]
    fn installer_errors_distinguish_authorization_from_runtime_failures() {
        for (error, code) in [
            ("windows_service_uac_cancelled", 85),
            ("windows_service_elevation_denied", 86),
            ("windows_service_access_denied", 87),
            ("windows_service_ipc_timeout", 88),
        ] {
            assert_eq!(super::installer_failure_code(82, error), code);
        }
        assert_eq!(super::installer_failure_code(83, "busy"), 83);
        assert_eq!(
            super::installer_failure_code(
                82,
                "windows_service_admin_required:stage=elevated_helper;exit_code=65",
            ),
            87
        );
        assert_eq!(
            super::installer_failure_code(84, "configuration_failed"),
            84
        );
    }

    #[test]
    fn diagnostic_report_is_bounded_and_cannot_overwrite_an_existing_file() {
        let path =
            std::env::temp_dir().join(format!("llremote-install-{}.ini", uuid::Uuid::new_v4()));
        super::write_installer_report(&path, 82, "windows_service_stop_failed:5\n[injected]")
            .unwrap();
        let report = std::fs::read_to_string(&path).unwrap();
        assert!(report.contains("protocol=1\r\nresult=82"));
        assert!(report.contains("detail=windows_service_stop_failed:5injected"));
        assert!(super::write_installer_report(&path, 0, "").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), report);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn credential_cleanup_is_limited_to_this_apps_host_namespace() {
        assert!(super::is_host_password_target(
            "host:123.LLRemote Host Password"
        ));
        assert!(!super::is_host_password_target("host:123.Other App"));
        assert!(!super::is_host_password_target(
            "other.LLRemote Host Password"
        ));
        assert!(!super::is_host_password_target(
            "host:123.LLRemote Host Password.extra"
        ));
    }
}
