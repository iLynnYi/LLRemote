use super::*;
use std::io::{Seek, SeekFrom};

struct AgentFixture {
    port: u16,
    session_id: String,
    root: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl AgentFixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let root = env::temp_dir().join(format!("vnc-agent-client-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("inbox")).unwrap();
        let inbox = root.join("inbox");
        let bootstrap = Uuid::new_v4().to_string();
        let worker_bootstrap = bootstrap.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            run_agent_listener(
                listener,
                worker_stop,
                AgentBootstrap {
                    key: worker_bootstrap,
                    tickets: Arc::new(Mutex::new(HashMap::new())),
                },
                "client-fixture".into(),
                Arc::new(Mutex::new(HashMap::new())),
                inbox,
                AgentRuntimeContext {
                    shared_display: Arc::new(RwLock::new(
                        shared_display_for_agent_request_from_inventory(vec![], None, None),
                    )),
                    view_only: false,
                    // Prevent fixture requests from reaching native desktop APIs.
                    interactive: Some(Arc::new(AtomicBool::new(false))),
                },
            );
        });
        let mut fixture = Self {
            port,
            session_id: String::new(),
            root,
            stop,
            worker: Some(worker),
        };
        let opened = fixture.send(serde_json::json!({
            "type": "agent.session.open", "bootstrapKey": bootstrap,
        }));
        assert_eq!(opened["ok"], true);
        fixture.session_id = opened["sessionId"].as_str().unwrap().to_string();
        fixture
    }

    fn send(&self, message: serde_json::Value) -> serde_json::Value {
        invoke_remote_agent("127.0.0.1", self.port, message).unwrap()
    }

    fn request(&self, mut message: serde_json::Value) -> serde_json::Value {
        message["sessionId"] = serde_json::json!(self.session_id);
        self.send(message)
    }

    fn upload(
        &self,
        session_id: String,
        on_progress: impl FnMut(AgentFileUploadProgress),
    ) -> Result<AgentFileUploadResult, String> {
        upload_agent_file_with_progress(
            AgentFileUploadRequest {
                host: "127.0.0.1".into(),
                port: self.port,
                session_id,
                file_path: self
                    .root
                    .join("中文 config.txt")
                    .to_string_lossy()
                    .into_owned(),
                transfer_id: None,
                relative_path: None,
                target_directory: None,
                rename_policy: None,
            },
            on_progress,
        )
    }
}

impl Drop for AgentFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn generic_agent_client_preserves_auth_errors_for_retired_wrapper_actions() {
    let fixture = AgentFixture::new();
    for action in [
        CLIPBOARD_TEXT_WRITE,
        AGENT_PERMISSIONS_DIAGNOSE,
        AGENT_DISPLAY_ENUMERATE,
        AGENT_DISPLAY_SELECT,
        DISPLAY_WAKE,
        REMOTE_DESKTOP_CONTROL,
        AGENT_SECURE_INPUT_RECOVER,
        FILE_OPEN,
        FILE_REVEAL,
        FILE_TRANSFER_PAUSE,
        FILE_TRANSFER_CANCEL,
    ] {
        let response = fixture.send(serde_json::json!({
            "type": action, "sessionId": Uuid::new_v4().to_string(),
        }));
        assert_eq!(response["ok"], false, "{action}");
        assert_eq!(response["error"], "agent_session_invalid", "{action}");
    }
    let recovery = fixture.request(serde_json::json!({"type": AGENT_SECURE_INPUT_RECOVER}));
    assert_eq!(recovery["ok"], true);
    assert_eq!(recovery["secureInput"]["status"], "unsupported");
    assert_eq!(recovery["secureInput"]["action"], "none");
}

