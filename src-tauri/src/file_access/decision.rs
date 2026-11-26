pub fn inspect_file_access(
    store: &FileAccessStore,
    request: FileAccessPathRequest,
) -> FileAccessDecision {
    if let Err(code) = store.ensure_available() {
        return FileAccessDecision {
            status: "settings_unavailable".into(),
            path: request.path.trim().into(),
            directory: String::new(),
            message: code,
        };
    }
    let (target, directory) = match canonical_target(&request.path) {
        Ok(result) => result,
        Err(error) => {
            return FileAccessDecision {
                status: "invalid_path".to_string(),
                path: request.path.trim().to_string(),
                directory: String::new(),
                message: error,
            };
        }
    };

    let target_text = target.to_string_lossy().to_string();
    let directory_text = directory.to_string_lossy().to_string();
    let directories = store.directories.lock().unwrap();
    let authorized_directory = directories
        .iter()
        .find(|entry| target.starts_with(Path::new(&entry.path)))
        .map(|entry| entry.path.clone());

    if let Some(authorized_directory) = authorized_directory {
        FileAccessDecision {
            status: "authorized".to_string(),
            path: target_text,
            directory: authorized_directory,
            message: "path is authorized".to_string(),
        }
    } else {
        FileAccessDecision {
            status: "needs_authorization".to_string(),
            path: target_text,
            directory: directory_text,
            message: "directory access authorization is required".to_string(),
        }
    }
}

pub fn reveal_authorized_path(
    store: &FileAccessStore,
    request: FileAccessPathRequest,
) -> Result<FileAccessActionResult, String> {
    let decision = inspect_file_access(store, request);
    if decision.status != "authorized" {
        return Ok(FileAccessActionResult {
            status: decision.status,
            path: decision.path,
            directory: decision.directory,
            message: decision.message,
        });
    }

    reveal_path(&decision.path)?;
    Ok(FileAccessActionResult {
        status: "opened".to_string(),
        path: decision.path,
        directory: decision.directory,
        message: "path revealed".to_string(),
    })
}

pub fn open_authorized_path(
    store: &FileAccessStore,
    request: FileAccessPathRequest,
) -> Result<FileAccessActionResult, String> {
    let decision = inspect_file_access(store, request);
    if decision.status != "authorized" {
        return Ok(FileAccessActionResult {
            status: decision.status,
            path: decision.path,
            directory: decision.directory,
            message: decision.message,
        });
    }

    open_path(&decision.path)?;
    Ok(FileAccessActionResult {
        status: "opened".to_string(),
        path: decision.path,
        directory: decision.directory,
        message: "path opened".to_string(),
    })
}
