use std::sync::{Arc, Mutex};
use LLRemoteLib::remote_session::{
    CapabilityItem, CapabilityMatrix, EnhancedChannel, LocalEndpointMatch, RemoteSessionManager,
    RemoteSessionRfbConnectRequest, RemoteSessionRfbInputRequest, RemoteSessionStartRequest,
    RemoteSessionUpdateRequest, ServerIdentity, SessionBaseTransport, SessionConnectionModel,
};

fn start_test_session(manager: &RemoteSessionManager, id: &str) {
    manager
        .start(RemoteSessionStartRequest {
            session_id: id.into(),
            target: "loopback-fixture".into(),
            connection_model: model(),
            display_source: "vnc".into(),
            input_source: "vnc".into(),
        })
        .unwrap();
}

#[test]
fn recording_surface_never_becomes_the_selected_display_during_attach_or_dock() {
    use LLRemoteLib::remote_session::SessionSurfaceAttachRequest;
    let manager = RemoteSessionManager::default();
    manager
        .start(RemoteSessionStartRequest {
            session_id: "recording-ownership".into(),
            target: "fixture".into(),
            connection_model: model(),
            display_source: "vnc".into(),
            input_source: "vnc".into(),
        })
        .unwrap();
    let attach = |id: &str, owner: &str, active| {
        manager.attach_surface(SessionSurfaceAttachRequest {
            session_id: "recording-ownership".into(),
            surface_id: id.into(),
            owner: owner.into(),
            window_label: "fixture".into(),
            active,
        })
    };
    attach("main", "embedded", true).unwrap();
    attach("recording", "recording", false).unwrap();
    assert!(attach("invalid-recorder", "recording", true).is_err());
    attach("external", "external", true).unwrap();
    assert!(manager
        .set_active_surface("recording-ownership", "recording")
        .is_err());
    let snapshot = manager
        .get_snapshot("recording-ownership")
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.active_surface_id.as_deref(), Some("external"));
    let docked = manager
        .detach_surface("recording-ownership", "external")
        .unwrap();
    assert_eq!(docked.active_surface_id.as_deref(), Some("main"));
    let no_display = manager
        .detach_surface("recording-ownership", "main")
        .unwrap();
    assert_eq!(no_display.active_surface_id, None);
    assert_eq!(no_display.surfaces.len(), 1);
    manager.stop("recording-ownership").unwrap();
}

fn fixture_first_frame() -> Vec<u8> {
    let mut wire = b"RFB 003.008\n".to_vec();
    wire.extend_from_slice(&[1, 1, 0, 0, 0, 0]);
    wire.extend_from_slice(&[
        0, 1, 0, 1, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    wire.extend_from_slice(&[
        0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 30, 20, 10, 0,
    ]);
    wire
}

#[test]
fn duplicate_session_start_preserves_the_existing_owner() {
    let manager = RemoteSessionManager::default();
    start_test_session(&manager, "duplicate");
    assert_eq!(
        manager
            .start(RemoteSessionStartRequest {
                session_id: "duplicate".into(),
                target: "replacement".into(),
                connection_model: model(),
                display_source: "vnc".into(),
                input_source: "vnc".into(),
            })
            .unwrap_err(),
        "remote_session_already_started"
    );
    assert_eq!(
        manager.get_snapshot("duplicate").unwrap().unwrap().target,
        "loopback-fixture"
    );
    start_test_session(&manager, "duplicate");
    assert_eq!(manager.active_session_count().unwrap(), 1);
}

#[test]
fn disconnect_closes_the_worker_socket_without_a_terminal_event() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;
    let manager = RemoteSessionManager::default();
    start_test_session(&manager, "cancel");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (closed_tx, closed_rx) = mpsc::channel();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(&fixture_first_frame()).unwrap();
        let mut bytes = [0; 4096];
        while stream.read(&mut bytes).unwrap() != 0 {}
        closed_tx.send(()).unwrap();
    });
    let (events_tx, events_rx) = mpsc::channel();
    manager
        .connect_rfb(
            RemoteSessionRfbConnectRequest {
                session_id: "cancel".into(),
                host: "127.0.0.1".into(),
                port,
                password: None,
            },
            move |event| {
                events_tx.send(event).unwrap();
            },
        )
        .unwrap();
    manager.disconnect_rfb("cancel").unwrap();
    closed_rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert!(
        events_rx.try_recv().is_err(),
        "cancelled worker must not emit Disconnected"
    );
    peer.join().unwrap();
}

