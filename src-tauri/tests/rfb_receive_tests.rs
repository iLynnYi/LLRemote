use std::io::Write;
use LLRemoteLib::remote_desktop_core::RfbClientSessionCore;

fn server_init(width: u16, height: u16, name_length: u32) -> Vec<u8> {
    let mut bytes = Vec::from(*b"RFB 003.008\n");
    bytes.extend_from_slice(&[1, 1, 0, 0, 0, 0]);
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    bytes.extend_from_slice(&name_length.to_be_bytes());
    bytes
}

fn ready() -> RfbClientSessionCore {
    let mut session = RfbClientSessionCore::default();
    session.consume(&server_init(2, 1, 0)).unwrap();
    session
}

#[test]
fn initialization_requests_canonical_pixels_independently_of_the_server_default() {
    use LLRemoteLib::remote_desktop_core::{
        PixelFormat, RfbClientHandshakeEvent, RfbClientSessionEvent,
    };
    const REQUEST_FORMAT: [u8; 20] = [
        0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0,
    ];
    let default_formats = [
        [8, 8, 0, 1, 0, 7, 0, 7, 0, 3, 5, 2, 0, 0, 0, 0],
        [16, 16, 0, 1, 0, 31, 0, 63, 0, 31, 11, 5, 0, 0, 0, 0],
        [16, 16, 1, 1, 0, 31, 0, 63, 0, 31, 11, 5, 0, 0, 0, 0],
        [8, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [32, 24, 1, 1, 0, 255, 0, 255, 0, 255, 24, 16, 8, 0, 0, 0],
    ];
    for minor in [3, 7, 8] {
        let banner = format!("RFB 003.00{minor}\n").into_bytes();
        for default in default_formats {
            let mut init = banner.clone();
            if minor == 3 {
                init.extend(1_u32.to_be_bytes());
            } else {
                init.extend([1, 1]);
                if minor == 8 {
                    init.extend(0_u32.to_be_bytes());
                }
            }
            init.extend([0, 2, 0, 1]);
            init.extend(default);
            init.extend(6_u32.to_be_bytes());
            init.extend(b"format");
            let mut first = vec![0, 0, 0, 1];
            first.extend(rectangle(0, 2, 1, 0));
            first.extend([0x56, 0x34, 0x12, 0, 0x98, 0x76, 0x54, 17]);
            let mut incremental = vec![0, 0, 0, 1];
            incremental.extend(rectangle(1, 1, 1, 1));
            incremental.extend([0, 0, 0, 0]);
            incremental.push(2);
            for (stage, length) in [
                ("init", init.len()),
                ("first", first.len()),
                ("copy", incremental.len()),
            ] {
                for split in 0..=length {
                    let mut session = RfbClientSessionCore::default();
                    let feed = |session: &mut RfbClientSessionCore, bytes: &[u8], current| {
                        if stage == current {
                            let mut writes = session.consume(&bytes[..split]).unwrap();
                            writes.extend(session.consume(&bytes[split..]).unwrap());
                            writes
                        } else {
                            session.consume(bytes).unwrap()
                        }
                    };
                    let writes = feed(&mut session, &init, "init");
                    let setup = if minor == 3 { 2 } else { 3 };
                    assert_eq!(writes.len(), setup + 3);
                    assert_eq!(writes[0], banner);
                    assert!(writes[1..setup].iter().all(|bytes| bytes == &[1]));
                    assert_eq!(writes[setup], REQUEST_FORMAT);
                    let encodings = &writes[setup + 1];
                    assert_eq!(encodings[..2], [2, 0]);
                    let count = u16::from_be_bytes(encodings[2..4].try_into().unwrap()) as usize;
                    assert_eq!(encodings.len(), 4 + count * 4);
                    let ids: Vec<_> = encodings[4..]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|bytes| i32::from_be_bytes(*bytes))
                        .collect();
                    assert!(ids.contains(&0) && ids.contains(&1));
                    assert_eq!(writes[setup + 2], [3, 0, 0, 0, 0, 0, 0, 2, 0, 1]);
                    assert!(!session.is_presentable());
                    let events = session.take_events();
                    let [RfbClientSessionEvent::Ready(RfbClientHandshakeEvent::Ready {
                        geometry,
                        desktop_name,
                        pixel_format,
                    })] = events.as_slice()
                    else {
                        panic!("missing Ready event")
                    };
                    assert_eq!(geometry.pixel_format, PixelFormat::Bgra8888);
                    assert_eq!(desktop_name, "format");
                    assert_eq!(pixel_format.bits_per_pixel, default[0]);
                    assert_eq!(pixel_format.depth, default[1]);
                    assert_eq!(pixel_format.big_endian, default[2] != 0);
                    assert_eq!(pixel_format.true_color, default[3] != 0);
                    // The full request establishes a Raw baseline before CopyRect is legal.
                    assert_eq!(
                        feed(&mut session, &first, "first"),
                        vec![vec![3, 1, 0, 0, 0, 0, 0, 2, 0, 1]]
                    );
                    assert_eq!(
                        session
                            .with_framebuffer(|core| core.pixels().to_vec())
                            .unwrap(),
                        [0x56, 0x34, 0x12, 255, 0x98, 0x76, 0x54, 255]
                    );
                    assert_eq!(
                        feed(&mut session, &incremental, "copy"),
                        vec![vec![3, 1, 0, 0, 0, 0, 0, 2, 0, 1]]
                    );
                    session
                        .with_framebuffer(|core| {
                            assert_eq!(core.pixels(), [0x56, 0x34, 0x12, 255].repeat(2));
                            assert_eq!(core.latest_frame_id(), Some(2));
                        })
                        .unwrap();
                    assert!(matches!(
                        session.take_events().last(),
                        Some(RfbClientSessionEvent::Bell)
                    ));
                }
            }
        }
    }
}