#[test]
fn current_upload_client_checks_auth_and_transfers_utf8_file_with_progress_and_checksum() {
    let fixture = AgentFixture::new();
    let contents = b"hello file transfer";
    fs::write(fixture.root.join("中文 config.txt"), contents).unwrap();
    assert_eq!(
        fixture
            .upload(Uuid::new_v4().to_string(), |_| {})
            .unwrap_err(),
        "agent_session_invalid"
    );
    let mut progress = Vec::new();
    let result = fixture
        .upload(fixture.session_id.clone(), |update| progress.push(update))
        .unwrap();
    assert_eq!(result.file_name, "中文 config.txt");
    assert_eq!(result.bytes_written, contents.len() as u64);
    assert_eq!(result.sha256, hex::encode(Sha256::digest(contents)));
    assert!(Path::new(&result.remote_path).starts_with(fixture.root.join("inbox")));
    assert_eq!(fs::read(&result.remote_path).unwrap(), contents);
    assert_eq!(progress.first().unwrap().transferred_bytes, 0);
    assert_eq!(
        progress.first().unwrap().phase,
        AgentFileUploadPhase::Preparing
    );
    assert_eq!(
        progress.last().unwrap().phase,
        AgentFileUploadPhase::Verifying
    );
    assert_eq!(
        progress.last().unwrap().transferred_bytes,
        contents.len() as u64
    );
}

#[test]
fn directory_upload_preserves_root_nested_paths_and_same_named_files() {
    let fixture = AgentFixture::new();
    let source = fixture.root.join("项目资料");
    fs::create_dir_all(source.join("子目录/deep")).unwrap();
    fs::create_dir_all(source.join("empty")).unwrap();
    for (path, content) in [
        ("config.txt", "root"),
        ("子目录/config.txt", "nested"),
        ("子目录/deep/config.txt", "deep"),
    ] {
        fs::write(source.join(path), content).unwrap();
    }
    let page = crate::file_transfer::FileTransferSourceScan::new(vec![source
        .to_string_lossy()
        .into_owned()])
    .next_page(200)
    .unwrap();
    assert!(page.done);
    let sources = page.sources;
    assert_eq!(sources.len(), 3);
    for target_directory in [None, Some(fixture.root.join("custom receive"))] {
        let receive_dir = target_directory
            .clone()
            .unwrap_or_else(|| fixture.root.join("inbox"));
        for source in &sources {
            // Exercise the same camelCase request boundary used by the native command.
            let request = serde_json::from_value(serde_json::json!({
                "host": "127.0.0.1", "port": fixture.port, "sessionId": fixture.session_id,
                "filePath": source.file_path, "relativePath": source.relative_path,
                "targetDirectory": target_directory,
            }))
            .unwrap();
            let result = upload_agent_file_with_progress(request, |_| {}).unwrap();
            let expected = receive_dir.join(source.relative_path.as_deref().unwrap());
            assert_eq!(Path::new(&result.remote_path), expected);
            assert_eq!(
                fs::read(&expected).unwrap(),
                fs::read(&source.file_path).unwrap()
            );
        }
        assert!(!receive_dir.join("config.txt").exists());
        assert!(!receive_dir.join("项目资料/empty").exists());
    }
}

#[test]
fn directory_upload_validates_paths_and_resumes_only_the_same_relative_file() {
    let fixture = AgentFixture::new();
    for relative_path in [
        "../resume.txt",
        "/resume.txt",
        "C:/resume.txt",
        "..\\resume.txt",
        "\\\\server\\share\\resume.txt",
        "Project/./resume.txt",
        "Project//resume.txt",
        "Project/other.txt",
        "",
        "Project/x:y/resume.txt",
        "Project/\0/resume.txt",
    ] {
        let response = fixture.request(serde_json::json!({
            "type": FILE_TRANSFER_CREATE, "fileName": "resume.txt", "size": 10,
            "relativePath": relative_path,
        }));
        assert_eq!(
            response["error"], "agent_file_relative_path_invalid",
            "{relative_path:?}"
        );
    }
    assert_eq!(fs::read_dir(fixture.root.join("inbox")).unwrap().count(), 0);

    let relative_path = "Project\\sub\\resume.txt";
    let created = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CREATE, "fileName": "resume.txt", "size": 10,
        "relativePath": relative_path,
    }));
    assert_eq!(created["ok"], true);
    let transfer_id = created["transferId"].as_str().unwrap();
    let chunk = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CHUNK, "transferId": transfer_id, "offset": 0, "dataBase64": "YWJjZA==",
    }));
    assert_eq!(chunk["offset"], 4);
    for action in [FILE_TRANSFER_CREATE, FILE_TRANSFER_RESUME] {
        let response = fixture.request(serde_json::json!({
            "type": action, "transferId": transfer_id, "fileName": "resume.txt", "size": 10,
            "relativePath": relative_path,
        }));
        assert_eq!(response["ok"], true);
        assert_eq!(response["offset"], 4);
        let changed_path = fixture.request(serde_json::json!({
            "type": action, "transferId": transfer_id, "fileName": "resume.txt", "size": 10,
            "relativePath": "Other/resume.txt",
        }));
        assert_eq!(changed_path["error"], "agent_transfer_fingerprint_mismatch");
    }
    let chunk = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CHUNK, "transferId": transfer_id, "offset": 4, "dataBase64": "ZWZnaGlq",
    }));
    assert_eq!(chunk["offset"], 10);
    let committed = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_COMMIT, "transferId": transfer_id, "size": 10,
        "sha256": hex::encode(Sha256::digest(b"abcdefghij")),
    }));
    assert_eq!(committed["ok"], true);
    let expected = fixture.root.join("inbox/Project/sub/resume.txt");
    assert_eq!(
        Path::new(committed["remotePath"].as_str().unwrap()),
        expected
    );
    assert_eq!(fs::read(expected).unwrap(), b"abcdefghij");
}

