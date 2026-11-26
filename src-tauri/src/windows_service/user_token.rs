//! Select a least-privilege token for user sharing and file/clipboard actions. In
//! particular, an elevated UAC-disabled account has no linked limited token.
use super::native::{self, Handle, SecurityDescriptor};
use std::mem::{size_of, size_of_val};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::{
    CreateRestrictedToken, CreateWellKnownSid, DuplicateTokenEx, GetSecurityDescriptorDacl,
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsWellKnownSid,
    SecurityImpersonation, SetTokenInformation, TokenDefaultDacl, TokenElevation,
    TokenElevationType, TokenElevationTypeDefault, TokenElevationTypeFull, TokenGroups,
    TokenIntegrityLevel, TokenLinkedToken, TokenPrimary, TokenSessionId,
    WinBuiltinAdministratorsSid, WinMediumLabelSid, DISABLE_MAX_PRIVILEGE, LUA_TOKEN,
    SID_AND_ATTRIBUTES, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY, TOKEN_DEFAULT_DACL,
    TOKEN_DUPLICATE, TOKEN_ELEVATION, TOKEN_GROUPS, TOKEN_INFORMATION_CLASS, TOKEN_LINKED_TOKEN,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows_sys::Win32::System::SystemServices::{
    SECURITY_MANDATORY_MEDIUM_RID, SE_GROUP_ENABLED, SE_GROUP_INTEGRITY,
};

fn failure(stage: &str) -> String {
    format!("windows_service_user_agent_identity_failed:{stage}")
}

fn api_failure(stage: &str) -> String {
    format!("{}:{}", failure(stage), native::last_error())
}

fn information<T: Default>(
    token: HANDLE,
    class: TOKEN_INFORMATION_CLASS,
    stage: &str,
) -> Result<T, String> {
    let mut value = T::default();
    let mut written = 0;
    if unsafe {
        GetTokenInformation(
            token,
            class,
            (&mut value as *mut T).cast(),
            size_of::<T>() as u32,
            &mut written,
        )
    } == 0
    {
        return Err(api_failure(stage));
    }
    Ok(value)
}

fn buffer(
    token: HANDLE,
    class: TOKEN_INFORMATION_CLASS,
    stage: &str,
) -> Result<Vec<usize>, String> {
    let mut needed = 0;
    unsafe { GetTokenInformation(token, class, null_mut(), 0, &mut needed) };
    if needed == 0 || needed > 65536 {
        return Err(api_failure(stage));
    }
    let mut data = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    if unsafe { GetTokenInformation(token, class, data.as_mut_ptr().cast(), needed, &mut needed) }
        == 0
    {
        return Err(api_failure(stage));
    }
    Ok(data)
}

fn user_sid(token: HANDLE) -> Result<String, String> {
    let sid = native::token_user_sid(token).map_err(|_| failure("user_sid"))?;
    if sid == "S-1-5-18" {
        return Err(failure("system_token"));
    }
    Ok(sid)
}

/// A de-elevated child must be able to reopen its own process and threads by
/// PID/TID. Winsock's WSADuplicateSocket (including TcpStream::try_clone) needs
/// that access. An administrator-only creator DACL loses its effective grant
/// after the Administrators group becomes deny-only.
pub(crate) fn object_security(token: HANDLE) -> Result<SecurityDescriptor, String> {
    let sid = user_sid(token)?;
    SecurityDescriptor::from_sddl(&format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})"))
        .map_err(|_| api_failure("object_security"))
}

fn set_user_default_dacl(token: HANDLE) -> Result<(), String> {
    let security = object_security(token)?;
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl = null_mut();
    if unsafe {
        GetSecurityDescriptorDacl(
            security.attributes().lpSecurityDescriptor,
            &mut present,
            &mut acl,
            &mut defaulted,
        )
    } == 0
        || present == 0
        || acl.is_null()
    {
        return Err(api_failure("default_dacl"));
    }
    let dacl = TOKEN_DEFAULT_DACL { DefaultDacl: acl };
    // SetTokenInformation copies the ACL; the descriptor stays alive until the
    // call returns. This changes only the new token, never the source token.
    if unsafe {
        SetTokenInformation(
            token,
            TokenDefaultDacl,
            (&dacl as *const TOKEN_DEFAULT_DACL).cast(),
            size_of::<TOKEN_DEFAULT_DACL>() as u32,
        )
    } == 0
    {
        return Err(api_failure("set_default_dacl"));
    }
    Ok(())
}