#[test]
fn reconnect_waits_for_a_new_frame_instead_of_reusing_the_old_generation() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;
    let manager = Arc::new(RemoteSessionManager::default());
    start_test_session(&manager, "retry-frame");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (banner_tx, banner_rx) = mpsc::channel();
    let peer = std::thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        first
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        first.write_all(&fixture_first_frame()).unwrap();
        let mut bytes = [0; 4096];
        while first.read(&mut bytes).unwrap() != 0 {}
        let (mut second, _) = listener.accept().unwrap();
        second
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        second.write_all(b"RFB 003.008\n").unwrap();
        banner_tx.send(()).unwrap();
        while second.read(&mut bytes).unwrap() != 0 {}
    });
    let request = RemoteSessionRfbConnectRequest {
        session_id: "retry-frame".into(),
        host: "127.0.0.1".into(),
        port,
        password: None,
    };
    manager.connect_rfb(request.clone(), |_| {}).unwrap();
    manager.disconnect_rfb("retry-frame").unwrap();
    let next_manager = Arc::clone(&manager);
    let (result_tx, result_rx) = mpsc::channel();
    let retry = std::thread::spawn(move || {
        result_tx
            .send(next_manager.connect_rfb(request, |_| {}))
            .unwrap();
    });
    banner_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let premature = result_rx.recv_timeout(Duration::from_millis(100));
    manager.disconnect_rfb("retry-frame").unwrap();
    let terminal = if let Ok(result) = &premature {
        result.clone()
    } else {
        result_rx.recv_timeout(Duration::from_secs(1)).unwrap()
    };
    retry.join().unwrap();
    peer.join().unwrap();
    assert!(
        premature.is_err(),
        "a previous framebuffer cannot prove a new connection ready: {premature:?}"
    );
    assert!(terminal.is_err());
}

#[test]
fn cancelling_a_silent_handshake_preserves_disconnected_state() {
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;
    let manager = Arc::new(RemoteSessionManager::default());
    start_test_session(&manager, "silent");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        accepted_tx.send(()).unwrap();
        assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    });
    let connect_manager = Arc::clone(&manager);
    let connect = std::thread::spawn(move || {
        connect_manager.connect_rfb(
            RemoteSessionRfbConnectRequest {
                session_id: "silent".into(),
                host: "127.0.0.1".into(),
                port,
                password: None,
            },
            |_| {},
        )
    });
    accepted_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    manager.disconnect_rfb("silent").unwrap();
    assert_eq!(connect.join().unwrap().unwrap_err(), "rfb_cancelled");
    assert_eq!(
        manager.get_snapshot("silent").unwrap().unwrap().state,
        "disconnected"
    );
    peer.join().unwrap();
}

fn model() -> SessionConnectionModel {
    SessionConnectionModel {
        base_transport: SessionBaseTransport { kind: "vnc".into() },
        server_identity: ServerIdentity {
            id: None,
            kind: "unknown".into(),
            verification: "none".into(),
            vendor: None,
            product: None,
            version: None,
            protocol_version: None,
            hint: None,
        },
        local_endpoint_match: LocalEndpointMatch {
            state: "not-matched".into(),
            view_only: None,
            selected_display_id: None,
            framebuffer_width: None,
            framebuffer_height: None,
        },
        enhanced_channel: EnhancedChannel {
            status: "unavailable".into(),
            endpoint: None,
            authentication: "not-attempted".into(),
            last_error_code: Some("discovery_not_negotiated".into()),
        },
        capabilities: CapabilityMatrix {
            items: vec![CapabilityItem {
                id: "framebuffer".into(),
                state: "supported".into(),
                source: "rfb".into(),
                reason_code: None,
            }],
        },
    }
}

#[test]
fn core_keeps_connection_model_atomic() {
    let manager = RemoteSessionManager::default();
    let started = manager
        .start(RemoteSessionStartRequest {
            session_id: "s".into(),
            target: "host:5900".into(),
            connection_model: model(),
            display_source: "vnc".into(),
            input_source: "vnc".into(),
        })
        .expect("start");
    assert_eq!(started.connection_model.base_transport.kind, "vnc");
    let updated = manager
        .update(RemoteSessionUpdateRequest {
            session_id: "s".into(),
            state: Some("connected".into()),
            connection_model: Some(model()),
            display_source: None,
            input_source: None,
        })
        .expect("update");
    assert_eq!(updated.state, "connected");
}

#[test]
fn rfb_input_request_accepts_frontend_camel_case_variant_fields() {
    let quality = serde_json::from_value::<RemoteSessionRfbInputRequest>(serde_json::json!({
        "sessionId": "s",
        "input": {
            "type": "setQuality",
            "qualityLevel": 9,
            "compressionLevel": 0
        }
    }));
    assert!(
        quality.is_ok(),
        "frontend quality command must deserialize: {quality:?}"
    );

    let pointer = serde_json::from_value::<RemoteSessionRfbInputRequest>(serde_json::json!({
        "sessionId": "s",
        "input": {
            "type": "pointer",
            "x": 12,
            "y": 34,
            "buttonMask": 1
        }
    }));
    assert!(
        pointer.is_ok(),
        "frontend pointer command must deserialize: {pointer:?}"
    );
}

