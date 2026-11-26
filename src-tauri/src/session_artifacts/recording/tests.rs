use super::*;
use std::io::Cursor;
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("recording-job-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn element(id: &[u8], data: &[u8]) -> Vec<u8> {
    assert!(data.len() < 127);
    [id, &[0x80 | data.len() as u8], data].concat()
}
fn sample() -> Vec<u8> {
    let header = element(&[0x1a, 0x45, 0xdf, 0xa3], &element(&[0x42, 0x82], b"webm"));
    let info = element(
        &[0x15, 0x49, 0xa9, 0x66],
        &element(&[0x2a, 0xd7, 0xb1], &[0x0f, 0x42, 0x40]),
    );
    let tracks = element(
        &[0x16, 0x54, 0xae, 0x6b],
        &element(
            &[0xae],
            &[element(&[0xd7], &[1]), element(&[0x83], &[1])].concat(),
        ),
    );
    let mut result = [
        header,
        vec![
            0x18, 0x53, 0x80, 0x67, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ],
        info,
        tracks,
    ]
    .concat();
    for time in [0_u16, 1000, 2000, 3000] {
        result.extend(
            [
                vec![
                    0x1f, 0x43, 0xb6, 0x75, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                ],
                element(&[0xe7], &time.to_be_bytes()),
                element(&[0xa3], &[0x81, 0, 0, 0x80, (time / 1000) as u8]),
            ]
            .concat(),
        );
    }
    result
}
fn start(files: &SessionRecordingFiles, directory: &Directory) -> RecordingFileHandle {
    files
        .start(RecordingStartRequest {
            directory: directory.0.to_string_lossy().into(),
            file_name: "capture.webm".into(),
        })
        .unwrap()
}
fn append(files: &SessionRecordingFiles, id: &str, bytes: &[u8], size: usize) {
    for (index, chunk) in bytes.chunks(size).enumerate() {
        files
            .append(RecordingChunkRequest {
                recording_id: id.into(),
                offset: (index * size) as u64,
                data_base64: base64::engine::general_purpose::STANDARD.encode(chunk),
            })
            .unwrap();
    }
}
fn wait_worker(files: &SessionRecordingFiles, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while files.get(id).unwrap().lock().unwrap().worker {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn continuous_encoder_segments_merge_with_all_blocks_and_cleanup_after_verification() {
    let directory = Directory::new();
    let files = SessionRecordingFiles::default();
    let handle = start(&files, &directory);
    files
        .get(&handle.recording_id)
        .unwrap()
        .lock()
        .unwrap()
        .time_limit = 900.0;
    let bytes = sample();
    append(&files, &handle.recording_id, &bytes, 7);
    wait_worker(&files, &handle.recording_id);
    let state = files.get(&handle.recording_id).unwrap();
    assert!(state.lock().unwrap().manifest.segments.len() >= 2);
    let root = state.lock().unwrap().root.clone();
    let saved = files
        .finish(RecordingFinishRequest {
            recording_id: handle.recording_id.clone(),
            duration_ms: 3100.0,
        })
        .unwrap();
    assert_eq!(saved.status, "saved");
    assert_eq!(saved.files.len(), 1);
    assert!(!root.exists());
    let mut output = File::open(saved.file_path).unwrap();
    webm::verify_webm(&mut output, 3100.0).unwrap();
    assert_eq!(
        webm::media_identity(&mut [&mut output]).unwrap(),
        webm::media_identity(&mut [Cursor::new(bytes)]).unwrap()
    );
}
#[test]
#[ignore = "requires an explicitly supplied encoded sample and scratch directory"]
fn split_supplied_capture() {
    let source = std::env::var("LLREMOTE_RECORDING_SAMPLE").unwrap();
    let directory = std::env::var("LLREMOTE_RECORDING_QA_DIRECTORY").unwrap();
    let duration: f64 = std::env::var("LLREMOTE_RECORDING_DURATION_MS")
        .unwrap()
        .parse()
        .unwrap();
    let files = SessionRecordingFiles::default();
    let handle = files
        .start(RecordingStartRequest {
            directory: directory.clone(),
            file_name: "merged.webm".into(),
        })
        .unwrap();
    files
        .get(&handle.recording_id)
        .unwrap()
        .lock()
        .unwrap()
        .time_limit = 900.0;
    append(
        &files,
        &handle.recording_id,
        &fs::read(source).unwrap(),
        257,
    );
    wait_worker(&files, &handle.recording_id);
    let state = files.get(&handle.recording_id).unwrap();
    {
        let mut r = state.lock().unwrap();
        r.file.take().unwrap().sync_all().unwrap();
        r.manifest.duration_ms = duration;
        let last = r.manifest.segments.last_mut().unwrap();
        last.duration_ms = duration - last.start_ms;
        files.shared.ledger.update(&handle.recording_id, r.demand());
    }
    files.finalize_segments(&handle.recording_id).unwrap();
    let segments = state.lock().unwrap().files();
    assert!(segments.len() > 1);
    for (index, segment) in segments.iter().enumerate() {
        fs::copy(
            &segment.file_path,
            Path::new(&directory).join(format!("segment-{index}.webm")),
        )
        .unwrap();
    }
    let result = files.merge(&handle.recording_id, None).unwrap();
    assert_eq!(result.status, "saved");
}
#[test]
fn scanner_handles_every_chunk_boundary_and_never_scans_pixel_payload_for_cluster_ids() {
    let bytes = sample();
    for size in 1..40 {
        let mut input = Cursor::new(Vec::new());
        let mut scanner = StreamScanner::default();
        let mut found = Vec::new();
        for chunk in bytes.chunks(size) {
            input.get_mut().extend_from_slice(chunk);
            found.extend(scanner.scan(&mut input).unwrap());
        }
        assert_eq!(
            found.iter().map(|v| v.time_ms as u64).collect::<Vec<_>>(),
            [0, 1000, 2000]
        );
        assert!(found.iter().all(|v| v.keyframe));
    }
}
#[test]
fn insufficient_merge_space_preserves_independent_segments_and_can_retry_elsewhere() {
    let directory = Directory::new();
    let other = Directory::new();
    let files = SessionRecordingFiles::default();
    let handle = start(&files, &directory);
    files
        .get(&handle.recording_id)
        .unwrap()
        .lock()
        .unwrap()
        .time_limit = 900.0;
    append(&files, &handle.recording_id, &sample(), 7);
    wait_worker(&files, &handle.recording_id);
    let state = files.get(&handle.recording_id).unwrap();
    {
        let mut r = state.lock().unwrap();
        r.file.take().unwrap().sync_all().unwrap();
        r.manifest.duration_ms = 3100.0;
        let last = r.manifest.segments.last_mut().unwrap();
        last.duration_ms = 3100.0 - last.start_ms;
        files.shared.ledger.update(&handle.recording_id, r.demand());
    }
    files.finalize_segments(&handle.recording_id).unwrap();
    let disk = space::disk_space(&directory.0).unwrap();
    files.shared.ledger.update(
        "busy-other-recording",
        Demand {
            volume: disk.volume,
            directory: directory.0.clone(),
            pending: u64::MAX,
            ..Default::default()
        },
    );
    let segmented = files.merge(&handle.recording_id, None).unwrap();
    assert_eq!(segmented.status, "segmented");
    assert!(segmented.files.len() >= 2);
    for part in segmented.files {
        assert!(Path::new(&part.file_path).exists());
    }
    files.shared.ledger.remove("busy-other-recording");
    let saved = files
        .retry(RecordingRetryRequest {
            recording_id: handle.recording_id,
            directory: Some(other.0.to_string_lossy().into()),
        })
        .unwrap();
    assert_eq!(saved.status, "saved");
    assert_eq!(
        Path::new(&saved.file_path).parent(),
        Some(other.0.canonicalize().unwrap().as_path())
    );
}
#[test]
fn restart_discovers_recording_and_recovers_a_torn_last_frame_without_touching_other_files() {
    let directory = Directory::new();
    let handle;
    {
        let files = SessionRecordingFiles::default();
        handle = start(&files, &directory);
        let mut bytes = sample();
        bytes.truncate(bytes.len() - 2);
        append(&files, &handle.recording_id, &bytes, 11);
        files
            .space(RecordingSpaceRequest {
                recording_id: handle.recording_id.clone(),
                pending_bytes: 0,
            })
            .unwrap();
    }
    fs::write(directory.0.join("unrelated.part"), b"keep").unwrap();
    let files = SessionRecordingFiles::default();
    let jobs = files.recover_directory(&directory.0).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "recoverable");
    let saved = files
        .retry(RecordingRetryRequest {
            recording_id: handle.recording_id,
            directory: None,
        })
        .unwrap();
    assert_eq!(saved.status, "saved");
    assert_eq!(
        fs::read(directory.0.join("unrelated.part")).unwrap(),
        b"keep"
    );
    let mut output = File::open(saved.file_path).unwrap();
    assert_eq!(webm::media_identity(&mut [&mut output]).unwrap().blocks, 3);
}
#[test]
fn same_second_new_recording_does_not_collide_with_background_job_destination() {
    let directory = Directory::new();
    let files = SessionRecordingFiles::default();
    let first = start(&files, &directory);
    let second = start(&files, &directory);
    let a = files.get(&first.recording_id).unwrap();
    let b = files.get(&second.recording_id).unwrap();
    assert_ne!(
        a.lock().unwrap().manifest.file_name,
        b.lock().unwrap().manifest.file_name
    );
}
#[test]
fn verification_rejects_corruption_before_intermediates_are_removed() {
    let directory = Directory::new();
    let files = SessionRecordingFiles::default();
    let handle = start(&files, &directory);
    append(&files, &handle.recording_id, &sample(), 17);
    let state = files.get(&handle.recording_id).unwrap();
    {
        let mut r = state.lock().unwrap();
        r.file.take();
        r.manifest.duration_ms = 3100.0;
        r.manifest.segments[0].duration_ms = 3100.0;
    }
    files.finalize_segments(&handle.recording_id).unwrap();
    let path = {
        let r = state.lock().unwrap();
        r.manifest.segments[0].video(&r.root)
    };
    fs::write(&path, b"corrupt").unwrap();
    assert!(files.merge(&handle.recording_id, None).is_err());
    assert!(path.exists());
    assert!(!directory.0.join("capture.webm").exists());
}

#[test]
fn relocation_preserves_stream_offsets_and_accepts_queued_chunks_on_the_new_disk() {
    let directory = Directory::new();
    let other = Directory::new();
    let files = SessionRecordingFiles::default();
    let handle = start(&files, &directory);
    let bytes = sample();
    let split = bytes.len() / 2;
    append(&files, &handle.recording_id, &bytes[..split], 9);
    files
        .relocate_file(RecordingRetryRequest {
            recording_id: handle.recording_id.clone(),
            directory: Some(other.0.to_string_lossy().into()),
        })
        .unwrap();
    files
        .append(RecordingChunkRequest {
            recording_id: handle.recording_id.clone(),
            offset: split as u64,
            data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes[split..]),
        })
        .unwrap();
    let result = files
        .finish(RecordingFinishRequest {
            recording_id: handle.recording_id,
            duration_ms: 3100.0,
        })
        .unwrap();
    assert_eq!(
        Path::new(&result.file_path).parent(),
        Some(other.0.canonicalize().unwrap().as_path())
    );
    let mut output = File::open(result.file_path).unwrap();
    assert_eq!(
        webm::media_identity(&mut [&mut output]).unwrap(),
        webm::media_identity(&mut [Cursor::new(bytes)]).unwrap()
    );
}

