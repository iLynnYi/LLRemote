#[test]
fn server_reader_drains_extension_payloads_before_the_next_frame_request() {
    let mut resize = vec![251, 0, 0, 0, 0, 0, 1, 0];
    resize.extend([0; 16]);
    let packets = [
        vec![150, 1, 0, 1, 0, 2, 0, 3, 0, 4],
        vec![248, 0, 0, 0, 128, 0, 0, 0, 3, b'a', b'b', b'c'],
        vec![250, 0, 1, 2],
        resize,
    ];
    for packet in packets {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut sender = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut receiver, _) = listener.accept().unwrap();
        sender.write_all(&packet).unwrap();
        sender.write_all(&[3, 1, 0, 0, 0, 0, 0, 2, 0, 1]).unwrap();
        let stop = AtomicBool::new(false);
        let message = read_client_message(&mut receiver, &stop).unwrap();
        match (packet[0], message) {
            (150, Some(ClientMessage::Continuous { enabled, region })) => {
                assert!(enabled);
                assert_eq!(
                    region,
                    InterestRect {
                        x: 1,
                        y: 2,
                        width: 3,
                        height: 4
                    }
                );
            }
            (248, Some(ClientMessage::Fence { flags, payload })) => {
                assert_eq!(flags, 0x8000_0000);
                assert_eq!(payload, b"abc");
            }
            (250 | 251, None) => {}
            _ => panic!("unexpected extension result for {}", packet[0]),
        }
        match read_client_message(&mut receiver, &stop).unwrap() {
            Some(ClientMessage::FrameRequest(interest)) => {
                assert!(interest.incremental);
                assert_eq!(
                    interest.region,
                    Some(InterestRect {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 1
                    })
                );
            }
            _ => panic!("extension payload desynchronized the next frame request"),
        }
    }
}

#[test]
fn server_inbox_coalesces_full_requests_without_crossing_control_messages() {
    let inbox = ClientInbox::default();
    let stop = AtomicBool::new(false);
    let left = InterestRect {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    };
    let right = InterestRect {
        x: 4,
        y: 2,
        width: 2,
        height: 2,
    };
    inbox
        .push(
            ClientMessage::FrameRequest(FrameInterest::full_region(left)),
            &stop,
        )
        .unwrap();
    inbox
        .push(
            ClientMessage::FrameRequest(FrameInterest::incremental_region(right)),
            &stop,
        )
        .unwrap();
    inbox
        .push(ClientMessage::PixelFormat([0; 19]), &stop)
        .unwrap();
    inbox
        .push(
            ClientMessage::FrameRequest(FrameInterest::incremental_region(left)),
            &stop,
        )
        .unwrap();
    assert_eq!(inbox.state.lock().unwrap().queue.len(), 3);
    match inbox.next(Duration::ZERO).unwrap() {
        ClientInboxItem::Message(ClientMessage::FrameRequest(interest)) => {
            assert!(!interest.incremental);
            assert_eq!(
                interest.region,
                Some(InterestRect {
                    x: 0,
                    y: 0,
                    width: 6,
                    height: 4
                })
            );
        }
        _ => panic!("expected merged full request"),
    }
    assert!(matches!(
        inbox.next(Duration::ZERO).unwrap(),
        ClientInboxItem::Message(ClientMessage::PixelFormat(_))
    ));
    assert!(matches!(
        inbox.next(Duration::ZERO).unwrap(),
        ClientInboxItem::Message(ClientMessage::FrameRequest(_))
    ));
}

#[test]
fn server_inbox_bounds_bytes_and_close_releases_a_waiting_reader() {
    let inbox = Arc::new(ClientInbox::default());
    let stop = AtomicBool::new(false);
    for _ in 0..3 {
        inbox
            .push(
                ClientMessage::CutText {
                    signed_length: 1024 * 1024,
                    payload: vec![0; 1024 * 1024],
                },
                &stop,
            )
            .unwrap();
    }
    let worker_inbox = Arc::clone(&inbox);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let pushed = worker_inbox
            .push(
                ClientMessage::CutText {
                    signed_length: 1024 * 1024,
                    payload: vec![0; 1024 * 1024],
                },
                &AtomicBool::new(false),
            )
            .unwrap();
        done_tx.send(pushed).unwrap();
    });
    assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
    assert!(inbox.state.lock().unwrap().bytes <= MAX_CLIENT_INBOX_BYTES);
    inbox.close(None);
    assert!(!done_rx.recv_timeout(Duration::from_secs(1)).unwrap());
    worker.join().unwrap();
    assert_eq!(inbox.state.lock().unwrap().bytes, 0);
    let inbox = ClientInbox::default();
    for _ in 0..MAX_CLIENT_INBOX_MESSAGES {
        inbox
            .push(ClientMessage::PixelFormat([0; 19]), &stop)
            .unwrap();
    }
    assert_eq!(
        inbox.state.lock().unwrap().queue.len(),
        MAX_CLIENT_INBOX_MESSAGES
    );
    inbox.close(None);
    assert!(!inbox
        .push(ClientMessage::PixelFormat([0; 19]), &stop)
        .unwrap());
}

