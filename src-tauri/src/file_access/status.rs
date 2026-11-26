fn inspect_file_access_directory(entry: &FileAccessDirectory) -> FileAccessDirectoryStatus {
    let path = PathBuf::from(&entry.path);
    if entry.is_builtin { let _ = fs::create_dir_all(&path); }
    if !path.exists() {
        return FileAccessDirectoryStatus {
            path: entry.path.clone(),
            added_at: entry.added_at.clone(),
            is_builtin: entry.is_builtin,
            status: "missing".to_string(),
            exists: false,
            is_directory: false,
            writable: false,
            message: "authorized directory does not exist".to_string(),
        };
    }

    if !path.is_dir() {
        return FileAccessDirectoryStatus {
            path: entry.path.clone(),
            added_at: entry.added_at.clone(),
            is_builtin: entry.is_builtin,
            status: "not_directory".to_string(),
            exists: true,
            is_directory: false,
            writable: false,
            message: "authorized path is not a directory".to_string(),
        };
    }

    if let Err(error) = verify_directory_writable(&path) {
        return FileAccessDirectoryStatus {
            path: entry.path.clone(),
            added_at: entry.added_at.clone(),
            is_builtin: entry.is_builtin,
            status: "not_writable".to_string(),
            exists: true,
            is_directory: true,
            writable: false,
            message: format!("authorized directory is not writable: {error}"),
        };
    }

    FileAccessDirectoryStatus {
        path: entry.path.clone(),
        added_at: entry.added_at.clone(),
        is_builtin: entry.is_builtin,
        status: "ready".to_string(),
        exists: true,
        is_directory: true,
        writable: true,
        message: "authorized directory is ready".to_string(),
    }
}

fn verify_directory_writable(directory: &Path) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let probe_path = directory.join(format!(".LLRemote-file-access-test-{stamp}.tmp"));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
        .map_err(|error| error.to_string())?;
    fs::remove_file(&probe_path).map_err(|error| error.to_string())
}
