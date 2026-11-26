pub(crate) fn wake_remote_display() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return wake_macos_display();

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_DISPLAY_REQUIRED};

        // Reset the display idle timer for this request and leave the power plan in effect.
        if unsafe { SetThreadExecutionState(ES_DISPLAY_REQUIRED) } == 0 {
            return Err("display_wake_failed: SetThreadExecutionState failed".to_string());
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err("display_wake_unsupported".to_string())
}

/// Submit a lock request in the logged-in user's Agent, never the service process.
pub(crate) fn lock_screen() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if unsafe { !AXIsProcessTrusted() } {
            return Err("screen_lock_permission_missing".to_string());
        }
        // Apple's Control-Command-Q shortcut. Allocate both events before posting
        // so a failed allocation cannot leave a partially sent shortcut.
        // https://support.apple.com/en-us/102650
        unsafe {
            let down = CGEventCreateKeyboardEvent(std::ptr::null(), 12, true);
            let up = CGEventCreateKeyboardEvent(std::ptr::null(), 12, false);
            if down.is_null() || up.is_null() {
                if !down.is_null() {
                    CFRelease(down.cast());
                }
                if !up.is_null() {
                    CFRelease(up.cast());
                }
                return Err("screen_lock_event_failed".to_string());
            }
            let flags = K_CG_EVENT_FLAG_MASK_CONTROL | (1 << 20); // Command
            CGEventSetIntegerValueField(
                down,
                42,
                crate::keyboard_input_source::OWN_INPUT_TAG as i64,
            );
            CGEventSetIntegerValueField(up, 42, crate::keyboard_input_source::OWN_INPUT_TAG as i64);
            CGEventSetFlags(down, flags);
            CGEventSetFlags(up, flags);
            CGEventPost(K_CG_HID_EVENT_TAP, down);
            CGEventPost(K_CG_HID_EVENT_TAP, up);
            CFRelease(down.cast());
            CFRelease(up.cast());
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        // Agent runs on the interactive user's desktop. Success means submitted.
        if unsafe { LockWorkStation() } == 0 {
            return Err("screen_lock_failed".to_string());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err("screen_lock_unsupported".to_string())
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn LockWorkStation() -> i32;
}

pub(crate) fn control_remote_desktop(action_id: &str) -> Result<(), String> {
    control_macos_remote_desktop(action_id)
}

pub(crate) fn send_secure_attention_sequence() -> Result<(), String> {
    send_platform_secure_attention_sequence()
}

#[cfg(target_os = "macos")]
fn control_macos_remote_desktop(action_id: &str) -> Result<(), String> {
    let (arrow_keysym, key_code) = macos_remote_desktop_arrow(action_id)?;

    if unsafe { !AXIsProcessTrusted() } {
        return Err(
            "remote_desktop_control_permission_missing: accessibility permission required"
                .to_string(),
        );
    }

    inject_platform_key_event(true, 0xffe3, Some("ControlLeft"));
    std::thread::sleep(std::time::Duration::from_millis(16));
    inject_macos_key_event_with_flags(true, arrow_keysym, None, K_CG_EVENT_FLAG_MASK_CONTROL);
    std::thread::sleep(std::time::Duration::from_millis(16));
    inject_macos_key_event_with_flags(false, arrow_keysym, None, K_CG_EVENT_FLAG_MASK_CONTROL);
    std::thread::sleep(std::time::Duration::from_millis(16));
    inject_platform_key_event(false, 0xffe3, Some("ControlLeft"));
    trigger_macos_system_events_control_arrow(key_code)
}

#[cfg(target_os = "macos")]
fn macos_remote_desktop_arrow(action_id: &str) -> Result<(u32, u16), String> {
    match action_id {
        "previous-desktop" => Ok((0xff51, 123)),
        "next-desktop" => Ok((0xff53, 124)),
        "overview" => Ok((0xff52, 126)),
        "show-current-desktop" => Ok((0xff54, 125)),
        _ => Err("remote_desktop_action_unsupported".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn trigger_macos_system_events_control_arrow(key_code: u16) -> Result<(), String> {
    let script =
        format!("tell application \"System Events\" to key code {key_code} using control down");
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| {
            format!(
                "remote_desktop_control_system_events_failed: unable to start osascript: {error}"
            )
        })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        format!(
            "remote_desktop_control_system_events_failed: osascript exited with {}",
            output.status
        )
    } else {
        format!("remote_desktop_control_system_events_failed: {stderr}")
    })
}

#[cfg(not(target_os = "macos"))]
fn control_macos_remote_desktop(_action_id: &str) -> Result<(), String> {
    Err("remote_desktop_control_unsupported: remote desktop control is currently implemented for macOS Agent targets only".to_string())
}

#[cfg(target_os = "windows")]
fn send_platform_secure_attention_sequence() -> Result<(), String> {
    send_windows_secure_attention_sequence()
}

#[cfg(target_os = "windows")]
fn send_windows_secure_attention_sequence() -> Result<(), String> {
    if windows_desktop::is_service_worker() {
        let mut session = 0;
        if unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                windows_sys::Win32::System::Threading::GetCurrentProcessId(),
                &mut session,
            )
        } == 0
        {
            return Err("windows_service_worker_identity_failed".into());
        }
        return crate::windows_service::request_secure_attention(session);
    }
    Err("secure_attention_unsupported".into())
}

#[cfg(not(target_os = "windows"))]
fn send_platform_secure_attention_sequence() -> Result<(), String> {
    Err(
        "secure_attention_unsupported: Ctrl+Alt+Delete requires a Windows target-side capability"
            .to_string(),
    )
}

#[cfg(target_os = "macos")]
fn wake_macos_display() -> Result<(), String> {
    // Agent requests can arrive on different threads. Reuse the process assertion
    // ID and let powerd expire it according to the display idle policy.
    static ASSERTION_ID: std::sync::Mutex<u32> = std::sync::Mutex::new(0);
    let mut assertion_id = ASSERTION_ID.lock().map_err(|_| {
        "display_wake_failed: user activity assertion state unavailable".to_string()
    })?;
    unsafe {
        let reason = CFStringCreateWithCString(
            std::ptr::null(),
            c"LL Remote display wake".as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        );
        if reason.is_null() {
            return Err("display_wake_failed: unable to create user activity reason".to_string());
        }

        let result =
            IOPMAssertionDeclareUserActivity(reason, K_IOPM_USER_ACTIVE_REMOTE, &mut *assertion_id);
        CFRelease(reason.cast());
        if result != K_IO_RETURN_SUCCESS {
            return Err(format!(
                "display_wake_failed: IOPMAssertionDeclareUserActivity returned {result}"
            ));
        }
    }

    Ok(())
}
