//! Stable, lossless categories across the owner task, helper and NSIS boundary.
const ERRORS: &[(&str, i32)] = &[
    ("windows_uninstall_owner_unknown", 101),
    ("windows_uninstall_owner_mismatch", 102),
    ("windows_uninstall_user_login_required", 103),
    ("windows_uninstall_user_context_failed", 104),
    ("windows_uninstall_service_owner_unknown", 105),
    ("windows_uninstall_profile_unavailable", 106),
    ("windows_uninstall_process_stop_failed", 107),
    ("windows_uninstall_service_admin_required", 108),
    ("windows_uninstall_background_stop_failed", 109),
    ("windows_uninstall_shortcut_cleanup_failed", 110),
    ("windows_uninstall_startup_cleanup_failed", 111),
    ("windows_uninstall_registry_cleanup_failed", 112),
    ("windows_uninstall_credential_cleanup_failed", 113),
    ("windows_uninstall_data_cleanup_failed", 114),
    ("windows_uninstall_data_in_use", 115),
    ("windows_uninstall_data_access_denied", 116),
    ("windows_uninstall_data_unsafe_path", 117),
    ("windows_uninstall_service_cleanup_failed", 118),
    ("windows_uninstall_service_access_denied", 119),
    ("windows_uninstall_service_timeout", 120),
    ("windows_uninstall_binary_missing", 121),
    ("windows_uninstall_helper_copy_failed", 122),
    ("windows_uninstall_helper_launch_failed", 123),
    ("windows_uninstall_process_owner_unknown", 124),
    ("windows_uninstall_process_access_denied", 125),
    ("windows_uninstall_process_query_failed", 126),
    ("windows_uninstall_process_stop_timeout", 127),
    ("windows_uninstall_user_task_timeout", 128),
    ("windows_uninstall_invalid_config", 129),
    ("windows_uninstall_admin_required", 130),
    ("windows_uninstall_service_stop_failed", 131),
    ("windows_uninstall_service_credential_failed", 132),
    ("windows_uninstall_report_failed", 133),
    ("windows_uninstall_files_failed", 134),
];

pub(super) fn exit_code(error: &str) -> i32 {
    // A previously unknown owner-task exit remains visible, including crashes.
    if let Some(raw) = error.strip_prefix("windows_uninstall_user_cleanup_failed:") {
        return u32::from_str_radix(raw, 16)
            .map(|value| value as i32)
            .unwrap_or(1);
    }
    if error == "windows_uninstall_process_stop_failed:timeout" {
        return 127;
    }
    let category = error.split(':').next().unwrap_or(error);
    if let Some((_, code)) = ERRORS.iter().find(|(known, _)| *known == category) {
        return *code;
    }
    match category {
        "windows_uninstall_service_owner_mismatch" => 105,
        "windows_uninstall_process_identity_changed" => 124,
        "windows_uninstall_user_task_failed" => 104,
        "windows_uninstall_registry_access_denied" | "windows_uninstall_registry_invalid" => 112,
        "windows_uninstall_invalid_arguments" => 129,
        _ => 1,
    }
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn from_exit(code: u32) -> String {
    ERRORS
        .iter()
        .find(|(_, known)| *known as u32 == code)
        .map(|(category, _)| (*category).to_owned())
        .unwrap_or_else(|| format!("windows_uninstall_user_cleanup_failed:{code:08x}"))
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn service_error(error: String) -> String {
    // Shared token/path helpers also use windows_service_* errors. Only label
    // them as service failures when the caller is actually in a service stage.
    let category = error.split(':').next().unwrap_or(&error);
    let reason = match category {
        "windows_service_uac_cancelled"
        | "windows_service_elevation_denied"
        | "windows_service_elevation_failed"
        | "windows_service_admin_required" => Some("windows_uninstall_service_admin_required"),
        "windows_service_access_denied" => Some("windows_uninstall_service_access_denied"),
        "windows_service_ipc_timeout" => Some("windows_uninstall_service_timeout"),
        "windows_service_binary_missing" => Some("windows_uninstall_binary_missing"),
        "windows_service_stop_failed" => Some("windows_uninstall_service_stop_failed"),
        value if value.starts_with("windows_service_credential_") => {
            Some("windows_uninstall_service_credential_failed")
        }
        _ => None,
    };
    if let Some(reason) = reason {
        return format!("{reason}:{error}");
    }
    if exit_code(&error) != 1 {
        error
    } else {
        format!("windows_uninstall_service_cleanup_failed:{error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_survives_the_owner_task_round_trip() {
        for &(reason, code) in ERRORS {
            assert_eq!(exit_code(reason), code);
            assert_eq!(exit_code(&from_exit(code as u32)), code, "{reason}");
        }
        for code in [1, 17, 0x80070002, 0xc0000005] {
            assert_eq!(exit_code(&from_exit(code)) as u32, code);
        }
    }

    #[test]
    fn process_identity_access_query_and_timeout_are_distinct() {
        assert_eq!(exit_code("windows_uninstall_process_owner_unknown"), 124);
        assert_eq!(exit_code("windows_uninstall_process_access_denied:5"), 125);
        assert_eq!(exit_code("windows_uninstall_process_query_failed:5"), 126);
        assert_eq!(
            exit_code("windows_uninstall_process_stop_failed:timeout"),
            127
        );
        assert_eq!(exit_code("windows_uninstall_user_task_timeout"), 128);
        assert_eq!(exit_code("unknown_failure"), 1);
    }

    #[test]
    fn service_failures_are_identified_at_the_service_call_site() {
        for (error, code) in [
            ("windows_service_access_denied", 119),
            ("windows_service_ipc_timeout", 120),
            ("windows_service_stop_failed", 131),
            ("windows_service_credential_unavailable", 132),
            ("windows_service_uninstall_failed", 118),
        ] {
            assert_eq!(exit_code(&service_error(error.into())), code);
        }
        assert_eq!(exit_code("windows_service_access_denied"), 1);
        assert_eq!(
            exit_code(&service_error("windows_uninstall_data_in_use:32".into())),
            115
        );
    }
}
