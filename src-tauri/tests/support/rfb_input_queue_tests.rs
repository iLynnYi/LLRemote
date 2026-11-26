use super::*;
use crate::remote_desktop_core::RfbCommandSource;

fn waiting_task() -> (
    Arc<RfbConnectionTask>,
    Arc<ConnectionControl>,
    mpsc::Sender<()>,
) {
    let (release, waiting) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = waiting.recv();
    });
    let session = RfbClientSessionCore::default();
    // Match spawn's heap ownership, including the address of its worker mutex.
    let task = Arc::new(RfbConnectionTask {
        control: Arc::new(ConnectionControl {
            stop: AtomicBool::new(false),
            socket: Mutex::new(None),
            authentication: session.authentication(),
            inputs: RfbInputQueue::default(),
        }),
        worker: Mutex::new(Some(worker)),
    });
    let receiver = Arc::clone(&task.control);
    (task, receiver, release)
}

#[test]
fn queued_clipboard_bytes_are_bounded_before_the_count_limit() {
    let (task, _receiver, release) = waiting_task();
    for _ in 0..8 {
        task.send(RfbClientCommand::Clipboard {
            text: "x".repeat(1024 * 1024),
        })
        .unwrap();
    }
    assert_eq!(
        task.send(RfbClientCommand::Clipboard {
            text: "x".repeat(1024 * 1024)
        })
        .unwrap_err(),
        "rfb_resource_limit:command_queue"
    );
    drop(release);
    task.join().unwrap();
}

#[test]
fn cancellation_drops_pending_input_while_the_worker_is_still_blocked() {
    let (task, receiver, release) = waiting_task();
    task.send(RfbClientCommand::Key {
        down: false,
        keysym: 42,
    })
    .unwrap();
    task.request_stop();
    assert!(!task.is_finished());
    assert!(
        receiver.inputs.try_recv_command().unwrap().is_none(),
        "cancelled input must not be retained by the blocked worker"
    );
    assert_eq!(
        task.send(RfbClientCommand::Key {
            down: false,
            keysym: 42
        })
        .unwrap_err(),
        "rfb_transport_closed"
    );
    drop(release);
    task.join().unwrap();
}

#[test]
fn joining_a_worker_that_finishes_after_polling_starts_is_bounded() {
    let (task, _control, release) = waiting_task();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        started_tx.send(()).unwrap();
        let _ = finished_tx.send(task.join());
    });
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    thread::sleep(Duration::from_millis(20));
    drop(release);
    assert_eq!(
        finished_rx
            .recv_timeout(SHUTDOWN_TIMEOUT + Duration::from_secs(2))
            .expect("join must finish when the previously blocked worker exits"),
        Ok(())
    );
}

#[test]
fn unused_string_capacity_is_charged_and_dequeue_restores_byte_budget() {
    let queue = RfbInputQueue::default();
    assert_eq!(
        queue
            .push(RfbClientCommand::Clipboard {
                text: String::with_capacity(8 * 1024 * 1024 + 1)
            })
            .unwrap_err(),
        "rfb_resource_limit:command_queue"
    );
    for _ in 0..8 {
        queue
            .push(RfbClientCommand::Clipboard {
                text: "x".repeat(1024 * 1024),
            })
            .unwrap();
    }
    assert!(queue
        .push(RfbClientCommand::Clipboard {
            text: "x".repeat(1024 * 1024)
        })
        .is_err());
    let Some(RfbClientCommand::Clipboard { text }) = queue.try_recv_command().unwrap() else {
        panic!("expected queued clipboard");
    };
    assert_eq!(text.len(), 1024 * 1024);
    drop(text);
    queue
        .push(RfbClientCommand::Clipboard {
            text: "x".repeat(1024 * 1024),
        })
        .unwrap();
    queue.close();
    assert!(queue.try_recv_command().unwrap().is_none());
}

#[test]
fn count_overflow_rejects_without_mutating_key_order() {
    let queue = RfbInputQueue::default();
    for keysym in 0..256 {
        queue
            .push(RfbClientCommand::Key {
                down: keysym % 2 == 0,
                keysym,
            })
            .unwrap();
    }
    assert_eq!(
        queue
            .push(RfbClientCommand::Key {
                down: false,
                keysym: 256
            })
            .unwrap_err(),
        "rfb_command_queue_full"
    );
    for expected in 0..256 {
        let Some(RfbClientCommand::Key { down, keysym }) = queue.try_recv_command().unwrap() else {
            panic!("expected queued key");
        };
        assert_eq!((down, keysym), (expected % 2 == 0, expected));
    }
    queue
        .push(RfbClientCommand::Key {
            down: false,
            keysym: 256,
        })
        .unwrap();
    assert!(matches!(
        queue.try_recv_command().unwrap(),
        Some(RfbClientCommand::Key {
            down: false,
            keysym: 256
        })
    ));
}

#[test]
fn concurrent_producers_cannot_exceed_the_shared_byte_budget() {
    let queue = Arc::new(RfbInputQueue::default());
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let workers = (0..16)
        .map(|_| {
            let queue = Arc::clone(&queue);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let command = RfbClientCommand::Clipboard {
                    text: "x".repeat(1024 * 1024),
                };
                barrier.wait();
                queue.push(command)
            })
        })
        .collect::<Vec<_>>();
    let mut accepted = 0;
    for worker in workers {
        match worker.join().unwrap() {
            Ok(()) => accepted += 1,
            Err(error) => assert_eq!(error, "rfb_resource_limit:command_queue"),
        }
    }
    assert_eq!(accepted, 8);
    for _ in 0..8 {
        assert!(queue.try_recv_command().unwrap().is_some());
    }
    assert!(queue.try_recv_command().unwrap().is_none());
}

#[test]
fn worker_unwind_closes_and_releases_its_owners_queue() {
    let queue = Arc::new(RfbInputQueue::default());
    queue
        .push(RfbClientCommand::Clipboard {
            text: "x".repeat(1024 * 1024),
        })
        .unwrap();
    let owned = Arc::clone(&queue);
    let worker = thread::spawn(move || {
        let _cleanup = InputQueueCleanup(&owned);
        panic!("synthetic worker unwind");
    });
    assert!(worker.join().is_err());
    assert!(queue.try_recv_command().unwrap().is_none());
    assert_eq!(
        queue
            .push(RfbClientCommand::Key {
                down: false,
                keysym: 42
            })
            .unwrap_err(),
        "rfb_transport_closed"
    );
}
