use super::*;
use std::cell::RefCell;

fn identity(hash: &str) -> PermissionIdentity {
    PermissionIdentity {
        identifier: "com.example.sharing".into(),
        cdhash: hash.into(),
    }
}
fn permissions(screen: Status, input: Status) -> Vec<LocalVncServerPermission> {
    vec![
        LocalVncServerPermission {
            kind: Kind::ScreenRecording,
            status: screen,
            required_for: "screen_capture".into(),
        },
        LocalVncServerPermission {
            kind: Kind::Accessibility,
            status: input,
            required_for: "input_injection".into(),
        },
    ]
}
fn history() -> PermissionHistory {
    PermissionHistory {
        path: None,
        grants: Grants::default(),
    }
}

#[test]
fn signature_difference_only_suggests_repair_for_a_previously_granted_missing_resource() {
    let mut h = history();
    h.record(
        Some(&identity("old")),
        &permissions(Status::Granted, Status::Missing),
    );
    assert_eq!(
        h.signature_changed(
            Some(&identity("new")),
            &permissions(Status::Missing, Status::Missing)
        ),
        vec![Kind::ScreenRecording]
    );
    assert!(h
        .signature_changed(
            Some(&identity("old")),
            &permissions(Status::Missing, Status::Missing)
        )
        .is_empty());
    assert!(h
        .signature_changed(
            Some(&identity("new")),
            &permissions(Status::Granted, Status::Missing)
        )
        .is_empty());
    assert!(h
        .signature_changed(None, &permissions(Status::Missing, Status::Missing))
        .is_empty());
    h.record(
        Some(&identity("new")),
        &permissions(Status::Missing, Status::Granted),
    );
    assert_eq!(h.grant(&Kind::ScreenRecording), Some(&identity("old")));
    assert_eq!(h.grant(&Kind::Accessibility), Some(&identity("new")));
}

#[test]
fn identity_history_survives_restart_without_saving_permission_booleans() {
    let directory =
        std::env::temp_dir().join(format!("vnc-permission-identity-{}", uuid::Uuid::new_v4()));
    let path = directory.join("permission-identities.json");
    let mut h = PermissionHistory {
        path: Some(path.clone()),
        grants: Grants::default(),
    };
    h.record(
        Some(&identity("old")),
        &permissions(Status::Granted, Status::Granted),
    );
    let stored = std::fs::read(&path).unwrap();
    let h = PermissionHistory {
        path: Some(path.clone()),
        grants: serde_json::from_slice(&stored).unwrap(),
    };
    assert_eq!(
        h.signature_changed(
            Some(&identity("new")),
            &permissions(Status::Granted, Status::Missing)
        ),
        vec![Kind::Accessibility]
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(directory).unwrap();
}

fn overview() -> super::super::types::BackgroundShareOverview {
    super::super::types::BackgroundShareOverview {
        server: crate::local_vnc_server::pending_local_vnc_server_overview(),
        service_fallback: false,
        temporary_share: false,
        configured: false,
        credential_configured: false,
        auto_start: false,
        startup_registration: Default::default(),
        last_login_attempt: None,
        process_running: true,
        last_error_code: None,
        runtime: None,
        current_build_id: super::super::BUILD_ID.into(),
        permission_source: Default::default(),
        restart_supported: true,
        ipc_version: super::super::IPC_VERSION,
        permission_identity: None,
        signature_changed_permissions: vec![],
    }
}

#[test]
fn explicit_repair_targets_one_permission_and_restores_even_if_reset_fails() {
    for fail in [false, true] {
        let mut o = overview();
        o.permission_identity = Some(identity("new"));
        o.server.permissions = permissions(Status::Granted, Status::Missing);
        let calls = RefCell::new(vec![]);
        let result = repair_with_actions(
            &o,
            &Kind::Accessibility,
            "com.example.sharing",
            || {
                calls.borrow_mut().push("stop");
                Ok(())
            },
            |kind, id| {
                assert_eq!(*kind, Kind::Accessibility);
                assert_eq!(id, "com.example.sharing");
                calls.borrow_mut().push("reset");
                if fail {
                    Err("background_share_permission_reset_failed".into())
                } else {
                    Ok(())
                }
            },
            || {
                calls.borrow_mut().push("restore");
                Ok(())
            },
        );
        assert_eq!(calls.into_inner(), vec!["stop", "reset", "restore"]);
        assert_eq!(result.is_err(), fail);
    }
}

#[test]
fn granted_unknown_active_or_unidentified_hosts_do_not_reset() {
    for scenario in [
        "granted",
        "unknown",
        "active",
        "no_identity",
        "wrong_identity",
    ] {
        let mut o = overview();
        o.permission_identity = Some(identity("new"));
        o.server.permissions = permissions(Status::Granted, Status::Missing);
        match scenario {
            "granted" => o.server.permissions[1].status = Status::Granted,
            "unknown" => o.server.permissions[1].status = Status::Unknown,
            "active" => o.server.status = crate::local_vnc_server::LocalVncServerStatus::Running,
            "no_identity" => o.permission_identity = None,
            "wrong_identity" => {
                o.permission_identity.as_mut().unwrap().identifier = "another.app".into()
            }
            _ => unreachable!(),
        }
        let result = repair_with_actions(
            &o,
            &Kind::Accessibility,
            "com.example.sharing",
            || panic!("must not stop"),
            |_, _| panic!("must not reset"),
            || panic!("must not restore"),
        );
        assert_eq!(result.is_ok(), scenario == "granted");
    }
}

#[test]
fn reset_command_is_always_scoped_to_one_resource_and_one_bundle() {
    for (kind, service) in [
        (Kind::ScreenRecording, "ScreenCapture"),
        (Kind::Accessibility, "Accessibility"),
    ] {
        let command = reset_command(&kind, "com.example.sharing").unwrap();
        assert_eq!(command.get_program(), "/usr/bin/tccutil");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec!["reset", service, "com.example.sharing"]
        );
    }
}

#[test]
fn screen_repair_resets_both_possible_tcc_owners_for_a_packaged_helper() {
    let mut calls = Vec::new();
    reset_for_host_with(
        &Kind::ScreenRecording,
        "com.example.LLRemote.sharing",
        Some("com.example.LLRemote"),
        |kind, identifier| {
            calls.push((kind.clone(), identifier.to_string()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        calls,
        vec![
            (Kind::ScreenRecording, "com.example.LLRemote.sharing".into()),
            (Kind::ScreenRecording, "com.example.LLRemote".into()),
        ]
    );
    let mut calls = Vec::new();
    reset_for_host_with(
        &Kind::Accessibility,
        "com.example.LLRemote.sharing",
        Some("com.example.LLRemote"),
        |_, identifier| {
            calls.push(identifier.to_string());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls, ["com.example.LLRemote.sharing"]);
    let mut calls = Vec::new();
    reset_for_host_with(
        &Kind::ScreenRecording,
        "com.example.LLRemote.sharing",
        None,
        |_, identifier| {
            calls.push(identifier.to_string());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls, ["com.example.LLRemote.sharing"]);
}

#[test]
fn stop_failure_never_resets_or_restores_a_host_that_may_still_be_sharing() {
    let mut o = overview();
    o.permission_identity = Some(identity("new"));
    o.server.permissions = permissions(Status::Missing, Status::Missing);
    let error = repair_with_actions(
        &o,
        &Kind::ScreenRecording,
        "com.example.sharing",
        || Err("background_share_already_running".into()),
        |_, _| panic!("must not reset after a stop refusal"),
        || panic!("must not start another host"),
    )
    .unwrap_err();
    assert_eq!(error, "background_share_already_running");
}
