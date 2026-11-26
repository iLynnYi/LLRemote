use super::*;

#[test]
fn cancelled_picker_returns_no_sources_without_defaulting_to_current_folder() {
    assert!(selected_sources(Vec::new()).unwrap().is_empty());
}

#[test]
fn mixed_picker_sources_keep_unicode_files_and_directories_for_existing_expansion() {
    let files = vec![
        PathBuf::from("C:/用户/报告 1.txt"),
        PathBuf::from("C:/用户/资料"),
    ];
    assert_eq!(
        selected_sources(files).unwrap(),
        ["C:/用户/报告 1.txt", "C:/用户/资料"]
    );
}

#[cfg(unix)]
#[test]
fn picker_does_not_silently_replace_invalid_path_characters() {
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]));
    assert!(matches!(
        selected_sources(vec![path]),
        Err(SourcePickerError::SourcePicker(
            SourcePickerErrorCode::InvalidPath
        ))
    ));
}

#[cfg(target_os = "windows")]
#[test]
fn picker_does_not_silently_replace_invalid_path_characters() {
    use std::os::windows::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_wide(&[0xd800]));
    assert!(matches!(
        selected_sources(vec![path]),
        Err(SourcePickerError::SourcePicker(
            SourcePickerErrorCode::InvalidPath
        ))
    ));
}
