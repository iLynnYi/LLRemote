use std::{fs, io, path::Path};

#[derive(Debug)]
struct RedirectedPath;
impl std::fmt::Display for RedirectedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("refusing redirected application data path")
    }
}
impl std::error::Error for RedirectedPath {}

pub(crate) fn cleanup_error(error: &io::Error) -> String {
    let category = if error
        .get_ref()
        .is_some_and(|cause| cause.is::<RedirectedPath>())
    {
        "windows_uninstall_data_unsafe_path"
    } else if matches!(error.raw_os_error(), Some(32 | 33)) {
        // Windows sharing/lock violations; these are not access-right failures.
        "windows_uninstall_data_in_use"
    } else if error.raw_os_error() == Some(5) || error.kind() == io::ErrorKind::PermissionDenied {
        "windows_uninstall_data_access_denied"
    } else {
        "windows_uninstall_data_cleanup_failed"
    };
    format!("{category}:{}", error.raw_os_error().unwrap_or(0))
}

/// Optional cleanup never prevents removal of an already stopped application.
pub(super) fn cleanup_user_data(local: &Path) -> super::report::CleanupReport {
    let mut report = super::report::CleanupReport::default();
    let root = local.join("LLRemote");
    let mut deadline = None;
    for name in ["Logs", "Data", "WebView", "Sharing"] {
        let path = root.join(name);
        if let Err(error) = retry_busy(|| remove_tree(&path), &mut deadline) {
            report.warning(&cleanup_error(&error), &path);
        }
    }
    if let Err(error) = remove_empty(&root) {
        report.warning(&cleanup_error(&error), &root);
    }
    report
}

/// One shared five-second budget, only for actual sharing/lock violations.
fn retry_busy<T>(
    mut operation: impl FnMut() -> io::Result<T>,
    deadline: &mut Option<std::time::Instant>,
) -> io::Result<T> {
    use std::time::{Duration, Instant};
    loop {
        match operation() {
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => {
                let end = *deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(5));
                if Instant::now() >= end {
                    return Err(error);
                }
                std::thread::sleep(
                    Duration::from_millis(100).min(end.saturating_duration_since(Instant::now())),
                );
            }
            result => return result,
        }
    }
}

pub(crate) fn remove_tree(path: &Path) -> io::Result<()> {
    check_path(path)?;
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn check_path(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err(io::Error::other(RedirectedPath));
                    }
                }
                if metadata.file_type().is_symlink() {
                    return Err(io::Error::other(RedirectedPath));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn remove_empty(path: &Path) -> io::Result<()> {
    check_path(path)?;
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_uninstall_data_tests.rs"]
mod tests;
