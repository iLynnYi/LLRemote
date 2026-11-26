fn exchange_server_authentication(
    version: &[u8; 12],
    legacy: bool,
    valid_response: bool,
) -> (Vec<u8>, Result<bool, std::io::Error>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let (mut server, _) = listener.accept().unwrap();
    server
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let key = crate::local_vnc_server::auth::create_vnc_auth_key(&uuid::Uuid::new_v4().to_string());
    let worker = thread::spawn(move || {
        authenticate_rfb_client(
            &mut RfbDeadlineIo {
                stream: &mut server,
                stop_requested: &worker_stop,
                deadline: Instant::now() + RFB_HANDSHAKE_TIMEOUT,
            },
            &key,
        )
    });
    let exchange = (|| -> Result<Vec<u8>, std::io::Error> {
        let mut banner = [0; 12];
        peer.read_exact(&mut banner)?;
        assert_eq!(&banner, RFB_VERSION);
        peer.write_all(version)?;
        if legacy {
            let mut security = [0; 4];
            peer.read_exact(&mut security)?;
            assert_eq!(security, 2_u32.to_be_bytes());
        } else {
            let mut security = [0; 2];
            peer.read_exact(&mut security)?;
            assert_eq!(security, [1, 2]);
            peer.write_all(&[2])?;
        }
        let mut challenge = [0; 16];
        peer.read_exact(&mut challenge)?;
        let mut response = create_vnc_auth_response_from_key(&key, &challenge);
        if !valid_response {
            response[0] ^= 1;
        }
        peer.write_all(&response)?;
        let mut result = Vec::new();
        peer.read_to_end(&mut result)?;
        Ok(result)
    })();
    stop.store(true, Ordering::SeqCst);
    let _ = peer.shutdown(Shutdown::Both);
    let result = worker.join().unwrap();
    (
        exchange.expect("version-specific authentication must finish"),
        result,
    )
}

#[test]
fn server_handshake_authentication_uses_negotiated_version() {
    for (version, legacy) in [
        (b"RFB 003.003\n", true),
        (b"RFB 003.005\n", true),
        (b"RFB 003.009\n", true),
        (b"RFB 003.007\n", false),
        (b"RFB 003.008\n", false),
    ] {
        let (wire, result) = exchange_server_authentication(version, legacy, true);
        assert!(result.unwrap());
        assert_eq!(wire, 0_u32.to_be_bytes());
    }
}

#[test]
fn server_handshake_rejects_invalid_banners_before_security() {
    for version in [b"RFB 004.008\n", b"RFB 003.x08\n", b"RFB 003.008 "] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let key =
            crate::local_vnc_server::auth::create_vnc_auth_key(&uuid::Uuid::new_v4().to_string());
        let worker = thread::spawn(move || {
            authenticate_rfb_client(
                &mut RfbDeadlineIo {
                    stream: &mut server,
                    stop_requested: &AtomicBool::new(false),
                    deadline: Instant::now() + RFB_HANDSHAKE_TIMEOUT,
                },
                &key,
            )
        });
        let mut banner = [0; 12];
        peer.read_exact(&mut banner).unwrap();
        peer.write_all(version).unwrap();
        let mut wire = Vec::new();
        peer.read_to_end(&mut wire).unwrap();
        assert!(wire.is_empty());
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn server_handshake_failure_reason_only_follows_rfb_38() {
    for (version, legacy, reason) in [
        (b"RFB 003.003\n", true, false),
        (b"RFB 003.007\n", false, false),
        (b"RFB 003.008\n", false, true),
    ] {
        let (wire, result) = exchange_server_authentication(version, legacy, false);
        assert!(!result.unwrap());
        let mut expected = 1_u32.to_be_bytes().to_vec();
        if reason {
            expected.extend_from_slice(&21_u32.to_be_bytes());
            expected.extend_from_slice(b"Authentication failed");
        }
        assert_eq!(wire, expected);
    }
}

#[test]
fn server_handshake_deadline_survives_authentication_and_client_init() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    let key = crate::local_vnc_server::auth::create_vnc_auth_key(&uuid::Uuid::new_v4().to_string());
    let (deadline_tx, deadline_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let stop = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_millis(500);
        deadline_tx.send(deadline).unwrap();
        let mut io = RfbDeadlineIo {
            stream: &mut server,
            stop_requested: &stop,
            deadline,
        };
        if !authenticate_rfb_client(&mut io, &key)? {
            return Err(std::io::Error::other("authentication rejected"));
        }
        io.read_exact(&mut [0; 1])
    });
    let deadline = deadline_rx.recv().unwrap();
    let exchange = (|| -> Result<(), std::io::Error> {
        peer.read_exact(&mut [0; 12])?;
        peer.write_all(b"RFB 003.")?;
        thread::sleep(Duration::from_millis(150));
        peer.write_all(b"008\n")?;
        let mut security = [0; 2];
        peer.read_exact(&mut security)?;
        peer.write_all(&[2])?;
        let mut challenge = [0; 16];
        peer.read_exact(&mut challenge)?;
        peer.write_all(&create_vnc_auth_response_from_key(&key, &challenge))?;
        let mut result = [1; 4];
        peer.read_exact(&mut result)?;
        assert_eq!(result, [0; 4]);
        // A fresh per-phase timeout would incorrectly accept this late ClientInit.
        if let Some(wait) =
            (deadline + Duration::from_millis(100)).checked_duration_since(Instant::now())
        {
            thread::sleep(wait);
        }
        let _ = peer.write_all(&[1]);
        Ok(())
    })();
    let _ = peer.shutdown(Shutdown::Both);
    let result = worker.join().unwrap();
    exchange.unwrap();
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
}

#[test]
fn server_handshake_write_progress_and_stop_are_bounded() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    let stop = AtomicBool::new(false);
    let mut io = RfbDeadlineIo {
        stream: &mut server,
        stop_requested: &stop,
        deadline: Instant::now() + Duration::from_millis(200),
    };
    // Socket buffer sizes differ across platforms. Reuse bounded chunks until the
    // shared deadline expires, even when the OS temporarily accepts more bytes.
    let payload = [0; 64 * 1024];
    let started = Instant::now();
    let error = loop {
        if let Err(error) = io.write_all(&payload) {
            break error;
        }
    };
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(2));
    stop.store(true, Ordering::SeqCst);
    assert_eq!(
        io.read_exact(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::Interrupted
    );
    assert_eq!(
        io.write_all(&[0]).unwrap_err().kind(),
        std::io::ErrorKind::ConnectionAborted
    );
}
