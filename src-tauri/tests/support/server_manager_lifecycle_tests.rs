use super::*;
use std::io::Read;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;

fn start_request() -> LocalVncServerStartRequest {
    LocalVncServerStartRequest {
        port: 0,
        bind_address: "127.0.0.1".into(),
        password: Uuid::new_v4().to_string(),
        view_only: true,
    }
}

#[test]
fn stopping_runtime_keeps_its_state_and_rejects_restart_until_workers_are_joined() {
    let manager = Arc::new(LocalVncServerManager::default());
    let calls = AtomicUsize::new(0);
    let (entered, enter) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let changed = Arc::new(move || {
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            let _ = entered.send(());
            let _ = released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5));
        }
    });
    let overview = manager
        .start_with_active_connections_callback(start_request(), Some(changed))
        .unwrap();
    let mut peer = TcpStream::connect(("127.0.0.1", overview.port)).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut banner = [0; 12];
    peer.read_exact(&mut banner).unwrap();
    assert_eq!(&banner, RFB_VERSION);

    let stopping = Arc::clone(&manager);
    let worker = thread::spawn(move || stopping.stop());
    enter
        .recv_timeout(Duration::from_secs(2))
        .expect("stop reached connection resource release");
    let status = manager.status().unwrap().status;
    let restart = manager.start(start_request());
    let duplicate_stop = manager.stop();
    release.send(()).unwrap();
    let stopped = worker.join().unwrap().unwrap();
    drop(peer);
    // Clean up an incorrectly accepted runtime before reporting the regression.
    manager.stop().unwrap();
    assert_eq!(status, LocalVncServerStatus::Stopping);
    assert!(
        restart.is_err(),
        "restart accepted while old workers were live"
    );
    assert!(
        duplicate_stop.is_err(),
        "duplicate stop reported completion early"
    );
    assert_eq!(stopped.status, LocalVncServerStatus::Stopped);
    assert_eq!(
        manager.status().unwrap().status,
        LocalVncServerStatus::Stopped
    );
    assert_eq!(
        manager.start(start_request()).unwrap().status,
        LocalVncServerStatus::Running
    );
    manager.stop().unwrap();
}

#[test]
fn stop_callback_unwind_waits_for_owned_workers_before_reopening_the_gate() {
    let manager = Arc::new(LocalVncServerManager::default());
    let (release, released) = mpsc::channel();
    let capture_worker = thread::spawn(move || {
        let _ = released.recv_timeout(Duration::from_secs(5));
    });
    let (entered, enter) = mpsc::channel();
    let connections = Arc::new(LocalVncServerConnectionRegistry::new(Some(Arc::new(
        move || {
            let _ = entered.send(());
            panic!("controlled stop callback failure");
        },
    ))));
    *manager.runtime.lock().unwrap() =
        LocalVncServerState::Running(Box::new(LocalVncServerRuntime {
            bind_address: "127.0.0.1".into(),
            port: 0,
            view_only: true,
            started_at: now_unix_seconds(),
            stop_requested: Arc::new(AtomicBool::new(false)),
            connections,
            shared_display: Arc::new(RwLock::new(SharedDisplay::fallback_main())),
            frame_cache: Arc::new(LocalVncServerFrameCache::default()),
            agent: None,
            agent_overview: stopped_agent_overview(),
            worker: None,
            capture_worker: Some(capture_worker),
            #[cfg(target_os = "windows")]
            discovery_revision: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }));
    let stopping = Arc::clone(&manager);
    let (finished, finish) = mpsc::channel();
    let worker = thread::spawn(move || {
        let failed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stopping.stop())).is_err();
        let _ = finished.send(failed);
    });
    enter.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = finish.recv_timeout(Duration::from_millis(200)).is_ok();
    let status = manager.status().unwrap().status;
    release.send(()).unwrap();
    worker.join().unwrap();
    assert!(
        !premature,
        "stop discarded a live capture worker during unwind"
    );
    assert!(finish.recv_timeout(Duration::from_secs(1)).unwrap());
    assert_eq!(status, LocalVncServerStatus::Stopping);
    assert_eq!(
        manager.status().unwrap().status,
        LocalVncServerStatus::Stopped
    );
}
