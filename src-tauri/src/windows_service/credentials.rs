use super::native;
use std::ptr::null_mut;
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};
use zeroize::{Zeroize, Zeroizing};

// Stable credential target for the LocalSystem sharing service.
pub(super) const CREDENTIAL_TARGET: &str = "LLRemote/Windows Service/RFB";

/// Only called in the LocalSystem service/worker context; ordinary user Host
/// CredentialManager entries are deliberately unrelated to this secret.
pub(super) fn read() -> Result<Zeroizing<String>, String> {
    let mut credential = null_mut();
    if unsafe {
        CredReadW(
            native::wide(CREDENTIAL_TARGET).as_ptr(),
            CRED_TYPE_GENERIC,
            0,
            &mut credential,
        )
    } == 0
    {
        return Err("windows_service_credential_missing".into());
    }
    unsafe {
        let entry = &mut *credential;
        let result = if entry.CredentialBlobSize > 0
            && entry.CredentialBlobSize <= 8
            && !entry.CredentialBlob.is_null()
        {
            let blob = std::slice::from_raw_parts_mut(
                entry.CredentialBlob,
                entry.CredentialBlobSize as usize,
            );
            let decoded = String::from_utf8(blob.to_vec())
                .map(Zeroizing::new)
                .map_err(|_| "windows_service_credential_missing".into());
            blob.zeroize();
            decoded
        } else {
            Err("windows_service_credential_missing".into())
        };
        CredFree(credential.cast());
        result
    }
}

pub(super) fn write(password: &str) -> Result<(), String> {
    if password.is_empty() || password.len() > 8 {
        return Err("windows_service_invalid_config".into());
    }
    let mut bytes = Zeroizing::new(password.as_bytes().to_vec());
    let mut target = native::wide(CREDENTIAL_TARGET);
    let mut username = native::wide("LL Remote Server");
    let entry = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        CredentialBlobSize: bytes.len() as u32,
        CredentialBlob: bytes.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: username.as_mut_ptr(),
        ..Default::default()
    };
    if unsafe { CredWriteW(&entry, 0) } == 0 {
        Err("windows_service_configure_failed".into())
    } else {
        Ok(())
    }
}

pub(super) fn remove() -> Result<(), String> {
    if unsafe {
        CredDeleteW(
            native::wide(CREDENTIAL_TARGET).as_ptr(),
            CRED_TYPE_GENERIC,
            0,
        )
    } == 0
        && native::last_error() != 1168
    {
        Err("windows_service_uninstall_failed".into())
    } else {
        Ok(())
    }
}
