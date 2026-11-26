#[cfg(target_os = "windows")]
fn inject_platform_pointer_event(
    button_mask: u8,
    x: u16,
    y: u16,
    last_button_mask: &mut u8,
    shared_display: &SharedDisplay,
) {
    inject_windows_pointer_event(button_mask, x, y, last_button_mask, shared_display);
}

#[cfg(target_os = "windows")]
fn inject_windows_pointer_event(
    button_mask: u8,
    x: u16,
    y: u16,
    last_button_mask: &mut u8,
    shared_display: &SharedDisplay,
) -> bool {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
        MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    let point = map_vnc_point_to_display(
        x,
        y,
        shared_display.framebuffer,
        shared_display.frame.clone(),
    );
    let origin_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let origin_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1);
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(1);
    let absolute_x = windows_absolute_coordinate(point.x.saturating_sub(origin_x), width);
    let absolute_y = windows_absolute_coordinate(point.y.saturating_sub(origin_y), height);

    let mut inputs = vec![windows_mouse_input(
        absolute_x,
        absolute_y,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )];

    if let Some(scroll_delta) = scroll_delta_for_pointer_transition(*last_button_mask, button_mask)
    {
        inputs.push(windows_mouse_input(
            0,
            0,
            (scroll_delta * 120) as u32,
            MOUSEEVENTF_WHEEL,
        ));
    }

    for (mask, down_event, up_event) in [
        (1_u8, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        (2_u8, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        (4_u8, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
    ] {
        let was_down = (*last_button_mask & mask) != 0;
        let is_down = (button_mask & mask) != 0;
        if was_down != is_down {
            inputs.push(windows_mouse_input(
                0,
                0,
                0,
                if is_down { down_event } else { up_event },
            ));
        }
    }

    unsafe {
        let inserted = SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        );
        windows_desktop::record_input_result(inputs.len() as u32, inserted);
        inserted == inputs.len() as u32
    }
}

#[cfg(target_os = "windows")]
fn windows_absolute_coordinate(value: i32, size: i32) -> i32 {
    let max = size.saturating_sub(1).max(1);
    (value.clamp(0, max) * 65_535 / max).clamp(0, 65_535)
}

#[cfg(target_os = "windows")]
fn windows_mouse_input(
    dx: i32,
    dy: i32,
    mouse_data: u32,
    flags: u32,
) -> windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
    };

    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
