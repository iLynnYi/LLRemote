// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    match LLRemoteLib::windows_uninstall::run_update_if_requested() {
        Ok(Some(exit)) => std::process::exit(exit),
        Ok(None) => {}
        Err(code) => {
            eprintln!("{code}");
            std::process::exit(1);
        }
    }
    match LLRemoteLib::windows_uninstall::run_if_requested() {
        Ok(Some(exit)) => std::process::exit(exit),
        Ok(None) => {}
        Err(code) => {
            eprintln!("{code}");
            std::process::exit(LLRemoteLib::windows_uninstall::error_exit_code(&code));
        }
    }
    match LLRemoteLib::windows_service::run_if_requested() {
        Ok(true) => return,
        Ok(false) => {}
        Err(code) => {
            eprintln!("{code}");
            std::process::exit(LLRemoteLib::windows_service::maintenance_exit_code(&code));
        }
    }
    match LLRemoteLib::background_share::run_if_requested() {
        Ok(true) => return,
        Ok(false) => {}
        Err(code) => {
            eprintln!("{code}");
            std::process::exit(1);
        }
    }
    LLRemoteLib::run()
}
