#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessDirectory {
    pub path: String,
    pub added_at: String,
    #[serde(default)]
    pub is_builtin: bool,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessDirectoryStatus {
    pub path: String,
    pub added_at: String,
    pub is_builtin: bool,
    pub status: String,
    pub exists: bool,
    pub is_directory: bool,
    pub writable: bool,
    pub message: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessGrantRequest {
    pub directory: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessPathRequest {
    pub path: String,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessDecision {
    pub status: String,
    pub path: String,
    pub directory: String,
    pub message: String,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessActionResult {
    pub status: String,
    pub path: String,
    pub directory: String,
    pub message: String,
}

#[derive(Debug)]
pub struct FileAccessStore {
    file_path: PathBuf,
    directories: Mutex<Vec<FileAccessDirectory>>,
    settings_error: Option<&'static str>,
}
