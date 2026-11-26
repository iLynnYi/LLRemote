pub fn list_local_vnc_server_displays() -> Vec<LocalVncServerDisplay> {
    list_platform_displays()
}

#[cfg(target_os = "macos")]
fn list_platform_displays() -> Vec<LocalVncServerDisplay> {
    unsafe {
        let mut count = 0_u32;
        if CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) != 0 || count == 0 {
            return vec![fallback_display_overview()];
        }

        let mut ids = vec![0_u32; count as usize];
        if CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut count) != 0 {
            return vec![fallback_display_overview()];
        }

        let main_id = CGMainDisplayID();
        let mut displays = Vec::new();
        for (index, display_id) in ids.into_iter().take(count as usize).enumerate() {
            let bounds = CGDisplayBounds(display_id);
            let width = bounds.size.width.max(1.0).round() as u32;
            let height = bounds.size.height.max(1.0).round() as u32;
            let pixel_width = CGDisplayPixelsWide(display_id).max(1) as u32;
            let pixel_height = CGDisplayPixelsHigh(display_id).max(1) as u32;
            let scale_factor = if width > 0 {
                pixel_width as f64 / width as f64
            } else {
                1.0
            };
            let is_primary = display_id == main_id;

            displays.push(LocalVncServerDisplay {
                id: format!("display:{display_id}"),
                label: if is_primary {
                    "Main display".to_string()
                } else {
                    format!("Display {}", index + 1)
                },
                kind: if is_primary {
                    LocalVncServerDisplayKind::Main
                } else {
                    LocalVncServerDisplayKind::Display
                },
                is_primary,
                x: bounds.origin.x.round() as i32,
                y: bounds.origin.y.round() as i32,
                width,
                height,
                pixel_width,
                pixel_height,
                scale_factor,
            });
        }

        displays.sort_by_key(|display| (!display.is_primary, display.x, display.y));
        if displays.is_empty() {
            vec![fallback_display_overview()]
        } else {
            displays
        }
    }
}

#[cfg(target_os = "windows")]
fn list_platform_displays() -> Vec<LocalVncServerDisplay> {
    list_windows_displays()
}

#[cfg(target_os = "windows")]
fn list_windows_displays() -> Vec<LocalVncServerDisplay> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1) as u32;
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(1) as u32;

    vec![LocalVncServerDisplay {
        id: "windows:virtual-screen".to_string(),
        label: "Windows desktop".to_string(),
        kind: LocalVncServerDisplayKind::Main,
        is_primary: true,
        x,
        y,
        width,
        height,
        pixel_width: width,
        pixel_height: height,
        scale_factor: 1.0,
    }]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn list_platform_displays() -> Vec<LocalVncServerDisplay> {
    vec![fallback_display_overview()]
}

#[cfg(not(target_os = "windows"))]
fn fallback_display_overview() -> LocalVncServerDisplay {
    LocalVncServerDisplay {
        id: "main".to_string(),
        label: "Main display".to_string(),
        kind: LocalVncServerDisplayKind::Main,
        is_primary: true,
        x: 0,
        y: 0,
        width: DEFAULT_FRAMEBUFFER_WIDTH as u32,
        height: DEFAULT_FRAMEBUFFER_HEIGHT as u32,
        pixel_width: DEFAULT_FRAMEBUFFER_WIDTH as u32,
        pixel_height: DEFAULT_FRAMEBUFFER_HEIGHT as u32,
        scale_factor: 1.0,
    }
}

pub fn map_vnc_point_to_screen(
    x: u16,
    y: u16,
    framebuffer: DisplaySize,
    screen: DisplaySize,
) -> ScreenPoint {
    let fb_width = framebuffer.width.max(1);
    let fb_height = framebuffer.height.max(1);

    ScreenPoint {
        x: ((x as u32).min(fb_width - 1) * screen.width.max(1) / fb_width) as i32,
        y: ((y as u32).min(fb_height - 1) * screen.height.max(1) / fb_height) as i32,
    }
}

pub fn map_vnc_point_to_display(
    x: u16,
    y: u16,
    framebuffer: DisplaySize,
    display: DisplayFrame,
) -> ScreenPoint {
    let local = map_vnc_point_to_screen(
        x,
        y,
        framebuffer,
        DisplaySize {
            width: display.width,
            height: display.height,
        },
    );

    ScreenPoint {
        x: display.x.saturating_add(local.x),
        y: display.y.saturating_add(local.y),
    }
}
