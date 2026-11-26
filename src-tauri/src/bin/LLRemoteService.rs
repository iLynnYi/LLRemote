#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(not(target_os = "macos"))]
fn main() {
    if let Err(code) = LLRemoteLib::windows_service::run_from_args() {
        // Errors are fixed categories, never request bodies or credential values.
        eprintln!("{code}");
        std::process::exit(LLRemoteLib::windows_service::maintenance_exit_code(&code));
    }
}

#[cfg(target_os = "macos")]
fn main() {
    // A service app never falls through to the foreground Tauri application,
    // including when Finder opens it without launchd's background arguments.
    if let Err(code) = LLRemoteLib::background_share::run_if_requested() {
        eprintln!("{code}");
        std::process::exit(1);
    }
}
