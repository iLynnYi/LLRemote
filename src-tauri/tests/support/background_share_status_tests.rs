use super::*;
use crate::local_vnc_server::{
    LocalVncServerPermission, LocalVncServerPermissionKind, LocalVncServerPermissionStatus,
};

fn host() -> Host {
    Host {
        service_fallback: false,
        temporary_share: false,
        manager: LocalVncServerManager::default(),
        config: Config::default(),
        error: None,
        stopped: false,
        permissions: crate::local_vnc_server::pending_local_vnc_server_permissions(),
        permission_identity: None,
        permission_history: super::super::permission_identity::PermissionHistory::in_memory(),
    }
}

fn permission(status: LocalVncServerPermissionStatus) -> Vec<LocalVncServerPermission> {
    vec![LocalVncServerPermission {
        kind: LocalVncServerPermissionKind::ScreenRecording,
        status,
        required_for: "screen_capture".into(),
    }]
}

#[test]
fn enabling_login_share_rechecks_actual_macos_capture_permission() {
    let mut host = host();
    host.config.view_only = true;
    let reply = host.handle_with_permission_probe(Request::SetAutoStart { enabled: true }, || {
        permission(LocalVncServerPermissionStatus::Missing)
    });
    assert_eq!(
        reply.error_code.as_deref(),
        Some("background_share_auto_start_permission_required")
    );
    assert!(!host.config.auto_start);
    assert_eq!(
        host.permissions[0].status,
        LocalVncServerPermissionStatus::Missing
    );
}

#[test]
fn runtime_status_never_invokes_the_permission_probe() {
    let mut host = host();
    let reply = host.handle_with_permission_probe(Request::RuntimeStatus, || {
        panic!("runtime polling must not query OS permissions")
    });
    let overview = reply.into_result().unwrap();
    assert_eq!(
        overview.permission_source,
        BackgroundSharePermissionSource::BackgroundCached
    );
    assert!(overview.process_running);
    assert_eq!(overview.server.status, LocalVncServerStatus::Stopped);
    assert!(overview
        .server
        .permissions
        .iter()
        .all(|item| item.status == LocalVncServerPermissionStatus::Unknown));
}

#[test]
fn explicit_inspection_updates_the_snapshot_and_polling_keeps_it_until_the_next_inspection() {
    let mut host = host();
    let fresh = host
        .handle_with_permission_probe(Request::Status, || {
            permission(LocalVncServerPermissionStatus::Granted)
        })
        .into_result()
        .unwrap();
    assert_eq!(
        fresh.permission_source,
        BackgroundSharePermissionSource::Background
    );
    for _ in 0..3 {
        let cached = host
            .handle_with_permission_probe(Request::RuntimeStatus, || {
                panic!("a changed OS grant is inspected only on an explicit trigger")
            })
            .into_result()
            .unwrap();
        assert_eq!(
            cached.server.permissions[0].status,
            LocalVncServerPermissionStatus::Granted
        );
        assert!(cached.runtime.is_some());
    }
    let revoked = host
        .handle_with_permission_probe(Request::Status, || {
            permission(LocalVncServerPermissionStatus::Missing)
        })
        .into_result()
        .unwrap();
    assert_eq!(
        revoked.server.permissions[0].status,
        LocalVncServerPermissionStatus::Missing
    );
}

#[test]
fn actual_grant_checks_update_signature_history_but_runtime_refresh_does_not() {
    use super::super::permission_identity::PermissionIdentity;
    let mut host = host();
    let old = PermissionIdentity {
        identifier: "com.example.sharing".into(),
        cdhash: "old".into(),
    };
    host.permission_history.record(
        Some(&old),
        &permission(LocalVncServerPermissionStatus::Granted),
    );
    host.permission_identity = Some(PermissionIdentity {
        cdhash: "new".into(),
        ..old
    });
    let missing = host
        .handle_with_permission_probe(Request::Status, || {
            permission(LocalVncServerPermissionStatus::Missing)
        })
        .into_result()
        .unwrap();
    assert_eq!(
        missing.signature_changed_permissions,
        vec![LocalVncServerPermissionKind::ScreenRecording]
    );
    let cached = host
        .handle_with_permission_probe(Request::RuntimeStatus, || panic!("no OS check"))
        .into_result()
        .unwrap();
    assert_eq!(
        cached.signature_changed_permissions,
        missing.signature_changed_permissions
    );
    let granted = host
        .handle_with_permission_probe(Request::Status, || {
            permission(LocalVncServerPermissionStatus::Granted)
        })
        .into_result()
        .unwrap();
    assert!(granted.signature_changed_permissions.is_empty());
    let revoked = host
        .handle_with_permission_probe(Request::Status, || {
            permission(LocalVncServerPermissionStatus::Missing)
        })
        .into_result()
        .unwrap();
    assert!(
        revoked.signature_changed_permissions.is_empty(),
        "same-signature revocation is not an upgrade failure"
    );
}
