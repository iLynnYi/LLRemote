use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    GetNamedSecurityInfoW, SetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetAce, GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    GetTokenInformation, IsValidAcl, IsValidSid, IsWellKnownSid, TokenElevation, TokenUser,
    WinBuiltinAdministratorsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE, ACL,
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    SECURITY_ATTRIBUTES, SE_DACL_PROTECTED, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileAttributesW, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_FILE_ATTRIBUTES,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcessToken,
};
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_System, SHGetKnownFolderPath,
};

// Stable SCM registration name across releases.
pub(super) const SERVICE_NAME: &str = "LLRemoteServer";
pub(super) const BINARY_NAME: &str = "LLRemoteService.exe";

/// Keep an installer's named object alive across UAC and parent timeouts.
/// This is only a lifetime lease, never ownership or permission to mutate it.
pub(super) fn installer_lease() -> Result<Option<Handle>, String> {
    installer_lease_for_name("Global\\LLRemote.Installer.v1")
}

fn installer_lease_for_name(name: &str) -> Result<Option<Handle>, String> {
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Threading::OpenMutexW;
    let handle = unsafe { OpenMutexW(0x0010_0000, 0, wide(name).as_ptr()) }; // SYNCHRONIZE only
    if !handle.is_null() {
        return Ok(Some(Handle(handle)));
    }
    match last_error() {
        ERROR_FILE_NOT_FOUND => Ok(None),
        _ => Err("windows_service_access_denied".into()),
    }
}

pub(crate) struct Handle(pub HANDLE);
impl Handle {
    pub fn new(handle: HANDLE, error: &str) -> Result<Self, String> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(error.into())
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(crate) fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

pub(crate) fn last_error() -> u32 {
    unsafe { GetLastError() }
}

pub(crate) fn current_session() -> Result<u32, String> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        Err("windows_service_internal".into())
    } else {
        Ok(session)
    }
}

/// The standalone worker has no Tauri window bootstrap. Set its process DPI
/// context before capture/input threads start so GDI and mouse coordinates use
/// physical pixels rather than Windows' virtualized screen dimensions.
pub(super) fn initialize_worker_dpi_awareness() -> Result<(), String> {
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 {
            return Ok(());
        }
        // A manifest may already have established per-monitor awareness.
        let current = GetThreadDpiAwarenessContext();
        if AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0
            || AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE) != 0
        {
            return Ok(());
        }
    }
    Err("windows_service_worker_start_failed".into())
}

pub(crate) fn process_token(process: HANDLE, access: u32) -> Result<Handle, String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(process, access, &mut token) } == 0 {
        return Err("windows_service_access_denied".into());
    }
    Handle::new(token, "windows_service_access_denied")
}

pub(super) fn token_is_elevated(token: HANDLE) -> bool {
    token_elevation(token).unwrap_or(false)
}

fn token_elevation(token: HANDLE) -> Result<bool, String> {
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut written = 0;
    if unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut written,
        )
    } == 0
    {
        return Err("windows_service_access_denied".into());
    }
    Ok(elevation.TokenIsElevated != 0)
}

pub(crate) fn current_elevation() -> Result<bool, String> {
    let token = process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY)?;
    token_elevation(token.0)
}

pub(super) fn is_elevated() -> bool {
    current_elevation().unwrap_or(false)
}

pub(super) fn token_is_system(token: HANDLE) -> bool {
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
    }
    if needed == 0 || needed > 65536 {
        return false;
    }
    let mut buffer = vec![0_usize; (needed as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return false;
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    unsafe { IsWellKnownSid(user.User.Sid, WinLocalSystemSid) != 0 }
}

pub(crate) fn current_user_sid() -> Result<String, String> {
    let token = process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY)?;
    token_user_sid(token.0)
}

/// Read the initiating App's actual token, even if UAC used another admin account.
pub(super) fn operator_user_sid(pid: u32) -> Result<String, String> {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let process = Handle::new(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) },
        "windows_service_access_denied",
    )?;
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut session) } == 0 || session != current_session()? {
        return Err("windows_service_access_denied".into());
    }
    let token = process_token(process.0, TOKEN_QUERY)?;
    if token_is_system(token.0) {
        return Err("windows_service_access_denied".into());
    }
    token_user_sid(token.0)
}

pub(crate) fn token_user_sid(token: HANDLE) -> Result<String, String> {
    let mut needed = 0;
    unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed) };
    if needed == 0 || needed > 65536 {
        return Err("windows_service_access_denied".into());
    }
    let mut buffer = vec![0_usize; (needed as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err("windows_service_access_denied".into());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut raw = null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut raw) } == 0 {
        return Err("windows_service_access_denied".into());
    }
    let mut length = 0;
    unsafe {
        while *raw.add(length) != 0 {
            length += 1;
        }
    }
    let sid = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(raw, length)) };
    unsafe { LocalFree(raw.cast()) };
    Ok(sid)
}

