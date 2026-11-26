use super::{native, repair, WindowsServiceRepairItem};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY_LOCAL_MACHINE, KEY_SET_VALUE,
    KEY_WOW64_64KEY, REG_DWORD, RRF_RT_REG_DWORD, RRF_SUBKEY_WOW6464KEY,
};

const POLICY_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System";
const POLICY_VALUE: &str = "EnableLUA";
static ENABLED_THIS_PROCESS: AtomicBool = AtomicBool::new(false);

fn policy_enabled() -> Option<bool> {
    let mut value = 0_u32;
    let mut size = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            native::wide(POLICY_KEY).as_ptr(),
            native::wide(POLICY_VALUE).as_ptr(),
            RRF_RT_REG_DWORD | RRF_SUBKEY_WOW6464KEY,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    match (result, value) {
        (0, 0) => Some(false),
        (0, 1) => Some(true),
        _ => None,
    }
}

/// Policy is a preflight. The helper must still authenticate its actual token.
pub(super) fn check_elevation() -> Result<(), String> {
    if policy_enabled() == Some(false) {
        return Err(repair::UAC_DISABLED.into());
    }
    Ok(())
}

pub(super) fn inspect_or_repair(elevated: bool) -> WindowsServiceRepairItem {
    repair::inspect_elevation(
        policy_enabled(),
        elevated,
        ENABLED_THIS_PROCESS.load(Ordering::Relaxed),
        enable,
    )
}

// Only explicit repair by an already-elevated App changes this machine policy.
fn enable() -> Result<(), &'static str> {
    const FAILED: &str = "windows_service_uac_repair_failed";
    let mut key = null_mut();
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            native::wide(POLICY_KEY).as_ptr(),
            0,
            KEY_SET_VALUE | KEY_WOW64_64KEY,
            &mut key,
        )
    } != 0
    {
        return Err(FAILED);
    }
    let enabled = 1_u32;
    let result = unsafe {
        let result = RegSetValueExW(
            key,
            native::wide(POLICY_VALUE).as_ptr(),
            0,
            REG_DWORD,
            (&enabled as *const u32).cast(),
            size_of::<u32>() as u32,
        );
        RegCloseKey(key);
        result
    };
    if result != 0 || policy_enabled() != Some(true) {
        return Err(FAILED);
    }
    ENABLED_THIS_PROCESS.store(true, Ordering::Relaxed);
    Ok(())
}