#[cfg(unix)]
#[test]
fn directory_upload_cannot_follow_a_child_symlink_outside_the_receive_directory() {
    let fixture = AgentFixture::new();
    let outside = fixture.root.join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.root.join("inbox/Project")).unwrap();
    let response = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CREATE, "fileName": "resume.txt", "size": 10,
        "relativePath": "Project/sub/resume.txt",
    }));
    assert_eq!(response["error"], "agent_file_relative_path_invalid");
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[test]
fn paused_transfer_rejects_new_chunks_and_commit_until_resumed() {
    let fixture = AgentFixture::new();
    let created = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CREATE, "fileName": "paused.txt", "size": 4,
    }));
    let transfer_id = created["transferId"].as_str().unwrap();
    let paused = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_PAUSE, "transferId": transfer_id,
    }));
    assert_eq!(paused["ok"], true);
    let chunk = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CHUNK, "transferId": transfer_id, "offset": 0, "dataBase64": "YWJjZA==",
    }));
    assert_eq!(chunk["error"], "agent_file_paused");
    let resumed = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CREATE, "transferId": transfer_id, "fileName": "paused.txt", "size": 4,
    }));
    assert_eq!(resumed["offset"], 0);
    let chunk = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CHUNK, "transferId": transfer_id, "offset": 0, "dataBase64": "YWJjZA==",
    }));
    assert_eq!(chunk["offset"], 4);
    let paused = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_PAUSE, "transferId": transfer_id,
    }));
    assert_eq!(paused["offset"], 4);
    let commit = serde_json::json!({
        "type": FILE_TRANSFER_COMMIT, "transferId": transfer_id, "size": 4,
        "sha256": hex::encode(Sha256::digest(b"abcd")),
    });
    let blocked = fixture.request(commit.clone());
    assert_eq!(blocked["error"], "agent_file_paused");
    let resumed = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_RESUME, "transferId": transfer_id, "fileName": "paused.txt", "size": 4,
    }));
    assert_eq!(resumed["offset"], 4);
    assert_eq!(fixture.request(commit)["ok"], true);
    assert_eq!(
        fs::read(fixture.root.join("inbox/paused.txt")).unwrap(),
        b"abcd"
    );
}

