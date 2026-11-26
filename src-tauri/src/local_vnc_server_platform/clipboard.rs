pub(crate) fn set_clipboard_text_from_vnc(text: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if windows_desktop::is_service_worker() {
        return Err("clipboard_write_unsupported: privileged service clipboard is disabled".into());
    }
    set_platform_clipboard_text(text)
}

pub(crate) fn get_clipboard_text_for_vnc() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    if windows_desktop::is_service_worker() { return Ok(None); }
    get_platform_clipboard_text()
}

#[cfg(target_os = "macos")]
fn set_platform_clipboard_text(text: &str) -> Result<(), String> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("clipboard_write_failed: unable to start pbcopy: {error}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "clipboard_write_failed: pbcopy stdin unavailable".to_string())?;
        stdin.write_all(text.as_bytes()).map_err(|error| {
            format!("clipboard_write_failed: unable to write UTF-8 text: {error}")
        })?;
    }

    let status = child
        .wait()
        .map_err(|error| format!("clipboard_write_failed: pbcopy wait failed: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "clipboard_write_failed: pbcopy exited with {status}"
        ))
    }
}

#[cfg(target_os = "macos")]
fn get_platform_clipboard_text() -> Result<Option<String>, String> {
    let output = Command::new("pbpaste")
        .output()
        .map_err(|error| format!("clipboard_read_failed: unable to start pbpaste: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "clipboard_read_failed: pbpaste exited with {}",
            output.status,
        ));
    }

    let text = String::from_utf8(output.stdout)
        .map_err(|error| format!("clipboard_read_failed: clipboard was not UTF-8 text: {error}"))?;
    Ok(Some(text))
}

#[cfg(target_os = "windows")]
fn set_platform_clipboard_text(text: &str) -> Result<(), String> {
    use std::ptr::{copy_nonoverlapping, null_mut};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let byte_len = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return Err("clipboard_write_failed: OpenClipboard failed".to_string());
        }

        let result = (|| {
            if EmptyClipboard() == 0 {
                return Err("clipboard_write_failed: EmptyClipboard failed".to_string());
            }

            let handle = GlobalAlloc(GMEM_MOVEABLE, byte_len);
            if handle.is_null() {
                return Err("clipboard_write_failed: GlobalAlloc failed".to_string());
            }

            let target = GlobalLock(handle) as *mut u16;
            if target.is_null() {
                let _ = GlobalFree(handle);
                return Err("clipboard_write_failed: GlobalLock failed".to_string());
            }

            copy_nonoverlapping(wide.as_ptr(), target, wide.len());
            let _ = GlobalUnlock(handle);

            if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
                let _ = GlobalFree(handle);
                return Err(
                    "clipboard_write_failed: SetClipboardData(CF_UNICODETEXT) failed".to_string(),
                );
            }

            Ok(())
        })();

        let _ = CloseClipboard();
        result
    }
}

#[cfg(target_os = "windows")]
fn get_platform_clipboard_text() -> Result<Option<String>, String> {
    use std::slice;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("clipboard_read_failed: OpenClipboard failed".to_string());
        }
        let result = (|| {
            if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) == 0 {
                return Ok(None);
            }
            let handle = GetClipboardData(CF_UNICODETEXT as u32);
            if handle.is_null() {
                return Err("clipboard_read_failed: GetClipboardData failed".to_string());
            }
            let text_ptr = GlobalLock(handle) as *const u16;
            if text_ptr.is_null() {
                return Err("clipboard_read_failed: GlobalLock failed".to_string());
            }
            let max_units = GlobalSize(handle) / std::mem::size_of::<u16>();
            let mut length = 0usize;
            while length < max_units && *text_ptr.add(length) != 0 {
                length += 1;
            }
            if length == max_units {
                let _ = GlobalUnlock(handle);
                return Err("clipboard_read_failed: missing UTF-16 terminator".to_string());
            }
            let text = String::from_utf16(slice::from_raw_parts(text_ptr, length))
                .map_err(|error| format!("clipboard_read_failed: invalid UTF-16 text: {error}"))?;
            let _ = GlobalUnlock(handle);
            Ok(Some(text))
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set_platform_clipboard_text(_text: &str) -> Result<(), String> {
    Err("clipboard_write_unsupported: local VNC server clipboard is only implemented on macOS and Windows".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn get_platform_clipboard_text() -> Result<Option<String>, String> {
    Err("clipboard_read_unsupported: local VNC server clipboard read is currently implemented for macOS only".to_string())
}
