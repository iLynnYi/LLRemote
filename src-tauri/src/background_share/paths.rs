use std::path::PathBuf;

pub(super) fn data_directory() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("background_share_user_directory_unavailable")?;
        Ok(PathBuf::from(home).join("Library/Application Support/LLRemote/Sharing"))
    }
    #[cfg(target_os = "windows")]
    {
        crate::windows_storage::local_root()
            .map(|root| root.join("Sharing"))
            .map_err(|_| "background_share_user_directory_unavailable".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err("background_share_user_directory_unavailable".into())
}

#[cfg(target_os = "macos")]
pub(super) fn runtime_directory() -> PathBuf {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    PathBuf::from(format!("/tmp/llremote-share-{}", unsafe { geteuid() }))
}

pub(super) fn ensure_private_directory(path: &std::path::Path) -> Result<(), String> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("background_share_user_directory_unavailable".into());
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder
            .create(path)
            .map_err(|_| "background_share_user_directory_unavailable".to_string())?;
        if std::fs::metadata(path)
            .map_err(|_| "background_share_user_directory_unavailable".to_string())?
            .uid()
            != unsafe { geteuid() }
        {
            return Err("background_share_user_directory_unavailable".into());
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "background_share_user_directory_unavailable".to_string())?;
    }
    #[cfg(target_os = "windows")]
    crate::windows_service::native::protect_user_directory(path)
        .map_err(|_| "background_share_user_directory_unavailable".to_string())?;
    Ok(())
}

pub(super) fn replace_file(
    source: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let target = target
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("background_share_config_unavailable".into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    std::fs::rename(source, target).map_err(|_| "background_share_config_unavailable".into())
}
