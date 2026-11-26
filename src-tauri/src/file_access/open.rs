#[cfg(target_os = "macos")]
fn reveal_path(path: &str) -> Result<(), String> {
    let target = Path::new(path);
    let mut command = Command::new("open");
    if target.is_dir() {
        command.arg(path);
    } else {
        command.arg("-R").arg(path);
    }
    run_command(command, "failed to reveal path")
}

#[cfg(target_os = "windows")]
fn reveal_path(path: &str) -> Result<(), String> {
    // 复用原生 Shell 定位，避免 Explorer 参数解析规范化路径时的兼容问题。
    tauri_plugin_opener::reveal_item_in_dir(path)
        .map_err(|error| format!("failed to reveal path: {error}"))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn reveal_path(path: &str) -> Result<(), String> {
    let target = Path::new(path);
    let directory = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    };
    let mut command = Command::new("xdg-open");
    command.arg(directory);
    run_command(command, "failed to reveal path")
}

#[cfg(target_os = "macos")]
fn open_path(path: &str) -> Result<(), String> {
    let mut command = Command::new("open");
    command.arg(path);
    run_command(command, "failed to open path")
}

#[cfg(target_os = "windows")]
fn open_path(path: &str) -> Result<(), String> {
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|error| format!("failed to open path: {error}"))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn open_path(path: &str) -> Result<(), String> {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    run_command(command, "failed to open path")
}

#[cfg(not(target_os = "windows"))]
fn run_command(mut command: Command, context: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("{context}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{context}: command exited with {status}"))
    }
}
