#[cfg(any(target_os = "windows", all(target_os = "macos", test)))]
use crate::local_vnc_server::framebuffer::scale_captured_framebuffer_to_rfb;
#[cfg(any(target_os = "windows", all(target_os = "macos", test)))]
use crate::local_vnc_server::types::CapturedPixelFormat;
use crate::local_vnc_server::types::{
    DisplayFrame, DisplaySize, LocalVncServerDisplay, LocalVncServerDisplayKind, ScreenPoint,
};
use crate::local_vnc_server::SharedDisplay;
#[cfg(not(target_os = "windows"))]
use crate::local_vnc_server::{DEFAULT_FRAMEBUFFER_HEIGHT, DEFAULT_FRAMEBUFFER_WIDTH};
#[cfg(target_os = "macos")]
use std::ffi::{c_char, c_void};
#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

#[cfg(target_os = "macos")]
mod capture_macos;
#[cfg(target_os = "windows")]
mod capture_windows;
#[cfg(any(target_os = "windows", test))]
mod desktop_epoch;
#[cfg(target_os = "windows")]
pub(crate) mod windows_desktop;
#[cfg(target_os = "windows")]
pub(crate) mod windows_input;

include!("display.rs");
include!("input.rs");
include!("desktop.rs");
include!("clipboard.rs");
include!("capture.rs");
include!("pointer_macos.rs");
include!("pointer_windows.rs");
include!("pointer_stub.rs");
include!("keyboard_macos.rs");
include!("keyboard_windows.rs");
include!("keyboard_stub.rs");
include!("ffi_macos.rs");

pub(crate) fn capture_desktop_is_current(epoch: u64) -> bool {
    #[cfg(target_os = "windows")]
    {
        windows_desktop::observed_frame_is_current(epoch)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = epoch;
        true
    }
}
