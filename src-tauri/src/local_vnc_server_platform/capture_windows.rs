use super::{
    capture_buffer_length, convert_captured_framebuffer, CaptureFailure, CapturedPixelFormat,
    SharedDisplay,
};
use std::mem::size_of;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, HDC, SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct VirtualDesktopGeometry {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl VirtualDesktopGeometry {
    pub(super) fn current() -> Result<Self, CaptureFailure> {
        let geometry = unsafe {
            Self {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        };
        if geometry.width <= 0 || geometry.height <= 0 {
            return Err(CaptureFailure::Unavailable);
        }
        capture_buffer_length(geometry.width as usize, geometry.height as usize)?;
        Ok(geometry)
    }
}

/// Created, used and dropped on the capture worker. Raw GDI handles keep the
/// owner non-Send; ReleaseDC must run on the thread that called GetDC.
pub(super) struct WindowsCapture {
    screen_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    geometry: VirtualDesktopGeometry,
    source: Vec<u8>,
}

impl WindowsCapture {
    pub(super) fn start(geometry: VirtualDesktopGeometry) -> Result<Self, CaptureFailure> {
        let length = capture_buffer_length(geometry.width as usize, geometry.height as usize)?;
        let mut source = Vec::new();
        source
            .try_reserve_exact(length)
            .map_err(|_| CaptureFailure::Unavailable)?;
        source.resize(length, 0);
        unsafe {
            let screen_dc = GetDC(null_mut());
            if screen_dc.is_null() {
                return Err(CaptureFailure::Unavailable);
            }
            // Construct the owner before further acquisitions so partial startup
            // failure releases exactly the handles acquired so far.
            let mut owner = Self {
                screen_dc,
                memory_dc: null_mut(),
                bitmap: null_mut(),
                geometry,
                source,
            };
            owner.memory_dc = CreateCompatibleDC(screen_dc);
            if owner.memory_dc.is_null() {
                return Err(CaptureFailure::Unavailable);
            }
            owner.bitmap = CreateCompatibleBitmap(screen_dc, geometry.width, geometry.height);
            if owner.bitmap.is_null() {
                return Err(CaptureFailure::Unavailable);
            }
            Ok(owner)
        }
    }

    pub(super) fn geometry(&self) -> VirtualDesktopGeometry {
        self.geometry
    }

    pub(super) fn capture(&mut self, display: &SharedDisplay) -> Result<Vec<u8>, CaptureFailure> {
        let geometry = self.geometry;
        unsafe {
            let previous = SelectObject(self.memory_dc, self.bitmap);
            if previous.is_null() {
                return Err(CaptureFailure::Unavailable);
            }
            let blit_ok = BitBlt(
                self.memory_dc,
                0,
                0,
                geometry.width,
                geometry.height,
                self.screen_dc,
                geometry.x,
                geometry.y,
                SRCCOPY | CAPTUREBLT,
            ) != 0;
            // GetDIBits requires this bitmap to be unselected from all DCs.
            let restored = SelectObject(self.memory_dc, previous);
            if !blit_ok || restored.is_null() {
                return Err(CaptureFailure::Unavailable);
            }
            let mut bitmap_info = BITMAPINFO::default();
            bitmap_info.bmiHeader.biSize =
                size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
            bitmap_info.bmiHeader.biWidth = geometry.width;
            bitmap_info.bmiHeader.biHeight = -geometry.height;
            bitmap_info.bmiHeader.biPlanes = 1;
            bitmap_info.bmiHeader.biBitCount = 32;
            bitmap_info.bmiHeader.biCompression = BI_RGB;
            let rows = GetDIBits(
                self.memory_dc,
                self.bitmap,
                0,
                geometry.height as u32,
                self.source.as_mut_ptr().cast(),
                &mut bitmap_info,
                DIB_RGB_COLORS,
            );
            if rows == 0 {
                return Err(CaptureFailure::Unavailable);
            }
            if rows != geometry.height {
                return Err(CaptureFailure::InvalidFrame);
            }
        }
        // A display-mode change during the native read invalidates this sample;
        // the caller drops the owner and reacquires compatible GDI resources.
        if VirtualDesktopGeometry::current()? != geometry {
            return Err(CaptureFailure::InvalidFrame);
        }
        // Only a complete native read is converted. Stale source bytes left by a
        // partial read never reach the published framebuffer.
        convert_captured_framebuffer(
            &self.source,
            geometry.width as usize,
            geometry.height as usize,
            geometry.width as usize * 4,
            display.framebuffer,
            CapturedPixelFormat::Bgra8888,
        )
    }
}

impl Drop for WindowsCapture {
    fn drop(&mut self) {
        unsafe {
            // Destroy the DC first: even a failed restore cannot leave the bitmap
            // selected when DeleteObject releases it.
            if !self.memory_dc.is_null() {
                let _ = DeleteDC(self.memory_dc);
            }
            if !self.bitmap.is_null() {
                let _ = DeleteObject(self.bitmap);
            }
            if !self.screen_dc.is_null() {
                let _ = ReleaseDC(null_mut(), self.screen_dc);
            }
        }
    }
}
