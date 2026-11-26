#[test]
fn framebuffer_update_batches_headers_and_pixels_until_explicit_flush() {
    use crate::remote_desktop_core::{
        FrameInterest, Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    #[derive(Default)]
    struct RecordedOutput {
        chunks: Vec<Vec<u8>>,
        flushes: usize,
    }
    impl Write for RecordedOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.chunks.push(bytes.to_vec());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }
    let core = ServerFramebufferCore::default();
    core.attach_client("batch-client").unwrap();
    core.publish(
        Framebuffer::new(
            FramebufferGeometry {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::Bgra8888,
            },
            vec![7; 8],
        )
        .unwrap(),
    )
    .unwrap();
    let frame = core
        .next_update("batch-client", FrameInterest::full())
        .unwrap()
        .unwrap();
    let mut output = RecordedOutput::default();
    write_core_framebuffer_update(
        &mut output,
        &frame,
        &LocalVncServerFrameCache::default(),
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Raw,
        &mut RfbZlibEncoder::new(),
    )
    .unwrap();
    assert_eq!(
        output.chunks,
        vec![vec![
            0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0, 7, 7, 7, 0, 7, 7, 7, 0
        ]]
    );
    assert_eq!(output.flushes, 1);
}

#[test]
fn failed_framebuffer_flush_does_not_retry_buffered_pixels_on_drop() {
    use crate::remote_desktop_core::{
        FrameInterest, Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    struct FailOnce {
        writes: usize,
    }
    impl Write for FailOnce {
        fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            if self.writes == 1 {
                Err(std::io::ErrorKind::TimedOut.into())
            } else {
                Ok(input.len())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let core = ServerFramebufferCore::default();
    core.attach_client("flush-client").unwrap();
    core.publish(
        Framebuffer::new(
            FramebufferGeometry {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::Bgra8888,
            },
            vec![7; 8],
        )
        .unwrap(),
    )
    .unwrap();
    let frame = core
        .next_update("flush-client", FrameInterest::full())
        .unwrap()
        .unwrap();
    let mut output = FailOnce { writes: 0 };
    let error = write_core_framebuffer_update(
        &mut output,
        &frame,
        &LocalVncServerFrameCache::default(),
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Raw,
        &mut RfbZlibEncoder::new(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(
        output.writes, 1,
        "failed pixels must never be retried by Drop"
    );
}

#[test]
fn active_framebuffer_writes_share_a_deadline_even_while_bytes_are_accepted() {
    use crate::remote_desktop_core::{
        FrameInterest, Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut sender, _) = listener.accept().unwrap();
    let stop = AtomicBool::new(false);
    let core = ServerFramebufferCore::default();
    core.attach_client("deadline-client").unwrap();
    let geometry = FramebufferGeometry {
        width: 512,
        height: 256,
        pixel_format: PixelFormat::Bgra8888,
    };
    core.publish(Framebuffer::new(geometry, vec![7; geometry.byte_len().unwrap()]).unwrap())
        .unwrap();
    let frame = core
        .next_update("deadline-client", FrameInterest::full())
        .unwrap()
        .unwrap();
    let cache = LocalVncServerFrameCache::default();
    let mut encoder = RfbZlibEncoder::new();
    let started = Instant::now();
    let mut output = RfbDeadlineIo {
        stream: &mut sender,
        stop_requested: &stop,
        deadline: started + Duration::from_millis(200),
    };
    // Reuse bounded pixels: a fixed payload may fit in the OS socket buffer.
    let error = loop {
        if let Err(error) = write_core_framebuffer_update(
            &mut output,
            &frame,
            &cache,
            default_rfb_pixel_format(),
            RfbFramebufferEncoding::Raw,
            &mut encoder,
        ) {
            break error;
        }
    };
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn expired_control_responses_cannot_emit_unbounded_wire_fragments() {
    for case in 0..6 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let (mut sender, _) = listener.accept().unwrap();
        let stop = AtomicBool::new(false);
        let mut output = RfbDeadlineIo {
            stream: &mut sender,
            stop_requested: &stop,
            deadline: Instant::now() - Duration::from_millis(1),
        };
        let result = match case {
            0 => write_end_of_continuous_updates(&mut output),
            1 => write_desktop_size_update(
                &mut output,
                DisplaySize {
                    width: 2,
                    height: 1,
                },
            ),
            2 => write_server_discovery_hello(&mut output),
            3 => write_extended_clipboard_caps(&mut output),
            4 => output.write_all(&[150]),
            _ => crate::server_discovery::handle_discover_request(
                &mut output,
                &crate::server_discovery::ServerDiscoveryContext::new(
                    "fixture".into(),
                    true,
                    None,
                    None,
                ),
                b"VWSD\x01\x02\x00\x00\x00\x01{}",
            ),
        };
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        assert!(peer.read(&mut [0]).is_err_and(|error| matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        )));
    }
}

#[test]
fn stop_during_buffered_framebuffer_write_aborts_and_joins_the_scoped_reader() {
    use crate::local_vnc_server::types::{
        DisplayFrame, LocalVncServerRefreshMode, LocalVncServerResolutionMode,
    };
    use crate::remote_desktop_core::{
        Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (mut sender, _) = listener.accept().unwrap();
    let geometry = FramebufferGeometry {
        width: 1920,
        height: 1080,
        pixel_format: PixelFormat::Bgra8888,
    };
    let core = Arc::new(ServerFramebufferCore::default());
    core.attach_client("stop-client").unwrap();
    core.publish_for_source(
        Framebuffer::new(geometry, vec![7; geometry.byte_len().unwrap()]).unwrap(),
        0,
    )
    .unwrap();
    let desktop_source =
        crate::local_vnc_server::desktop_relay::test_support::SourceFixture::new(7);
    let display = SharedDisplay {
        relay: Some(desktop_source.relay.clone()),
        framebuffer: DisplaySize {
            width: 1920,
            height: 1080,
        },
        revision: 0,
        desktop_epoch: 0,
        id: "stop-fixture".into(),
        label: "stop-fixture".into(),
        frame: DisplayFrame {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            pixel_width: 1920,
            pixel_height: 1080,
        },
        resolution_mode: LocalVncServerResolutionMode::Native,
        refresh_mode: LocalVncServerRefreshMode::Responsive,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let result = run_client_message_loop(
            &mut sender,
            &worker_stop,
            &RfbServerContext {
                auth_key: [0; 8],
                view_only: true,
                shared_display: Arc::new(RwLock::new(display.clone())),
                frame_cache: Arc::new(LocalVncServerFrameCache::default()),
                framebuffer_core: core,
                discovery: crate::server_discovery::ServerDiscoveryContext::new(
                    "fixture".into(),
                    true,
                    None,
                    None,
                ),
            },
            "stop-client".into(),
            display,
        );
        done_tx.send(result).unwrap();
    });
    peer.write_all(&[3, 0, 0, 0, 0, 0, 7, 128, 4, 56]).unwrap();
    let received = peer.read_exact(&mut [0; 16]);
    stop.store(true, Ordering::SeqCst);
    let stopped = done_rx.recv_timeout(Duration::from_secs(2));
    let _ = peer.shutdown(Shutdown::Both);
    worker.join().unwrap();
    received.unwrap();
    let result = stopped.expect("stop must abort buffered writes and join the reader");
    assert!(
        result.is_ok()
            || result.as_ref().is_err_and(|error| matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::Interrupted
            ))
    );
}
