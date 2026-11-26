use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use LLRemoteLib::session_artifacts::{
    inspect_session_artifact_directory, write_session_artifact_file, SessionArtifactWriteRequest,
};

fn temp_artifact_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("llremote-artifacts-{name}-{stamp}"))
}

#[test]
fn writes_base64_artifact_file_inside_selected_directory() {
    let directory = temp_artifact_dir("write");
    fs::create_dir_all(&directory).expect("create temp dir");

    let result = write_session_artifact_file(SessionArtifactWriteRequest {
        directory: directory.to_string_lossy().to_string(),
        file_name: "20260710-080910_target_session_screenshot.png".to_string(),
        data_base64: "aGVsbG8=".to_string(),
    })
    .expect("write artifact");

    assert_eq!(
        result.file_name,
        "20260710-080910_target_session_screenshot.png"
    );
    assert_eq!(result.bytes_written, 5);
    assert_eq!(fs::read(result.file_path).expect("read artifact"), b"hello");
}

#[test]
fn rejects_empty_directory_and_path_traversal_file_names() {
    let empty_dir = write_session_artifact_file(SessionArtifactWriteRequest {
        directory: "".to_string(),
        file_name: "shot.png".to_string(),
        data_base64: "aGVsbG8=".to_string(),
    });
    assert!(empty_dir
        .expect_err("empty directory should fail")
        .contains("directory"));

    let directory = temp_artifact_dir("traversal");
    fs::create_dir_all(&directory).expect("create temp dir");

    let traversal = write_session_artifact_file(SessionArtifactWriteRequest {
        directory: directory.to_string_lossy().to_string(),
        file_name: "../secret.png".to_string(),
        data_base64: "aGVsbG8=".to_string(),
    });

    assert!(traversal
        .expect_err("path traversal should fail")
        .contains("file name"));
}

#[test]
fn inspects_artifact_directory_status_before_writing() {
    let directory = temp_artifact_dir("inspect");

    let missing = inspect_session_artifact_directory(directory.to_string_lossy().to_string());
    assert_eq!(missing.status, "missing");
    assert!(!missing.exists);
    assert!(!missing.writable);

    fs::create_dir_all(&directory).expect("create temp dir");
    let ready = inspect_session_artifact_directory(directory.to_string_lossy().to_string());
    assert_eq!(ready.status, "ready");
    assert!(ready.exists);
    assert!(ready.is_directory);
    assert!(ready.writable);

    let file_path = directory.join("not-a-directory");
    fs::write(&file_path, b"not a directory").expect("write marker file");
    let not_directory = inspect_session_artifact_directory(file_path.to_string_lossy().to_string());
    assert_eq!(not_directory.status, "not_directory");
    assert!(not_directory.exists);
    assert!(!not_directory.is_directory);
    assert!(!not_directory.writable);
}