#[test]
fn encoded_payload_corruption_is_detected_even_when_container_and_cues_are_valid() {
    let directory = Directory::new();
    let files = SessionRecordingFiles::default();
    let handle = start(&files, &directory);
    append(&files, &handle.recording_id, &sample(), 17);
    let state = files.get(&handle.recording_id).unwrap();
    {
        let mut r = state.lock().unwrap();
        r.file.take();
        r.manifest.duration_ms = 3100.0;
        r.manifest.segments[0].duration_ms = 3100.0;
    }
    files.finalize_segments(&handle.recording_id).unwrap();
    let path = {
        let r = state.lock().unwrap();
        r.manifest.segments[0].video(&r.root)
    };
    let mut bytes = fs::read(&path).unwrap();
    let offset = bytes
        .windows(5)
        .position(|v| v == [0x81, 0, 0, 0x80, 0])
        .unwrap()
        + 4;
    bytes[offset] = 42;
    fs::write(&path, bytes).unwrap();
    webm::verify_webm(&mut File::open(&path).unwrap(), 3100.0).unwrap();
    assert_eq!(
        files.merge(&handle.recording_id, None).err().as_deref(),
        Some("recording_verification_failed")
    );
    assert!(path.exists());
    assert!(!directory.0.join("capture.webm").exists());
}

#[test]
fn busy_background_task_does_not_lock_the_registry_or_delay_a_new_recording() {
    let directory = Directory::new();
    let files = SessionRecordingFiles::default();
    let handle = start(&files, &directory);
    let state = files.get(&handle.recording_id).unwrap();
    let guard = state.lock().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let worker = files.clone();
    let path = directory.0.to_string_lossy().into_owned();
    let thread = std::thread::spawn(move || {
        let result = worker.start(RecordingStartRequest {
            directory: path,
            file_name: "capture.webm".into(),
        });
        let _ = send.send(result);
    });
    let result = receive.recv_timeout(Duration::from_secs(2));
    drop(guard);
    thread.join().unwrap();
    assert!(
        result.unwrap().is_ok(),
        "new capture must not acquire another task's lock"
    );
}
