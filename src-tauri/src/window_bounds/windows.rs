use super::geometry::Rect;
use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
    UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER},
};

pub(super) fn visible_bounds(window: &tauri::Window) -> Option<Rect> {
    let hwnd = window.hwnd().ok()?.0;
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut rect as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if result < 0 || rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }
    Some(Rect {
        x: rect.left,
        y: rect.top,
        width: (rect.right - rect.left) as u32,
        height: (rect.bottom - rect.top) as u32,
    })
}

pub(super) fn set_maximized_bounds(window: &tauri::Window, bounds: Rect) -> tauri::Result<()> {
    let result = unsafe {
        SetWindowPos(
            window.hwnd()?.0,
            std::ptr::null_mut(),
            bounds.x,
            bounds.y,
            bounds.width as i32,
            bounds.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
