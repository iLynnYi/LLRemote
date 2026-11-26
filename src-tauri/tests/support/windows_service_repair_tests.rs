use super::*;

#[test]
fn read_only_inspection_preserves_deferred_and_unrepairable_runtime_errors() {
    let mut ops = FakeRepair::new();
    ops.view.status = "stopped".into();
    ops.view.worker_running = false;
    ops.view.password_management_authorized = true;
    ops.view.sas_policy = "services_allowed".into();
    ops.view.last_error_code = Some("windows_service_port_conflict".into());
    let report = inspect(ops.view, false);
    assert_eq!(report.items[0].status, RepairStatus::Failed);
    assert_eq!(report.items[3].status, RepairStatus::Deferred);
}

#[test]
fn ordinary_inspection_lists_the_exact_repairs_requiring_administrator_access() {
    let mut ops = FakeRepair::new();
    ops.view.status = "stopped".into();
    ops.view.worker_running = false;
    let report = inspect(ops.view, true);
    let pending: Vec<_> = report
        .items
        .iter()
        .filter(|item| item.status == RepairStatus::NeedsAdministrator)
        .map(|item| item.kind.clone())
        .collect();
    assert_eq!(
        pending,
        [
            RepairKind::Service,
            RepairKind::Permissions,
            RepairKind::Sas
        ]
    );
    assert_eq!(report.items[3].status, RepairStatus::Deferred);
}

#[test]
fn explicit_uac_repair_requires_elevation_and_reports_restart_or_write_failure() {
    let no_write = || panic!("must not write the machine policy");
    let ordinary = inspect_elevation(Some(false), false, false, no_write);
    assert_eq!(ordinary.status, RepairStatus::NeedsAdministrator);
    assert_eq!(ordinary.error_code.as_deref(), Some(UAC_DISABLED));
    assert_eq!(
        inspect_elevation(None, true, false, no_write).status,
        RepairStatus::Unknown
    );
    assert_eq!(
        inspect_elevation(Some(true), true, false, no_write).status,
        RepairStatus::Healthy
    );
    assert_eq!(
        inspect_elevation(Some(true), true, true, no_write).status,
        RepairStatus::RestartRequired
    );
    assert_eq!(
        inspect_elevation(Some(false), true, false, || Ok(())).status,
        RepairStatus::RestartRequired
    );
    let failed = inspect_elevation(Some(false), true, false, || Err("policy_write_failed"));
    assert_eq!(failed.status, RepairStatus::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("policy_write_failed"));
}

struct FakeRepair {
    view: WindowsServiceOverview,
    calls: Vec<&'static str>,
    fail: Option<&'static str>,
    acknowledge_only: bool,
    revert_policy_during_agent: bool,
    update_required: bool,
}

impl FakeRepair {
    fn new() -> Self {
        let mut view = WindowsServiceOverview::unsupported();
        view.supported = true;
        view.installed = true;
        view.status = "running".into();
        view.worker_running = true;
        view.sas_policy = "services_denied".into();
        view.user_agent_repair_supported = true;
        Self {
            view,
            calls: vec![],
            fail: None,
            acknowledge_only: false,
            revert_policy_during_agent: false,
            update_required: false,
        }
    }
    fn action(&mut self, name: &'static str) -> Result<bool, String> {
        self.calls.push(name);
        if self.fail == Some(name) {
            return Err("windows_service_access_denied".into());
        }
        Ok(!self.acknowledge_only)
    }
}

impl RepairOperations for FakeRepair {
    fn update_required(&self) -> bool {
        self.update_required
    }
    fn snapshot(&mut self) -> Result<WindowsServiceOverview, String> {
        Ok(self.view.clone())
    }
    fn update_service(&mut self) -> Result<(), String> {
        if self.action("update")? {
            self.view.user_agent_repair_supported = true;
            self.update_required = false;
        }
        Ok(())
    }
    fn authorize(&mut self) -> Result<(), String> {
        if self.action("authorize")? {
            self.view.password_management_authorized = true;
        }
        Ok(())
    }
    fn repair_sas(&mut self) -> Result<(), String> {
        if self.action("sas")? {
            self.view.sas_policy = "services_allowed".into();
        }
        Ok(())
    }
    fn repair_agent(&mut self) -> Result<(), String> {
        if self.action("agent")? {
            self.view.user_agent_running = true;
            self.view.user_agent_error_code = None;
        }
        if self.revert_policy_during_agent {
            self.view.sas_policy = "services_denied".into();
        }
        Ok(())
    }
}

