use super::*;

#[test]
fn agent_listener_limits_silent_peers_and_closes_them_before_returning() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let listener = thread::spawn(move || {
        run_agent_listener(
            listener,
            worker_stop,
            AgentBootstrap {
                key: Uuid::new_v4().to_string(),
                tickets: Arc::new(Mutex::new(HashMap::new())),
            },
            "fixture-agent".into(),
            Arc::new(Mutex::new(HashMap::new())),
            PathBuf::new(),
            AgentRuntimeContext {
                shared_display: Arc::new(RwLock::new(SharedDisplay {
                    relay: None,
                    revision: 0,
                    desktop_epoch: 0,
                    id: "fixture-display".into(),
                    label: "Fixture".into(),
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
                })),
                view_only: true,
                interactive: None,
            },
        );
    });
    let mut peers = (0..9)
        .map(|_| {
            let peer = TcpStream::connect(("127.0.0.1", port)).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            peer
        })
        .collect::<Vec<_>>();
    let mut byte = [0];
    let rejected = matches!(peers.last_mut().unwrap().read(&mut byte), Ok(0));
    stop.store(true, Ordering::SeqCst);
    listener.join().unwrap();
    let cancelled = peers[..8]
        .iter_mut()
        .all(|peer| matches!(peer.read(&mut byte), Ok(0)));
    drop(peers);
    assert!(rejected, "ninth silent Agent peer received a worker");
    assert!(
        cancelled,
        "Agent listener returned with live client sockets"
    );
}
