use std::path::PathBuf;

#[cfg(target_os = "windows")]
#[path = "picker_windows_dialog.rs"]
mod dialog;

#[derive(Debug, serde::Serialize)]
#[serde(tag = "category", content = "code", rename_all = "camelCase")]
pub enum SourcePickerError {
    SourcePicker(SourcePickerErrorCode),
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SourcePickerErrorCode {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Unavailable,
    InvalidPath,
}

/// A fresh STA owns the modal Shell dialog; the WebView and async runtime stay
/// responsive, and no file/folder mode question precedes the picker.
#[cfg(target_os = "windows")]
pub(crate) async fn select(window: tauri::WebviewWindow) -> Result<Vec<String>, SourcePickerError> {
    let owner = window.hwnd().map_err(|_| unavailable())?.0 as isize;
    let result = tauri::async_runtime::spawn_blocking(move || {
        std::thread::Builder::new()
            .name("upload-source-picker".into())
            .spawn(move || dialog::select(owner))
            .map_err(|_| unavailable())?
            .join()
            .map_err(|_| unavailable())?
    })
    .await
    .map_err(|_| unavailable())?;
    // Keep the parent window alive until its modal dialog has returned.
    drop(window);
    selected_sources(result?)
}

#[cfg(target_os = "windows")]
fn unavailable() -> SourcePickerError {
    SourcePickerError::SourcePicker(SourcePickerErrorCode::Unavailable)
}

fn selected_sources(paths: Vec<PathBuf>) -> Result<Vec<String>, SourcePickerError> {
    paths
        .into_iter()
        .map(|path| {
            path.to_str()
                .map(str::to_owned)
                .ok_or(SourcePickerError::SourcePicker(
                    SourcePickerErrorCode::InvalidPath,
                ))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/support/file_picker_tests.rs"]
mod tests;
