pub fn list_file_access_directories(store: &FileAccessStore) -> Vec<FileAccessDirectory> {
    store.directories.lock().unwrap().clone()
}

pub fn list_file_access_directory_statuses(
    store: &FileAccessStore,
) -> Vec<FileAccessDirectoryStatus> {
    store
        .directories
        .lock()
        .unwrap()
        .iter()
        .map(inspect_file_access_directory)
        .collect()
}

pub fn grant_file_access_directory(
    store: &FileAccessStore,
    request: FileAccessGrantRequest,
) -> Result<FileAccessDirectory, String> {
    store.ensure_available()?;
    let directory = canonical_directory(&request.directory)?;
    let path = directory.to_string_lossy().to_string();
    let mut directories = store.directories.lock().unwrap();

    if let Some(existing) = directories.iter().find(|entry| entry.path == path) {
        return Ok(existing.clone());
    }

    let entry = FileAccessDirectory {
        path,
        added_at: now_stamp(),
        is_builtin: false,
    };
    let mut next = directories.clone();
    next.push(entry.clone());
    sort_directories(&mut next);
    store.save(&next)?;
    *directories = next;
    Ok(entry)
}

pub fn revoke_file_access_directory(
    store: &FileAccessStore,
    request: FileAccessGrantRequest,
) -> Result<bool, String> {
    store.ensure_available()?;
    let trimmed = request.directory.trim();
    if trimmed.is_empty() {
        return Err("directory is required".to_string());
    }

    let path = PathBuf::from(trimmed)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(trimmed))
        .to_string_lossy()
        .to_string();
    let mut directories = store.directories.lock().unwrap();
    if directories.iter().any(|entry| entry.path == path && entry.is_builtin) {
        return Err("file_access_builtin_cannot_revoke".into());
    }
    let before = directories.len();
    let next: Vec<_> = directories.iter().filter(|entry| entry.path != path).cloned().collect();
    let removed = next.len() != before;
    if removed {
        store.save(&next)?;
        *directories = next;
    }
    Ok(removed)
}

pub fn set_builtin_file_access_directory(
    store: &FileAccessStore,
    request: FileAccessGrantRequest,
) -> Result<FileAccessDirectory, String> {
    store.ensure_available()?;
    let path = canonical_directory(&request.directory)?;
    verify_directory_writable(&path).map_err(|error| format!("file_access_directory_not_writable: {error}"))?;
    let path = path.to_string_lossy().to_string();
    let mut directories = store.directories.lock().unwrap();
    let mut next = directories.clone();
    for entry in &mut next { entry.is_builtin = entry.path == path; }
    if !next.iter().any(|entry| entry.is_builtin) {
        next.push(FileAccessDirectory { path, added_at: now_stamp(), is_builtin: true });
    }
    sort_directories(&mut next);
    let builtin = next[0].clone();
    store.save(&next)?;
    *directories = next;
    Ok(builtin)
}

fn sort_directories(directories: &mut [FileAccessDirectory]) {
    directories.sort_by(|left, right| right.is_builtin.cmp(&left.is_builtin).then_with(|| left.path.cmp(&right.path)));
}

/// Recreate only the fixed built-in grant; custom directories remain user managed.
pub fn prepare_builtin_file_access_directory(store: &FileAccessStore, directory: &str) -> Result<(), String> {
    store.ensure_available()?;
    let path = PathBuf::from(directory.trim());
    let normalized = path.canonicalize().unwrap_or(path);
    let builtin = store.directories.lock().unwrap().iter()
        .find(|entry| entry.is_builtin && Path::new(&entry.path) == normalized).cloned();
    if let Some(entry) = builtin {
        fs::create_dir_all(&entry.path).map_err(|error| format!("file_access_directory_create_failed: {error}"))?;
        verify_directory_writable(Path::new(&entry.path))
            .map_err(|error| format!("file_access_directory_not_writable: {error}"))?;
    }
    Ok(())
}
