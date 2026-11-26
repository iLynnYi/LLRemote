use super::*;

#[test]
fn cancellation_erases_auth_while_a_connection_worker_is_still_blocked() {
    let session = RfbClientSessionCore::with_password(Some(uuid::Uuid::new_v4().to_string()));
    let control = ConnectionControl {
        stop: AtomicBool::new(false),
        socket: Mutex::new(None),
        authentication: session.authentication(),
        inputs: RfbInputQueue::default(),
    };
    let (waiting_tx, waiting_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        waiting_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        assert!(!session.authentication().has_key().unwrap());
    });
    waiting_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(control.authentication.has_key().unwrap());
    control.stop();
    assert!(control.stop.load(Ordering::Acquire));
    assert!(!control.authentication.has_key().unwrap());
    assert!(
        !worker.is_finished(),
        "secret cleanup does not wait for worker exit"
    );
    release_tx.send(()).unwrap();
    worker.join().unwrap();
}
