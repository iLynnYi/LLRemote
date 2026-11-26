use super::*;

#[test]
fn missing_credential_category_survives_an_auto_start_helper_exit() {
    let action = Some("--enable-autostart");
    let exit_code = helper_exit_code(action, "windows_service_credential_missing");
    assert_ne!(exit_code, 1);
    assert_eq!(
        helper_error(action, exit_code as u32),
        Some("windows_service_credential_missing")
    );
}

#[test]
fn setup_helper_preserves_partial_failures_without_changing_other_actions() {
    for &(exit, error) in SETUP_ERRORS.iter().chain(ERRORS) {
        assert_eq!(
            helper_exit_code(Some("--install-relay"), error),
            exit as i32
        );
        assert_eq!(helper_error(Some("--install-relay"), exit), Some(error));
    }
    for &(exit, _) in SETUP_ERRORS {
        assert_eq!(helper_error(Some("--start"), exit), None);
        assert_eq!(helper_error(Some("--configure-relay"), exit), None);
    }
}

#[test]
fn helper_exit_mapping_rejects_unknown_errors_and_unrelated_actions() {
    for action in [
        None,
        Some("--disable-autostart"),
        Some("--configure-relay"),
        Some("--worker"),
    ] {
        assert_eq!(
            helper_exit_code(action, "windows_service_credential_missing"),
            1
        );
    }
    for error in [
        "windows_service_configure_failed",
        "unexpected native failure",
        "windows_service_credential_missing: detail",
    ] {
        assert_eq!(helper_exit_code(Some("--enable-autostart"), error), 1);
    }
}

#[test]
fn unknown_helper_exit_codes_and_unrelated_actions_keep_the_callers_fallback() {
    let credential_exit = helper_exit_code(
        Some("--enable-autostart"),
        "windows_service_credential_missing",
    ) as u32;
    for action in [
        None,
        Some("--disable-autostart"),
        Some("--configure-relay"),
        Some("--stop"),
    ] {
        assert_eq!(helper_error(action, credential_exit), None);
    }
    for exit in [0, 1, u32::MAX] {
        assert_eq!(helper_error(Some("--enable-autostart"), exit), None);
    }
}

#[test]
fn installation_helper_preserves_permission_and_binary_failures() {
    for error in [
        "windows_service_admin_required",
        "windows_service_access_denied",
        "windows_service_binary_missing",
        "windows_service_ipc_timeout",
        "windows_service_install_copy_failed",
        "windows_service_install_register_failed",
        "windows_service_install_authorization_failed",
    ] {
        let exit = helper_exit_code(Some("--install"), error);
        assert_ne!(
            exit, 1,
            "{error} must not become a generic installation failure"
        );
        assert_eq!(helper_error(Some("--install"), exit as u32), Some(error));
    }
}

#[test]
fn elevation_launch_distinguishes_cancel_policy_binary_and_shell_failures() {
    assert_eq!(elevation_error(1223), "windows_service_uac_cancelled");
    for code in [5, 1260] {
        assert_eq!(elevation_error(code), "windows_service_elevation_denied");
    }
    for code in [2, 3] {
        assert_eq!(elevation_error(code), "windows_service_binary_missing");
    }
    for code in [0, 8, 31, u32::MAX] {
        assert_eq!(elevation_error(code), "windows_service_elevation_failed");
    }
}

#[test]
fn stable_failures_round_trip_only_for_maintenance_actions() {
    for action in [
        "--install",
        "--install-relay",
        "--configure-relay",
        "--repair-sharing-relay",
        "--uninstall",
        "--start",
        "--stop",
        "--enable-sas",
        "--enable-autostart",
        "--disable-autostart",
        "--authorize-password-management",
    ] {
        for &(exit, error) in ERRORS {
            assert_eq!(helper_exit_code(Some(action), error), exit as i32);
            assert_eq!(helper_error(Some(action), exit), Some(error));
        }
    }
    for action in [None, Some("--worker"), Some("--service")] {
        for &(exit, error) in ERRORS {
            assert_eq!(helper_exit_code(action, error), 1);
            assert_eq!(helper_error(action, exit), None);
        }
    }
}
