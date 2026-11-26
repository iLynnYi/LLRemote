use super::*;
use crate::background_share::types::{
    BackgroundShareOverview, BackgroundSharePermissionSource, BackgroundShareRuntime,
    StartupRegistration,
};
use crate::background_share::{
    normalize_overview, validate_host_protocol, BUILD_ID, IPC_VERSION, MIN_SUPPORTED_IPC_VERSION,
};
use crate::local_vnc_server::{pending_local_vnc_server_overview, LocalVncServerPermissionStatus};

fn overview() -> BackgroundShareOverview {
    BackgroundShareOverview {
        service_fallback: false,
        temporary_share: false,
        server: pending_local_vnc_server_overview(),
        configured: true,
        credential_configured: false,
        auto_start: false,
        startup_registration: StartupRegistration::Unknown,
        last_login_attempt: None,
        process_running: false,
        last_error_code: None,
        runtime: None,
        current_build_id: String::new(),
        permission_source: BackgroundSharePermissionSource::Pending,
        restart_supported: false,
        ipc_version: IPC_VERSION,
        permission_identity: None,
        signature_changed_permissions: vec![],
    }
}

#[test]
fn absent_background_cannot_report_foreground_grants() {
    let mut status = overview();
    for permission in &mut status.server.permissions {
        permission.status = LocalVncServerPermissionStatus::Granted;
    }
    let status = normalize_overview(status);
    assert_eq!(
        status.permission_source,
        BackgroundSharePermissionSource::Pending
    );
    assert!(status.runtime.is_none());
    assert_eq!(status.current_build_id, BUILD_ID);
    assert!(status
        .server
        .permissions
        .iter()
        .all(|permission| permission.status == LocalVncServerPermissionStatus::Unknown));
}

#[cfg(target_os = "macos")]
#[test]
fn permission_requests_refresh_only_an_idle_old_host_and_never_replace_a_listener() {
    let mut status = overview();
    status.process_running = true;
    status.runtime = Some(BackgroundShareRuntime {
        pid: 123,
        build_id: BUILD_ID.into(),
    });
    assert!(!crate::background_share::permission_host_needs_refresh(&status).unwrap());
    status.runtime.as_mut().unwrap().build_id = "old-permission-policy".into();
    assert!(crate::background_share::permission_host_needs_refresh(&status).unwrap());
    status.server.status = crate::local_vnc_server::LocalVncServerStatus::Running;
    assert_eq!(
        crate::background_share::permission_host_needs_refresh(&status).unwrap_err(),
        "background_share_runtime_outdated"
    );
    status.runtime = None;
    assert!(crate::background_share::permission_host_needs_refresh(&status).is_err());
}

#[test]
fn current_background_keeps_its_real_permission_results_and_pid() {
    let mut status = overview();
    status.process_running = true;
    status.runtime = Some(BackgroundShareRuntime {
        pid: 321,
        build_id: BUILD_ID.into(),
    });
    status.permission_source = BackgroundSharePermissionSource::Background;
    status.server.permissions[0].status = LocalVncServerPermissionStatus::Missing;
    let status = normalize_overview(status);
    assert_eq!(status.runtime.as_ref().unwrap().pid, 321);
    assert_eq!(
        status.permission_source,
        BackgroundSharePermissionSource::Background
    );
    assert_eq!(
        status.server.permissions[0].status,
        LocalVncServerPermissionStatus::Missing
    );
    validate_host_protocol(&status).unwrap();
    let mut outdated = status;
    outdated.runtime.as_mut().unwrap().build_id = "older-build".into();
    validate_host_protocol(&outdated).unwrap();
    outdated.ipc_version = 99;
    assert_eq!(
        validate_host_protocol(&outdated).unwrap_err(),
        "background_share_protocol_incompatible"
    );
}

#[test]
fn runtime_snapshot_normalization_keeps_the_cached_permission_source() {
    let mut status = overview();
    status.process_running = true;
    status.permission_source = BackgroundSharePermissionSource::BackgroundCached;
    status.server.permissions[0].status = LocalVncServerPermissionStatus::Granted;
    let normalized = normalize_overview(status);
    assert_eq!(
        normalized.permission_source,
        BackgroundSharePermissionSource::BackgroundCached
    );
    assert_eq!(
        normalized.server.permissions[0].status,
        LocalVncServerPermissionStatus::Granted
    );
}

#[test]
fn user_ipc_rejects_the_foreground_path_and_round_trips_request_framing() {
    let (mut client, mut server) = UnixStream::pair().unwrap();
    assert_eq!(
        verify_server(&client).unwrap_err(),
        "background_share_access_denied:peer_path_mismatch"
    );
    verify_user(&server).unwrap();
    write_message(&mut client, &Request::Status).unwrap();
    assert!(matches!(
        read_message::<Request>(&mut server).unwrap(),
        Request::Status
    ));
}

#[test]
fn user_ipc_rejects_oversized_frames_before_reading_or_allocating_the_body() {
    let (mut client, mut server) = UnixStream::pair().unwrap();
    client
        .write_all(&((MAX_MESSAGE_BYTES + 1) as u32).to_be_bytes())
        .unwrap();
    assert!(
        matches!(read_message::<Request>(&mut server), Err(code) if code == "background_share_invalid_request")
    );
}

#[test]
fn ipc_timeouts_do_not_masquerade_as_an_absent_process() {
    assert_eq!(
        ipc_io_error(std::io::Error::from(std::io::ErrorKind::TimedOut)),
        "background_share_ipc_busy"
    );
    assert_eq!(
        ipc_io_error(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        "background_share_ipc_busy"
    );
}

#[test]
fn host_protocol_requires_a_reported_version_within_the_supported_range() {
    use crate::background_share::host_requires_write_upgrade;

    let mut value = serde_json::to_value(overview()).unwrap();
    value.as_object_mut().unwrap().remove("ipcVersion");
    assert!(serde_json::from_value::<BackgroundShareOverview>(value).is_err());

    let mut status = overview();
    assert!(!host_requires_write_upgrade(&status).unwrap());
    for version in [MIN_SUPPORTED_IPC_VERSION - 1, IPC_VERSION + 1] {
        status.ipc_version = version;
        assert_eq!(
            host_requires_write_upgrade(&status).unwrap_err(),
            "background_share_protocol_incompatible"
        );
    }
}

#[test]
fn independent_auto_start_action_round_trips_through_ipc() {
    let request = Request::SetAutoStart { enabled: true };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(matches!(
        serde_json::from_slice::<Request>(&bytes).unwrap(),
        Request::SetAutoStart { enabled: true }
    ));
}

#[test]
fn controlled_sharing_rechecks_input_permission_but_view_only_needs_only_capture() {
    use crate::background_share::host::validate_start_permissions;
    let mut status = overview();
    status.server.permissions[0].status = LocalVncServerPermissionStatus::Granted;
    validate_start_permissions(&status.server.permissions, true).unwrap();
    assert!(
        validate_start_permissions(&status.server.permissions, false)
            .unwrap_err()
            .starts_with("permission_missing")
    );
    status.server.permissions[1].status = LocalVncServerPermissionStatus::Granted;
    validate_start_permissions(&status.server.permissions, false).unwrap();
    status.server.permissions[0].status = LocalVncServerPermissionStatus::Missing;
    assert!(validate_start_permissions(&status.server.permissions, true).is_err());
}
