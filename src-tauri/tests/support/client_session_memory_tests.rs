use super::*;

fn ready(width: u16, height: u16, memory: ClientMemoryBudget) -> RfbClientSessionCore {
    let framebuffer = Arc::new(ClientFramebufferHub::with_memory(memory));
    let mut session = RfbClientSessionCore::with_shared_framebuffer(None, framebuffer);
    session.consume(b"RFB 003.008\n").unwrap();
    session.consume(&[1, 1]).unwrap();
    session.consume(&0u32.to_be_bytes()).unwrap();
    let mut init = Vec::new();
    init.extend_from_slice(&width.to_be_bytes());
    init.extend_from_slice(&height.to_be_bytes());
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&0u32.to_be_bytes());
    session.consume(&init).unwrap();
    session.take_events();
    session
}

fn rectangle(width: u16, height: u16, encoding: i32, payload: &[u8]) -> Vec<u8> {
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0];
    message.extend_from_slice(&width.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message.extend_from_slice(&encoding.to_be_bytes());
    message.extend_from_slice(payload);
    message
}

#[test]
fn receive_and_decode_use_the_same_budget_and_admission_failure_preserves_the_frame() {
    let memory = ClientMemoryBudget::new(8192, 0);
    let mut session = ready(4, 2, memory.clone());
    session.consume(&rectangle(4, 2, 0, &[7; 32])).unwrap();
    let pressure = memory.reserve(8192 - memory.in_use() - 16).unwrap();
    assert_eq!(
        session.consume(&rectangle(4, 2, 0, &[9; 32])).unwrap_err(),
        "rfb_resource_limit:session_memory"
    );
    session
        .with_framebuffer(|core| {
            assert_eq!(core.latest_frame_id(), Some(1));
            assert_eq!(&core.pixels()[..3], &[7; 3]);
        })
        .unwrap();
    drop(pressure);
    drop(session);
    assert_eq!(memory.in_use(), 0);
}

#[test]
fn stopping_after_a_partial_connected_update_preserves_the_frame_and_releases_the_budget() {
    use std::io;
    struct PartialPeer<'a> {
        stop: &'a AtomicBool,
        partial: Vec<u8>,
        reads: usize,
        written: usize,
    }
    impl Read for PartialPeer<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            match self.reads {
                1 => {
                    buffer[..self.partial.len()].copy_from_slice(&self.partial);
                    Ok(self.partial.len())
                }
                2 => Err(io::ErrorKind::TimedOut.into()),
                3 => {
                    self.stop.store(true, Ordering::SeqCst);
                    Err(io::ErrorKind::TimedOut.into())
                }
                _ => panic!("transport read again after cancellation"),
            }
        }
    }
    impl Write for PartialPeer<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.written += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let memory = ClientMemoryBudget::new(16 * 1024, 0);
    let mut session = ready(4, 2, memory.clone());
    session.consume(&rectangle(4, 2, 0, &[7; 32])).unwrap();
    session.take_events();
    session.take_performance_metrics(Duration::from_secs(5));
    let stop = AtomicBool::new(false);
    let mut peer = PartialPeer {
        stop: &stop,
        partial: rectangle(4, 2, 0, &[9; 7]), // Header complete, 25 pixel bytes still missing.
        reads: 0,
        written: 0,
    };
    run_rfb_client_tcp_loop_until(&mut peer, &mut session, &stop).unwrap();
    assert_eq!(peer.reads, 3);
    assert_eq!(
        peer.written, 0,
        "an incomplete update must not trigger a new frame request"
    );
    assert!(session.take_events().is_empty());
    session
        .with_framebuffer(|core| {
            assert_eq!(core.latest_frame_id(), Some(1));
            assert_eq!(core.pixels(), [7, 7, 7, 255].repeat(8));
        })
        .unwrap();
    let metrics = session.take_performance_metrics(Duration::from_secs(5));
    assert_eq!(metrics.read_calls, 3);
    assert_eq!(metrics.read_timeouts, 2);
    assert_eq!(metrics.received_bytes, peer.partial.len() as u64);
    assert_eq!(metrics.updates, 0);
    assert!(memory.in_use() > 0);
    drop(session);
    assert_eq!(memory.in_use(), 0);
}

