/// The filesystem owns the no-replace decision, including competing processes.
/// Never implement overwrite by deleting the previous destination first.
fn commit_received_file(transfer: &mut AgentTransferState) -> Result<(), String> {
    for _ in 0..1024 {
        match move_received_file(
            &transfer.part_path,
            &transfer.final_path,
            transfer.rename_policy == "overwrite",
        ) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                match transfer.rename_policy.as_str() {
                    "autoRename" | "" => {
                        let directory = transfer
                            .final_path
                            .parent()
                            .ok_or("agent_target_directory_invalid")?;
                        transfer.final_path = unique_inbox_path(directory, &transfer.file_name);
                    }
                    "skip" => return Err("agent_file_conflict_skipped".into()),
                    "ask" => return Err("agent_file_conflict_requires_choice".into()),
                    _ => return Err(format!("agent_file_commit_rename_failed: {error}")),
                }
            }
            Err(error) => return Err(format!("agent_file_commit_rename_failed: {error}")),
        }
    }
    Err("agent_file_commit_name_busy".into())
}

#[cfg(target_os = "macos")]
fn move_received_file(source: &Path, target: &Path, overwrite: bool) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    if overwrite {
        return fs::rename(source, target);
    }
    unsafe extern "C" {
        fn renamex_np(
            from: *const std::ffi::c_char,
            to: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let source = std::ffi::CString::new(source.as_os_str().as_bytes())?;
    let target = std::ffi::CString::new(target.as_os_str().as_bytes())?;
    // Darwin stdio.h: RENAME_EXCL fails atomically for any existing directory entry.
    if unsafe { renamex_np(source.as_ptr(), target.as_ptr(), 0x0000_0004) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "windows")]
fn move_received_file(source: &Path, target: &Path, overwrite: bool) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = MOVEFILE_WRITE_THROUGH
        | if overwrite {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn move_received_file(source: &Path, target: &Path, overwrite: bool) -> std::io::Result<()> {
    if overwrite {
        return fs::rename(source, target);
    }
    fs::hard_link(source, target)?;
    // Destination is already committed. A leftover part can be cleaned later.
    let _ = fs::remove_file(source);
    Ok(())
}
