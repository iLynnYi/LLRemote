//! Repair decisions shared by the elevated helper and the service. Repair is
//! explicit; normal installation never overrides an existing SAS policy.
use super::types::{
    RepairKind, RepairStatus, WindowsServiceRepairItem, WindowsServiceRepairReport,
};
use super::WindowsServiceOverview;

pub(super) const UAC_DISABLED: &str = "windows_service_uac_disabled";

pub(super) trait RepairOperations {
    fn snapshot(&mut self) -> Result<WindowsServiceOverview, String>;
    fn update_required(&self) -> bool {
        false
    }
    fn update_service(&mut self) -> Result<(), String>;
    fn authorize(&mut self) -> Result<(), String>;
    fn repair_sas(&mut self) -> Result<(), String>;
    fn repair_agent(&mut self) -> Result<(), String>;
}

fn service_update_needed(view: &WindowsServiceOverview, update_required: bool) -> bool {
    update_required || (view.status == "running" && !view.user_agent_repair_supported)
}

pub(super) fn item(
    kind: RepairKind,
    status: RepairStatus,
    error_code: Option<String>,
) -> WindowsServiceRepairItem {
    WindowsServiceRepairItem {
        kind,
        status,
        error_code,
    }
}

pub(super) fn inspect_elevation(
    enabled: Option<bool>,
    elevated: bool,
    restart_pending: bool,
    enable: impl FnOnce() -> Result<(), &'static str>,
) -> WindowsServiceRepairItem {
    let (status, error) = match enabled {
        Some(false) if !elevated => (RepairStatus::NeedsAdministrator, Some(UAC_DISABLED)),
        Some(false) => match enable() {
            Ok(()) => (RepairStatus::RestartRequired, None),
            Err(code) => (RepairStatus::Failed, Some(code)),
        },
        Some(true) if restart_pending => (RepairStatus::RestartRequired, None),
        Some(true) => (RepairStatus::Healthy, None),
        None => (
            RepairStatus::Unknown,
            Some("windows_service_uac_status_unknown"),
        ),
    };
    item(RepairKind::Elevation, status, error.map(str::to_owned))
}

fn outcome(kind: RepairKind, result: Result<(), String>) -> WindowsServiceRepairItem {
    match result {
        Ok(()) => item(kind, RepairStatus::Repaired, None),
        Err(code) => item(kind, RepairStatus::Failed, Some(code)),
    }
}

/// Read-only results are also used when there is nothing actionable, avoiding
/// an elevation prompt just to report a healthy or stopped service.
pub(super) fn inspect(
    view: WindowsServiceOverview,
    update_required: bool,
) -> WindowsServiceRepairReport {
    let service_error = view.last_error_code.clone();
    let agent_status =
        if view.status != "running" || !view.worker_running || !view.user_agent_repair_supported {
            RepairStatus::Deferred
        } else if view.user_agent_running && view.user_agent_error_code.is_none() {
            RepairStatus::Healthy
        } else {
            RepairStatus::NeedsAdministrator
        };
    let items = vec![
        item(
            RepairKind::Service,
            if service_update_needed(&view, update_required) {
                RepairStatus::NeedsAdministrator
            } else if service_error.is_some() {
                RepairStatus::Failed
            } else {
                RepairStatus::Healthy
            },
            service_error,
        ),
        item(
            RepairKind::Permissions,
            if view.password_management_authorized {
                RepairStatus::Healthy
            } else {
                RepairStatus::NeedsAdministrator
            },
            None,
        ),
        item(
            RepairKind::Sas,
            if view.sas_policy == "services_allowed" {
                RepairStatus::Healthy
            } else {
                RepairStatus::NeedsAdministrator
            },
            None,
        ),
        item(
            RepairKind::Agent,
            agent_status,
            view.user_agent_error_code.clone(),
        ),
    ];
    WindowsServiceRepairReport {
        overview: view,
        items,
    }
}

