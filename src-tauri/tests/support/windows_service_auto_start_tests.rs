use super::*;

struct FakeService {
    running: bool,
    auto_start: bool,
    address: Option<String>,
    port: Option<u16>,
    credential_configured: bool,
    failure: Option<&'static str>,
    events: Vec<&'static str>,
}

impl Default for FakeService {
    fn default() -> Self {
        Self {
            running: false,
            auto_start: false,
            address: Some("0.0.0.0".into()),
            port: Some(5904),
            credential_configured: true,
            failure: None,
            events: Vec::new(),
        }
    }
}

impl AutoStartOperations for FakeService {
    fn is_running(&mut self) -> Result<bool, String> {
        self.events.push("state");
        if self.failure == Some("state") {
            Err("windows_service_not_installed".into())
        } else {
            Ok(self.running)
        }
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

    fn readiness(&mut self) -> Result<AutoStartReadiness, String> {
        self.events.push("readiness");
        assert!(
            self.running,
            "only a live service can verify its system credential"
        );
        if self.failure == Some("readiness") {
            Err("windows_service_ipc_unavailable".into())
        } else {
            Ok(AutoStartReadiness::new(
                self.address.as_deref(),
                self.port,
                self.credential_configured,
            ))
        }
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

    fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.events.push(if enabled { "enable" } else { "disable" });
        if self.failure == Some("change") {
            Err("windows_service_configure_failed".into())
        } else {
            self.auto_start = enabled;
            Ok(())
        }
    }
}

#[test]
fn enabling_without_a_saved_system_credential_is_rejected() {
    let mut service = FakeService {
        credential_configured: false,
        ..FakeService::default()
    };
    assert_eq!(
        set_enabled(true, &mut service),
        Err("windows_service_credential_missing".into())
    );
    assert!(!service.auto_start);
    assert!(!service.running);
    assert_eq!(
        service.events,
        ["state", "configure_only", "readiness", "stop"]
    );
}

#[test]
fn enabling_requires_valid_shared_configuration_even_when_a_credential_exists() {
    for (address, port) in [
        (None, Some(5904)),
        (Some("invalid"), Some(5904)),
        (Some("0.0.0.0"), None),
        (Some("0.0.0.0"), Some(0)),
    ] {
        let mut service = FakeService {
            address: address.map(str::to_string),
            port,
            ..FakeService::default()
        };
        assert_eq!(
            set_enabled(true, &mut service),
            Err("windows_service_credential_missing".into())
        );
        assert!(!service.auto_start);
        assert!(!service.running);
        assert!(!service.events.contains(&"enable"));
    }
}

#[test]
fn stopped_service_is_restored_before_changing_its_startup_policy() {
    let mut service = FakeService::default();
    set_enabled(true, &mut service).unwrap();
    assert!(service.auto_start);
    assert!(!service.running);
    assert_eq!(
        service.events,
        ["state", "configure_only", "readiness", "stop", "enable"]
    );
}

#[test]
fn running_service_is_verified_without_restarting_or_stopping_it() {
    let mut service = FakeService {
        running: true,
        ..FakeService::default()
    };
    set_enabled(true, &mut service).unwrap();
    assert!(service.running);
    assert!(service.auto_start);
    assert_eq!(service.events, ["state", "readiness", "enable"]);
}

#[test]
fn credential_failure_keeps_an_existing_running_service_and_startup_policy() {
    let mut service = FakeService {
        running: true,
        auto_start: true,
        credential_configured: false,
        ..FakeService::default()
    };
    assert_eq!(
        set_enabled(true, &mut service),
        Err("windows_service_credential_missing".into())
    );
    assert!(service.running);
    assert!(service.auto_start);
    assert_eq!(service.events, ["state", "readiness"]);
}

#[test]
fn an_unavailable_service_cannot_trigger_validation_or_change_policy() {
    let mut service = FakeService {
        failure: Some("state"),
        ..FakeService::default()
    };
    assert_eq!(
        set_enabled(true, &mut service),
        Err("windows_service_not_installed".into())
    );
    assert!(!service.auto_start);
    assert_eq!(service.events, ["state"]);
}

#[test]
fn verification_and_temporary_start_failures_restore_stopped_state_and_preserve_policy() {
    for (failure, expected) in [
        ("readiness", "windows_service_ipc_unavailable"),
        ("start", "windows_service_start_failed"),
    ] {
        let mut service = FakeService {
            auto_start: true,
            failure: Some(failure),
            ..FakeService::default()
        };
        assert_eq!(set_enabled(true, &mut service), Err(expected.into()));
        assert!(service.auto_start);
        assert!(!service.running);
        assert_eq!(service.events.last(), Some(&"stop"));
        assert!(!service.events.contains(&"enable"));
    }
}

#[test]
fn restoration_failure_cannot_change_the_startup_policy() {
    let mut service = FakeService {
        failure: Some("stop"),
        ..FakeService::default()
    };
    assert_eq!(
        set_enabled(true, &mut service),
        Err("windows_service_stop_failed".into())
    );
    assert!(!service.auto_start);
    assert!(!service.events.contains(&"enable"));
}

#[test]
fn scm_change_failure_leaves_the_service_stopped_and_preserves_its_policy() {
    let mut service = FakeService {
        failure: Some("change"),
        ..FakeService::default()
    };
    assert_eq!(
        set_enabled(true, &mut service),
        Err("windows_service_configure_failed".into())
    );
    assert!(!service.running);
    assert!(!service.auto_start);
}

#[test]
fn disabling_needs_neither_configuration_nor_credentials_nor_a_temporary_service() {
    let mut service = FakeService {
        running: true,
        auto_start: true,
        address: None,
        port: None,
        credential_configured: false,
        failure: Some("readiness"),
        ..FakeService::default()
    };
    set_enabled(false, &mut service).unwrap();
    assert!(!service.auto_start);
    assert!(service.running);
    assert_eq!(service.events, ["disable"]);
}
