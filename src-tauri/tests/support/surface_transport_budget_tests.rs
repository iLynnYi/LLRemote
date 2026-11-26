use super::*;
use crate::remote_desktop_core::{
    ClientFramebufferCore, ClientFramebufferHub, DecodedFramebufferRect, DecodedFramebufferUpdate,
    FramebufferGeometry, FramebufferRect, PixelFormat, RfbClientSessionCore,
};
use std::net::TcpListener;
use std::thread;
use tungstenite::handshake::server::NoCallback;

#[test]
fn an_incomplete_handshake_on_an_accepted_listener_socket_honors_its_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.write_all(b"GET /test HTTP/1.1\r\n").unwrap();
    let admission_deadline = Instant::now() + Duration::from_secs(1);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    && Instant::now() < admission_deadline =>
            {
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    };
    let started = Instant::now();
    assert!(SurfaceTransport::accept(stream, NoCallback, ClientMemoryBudget::default(),).is_err());
    assert!(started.elapsed() >= Duration::from_millis(50),
        "incomplete handshake returned immediately after {:?}; accepted socket must use blocking I/O", started.elapsed());
}

#[test]
fn connection_admission_is_bounded_and_failed_budget_claims_do_not_consume_slots() {
    let memory = ClientMemoryBudget::new(4 * 1024 * 1024, 0);
    let admission = SurfaceAdmission::new(memory.clone());
    let first = admission.acquire().unwrap();
    let second = admission.acquire().unwrap();
    assert!(admission.acquire().is_err());
    assert_eq!(admission.active.load(Ordering::Acquire), 2);
    drop(first);
    drop(second);
    assert_eq!(memory.in_use(), 0);
    let admission = SurfaceAdmission::new(ClientMemoryBudget::default());
    let mut permits = (0..8)
        .map(|_| admission.acquire().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        admission.acquire().err().unwrap(),
        "surface_connection_limit"
    );
    permits.pop();
    let last = admission.acquire().unwrap();
    drop(permits);
    drop(last);
    assert_eq!(admission.active.load(Ordering::Acquire), 0);
}

#[test]
fn a_drip_feed_cannot_extend_the_absolute_socket_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        for _ in 0..20 {
            if stream.write_all(&[1]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    let (stream, _) = listener.accept().unwrap();
    let started = Instant::now();
    let mut socket = SurfaceSocket {
        stream,
        remaining_handshake: None,
        read_deadline: Some(started + Duration::from_millis(75)),
        write_deadline: None,
    };
    let mut bytes = [0; 16];
    let error = loop {
        if let Err(error) = socket.read(&mut bytes) {
            break error;
        }
    };
    assert!(matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ));
    assert!(started.elapsed() < Duration::from_millis(500));
    drop(socket);
    peer.join().unwrap();
}

#[test]
fn handshake_reads_stop_at_the_byte_cap_without_waiting_for_more_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (stream, _) = listener.accept().unwrap();
    peer.write_all(&[1; 9]).unwrap();
    let mut socket = SurfaceSocket {
        stream,
        remaining_handshake: Some(8),
        read_deadline: Some(Instant::now() + Duration::from_millis(100)),
        write_deadline: None,
    };
    let mut bytes = [0; 16];
    assert_eq!(socket.read(&mut bytes).unwrap(), 8);
    assert_eq!(
        socket.read(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn websocket_ack_input_rejects_large_messages_at_the_frame_header() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut transport =
            SurfaceTransport::accept(stream, NoCallback, ClientMemoryBudget::default()).unwrap();
        transport.begin_ack(Instant::now() + Duration::from_secs(1));
        assert!(matches!(transport.read(), Err(WebSocketError::Capacity(_))));
    });
    let (mut peer, _) = tungstenite::connect(format!("ws://{address}/test")).unwrap();
    peer.send(Message::Binary(vec![1; 257].into())).unwrap();
    server.join().unwrap();
}

