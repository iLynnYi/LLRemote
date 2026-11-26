use super::*;
use std::cell::Cell;

fn overview(process_running: bool) -> BackgroundShareOverview {
    normalize_overview(BackgroundShareOverview {
        service_fallback: false,
        temporary_share: false,
        server: crate::local_vnc_server::pending_local_vnc_server_overview(),
        configured: false,
        credential_configured: false,
        auto_start: false,
        startup_registration: StartupRegistration::Unknown,
        last_login_attempt: None,
        process_running,
        last_error_code: None,
        runtime: None,
        current_build_id: BUILD_ID.into(),
        permission_source: BackgroundSharePermissionSource::Pending,
        restart_supported: cfg!(target_os = "macos"),
        ipc_version: IPC_VERSION,
        #[cfg(target_os = "macos")]
        permission_identity: None,
        signature_changed_permissions: vec![],
    })
}

#[test]
fn build_update_replaces_only_a_known_idle_host_with_atomic_recovery() {
    for state in ["running", "starting", "stopping", "stopped"] {
        let mut view = overview(true);
        view.server.status = serde_json::from_value(serde_json::json!(state)).unwrap();
        view.runtime = Some(types::BackgroundShareRuntime {
            pid: 123,
            build_id: "old-build".into(),
        });
        assert_eq!(idle_host_needs_update(&view), state == "stopped");
        view.runtime.as_mut().unwrap().build_id = BUILD_ID.into();
        assert!(!idle_host_needs_update(&view));
    }
    let mut missing = overview(false);
    missing.server.status = crate::local_vnc_server::LocalVncServerStatus::Stopped;
    assert!(!idle_host_needs_update(&missing));
}

#[test]
fn ordinary_repair_prepares_only_an_idle_host_and_defers_file_checks() {
    let reads = Cell::new(0);
    let ensured = Cell::new(false);
    let items = repair::run(
        || {
            reads.set(reads.get() + 1);
            if reads.get() == 1 {
                return Err("background_share_ipc_unavailable".into());
            }
            assert!(ensured.get());
            let mut view = overview(true);
            view.server.status = crate::local_vnc_server::LocalVncServerStatus::Stopped;
            Ok(view)
        },
        || {
            ensured.set(true);
            Ok(())
        },
        || panic!("a newly prepared idle host needs no restart"),
    );
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].kind,
        crate::windows_service::RepairKind::Background
    );
    assert_eq!(
        items[0].status,
        crate::windows_service::RepairStatus::Repaired
    );
    assert_eq!(
        items[1].status,
        crate::windows_service::RepairStatus::Deferred
    );
}

#[test]
fn ordinary_repair_retains_native_launch_stage_without_reporting_success() {
    let code = "background_share_user_context_required:set_integrity:5";
    let items = repair::run(
        || Err("background_share_ipc_unavailable".into()),
        || Err(code.into()),
        || panic!("a failed launch must not attempt to stop anything"),
    );
    assert_eq!(
        items[0].status,
        crate::windows_service::RepairStatus::Failed
    );
    assert_eq!(items[0].error_code.as_deref(), Some(code));
    assert_eq!(
        items[1].status,
        crate::windows_service::RepairStatus::Deferred
    );
}

#[test]
fn ordinary_repair_updates_an_idle_host_without_restarting_a_live_listener() {
    for running in [false, true] {
        let restarted = Cell::new(false);
        let items = repair::run(
            || {
                let mut view = overview(true);
                view.runtime = Some(types::BackgroundShareRuntime {
                    pid: 123,
                    build_id: "previous-build".into(),
                });
                view.server.status = if running {
                    crate::local_vnc_server::LocalVncServerStatus::Running
                } else {
                    crate::local_vnc_server::LocalVncServerStatus::Stopped
                };
                Ok(view)
            },
            || Ok(()),
            || {
                assert!(!running);
                restarted.set(true);
                let mut view = overview(true);
                view.server.status = crate::local_vnc_server::LocalVncServerStatus::Stopped;
                Ok(view)
            },
        );
        assert_eq!(restarted.get(), !running);
        assert_eq!(
            items[0].status,
            if running {
                crate::windows_service::RepairStatus::Failed
            } else {
                crate::windows_service::RepairStatus::Repaired
            }
        );
    }
}

#[test]
fn ordinary_repair_verifies_file_capabilities_without_interrupting_active_sharing() {
    let items = repair::run(
        || {
            let mut view = overview(true);
            view.server.status = crate::local_vnc_server::LocalVncServerStatus::Running;
            view.server.agent.status = crate::local_agent_service::LocalAgentStatus::Running;
            view.server.agent.capabilities =
                vec!["file.transfer.upload".into(), "clipboard.text.write".into()];
            Ok(view)
        },
        || Ok(()),
        || panic!("a healthy active listener must stay connected"),
    );
    assert!(items
        .iter()
        .all(|item| item.status == crate::windows_service::RepairStatus::Healthy));
}

