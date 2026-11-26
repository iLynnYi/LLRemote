#[cfg(target_os = "macos")]
fn inject_platform_key_event(down: bool, keysym: u32, code: Option<&str>) {
    inject_macos_key_event_with_flags(down, keysym, code, 0);
}

#[cfg(target_os = "macos")]
fn inject_macos_key_event_with_flags(down: bool, keysym: u32, code: Option<&str>, flags: u64) {
    if unsafe { !AXIsProcessTrusted() } {
        return;
    }

    let Some(keycode) = mac_keycode_for_keyboard_input(keysym, code) else {
        return;
    };

    unsafe {
        let event = CGEventCreateKeyboardEvent(std::ptr::null(), keycode, down);
        if !event.is_null() {
            CGEventSetIntegerValueField(
                event,
                42,
                crate::keyboard_input_source::OWN_INPUT_TAG as i64,
            );
            if flags != 0 {
                CGEventSetFlags(event, flags);
            }
            CGEventPost(K_CG_HID_EVENT_TAP, event);
            CFRelease(event.cast());
        }
    }
}
