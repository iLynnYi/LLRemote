use super::*;

struct Uploads {
    root: PathBuf,
    transfers: AgentTransferMap,
}

impl Uploads {
    fn new() -> Self {
        let root = env::temp_dir().join(format!("llremote-file-commit-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self {
            root,
            transfers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn request(value: serde_json::Value) -> AgentRequest {
        serde_json::from_value(value).unwrap()
    }

    fn prepare(&self, id: &str, policy: &str, data: &[u8]) {
        create_file_transfer(
            Self::request(serde_json::json!({
                "type": FILE_TRANSFER_CREATE, "transferId": id, "fileName": "中文 same.txt",
                "size": data.len(), "renamePolicy": policy,
            })),
            &self.transfers,
            &self.root,
        )
        .unwrap();
        write_file_transfer_chunk(
            Self::request(serde_json::json!({
                "type": FILE_TRANSFER_CHUNK, "transferId": id, "offset": 0,
                "dataBase64": general_purpose::STANDARD.encode(data),
            })),
            &self.transfers,
        )
        .unwrap();
    }

    fn commit(&self, id: &str, data: &[u8]) -> Result<AgentResponse, String> {
        commit_file_transfer(
            Self::request(serde_json::json!({
                "type": FILE_TRANSFER_COMMIT, "transferId": id, "size": data.len(),
                "sha256": hex::encode(Sha256::digest(data)),
            })),
            &self.transfers,
        )
    }
}

impl Drop for Uploads {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn concurrent_auto_rename_commits_keep_both_uploads() {
    let uploads = Uploads::new();
    uploads.prepare("first", "autoRename", b"first");
    uploads.prepare("second", "autoRename", b"second");
    let barrier = std::sync::Barrier::new(2);
    let (first, second) = thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            uploads.commit("first", b"first")
        });
        let b = scope.spawn(|| {
            barrier.wait();
            uploads.commit("second", b"second")
        });
        (a.join().unwrap().unwrap(), b.join().unwrap().unwrap())
    });
    let first_path = first.remote_path.unwrap();
    let second_path = second.remote_path.unwrap();
    assert_ne!(first_path, second_path);
    assert_eq!(fs::read(first_path).unwrap(), b"first");
    assert_eq!(fs::read(second_path).unwrap(), b"second");
    assert_eq!(fs::read_dir(&uploads.root).unwrap().count(), 2);
}

#[test]
fn auto_rename_preserves_a_file_created_after_transfer_start() {
    let uploads = Uploads::new();
    uploads.prepare("first", "autoRename", b"upload");
    let existing = uploads.root.join("中文 same.txt");
    fs::write(&existing, b"existing").unwrap();
    let result = uploads.commit("first", b"upload").unwrap();
    assert_eq!(fs::read(existing).unwrap(), b"existing");
    assert_eq!(fs::read(result.remote_path.unwrap()).unwrap(), b"upload");
}

#[test]
fn late_skip_and_ask_conflicts_keep_original_and_retryable_upload() {
    for (policy, error) in [
        ("skip", "agent_file_conflict_skipped"),
        ("ask", "agent_file_conflict_requires_choice"),
    ] {
        let uploads = Uploads::new();
        uploads.prepare("first", policy, b"upload");
        let existing = uploads.root.join("中文 same.txt");
        fs::write(&existing, b"existing").unwrap();
        assert_eq!(uploads.commit("first", b"upload").unwrap_err(), error);
        assert_eq!(fs::read(&existing).unwrap(), b"existing");
        let guard = uploads.transfers.lock().unwrap();
        assert_eq!(fs::read(&guard["first"].part_path).unwrap(), b"upload");
        drop(guard);
        fs::remove_file(existing).unwrap();
        assert!(uploads.commit("first", b"upload").is_ok());
    }
}

#[test]
fn overwrite_replaces_only_after_complete_checksum_validation() {
    let uploads = Uploads::new();
    let existing = uploads.root.join("中文 same.txt");
    fs::write(&existing, b"existing").unwrap();
    uploads.prepare("first", "overwrite", b"upload");
    let invalid = commit_file_transfer(
        Uploads::request(serde_json::json!({
            "type": FILE_TRANSFER_COMMIT, "transferId": "first", "size": 6, "sha256": "invalid",
        })),
        &uploads.transfers,
    )
    .unwrap_err();
    assert_eq!(invalid, "agent_file_checksum_mismatch");
    assert_eq!(fs::read(&existing).unwrap(), b"existing");
    uploads.prepare("second", "overwrite", b"complete");
    uploads.commit("second", b"complete").unwrap();
    assert_eq!(fs::read(existing).unwrap(), b"complete");
}

#[test]
fn failed_atomic_overwrite_keeps_the_existing_destination() {
    let uploads = Uploads::new();
    let existing = uploads.root.join("中文 same.txt");
    fs::write(&existing, b"existing").unwrap();
    assert!(move_received_file(&uploads.root.join("missing.part"), &existing, true).is_err());
    assert_eq!(fs::read(existing).unwrap(), b"existing");
}

#[test]
fn commit_io_failure_keeps_the_partial_file_and_transfer_for_retry() {
    let uploads = Uploads::new();
    uploads.prepare("first", "overwrite", b"upload");
    uploads
        .transfers
        .lock()
        .unwrap()
        .get_mut("first")
        .unwrap()
        .final_path = uploads.root.join("missing/target.txt");
    assert!(uploads
        .commit("first", b"upload")
        .unwrap_err()
        .starts_with("agent_file_commit_rename_failed"));
    let guard = uploads.transfers.lock().unwrap();
    assert_eq!(fs::read(&guard["first"].part_path).unwrap(), b"upload");
}

