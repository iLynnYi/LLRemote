//! Delegates only lifecycle control to the user who requested installation or
//! first-time authorization. Existing service security and maintenance stay intact.
use super::native;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSidToSidW, GetSecurityInfo, SetSecurityInfo, SE_SERVICE,
};
use windows_sys::Win32::Security::{
    AddAce, EqualSid, GetAce, GetLengthSid, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::System::Services::{
    SC_HANDLE, SERVICE_QUERY_STATUS, SERVICE_START, SERVICE_STOP,
};

const CONTROL_RIGHTS: u32 = SERVICE_QUERY_STATUS | SERVICE_START | SERVICE_STOP;

struct LocalAllocation(*mut std::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

struct ControlAcl(Vec<usize>);
impl ControlAcl {
    fn pointer(&self) -> *mut ACL {
        self.0.as_ptr().cast_mut().cast()
    }
}

fn sid(value: &str) -> Result<LocalAllocation, String> {
    let mut pointer = null_mut();
    if unsafe { ConvertStringSidToSidW(native::wide(value).as_ptr(), &mut pointer) } == 0 {
        return Err("windows_service_access_denied".into());
    }
    Ok(LocalAllocation(pointer))
}

fn merge_control_acl(acl: *const ACL, user: &LocalAllocation) -> Result<ControlAcl, String> {
    // Reject an absent/null DACL rather than turning an unrestricted descriptor
    // into a new policy with unintended ownership or administrator changes.
    if acl.is_null() {
        return Err("windows_service_access_denied".into());
    }
    let original_size = usize::from(unsafe { (*acl).AclSize });
    let sid_size = unsafe { GetLengthSid(user.0) } as usize;
    // A basic allow ACE has an eight-byte header/mask before its variable SID.
    let ace_size = 8 + sid_size;
    if original_size < std::mem::size_of::<ACL>() {
        return Err("windows_service_access_denied".into());
    }
    let mut already_authorized = false;
    for index in 0..unsafe { (*acl).AceCount } {
        let mut entry = null_mut();
        if unsafe { GetAce(acl, u32::from(index), &mut entry) } == 0 {
            return Err("windows_service_access_denied".into());
        }
        let existing = entry.cast::<ACCESS_ALLOWED_ACE>();
        if unsafe { (*existing).Header.AceType } == 0
            && unsafe { EqualSid(std::ptr::addr_of_mut!((*existing).SidStart).cast(), user.0) } != 0
            && unsafe { (*existing).Mask } & CONTROL_RIGHTS == CONTROL_RIGHTS
        {
            already_authorized = true;
            break;
        }
    }
    let size = original_size + if already_authorized { 0 } else { ace_size };
    if size > u16::MAX as usize {
        return Err("windows_service_access_denied".into());
    }
    let merged = ControlAcl(vec![0_usize; size.div_ceil(std::mem::size_of::<usize>())]);
    unsafe {
        std::ptr::copy_nonoverlapping(acl.cast::<u8>(), merged.pointer().cast(), original_size);
        (*merged.pointer()).AclSize = size as u16;
    }
    if already_authorized {
        return Ok(merged);
    }
    // SetEntriesInAcl(GRANT_ACCESS) can replace a same-user deny ACE. Append a
    // separate allow ACE after existing entries so all prior denials still win.
    let mut bytes = vec![0_usize; ace_size.div_ceil(std::mem::size_of::<usize>())];
    let entry = bytes.as_mut_ptr().cast::<ACCESS_ALLOWED_ACE>();
    unsafe {
        (*entry).Header.AceType = 0;
        (*entry).Header.AceSize = ace_size as u16;
        (*entry).Mask = CONTROL_RIGHTS;
        std::ptr::copy_nonoverlapping(
            user.0.cast::<u8>(),
            std::ptr::addr_of_mut!((*entry).SidStart).cast(),
            sid_size,
        );
    }
    if unsafe {
        AddAce(
            merged.pointer(),
            u32::from((*acl).AclRevision),
            u32::MAX,
            entry.cast(),
            ace_size as u32,
        )
    } == 0
    {
        return Err("windows_service_access_denied".into());
    }
    Ok(merged)
}

pub(super) fn grant(service: SC_HANDLE, user_sid: &str) -> Result<(), String> {
    let user = sid(user_sid)?;
    let mut descriptor = null_mut();
    let mut acl = null_mut();
    if unsafe {
        GetSecurityInfo(
            service.cast(),
            SE_SERVICE,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut acl,
            null_mut(),
            &mut descriptor,
        )
    } != 0
    {
        return Err("windows_service_access_denied".into());
    }
    let _descriptor = LocalAllocation(descriptor);
    let merged = merge_control_acl(acl, &user)?;
    if unsafe {
        SetSecurityInfo(
            service.cast(),
            SE_SERVICE,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            merged.pointer(),
            null_mut(),
        )
    } != 0
    {
        return Err("windows_service_access_denied".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_control_access_tests.rs"]
mod tests;
