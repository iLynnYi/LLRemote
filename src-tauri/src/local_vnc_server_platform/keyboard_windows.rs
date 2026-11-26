#[cfg(target_os = "windows")]
fn inject_platform_key_event(down: bool, keysym: u32, _code: Option<&str>) {
    inject_windows_key_event(down, keysym);
}

#[cfg(target_os = "windows")]
fn inject_windows_key_event(down: bool, keysym: u32) -> bool {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};

    let Some(input) = windows_key_input(down, keysym) else {
        return false;
    };

    unsafe {
        let inserted = SendInput(1, &input, size_of::<INPUT>() as i32);
        windows_desktop::record_input_result(1, inserted);
        inserted == 1
    }
}

#[cfg(target_os = "windows")]
fn windows_key_input(
    down: bool,
    keysym: u32,
) -> Option<windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };

    let keyup = if down { 0 } else { KEYEVENTF_KEYUP };
    if let Some(virtual_key) = windows_vk_for_keysym(keysym) {
        return Some(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: virtual_key,
                    wScan: 0,
                    dwFlags: keyup,
                    time: 0,
                    dwExtraInfo: crate::keyboard_input_source::OWN_INPUT_TAG,
                },
            },
        });
    }

    if let Some(unit) = windows_unicode_unit_for_keysym(keysym) {
        return Some(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | keyup,
                    time: 0,
                    dwExtraInfo: crate::keyboard_input_source::OWN_INPUT_TAG,
                },
            },
        });
    }

    None
}

#[cfg(target_os = "windows")]
fn windows_unicode_unit_for_keysym(keysym: u32) -> Option<u16> {
    match keysym {
        0x20..=0x7e => Some(keysym as u16),
        0xa0..=0xd7ff | 0xe000..=0xffff => Some(keysym as u16),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn windows_vk_for_keysym(keysym: u32) -> Option<u16> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME,
        VK_INSERT, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB,
        VK_UP,
    };

    match keysym {
        0x30..=0x39 => Some(keysym as u16),
        0x41..=0x5a => Some(keysym as u16),
        0x61..=0x7a => Some((keysym - 0x20) as u16),
        0xff08 => Some(VK_BACK),
        0xff09 => Some(VK_TAB),
        0xff0d => Some(VK_RETURN),
        0xff1b => Some(VK_ESCAPE),
        0xffff => Some(VK_DELETE),
        0xff50 => Some(VK_HOME),
        0xff51 => Some(VK_LEFT),
        0xff52 => Some(VK_UP),
        0xff53 => Some(VK_RIGHT),
        0xff54 => Some(VK_DOWN),
        0xff55 => Some(VK_PRIOR),
        0xff56 => Some(VK_NEXT),
        0xff57 => Some(VK_END),
        0xff63 => Some(VK_INSERT),
        0xffbe..=0xffc9 => Some(VK_F1 + (keysym - 0xffbe) as u16),
        0xffe1 | 0xffe2 => Some(VK_SHIFT),
        0xffe3 | 0xffe4 => Some(VK_CONTROL),
        0xffe5 => Some(VK_CAPITAL),
        0xffe7 | 0xffe8 | 0xffeb => Some(0x5b),
        0xffe9 | 0xffea => Some(VK_MENU),
        _ => None,
    }
}
