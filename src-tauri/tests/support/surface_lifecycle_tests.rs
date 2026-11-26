use super::*;
use std::io::{Read, Write};
use std::net::Shutdown;

fn fixture_framebuffer() -> Arc<ClientFramebufferHub> {
    use crate::remote_desktop_core::{
        DecodedFramebufferRect, DecodedFramebufferUpdate, FramebufferGeometry, FramebufferRect,
        PixelFormat,
    };
    let hub = Arc::new(ClientFramebufferHub::default());
    let geometry = FramebufferGeometry {
        width: 4,
        height: 2,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut core = hub.lock().unwrap();
    core.attach_surface("main", true).unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry,
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry),
            pixels: vec![7; 32],
        }],
    })
    .unwrap();
    drop(core);
    hub
}

fn ack(frame_id: u64) -> Message {
    let mut bytes = b"VNCA".to_vec();
    bytes.extend_from_slice(&frame_id.to_be_bytes());
    Message::Binary(bytes.into())
}

#[test]
fn a_replacement_surface_socket_receives_a_full_baseline_and_closes_the_old_owner() {
    let hub = fixture_framebuffer();
    let stream = SurfaceStream::start("fixture".into(), Arc::clone(&hub)).unwrap();
    let (mut old, _) = tungstenite::connect(stream.url_for("fixture", "main")).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(socket) = old.get_mut() {
        socket
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
    }
    let baseline = old.read().unwrap().into_data();
    old.send(ack(1)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while hub.lock().unwrap().surfaces()[0].last_presented_frame != Some(1) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let (mut next, _) = tungstenite::connect(stream.url_for("fixture", "main")).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(socket) = next.get_mut() {
        socket
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
    }
    assert_eq!(
        next.read().unwrap().into_data(),
        baseline,
        "new Canvas needs all pixels even when the previous owner already ACKed the latest frame"
    );
    match old.get_mut().read(&mut [0; 1]) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        result => panic!("previous Surface owner is still open: {result:?}"),
    }
    stream.shutdown().unwrap();
}

#[test]
fn an_old_generation_ack_cannot_turn_a_new_framebuffer_baseline_into_a_patch() {
    use crate::remote_desktop_core::{
        DecodedFramebufferRect, DecodedFramebufferUpdate, FramebufferRect,
    };
    let hub = fixture_framebuffer();
    let stream = SurfaceStream::start("fixture".into(), Arc::clone(&hub)).unwrap();
    let (mut peer, _) = tungstenite::connect(stream.url_for("fixture", "main")).unwrap();
    peer.read().unwrap();
    for (previous_id, reset_geometry) in [(1, false), (2, true)] {
        let mut core = hub.lock().unwrap();
        let geometry = core.geometry().unwrap();
        if reset_geometry {
            core.reset_geometry(geometry).unwrap();
        } else {
            core.clear_connection();
        }
        assert!(
            core.mark_surface_presented("main", previous_id).is_ok(),
            "an old ACK during the resize gap must be ignored without closing the Surface"
        );
        core.apply(DecodedFramebufferUpdate {
            geometry,
            rectangles: vec![DecodedFramebufferRect {
                rect: FramebufferRect {
                    x: 1,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                pixels: vec![11, 22, 33, 255],
            }],
        })
        .unwrap();
        drop(core);
        peer.send(ack(previous_id)).unwrap();
        let packet = peer.read().unwrap().into_data();
        assert_eq!(
            packet.len(),
            76,
            "late ACK cannot reuse Canvas pixels from a cleared or resized epoch"
        );
        assert_eq!(&packet[44..48], &[0; 4]);
        assert_eq!(&packet[48..52], &[33, 22, 11, 255]);
    }
    stream.shutdown().unwrap();
}

fn wait_for_connection_budget(hub: &ClientFramebufferHub, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while hub.memory_budget().in_use() != expected {
        assert!(
            Instant::now() < deadline,
            "Surface connection was not admitted"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn drop_closes_an_admitted_incomplete_http_handshake_before_returning() {
    let hub = Arc::new(ClientFramebufferHub::default());
    let baseline = hub.memory_budget().in_use();
    let stream = SurfaceStream::start("fixture".into(), Arc::clone(&hub)).unwrap();
    let mut peer = TcpStream::connect(("127.0.0.1", stream.port)).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();
    peer.write_all(b"GET /surface/fixture/main HTTP/1.1\r\n")
        .unwrap();
    wait_for_connection_budget(&hub, baseline + 2 * 1024 * 1024);
    drop(stream);
    assert_eq!(
        hub.memory_budget().in_use(),
        baseline,
        "Drop must join the admitted worker and release its allowance"
    );
    let mut byte = [0];
    match peer.read(&mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        result => panic!("Surface socket is still open after Drop: {result:?}"),
    }
}

#[test]
fn dropping_a_surface_listener_joins_all_eight_silent_connections() {
    let hub = Arc::new(ClientFramebufferHub::default());
    let baseline = hub.memory_budget().in_use();
    let stream = SurfaceStream::start("fixture".into(), Arc::clone(&hub)).unwrap();
    let peers = (0..8)
        .map(|_| TcpStream::connect(("127.0.0.1", stream.port)).unwrap())
        .collect::<Vec<_>>();
    wait_for_connection_budget(&hub, baseline + 16 * 1024 * 1024);
    drop(stream);
    assert_eq!(hub.memory_budget().in_use(), baseline);
    assert_eq!(
        Arc::strong_count(&hub),
        1,
        "no worker retains the framebuffer after Drop"
    );
    for peer in peers {
        let _ = peer.shutdown(Shutdown::Both);
    }
}

#[test]
fn shutdown_joins_a_worker_waiting_for_the_canvas_ack() {
    let hub = fixture_framebuffer();
    let baseline = hub.memory_budget().in_use();
    let stream = SurfaceStream::start("fixture".into(), Arc::clone(&hub)).unwrap();
    let (mut peer, _) = tungstenite::connect(stream.url_for("fixture", "main")).unwrap();
    assert!(matches!(peer.read().unwrap(), Message::Binary(_)));
    assert!(hub.memory_budget().in_use() > baseline);
    // No ACK: shutdown must interrupt the socket rather than wait for its 5s deadline.
    let started = Instant::now();
    stream.shutdown().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(stream.listener.lock().unwrap().is_none());
    assert_eq!(hub.memory_budget().in_use(), baseline);
    drop(stream);
    assert_eq!(Arc::strong_count(&hub), 1);
}
