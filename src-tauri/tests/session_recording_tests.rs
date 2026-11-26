use base64::Engine;
use std::{fs, io::Cursor, path::PathBuf};
use LLRemoteLib::session_artifacts::{
    webm::finalize_webm, RecordingChunkRequest, RecordingFinishRequest, RecordingStartRequest,
    SessionRecordingFiles,
};

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
    let entry = element(
        &[0xae],
        &[element(&[0xd7], &[1]), element(&[0x83], &[1])].concat(),
    );
    let tracks = element(&[0x16, 0x54, 0xae, 0x6b], &entry);
    // Unknown-size WebKit clusters: the parser must walk element boundaries,
    // including payload bytes resembling a Cluster ID, instead of scanning bytes.
    let block = element(&[0xa3], &[0x81, 0, 0, 0x80, 0x1f, 0x43, 0xb6, 0x75]);
    let mut clusters = Vec::new();
    for timestamp in [0_u16, 1000] {
        clusters.extend(
            [
                vec![
                    0x1f, 0x43, 0xb6, 0x75, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                ],
                element(&[0xe7], &timestamp.to_be_bytes()),
                block.clone(),
            ]
            .concat(),
        );
    }
    [
        header,
        vec![
            0x18, 0x53, 0x80, 0x67, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ],
        info,
        tracks,
        clusters,
    ]
    .concat()
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("vnc-recording-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn finalization_adds_duration_and_seekable_cues_without_changing_clusters() {
    let input = sample();
    let mut output = Cursor::new(Vec::new());
    finalize_webm(&mut Cursor::new(&input), &mut output, 2100.0).unwrap();
    let output = output.into_inner();
    assert!(output
        .windows(8)
        .any(|bytes| bytes == 2100.0_f64.to_be_bytes()));
    for id in [[0x11, 0x4d, 0x9b, 0x74], [0x1c, 0x53, 0xbb, 0x6b]] {
        assert!(output.windows(4).any(|bytes| bytes == id));
    }
    let block = [0xa3, 0x88, 0x81, 0, 0, 0x80, 0x1f, 0x43, 0xb6, 0x75];
    assert_eq!(
        output
            .windows(block.len())
            .filter(|bytes| *bytes == block)
            .count(),
        2
    );
}

#[test]
fn chunks_are_ordered_and_failed_finalization_can_retry_from_the_same_file() {
    let directory = Directory::new();
    let files = SessionRecordingFiles::default();
    let handle = files
        .start(RecordingStartRequest {
            directory: directory.0.to_string_lossy().into(),
            file_name: "recording.webm".into(),
        })
        .unwrap();
    let bytes = sample();
    for (index, chunk) in bytes.chunks(17).enumerate() {
        files
            .append(RecordingChunkRequest {
                recording_id: handle.recording_id.clone(),
                offset: (index * 17) as u64,
                data_base64: base64::engine::general_purpose::STANDARD.encode(chunk),
            })
            .unwrap();
    }
    files
        .append(RecordingChunkRequest {
            recording_id: handle.recording_id.clone(),
            offset: 0,
            data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes[..17]),
        })
        .unwrap();
    assert!(files
        .append(RecordingChunkRequest {
            recording_id: handle.recording_id.clone(),
            offset: 0,
            data_base64: "eA==".into()
        })
        .is_err());
    assert!(files
        .finish(RecordingFinishRequest {
            recording_id: handle.recording_id.clone(),
            duration_ms: 0.0
        })
        .is_err());
    assert_eq!(
        fs::read(PathBuf::from(&handle.recovery_path).join("0001.part")).unwrap(),
        bytes
    );
    assert!(!directory.0.join("recording.webm").exists());
    let saved = files
        .finish(RecordingFinishRequest {
            recording_id: handle.recording_id,
            duration_ms: 2100.0,
        })
        .unwrap();
    assert!(saved.bytes_written > bytes.len() as u64);
    assert!(!PathBuf::from(handle.recovery_path).exists());
}

#[test]
fn failed_recording_release_preserves_received_bytes_and_never_overwrites_existing_files() {
    let directory = Directory::new();
    let files = SessionRecordingFiles::default();
    let request = || RecordingStartRequest {
        directory: directory.0.to_string_lossy().into(),
        file_name: "recording.webm".into(),
    };
    fs::write(directory.0.join("recording.webm"), b"existing").unwrap();
    assert!(files.start(request()).is_err());
    fs::remove_file(directory.0.join("recording.webm")).unwrap();
    let handle = files.start(request()).unwrap();
    files
        .append(RecordingChunkRequest {
            recording_id: handle.recording_id.clone(),
            offset: 0,
            data_base64: "cGFydGlhbA==".into(),
        })
        .unwrap();
    files.release(&handle.recording_id).unwrap();
    assert_eq!(
        fs::read(PathBuf::from(handle.recovery_path).join("0001.part")).unwrap(),
        b"partial"
    );
    assert!(files
        .start(RecordingStartRequest {
            directory: directory.0.to_string_lossy().into(),
            file_name: "../bad.webm".into()
        })
        .is_err());
}

#[test]
fn malformed_and_empty_media_are_rejected_instead_of_reported_as_saved() {
    for bytes in [Vec::new(), vec![0], vec![0x1a, 0x45, 0xdf, 0xa3, 0xff]] {
        assert!(finalize_webm(
            &mut Cursor::new(bytes),
            &mut Cursor::new(Vec::new()),
            1000.0
        )
        .is_err());
    }
}

#[test]
#[ignore = "requires an explicitly supplied local recording and output path"]
fn finalize_supplied_capture() {
    let input = std::env::var("VNC_RECORDING_SAMPLE").expect("sample path");
    let output = std::env::var("VNC_RECORDING_OUTPUT").expect("output path");
    let duration: f64 = std::env::var("VNC_RECORDING_DURATION_MS")
        .expect("duration")
        .parse()
        .unwrap();
    let mut input = fs::File::open(input).unwrap();
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    finalize_webm(&mut input, &mut output, duration).unwrap();
}

#[test]
fn verification_rejects_wrong_cue_times_even_when_they_remain_monotonic() {
    use LLRemoteLib::session_artifacts::webm::verify_webm;
    let mut output = Cursor::new(Vec::new());
    finalize_webm(&mut Cursor::new(sample()), &mut output, 2100.0).unwrap();
    verify_webm(&mut output, 2100.0).unwrap();
    let mut bytes = output.into_inner();
    let index = bytes
        .windows(9)
        .position(|v| v == [0xb3, 1, 0, 0, 0, 0, 0, 0, 8])
        .unwrap();
    bytes[index + 16] = 1;
    assert!(verify_webm(&mut Cursor::new(bytes), 2100.0).is_err());
}