#[test]
fn pending_events_cannot_accumulate_unbounded_bell_notifications() {
    let mut session = ready(4, 2, ClientMemoryBudget::default());
    assert_eq!(
        session.consume(&[2; 257]).unwrap_err(),
        "rfb_resource_limit:events"
    );
    assert_eq!(session.take_events().len(), 256);
}

#[test]
fn invalid_discovery_fragments_disable_only_discovery_without_unbounded_state() {
    let mut session = ready(4, 2, ClientMemoryBudget::default());
    let fragment = encode_discovery_frames(
        DiscoveryFrameType::Manifest,
        7,
        &serde_json::json!({"text": "x".repeat(128)}),
    )
    .unwrap()
    .remove(0);
    let mut message = vec![248, 0, 0, 0];
    message.extend_from_slice(&FENCE_FLAG_REQUEST.to_be_bytes());
    message.push(fragment.len() as u8);
    message.extend_from_slice(&fragment);
    for _ in 0..1000 {
        session.consume(&message).unwrap();
    }
    let events = session.take_events();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], RfbClientSessionEvent::DiscoveryFailure(code) if code == "discovery_fragment_sequence_invalid")
    );
    assert!(session.discovery_frames.is_empty());
    session.consume(&rectangle(4, 2, 0, &[7; 32])).unwrap();
    assert!(
        session.is_presentable(),
        "discovery failure leaves the base framebuffer path usable"
    );
}

#[test]
fn cursor_headers_are_limited_before_receiving_or_decoding_their_pixel_payload() {
    let mut session = ready(4, 2, ClientMemoryBudget::default());
    let message = rectangle(1024, 1024, -239, &[]);
    assert_eq!(
        session.consume(&message).unwrap_err(),
        "rfb_resource_limit:cursor"
    );
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let geometry = FramebufferGeometry {
        width: 4,
        height: 2,
        pixel_format: super::super::PixelFormat::Bgra8888,
    };
    let mut core = ClientFramebufferCore::default();
    assert_eq!(
        decoder
            .decode_framebuffer_update(&message, geometry, &mut core)
            .unwrap_err(),
        "rfb_resource_limit:cursor"
    );
    assert!(core.snapshot().is_none());
}

#[test]
fn dual_4k_updates_and_a_surface_packet_fit_the_shared_production_budget() {
    let memory = ClientMemoryBudget::default();
    let mut session = ready(7680, 2160, memory.clone());
    session
        .framebuffer
        .lock()
        .unwrap()
        .attach_surface("main", true)
        .unwrap();
    for _ in 0..2 {
        // Keep the dual-4K geometry, but respect Tight's 2048-pixel width limit.
        let mut update = vec![0, 0, 0, 4];
        for (x, width) in [(0_u16, 2048_u16), (2048, 2048), (4096, 2048), (6144, 1536)] {
            update.extend(x.to_be_bytes());
            update.extend(0_u16.to_be_bytes());
            update.extend(width.to_be_bytes());
            update.extend(2160_u16.to_be_bytes());
            update.extend(7_i32.to_be_bytes());
            update.extend([0x80, 0x12, 0x34, 0x56]);
        }
        session.consume(&update).unwrap();
    }
    let core = session.framebuffer.lock().unwrap();
    let update = core.next_surface_update("main").unwrap().unwrap();
    let packet = super::super::SurfaceFramePacket::from_core(&core, &update).unwrap();
    let frame_bytes = 7680 * 2160 * 4;
    assert_eq!(core.pixels().len(), frame_bytes);
    assert_eq!(&core.pixels()[..4], &[0x56, 0x34, 0x12, 255]);
    assert_eq!(&core.pixels()[frame_bytes - 4..], &[0x56, 0x34, 0x12, 255]);
    assert_eq!(packet.bytes().len(), frame_bytes + 44);
    assert!(memory.in_use() <= super::super::memory::CLIENT_MEMORY_LIMIT);
    drop(packet);
    drop(core);
    drop(session);
    assert_eq!(memory.in_use(), 64 * 1024 * 1024);
}
