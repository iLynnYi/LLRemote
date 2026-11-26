use super::*;

fn material() -> RfbAuthentication {
    RfbAuthentication::from_password(Some(uuid::Uuid::new_v4().to_string()))
}

#[test]
fn challenge_material_has_one_consumer_across_all_handles() {
    let auth = material();
    let cancellation = auth.clone();
    assert!(auth.respond(&[0; 16]).is_ok());
    assert!(!cancellation.has_key().unwrap());
    assert_eq!(
        auth.respond(&[0; 16]).unwrap_err(),
        "rfb_credentials_required"
    );
}

#[test]
fn clearing_an_owner_handle_erases_worker_material_and_is_idempotent() {
    let worker = material();
    let owner = worker.clone();
    owner.clear();
    owner.clear();
    assert!(!worker.has_key().unwrap());
    assert_eq!(
        worker.respond(&[0; 16]).unwrap_err(),
        "rfb_credentials_required"
    );
}

#[test]
fn poisoned_state_still_allows_secret_cleanup() {
    let auth = material();
    let panicking = auth.clone();
    assert!(std::thread::spawn(move || {
        let _lock = panicking.0.lock().unwrap();
        panic!("synthetic auth state panic");
    })
    .join()
    .is_err());
    assert_eq!(auth.has_key().unwrap_err(), "rfb_auth_state_poisoned");
    auth.clear();
    assert!(auth.0.lock().unwrap_err().into_inner().is_none());
}