/// Repair only detected problems; preserve a stopped service and continue
/// independent repairs when one item fails. Verify each successful operation.
pub(super) fn run(ops: &mut impl RepairOperations) -> Result<WindowsServiceRepairReport, String> {
    let mut view = ops.snapshot()?;
    if !view.installed {
        return Err("windows_service_not_installed".into());
    }
    let was_running = view.status == "running";
    let mut items = Vec::new();
    if service_update_needed(&view, ops.update_required()) {
        let result = ops.update_service();
        view = ops.snapshot()?;
        items.push(outcome(
            RepairKind::Service,
            result.and_then(|()| {
                if !ops.update_required()
                    && (!was_running
                        || (view.status == "running" && view.user_agent_repair_supported))
                {
                    Ok(())
                } else {
                    Err("windows_service_protocol_incompatible".into())
                }
            }),
        ));
    } else {
        items.push(item(
            RepairKind::Service,
            if view.last_error_code.is_some() {
                RepairStatus::Failed
            } else {
                RepairStatus::Healthy
            },
            view.last_error_code.clone(),
        ));
    }
    if view.password_management_authorized {
        items.push(item(RepairKind::Permissions, RepairStatus::Healthy, None));
    } else {
        let result = ops.authorize();
        view = ops.snapshot()?;
        items.push(outcome(
            RepairKind::Permissions,
            result.and_then(|()| {
                if view.password_management_authorized {
                    Ok(())
                } else {
                    Err("windows_service_password_management_authorization_failed".into())
                }
            }),
        ));
    }
    if view.sas_policy == "services_allowed" {
        items.push(item(RepairKind::Sas, RepairStatus::Healthy, None));
    } else {
        let result = ops.repair_sas();
        view = ops.snapshot()?;
        items.push(outcome(
            RepairKind::Sas,
            result.and_then(|()| {
                if view.sas_policy == "services_allowed" {
                    Ok(())
                } else {
                    Err("windows_service_sas_policy_configure_failed".into())
                }
            }),
        ));
    }
    if view.status != "running" || !view.worker_running {
        items.push(item(
            RepairKind::Agent,
            RepairStatus::Deferred,
            view.user_agent_error_code.clone(),
        ));
    } else if view.user_agent_running && view.user_agent_error_code.is_none() {
        items.push(item(RepairKind::Agent, RepairStatus::Healthy, None));
    } else if !view.user_agent_repair_supported {
        items.push(item(
            RepairKind::Agent,
            RepairStatus::Deferred,
            Some("windows_service_protocol_incompatible".into()),
        ));
    } else {
        let result = ops.repair_agent();
        view = ops.snapshot()?;
        items.push(outcome(
            RepairKind::Agent,
            result.and_then(|()| {
                if agent_ready(&view)? {
                    Ok(())
                } else {
                    Err("windows_service_user_agent_repair_timeout".into())
                }
            }),
        ));
    }
    // A later operation or a managed policy refresh can invalidate an earlier
    // repair. Final status, rather than an operation's ACK, owns the result.
    for entry in &mut items {
        if !matches!(entry.status, RepairStatus::Healthy | RepairStatus::Repaired) {
            continue;
        }
        let error = match entry.kind {
            RepairKind::Service if view.last_error_code.is_some() => view.last_error_code.clone(),
            RepairKind::Service if was_running && view.status != "running" => {
                Some("windows_service_start_failed".into())
            }
            RepairKind::Permissions if !view.password_management_authorized => {
                Some("windows_service_password_management_authorization_failed".into())
            }
            RepairKind::Sas if view.sas_policy != "services_allowed" => {
                Some("windows_service_sas_policy_configure_failed".into())
            }
            RepairKind::Agent
                if !view.user_agent_running || view.user_agent_error_code.is_some() =>
            {
                Some(
                    view.user_agent_error_code
                        .clone()
                        .unwrap_or("windows_service_user_agent_repair_timeout".into()),
                )
            }
            _ => None,
        };
        if let Some(code) = error {
            entry.status = RepairStatus::Failed;
            entry.error_code = Some(code);
        }
    }
    Ok(WindowsServiceRepairReport {
        overview: view,
        items,
    })
}

pub(super) fn sas_value(current: Option<u32>, explicit_repair: bool) -> Result<u32, String> {
    match current {
        None => Ok(1),
        Some(value @ (1 | 3)) => Ok(value),
        Some(value @ (0 | 2)) if explicit_repair => Ok(value | 1),
        Some(0 | 2) => Err("windows_service_sas_policy_denied".into()),
        _ => Err("windows_service_sas_policy_configure_failed".into()),
    }
}

pub(super) fn agent_ready(overview: &WindowsServiceOverview) -> Result<bool, String> {
    if !overview.user_agent_repair_supported {
        return Err("windows_service_protocol_incompatible".into());
    }
    if let Some(code) = &overview.user_agent_error_code {
        return Err(code.clone());
    }
    Ok(overview.user_agent_running)
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_repair_tests.rs"]
mod tests;
