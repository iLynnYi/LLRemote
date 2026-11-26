//! Stable failures across the short-lived, elevated maintenance process.

const ERRORS: &[(u32, &str)] = &[
    (65, "windows_service_admin_required"),
    (66, "windows_service_access_denied"),
    (67, "windows_service_binary_missing"),
    (68, "windows_service_ipc_timeout"),
    (69, "windows_service_not_installed"),
    (70, "windows_service_invalid_config"),
    (71, "windows_service_install_copy_failed"),
    (72, "windows_service_install_register_failed"),
    (73, "windows_service_install_authorization_failed"),
    (74, "windows_uninstall_service_owner_unknown"),
];
const SETUP_ERRORS: &[(u32, &str)] = &[
    (75, "windows_service_configure_failed"),
    (76, "windows_service_credential_missing"),
];

fn is_action(action: Option<&str>) -> bool {
    matches!(
        action,
        Some(
            "--install"
                | "--install-relay"
                | "--repair-sharing-relay"
                | "--configure-relay"
                | "--uninstall"
                | "--start"
                | "--stop"
                | "--enable-sas"
                | "--enable-autostart"
                | "--disable-autostart"
                | "--authorize-password-management"
        )
    )
}

pub(super) fn helper_exit_code(action: Option<&str>, error: &str) -> i32 {
    if action == Some("--install-relay") {
        if let Some((exit, _)) = SETUP_ERRORS.iter().find(|(_, known)| *known == error) {
            return *exit as i32;
        }
    }
    if action == Some("--enable-autostart") && error == "windows_service_credential_missing" {
        return 64;
    }
    if is_action(action) {
        if let Some((exit, _)) = ERRORS.iter().find(|(_, known)| *known == error) {
            return *exit as i32;
        }
    }
    1
}

pub(super) fn helper_error(action: Option<&str>, exit: u32) -> Option<&'static str> {
    if action == Some("--install-relay") {
        if let Some((_, error)) = SETUP_ERRORS.iter().find(|(known, _)| *known == exit) {
            return Some(error);
        }
    }
    if action == Some("--enable-autostart") && exit == 64 {
        return Some("windows_service_credential_missing");
    }
    is_action(action)
        .then(|| {
            ERRORS
                .iter()
                .find(|(known, _)| *known == exit)
                .map(|(_, error)| *error)
        })
        .flatten()
}

pub(super) fn elevation_error(error: u32) -> &'static str {
    match error {
        1223 => "windows_service_uac_cancelled", // ERROR_CANCELLED
        2 | 3 => "windows_service_binary_missing",
        5 | 1260 => "windows_service_elevation_denied", // access denied / policy
        _ => "windows_service_elevation_failed",
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_maintenance_tests.rs"]
mod tests;
