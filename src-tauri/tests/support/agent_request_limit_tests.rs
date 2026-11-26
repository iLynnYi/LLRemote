use super::*;

#[test]
fn agent_request_admission_is_bounded_and_released_after_worker_failure() {
    let mut permits = (0..MAX_APP_AGENT_REQUESTS)
        .map(|_| admit_agent_request().unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(admit_agent_request(), Err(error) if error == "agent_request_busy"));
    let permit = permits.pop().unwrap();
    let failed = thread::spawn(move || {
        let _permit = permit;
        panic!("synthetic worker failure");
    });
    assert!(failed.join().is_err());
    let recovered = admit_agent_request().unwrap();
    assert!(admit_agent_request().is_err());
    drop(recovered);
    drop(permits);
    assert_eq!(ACTIVE_APP_AGENT_REQUESTS.load(Ordering::Acquire), 0);
}