pub(crate) struct SecurityDescriptor(*mut std::ffi::c_void);
impl SecurityDescriptor {
    pub fn from_sddl(sddl: &str) -> Result<Self, String> {
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(sddl).as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err("windows_service_access_denied".into());
        }
        Ok(Self(descriptor))
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}
impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

pub(crate) fn known_directory(id: &windows_sys::core::GUID) -> Result<PathBuf, String> {
    let mut value = null_mut();
    if unsafe { SHGetKnownFolderPath(id, 0, null_mut(), &mut value) } < 0 || value.is_null() {
        return Err("windows_service_install_failed".into());
    }
    let mut len = 0;
    unsafe {
        while *value.add(len) != 0 {
            len += 1;
        }
        let path = PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(
            value, len,
        )));
        CoTaskMemFree(value.cast());
        Ok(path)
    }
}

pub(crate) fn install_directory() -> Result<PathBuf, String> {
    Ok(super::storage_layout::install_directory(&known_directory(
        &FOLDERID_ProgramFiles,
    )?))
}
pub(crate) fn system_directory() -> Result<PathBuf, String> {
    known_directory(&FOLDERID_System)
}
pub(super) fn installed_binary() -> Result<PathBuf, String> {
    Ok(install_directory()?.join(BINARY_NAME))
}
pub(super) fn data_directory() -> Result<PathBuf, String> {
    Ok(super::storage_layout::data_directory(&known_directory(
        &FOLDERID_ProgramData,
    )?))
}

pub(super) fn reject_reparse(path: &Path) -> Result<(), String> {
    let attrs = unsafe { GetFileAttributesW(wide(path).as_ptr()) };
    if attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        Err("windows_service_access_denied".into())
    } else {
        Ok(())
    }
}

/// Role metadata must not traverse a junction in either the child or its ancestors.
pub(crate) fn reject_reparse_chain(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        reject_reparse(ancestor)?;
    }
    Ok(())
}

fn administrator_sid(sid: *mut std::ffi::c_void) -> bool {
    !sid.is_null()
        && unsafe { IsValidSid(sid) } != 0
        && (unsafe { IsWellKnownSid(sid, WinLocalSystemSid) } != 0
            || unsafe { IsWellKnownSid(sid, WinBuiltinAdministratorsSid) } != 0)
}

fn protected_metadata_descriptor(descriptor: &SecurityDescriptor) -> bool {
    let mut owner = null_mut();
    let mut acl: *mut ACL = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    let mut control = 0_u16;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorOwner(descriptor.0, &mut owner, &mut defaulted) } == 0
        || !administrator_sid(owner)
        || unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } == 0
        || control & SE_DACL_PROTECTED == 0
        || unsafe {
            GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted)
        } == 0
        || present == 0
        || acl.is_null()
        || unsafe { IsValidAcl(acl) } == 0
    {
        return false;
    }
    use windows_sys::Win32::Foundation::{GENERIC_EXECUTE, GENERIC_READ};
    use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_EXECUTE, FILE_GENERIC_READ};
    let read_only = GENERIC_READ | GENERIC_EXECUTE | FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
    for index in 0..u32::from(unsafe { (*acl).AceCount }) {
        let mut entry = null_mut();
        if unsafe { GetAce(acl, index, &mut entry) } == 0 {
            return false;
        }
        let ace = entry.cast::<ACCESS_ALLOWED_ACE>();
        let kind = unsafe { (*ace).Header.AceType };
        if kind == 1 {
            continue;
        } // Deny ACEs cannot add mutation rights.
        if kind != 0 {
            return false;
        } // Unknown/object ACE layouts fail closed.
        let sid = unsafe { std::ptr::addr_of_mut!((*ace).SidStart).cast() };
        if !administrator_sid(sid) && unsafe { (*ace).Mask } & !read_only != 0 {
            return false;
        }
    }
    true
}