#[test]
fn transfer_ids_cannot_truncate_files_outside_the_receive_directory() {
    let uploads = Uploads::new();
    let inbox = uploads.root.join("inbox");
    fs::create_dir(&inbox).unwrap();
    let outside = uploads.root.join("victim.part");
    fs::write(&outside, b"keep").unwrap();
    for id in [
        "/../victim",
        "../victim",
        "a/b",
        "a\\b",
        "C:other",
        "",
        ".",
        "..",
        &"a".repeat(129),
    ] {
        let result = create_file_transfer(
            Uploads::request(serde_json::json!({
                "type": FILE_TRANSFER_CREATE, "transferId": id, "fileName": "test.txt", "size": 0,
            })),
            &uploads.transfers,
            &inbox,
        );
        assert_eq!(result.unwrap_err(), "agent_transfer_id_invalid", "{id}");
    }
    assert_eq!(fs::read(outside).unwrap(), b"keep");
    assert_eq!(fs::read_dir(inbox).unwrap().count(), 0);
}

#[test]
fn caller_id_does_not_choose_or_truncate_a_temporary_file() {
    let uploads = Uploads::new();
    let existing = uploads.root.join(".first.part");
    fs::write(&existing, b"keep").unwrap();
    uploads.prepare("first", "autoRename", b"upload");
    let guard = uploads.transfers.lock().unwrap();
    assert_ne!(guard["first"].part_path, existing);
    assert_eq!(fs::read(existing).unwrap(), b"keep");
}

#[test]
fn concurrent_create_with_one_id_preserves_acknowledged_bytes() {
    let uploads = Uploads::new();
    let barrier = std::sync::Barrier::new(8);
    thread::scope(|scope| {
        let threads = (0..8).map(|_| scope.spawn(|| {
            barrier.wait();
            create_file_transfer(Uploads::request(serde_json::json!({
                "type": FILE_TRANSFER_CREATE, "transferId": "same-id", "fileName": "test.txt", "size": 6,
            })), &uploads.transfers, &uploads.root).unwrap();
        })).collect::<Vec<_>>();
        for worker in threads {
            worker.join().unwrap();
        }
    });
    assert_eq!(fs::read_dir(&uploads.root).unwrap().count(), 1);
    write_file_transfer_chunk(Uploads::request(serde_json::json!({
        "type": FILE_TRANSFER_CHUNK, "transferId": "same-id", "offset": 0, "dataBase64": "dXBsb2Fk",
    })), &uploads.transfers).unwrap();
    let response = create_file_transfer(Uploads::request(serde_json::json!({
        "type": FILE_TRANSFER_CREATE, "transferId": "same-id", "fileName": "test.txt", "size": 6,
    })), &uploads.transfers, &uploads.root).unwrap();
    assert_eq!(response.offset, Some(6));
    assert_eq!(
        fs::read(&uploads.transfers.lock().unwrap()["same-id"].part_path).unwrap(),
        b"upload"
    );
}

#[test]
fn committing_transfer_cannot_be_recreated_mutated_or_cancelled() {
    let uploads = Uploads::new();
    uploads.prepare("first", "autoRename", b"upload");
    uploads
        .transfers
        .lock()
        .unwrap()
        .get_mut("first")
        .unwrap()
        .committing = true;
    let request = || {
        Uploads::request(serde_json::json!({
            "type": FILE_TRANSFER_CREATE, "transferId": "first", "fileName": "中文 same.txt",
            "size": 6, "offset": 6, "dataBase64": "",
        }))
    };
    let errors = [
        create_file_transfer(request(), &uploads.transfers, &uploads.root).unwrap_err(),
        resume_file_transfer(request(), &uploads.transfers).unwrap_err(),
        write_file_transfer_chunk(request(), &uploads.transfers).unwrap_err(),
        pause_file_transfer(request(), &uploads.transfers).unwrap_err(),
        cancel_file_transfer(request(), &uploads.transfers).unwrap_err(),
        uploads.commit("first", b"upload").unwrap_err(),
    ];
    assert!(errors
        .iter()
        .all(|error| error == "agent_file_commit_in_progress"));
    let guard = uploads.transfers.lock().unwrap();
    assert_eq!(fs::read(&guard["first"].part_path).unwrap(), b"upload");
}

#[cfg(unix)]
#[test]
fn a_replaced_part_symlink_cannot_append_to_an_outside_file() {
    let uploads = Uploads::new();
    uploads.prepare("first", "autoRename", b"upload");
    let part = uploads.transfers.lock().unwrap()["first"].part_path.clone();
    let outside = uploads.root.join("outside.txt");
    fs::write(&outside, b"keep").unwrap();
    fs::remove_file(&part).unwrap();
    std::os::unix::fs::symlink(&outside, &part).unwrap();
    assert!(open_agent_part_file(&part, false).is_err());
    let request = Uploads::request(serde_json::json!({
        "type": FILE_TRANSFER_CHUNK, "transferId": "first", "offset": 6, "dataBase64": "eA==",
    }));
    // Increase the declared size so rejection must come from file safety, not size validation.
    uploads
        .transfers
        .lock()
        .unwrap()
        .get_mut("first")
        .unwrap()
        .expected_size = 7;
    assert!(write_file_transfer_chunk(request, &uploads.transfers).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
    assert!(uploads.commit("first", b"upload").is_err());
    assert_eq!(fs::read(outside).unwrap(), b"keep");
}