#[test]
fn presentation_connecting_state_does_not_impersonate_a_running_rfb_transport() {
    let manager = RemoteSessionManager::default();
    let startup_events = Arc::new(Mutex::new(0_u32));
    manager
        .start(RemoteSessionStartRequest {
            session_id: "transport-state-separation".into(),
            target: "127.0.0.1:0".into(),
            connection_model: model(),
            display_source: "vnc".into(),
            input_source: "vnc".into(),
        })
        .expect("start");
    manager
        .update(RemoteSessionUpdateRequest {
            session_id: "transport-state-separation".into(),
            state: Some("connecting".into()),
            connection_model: None,
            display_source: None,
            input_source: None,
        })
        .expect("mirror presentation state");

    let captured_startup_events = Arc::clone(&startup_events);
    let error = manager
        .connect_rfb(
            RemoteSessionRfbConnectRequest {
                session_id: "transport-state-separation".into(),
                host: "127.0.0.1".into(),
                port: 0,
                password: None,
            },
            move |_| {
                *captured_startup_events.lock().expect("startup events") += 1;
            },
        )
        .expect_err("port zero must fail after the native transport attempts startup");

    assert_eq!(error, "rfb_tcp_connect_failed");
    assert_eq!(*startup_events.lock().expect("startup events"), 0);

    let retry_error = manager
        .connect_rfb(
            RemoteSessionRfbConnectRequest {
                session_id: "transport-state-separation".into(),
                host: "127.0.0.1".into(),
                port: 0,
                password: None,
            },
            |_| {},
        )
        .expect_err("a failed native startup must release the transport slot");
    assert_eq!(retry_error, "rfb_tcp_connect_failed");
}

#[test]
fn connect_request_debug_does_not_expose_its_transient_password() {
    let secret = uuid::Uuid::new_v4().to_string();
    let request = RemoteSessionRfbConnectRequest {
        session_id: "redaction-check".into(),
        host: "127.0.0.1".into(),
        port: 5900,
        password: Some(secret.clone()),
    };
    let debug = format!("{request:?}");
    assert!(
        !debug.contains(&secret),
        "Debug must redact connection credentials"
    );
}

#[test]
fn runtime_disconnect_serializes_the_same_structured_failure_as_the_command_boundary() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::Duration,
    };
    use LLRemoteLib::remote_session::{
        RemoteSessionManager, RemoteSessionRfbConnectRequest, RemoteSessionRfbEvent, RfbFailure,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket.write_all(b"RFB 003.008\n").unwrap();
        socket.read_exact(&mut [0; 12]).unwrap();
        socket.write_all(&[1, 1]).unwrap();
        socket.read_exact(&mut [0; 1]).unwrap();
        socket.write_all(&[0; 4]).unwrap();
        socket.read_exact(&mut [0; 1]).unwrap();
        let init = [
            0, 1, 0, 1, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        socket.write_all(&init).unwrap();
        socket.read_exact(&mut [0; 20]).unwrap();
        let mut encodings = [0; 4];
        socket.read_exact(&mut encodings).unwrap();
        let count = u16::from_be_bytes([encodings[2], encodings[3]]) as usize;
        socket.read_exact(&mut vec![0; count * 4]).unwrap();
        socket.read_exact(&mut [0; 10]).unwrap();
        socket
            .write_all(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 2, 3, 0])
            .unwrap();
        socket.read_exact(&mut [0; 10]).unwrap();
        socket.write_all(&[99]).unwrap();
    });
    let manager = RemoteSessionManager::default();
    start_test_session(&manager, "failure-event-check");
    let (sender, receiver) = mpsc::channel();
    manager
        .connect_rfb(
            RemoteSessionRfbConnectRequest {
                session_id: "failure-event-check".into(),
                host: "127.0.0.1".into(),
                port,
                password: None,
            },
            move |event| {
                sender.send(event).unwrap();
            },
        )
        .unwrap();
    let event = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    let RemoteSessionRfbEvent::Disconnected { reason, failure } = event else {
        panic!("expected terminal event")
    };
    assert_eq!(
        failure,
        RfbFailure::from_internal("rfb_unsupported_server_message_type:99")
    );
    assert_eq!(reason, failure.code);
    assert_eq!(
        serde_json::to_value(&failure).unwrap()["category"],
        "protocol_error"
    );
    manager.stop("failure-event-check").unwrap();
    peer.join().unwrap();
}
