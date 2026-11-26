use super::data;
use crate::windows_service::native;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredEnumerateW, CredFree, CREDENTIALW, CRED_TYPE_GENERIC,
};

pub(super) fn prepare(
    directory: &Path,
    delete_data: bool,
    requested: Option<&str>,
) -> Result<super::report::CleanupReport, String> {
    use super::identity::CleanupContext;
    use super::report::CleanupReport;
    use super::{installation, user, user_task};
    let plan = installation::plan(directory, requested)?;
    if plan.needs_elevation {
        return Err("windows_uninstall_admin_required".into());
    }
    let operator = native::current_user_sid()?;
    let logged_on = plan.user_sid != operator && user::logged_on(&plan.user_sid)?;
    let mut context = CleanupContext::select(&plan.user_sid, &operator, logged_on, delete_data)?;
    let user = user::User::resolve(&plan.user_sid)?;
    installation::validate_registration(directory, &plan, &user)?;
    let cleanup = std::cell::RefCell::new(CleanupReport::default());
    if context == CleanupContext::OwnerSession {
        if let Err(error) = user_task::run(&user, directory, user_task::UserAction::Check) {
            // A missing owner session may prevent optional secret cleanup, but
            // must not leave a live background host behind.
            if super::process::background_running(&user)? {
                return Err(format!("windows_uninstall_background_stop_failed:{error}"));
            }
            cleanup
                .borrow_mut()
                .warning(&error, user.local.join("LLRemote"));
            context = CleanupContext::Offline;
        }
    }
    super::prepare_uninstall_with_actions(
        || {
            let service = crate::windows_service::validate_app_uninstall(directory)
                .map_err(super::errors::service_error)?;
            if plan.service_needs_elevation(service, native::current_elevation()?) {
                return Err("windows_uninstall_service_admin_required".into());
            }
            Ok(())
        },
        || {
            super::process::close_applications(directory, &plan)?;
            if context == CleanupContext::OwnerSession {
                let stopped = user_task::run(&user, directory, user_task::UserAction::Stop)?;
                cleanup.borrow_mut().merge(stopped);
            } else {
                stop_personal(&user, context == CleanupContext::CurrentUser)?;
            }
            Ok(())
        },
        || {
            cleanup.borrow_mut().merge(
                crate::windows_service::prepare_app_uninstall(delete_data, directory)
                    .map_err(super::errors::service_error)?,
            );
            Ok(())
        },
        || {
            let personal = if context == CleanupContext::OwnerSession {
                match user_task::run(
                    &user,
                    directory,
                    user_task::UserAction::Clean { delete_data },
                ) {
                    Ok(report) => report,
                    Err(error) => {
                        if matches!(super::errors::exit_code(&error), 101 | 102 | 109 | 111)
                            || super::process::background_running(&user)?
                        {
                            return Err(error);
                        }
                        let mut report = CleanupReport::default();
                        report.warning(&error, user.local.join("LLRemote"));
                        if delete_data {
                            report.warning(
                                "windows_uninstall_user_login_required",
                                "Windows Credential Manager",
                            );
                            report.warning(
                                "windows_uninstall_data_cleanup_failed",
                                user.local.join("LLRemote"),
                            );
                        }
                        report
                    }
                }
            } else {
                clean_personal(
                    &user,
                    directory,
                    plan.machine,
                    delete_data,
                    context == CleanupContext::CurrentUser,
                )?
            };
            cleanup.borrow_mut().merge(personal);
            Ok(())
        },
    )?;
    cleanup.borrow_mut().capture(
        user.close()
            .map_err(|error| format!("windows_uninstall_profile_unavailable:{error}")),
        "Windows user profile (temporary uninstall mount)",
    );
    Ok(cleanup.into_inner())
}

pub(super) fn user_action(
    directory: &Path,
    sid: &str,
    action: super::user_task::UserAction,
) -> Result<super::report::CleanupReport, String> {
    use super::user_task::UserAction;
    use super::{installation, user};
    if native::current_user_sid()? != sid {
        return Err("windows_uninstall_owner_mismatch".into());
    }
    let plan = installation::plan(directory, Some(sid))?;
    let user = user::User::resolve(sid)?;
    installation::validate_registration(directory, &plan, &user)?;
    let mut report = match action {
        UserAction::Check => super::report::CleanupReport::default(),
        UserAction::Stop => {
            stop_personal(&user, true)?;
            super::report::CleanupReport::default()
        }
        UserAction::Clean { delete_data } => {
            clean_personal(&user, directory, plan.machine, delete_data, true)?
        }
    };
    report.capture(
        user.close()
            .map_err(|error| format!("windows_uninstall_profile_unavailable:{error}")),
        "Windows user profile (temporary uninstall mount)",
    );
    Ok(report)
}

