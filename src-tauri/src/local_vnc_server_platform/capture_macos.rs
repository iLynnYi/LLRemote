use super::{capture_buffer_length, CaptureFailure, SharedDisplay};
use crate::local_vnc_server::refresh_interval;
use std::ffi::c_void;
use std::ptr::NonNull;

extern "C" {
    fn vnc_screen_capture_start(
        display: u32,
        width: u32,
        height: u32,
        interval: u32,
    ) -> *mut c_void;
    fn vnc_screen_capture_copy(
        handle: *mut c_void,
        bytes: *mut u8,
        capacity: usize,
        sequence: *mut u64,
    ) -> i32;
    fn vnc_screen_capture_stop(handle: *mut c_void);
}

/// The capture worker owns this native stream. Its non-Send handle keeps creation,
/// frame copying and stop on that worker; native callbacks never borrow Rust data.
pub(super) struct MacosCapture {
    handle: NonNull<c_void>,
    sequence: u64,
    length: usize,
}

impl MacosCapture {
    pub(super) fn start(display: &SharedDisplay) -> Result<Self, CaptureFailure> {
        let size = display.framebuffer;
        let length = capture_buffer_length(size.width as usize, size.height as usize)?;
        if size.width > u16::MAX as u32 || size.height > u16::MAX as u32 {
            return Err(CaptureFailure::InvalidFrame);
        }
        let display_id = display
            .id
            .strip_prefix("display:")
            .and_then(|id| id.parse().ok())
            .unwrap_or_else(|| unsafe { super::CGMainDisplayID() });
        let interval = refresh_interval(display.refresh_mode).as_millis().max(1) as u32;
        let handle = NonNull::new(unsafe {
            vnc_screen_capture_start(display_id, size.width, size.height, interval)
        })
        .ok_or(CaptureFailure::Unavailable)?;
        Ok(Self {
            handle,
            sequence: 0,
            length,
        })
    }

    pub(super) fn next_frame(&mut self) -> Result<Option<Vec<u8>>, CaptureFailure> {
        let status = unsafe {
            vnc_screen_capture_copy(
                self.handle.as_ptr(),
                std::ptr::null_mut(),
                0,
                &mut self.sequence,
            )
        };
        if !frame_ready(status)? {
            return Ok(None);
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.length)
            .map_err(|_| CaptureFailure::Unavailable)?;
        pixels.resize(self.length, 0);
        let status = unsafe {
            vnc_screen_capture_copy(
                self.handle.as_ptr(),
                pixels.as_mut_ptr(),
                pixels.len(),
                &mut self.sequence,
            )
        };
        if !frame_ready(status)? {
            return Ok(None);
        }
        Ok(Some(pixels))
    }
}

fn frame_ready(status: i32) -> Result<bool, CaptureFailure> {
    match status {
        0 => Ok(true),
        1 => Ok(false),
        3 => Err(CaptureFailure::PermissionDenied),
        4 => Err(CaptureFailure::InvalidFrame),
        _ => Err(CaptureFailure::Unavailable),
    }
}

impl Drop for MacosCapture {
    fn drop(&mut self) {
        // Native stop waits for startup /stop completions and drains its output
        // queue before releasing the owner. No callback can outlive Rust buffers.
        unsafe {
            vnc_screen_capture_stop(self.handle.as_ptr());
        }
    }
}