#[test]
fn upload_worker_stops_after_pause_and_continues_from_the_received_offset() {
    let fixture = AgentFixture::new();
    let contents = vec![b'x'; DEFAULT_FILE_CHUNK_BYTES * 2 + 31];
    let source = fixture.root.join("中文 config.txt");
    fs::write(&source, &contents).unwrap();
    let mut paused_id = None;
    let error = fixture
        .upload(fixture.session_id.clone(), |progress| {
            if paused_id.is_none() && progress.transferred_bytes == DEFAULT_FILE_CHUNK_BYTES as u64
            {
                let response = fixture.request(serde_json::json!({
                    "type": FILE_TRANSFER_PAUSE, "transferId": progress.transfer_id,
                }));
                assert_eq!(response["ok"], true);
                paused_id = Some(progress.transfer_id);
            }
        })
        .unwrap_err();
    assert_eq!(error, "agent_file_paused");
    let transfer_id = paused_id.unwrap();
    let status = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_STATUS, "transferId": transfer_id,
    }));
    assert_eq!(status["offset"], DEFAULT_FILE_CHUNK_BYTES);
    let mut progress_events = Vec::new();
    let result = upload_agent_file_with_progress(
        AgentFileUploadRequest {
            host: "127.0.0.1".into(),
            port: fixture.port,
            session_id: fixture.session_id.clone(),
            file_path: source.to_string_lossy().into_owned(),
            transfer_id: Some(transfer_id),
            relative_path: None,
            target_directory: None,
            rename_policy: None,
        },
        |progress| progress_events.push((progress.phase, progress.transferred_bytes)),
    )
    .unwrap();
    assert!(progress_events.contains(&(
        AgentFileUploadPhase::Preparing,
        DEFAULT_FILE_CHUNK_BYTES as u64
    )));
    assert!(progress_events.contains(&(
        AgentFileUploadPhase::Transferring,
        DEFAULT_FILE_CHUNK_BYTES as u64
    )));
    assert_eq!(
        progress_events.last(),
        Some(&(AgentFileUploadPhase::Verifying, contents.len() as u64))
    );
    assert_eq!(fs::read(result.remote_path).unwrap(), contents);
}

#[test]
fn upload_hash_matches_bytes_sent_when_unread_source_changes_after_first_chunk() {
    let fixture = AgentFixture::new();
    let source = fixture.root.join("中文 config.txt");
    let mut expected = vec![b'x'; DEFAULT_FILE_CHUNK_BYTES * 2 + 31];
    fs::write(&source, &expected).unwrap();
    let mut changed = false;
    let result = fixture
        .upload(fixture.session_id.clone(), |progress| {
            if !changed && progress.transferred_bytes == DEFAULT_FILE_CHUNK_BYTES as u64 {
                let mut file = OpenOptions::new().write(true).open(&source).unwrap();
                file.seek(SeekFrom::Start(DEFAULT_FILE_CHUNK_BYTES as u64))
                    .unwrap();
                let tail = vec![b'y'; expected.len() - DEFAULT_FILE_CHUNK_BYTES];
                file.write_all(&tail).unwrap();
                expected[DEFAULT_FILE_CHUNK_BYTES..].copy_from_slice(&tail);
                changed = true;
            }
        })
        .unwrap();
    assert!(changed);
    assert_eq!(fs::read(&result.remote_path).unwrap(), expected);
}

#[test]
fn generic_agent_client_preserves_path_validation_resume_and_cancel() {
    let fixture = AgentFixture::new();
    let invalid = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CREATE, "fileName": "../escape.txt", "size": 10,
    }));
    assert_eq!(invalid["error"], "agent_file_name_invalid");
    let created = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CREATE, "fileName": "resume.txt", "size": 10,
    }));
    assert_eq!(created["ok"], true);
    let transfer_id = created["transferId"].as_str().unwrap();
    let chunk = fixture.request(serde_json::json!({
        "type": FILE_TRANSFER_CHUNK, "transferId": transfer_id, "offset": 0, "dataBase64": "YWJjZA==",
    }));
    assert_eq!(chunk["offset"], 4);
    for action in [FILE_TRANSFER_STATUS, FILE_TRANSFER_RESUME] {
        let response = fixture.request(serde_json::json!({
            "type": action, "transferId": transfer_id, "fileName": "resume.txt", "size": 10,
        }));
        assert_eq!(response["ok"], true);
        assert_eq!(response["offset"], 4);
    }
    let cancelled = fixture
        .request(serde_json::json!({"type": FILE_TRANSFER_CANCEL, "transferId": transfer_id}));
    assert_eq!(cancelled["ok"], true);
    assert_eq!(fs::read_dir(fixture.root.join("inbox")).unwrap().count(), 0);
}
