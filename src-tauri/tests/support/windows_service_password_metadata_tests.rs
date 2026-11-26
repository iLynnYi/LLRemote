use super::*;

#[test]
fn administrator_owned_protected_readable_metadata_is_trusted() {
    let descriptor =
        SecurityDescriptor::from_sddl("O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;GRGX;;;BU)").unwrap();
    assert!(protected_metadata_descriptor(&descriptor));
}

#[test]
fn attacker_owned_precreated_child_is_not_trusted_even_with_read_only_acl() {
    let descriptor = SecurityDescriptor::from_sddl(
        "O:S-1-5-21-10-20-30-1001D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;GR;;;BU)",
    )
    .unwrap();
    assert!(!protected_metadata_descriptor(&descriptor));
}

#[test]
fn role_metadata_cannot_be_modified_or_replaced_by_other_accounts() {
    for right in ["GW", "WD", "WO", "SD", "FA"] {
        let descriptor = SecurityDescriptor::from_sddl(&format!(
            "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;{right};;;BU)"
        ))
        .unwrap();
        assert!(!protected_metadata_descriptor(&descriptor));
    }
}

#[test]
fn inherited_or_null_dacl_is_not_trusted() {
    for sddl in [
        "O:BAD:(A;;FA;;;SY)(A;;FA;;;BA)(A;;GR;;;BU)",
        "O:BAD:NO_ACCESS_CONTROL",
    ] {
        let descriptor = SecurityDescriptor::from_sddl(sddl).unwrap();
        assert!(!protected_metadata_descriptor(&descriptor));
    }
}

struct LockFixture(std::path::PathBuf);

impl LockFixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "LLRemoteManagementLockTest-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        Self(path)
    }

    fn reader(&self) -> Handle {
        use std::os::windows::io::IntoRawHandle;
        Handle::new(
            std::fs::File::open(&self.0)
                .unwrap()
                .into_raw_handle()
                .cast(),
            "unused",
        )
        .unwrap()
    }
}

impl Drop for LockFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn read_only_handles_can_take_an_exclusive_gate_and_release_it() {
    let fixture = LockFixture::new();
    let gate = ManagementLock::acquire(fixture.reader(), 0, std::time::Duration::ZERO).unwrap();
    let blocked = ManagementLock::acquire(fixture.reader(), 0, std::time::Duration::ZERO);
    assert_eq!(
        blocked.err().as_deref(),
        Some("windows_service_ipc_timeout")
    );
    drop(gate);
    assert!(ManagementLock::acquire(fixture.reader(), 0, std::time::Duration::ZERO).is_ok());
}

#[test]
fn role_gate_serializes_read_modify_write_on_independent_handles() {
    use std::sync::{Arc, Barrier, Mutex};
    let fixture = Arc::new(LockFixture::new());
    let start = Arc::new(Barrier::new(2));
    let roles = Arc::new(Mutex::new(Vec::<u32>::new()));
    let workers = [1, 2].map(|user| {
        let fixture = Arc::clone(&fixture);
        let start = Arc::clone(&start);
        let roles = Arc::clone(&roles);
        std::thread::spawn(move || {
            start.wait();
            let _gate =
                ManagementLock::acquire(fixture.reader(), 1, std::time::Duration::from_secs(1))
                    .unwrap();
            let mut saved = roles.lock().unwrap().clone();
            std::thread::sleep(std::time::Duration::from_millis(10));
            saved.push(user);
            *roles.lock().unwrap() = saved;
        })
    });
    for worker in workers {
        worker.join().unwrap();
    }
    let mut saved = roles.lock().unwrap().clone();
    saved.sort_unstable();
    assert_eq!(saved, [1, 2]);
}