#[test]
fn ordinary_fence_round_trip_is_bounded_and_does_not_claim_synchronization() {
    for length in [0, 1, 64] {
        let mut session = ready();
        session.take_events();
        let payload = vec![0x5a; length];
        let mut wire = vec![248, 0, 0, 0];
        wire.extend(0x8000_00ff_u32.to_be_bytes());
        wire.push(length as u8);
        wire.extend(&payload);
        wire.push(2); // A following message remains aligned after the opaque cookie.
        let mut writes = Vec::new();
        for byte in wire {
            writes.extend(session.consume(&[byte]).unwrap());
        }
        let mut expected = vec![248, 0, 0, 0, 0, 0, 0, 0, length as u8];
        expected.extend(payload);
        assert_eq!(writes, vec![expected.clone()]);
        assert!(
            session.consume(&expected).unwrap().is_empty(),
            "responses must not cause ping-pong"
        );
        assert_eq!(
            session.take_events(),
            vec![LLRemoteLib::remote_desktop_core::RfbClientSessionEvent::Bell]
        );
    }
    assert_eq!(
        ready()
            .consume(&[248, 0, 0, 0, 128, 0, 0, 0, 65])
            .unwrap_err(),
        "rfb_resource_limit:fence_payload"
    );
}

fn desktop_layout_update(reason: u16, status: u16, width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 1];
    for value in [reason, status, width, height] {
        bytes.extend(value.to_be_bytes());
    }
    bytes.extend((-308_i32).to_be_bytes());
    bytes.extend([1, 0, 0, 0]);
    bytes.extend(1_u32.to_be_bytes());
    for value in [0_u16, 0, width, height] {
        bytes.extend(value.to_be_bytes());
    }
    bytes.extend(0_u32.to_be_bytes());
    bytes
}

fn seed_pixels(session: &mut RfbClientSessionCore) {
    let mut bytes = vec![0, 0, 0, 1];
    bytes.extend(rectangle(0, 2, 1, 0));
    bytes.extend([1, 2, 3, 0, 4, 5, 6, 0]);
    session.consume(&bytes).unwrap();
    session.take_events();
}

#[test]
fn standard_continuous_confirmation_enables_once_and_end_resumes_requests() {
    let mut session = ready();
    assert_eq!(
        session.consume(&[150]).unwrap(),
        vec![vec![150, 1, 0, 0, 0, 0, 0, 2, 0, 1]]
    );
    let mut update = vec![0, 0, 0, 1];
    update.extend(rectangle(0, 2, 1, 0));
    update.extend([1, 2, 3, 0, 4, 5, 6, 0]);
    assert!(session.consume(&update).unwrap().is_empty());

    let resized = desktop_layout_update(0, 0, 3, 1);
    assert_eq!(
        session.consume(&resized).unwrap(),
        vec![vec![150, 1, 0, 0, 0, 0, 0, 3, 0, 1]]
    );
    let request = vec![3, 1, 0, 0, 0, 0, 0, 3, 0, 1];
    assert_eq!(session.consume(&[150]).unwrap(), vec![request.clone()]);
    // A legacy marker after stopping (e.g. quality renegotiation) must not
    // silently re-enable the stream or create an enable/end feedback loop.
    let mut legacy = vec![0, 0, 0, 1];
    legacy.extend(rectangle(0, 0, 0, -313));
    assert_eq!(session.consume(&legacy).unwrap(), vec![request.clone()]);
    assert_eq!(session.consume(&update).unwrap(), vec![request]);
}

#[test]
fn same_size_extended_desktop_notification_keeps_pixels_and_uses_incremental_request() {
    let mut session = ready();
    seed_pixels(&mut session);
    let id = session
        .with_framebuffer(|core| core.latest_frame_id())
        .unwrap();
    for chunk_size in [1, 7, usize::MAX] {
        let mut writes = Vec::new();
        for chunk in desktop_layout_update(0, 0, 2, 1).chunks(chunk_size) {
            writes.extend(session.consume(chunk).unwrap());
        }
        assert_eq!(writes.len(), 1);
        assert_eq!(
            &writes[0][..2],
            &[3, 1],
            "layout notices must not trigger full-request loops"
        );
        session
            .with_framebuffer(|core| {
                assert_eq!(core.latest_frame_id(), id);
                assert_eq!(core.pixels(), [1, 2, 3, 255, 4, 5, 6, 255]);
            })
            .unwrap();
    }
}

#[test]
fn failed_extended_desktop_response_ignores_undefined_geometry_and_consumes_its_payload() {
    use LLRemoteLib::remote_desktop_core::RfbClientSessionEvent;
    for status in [1, 2, 3, 4, u16::MAX] {
        let mut session = ready();
        seed_pixels(&mut session);
        let id = session
            .with_framebuffer(|core| core.latest_frame_id())
            .unwrap();
        // Other fields are undefined on failed/forwarded requests, but wire structure remains.
        let dimension = if status % 2 == 0 { u16::MAX } else { 0 };
        let mut bytes = desktop_layout_update(1, status, dimension, dimension);
        bytes.push(2); // Following Bell proves stream alignment.
        let mut writes = Vec::new();
        for chunk in bytes.chunks(1) {
            writes.extend(session.consume(chunk).unwrap());
        }
        assert_eq!(&writes[0][..2], &[3, 1]);
        assert_eq!(session.take_events(), [RfbClientSessionEvent::Bell]);
        session
            .with_framebuffer(|core| {
                assert_eq!(core.geometry().unwrap().width, 2);
                assert_eq!(core.latest_frame_id(), id);
                assert_eq!(core.pixels(), [1, 2, 3, 255, 4, 5, 6, 255]);
            })
            .unwrap();
    }
}

