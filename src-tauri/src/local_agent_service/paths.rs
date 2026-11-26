fn reveal_file_response(request: AgentRequest) -> Result<AgentResponse, String> {
    open_remote_path(
        &request
            .path
            .ok_or_else(|| "agent_file_path_required".to_string())?,
        true,
    )?;
    Ok(ok_response())
}

fn open_file_response(request: AgentRequest) -> Result<AgentResponse, String> {
    open_remote_path(
        &request
            .path
            .ok_or_else(|| "agent_file_path_required".to_string())?,
        false,
    )?;
    Ok(ok_response())
}

fn open_remote_path(path: &str, reveal: bool) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("agent_file_path_required".to_string());
    }

    let target = resolve_agent_path(path);
    if !target.exists() {
        return Err("agent_file_path_missing".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        let status = if reveal && target.is_file() {
            Command::new("open").arg("-R").arg(&target).status()
        } else {
            Command::new("open").arg(&target).status()
        };
        status
            .map_err(|error| format!("agent_file_open_failed: {error}"))
            .and_then(command_status_result)
    }

    #[cfg(target_os = "windows")]
    {
        let status = if reveal && target.is_file() {
            Command::new("explorer")
                .arg(format!("/select,{}", target.to_string_lossy()))
                .status()
        } else {
            Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(&target)
                .status()
        };
        status
            .map_err(|error| format!("agent_file_open_failed: {error}"))
            .and_then(command_status_result)
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let reveal_target = if reveal && target.is_file() {
            target.parent().unwrap_or(target.as_path())
        } else {
            target.as_path()
        };
        let status = Command::new("xdg-open").arg(reveal_target).status();
        status
            .map_err(|error| format!("agent_file_open_failed: {error}"))
            .and_then(command_status_result)
    }
}

fn resolve_agent_path(path: &str) -> PathBuf {
    match env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
        Some(home) => resolve_agent_path_for_home(path, Path::new(&home)),
        None => PathBuf::from(path),
    }
}

pub fn resolve_agent_path_for_home(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn command_status_result(status: std::process::ExitStatus) -> Result<(), String> {
    if status.success() {
        Ok(())
    } else {
        Err(format!("agent_file_open_failed: status {status}"))
    }
}

fn agent_inbox_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    return crate::windows_storage::local_root().map(|root| root.join("ReceivedFiles")).map_err(|_| "agent_inbox_unavailable".to_string());
    #[cfg(not(windows))]
    Ok(default_agent_inbox_dir().unwrap_or_else(|| std::env::temp_dir().join("LLRemote-agent-inbox")))
}

#[cfg(not(windows))]
fn default_agent_inbox_dir() -> Option<PathBuf> {
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
    Some(default_agent_inbox_dir_for_home(Path::new(&home)))
}

pub fn default_agent_inbox_dir_for_home(home: &Path) -> PathBuf {
    home.join("Downloads").join("LL Remote")
}