#[test]
fn sending_and_acknowledging_keeps_the_writer_capacity_charged_until_socket_drop() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let memory = ClientMemoryBudget::new(1024 * 1024, 0);
        let (stream, _) = listener.accept().unwrap();
        let mut transport = SurfaceTransport::accept(stream, NoCallback, memory.clone()).unwrap();
        let geometry = FramebufferGeometry {
            width: 4,
            height: 2,
            pixel_format: PixelFormat::Bgra8888,
        };
        let mut core = ClientFramebufferCore::with_memory(memory.clone());
        core.attach_surface("main", true).unwrap();
        core.apply(DecodedFramebufferUpdate {
            geometry,
            rectangles: vec![DecodedFramebufferRect {
                rect: FramebufferRect::full(geometry),
                pixels: vec![0; 32],
            }],
        })
        .unwrap();
        let update = core.next_surface_update("main").unwrap().unwrap();
        let packet = SurfaceFramePacket::from_core(&core, &update).unwrap();
        transport.send_packet(packet).unwrap();
        assert_eq!(memory.in_use(), 32 + 76 + transport.writer_bytes);
        transport.begin_ack(Instant::now() + Duration::from_secs(1));
        assert!(matches!(transport.read(), Ok(Message::Binary(_))));
        transport.presented();
        assert_eq!(memory.in_use(), 32 + transport.writer_bytes);
        drop(transport);
        assert_eq!(memory.in_use(), 32);
        core.clear_connection();
        assert_eq!(memory.in_use(), 0);
    });
    let (mut peer, _) = tungstenite::connect(format!("ws://{address}/test")).unwrap();
    let message = peer.read().unwrap();
    assert!(matches!(message, Message::Binary(bytes) if bytes.len() == 76));
    let mut ack = b"VNCA".to_vec();
    ack.extend_from_slice(&1u64.to_be_bytes());
    peer.send(Message::Binary(ack.into())).unwrap();
    server.join().unwrap();
}

#[test]
fn dual_4k_decoding_continues_while_a_real_surface_packet_awaits_ack() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let framebuffer = Arc::new(ClientFramebufferHub::default());
        let memory = framebuffer.memory_budget();
        let admission = SurfaceAdmission::new(memory.clone());
        let permit = admission.acquire().unwrap();
        let mut session = RfbClientSessionCore::with_shared_framebuffer(None, framebuffer.clone());
        session.consume(b"RFB 003.008\n").unwrap();
        session.consume(&[1, 1]).unwrap();
        session.consume(&0u32.to_be_bytes()).unwrap();
        let mut init = Vec::from(7680u16.to_be_bytes());
        init.extend_from_slice(&2160u16.to_be_bytes());
        init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
        init.extend_from_slice(&0u32.to_be_bytes());
        session.consume(&init).unwrap();
        session.take_events();
        framebuffer
            .lock()
            .unwrap()
            .attach_surface("main", true)
            .unwrap();
        // Split the same full dual-4K workload into legal Tight rectangles.
        let tight_update = |blue: u8| {
            let mut update = vec![0, 0, 0, 4];
            for (x, width) in [(0_u16, 2048_u16), (2048, 2048), (4096, 2048), (6144, 1536)] {
                update.extend(x.to_be_bytes());
                update.extend(0_u16.to_be_bytes());
                update.extend(width.to_be_bytes());
                update.extend(2160_u16.to_be_bytes());
                update.extend(7_i32.to_be_bytes());
                update.extend([0x80, 0x12, 0x34, blue]);
            }
            update
        };
        let mut update = tight_update(0x56);
        session.consume(&update).unwrap();
        let packet = {
            let core = framebuffer.lock().unwrap();
            let dirty = core.next_surface_update("main").unwrap().unwrap();
            SurfaceFramePacket::from_core(&core, &dirty).unwrap()
        };
        let (stream, _) = listener.accept().unwrap();
        let mut transport = SurfaceTransport::accept(stream, NoCallback, memory.clone()).unwrap();
        transport.send_packet(packet).unwrap();
        // The writer allocation and in-flight packet remain charged during the
        // next decode, before any matching ACK is processed.
        assert!(transport.in_flight.is_some());
        update = tight_update(0x78);
        session.consume(&update).unwrap();
        session
            .with_framebuffer(|core| {
                assert_eq!(core.latest_frame_id(), Some(2));
                assert_eq!(&core.pixels()[..4], &[0x78, 0x34, 0x12, 255]);
            })
            .unwrap();
        transport.begin_ack(Instant::now() + Duration::from_secs(1));
        assert!(matches!(transport.read(), Ok(Message::Binary(_))));
        transport.presented();
        drop(transport);
        drop(permit);
        drop(session);
        drop(framebuffer);
        assert_eq!(memory.in_use(), 64 * 1024 * 1024);
    });
    let config = WebSocketConfig::default()
        .max_frame_size(Some(128 * 1024 * 1024))
        .max_message_size(Some(128 * 1024 * 1024));
    let (mut peer, _) =
        tungstenite::client::connect_with_config(format!("ws://{address}/test"), Some(config), 3)
            .unwrap();
    let message = peer.read().unwrap();
    assert!(matches!(message, Message::Binary(bytes) if bytes.len() == 7680 * 2160 * 4 + 44));
    let mut ack = b"VNCA".to_vec();
    ack.extend_from_slice(&1u64.to_be_bytes());
    peer.send(Message::Binary(ack.into())).unwrap();
    server.join().unwrap();
}
