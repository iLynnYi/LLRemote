use super::*;

#[test]
fn repair_report_from_an_unexpected_helper_is_rejected_before_reading_payload() {
    let name = format!(r"\\.\pipe\LLRemoteRepairTest-{}", uuid::Uuid::new_v4());
    let server = create_server(&name, "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)").unwrap();
    let _client = open_test_client(&name).unwrap();
    let wrong_pid =
        unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() }.wrapping_add(1);
    assert_eq!(
        receive_repair_report(server.0, wrong_pid, || Ok(())).unwrap_err(),
        "windows_service_access_denied"
    );
}

#[test]
fn waiting_for_a_helper_preserves_the_exit_reason_before_a_pipe_connects() {
    let name = format!(r"\\.\pipe\LLRemoteRepairExit-{}", uuid::Uuid::new_v4());
    let server = create_server(&name, "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)").unwrap();
    let mut polls = 0;
    let result = receive_repair_report(server.0, 0, || {
        polls += 1;
        Err("windows_service_admin_required".into())
    });
    assert_eq!(result.unwrap_err(), "windows_service_admin_required");
    assert_eq!(polls, 1);
}

#[test]
fn repair_never_runs_for_a_pipe_owned_by_the_wrong_app() {
    let id = uuid::Uuid::new_v4().to_string();
    let _server = create_server(
        &relay_name(&id).unwrap(),
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)",
    )
    .unwrap();
    let wrong_pid =
        unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() }.wrapping_add(1);
    assert_eq!(
        send_repair_report(&id, wrong_pid, || panic!(
            "repair cannot run before authentication"
        ))
        .unwrap_err(),
        "windows_service_access_denied"
    );
}

#[test]
fn disconnect_connection_request_round_trips_through_service_wire_format() {
    let request = Request::DisconnectConnection {
        connection_id: "ip:192.168.85.1".into(),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    let decoded: Request = serde_json::from_slice(&bytes).unwrap();
    assert!(
        matches!(decoded, Request::DisconnectConnection { connection_id }
        if connection_id == "ip:192.168.85.1")
    );
}

#[test]
fn an_abandoned_client_does_not_keep_the_service_pipe_busy() {
    let name = format!(r"\\.\pipe\LLRemotePipeTest-{}", uuid::Uuid::new_v4());
    let server = create_server(&name, "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)").unwrap();
    assert!(!connected(server.0).unwrap());
    let client = open_test_client(&name).unwrap();
    drop(client);

    // The client disappears before the broker accepts its request. Subsequent
    // heartbeat/status callers must still be able to use this same instance.
    assert!(!connected(server.0).unwrap());
    assert!(!connected(server.0).unwrap());
    let next = open_test_client(&name);
    assert!(next.is_ok(), "abandoned client left the instance busy");
    assert!(connected(server.0).unwrap());
}

fn open_test_client(name: &str) -> Result<Handle, u32> {
    let raw = unsafe {
        CreateFileW(
            native::wide(name).as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    Handle::new(raw, "unused").map_err(|_| native::last_error())
}
