#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureFailure {
    #[cfg(target_os = "windows")]
    DesktopUnavailable,
    #[cfg(any(target_os = "macos", test))]
    PermissionDenied,
    Unavailable,
    InvalidFrame,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Unsupported,
}

#[derive(Default)]
pub(crate) struct PlatformFrameCapture {
    #[cfg(target_os = "macos")]
    session: Option<capture_macos::MacosCapture>,
    #[cfg(target_os = "windows")]
    session: Option<capture_windows::WindowsCapture>,
    #[cfg(target_os = "windows")]
    desktop: Option<windows_desktop::DesktopOwner>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    revision: Option<u64>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    retry_after: Option<std::time::Instant>,
}

impl PlatformFrameCapture {
    /// Release desktop-bound GDI objects before assigning the capture thread to
    /// a different desktop. The returned epoch also invalidates same-size frames.
    pub(crate) fn prepare_desktop(&mut self) -> Result<Option<u64>, CaptureFailure> {
        #[cfg(target_os = "windows")]
        if windows_desktop::is_service_worker() {
            let next =
                windows_desktop::InputDesktop::open().map_err(|_| CaptureFailure::Unavailable)?;
            let desktop = self.desktop.get_or_insert_with(Default::default);
            if !desktop.matches(&next) {
                self.session = None;
                self.retry_after = None;
            }
            return desktop
                .bind(next)
                .map(Some)
                .map_err(|_| CaptureFailure::Unavailable);
        }
        #[cfg(target_os = "windows")]
        {
            windows_desktop::ordinary_desktop_epoch()
                .map(Some)
                .map_err(|_| {
                    self.session = None;
                    CaptureFailure::DesktopUnavailable
                })
        }
        #[cfg(not(target_os = "windows"))]
        Ok(None)
    }

    /// Returns only a new complete frame; pending startup /unchanged content is
    /// not a capture failure. The worker retains this owner across polling calls.
    pub(crate) fn capture(
        &mut self,
        display: &SharedDisplay,
    ) -> Result<Option<Vec<u8>>, CaptureFailure> {
        #[cfg(target_os = "macos")]
        {
            if self.revision != Some(display.revision) {
                self.session = None;
                self.retry_after = None;
                self.revision = Some(display.revision);
            }
            if !unsafe { CGPreflightScreenCaptureAccess() } {
                self.session = None;
                return Err(CaptureFailure::PermissionDenied);
            }
            if self
                .retry_after
                .is_some_and(|retry| std::time::Instant::now() < retry)
            {
                return Ok(None);
            }
            let result = match self.session.as_mut() {
                Some(session) => session.next_frame(),
                None => match capture_macos::MacosCapture::start(display) {
                    Ok(session) => {
                        self.session = Some(session);
                        Ok(None)
                    }
                    Err(error) => Err(error),
                },
            };
            if result.is_err() {
                self.session = None;
                self.retry_after =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
            }
            result
        }
        #[cfg(target_os = "windows")]
        {
            let result = self.capture_windows(display);
            if result.is_err() {
                self.session = None;
                self.retry_after =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
            }
            result
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = display;
            Err(CaptureFailure::Unsupported)
        }
    }

    #[cfg(target_os = "windows")]
    fn capture_windows(
        &mut self,
        display: &SharedDisplay,
    ) -> Result<Option<Vec<u8>>, CaptureFailure> {
        if self.revision != Some(display.revision) {
            self.session = None;
            self.retry_after = None;
            self.revision = Some(display.revision);
        }
        if self
            .retry_after
            .is_some_and(|retry| std::time::Instant::now() < retry)
        {
            return Ok(None);
        }
        let geometry = capture_windows::VirtualDesktopGeometry::current()?;
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.geometry() != geometry)
        {
            self.session = None;
        }
        if self.session.is_none() {
            self.session = Some(capture_windows::WindowsCapture::start(geometry)?);
        }
        self.session
            .as_mut()
            .ok_or(CaptureFailure::Unavailable)?
            .capture(display)
            .map(Some)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
const MAX_CAPTURE_BYTES: usize = 128 * 1024 * 1024;

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn capture_buffer_length(width: usize, height: usize) -> Result<usize, CaptureFailure> {
    if width == 0 || height == 0 {
        return Err(CaptureFailure::InvalidFrame);
    }
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= MAX_CAPTURE_BYTES)
        .ok_or(CaptureFailure::InvalidFrame)
}

/// Validate complete, bounded capture rows before scaling.
#[cfg(any(target_os = "windows", all(target_os = "macos", test)))]
fn convert_captured_framebuffer(
    source: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    target: DisplaySize,
    pixel_format: CapturedPixelFormat,
) -> Result<Vec<u8>, CaptureFailure> {
    capture_buffer_length(width, height)?;
    capture_buffer_length(target.width as usize, target.height as usize)?;
    let row_bytes = width.checked_mul(4).ok_or(CaptureFailure::InvalidFrame)?;
    let required = (height - 1)
        .checked_mul(stride)
        .and_then(|rows| rows.checked_add(row_bytes))
        .ok_or(CaptureFailure::InvalidFrame)?;
    if stride < row_bytes
        || required > source.len()
        || target.width > u16::MAX as u32
        || target.height > u16::MAX as u32
    {
        return Err(CaptureFailure::InvalidFrame);
    }
    Ok(scale_captured_framebuffer_to_rfb(
        source,
        width,
        height,
        stride,
        target.width as usize,
        target.height as usize,
        pixel_format,
    ))
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
#[path = "../../tests/support/platform_capture_tests.rs"]
mod platform_capture_tests;