/// Reject user-owned or user-writable precreated metadata instead of trusting repaired contents.
pub(super) fn verify_protected_metadata(path: &Path) -> Result<(), String> {
    reject_reparse_chain(path)?;
    let mut descriptor = null_mut();
    if unsafe {
        GetNamedSecurityInfoW(
            wide(path).as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    } != 0
        || descriptor.is_null()
    {
        return Err("windows_service_access_denied".into());
    }
    if protected_metadata_descriptor(&SecurityDescriptor(descriptor)) {
        Ok(())
    } else {
        Err("windows_service_access_denied".into())
    }
}

pub(crate) struct ManagementLock {
    file: Handle,
    overlap: windows_sys::Win32::System::IO::OVERLAPPED,
}

impl ManagementLock {
    fn acquire(file: Handle, offset: u32, timeout: std::time::Duration) -> Result<Self, String> {
        use windows_sys::Win32::Storage::FileSystem::{
            LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
        };
        let mut overlap = windows_sys::Win32::System::IO::OVERLAPPED::default();
        overlap.Anonymous.Anonymous.Offset = offset;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if unsafe {
                LockFileEx(
                    file.0,
                    LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                    0,
                    1,
                    0,
                    &mut overlap,
                )
            } != 0
            {
                return Ok(Self { file, overlap });
            }
            if last_error() != 33 {
                return Err("windows_service_access_denied".into());
            } // ERROR_LOCK_VIOLATION.
            if std::time::Instant::now() >= deadline {
                return Err("windows_service_ipc_timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

impl Drop for ManagementLock {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Storage::FileSystem::UnlockFileEx(
                self.file.0,
                0,
                1,
                0,
                &mut self.overlap,
            );
        }
    }
}

fn management_lock(offset: u32) -> Result<ManagementLock, String> {
    use windows_sys::Win32::Foundation::GENERIC_READ;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_ALWAYS,
    };
    let directory = data_directory()?;
    reject_reparse_chain(&directory)?;
    if is_elevated() {
        if let Some(parent) = directory.parent() {
            protect_directory(parent, true)?;
        }
        protect_directory(&directory, true)?;
    }
    verify_protected_metadata(&directory)?;
    let path = directory.join("management.lock");
    reject_reparse_chain(&path)?;
    // Creation supplies owner/DACL atomically; existing untrusted children are never repaired.
    let descriptor = SecurityDescriptor::from_sddl("O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;GR;;;BU)")?;
    let attributes = descriptor.attributes();
    let file = Handle::new(
        unsafe {
            CreateFileW(
                wide(&path).as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                &attributes,
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        },
        "windows_service_access_denied",
    )?;
    verify_protected_metadata(&path)?;
    ManagementLock::acquire(file, offset, std::time::Duration::from_secs(30))
}

/// One byte serializes direct App lifecycle operations; callers waiting on UAC hold no gate.
pub(crate) fn lifecycle_lock() -> Result<ManagementLock, String> {
    management_lock(0)
}

/// A separate byte protects role read-modify-replace across administrator helper processes.
pub(super) fn role_lock() -> Result<ManagementLock, String> {
    management_lock(1)
}

/// The installer repairs ownership/DACL before creating files; no user-writable
/// location can become a SYSTEM executable or mutable service configuration.
pub(super) fn protect_directory(path: &Path, readable: bool) -> Result<(), String> {
    reject_reparse(path)?;
    std::fs::create_dir_all(path).map_err(|_| "windows_service_install_failed".to_string())?;
    protect_file(path, readable)
}

pub(crate) fn protect_file(path: &Path, readable: bool) -> Result<(), String> {
    reject_reparse(path)?;
    let sddl = if readable {
        "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)"
    } else {
        "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
    };
    let descriptor = SecurityDescriptor::from_sddl(sddl)?;
    let mut acl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    let mut owner = null_mut();
    unsafe {
        if GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) == 0
            || GetSecurityDescriptorOwner(descriptor.0, &mut owner, &mut defaulted) == 0
        {
            return Err("windows_service_access_denied".into());
        }
        let code = SetNamedSecurityInfoW(
            wide(path).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION
                | OWNER_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            owner,
            null_mut(),
            acl,
            null_mut(),
        );
        if code != 0 {
            return Err("windows_service_access_denied".into());
        }
    }
    Ok(())
}

/// User background sharing is writable only by its owner and SYSTEM. It never
/// uses the system service's administrator-owned configuration directory.
pub(crate) fn protect_user_directory(path: &Path) -> Result<(), String> {
    reject_reparse(path)?;
    std::fs::create_dir_all(path)
        .map_err(|_| "background_share_user_directory_unavailable".to_string())?;
    let sid = current_user_sid()?;
    let descriptor =
        SecurityDescriptor::from_sddl(&format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)"))?;
    let mut acl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) }
        == 0
        || unsafe {
            SetNamedSecurityInfoW(
                wide(path).as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        } != 0
    {
        return Err("background_share_user_directory_unavailable".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_password_metadata_tests.rs"]
mod password_metadata_tests;

#[cfg(test)]
mod installer_lease_tests {
    #[test]
    fn a_maintenance_lease_outlives_the_original_installer_handle() {
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let name = format!("Local\\LLRemote-installer-test-{}", uuid::Uuid::new_v4());
        assert!(super::installer_lease_for_name(&name).unwrap().is_none());
        let original = super::Handle::new(
            unsafe { CreateMutexW(std::ptr::null(), 0, super::wide(&name).as_ptr()) },
            "create test mutex",
        )
        .unwrap();
        let lease = super::installer_lease_for_name(&name).unwrap().unwrap();
        drop(original);
        assert!(super::installer_lease_for_name(&name).unwrap().is_some());
        drop(lease);
        assert!(super::installer_lease_for_name(&name).unwrap().is_none());
    }
}
