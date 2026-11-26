//! Windows application directories shared by the GUI and background processes.
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Configure the Rust runtime before constructing the Tauri manager. The
/// locked CLI (2.11.3) rejects this newer Rust SDK field in JSON configuration.
/// Dynamic session windows inherit the same manager directory as the main one.
pub(crate) fn configure_directories(config: &mut tauri::Config) {
    use tauri::utils::config::{AppDirectoriesOverride, AppDirectoryOverrides};
    config.app.app_directories_override =
        Some(AppDirectoriesOverride::Directories(AppDirectoryOverrides {
            config: Some("$LOCALDATA/LLRemote/Data".into()),
            data: Some("$LOCALDATA/LLRemote/Data".into()),
            local_data: Some("$LOCALDATA/LLRemote/WebView".into()),
            cache: Some("$LOCALDATA/LLRemote/WebView".into()),
            log: Some("$LOCALDATA/LLRemote/Logs".into()),
        }));
}

fn user_directory(id: &windows_sys::core::GUID) -> io::Result<PathBuf> {
    // Match Tauri's Windows known-folder resolver, including service/user-agent
    // contexts that do not inherit LOCALAPPDATA in their process environment.
    let path = crate::windows_service::native::known_directory(id)
        .map_err(|_| io::Error::other("Windows user directory unavailable"))?;
    check_path(&path)?;
    Ok(path)
}

pub(crate) fn local_root() -> io::Result<PathBuf> {
    Ok(user_directory(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData)?.join("LLRemote"))
}

pub(crate) fn default_artifacts(data: &Path) -> PathBuf {
    data.parent().unwrap_or(data).join("Artifacts")
}

pub(crate) fn check_path(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(io::Error::other("redirected storage path"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
