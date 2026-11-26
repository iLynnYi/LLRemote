use super::*;
use std::cell::Cell;

#[test]
fn successful_partial_writes_cannot_renew_the_message_deadline() {
    let start = Instant::now();
    let elapsed = Cell::new(Duration::ZERO);
    let mut timeouts = Vec::new();
    let mut accepted = Vec::new();
    let error = write_before_deadline(
        &[1, 2, 3, 4],
        &AtomicBool::new(false),
        start + Duration::from_millis(25),
        || start + elapsed.get(),
        |bytes, timeout| {
            timeouts.push(timeout);
            accepted.push(bytes[0]);
            elapsed.set(elapsed.get() + Duration::from_millis(10));
            Ok(1)
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(accepted, [1, 2, 3]);
    assert_eq!(
        timeouts,
        [
            Duration::from_millis(25),
            Duration::from_millis(15),
            Duration::from_millis(5)
        ]
    );
}

#[test]
fn transient_errors_retry_the_unsent_suffix_under_the_same_budget() {
    let start = Instant::now();
    let elapsed = Cell::new(Duration::ZERO);
    let mut accepted = Vec::new();
    let mut attempts = 0;
    write_before_deadline(
        &[1, 2, 3],
        &AtomicBool::new(false),
        start + Duration::from_millis(50),
        || start + elapsed.get(),
        |bytes, _| {
            attempts += 1;
            elapsed.set(elapsed.get() + Duration::from_millis(5));
            match attempts {
                1 => Err(io::ErrorKind::TimedOut.into()),
                3 => Err(io::ErrorKind::WouldBlock.into()),
                4 => Err(io::ErrorKind::Interrupted.into()),
                _ => {
                    accepted.push(bytes[0]);
                    Ok(1)
                }
            }
        },
    )
    .unwrap();
    assert_eq!(accepted, [1, 2, 3]);
    assert_eq!(attempts, 6);

    elapsed.set(Duration::ZERO);
    attempts = 0;
    let error = write_before_deadline(
        &[1],
        &AtomicBool::new(false),
        start + Duration::from_millis(25),
        || start + elapsed.get(),
        |_, _| {
            attempts += 1;
            elapsed.set(elapsed.get() + Duration::from_millis(10));
            Err(io::ErrorKind::TimedOut.into())
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(attempts, 3);
}

#[test]
fn cancellation_after_a_partial_write_never_sends_the_suffix() {
    let stop = AtomicBool::new(false);
    let mut attempts = 0;
    let error = write_before_deadline(
        &[1, 2],
        &stop,
        Instant::now() + MESSAGE_WRITE_TIMEOUT,
        Instant::now,
        |_, _| {
            attempts += 1;
            stop.store(true, Ordering::Release);
            Ok(1)
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
    assert_eq!(attempts, 1);
}