#[test]
fn cold_start_launches_an_absent_user_host_before_sending_start() {
    let launched = Cell::new(false);
    let result = ensure_host_with_actions(
        || require_running_host(overview(launched.get())),
        || {
            launched.set(true);
            Ok(())
        },
        |_: &()| Ok(()),
    )
    .and_then(|()| {
        // The subsequent Start IPC has no endpoint until launch actually ran.
        if launched.get() {
            Ok(())
        } else {
            Err("background_share_ipc_unavailable".into())
        }
    });
    assert_eq!(result, Ok(()));
}

#[test]
fn launch_waits_for_a_real_host_instead_of_the_pending_status() {
    let reads = Cell::new(0);
    ensure_host_with_actions(
        || {
            reads.set(reads.get() + 1);
            require_running_host(overview(reads.get() >= 3))
        },
        || Ok(()),
        |_: &()| Ok(()),
    )
    .unwrap();
    assert_eq!(reads.get(), 3);
}

#[test]
fn an_idle_live_host_is_reused_without_launching_another_process() {
    ensure_host_with_actions(
        || require_running_host(overview(true)),
        || panic!("the existing idle host owns the endpoint"),
        |_: &()| Ok(()),
    )
    .unwrap();
}

#[test]
fn control_errors_do_not_launch_a_replacement() {
    for code in ["background_share_busy", "background_share_access_denied"] {
        let result = ensure_host_with_actions(
            || Err(code.into()),
            || panic!("an IPC error is not evidence that the host is absent"),
            |_: &()| Ok(()),
        );
        assert_eq!(result, Err(code.into()));
    }
}

#[test]
fn absent_host_propagates_launch_failure() {
    let result = ensure_host_with_actions(
        || require_running_host(overview(false)),
        || Err("background_share_startup_unavailable".into()),
        |_: &()| Ok(()),
    );
    assert_eq!(result, Err("background_share_startup_unavailable".into()));
}

#[test]
fn stopped_process_is_absent_for_shutdown_waiting_but_display_status_stays_readable() {
    let pending = overview(false);
    assert!(!pending.process_running);
    assert_eq!(
        pending.permission_source,
        BackgroundSharePermissionSource::Pending
    );
    let result = require_running_host(pending);
    assert_eq!(result.unwrap_err(), "background_share_ipc_unavailable");
}

#[test]
fn incompatible_live_host_does_not_launch_a_replacement() {
    let result = ensure_host_with_actions(
        || {
            let mut host = overview(true);
            host.ipc_version = IPC_VERSION + 1;
            require_running_host(host)
        },
        || panic!("the incompatible process still owns the endpoint"),
        |_: &()| Ok(()),
    );
    assert_eq!(result, Err("background_share_protocol_incompatible".into()));
}

#[test]
fn a_child_exit_is_reported_before_the_readiness_timeout() {
    let reads = Cell::new(0);
    let result = ensure_host_with_actions(
        || {
            reads.set(reads.get() + 1);
            if reads.get() < 3 {
                Err("background_share_ipc_unavailable".into())
            } else {
                Ok(overview(true))
            }
        },
        || Ok(()),
        |_: &()| Err("background_share_runtime_failed: stage=process_exit;exit_code=1".into()),
    );
    assert_eq!(
        result,
        Err("background_share_runtime_failed: stage=process_exit;exit_code=1".into())
    );
}

#[test]
fn readiness_checks_keep_non_transient_ipc_failures() {
    for code in [
        "background_share_access_denied",
        "background_share_invalid_reply",
    ] {
        let reads = Cell::new(0);
        let result = ensure_host_with_actions(
            || {
                reads.set(reads.get() + 1);
                match reads.get() {
                    1 => Err("background_share_ipc_unavailable".into()),
                    2 => Err(code.into()),
                    _ => Ok(overview(true)),
                }
            },
            || Ok(()),
            |_: &()| Ok(()),
        );
        assert_eq!(result, Err(code.into()));
    }
}

#[test]
fn explicit_stop_waits_for_the_service_process_not_just_the_stop_reply() {
    let checks = Cell::new(0);
    let unloaded = Cell::new(false);
    wait_for_host_exit(
        || Ok(false), // Stop replies report processRunning=false before AppKit exits.
        || {
            checks.set(checks.get() + 1);
            checks.get() < 3
        },
        || {
            assert_eq!(
                checks.get(),
                3,
                "must reap the service before unloading its job"
            );
            unloaded.set(true);
            Ok(())
        },
    )
    .unwrap();
    assert!(unloaded.get());
}

#[test]
fn an_absent_host_still_unloads_the_job_and_repeated_stops_succeed() {
    let unloads = Cell::new(0);
    for _ in 0..2 {
        wait_for_host_exit(
            || Ok(false),
            || false,
            || {
                unloads.set(unloads.get() + 1);
                Ok(())
            },
        )
        .unwrap();
    }
    assert_eq!(unloads.get(), 2);
}
