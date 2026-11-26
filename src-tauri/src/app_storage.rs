use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn application_directory(default: &Path) -> io::Result<PathBuf> {
    #[cfg(windows)]
    return Ok(default.to_path_buf());

    #[cfg(not(windows))]
    default
        .parent()
        .map(|parent| parent.join("LLRemote"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "application directory has no parent",
            )
        })
}
