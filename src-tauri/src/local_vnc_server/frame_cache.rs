use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::local_vnc_server::types::{LocalVncServerCaptureStatus, LocalVncServerFrameDiagnostics};

use super::duration_ms;

#[derive(Default)]
pub(crate) struct LocalVncServerFrameCache {
    capture: Mutex<CaptureDiagnostics>,
    framebuffer_update_requests: AtomicU64,
    framebuffer_updates: AtomicU64,
    last_diff_duration_ms: AtomicU64,
    last_write_duration_ms: AtomicU64,
    last_dirty_rects: AtomicU64,
    last_framebuffer_encoding: Mutex<String>,
    last_raw_bytes_written: AtomicU64,
    last_encoded_bytes_written: AtomicU64,
    last_bytes_written: AtomicU64,
}

#[derive(Default, Clone, Copy)]
struct CaptureDiagnostics {
    desktop_unavailable: bool,
    status: LocalVncServerCaptureStatus,
    frames: u64,
    failures: u64,
    last_duration_ms: u64,
    last_captured_at: Option<Instant>,
}

impl LocalVncServerFrameCache {
    /// Metrics only. Captured pixels and connection delivery state belong to
    /// `ServerFramebufferCore`.
    pub(super) fn record_capture(&self, capture_duration: Duration) {
        if let Ok(mut capture) = self.capture.lock() {
            capture.desktop_unavailable = false;
            capture.status = LocalVncServerCaptureStatus::Ready;
            capture.frames = capture.frames.saturating_add(1);
            capture.last_duration_ms = duration_ms(capture_duration);
            capture.last_captured_at = Some(Instant::now());
        }
    }

    pub(super) fn mark_desktop_unavailable(&self) {
        if let Ok(mut capture) = self.capture.lock() {
            capture.desktop_unavailable = true;
        }
    }

    pub(crate) fn desktop_state(&self) -> crate::remote_desktop_core::DesktopAccessState {
        use crate::remote_desktop_core::DesktopAccessState;
        match self.capture.lock() {
            Ok(capture) if capture.desktop_unavailable => DesktopAccessState::Unavailable,
            Ok(capture) if capture.status == LocalVncServerCaptureStatus::Ready => {
                DesktopAccessState::Available
            }
            Ok(capture) if capture.status == LocalVncServerCaptureStatus::Waiting => {
                DesktopAccessState::Waiting
            }
            _ => DesktopAccessState::CaptureUnavailable,
        }
    }

    pub(super) fn record_capture_failure(
        &self,
        status: LocalVncServerCaptureStatus,
        duration: Duration,
    ) {
        if let Ok(mut capture) = self.capture.lock() {
            capture.status = status;
            capture.failures = capture.failures.saturating_add(1);
            capture.last_duration_ms = duration_ms(duration);
        }
    }

    pub(crate) fn record_framebuffer_update_request(&self) {
        self.framebuffer_update_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_send(
        &self,
        diff_duration: Duration,
        write_duration: Duration,
        dirty_rects: usize,
        framebuffer_encoding: &str,
        raw_bytes_written: u64,
        encoded_bytes_written: u64,
    ) {
        self.framebuffer_updates.fetch_add(1, Ordering::Relaxed);
        self.last_diff_duration_ms
            .store(duration_ms(diff_duration), Ordering::Relaxed);
        self.last_write_duration_ms
            .store(duration_ms(write_duration), Ordering::Relaxed);
        self.last_dirty_rects
            .store(dirty_rects as u64, Ordering::Relaxed);
        if let Ok(mut encoding) = self.last_framebuffer_encoding.lock() {
            *encoding = framebuffer_encoding.to_string();
        }
        self.last_raw_bytes_written
            .store(raw_bytes_written, Ordering::Relaxed);
        self.last_encoded_bytes_written
            .store(encoded_bytes_written, Ordering::Relaxed);
        self.last_bytes_written
            .store(encoded_bytes_written, Ordering::Relaxed);
    }

    pub(super) fn diagnostics(&self) -> LocalVncServerFrameDiagnostics {
        let capture = self
            .capture
            .lock()
            .map(|capture| *capture)
            .unwrap_or(CaptureDiagnostics {
                status: LocalVncServerCaptureStatus::Unavailable,
                ..CaptureDiagnostics::default()
            });
        let last_frame_age_ms = capture
            .last_captured_at
            .map(|timestamp| duration_ms(timestamp.elapsed()));

        LocalVncServerFrameDiagnostics {
            capture_status: capture.status,
            capture_frames: capture.frames,
            capture_failures: capture.failures,
            framebuffer_update_requests: self.framebuffer_update_requests.load(Ordering::Relaxed),
            framebuffer_updates: self.framebuffer_updates.load(Ordering::Relaxed),
            last_capture_duration_ms: capture.last_duration_ms,
            last_frame_age_ms,
            last_diff_duration_ms: self.last_diff_duration_ms.load(Ordering::Relaxed),
            last_write_duration_ms: self.last_write_duration_ms.load(Ordering::Relaxed),
            last_dirty_rects: self.last_dirty_rects.load(Ordering::Relaxed),
            last_framebuffer_encoding: self
                .last_framebuffer_encoding
                .lock()
                .map(|encoding| encoding.clone())
                .unwrap_or_else(|_| "unknown".to_string()),
            last_raw_bytes_written: self.last_raw_bytes_written.load(Ordering::Relaxed),
            last_encoded_bytes_written: self.last_encoded_bytes_written.load(Ordering::Relaxed),
            last_bytes_written: self.last_bytes_written.load(Ordering::Relaxed),
        }
    }
}
