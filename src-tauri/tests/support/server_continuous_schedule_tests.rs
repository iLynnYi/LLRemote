type AfterFirstOutput = Option<Box<dyn FnOnce(&ClientInbox)>>;

#[test]
fn continuous_updates_send_available_pixels_before_waiting_for_input_timeout() {
    run_continuous_schedule_case(false, false);
}

#[test]
fn continuous_updates_continue_while_a_pointer_message_is_incomplete() {
    run_continuous_schedule_case(true, false);
}

#[test]
fn framebuffer_request_waits_for_capture_instead_of_sending_empty_updates() {
    run_continuous_schedule_case(false, true);
}

#[test]
fn continuous_support_uses_one_initial_end_message_per_connection() {
    let region = InterestRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    assert_eq!(
        continuous_control_wire(vec![
            ClientMessage::Encodings((-313_i32).to_be_bytes().to_vec()),
            ClientMessage::Encodings((-313_i32).to_be_bytes().to_vec()),
            ClientMessage::Continuous {
                enabled: false,
                region
            },
            ClientMessage::Continuous {
                enabled: false,
                region
            },
        ]),
        [150, 150, 150]
    );
}

#[test]
fn continuous_mode_ignores_incremental_requests_but_honors_full_outside_region() {
    let left = InterestRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    let right = InterestRect { x: 1, ..left };
    let left_update = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 7, 7, 7, 0];
    let mut incremental_expected = left_update.clone();
    incremental_expected.push(150);
    assert_eq!(
        continuous_control_wire(vec![
            ClientMessage::Continuous {
                enabled: true,
                region: left
            },
            ClientMessage::FrameRequest(FrameInterest::incremental_region(right)),
            ClientMessage::Continuous {
                enabled: false,
                region: left
            },
        ]),
        incremental_expected
    );
    let wire = continuous_control_wire(vec![
        ClientMessage::Continuous {
            enabled: true,
            region: left,
        },
        ClientMessage::FrameRequest(FrameInterest::full_region(right)),
        ClientMessage::Continuous {
            enabled: false,
            region: left,
        },
    ]);
    let mut expected = left_update.clone();
    expected.extend([0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 7, 7, 7, 0]);
    expected.push(150);
    assert_eq!(wire, expected);
}

// Exercise the production sole writer with a bounded inbox and real loopback,
// then stop and join before asserting so failures never leave a worker running.
fn continuous_control_wire(messages: Vec<ClientMessage>) -> Vec<u8> {
    continuous_control_wire_with_discovery(
        messages,
        crate::server_discovery::ServerDiscoveryContext::new(
            "continuous-control".into(),
            true,
            None,
            None,
        ),
        None,
    )
}

