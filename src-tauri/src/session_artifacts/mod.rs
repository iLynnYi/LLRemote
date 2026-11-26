use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;

mod recording;
pub mod space;
pub mod webm;
pub use recording::{
    RecordingChunkRequest, RecordingFileHandle, RecordingFinishRequest, RecordingJob,
    RecordingRetryRequest, RecordingSaveResult, RecordingSpaceRequest, RecordingStartRequest,
    SessionRecordingFiles,
};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionArtifactWriteRequest {
    pub directory: String,
    pub file_name: String,
    pub data_base64: String,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionArtifactWriteResult {
    pub file_path: String,
    pub file_name: String,
    pub bytes_written: u64,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionArtifactDirectoryStatus {
    pub directory: String,
    pub status: String,
    pub exists: bool,
    pub is_directory: bool,
    pub writable: bool,
    pub message: String,
}

pub fn inspect_session_artifact_directory(directory: String) -> SessionArtifactDirectoryStatus {
    let trimmed = directory.trim().to_string();
    if trimmed.is_empty() {
        return SessionArtifactDirectoryStatus {
            directory: trimmed,
            status: "empty".to_string(),
            exists: false,
            is_directory: false,
            writable: false,
            message: "artifact directory is required".to_string(),
        };
    }

    let path = PathBuf::from(&trimmed);
    if !path.exists() {
        return SessionArtifactDirectoryStatus {
            directory: trimmed,
            status: "missing".to_string(),
            exists: false,
            is_directory: false,
            writable: false,
            message: "artifact directory does not exist".to_string(),
        };
    }

    if !path.is_dir() {
        return SessionArtifactDirectoryStatus {
            directory: trimmed,
            status: "not_directory".to_string(),
            exists: true,
            is_directory: false,
            writable: false,
            message: "artifact directory path is not a directory".to_string(),
        };
    }

    if let Err(error) = verify_directory_writable(&path) {
        return SessionArtifactDirectoryStatus {
            directory: trimmed,
            status: "not_writable".to_string(),
            exists: true,
            is_directory: true,
            writable: false,
            message: format!("artifact directory is not writable: {error}"),
        };
    }

    SessionArtifactDirectoryStatus {
        directory: trimmed,
        status: "ready".to_string(),
        exists: true,
        is_directory: true,
        writable: true,
        message: "artifact directory is ready".to_string(),
    }
}

fn verify_directory_writable(directory: &Path) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let probe_path = directory.join(format!(".LLRemote-artifact-write-test-{stamp}.tmp"));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
        .map_err(|error| error.to_string())?;
    fs::remove_file(&probe_path).map_err(|error| error.to_string())
}

fn validate_directory(directory: &str) -> Result<PathBuf, String> {
    let status = inspect_session_artifact_directory(directory.to_string());
    if status.status != "ready" {
        return Err(status.message);
    }

    Ok(PathBuf::from(status.directory))
}

fn validate_file_name(file_name: &str) -> Result<&str, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err("artifact file name is required".to_string());
    }

    let path = Path::new(trimmed);
    if path.components().count() != 1
        || path.file_name().and_then(|name| name.to_str()) != Some(trimmed)
    {
        return Err("artifact file name must not contain path separators".to_string());
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("secret")
        || lower.contains("token")
    {
        return Err("artifact file name must not contain sensitive fields".to_string());
    }

    Ok(trimmed)
}

pub fn write_session_artifact_file(
    request: SessionArtifactWriteRequest,
) -> Result<SessionArtifactWriteResult, String> {
    let directory = validate_directory(&request.directory)?;
    let file_name = validate_file_name(&request.file_name)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.data_base64.trim())
        .map_err(|error| format!("artifact data is not valid base64: {error}"))?;
    let file_path = directory.join(file_name);

    fs::write(&file_path, &bytes)
        .map_err(|error| format!("failed to write artifact file: {error}"))?;

    Ok(SessionArtifactWriteResult {
        file_path: file_path.to_string_lossy().to_string(),
        file_name: file_name.to_string(),
        bytes_written: bytes.len() as u64,
    })
}
