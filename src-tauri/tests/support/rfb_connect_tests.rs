use super::*;
use std::sync::Arc;

struct DropProbe(Arc<AtomicBool>);
impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn cancelling_a_pending_connect_drops_the_operation_without_waiting_for_its_deadline() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    let stop = AtomicBool::new(false);
    let dropped = Arc::new(AtomicBool::new(false));
    let probe = DropProbe(Arc::clone(&dropped));
    let operation = async {
        let _probe = probe;
        stop.store(true, Ordering::Release);
        std::future::pending::<io::Result<()>>().await
    };
    runtime.block_on(async {
        let started = Instant::now();
        let result = await_connect(operation, &stop, started + Duration::from_millis(250)).await;
        assert!(matches!(result, Err(ConnectError::Cancelled)));
        assert!(started.elapsed() < Duration::from_millis(150));
        assert!(dropped.load(Ordering::Acquire));
    });
}

#[test]
fn connect_deadline_owns_one_future_and_pre_cancel_never_polls_it() {
    use std::sync::atomic::AtomicUsize;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let stop = AtomicBool::new(false);
    let starts = AtomicUsize::new(0);
    let dropped = Arc::new(AtomicBool::new(false));
    let probe = DropProbe(Arc::clone(&dropped));
    let operation = async {
        let _probe = probe;
        starts.fetch_add(1, Ordering::Relaxed);
        std::future::pending::<io::Result<()>>().await
    };
    let result = runtime.block_on(await_connect(
        operation,
        &stop,
        Instant::now() + Duration::from_millis(120),
    ));
    assert!(matches!(result, Err(ConnectError::TimedOut)));
    assert_eq!(
        starts.load(Ordering::Relaxed),
        1,
        "poll intervals must not recreate connect / resend SYN"
    );
    assert!(dropped.load(Ordering::Acquire));
    stop.store(true, Ordering::Release);
    let result = runtime.block_on(await_connect(
        async {
            starts.fetch_add(1, Ordering::Relaxed);
            Ok(())
        },
        &stop,
        Instant::now() + Duration::from_secs(5),
    ));
    assert!(matches!(result, Err(ConnectError::Cancelled)));
    assert_eq!(starts.load(Ordering::Relaxed), 1);
}

#[test]
fn numeric_connect_falls_back_and_returns_a_blocking_stream_after_driver_drop() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (send_tx, send_rx) = mpsc::sync_channel(1);
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        send_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        stream.write_all(&[42]).unwrap();
    });
    let mut stream = connect_addresses(
        [SocketAddr::from(([127, 0, 0, 1], 0)), address],
        &AtomicBool::new(false),
        Duration::from_secs(1),
    )
    .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    send_tx.send(()).unwrap();
    let mut byte = [0];
    let read = stream.read_exact(&mut byte);
    peer.join().unwrap();
    read.unwrap();
    assert_eq!(byte, [42]);
}
