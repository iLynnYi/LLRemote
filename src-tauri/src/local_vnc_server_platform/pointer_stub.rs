#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn inject_platform_pointer_event(
    _button_mask: u8,
    _x: u16,
    _y: u16,
    _last_button_mask: &mut u8,
    _shared_display: &SharedDisplay,
) {
}