#[test]
fn extended_desktop_resize_retains_overlap_without_requesting_full_updates() {
    let mut session = ready();
    seed_pixels(&mut session);
    let writes = session.consume(&desktop_layout_update(2, 0, 3, 2)).unwrap();
    assert_eq!(&writes[0][..2], &[3, 1]);
    session
        .with_framebuffer(|core| {
            let geometry = core.geometry().unwrap();
            assert_eq!((geometry.width, geometry.height), (3, 2));
            assert!(core.snapshot().is_some());
            assert_eq!(&core.pixels()[..8], &[1, 2, 3, 255, 4, 5, 6, 255]);
            assert!(core.pixels()[8..].iter().all(|byte| *byte == 0));
        })
        .unwrap();
    let mut update = vec![0, 0, 0, 1];
    update.extend(rectangle(2, 1, 1, 0));
    update.extend([9, 8, 7, 0]);
    session.consume(&update).unwrap();
    session
        .with_framebuffer(|core| {
            assert_eq!(&core.pixels()[..8], &[1, 2, 3, 255, 4, 5, 6, 255]);
            assert_eq!(&core.pixels()[8..12], &[9, 8, 7, 255]);
        })
        .unwrap();
    let writes = session
        .consume(&desktop_layout_update(u16::MAX, 99, 1, 1))
        .unwrap();
    assert_eq!(&writes[0][..2], &[3, 1]);
    session
        .with_framebuffer(|core| {
            assert_eq!(core.pixels(), [1, 2, 3, 255]);
        })
        .unwrap();
}

#[test]
fn transport_metrics_count_idle_timeouts_and_successful_reads_then_reset() {
    use std::io::{self, Read};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
    use LLRemoteLib::remote_desktop_core::run_rfb_client_tcp_loop_until;
    struct Peer(u8);
    impl Read for Peer {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.0 += 1;
            match self.0 {
                1 => Err(io::ErrorKind::TimedOut.into()),
                2 => {
                    buffer[0] = 2;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }
    impl Write for Peer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut session = ready();
    run_rfb_client_tcp_loop_until(&mut Peer(0), &mut session, &AtomicBool::new(false)).unwrap();
    let metrics = session.take_performance_metrics(Duration::from_secs(5));
    assert_eq!(metrics.read_calls, 3);
    assert_eq!(metrics.read_timeouts, 1);
    assert_eq!(metrics.received_bytes, 1);
    assert_eq!(metrics.updates, 0);
    assert!(metrics.write_total_us >= metrics.write_max_us);
    assert_eq!(
        session
            .take_performance_metrics(Duration::from_secs(5))
            .read_calls,
        0
    );
}

#[test]
fn transport_failures_retain_io_stage_and_safe_kind_without_os_text() {
    use std::io::{self, ErrorKind, Read};
    use std::sync::atomic::AtomicBool;
    use LLRemoteLib::remote_desktop_core::run_rfb_client_tcp_loop_until;
    use LLRemoteLib::remote_session::RfbFailure;
    struct Peer {
        stage: u8,
        kind: ErrorKind,
        detail: String,
    }
    impl Read for Peer {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.stage == 0 {
                return Err(io::Error::new(self.kind, self.detail.clone()));
            }
            buffer[..4].copy_from_slice(&[0, 0, 0, 0]); // Empty update requests another update.
            Ok(4)
        }
    }
    impl Write for Peer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.stage == 1 {
                return Err(io::Error::new(self.kind, self.detail.clone()));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(self.kind, self.detail.clone()))
        }
    }
    for (stage, kind, expected) in [
        (0, ErrorKind::ConnectionReset, "rfb_tcp_read_reset"),
        (0, ErrorKind::ConnectionAborted, "rfb_tcp_read_aborted"),
        (0, ErrorKind::Other, "rfb_tcp_read_failed"),
        (1, ErrorKind::TimedOut, "rfb_tcp_write_timeout"),
        (1, ErrorKind::WouldBlock, "rfb_tcp_write_timeout"),
        (1, ErrorKind::ConnectionReset, "rfb_tcp_write_reset"),
        (1, ErrorKind::BrokenPipe, "rfb_tcp_write_broken_pipe"),
        (1, ErrorKind::Other, "rfb_tcp_write_failed"),
        (2, ErrorKind::Other, "rfb_tcp_flush_failed"),
    ] {
        let detail = uuid::Uuid::new_v4().to_string();
        let mut peer = Peer {
            stage,
            kind,
            detail: detail.clone(),
        };
        let error = run_rfb_client_tcp_loop_until(&mut peer, &mut ready(), &AtomicBool::new(false))
            .unwrap_err();
        assert_eq!(error, expected);
        let failure = RfbFailure::from_internal(&error);
        assert_eq!(failure.code, expected);
        assert_eq!(
            failure.category,
            LLRemoteLib::remote_session::RfbFailureCategory::TargetUnreachable
        );
        assert_eq!(RfbFailure::from_internal(failure.code), failure);
        assert!(!serde_json::to_string(&failure).unwrap().contains(&detail));
    }
}

