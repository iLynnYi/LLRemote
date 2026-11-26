fn now_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn canonical_directory(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("directory is required".to_string());
    }

    let directory = PathBuf::from(trimmed)
        .canonicalize()
        .map_err(|error| format!("failed to resolve directory: {error}"))?;
    if !directory.is_dir() {
        return Err("path is not a directory".to_string());
    }

    Ok(directory)
}

fn canonical_target(path: &str) -> Result<(PathBuf, PathBuf), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is required".to_string());
    }

    let target = PathBuf::from(trimmed)
        .canonicalize()
        .map_err(|error| format!("failed to resolve path: {error}"))?;
    let directory = if target.is_dir() {
        target.clone()
    } else {
        target
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "path has no parent directory".to_string())?
    };

    Ok((target, directory))
}
