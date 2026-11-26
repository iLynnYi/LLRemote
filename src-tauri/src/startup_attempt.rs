//! Last automatic listener start result, kept outside the listener process.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupAttempt {
    pub started_at_ms: u64,
    pub state: StartupAttemptState,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupAttemptState {
    Starting,
    Succeeded,
    Failed,
}

impl StartupAttempt {
    pub(crate) fn starting() -> Self {
        Self {
            started_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            state: StartupAttemptState::Starting,
            error_code: None,
        }
    }

    pub(crate) fn finish(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.state = StartupAttemptState::Succeeded;
                self.error_code = None;
            }
            Err(code) => {
                self.state = StartupAttemptState::Failed;
                self.error_code = Some(code.chars().take(512).collect());
            }
        }
    }
}

pub(crate) fn read(directory: &Path) -> Option<StartupAttempt> {
    let bytes = std::fs::read(directory.join("startup-attempt.json")).ok()?;
    (bytes.len() <= 4096)
        .then(|| serde_json::from_slice(&bytes).ok())
        .flatten()
}

pub(crate) fn write(directory: &Path, attempt: &StartupAttempt) -> std::io::Result<()> {
    let temporary = directory.join(format!("startup-attempt-{}.next", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec(attempt).map_err(std::io::Error::other)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&temporary)?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    drop(file);
    let target = directory.join("startup-attempt.json");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source = temporary
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let destination = target
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            let _ = std::fs::remove_file(temporary);
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let result = std::fs::rename(&temporary, target);
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_a_failed_login_attempt_after_the_process_exits() {
        let directory =
            std::env::temp_dir().join(format!("sharing-attempt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let mut attempt = StartupAttempt::starting();
        write(&directory, &attempt).unwrap();
        assert!(matches!(
            read(&directory).unwrap().state,
            StartupAttemptState::Starting
        ));
        attempt.finish(Err("permission_missing: screen_recording".into()));
        write(&directory, &attempt).unwrap();
        let persisted = read(&directory).unwrap();
        assert!(matches!(persisted.state, StartupAttemptState::Failed));
        assert_eq!(
            persisted.error_code.as_deref(),
            Some("permission_missing: screen_recording")
        );
        assert_eq!(persisted.started_at_ms, attempt.started_at_ms);
        attempt.finish(Ok(()));
        write(&directory, &attempt).unwrap();
        let recovered = read(&directory).unwrap();
        assert!(matches!(recovered.state, StartupAttemptState::Succeeded));
        assert!(recovered.error_code.is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
