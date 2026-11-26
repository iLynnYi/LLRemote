use super::*;

const USER: &str = "S-1-5-21-10-20-30-1001";
const OTHER: &str = "S-1-5-21-10-20-30-1002";

#[test]
fn ordinary_user_with_an_explicit_role_can_change_the_password() {
    assert!(can_manage_password(false, USER, &[USER.into()]));
}

#[test]
fn lifecycle_rights_and_another_accounts_role_do_not_grant_password_management() {
    assert!(!can_manage_password(false, USER, &[]));
    assert!(!can_manage_password(false, USER, &[OTHER.into()]));
    assert!(can_manage_password(true, USER, &[]));
}

#[test]
fn malformed_roles_and_untrusted_sid_shapes_fail_closed() {
    for roles in [
        vec![USER.into(), "invalid".into()],
        vec![USER.into(), USER.into()],
    ] {
        assert!(!can_manage_password(false, USER, &roles));
        assert!(validate_roles(&roles).is_err());
    }
    assert!(!can_manage_password(false, "invalid", &[USER.into()]));
}

#[test]
fn granting_is_idempotent_and_preserves_other_authorized_accounts() {
    let mut roles = vec![OTHER.into()];
    add_user(&mut roles, USER).unwrap();
    add_user(&mut roles, USER).unwrap();
    assert_eq!(roles, [OTHER, USER]);
    assert!(add_user(&mut roles, "invalid").is_err());
    assert_eq!(roles, [OTHER, USER]);
}

#[test]
fn sid_validation_accepts_canonical_user_sids_and_rejects_aliases_overflow_and_extra_parts() {
    assert!(valid_user_sid(USER));
    assert!(valid_user_sid("S-1-12-1-1-2-3-4"));
    for sid in [
        "BA",
        "S-1-5",
        "S-2-5-21-1",
        "S-1-05-21-1",
        "S-1-5--1",
        "S-1-5-4294967296",
        "S-1-281474976710656-1",
    ] {
        assert!(!valid_user_sid(sid), "accepted malformed SID");
    }
    assert!(!valid_user_sid(&format!("S-1-5{}", "-1".repeat(16))));
}

#[test]
fn password_change_requires_a_configuration_only_host_with_existing_configuration() {
    assert_eq!(
        validate_change_context(false, true),
        Err("windows_service_invalid_config".into())
    );
    assert_eq!(
        validate_change_context(true, false),
        Err("windows_service_credential_missing".into())
    );
    assert!(validate_change_context(true, true).is_ok());
}

struct FakeChange {
    authorized: bool,
    running: bool,
    failure: Option<&'static str>,
    changed: bool,
    supported: bool,
    events: Vec<&'static str>,
}

impl Default for FakeChange {
    fn default() -> Self {
        Self {
            authorized: true,
            running: false,
            failure: None,
            changed: false,
            supported: true,
            events: Vec::new(),
        }
    }
}

impl PasswordChangeOperations for FakeChange {
    fn ensure_authorized(&mut self) -> Result<(), String> {
        self.events.push("authorize");
        if self.authorized {
            Ok(())
        } else {
            Err("windows_service_password_management_authorization_required".into())
        }
    }
    fn is_running(&mut self) -> Result<bool, String> {
        self.events.push("state");
        Ok(self.running)
    }
    fn start_configuration_only(&mut self) -> Result<(), String> {
        self.events.push("configure_only");
        self.running = true;
        if self.failure == Some("start") {
            Err("windows_service_start_failed".into())
        } else {
            Ok(())
        }
    }
    fn change_password(&mut self) -> Result<(), String> {
        self.events.push("password");
        if self.failure == Some("password") {
            Err("windows_service_configure_failed".into())
        } else {
            self.changed = true;
            Ok(())
        }
    }
    fn password_management_supported(&mut self) -> Result<bool, String> {
        self.events.push("status");
        Ok(self.supported)
    }
    fn stop(&mut self) -> Result<(), String> {
        self.events.push("stop");
        if self.failure == Some("stop") {
            Err("windows_service_stop_failed".into())
        } else {
            self.running = false;
            Ok(())
        }
    }
}