fn stop_personal(user: &super::user::User, in_owner_session: bool) -> Result<(), String> {
    use super::shortcuts;
    if in_owner_session {
        if native::current_user_sid()? != user.sid {
            return Err("windows_uninstall_owner_mismatch".into());
        }
        crate::background_share::stop_for_user_uninstall(&user.sid, &user.local)
            .map_err(|error| format!("windows_uninstall_background_stop_failed:{error}"))?;
    }
    if super::process::background_running(user)? {
        return Err("windows_uninstall_background_stop_failed".into());
    }
    // This shortcut can launch a still-valid background executable even after
    // the App is removed, unlike a stale main-application shortcut.
    shortcuts::disable_background_startup(user)
        .map_err(|error| format!("windows_uninstall_startup_cleanup_failed:{error}"))?;
    Ok(())
}

fn clean_personal(
    user: &super::user::User,
    directory: &std::path::Path,
    machine: bool,
    delete_data: bool,
    in_owner_session: bool,
) -> Result<super::report::CleanupReport, String> {
    use super::shortcuts;
    stop_personal(user, in_owner_session)?;
    let mut report = super::report::CleanupReport::default();
    report.capture(remove_startup_approvals(user.hive.0), &user.startup);
    report.merge(shortcuts::remove(user, directory, machine));
    if delete_data {
        if in_owner_session {
            report.capture(remove_host_passwords(), "Windows Credential Manager");
            report.capture(
                crate::background_share::delete_saved_password().map_err(|error| {
                    format!("windows_uninstall_credential_cleanup_failed:{error}")
                }),
                "Windows Credential Manager",
            );
        } else {
            report.warning(
                "windows_uninstall_user_login_required",
                "Windows Credential Manager",
            );
        }
        report.merge(data::cleanup_user_data(&user.local));
    }
    Ok(report)
}

pub(super) fn finish(
    directory: &Path,
    delete_data: bool,
    requested: &str,
) -> Result<super::report::CleanupReport, String> {
    use super::{installation, user};
    let mut report = super::report::CleanupReport::default();
    // Package files have gone. Registry cleanup is now a residue, not a failed
    // removal of the executable. Still never remove an unverified user's keys.
    let cleaned = (|| -> Result<(), String> {
        let plan = installation::plan(directory, Some(requested))?;
        if plan.needs_elevation {
            return Err("windows_uninstall_admin_required".into());
        }
        let user = user::User::resolve(&plan.user_sid)?;
        installation::validate_registration(directory, &plan, &user)?;
        let root = if plan.machine {
            windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE
        } else {
            user.hive.0
        };
        if delete_data {
            report.capture(user::delete_key(root, user::PRODUCT_KEY), user::PRODUCT_KEY);
        }
        report.capture(
            user::delete_key(root, user::UNINSTALL_KEY),
            user::UNINSTALL_KEY,
        );
        report.capture(
            user.close()
                .map_err(|error| format!("windows_uninstall_profile_unavailable:{error}")),
            "Windows user profile (temporary uninstall mount)",
        );
        Ok(())
    })();
    report.capture(cleaned, user::UNINSTALL_KEY);
    Ok(report)
}

fn remove_startup_approvals(
    hive: windows_sys::Win32::System::Registry::HKEY,
) -> Result<(), String> {
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, KEY_SET_VALUE,
    };
    let path = native::wide(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder",
    );
    let mut key = null_mut();
    let result = unsafe { RegOpenKeyExW(hive, path.as_ptr(), 0, KEY_SET_VALUE, &mut key) };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if result != 0 {
        return Err("windows_uninstall_startup_cleanup_failed".into());
    }
    let result = unsafe { RegDeleteValueW(key, native::wide("LLRemote Sharing.lnk").as_ptr()) };
    unsafe { RegCloseKey(key) };
    if result != 0 && result != ERROR_FILE_NOT_FOUND {
        Err("windows_uninstall_startup_cleanup_failed".into())
    } else {
        Ok(())
    }
}

struct Credentials(*mut *mut CREDENTIALW);
impl Drop for Credentials {
    fn drop(&mut self) {
        unsafe { CredFree(self.0.cast()) };
    }
}

fn remove_host_passwords() -> Result<(), String> {
    let mut count = 0;
    let mut entries = null_mut();
    if unsafe { CredEnumerateW(native::wide("host:*").as_ptr(), 0, &mut count, &mut entries) } == 0
    {
        return if native::last_error() == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err("windows_uninstall_credential_cleanup_failed".into())
        };
    }
    let entries = Credentials(entries);
    for entry in unsafe { std::slice::from_raw_parts(entries.0, count as usize) } {
        let entry = unsafe { &**entry };
        if entry.Type != CRED_TYPE_GENERIC || entry.TargetName.is_null() {
            continue;
        }
        let mut length = 0;
        while unsafe { *entry.TargetName.add(length) } != 0 {
            length += 1;
        }
        let target = String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(entry.TargetName, length)
        });
        if !super::is_host_password_target(&target) {
            continue;
        }
        if unsafe { CredDeleteW(entry.TargetName, CRED_TYPE_GENERIC, 0) } == 0
            && native::last_error() != ERROR_NOT_FOUND
        {
            return Err("windows_uninstall_credential_cleanup_failed".into());
        }
    }
    Ok(())
}