#[test]
fn numeric_metrics_count_only_completed_updates_and_reset_between_idle_windows() {
    use std::time::Duration;
    let mut session = ready();
    let mut update = vec![0, 0, 0, 2];
    update.extend(rectangle(0, 1, 1, 7));
    update.extend([0x80, 9, 8, 7]); // Tight Fill.
    update.extend(rectangle(1, 1, 1, 1));
    update.extend([0, 0, 0, 0]); // CopyRect from the preceding rectangle.
    session.consume(&update[..update.len() - 1]).unwrap();
    let partial = session.take_performance_metrics(Duration::from_secs(5));
    assert_eq!(partial.updates, 0);
    assert_eq!(partial.received_bytes, (update.len() - 1) as u64);
    session.consume(&update[update.len() - 1..]).unwrap();
    let metrics = session.take_performance_metrics(Duration::from_millis(5200));
    assert_eq!(metrics.window_ms, 5200);
    assert_eq!(metrics.received_bytes, 1);
    assert_eq!(metrics.updates, 1);
    assert_eq!(metrics.committed_frames, 1);
    assert_eq!(metrics.encodings.rectangles, 2);
    assert_eq!(metrics.encodings.tight, 1);
    assert_eq!(metrics.encodings.copy_rect, 1);
    assert_eq!(metrics.encodings.jpeg, 0);
    assert_eq!(metrics.decode_total_us, metrics.decode_max_us);
    assert_eq!(
        session
            .with_framebuffer(|core| core.pixels().to_vec())
            .unwrap(),
        [7, 8, 9, 255, 7, 8, 9, 255]
    );
    let idle = session.take_performance_metrics(Duration::from_secs(5));
    assert_eq!(idle.window_ms, 5000);
    assert_eq!(idle.updates, 0);
    assert_eq!(idle.received_bytes, 0);
    assert_eq!(idle.decode_total_us, 0);
    assert_eq!(idle.encodings.rectangles, 0);
}

fn rectangle(x: u16, width: u16, height: u16, encoding: i32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&x.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&encoding.to_be_bytes());
    bytes
}

#[test]
fn receive_rejects_oversized_name_as_soon_as_the_length_arrives() {
    assert_eq!(
        RfbClientSessionCore::default()
            .consume(&server_init(2, 1, u32::MAX))
            .unwrap_err(),
        "rfb_resource_limit:desktop_name"
    );
}

#[test]
fn receive_rejects_geometry_before_waiting_for_name_or_pixel_payload() {
    assert_eq!(
        RfbClientSessionCore::default()
            .consume(&server_init(u16::MAX, u16::MAX, 8))
            .unwrap_err(),
        "rfb_resource_limit:framebuffer"
    );
    assert_eq!(
        RfbClientSessionCore::default()
            .consume(&server_init(0, 1, 0))
            .unwrap_err(),
        "rfb_invalid_framebuffer_geometry"
    );
}

#[test]
fn receive_rejects_unbounded_clipboard_and_compressed_rect_without_body() {
    let mut clipboard = vec![3, 0, 0, 0];
    clipboard.extend_from_slice(&i32::MAX.to_be_bytes());
    assert_eq!(
        ready().consume(&clipboard).unwrap_err(),
        "rfb_resource_limit:clipboard"
    );
    for encoding in [6, 16] {
        let mut update = vec![0, 0, 0, 1];
        update.extend(rectangle(0, 1, 1, encoding));
        update.extend_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            ready().consume(&update).unwrap_err(),
            "rfb_resource_limit:server_message"
        );
    }
}

#[test]
fn receive_rejects_oversized_raw_rect_at_header_boundary() {
    let mut update = vec![0, 0, 0, 1];
    update.extend(rectangle(0, u16::MAX, u16::MAX, 0));
    assert_eq!(
        ready().consume(&update).unwrap_err(),
        "rfb_resource_limit:framebuffer"
    );
}

#[test]
fn receive_preserves_multiple_rectangles_and_coalesced_messages_at_every_split() {
    let mut wire = server_init(2, 1, 0);
    wire.extend_from_slice(&[0, 0, 0, 2]);
    wire.extend(rectangle(0, 1, 1, 0));
    wire.extend_from_slice(&[1, 2, 3, 0]);
    wire.extend(rectangle(1, 1, 1, 0));
    wire.extend_from_slice(&[4, 5, 6, 0]);
    wire.push(2); // Coalesced Bell after FramebufferUpdate.
    for split in 0..=wire.len() {
        let mut session = RfbClientSessionCore::default();
        session.consume(&wire[..split]).unwrap();
        session.consume(&wire[split..]).unwrap();
        let pixels = session
            .with_framebuffer(|core| core.pixels().to_vec())
            .unwrap();
        assert_eq!(pixels, [1, 2, 3, 255, 4, 5, 6, 255], "split {split}");
        assert!(session.take_events().iter().any(|event| matches!(
            event,
            LLRemoteLib::remote_desktop_core::RfbClientSessionEvent::Bell
        )));
    }
}

