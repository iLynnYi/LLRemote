#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn inject_platform_key_event(_down: bool, _keysym: u32, _code: Option<&str>) {}
