use super::*;

#[test]
fn optional_data_failure_keeps_cleaning_other_roots_and_reports_the_residue() {
    let directory = Directory::new();
    directory.write("Local/LLRemote/Data");
    directory.write("Local/LLRemote/WebView/cache");
    directory.write("Local/LLRemote/Artifacts/keep.txt");
    let report = cleanup_user_data(&directory.0.join("Local"));
    assert_eq!(report.exit_code(), 200);
    assert!(report
        .warnings
        .iter()
        .any(|item| item.path.ends_with("Data")));
    assert!(!directory.0.join("Local/LLRemote/WebView").exists());
    assert!(directory
        .0
        .join("Local/LLRemote/Artifacts/keep.txt")
        .exists());
}

#[test]
fn only_transient_locks_are_retried_and_the_budget_is_bounded() {
    let mut calls = 0;
    let mut deadline = None;
    retry_busy(
        || {
            calls += 1;
            if calls == 1 {
                Err(io::Error::from_raw_os_error(32))
            } else {
                Ok(())
            }
        },
        &mut deadline,
    )
    .unwrap();
    assert_eq!(calls, 2);
    let mut calls = 0;
    let mut deadline = None;
    assert!(retry_busy(
        || {
            calls += 1;
            Err::<(), _>(io::Error::from_raw_os_error(5))
        },
        &mut deadline
    )
    .is_err());
    assert_eq!(calls, 1);
    let mut deadline = Some(std::time::Instant::now());
    assert_eq!(
        retry_busy(
            || Err::<(), _>(io::Error::from_raw_os_error(33)),
            &mut deadline
        )
        .unwrap_err()
        .raw_os_error(),
        Some(33)
    );
}

#[test]
fn data_cleanup_distinguishes_locked_denied_and_redirected_paths() {
    for (raw, expected) in [(32, 115), (33, 115), (5, 116), (123, 114)] {
        let error = cleanup_error(&io::Error::from_raw_os_error(raw));
        assert_eq!(super::super::error_exit_code(&error), expected);
    }
    let error = cleanup_error(&io::Error::other(RedirectedPath));
    assert_eq!(super::super::error_exit_code(&error), 117);
}

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        Self(
            fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("LLRemote-uninstall-{}", uuid::Uuid::new_v4())),
        )
    }
    fn write(&self, name: &str) {
        let file = self.0.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, b"fixture").unwrap();
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[test]
fn full_uninstall_refuses_redirected_data_directories() {
    let directory = Directory::new();
    directory.write("Outside/hosts.sqlite3");
    fs::create_dir_all(directory.0.join("Local/LLRemote")).unwrap();
    std::os::unix::fs::symlink(
        directory.0.join("Outside"),
        directory.0.join("Local/LLRemote/Data"),
    )
    .unwrap();
    let report = cleanup_user_data(&directory.0.join("Local"));
    assert!(report
        .warnings
        .iter()
        .any(|item| item.code == 117 && item.path.ends_with("Data")));
    assert_eq!(
        fs::read(directory.0.join("Outside/hosts.sqlite3")).unwrap(),
        b"fixture"
    );
}

#[test]
fn full_uninstall_removes_personal_storage_and_preserves_outputs() {
    let directory = Directory::new();
    let root = directory.0.join("Local/LLRemote");
    for name in [
        "App/LLRemote.exe",
        "Data/hosts.sqlite3",
        "Data/.app-version",
        "Logs/LLRemote.log",
        "Sharing/bin/LLRemoteService-build.exe",
        "WebView/Default/prefs",
        "Artifacts/screenshot.png",
        "ReceivedFiles/received.txt",
    ] {
        directory.write(&format!("Local/LLRemote/{name}"));
    }
    assert_eq!(cleanup_user_data(&directory.0.join("Local")).exit_code(), 0);
    for name in ["Data", "Logs", "Sharing", "WebView"] {
        assert!(!root.join(name).exists());
    }
    for name in [
        "App/LLRemote.exe",
        "Artifacts/screenshot.png",
        "ReceivedFiles/received.txt",
    ] {
        assert_eq!(fs::read(root.join(name)).unwrap(), b"fixture");
    }
    assert_eq!(cleanup_user_data(&directory.0.join("Local")).exit_code(), 0);
}