#[test]
fn control_messages_and_clipboard_keep_exact_replies_at_arbitrary_splits() {
    use LLRemoteLib::remote_desktop_core::RfbClientSessionEvent;
    fn cut_text(flags: u32, data: &[u8]) -> Vec<u8> {
        let length = -i32::try_from(4 + data.len()).unwrap();
        [
            &[3, 0, 0, 0][..],
            length.to_be_bytes().as_slice(),
            flags.to_be_bytes().as_slice(),
            data,
        ]
        .concat()
    }
    let mut wire = vec![1, 0, 0, 7, 0, 2]; // SetColorMapEntries: two U16 RGB entries.
    wire.extend([0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6]);
    wire.push(2);
    wire.extend([0, 0, 0, 1]);
    wire.extend(rectangle(1, 0, 0, -232)); // PointerPosition has no body.
    wire.extend([0, 0, 0, 1]);
    wire.extend(rectangle(0, 2, 1, 0));
    wire.extend([10, 20, 30, 0, 40, 50, 60, 0]);
    wire.extend([3, 0, 0, 0, 0, 0, 0, 4, b'c', b'a', b'f', 0xe9]);
    let utf8 = "远端\n".as_bytes();
    wire.extend([3, 0, 0, 0]);
    wire.extend((utf8.len() as u32).to_be_bytes());
    wire.extend(utf8);
    wire.extend(cut_text(0x1b00_0001, &65_536_u32.to_be_bytes())); // Caps.
    wire.extend(cut_text(0x0800_0001, &[])); // Notify text -> Request text.
    let text = "你好\r\nWorld\0".as_bytes();
    let compressed = compressed(&[(text.len() as u32).to_be_bytes().as_slice(), text].concat());
    wire.extend(cut_text(0x1000_0001, &compressed)); // Provide UTF-8 text.
    wire.push(2);
    let request = vec![3, 1, 0, 0, 0, 0, 0, 2, 0, 1];
    // Independent client messages, including signed lengths and capability bits.
    let expected_writes = vec![
        request.clone(),
        request,
        vec![
            6, 0, 0, 0, 0xff, 0xff, 0xff, 0xf8, 0x1b, 0, 0, 1, 0, 0x10, 0, 0,
        ],
        vec![6, 0, 0, 0, 0xff, 0xff, 0xff, 0xfc, 2, 0, 0, 1],
    ];
    let expected_events = vec![
        RfbClientSessionEvent::Bell,
        RfbClientSessionEvent::PointerPosition { x: 1, y: 0 },
        RfbClientSessionEvent::Clipboard("café".into()),
        RfbClientSessionEvent::Clipboard("远端\n".into()),
        RfbClientSessionEvent::Clipboard("你好\nWorld".into()),
        RfbClientSessionEvent::Bell,
    ];
    let check = |ends: &[usize]| {
        let mut session = ready();
        seed_pixels(&mut session);
        let mut writes = Vec::new();
        let mut offset = 0;
        for &end in ends {
            writes.extend(session.consume(&wire[offset..end]).unwrap());
            offset = end;
        }
        assert_eq!(offset, wire.len());
        assert_eq!(writes, expected_writes);
        assert_eq!(session.take_events(), expected_events);
        session
            .with_framebuffer(|core| {
                assert_eq!(core.latest_frame_id(), Some(2));
                assert_eq!(core.pixels(), [10, 20, 30, 255, 40, 50, 60, 255]);
            })
            .unwrap();
    };
    for split in 0..=wire.len() {
        check(&[split, wire.len()]);
    }
    check(&(1..=wire.len()).collect::<Vec<_>>());
    // Reproducible irregular chunks exercise several messages in a single read.
    let mut ends = Vec::new();
    let mut offset = 0;
    let mut seed = 0x12345678_u32;
    while offset < wire.len() {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        offset = (offset + 1 + (seed as usize % 23)).min(wire.len());
        ends.push(offset);
    }
    check(&ends);
}

#[test]
fn receive_accepts_dual_4k_geometry_without_allocating_its_pixels() {
    let mut session = RfbClientSessionCore::default();
    session.consume(&server_init(7680, 2160, 0)).unwrap();
    assert!(session.is_ready());
    assert!(!session.is_presentable());
}

fn compressed(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn assert_fragmented_pixels(
    width: u16,
    height: u16,
    encoding: i32,
    payload: &[u8],
    expected: &[u8],
) {
    let mut wire = server_init(width, height, 0);
    wire.extend_from_slice(&[0, 0, 0, 1]);
    wire.extend(rectangle(0, width, height, encoding));
    wire.extend_from_slice(payload);
    wire.push(2);
    for split in 0..=wire.len() {
        let mut session = RfbClientSessionCore::default();
        session.consume(&wire[..split]).unwrap();
        session.consume(&wire[split..]).unwrap();
        assert_eq!(
            session
                .with_framebuffer(|core| core.pixels().to_vec())
                .unwrap(),
            expected,
            "encoding {encoding}, split {split}"
        );
    }
    for chunk_size in 1..=wire.len() {
        let mut session = RfbClientSessionCore::default();
        for chunk in wire.chunks(chunk_size) {
            session.consume(chunk).unwrap();
        }
        assert_eq!(
            session
                .with_framebuffer(|core| core.pixels().to_vec())
                .unwrap(),
            expected,
            "encoding {encoding}, chunk size {chunk_size}"
        );
        assert_eq!(
            session
                .take_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    LLRemoteLib::remote_desktop_core::RfbClientSessionEvent::Bell
                ))
                .count(),
            1
        );
    }
}

fn compact_length(length: usize) -> Vec<u8> {
    let mut bytes = vec![(length & 0x7f) as u8];
    if length > 0x7f {
        bytes[0] |= 0x80;
        bytes.push(((length >> 7) & 0x7f) as u8);
        if length > 0x3fff {
            bytes[1] |= 0x80;
            bytes.push((length >> 14) as u8);
        }
    }
    bytes
}