/// Used again inside the child. Query failures are errors, never evidence of an
/// ordinary token. No SID, credentials, or paths enter diagnostic messages.
pub(crate) fn validate(token: HANDLE, session_id: u32) -> Result<(), String> {
    user_sid(token)?;
    let elevation: TOKEN_ELEVATION = information(token, TokenElevation, "elevation")?;
    if elevation.TokenIsElevated != 0 {
        return Err(failure("elevated_token"));
    }
    let actual_session: u32 = information(token, TokenSessionId, "session")?;
    if actual_session != session_id {
        return Err(failure("session_mismatch"));
    }
    let label = buffer(token, TokenIntegrityLevel, "integrity")?;
    let label = unsafe { &*label.as_ptr().cast::<TOKEN_MANDATORY_LABEL>() };
    let count = unsafe { *GetSidSubAuthorityCount(label.Label.Sid) };
    if count == 0
        || unsafe { *GetSidSubAuthority(label.Label.Sid, u32::from(count - 1)) }
            > SECURITY_MANDATORY_MEDIUM_RID as u32
    {
        return Err(failure("high_integrity"));
    }
    let groups = buffer(token, TokenGroups, "groups")?;
    let group_list = unsafe { &*groups.as_ptr().cast::<TOKEN_GROUPS>() };
    let entries = group_list.Groups.as_ptr();
    let offset = entries as usize - groups.as_ptr() as usize;
    let available = groups.len() * size_of::<usize>();
    if group_list.GroupCount as usize
        > available.saturating_sub(offset) / size_of::<SID_AND_ATTRIBUTES>()
    {
        return Err(failure("invalid_groups"));
    }
    for entry in unsafe { std::slice::from_raw_parts(entries, group_list.GroupCount as usize) } {
        if entry.Attributes & SE_GROUP_ENABLED as u32 != 0
            && unsafe { IsWellKnownSid(entry.Sid, WinBuiltinAdministratorsSid) } != 0
        {
            return Err(failure("enabled_administrators"));
        }
    }
    Ok(())
}

pub(crate) fn for_session(source: Handle, session_id: u32) -> Result<Handle, String> {
    let original_sid = user_sid(source.0)?;
    let elevation: TOKEN_ELEVATION = information(source.0, TokenElevation, "elevation")?;
    let token = if elevation.TokenIsElevated == 0 {
        source
    } else {
        let kind: i32 = information(source.0, TokenElevationType, "elevation_type")?;
        if kind == TokenElevationTypeFull {
            let linked: TOKEN_LINKED_TOKEN =
                information(source.0, TokenLinkedToken, "linked_token")?;
            Handle::new(linked.LinkedToken, &failure("invalid_linked_token"))?
        } else if kind == TokenElevationTypeDefault {
            // An unsplit administrator token (e.g. UAC disabled) cannot be
            // de-elevated by asking for a nonexistent linked token.
            restricted(source.0)?
        } else {
            return Err(failure("inconsistent_elevation"));
        }
    };
    if user_sid(token.0)? != original_sid {
        return Err(failure("user_mismatch"));
    }
    validate(token.0, session_id)?;
    Ok(token)
}

pub(crate) fn background_error(error: String) -> String {
    match error.strip_prefix("windows_service_user_agent_identity_failed") {
        Some(stage) => format!("background_share_user_context_required{stage}"),
        None => "background_share_access_denied".into(),
    }
}

fn restricted(source: HANDLE) -> Result<Handle, String> {
    let mut raw = null_mut();
    if unsafe {
        CreateRestrictedToken(
            source,
            LUA_TOKEN | DISABLE_MAX_PRIVILEGE,
            0,
            null(),
            0,
            null(),
            0,
            null(),
            &mut raw,
        )
    } == 0
    {
        return Err(api_failure("restricted_token"));
    }
    let restricted = Handle::new(raw, &failure("invalid_restricted_token"))?;
    // CreateRestrictedToken preserves the source handle's access (normally
    // QUERY | DUPLICATE). Adjusting integrity requires ADJUST_DEFAULT on the
    // new token, even though its administrator groups are already disabled.
    let mut primary = null_mut();
    if unsafe {
        DuplicateTokenEx(
            restricted.0,
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY | TOKEN_ADJUST_DEFAULT,
            null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut primary,
        )
    } == 0
    {
        return Err(api_failure("restricted_primary_token"));
    }
    let token = Handle::new(primary, &failure("invalid_restricted_primary_token"))?;
    let mut sid = [0usize; 16];
    let mut sid_size = size_of_val(&sid) as u32;
    if unsafe {
        CreateWellKnownSid(
            WinMediumLabelSid,
            null_mut(),
            sid.as_mut_ptr().cast(),
            &mut sid_size,
        )
    } == 0
    {
        return Err(api_failure("medium_sid"));
    }
    let label = TOKEN_MANDATORY_LABEL {
        Label: SID_AND_ATTRIBUTES {
            Sid: sid.as_mut_ptr().cast(),
            Attributes: SE_GROUP_INTEGRITY as u32,
        },
    };
    if unsafe {
        SetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            (&label as *const TOKEN_MANDATORY_LABEL).cast(),
            size_of::<TOKEN_MANDATORY_LABEL>() as u32 + sid_size,
        )
    } == 0
    {
        return Err(api_failure("set_integrity"));
    }
    set_user_default_dacl(token.0)?;
    Ok(token)
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_user_token_tests.rs"]
mod tests;
