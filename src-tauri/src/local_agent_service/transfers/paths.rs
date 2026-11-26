fn sanitize_agent_file_name(file_name: &str) -> Result<String, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || Path::new(trimmed).components().count() != 1
    {
        return Err("agent_file_name_invalid".to_string());
    }

    Ok(trimmed.to_string())
}

fn validate_agent_transfer_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("agent_transfer_id_invalid".into());
    }
    Ok(())
}

fn ensure_transfer_writable(transfer: &AgentTransferState) -> Result<(), String> {
    if transfer.committing {
        return Err("agent_file_commit_in_progress".into());
    }
    Ok(())
}

/// Caller IDs are logical keys only; temporary disk names are private and exclusive.
fn create_agent_part_file(directory: &Path) -> Result<PathBuf, String> {
    let path = directory.join(format!(".llremote-upload-{}.part", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .map_err(|error| format!("agent_file_part_create_failed: {error}"))?;
    Ok(path)
}

/// Refuse replaced symlinks/reparse points instead of following them on resume or append.
fn open_agent_part_file(path: &Path, append: bool) -> Result<File, String> {
    let failed = |error| format!("agent_file_part_open_failed: {error}");
    let mut options = OpenOptions::new();
    options.read(!append).append(append);
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x0000_0100); // Darwin O_NOFOLLOW.
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(failed)?;
    let metadata = file.metadata().map_err(failed)?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err("agent_file_part_invalid".into());
        }
    }
    if !metadata.is_file() {
        return Err("agent_file_part_invalid".into());
    }
    Ok(file)
}

/// Relative paths always include the selected root directory and the file name.
/// Accept either separator across platforms, but never allow a path to escape the inbox.
fn create_transfer_receive_directory(
    receive_dir: &Path,
    file_name: &str,
    relative_path: Option<&str>,
) -> Result<PathBuf, String> {
    let normalized = relative_path.map(|path| path.replace('\\', "/"));
    let mut parts = Vec::new();
    if let Some(path) = normalized.as_deref() {
        for part in path.split('/') {
            if part.contains([':', '\0'])
                || part.trim_end_matches(['.', ' ']).is_empty()
                || sanitize_agent_file_name(part).is_err()
            {
                return Err("agent_file_relative_path_invalid".into());
            }
            parts.push(part);
        }
        if parts
            .pop()
            .and_then(|name| sanitize_agent_file_name(name).ok())
            .as_deref()
            != Some(file_name)
        {
            return Err("agent_file_relative_path_invalid".into());
        }
    }

    fs::create_dir_all(receive_dir)
        .map_err(|error| format!("agent_inbox_create_failed: {error}"))?;
    let mut directory = receive_dir.to_path_buf();
    for part in parts {
        directory.push(part);
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("agent_inbox_create_failed: {error}")),
        }
        // Existing child symlinks must not redirect a directory upload outside its inbox.
        let metadata = fs::symlink_metadata(&directory)
            .map_err(|error| format!("agent_inbox_create_failed: {error}"))?;
        if !metadata.file_type().is_dir() {
            return Err("agent_file_relative_path_invalid".into());
        }
    }
    Ok(directory)
}

fn unique_inbox_path(inbox_dir: &Path, file_name: &str) -> PathBuf {
    let candidate = inbox_dir.join(file_name);
    if fs::symlink_metadata(&candidate)
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..1000 {
        let name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = inbox_dir.join(name);
        if fs::symlink_metadata(&candidate)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return candidate;
        }
    }

    inbox_dir.join(format!("{}-{}", Uuid::new_v4(), file_name))
}

fn resolve_receive_directory(
    target_directory: Option<&str>,
    default_dir: &Path,
) -> Result<PathBuf, String> {
    let Some(directory) = target_directory
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(default_dir.to_path_buf());
    };
    let path = PathBuf::from(directory);
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("agent_target_directory_invalid".to_string());
    }
    Ok(path)
}

fn resolve_final_receive_path(
    receive_dir: &Path,
    file_name: &str,
    rename_policy: Option<&str>,
) -> Result<PathBuf, String> {
    let policy = rename_policy.unwrap_or("autoRename");
    let final_path = receive_dir.join(file_name);
    match policy {
        "autoRename" | "" => Ok(unique_inbox_path(receive_dir, file_name)),
        "overwrite" => Ok(final_path),
        "skip" if final_path.exists() => Err("agent_file_conflict_skipped".to_string()),
        "skip" => Ok(final_path),
        "ask" if final_path.exists() => Err("agent_file_conflict_requires_choice".to_string()),
        "ask" => Ok(final_path),
        _ => Err("agent_rename_policy_invalid".to_string()),
    }
}