#[test]
fn receive_frames_tight_images_and_rejects_image_dimensions_above_the_rectangle() {
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[10, 20, 30, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    let mut payload = vec![0xa0];
    payload.extend(compact_length(png.len()));
    payload.extend(png);
    assert_fragmented_pixels(1, 1, -260, &payload, &[30, 20, 10, 255]);

    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .encode(&[40, 40, 40], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut payload = vec![0x90];
    payload.extend(compact_length(jpeg.len()));
    payload.extend(jpeg);
    assert_fragmented_pixels(1, 1, 7, &payload, &[40, 40, 40, 255]);

    let mut oversized = Vec::new();
    image::codecs::png::PngEncoder::new(&mut oversized)
        .write_image(&[0; 8], 2, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    let mut wire = vec![0, 0, 0, 1];
    wire.extend(rectangle(0, 1, 1, -260));
    wire.push(0xa0);
    wire.extend(compact_length(oversized.len()));
    wire.extend(oversized);
    let mut session = ready();
    assert_eq!(
        session.consume(&wire).unwrap_err(),
        "rfb_resource_limit:tight_image"
    );
    assert!(!session.is_presentable());
}

#[test]
fn receive_frames_compressed_and_tight_rectangles_with_fragmented_lengths() {
    for (encoding, source) in [(6, vec![40, 40, 40, 0]), (16, vec![0, 40, 40, 40])] {
        let body = compressed(&source);
        let mut payload = (body.len() as u32).to_be_bytes().to_vec();
        payload.extend(body);
        // Grey isolates framing from the separately tracked ZRLE CPIXEL bug.
        assert_fragmented_pixels(1, 1, encoding, &payload, &[40, 40, 40, 255]);
    }
    assert_fragmented_pixels(1, 1, 7, &[0, 10, 20, 30], &[30, 20, 10, 255]);
    assert_fragmented_pixels(
        2,
        1,
        7,
        &[0x80, 10, 20, 30],
        &[30, 20, 10, 255, 30, 20, 10, 255],
    );
    assert_fragmented_pixels(
        2,
        1,
        7,
        &[0x40, 1, 1, 1, 2, 3, 10, 20, 30, 0x40],
        &[3, 2, 1, 255, 30, 20, 10, 255],
    );
    let body = compressed(&[10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120]);
    assert!(body.len() < 128);
    let mut payload = vec![0x24, body.len() as u8];
    payload.extend(body);
    assert_fragmented_pixels(
        2,
        2,
        7,
        &payload,
        &[
            30, 20, 10, 255, 60, 50, 40, 255, 90, 80, 70, 255, 120, 110, 100, 255,
        ],
    );
}

#[test]
fn receive_frames_tight_no_zlib_copy_and_filtered_data_at_every_split() {
    assert_fragmented_pixels(1, 1, 7, &[0xa0, 10, 20, 30], &[30, 20, 10, 255]);
    assert_fragmented_pixels(
        2,
        2,
        7,
        &[0xa0, 12, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        &[3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255],
    );
    assert_fragmented_pixels(
        2,
        1,
        7,
        &[0xe0, 1, 1, 1, 2, 3, 10, 20, 30, 0x40],
        &[3, 2, 1, 255, 30, 20, 10, 255],
    );
    assert_fragmented_pixels(1, 1, 7, &[0xe0, 2, 10, 20, 30], &[30, 20, 10, 255]);
}

#[test]
fn receive_rejects_invalid_tight_png_modes_and_oversized_tight_at_the_header() {
    for (width, encoding, control) in [(1, -260, 0), (1, -260, 0xe0)] {
        let mut session = RfbClientSessionCore::default();
        session.consume(&server_init(width, 1, 0)).unwrap();
        let mut wire = vec![0, 0, 0, 1];
        wire.extend(rectangle(0, width, 1, encoding));
        wire.push(control);
        assert!(session.consume(&wire).is_err());
        assert!(!session.is_presentable());
    }
    for encoding in [7, -260] {
        let mut wire = vec![0, 0, 0, 1];
        wire.extend(rectangle(0, u16::MAX, u16::MAX, encoding));
        assert_eq!(
            ready().consume(&wire).unwrap_err(),
            "rfb_resource_limit:framebuffer"
        );
    }
    let mut session = RfbClientSessionCore::default();
    session.consume(&server_init(4, 1, 0)).unwrap();
    let mut wire = vec![0, 0, 0, 1];
    wire.extend(rectangle(0, 4, 1, 7));
    wire.extend([0xa0, 11]); // 4 RGB pixels require 12 bytes, even without zlib.
    assert_eq!(
        session.consume(&wire).unwrap_err(),
        "rfb_tight_decoded_length_mismatch:framing,0,4,1,11,0,12,11"
    );
}

#[test]
fn receive_resumes_hextile_tiles_and_handles_edge_tiles() {
    // Tile 0 sets background and a coloured subrectangle; tile 1 inherits
    // background; the final one-pixel edge tile is Raw.
    let payload = [
        2 | 8 | 16,
        3,
        2,
        1,
        0,
        1,
        30,
        20,
        10,
        0,
        0x00,
        0x00,
        0,
        1,
        60,
        50,
        40,
        0,
    ];
    let mut pixels = [3, 2, 1, 255].repeat(33);
    pixels[..4].copy_from_slice(&[30, 20, 10, 255]);
    pixels[128..].copy_from_slice(&[60, 50, 40, 255]);
    assert_fragmented_pixels(33, 1, 5, &payload, &pixels);
}

#[test]
fn receive_frames_resize_cursor_lastrect_and_following_bell() {
    let mut wire = server_init(2, 1, 0);
    wire.extend_from_slice(&[0, 0, 0xff, 0xff]);
    wire.extend(rectangle(0, 1, 1, -223));
    wire.extend(rectangle(0, 1, 1, -239));
    wire.extend_from_slice(&[10, 20, 30, 0, 0x80]);
    wire.extend(rectangle(0, 1, 1, 0));
    wire.extend_from_slice(&[30, 20, 10, 0]);
    wire.extend(rectangle(0, 0, 0, -224));
    wire.push(2);
    for split in 0..=wire.len() {
        let mut session = RfbClientSessionCore::default();
        session.consume(&wire[..split]).unwrap();
        session.consume(&wire[split..]).unwrap();
        assert_eq!(
            session
                .with_framebuffer(|core| core.pixels().to_vec())
                .unwrap(),
            [30, 20, 10, 255]
        );
        let events = session.take_events();
        assert!(events.iter().any(|event| matches!(event, LLRemoteLib::remote_desktop_core::RfbClientSessionEvent::Cursor(cursor) if cursor.pixels == [10,20,30,255])));
        assert!(events.iter().any(|event| matches!(
            event,
            LLRemoteLib::remote_desktop_core::RfbClientSessionEvent::Bell
        )));
    }
}

#[test]
fn x_cursor_uses_primary_rgb_and_row_padded_color_and_transparency_bits() {
    use LLRemoteLib::remote_desktop_core::RfbClientSessionEvent;
    let mut wire = vec![0, 0, 0, 1];
    let mut cursor = rectangle(8, 9, 2, -240);
    cursor[2..4].copy_from_slice(&1_u16.to_be_bytes());
    wire.extend(cursor);
    wire.extend([10, 20, 30, 90, 80, 70]); // Primary RGB, then secondary RGB.
    wire.extend([0xaa, 0xff, 0x55, 0x00]); // Two independently padded nine-bit rows.
    wire.extend([0xef, 0xff, 0x7f, 0x80]); // Transparency, with padding bits ignored.
    wire.push(2);
    let foreground = [30, 20, 10, 255];
    let background = [70, 80, 90, 255];
    let transparent = [70, 80, 90, 0];
    let expected = [
        foreground,
        background,
        foreground,
        transparent,
        foreground,
        background,
        foreground,
        background,
        foreground,
        transparent,
        foreground,
        background,
        foreground,
        background,
        foreground,
        background,
        foreground,
        background,
    ]
    .concat();
    for split in 0..=wire.len() {
        let mut session = ready();
        seed_pixels(&mut session);
        session.consume(&wire[..split]).unwrap();
        session.consume(&wire[split..]).unwrap();
        let events = session.take_events();
        let [RfbClientSessionEvent::Cursor(cursor), RfbClientSessionEvent::Bell] =
            events.as_slice()
        else {
            panic!("cursor and following Bell missing at split {split}");
        };
        assert_eq!((cursor.hotspot_x, cursor.hotspot_y), (8, 1));
        assert_eq!((cursor.width, cursor.height), (9, 2));
        assert_eq!(cursor.pixels, expected, "split {split}");
        session
            .with_framebuffer(|core| {
                assert_eq!(core.latest_frame_id(), Some(1));
                assert_eq!(core.pixels(), [1, 2, 3, 255, 4, 5, 6, 255]);
            })
            .unwrap();
    }
}

#[test]
fn empty_x_cursor_has_no_body_and_does_not_consume_the_next_update() {
    use LLRemoteLib::remote_desktop_core::{
        ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
        RfbClientFramebufferDecoder, RfbClientSessionEvent,
    };
    for (width, height) in [(0, 0), (0, 17), (9, 0)] {
        let mut hidden = vec![0, 0, 0, 1];
        hidden.extend(rectangle(0, width, height, -240));
        let mut wire = hidden.clone();
        wire.extend([0, 0, 0, 1]);
        wire.extend(rectangle(0, 2, 1, 0));
        wire.extend([9, 8, 7, 0, 6, 5, 4, 0]);
        wire.push(2);
        for split in 0..=wire.len() {
            let mut session = ready();
            seed_pixels(&mut session);
            session.consume(&wire[..split]).unwrap();
            session.consume(&wire[split..]).unwrap();
            let events = session.take_events();
            let [RfbClientSessionEvent::Cursor(cursor), RfbClientSessionEvent::Bell] =
                events.as_slice()
            else {
                panic!("hidden cursor and following Bell missing at split {split}");
            };
            assert_eq!(
                (cursor.width, cursor.height),
                (u32::from(width), u32::from(height))
            );
            assert!(cursor.pixels.is_empty());
            session
                .with_framebuffer(|core| {
                    assert_eq!(core.latest_frame_id(), Some(2));
                    assert_eq!(core.pixels(), [9, 8, 7, 255, 6, 5, 4, 255]);
                })
                .unwrap();
        }
        // Also exercise the decoder boundary without the incremental framer.
        let geometry = FramebufferGeometry {
            width: 2,
            height: 1,
            pixel_format: PixelFormat::Bgra8888,
        };
        let mut decoder =
            RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
        let mut core = ClientFramebufferCore::default();
        let outcome = decoder
            .decode_framebuffer_update(&hidden, geometry, &mut core)
            .unwrap();
        assert!(outcome.cursor.unwrap().pixels.is_empty());
        assert_eq!(outcome.frame_id, None);
        assert!(core.snapshot().is_none());
    }
}

#[test]
fn hextile_undefined_colors_and_conflicting_flags_do_not_publish_a_frame() {
    use LLRemoteLib::remote_desktop_core::{
        ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
        RfbClientFramebufferDecoder,
    };
    let mut after_raw = vec![1];
    after_raw.extend([1, 2, 3, 0].repeat(16));
    after_raw.push(0);
    let after_colored = vec![
        6, 1, 2, 3, 0, 4, 5, 6, 0, 24, 1, 7, 8, 9, 0, 0, 0, 8, 1, 0, 0,
    ];
    for (width, payload, code) in [
        (1, vec![0], "rfb_hextile_background_missing"),
        (17, after_raw, "rfb_hextile_background_missing"),
        (
            1,
            vec![10, 1, 2, 3, 0, 1, 0, 0],
            "rfb_hextile_foreground_missing",
        ),
        (33, after_colored, "rfb_hextile_foreground_missing"),
        (
            1,
            vec![30, 1, 2, 3, 0, 4, 5, 6, 0, 0],
            "rfb_hextile_invalid_flags",
        ),
    ] {
        let mut wire = vec![0, 0, 0, 1];
        wire.extend(rectangle(0, width, 1, 5));
        wire.extend(payload);
        let mut core = ClientFramebufferCore::default();
        let mut decoder =
            RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
        assert_eq!(
            decoder
                .decode_framebuffer_update(
                    &wire,
                    FramebufferGeometry {
                        width: width.into(),
                        height: 1,
                        pixel_format: PixelFormat::Bgra8888,
                    },
                    &mut core
                )
                .unwrap_err(),
            code
        );
        assert!(core.snapshot().is_none());
        for split in 0..=wire.len() {
            let mut session = RfbClientSessionCore::default();
            session.consume(&server_init(width, 1, 0)).unwrap();
            let result = session
                .consume(&wire[..split])
                .and_then(|_| session.consume(&wire[split..]));
            assert_eq!(result.unwrap_err(), code, "width={width}, split={split}");
            assert!(!session.is_presentable());
        }
    }
}

#[test]
fn hextile_keeps_valid_colors_through_solid_tiles_and_raw_ignores_other_flags() {
    let payload = [
        14, 3, 2, 1, 0, 30, 20, 10, 0, 1, 0, 0, 0, 8, 1, 0, 0, 31, 90, 80, 70, 0,
    ];
    let mut expected = [3, 2, 1, 255].repeat(49);
    expected[..4].copy_from_slice(&[30, 20, 10, 255]);
    expected[32 * 4..33 * 4].copy_from_slice(&[30, 20, 10, 255]);
    expected[48 * 4..].copy_from_slice(&[90, 80, 70, 255]);
    assert_fragmented_pixels(49, 1, 5, &payload, &expected);
}

#[test]
fn receive_limits_the_sum_of_decoded_rectangles_before_decode() {
    let mut update = vec![0, 0, 0, 2];
    update.extend(rectangle(0, 8192, 4096, 1));
    update.extend_from_slice(&[0; 4]);
    update.extend(rectangle(0, 1, 1, 1));
    assert_eq!(
        ready().consume(&update).unwrap_err(),
        "rfb_resource_limit:decoded_update"
    );
}

#[test]
fn receive_limits_extended_clipboard_inflation() {
    let mut source = vec![0; 1024 * 1024 + 5];
    source[..4].copy_from_slice(&(1024 * 1024 + 1_u32).to_be_bytes());
    let body = compressed(&source);
    let mut wire = vec![3, 0, 0, 0];
    wire.extend_from_slice(&(-((body.len() + 4) as i32)).to_be_bytes());
    wire.extend_from_slice(&((1_u32 << 28) | 1).to_be_bytes());
    wire.extend(body);
    assert_eq!(
        ready().consume(&wire).unwrap_err(),
        "rfb_resource_limit:clipboard_inflate"
    );
}

#[test]
fn client_reads_between_input_batches_without_losing_key_order() {
    use std::io::{self, Read};
    use std::sync::{atomic::AtomicBool, mpsc};
    use LLRemoteLib::remote_desktop_core::{
        run_rfb_client_tcp_loop_with_commands, RfbClientCommand,
    };
    #[derive(Default)]
    struct Peer {
        writes: Vec<u8>,
        first_read_bytes: Option<usize>,
        reads: usize,
    }
    impl Write for Peer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Read for Peer {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            if self.reads == 1 {
                self.first_read_bytes = Some(self.writes.len());
                buffer[0] = 2;
                Ok(1)
            } else {
                Ok(0)
            }
        }
    }
    let (tx, rx) = mpsc::channel();
    for keysym in 0..100 {
        tx.send(RfbClientCommand::Key { down: true, keysym })
            .unwrap();
    }
    let mut peer = Peer::default();
    run_rfb_client_tcp_loop_with_commands(
        &mut peer,
        &mut ready(),
        &AtomicBool::new(false),
        Some(&rx),
    )
    .unwrap();
    assert!(
        peer.first_read_bytes.unwrap() < 800,
        "network read must occur before the input backlog drains"
    );
    assert_eq!(peer.writes.len(), 800);
    for (keysym, message) in peer.writes.as_chunks::<8>().0.iter().enumerate() {
        assert_eq!(message[0], 4);
        assert_eq!(
            u32::from_be_bytes(message[4..].try_into().unwrap()) as usize,
            keysym
        );
    }
}

#[test]
fn receive_accepts_bounded_legacy_wide_tight_rectangles_without_losing_rows_or_messages() {
    let (width, height) = (4097u16, 2u16);
    let count = usize::from(width) * usize::from(height);
    let color = [30, 20, 10, 255];
    for encoding in [7, -260] {
        assert_fragmented_pixels(
            width,
            height,
            encoding,
            &[0x80, 10, 20, 30],
            &color.repeat(count),
        );
    }
    let append_body = |payload: &mut Vec<u8>, source: &[u8]| {
        let body = compressed(source);
        let mut length = body.len();
        for _ in 0..2 {
            if length < 128 {
                payload.push(length as u8);
                payload.extend(&body);
                return;
            }
            payload.push((length as u8 & 0x7f) | 0x80);
            length >>= 7;
        }
        payload.push(length as u8);
        payload.extend(body);
    };
    let mut copy = vec![0x01];
    append_body(&mut copy, &[10, 20, 30].repeat(count));
    assert_fragmented_pixels(width, height, 7, &copy, &color.repeat(count));

    // Odd width: the next row starts at a fresh byte, with opposite colours.
    let row_bytes = usize::from(width).div_ceil(8);
    let mut indices = vec![0xaa; row_bytes];
    indices[row_bytes - 1] = 0x80;
    indices.extend(vec![0x55; row_bytes]);
    indices[2 * row_bytes - 1] = 0;
    let mut palette = vec![0x41, 1, 1, 10, 20, 30, 0, 0, 0];
    append_body(&mut palette, &indices);
    let expected: Vec<_> = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                if (x + y) % 2 == 0 {
                    [0, 0, 0, 255]
                } else {
                    color
                }
            })
        })
        .collect();
    assert_fragmented_pixels(width, height, 7, &palette, &expected);

    let mut differences = vec![0; count * 3];
    differences[..3].copy_from_slice(&[10, 20, 30]);
    let mut gradient = vec![0x41, 2];
    append_body(&mut gradient, &differences);
    assert_fragmented_pixels(width, height, 7, &gradient, &color.repeat(count));
}
