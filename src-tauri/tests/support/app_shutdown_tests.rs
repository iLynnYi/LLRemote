use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

#[test]
fn exit_coordinator_remains_owned_and_does_not_allow_exit_before_cleanup_finishes() {
    let shutdown = AppShutdown::default();
    let (release, released) = mpsc::channel();
    let (woke, wake) = mpsc::channel();
    shutdown
        .start(
            7,
            move || {
                released.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            },
            move || {
                woke.send(()).unwrap();
            },
        )
        .unwrap();
    shutdown
        .start(9, || panic!("must not overlap an existing shutdown"), || {})
        .unwrap();
    assert!(!shutdown.is_complete());
    assert!(!shutdown.is_finished());
    release.send(()).unwrap();
    wake.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(shutdown.finish().unwrap(), Some(7));
    assert!(shutdown.is_complete());
}

#[test]
fn service_shutdown_reaps_surfaces_and_permanently_rejects_new_connections() {
    let clients = RemoteSessionManager::default();
    let request = |id| {
        serde_json::from_value(serde_json::json!({
            "sessionId": id, "target": "loopback-fixture",
            "displaySource": "vnc", "inputSource": "vnc",
            "connectionModel": {
                "baseTransport": { "kind": "vnc" },
                "serverIdentity": { "kind": "unknown", "verification": "none" },
                "localEndpointMatch": { "state": "not-matched" },
                "enhancedChannel": { "status": "unavailable", "authentication": "not-attempted" },
                "capabilities": { "items": [] }
            }
        }))
        .unwrap()
    };
    let (closed, close) = mpsc::channel();
    let mut peers = Vec::new();
    for id in ["shutdown-one", "shutdown-two"] {
        clients.start(request(id)).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let closed = closed.clone();
        peers.push(thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut wire = b"RFB 003.008\n".to_vec();
            wire.extend_from_slice(&[1, 1, 0, 0, 0, 0]);
            wire.extend_from_slice(&[
                0, 1, 0, 1, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0, 0, 0, 0, 0,
            ]);
            wire.extend_from_slice(&[
                0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 30, 20, 10, 0,
            ]);
            stream.write_all(&wire).unwrap();
            let mut bytes = [0; 4096];
            loop {
                match stream.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(_) => {}
                    // Windows may reset a socket cancelled with Shutdown::Both.
                    Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                    Err(error) => panic!("fixture peer read failed: {error}"),
                }
            }
            closed.send(()).unwrap();
        }));
        clients
            .connect_rfb(
                crate::remote_session::RemoteSessionRfbConnectRequest {
                    session_id: id.into(),
                    host: "127.0.0.1".into(),
                    port,
                    password: None,
                },
                |_| {},
            )
            .unwrap();
    }
    clients.begin_shutdown();
    assert_eq!(clients.start(request("late")).unwrap_err(), "rfb_cancelled");
    assert_eq!(
        clients
            .connect_rfb(
                crate::remote_session::RemoteSessionRfbConnectRequest {
                    session_id: "shutdown-one".into(),
                    host: "127.0.0.1".into(),
                    port: 0,
                    password: None,
                },
                |_| {}
            )
            .unwrap_err(),
        "rfb_cancelled"
    );
    clients.request_stop_all().unwrap();
    // Every socket is cancelled before waiting for the first join.
    for _ in 0..2 {
        close.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    clients.shutdown().unwrap();
    for peer in peers {
        peer.join().unwrap();
    }
    assert!(clients.list_snapshots().unwrap().is_empty());
}