fn continuous_control_wire_with_discovery(
    messages: Vec<ClientMessage>,
    discovery: crate::server_discovery::ServerDiscoveryContext,
    mut after_first_output: AfterFirstOutput,
) -> Vec<u8> {
    use crate::local_vnc_server::types::{
        DisplayFrame, LocalVncServerRefreshMode, LocalVncServerResolutionMode,
    };
    use crate::remote_desktop_core::{
        Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (mut sender, _) = listener.accept().unwrap();
    let desktop_source =
        crate::local_vnc_server::desktop_relay::test_support::SourceFixture::new(7)
            .without_secure_attention();
    let display = SharedDisplay {
        relay: Some(desktop_source.relay.clone()),
        revision: 0,
        desktop_epoch: 0,
        id: "continuous-control".into(),
        label: "continuous-control".into(),
        frame: DisplayFrame {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
            pixel_width: 2,
            pixel_height: 1,
        },
        framebuffer: DisplaySize {
            width: 2,
            height: 1,
        },
        resolution_mode: LocalVncServerResolutionMode::Native,
        refresh_mode: LocalVncServerRefreshMode::Responsive,
    };
    let core = Arc::new(ServerFramebufferCore::default());
    core.attach_client("continuous-control").unwrap();
    core.publish_for_source(
        Framebuffer::new(
            FramebufferGeometry {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::Bgra8888,
            },
            vec![7; 8],
        )
        .unwrap(),
        0,
    )
    .unwrap();
    let inbox = Arc::new(ClientInbox::new(
        core.client_update_signal("continuous-control").unwrap(),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    for message in messages {
        inbox.push(message, &stop).unwrap();
    }
    let worker_stop = Arc::clone(&stop);
    let worker_inbox = Arc::clone(&inbox);
    let worker = thread::spawn(move || {
        run_server_writer(
            &mut sender,
            &worker_stop,
            &RfbServerContext {
                auth_key: [0; 8],
                view_only: true,
                shared_display: Arc::new(RwLock::new(display.clone())),
                frame_cache: Arc::new(LocalVncServerFrameCache::default()),
                framebuffer_core: core,
                discovery,
            },
            "continuous-control".into(),
            display,
            &worker_inbox,
            &RwLock::new(None),
        )
    });
    let mut wire = Vec::new();
    let mut buffer = [0; 256];
    let read_result = loop {
        match peer.read(&mut buffer) {
            Ok(0) => break Ok(()),
            Ok(count) => {
                wire.extend_from_slice(&buffer[..count]);
                if let Some(callback) = after_first_output.take() {
                    callback(&inbox);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                break Ok(())
            }
            Err(error) => break Err(error),
        }
    };
    stop.store(true, Ordering::SeqCst);
    inbox.close(None);
    let result = worker.join().unwrap();
    read_result.unwrap();
    result.unwrap();
    wire
}

fn run_continuous_schedule_case(partial_pointer: bool, request_before_capture: bool) {
    use crate::local_vnc_server::types::{
        DisplayFrame, LocalVncServerRefreshMode, LocalVncServerResolutionMode,
    };
    use crate::remote_desktop_core::{
        Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let (mut sender, _) = listener.accept().unwrap();
    // Amplify the scheduling dependency without relying on platform timer granularity.
    sender
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let size = DisplaySize {
        width: 2,
        height: 1,
    };
    let desktop_source =
        crate::local_vnc_server::desktop_relay::test_support::SourceFixture::new(7);
    let display = SharedDisplay {
        relay: Some(desktop_source.relay.clone()),
        revision: 0,
        desktop_epoch: 0,
        id: "continuous-fixture".into(),
        label: "continuous-fixture".into(),
        frame: DisplayFrame {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
            pixel_width: 2,
            pixel_height: 1,
        },
        framebuffer: size,
        resolution_mode: LocalVncServerResolutionMode::Native,
        refresh_mode: LocalVncServerRefreshMode::Responsive,
    };
    let core = Arc::new(ServerFramebufferCore::default());
    core.attach_client("continuous-client").unwrap();
    if !request_before_capture {
        core.publish_for_source(
            Framebuffer::new(
                FramebufferGeometry {
                    width: 2,
                    height: 1,
                    pixel_format: PixelFormat::Bgra8888,
                },
                vec![7; 8],
            )
            .unwrap(),
            0,
        )
        .unwrap();
    }
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let mut input = vec![
        if request_before_capture { 3 } else { 150 },
        1,
        0,
        0,
        0,
        0,
        0,
        2,
        0,
        1,
    ];
    if partial_pointer {
        input.extend_from_slice(&[5, 0, 0]);
    } else {
        for _ in 0..64 {
            input.extend_from_slice(&[5, 0, 0, 0, 0, 0]);
        }
    }
    peer.write_all(&input).unwrap();
    let worker_core = Arc::clone(&core);
    let worker = thread::spawn(move || {
        run_client_message_loop(
            &mut sender,
            &worker_stop,
            &RfbServerContext {
                auth_key: [0; 8],
                view_only: true,
                shared_display: Arc::new(RwLock::new(display.clone())),
                frame_cache: Arc::new(LocalVncServerFrameCache::default()),
                framebuffer_core: worker_core,
                discovery: crate::server_discovery::ServerDiscoveryContext::new(
                    "continuous-server".into(),
                    true,
                    None,
                    None,
                ),
            },
            "continuous-client".into(),
            display,
        )
    });
    if request_before_capture {
        let mut probe = [0];
        let waiting = peer.read(&mut probe);
        core.publish_for_source(
            Framebuffer::new(
                FramebufferGeometry {
                    width: 2,
                    height: 1,
                    pixel_format: PixelFormat::Bgra8888,
                },
                vec![7; 8],
            )
            .unwrap(),
            0,
        )
        .unwrap();
        if !waiting.as_ref().is_err_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            )
        }) {
            stop.store(true, Ordering::SeqCst);
            let _ = peer.shutdown(Shutdown::Both);
            let _ = worker.join();
            panic!("a pending request must wait for capture rather than receive an empty update");
        }
    }
    let mut wire = [0; 24];
    let received = peer.read_exact(&mut wire).and_then(|_| {
        if partial_pointer {
            core.publish_for_source(
                Framebuffer::new(
                    FramebufferGeometry {
                        width: 2,
                        height: 1,
                        pixel_format: PixelFormat::Bgra8888,
                    },
                    vec![9; 8],
                )
                .unwrap(),
                0,
            )
            .unwrap();
            peer.read_exact(&mut wire)?;
        }
        Ok(())
    });
    stop.store(true, Ordering::SeqCst);
    peer.shutdown(Shutdown::Both).unwrap();
    let stopped = worker.join().unwrap();
    received.expect("available frames must not wait for a complete input message or input timeout");
    // Stop may cancel a queued read or the final writer flush after pixels arrive.
    assert!(
        stopped.is_ok()
            || stopped.as_ref().is_err_and(|error| matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted
            )),
        "unexpected cleanup kind: {:?}",
        stopped.as_ref().err().map(std::io::Error::kind)
    );
    assert_eq!(
        &wire[..16],
        &[0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0]
    );
    let value = if partial_pointer { 9 } else { 7 };
    assert_eq!(&wire[16..20], &[value, value, value, 0]);
}
