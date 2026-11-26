use super::{ipc::Peer, native};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::FreeLibrary;
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, RRF_RT_REG_DWORD,
};
use windows_sys::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;

const POLICY_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System";
const POLICY_VALUE: &str = "SoftwareSASGeneration";

pub(super) fn policy() -> String {
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            native::wide(POLICY_KEY).as_ptr(),
            native::wide(POLICY_VALUE).as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    match result {
        0 if matches!(value, 1 | 3) => "services_allowed",
        0 if matches!(value, 0 | 2) => "services_denied",
        2 => "not_configured",
        _ => "unknown",
    }
    .into()
}

struct PolicyKey(HKEY);
impl Drop for PolicyKey {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

/// Only the explicitly requested, elevated helper may enable service SAS. A
/// configured policy is preserved, including policies that deny service access.
pub(super) fn enable_service_policy() -> Result<(), String> {
    configure_service_policy(false)
}

pub(super) fn repair_service_policy() -> Result<(), String> {
    configure_service_policy(true)?;
    if policy() != "services_allowed" {
        return Err("windows_service_sas_policy_configure_failed".into());
    }
    Ok(())
}

fn configure_service_policy(explicit_repair: bool) -> Result<(), String> {
    let mut raw = null_mut();
    if unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            native::wide(POLICY_KEY).as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            null(),
            &mut raw,
            null_mut(),
        )
    } != 0
    {
        return Err("windows_service_sas_policy_configure_failed".into());
    }
    let key = PolicyKey(raw);
    configure_key(key.0, explicit_repair)
}

#[cfg(test)]
fn allow_services_for_key(key: HKEY) -> Result<(), String> {
    configure_key(key, false)
}

fn configure_key(key: HKEY, explicit_repair: bool) -> Result<(), String> {
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            key,
            null(),
            native::wide(POLICY_VALUE).as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    let current = match result {
        0 => Some(value),
        2 => None,
        _ => return Err("windows_service_sas_policy_configure_failed".into()),
    };
    let allowed_value = super::repair::sas_value(current, explicit_repair)?;
    if current == Some(allowed_value) {
        return Ok(());
    }
    if unsafe {
        RegSetValueExW(
            key,
            native::wide(POLICY_VALUE).as_ptr(),
            0,
            REG_DWORD,
            (&allowed_value as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    } != 0
    {
        return Err("windows_service_sas_policy_configure_failed".into());
    }
    Ok(())
}

pub(super) fn send_for_peer(peer: &Peer) -> Result<(), String> {
    // Only console workers are supported. SendSAS(FALSE) runs in the SCM
    // service's own LocalSystem context; a migrated SYSTEM worker token is not
    // an interactive user's logon token. Reject a stale worker after a switch.
    let console_session = unsafe { WTSGetActiveConsoleSessionId() };
    if console_session == u32::MAX || console_session != peer.session_id {
        return Err("windows_service_access_denied".into());
    }
    if policy() != "services_allowed" {
        return Err("windows_service_sas_policy_denied".into());
    }
    let module = unsafe {
        LoadLibraryExW(
            native::wide("sas.dll").as_ptr(),
            null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module.is_null() {
        return Err("windows_service_sas_unavailable".into());
    }
    let result = (|| {
        let proc = unsafe { GetProcAddress(module, c"SendSAS".as_ptr().cast()) }
            .ok_or_else(|| "windows_service_sas_unavailable".to_string())?;
        let send_sas: unsafe extern "system" fn(i32) = unsafe { std::mem::transmute(proc) };
        // SendSAS is void: this means requested, never an observed SAS result.
        unsafe {
            send_sas(0);
        }
        Ok(())
    })();
    unsafe {
        FreeLibrary(module);
    }
    result
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_sas_tests.rs"]
mod tests;