#[test]
fn unified_repair_detects_old_service_permissions_policy_and_agent() {
    let mut ops = FakeRepair::new();
    ops.view.user_agent_repair_supported = false;
    let report = run(&mut ops).unwrap();
    assert_eq!(ops.calls, ["update", "authorize", "sas", "agent"]);
    assert!(report
        .items
        .iter()
        .all(|item| item.status == RepairStatus::Repaired));
}

#[test]
fn a_healthy_runtime_is_checked_without_restarting_anything() {
    let mut ops = FakeRepair::new();
    ops.view.password_management_authorized = true;
    ops.view.sas_policy = "services_allowed".into();
    ops.view.user_agent_running = true;
    assert!(inspect(ops.view.clone(), false)
        .items
        .iter()
        .all(|item| item.status == RepairStatus::Healthy));
    let report = run(&mut ops).unwrap();
    assert!(ops.calls.is_empty());
    assert!(report
        .items
        .iter()
        .all(|item| item.status == RepairStatus::Healthy));
}

#[test]
fn repairing_a_stopped_service_never_starts_a_listener_and_defers_agent_checks() {
    let mut ops = FakeRepair::new();
    ops.view.status = "stopped".into();
    ops.view.worker_running = false;
    ops.view.user_agent_repair_supported = false;
    let report = run(&mut ops).unwrap();
    assert_eq!(ops.calls, ["authorize", "sas"]);
    assert_eq!(report.overview.status, "stopped");
    assert_eq!(report.items.last().unwrap().status, RepairStatus::Deferred);
}

#[test]
fn a_stopped_outdated_binary_is_updated_without_starting_sharing() {
    let mut ops = FakeRepair::new();
    ops.view.status = "stopped".into();
    ops.view.worker_running = false;
    ops.update_required = true;
    let report = run(&mut ops).unwrap();
    assert_eq!(ops.calls, ["update", "authorize", "sas"]);
    assert_eq!(report.overview.status, "stopped");
    assert_eq!(report.items[0].status, RepairStatus::Repaired);
    assert_eq!(report.items[3].status, RepairStatus::Deferred);
}

#[test]
fn independent_repairs_continue_after_one_failure_and_keep_partial_results() {
    let mut ops = FakeRepair::new();
    ops.fail = Some("sas");
    let report = run(&mut ops).unwrap();
    assert_eq!(ops.calls, ["authorize", "sas", "agent"]);
    assert_eq!(report.items[2].status, RepairStatus::Failed);
    assert_eq!(report.items[3].status, RepairStatus::Repaired);
    assert_eq!(
        report.items[2].error_code.as_deref(),
        Some("windows_service_access_denied")
    );
}

#[test]
fn acknowledgments_without_actual_recovery_and_reverted_policy_are_not_success() {
    let mut ops = FakeRepair::new();
    ops.acknowledge_only = true;
    let report = run(&mut ops).unwrap();
    assert!(report.items[1..]
        .iter()
        .all(|item| item.status == RepairStatus::Failed));
    let mut ops = FakeRepair::new();
    ops.revert_policy_during_agent = true;
    let report = run(&mut ops).unwrap();
    assert_eq!(report.items[2].status, RepairStatus::Failed);
    assert_eq!(report.items[3].status, RepairStatus::Repaired);
}

#[test]
fn explicit_sas_repair_allows_services_and_preserves_uiaccess_scope() {
    assert_eq!(sas_value(Some(0), true).unwrap(), 1);
    assert_eq!(sas_value(Some(2), true).unwrap(), 3);
    for value in [1, 3] {
        assert_eq!(sas_value(Some(value), true).unwrap(), value);
    }
    assert_eq!(sas_value(None, true).unwrap(), 1);
}

#[test]
fn ordinary_enable_preserves_denials_and_repair_never_replaces_unknown_values() {
    for value in [0, 2] {
        assert_eq!(
            sas_value(Some(value), false).unwrap_err(),
            "windows_service_sas_policy_denied"
        );
    }
    for repair in [false, true] {
        assert_eq!(
            sas_value(Some(7), repair).unwrap_err(),
            "windows_service_sas_policy_configure_failed"
        );
    }
}

#[test]
fn enhancement_repair_requires_ready_runtime_and_does_not_hide_errors_or_old_services() {
    let mut overview = WindowsServiceOverview::unsupported();
    assert_eq!(
        agent_ready(&overview).unwrap_err(),
        "windows_service_protocol_incompatible"
    );
    overview.user_agent_repair_supported = true;
    assert!(!agent_ready(&overview).unwrap());
    overview.user_agent_running = true;
    overview.user_agent_error_code = Some("windows_service_user_agent_identity_failed".into());
    assert_eq!(
        agent_ready(&overview).unwrap_err(),
        "windows_service_user_agent_identity_failed"
    );
    overview.user_agent_error_code = None;
    assert!(agent_ready(&overview).unwrap());
}