#[test]
fn server_reader_rejects_unknown_messages_and_lengths_before_payload_allocation() {
    for (wire, expected) in [
        (vec![255], std::io::ErrorKind::Unsupported),
        (
            [vec![6, 0, 0, 0], i32::MIN.to_be_bytes().to_vec()].concat(),
            std::io::ErrorKind::InvalidData,
        ),
        (vec![2, 0, 4, 1], std::io::ErrorKind::InvalidData),
        (
            vec![248, 0, 0, 0, 128, 0, 0, 0, 65],
            std::io::ErrorKind::InvalidData,
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        peer.write_all(&wire).unwrap();
        let error = read_client_message(&mut server, &AtomicBool::new(false))
            .err()
            .expect("reject at the message header");
        assert_eq!(error.kind(), expected);
    }
}

#[test]
fn server_fence_echoes_opaque_requests_and_keeps_discovery_separate() {
    let payload = vec![0x5a; 64];
    let wire = continuous_control_wire(vec![
        ClientMessage::Encodings((-312_i32).to_be_bytes().to_vec()),
        ClientMessage::Fence {
            flags: 0x8000_00ff,
            payload: payload.clone(),
        },
        ClientMessage::Fence {
            flags: 0,
            payload: payload.clone(),
        },
    ]);
    let hello = crate::server_discovery::hello_frame();
    let mut expected = vec![248, 0, 0, 0, 128, 0, 0, 0, hello.len() as u8];
    expected.extend(hello);
    expected.extend([248, 0, 0, 0, 0, 0, 0, 0, 64]);
    expected.extend(payload);
    assert_eq!(wire, expected);

    use crate::remote_desktop_core::{
        encode_discovery_frames, DiscoveryFrameType, RfbClientSessionCore, RfbClientSessionEvent,
    };
    let discover = encode_discovery_frames(
        DiscoveryFrameType::Discover,
        0,
        &serde_json::json!({"version":"1.0"}),
    )
    .unwrap()
    .remove(0);
    let wire = continuous_control_wire(vec![
        ClientMessage::Encodings((-312_i32).to_be_bytes().to_vec()),
        ClientMessage::Fence {
            flags: 1 << 31,
            payload: discover.clone(),
        },
    ]);
    let hello_length = 9 + crate::server_discovery::hello_frame().len();
    assert_eq!(
        &wire[hello_length..hello_length + 9],
        &[248, 0, 0, 0, 0, 0, 0, 0, discover.len() as u8]
    );
    assert_eq!(
        &wire[hello_length + 9..hello_length + 9 + discover.len()],
        discover
    );
    let mut session = RfbClientSessionCore::default();
    let mut init = b"RFB 003.008\n\x01\x01\0\0\0\0".to_vec();
    init.extend([
        0, 2, 0, 1, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    session.consume(&init).unwrap();
    session.take_events();
    session.consume(&wire).unwrap();
    let events = session.take_events();
    assert!(
        matches!(events.as_slice(), [RfbClientSessionEvent::DiscoveryManifest(value)] if value["server"]["id"] == "continuous-control")
    );
}

#[test]
fn server_extended_clipboard_inflation_has_a_byte_limit() {
    let payload = build_extended_clipboard_provide_payload(
        &"x".repeat(client_messages::MAX_CLIENT_CUT_TEXT_BYTES),
    );
    assert_eq!(
        decode_extended_clipboard_provide(&payload).unwrap_err(),
        "clipboard_resource_limit"
    );
}

#[test]
fn server_reannounces_discovery_once_when_user_agent_state_changes() {
    let context = crate::server_discovery::ServerDiscoveryContext::new(
        "continuous-control".into(),
        true,
        None,
        None,
    );
    let revision = context.revision_handle();
    let wire = continuous_control_wire_with_discovery(
        vec![ClientMessage::Encodings((-312_i32).to_be_bytes().to_vec())],
        context,
        Some(Box::new(move |inbox| {
            revision.store(1, Ordering::SeqCst);
            inbox
                .push(ClientMessage::PixelFormat([0; 19]), &AtomicBool::new(false))
                .unwrap();
        })),
    );
    let hello = crate::server_discovery::hello_frame();
    let mut expected = vec![248, 0, 0, 0, 128, 0, 0, 0, hello.len() as u8];
    expected.extend(hello);
    assert_eq!(wire, expected.repeat(2));
}

#[test]
fn discovery_changes_during_a_write_are_not_marked_as_already_announced() {
    struct ChangeDuringWrite(Arc<std::sync::atomic::AtomicU64>);
    impl Write for ChangeDuringWrite {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.store(1, Ordering::SeqCst);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let context =
        crate::server_discovery::ServerDiscoveryContext::new("fixture".into(), true, None, None);
    let announced =
        announce_server_discovery(&mut ChangeDuringWrite(context.revision_handle()), &context)
            .unwrap();
    assert_eq!(announced, 0);
    assert_eq!(context.revision(), 1);
}