#[test]
fn delegated_change_uses_a_temporary_service_and_restores_stopped_state() {
    let mut change = FakeChange::default();
    change_password(&mut change).unwrap();
    assert!(change.changed);
    assert!(!change.running);
    assert_eq!(
        change.events,
        [
            "authorize",
            "state",
            "configure_only",
            "status",
            "password",
            "stop"
        ]
    );
}

#[test]
fn ungranted_users_and_running_services_are_not_started_or_changed() {
    let mut ungranted = FakeChange {
        authorized: false,
        ..FakeChange::default()
    };
    assert_eq!(
        change_password(&mut ungranted),
        Err("windows_service_password_management_authorization_required".into())
    );
    assert_eq!(ungranted.events, ["authorize"]);
    let mut running = FakeChange {
        running: true,
        ..FakeChange::default()
    };
    assert_eq!(
        change_password(&mut running),
        Err("windows_service_invalid_config".into())
    );
    assert!(running.running);
    assert_eq!(running.events, ["authorize", "state"]);
}

#[test]
fn failed_start_or_password_change_still_restores_stopped_state() {
    for failure in ["start", "password"] {
        let mut change = FakeChange {
            failure: Some(failure),
            ..FakeChange::default()
        };
        assert!(change_password(&mut change).is_err());
        assert!(!change.changed);
        assert!(!change.running);
        assert_eq!(change.events.last(), Some(&"stop"));
    }
}

#[test]
fn unsupported_old_service_is_rejected_before_sending_the_password_request() {
    assert_eq!(
        ensure_supported(false),
        Err("windows_service_password_management_unsupported".into())
    );
    assert!(ensure_supported(true).is_ok());
    let mut change = FakeChange {
        supported: false,
        ..FakeChange::default()
    };
    assert_eq!(
        change_password(&mut change),
        Err("windows_service_password_management_unsupported".into())
    );
    assert!(!change.changed);
    assert!(!change.running);
    assert_eq!(
        change.events,
        ["authorize", "state", "configure_only", "status", "stop"]
    );
}

#[test]
fn cleanup_cannot_stop_a_service_started_by_another_caller_or_a_replacement_pid() {
    assert_eq!(owns_live_service(false, None, Some(200)), Ok(false));
    assert_eq!(
        owns_live_service(true, Some(100), Some(200)),
        Err("windows_service_stop_failed".into())
    );
    assert_eq!(
        owns_live_service(true, None, Some(200)),
        Err("windows_service_stop_failed".into())
    );
    assert_eq!(owns_live_service(true, Some(100), Some(100)), Ok(true));
    assert_eq!(owns_live_service(true, Some(100), None), Ok(true));
}

#[test]
fn configuration_only_ipc_cannot_target_a_replacement_or_normal_listener() {
    assert!(validate_configuration_instance(100, 100, true).is_ok());
    assert_eq!(
        validate_configuration_instance(100, 200, true),
        Err("windows_service_stop_failed".into())
    );
    assert_eq!(
        validate_configuration_instance(100, 100, false),
        Err("windows_service_stop_failed".into())
    );
    assert_eq!(
        validate_configuration_instance(0, 100, true),
        Err("windows_service_stop_failed".into())
    );
}

#[test]
fn a_successful_start_can_bind_its_pid_after_pending_zero_reaches_running() {
    assert!(bind_running_instance(true, false, 0).is_err());
    assert_eq!(bind_running_instance(true, true, 100), Ok(100));
    assert!(bind_running_instance(false, true, 100).is_err());
    assert!(bind_running_instance(true, false, 100).is_err());
    assert!(bind_running_instance(true, true, 0).is_err());
}
