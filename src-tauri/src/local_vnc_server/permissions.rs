use crate::local_vnc_server::{
    LocalVncServerPermission, LocalVncServerPermissionKind, LocalVncServerPermissionRequestResult,
    LocalVncServerPermissionStatus,
};

#[cfg(target_os = "macos")]
use std::process::Command;

pub fn get_local_vnc_server_permissions() -> Vec<LocalVncServerPermission> {
    collect_local_vnc_server_permissions()
}

/// Describe required grants without querying the foreground process's TCC identity.
pub(crate) fn pending_local_vnc_server_permissions() -> Vec<LocalVncServerPermission> {
    if cfg!(target_os = "macos") {
        vec![
            LocalVncServerPermission {
                kind: LocalVncServerPermissionKind::ScreenRecording,
                status: LocalVncServerPermissionStatus::Unknown,
                required_for: "screen_capture".into(),
            },
            LocalVncServerPermission {
                kind: LocalVncServerPermissionKind::Accessibility,
                status: LocalVncServerPermissionStatus::Unknown,
                required_for: "input_injection".into(),
            },
        ]
    } else {
        vec![]
    }
}

pub fn request_local_vnc_server_permission(
    kind: LocalVncServerPermissionKind,
) -> Result<LocalVncServerPermissionRequestResult, String> {
    let platform = std::env::consts::OS.to_string();

    if platform != "macos" {
        return Ok(LocalVncServerPermissionRequestResult {
            opened: false,
            kind,
            platform,
            target: "manual_platform_setup".to_string(),
            message: "Automatic permission request is not available on this platform.".to_string(),
        });
    }

    request_macos_permission(kind)
}

/// Opening the settings pane is a separate user action; it does not request
/// trust for the foreground process or imply that a permission was granted.
pub fn open_local_vnc_server_permission_settings(
    kind: LocalVncServerPermissionKind,
) -> Result<LocalVncServerPermissionRequestResult, String> {
    #[cfg(target_os = "macos")]
    return perform_macos_permission_action(
        kind,
        true,
        prompt_macos_permission,
        open_macos_settings_target,
    );
    #[cfg(not(target_os = "macos"))]
    request_macos_permission(kind)
}

pub(crate) fn collect_local_vnc_server_permissions() -> Vec<LocalVncServerPermission> {
    match std::env::consts::OS {
        "macos" => macos_permissions(),
        _ => vec![],
    }
}

#[cfg(target_os = "macos")]
fn macos_permissions() -> Vec<LocalVncServerPermission> {
    vec![
        LocalVncServerPermission {
            kind: LocalVncServerPermissionKind::ScreenRecording,
            status: if unsafe { CGPreflightScreenCaptureAccess() } {
                LocalVncServerPermissionStatus::Granted
            } else {
                LocalVncServerPermissionStatus::Missing
            },
            required_for: "screen_capture".to_string(),
        },
        LocalVncServerPermission {
            kind: LocalVncServerPermissionKind::Accessibility,
            status: if unsafe { AXIsProcessTrusted() } {
                LocalVncServerPermissionStatus::Granted
            } else {
                LocalVncServerPermissionStatus::Missing
            },
            required_for: "input_injection".to_string(),
        },
    ]
}

#[cfg(not(target_os = "macos"))]
fn macos_permissions() -> Vec<LocalVncServerPermission> {
    vec![]
}

#[cfg(target_os = "macos")]
fn request_macos_permission(
    kind: LocalVncServerPermissionKind,
) -> Result<LocalVncServerPermissionRequestResult, String> {
    perform_macos_permission_action(
        kind,
        false,
        prompt_macos_permission,
        open_macos_settings_target,
    )
}

#[cfg(target_os = "macos")]
fn prompt_macos_permission(kind: LocalVncServerPermissionKind) -> bool {
    match kind {
        LocalVncServerPermissionKind::ScreenRecording => unsafe { CGRequestScreenCaptureAccess() },
        LocalVncServerPermissionKind::Accessibility => {
            unsafe extern "C" {
                fn vnc_sharing_host_request_accessibility() -> bool;
            }
            unsafe { vnc_sharing_host_request_accessibility() }
        }
    }
}

// Keep the request policy separate from OS effects so tests cannot open real
// settings or change the test process's macOS permissions.
#[cfg(target_os = "macos")]
fn perform_macos_permission_action(
    kind: LocalVncServerPermissionKind,
    open_settings: bool,
    prompt: impl FnOnce(LocalVncServerPermissionKind) -> bool,
    open: impl FnOnce(
        LocalVncServerPermissionKind,
        &str,
    ) -> Result<LocalVncServerPermissionRequestResult, String>,
) -> Result<LocalVncServerPermissionRequestResult, String> {
    let (target, prompt_target) = match kind {
        LocalVncServerPermissionKind::ScreenRecording => (
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            "screen_recording_prompt",
        ),
        LocalVncServerPermissionKind::Accessibility => (
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "accessibility_prompt",
        ),
    };
    if open_settings {
        return open(kind, target);
    }
    let trusted = prompt(kind.clone());
    // A false trust result also means approval is still pending. Never open
    // settings here: macOS owns its prompt's button, and the App has an explicit action.
    Ok(LocalVncServerPermissionRequestResult {
        opened: true,
        kind,
        platform: "macos".into(),
        target: prompt_target.into(),
        message: if trusted {
            "Permission is already granted."
        } else {
            "Requested permission. Approve it in the system prompt."
        }
        .into(),
    })
}

#[cfg(not(target_os = "macos"))]
fn request_macos_permission(
    kind: LocalVncServerPermissionKind,
) -> Result<LocalVncServerPermissionRequestResult, String> {
    Ok(LocalVncServerPermissionRequestResult {
        opened: false,
        kind,
        platform: std::env::consts::OS.to_string(),
        target: "manual_platform_setup".to_string(),
        message: "Automatic permission request is not available on this platform.".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn open_macos_settings_target(
    kind: LocalVncServerPermissionKind,
    target: &str,
) -> Result<LocalVncServerPermissionRequestResult, String> {
    match Command::new("open").arg(target).status() {
        Ok(status) if status.success() => Ok(LocalVncServerPermissionRequestResult {
            opened: true,
            kind,
            platform: "macos".to_string(),
            target: target.to_string(),
            message: "Opened the macOS permission entry.".to_string(),
        }),
        Ok(status) => Ok(LocalVncServerPermissionRequestResult {
            opened: false,
            kind,
            platform: "macos".to_string(),
            target: target.to_string(),
            message: format!("Permission entry exited with status {status}."),
        }),
        Err(error) => Err(format!("Unable to open macOS permission entry: {error}")),
    }
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../../tests/support/local_share_permissions_tests.rs"]
mod permission_tests;
