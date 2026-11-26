use std::path::{Path, PathBuf};

pub(super) fn install_directory(root: &Path) -> PathBuf {
    root.join("LLRemote/Server")
}

pub(super) fn data_directory(root: &Path) -> PathBuf {
    root.join("LLRemote/Server")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_install_uses_llremote_paths() {
        let root = std::env::temp_dir().join(format!("LLRemote-layout-{}", uuid::Uuid::new_v4()));
        assert_eq!(install_directory(&root), root.join("LLRemote/Server"));
        assert_eq!(data_directory(&root), root.join("LLRemote/Server"));
    }
}
