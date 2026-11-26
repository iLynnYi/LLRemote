use super::test_support::SourceFixture;
use super::*;
use crate::local_vnc_server::{LocalVncServerManager, LocalVncServerStartRequest};
use std::time::Instant;

fn start_server(relay: Arc<DesktopRelay>) -> (LocalVncServerManager, u16) {
    let manager = LocalVncServerManager::default();
    let overview = manager
        .start_service_broker(
            LocalVncServerStartRequest {
                bind_address: "127.0.0.1".into(),
                port: 0,
                password: "fixture".into(),
                view_only: false,
            },
            "fixture-broker".into(),
            relay,
        )
        .unwrap();
    (manager, overview.port)
}
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "desktop relay condition did not recover"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn desktop_replacement_preserves_the_authenticated_public_socket_and_releases_input() {
    let first = SourceFixture::new(17);
    let (worker, port) = start_server(first.relay.clone());
    let relay = Arc::new(DesktopRelay::default());
    relay.attach(port, "fixture".into()).unwrap();
    let (broker, public_port) = start_server(relay.clone());
    let frames = Arc::new(ClientFramebufferHub::default());
    let mut client =
        RfbClientSessionCore::with_shared_framebuffer(Some("fixture".into()), frames.clone());
    let mut stream = TcpStream::connect(("127.0.0.1", public_port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(25)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let socket = stream.try_clone().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let (commands, receiver) = mpsc::channel();
    let notices = Arc::new(Mutex::new(Vec::new()));
    let observed = notices.clone();
    let client_task = std::thread::spawn(move || {
        run_rfb_client_tcp_loop_with_commands_ready_and_events(
            &mut stream,
            &mut client,
            &stopping,
            Some(&receiver),
            || {},
            |event| observed.lock().unwrap().push(event),
        )
    });
    let pixel = || {
        frames
            .lock()
            .unwrap()
            .snapshot()
            .map(|frame| frame.pixels()[0])
    };
    until(|| pixel() == Some(17));
    let initial = broker.status().unwrap();
    assert_eq!(initial.active_connections, 1);

    relay.detach();
    worker.stop().unwrap();
    until(|| {
        notices.lock().unwrap().iter().any(|event| {
            matches!(
                event,
                RfbClientSessionEvent::ServerRuntime(ServerRuntimeMessage::Desktop {
                    desktop: DesktopAccessState::Unavailable,
                    ..
                })
            )
        })
    });
    commands
        .send(RfbClientCommand::Key {
            down: true,
            keysym: b'x' as u32,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(
        broker.status().unwrap().active_connections,
        1,
        "desktop replacement closed the public TCP connection"
    );

    let second = SourceFixture::new(93);
    let (replacement, next_port) = start_server(second.relay.clone());
    relay.attach(next_port, "wrong".into()).unwrap();
    until(|| !relay.attached());
    assert_eq!(
        broker.status().unwrap().active_connections,
        1,
        "failed local authentication closed a public client"
    );
    relay.attach(next_port, "fixture".into()).unwrap();
    until(|| pixel() == Some(93));
    assert_eq!(broker.status().unwrap().started_at, initial.started_at);
    assert_eq!(broker.status().unwrap().active_connections, 1);
    assert_eq!(
        notices
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, RfbClientSessionEvent::Ready(_)))
            .count(),
        1,
        "client reauthenticated instead of keeping its existing session"
    );
    assert!(
        second.inputs.try_recv().is_err(),
        "old desktop input was replayed on the new desktop"
    );

    commands
        .send(RfbClientCommand::Key {
            down: true,
            keysym: b'y' as u32,
        })
        .unwrap();
    until(
        || matches!(second.inputs.try_recv(), Ok(RfbClientCommand::Key { down: true, keysym }) if keysym == b'y' as u32),
    );
    stop.store(true, Ordering::Release);
    let _ = socket.shutdown(Shutdown::Both);
    let _ = client_task.join().unwrap();
    until(
        || matches!(second.inputs.try_recv(), Ok(RfbClientCommand::Key { down: false, keysym }) if keysym == b'y' as u32),
    );
    broker.stop().unwrap();
    relay.detach();
    replacement.stop().unwrap();
}

#[test]
fn desktop_recovery_requires_a_new_frame_and_rejects_the_previous_epoch() {
    let source = SourceFixture::new(7);
    assert!(source.relay.ready());
    let old = source.relay.with_frame(|epoch, _| epoch).unwrap();
    source.relay.consume(RfbClientSessionEvent::ServerRuntime(
        ServerRuntimeMessage::Desktop {
            desktop: DesktopAccessState::Unavailable,
            secure_attention_available: true,
        },
    ));
    assert!(!source.relay.accepts(old));
    source.relay.consume(RfbClientSessionEvent::ServerRuntime(
        ServerRuntimeMessage::Desktop {
            desktop: DesktopAccessState::Available,
            secure_attention_available: true,
        },
    ));
    assert!(
        source.relay.with_frame(|_, _| ()).is_none(),
        "cached pixels incorrectly cleared recovery"
    );
    source.publish(7); // Equal pixels still belong to a new desktop baseline.
    let next = source.relay.with_frame(|epoch, _| epoch).unwrap();
    assert_ne!(old, next);
    assert!(source.relay.accepts(next));
    source.relay.send(
        old,
        RfbClientCommand::Key {
            down: true,
            keysym: 65,
        },
    );
    assert!(source.inputs.try_recv().is_err());
}

#[test]
fn secure_attention_replies_are_correlated_and_replacement_cancels_pending_requests() {
    let source = SourceFixture::new(1);
    let relay = source.relay.clone();
    let request = std::thread::spawn(move || relay.secure_attention());
    let RfbClientCommand::SecureAttention { request_id } =
        source.inputs.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("expected SAS request");
    };
    source.relay.consume(RfbClientSessionEvent::ServerRuntime(
        ServerRuntimeMessage::SecureAttentionResponse {
            request_id: request_id + 1,
            result: SecureAttentionResult::Denied,
        },
    ));
    source.relay.consume(RfbClientSessionEvent::ServerRuntime(
        ServerRuntimeMessage::SecureAttentionResponse {
            request_id,
            result: SecureAttentionResult::Submitted,
        },
    ));
    assert_eq!(request.join().unwrap(), SecureAttentionResult::Submitted);
    let relay = source.relay.clone();
    let pending = std::thread::spawn(move || relay.secure_attention());
    source.inputs.recv_timeout(Duration::from_secs(1)).unwrap();
    source.relay.detach();
    assert_eq!(pending.join().unwrap(), SecureAttentionResult::Failed);
    assert!(!source.relay.ready());
}

#[test]
fn overflowing_input_stops_the_source_instead_of_losing_a_key_release() {
    let source = SourceFixture::new(1);
    let epoch = source.relay.with_frame(|epoch, _| epoch).unwrap();
    for _ in 0..257 {
        source.relay.send(
            epoch,
            RfbClientCommand::Key {
                down: true,
                keysym: 65,
            },
        );
    }
    assert!(!source.relay.ready());
    assert!(!source.relay.attached());
}
