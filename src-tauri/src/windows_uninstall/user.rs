//! Resolve a named Windows user without borrowing the administrator's profile.
use crate::windows_service::native;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteTreeW, RegGetValueW, RegLoadKeyW, RegOpenCurrentUser, RegOpenKeyExW,
    RegUnLoadKeyW, HKEY, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ, KEY_WRITE, RRF_NOEXPAND,
    RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};
use windows_sys::Win32::System::RemoteDesktop::{
    WTSEnumerateProcessesW, WTSFreeMemory, WTS_CURRENT_SERVER_HANDLE, WTS_PROCESS_INFOW,
};

pub(super) const UNINSTALL_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LL Remote";
pub(super) const PRODUCT_KEY: &str = "Software\\iLynnYi\\LL Remote";

pub(super) struct Key(pub HKEY, Option<ProfileMount>);
impl Drop for Key {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { RegCloseKey(self.0) };
        }
    }
}

impl Key {
    fn close(mut self) -> Result<(), String> {
        let key = std::mem::replace(&mut self.0, null_mut());
        if !key.is_null() && unsafe { RegCloseKey(key) } != 0 {
            return Err("windows_uninstall_registry_cleanup_failed".into());
        }
        if let Some(mut mount) = self.1.take() {
            mount.unload()?;
        }
        Ok(())
    }
}

fn missing(result: u32) -> bool {
    matches!(result, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
}

pub(super) fn open(root: HKEY, path: &str, access: u32) -> Result<Option<Key>, String> {
    let mut key = null_mut();
    let result = unsafe { RegOpenKeyExW(root, native::wide(path).as_ptr(), 0, access, &mut key) };
    if missing(result) {
        return Ok(None);
    }
    if result != 0 {
        return Err("windows_uninstall_registry_access_denied".into());
    }
    Ok(Some(Key(key, None)))
}

pub(super) fn read(root: HKEY, path: &str, name: &str) -> Result<Option<String>, String> {
    let mut bytes = 0;
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let result = unsafe {
        RegGetValueW(
            root,
            native::wide(path).as_ptr(),
            native::wide(name).as_ptr(),
            flags,
            null_mut(),
            null_mut(),
            &mut bytes,
        )
    };
    if missing(result) {
        return Ok(None);
    }
    if result != 0 || bytes > 65536 || bytes % 2 != 0 {
        return Err("windows_uninstall_registry_invalid".into());
    }
    let mut data = vec![0u16; bytes as usize / 2];
    if unsafe {
        RegGetValueW(
            root,
            native::wide(path).as_ptr(),
            native::wide(name).as_ptr(),
            flags,
            null_mut(),
            data.as_mut_ptr().cast(),
            &mut bytes,
        )
    } != 0
    {
        return Err("windows_uninstall_registry_access_denied".into());
    }
    let length = data
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(data.len());
    String::from_utf16(&data[..length])
        .map(Some)
        .map_err(|_| "windows_uninstall_registry_invalid".into())
}

pub(super) fn delete_key(root: HKEY, path: &str) -> Result<(), String> {
    let result = unsafe { RegDeleteTreeW(root, native::wide(path).as_ptr()) };
    if result == 0 || missing(result) {
        Ok(())
    } else {
        Err("windows_uninstall_registry_cleanup_failed".into())
    }
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    // Existing paths are canonicalized; missing paths are compared lexically
    // only for stale shortcuts/registry records, never for authorizing deletion.
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_owned());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_owned());
    left.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(
            right
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .trim_end_matches('\\'),
        )
}

pub(super) fn registered_at(root: HKEY, directory: &Path) -> Result<bool, String> {
    let Some(command) = read(root, UNINSTALL_KEY, "UninstallString")? else {
        return Ok(false);
    };
    // Only accept our exact quoted command, not a prefix, command arguments,
    // or a similarly named executable in an unrelated directory.
    let Some(path) = command.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
        return Ok(false);
    };
    Ok(same_path(Path::new(path), &directory.join("uninstall.exe")))
}

pub(super) fn current_hive() -> Result<Key, String> {
    let mut key = null_mut();
    if unsafe { RegOpenCurrentUser(KEY_READ | KEY_WRITE, &mut key) } != 0 {
        return Err("windows_uninstall_registry_access_denied".into());
    }
    Ok(Key(key, None))
}

pub(super) struct User {
    pub sid: String,
    pub hive: Key,
    pub local: PathBuf,
    pub desktop: Option<PathBuf>,
    pub programs: Option<PathBuf>,
    pub startup: PathBuf,
}

impl User {
    pub fn resolve(sid: &str) -> Result<Self, String> {
        let current = native::current_user_sid()?;
        let profile = read(
            HKEY_LOCAL_MACHINE,
            &format!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\{sid}"),
            "ProfileImagePath",
        )?
        .ok_or("windows_uninstall_profile_unavailable")?;
        let profile = expand_machine_path(&profile)?;
        native::reject_reparse_chain(&profile)?;
        let hive = if current == sid {
            current_hive()?
        } else if let Some(key) = open(HKEY_USERS, sid, KEY_READ | KEY_WRITE)? {
            key
        } else {
            // NTUSER.DAT can contain different key ACLs, so RegLoadAppKey is
            // unsuitable. Use a unique temporary mount and restore privileges
            // after closing/unloading it, never substitute the admin's HKCU.
            let file = profile.join("NTUSER.DAT");
            native::reject_reparse_chain(&file)?;
            ProfileMount::open(&file)?
        };
        let roaming = folder(&hive, "AppData", &profile, &profile.join("AppData/Roaming"))?;
        let local = folder(
            &hive,
            "Local AppData",
            &profile,
            &profile.join("AppData/Local"),
        )?;
        let desktop = folder(&hive, "Desktop", &profile, &profile.join("Desktop")).ok();
        let programs = folder(
            &hive,
            "Programs",
            &profile,
            &roaming.join("Microsoft/Windows/Start Menu/Programs"),
        )
        .ok();
        let fallback_startup = programs.as_ref().map(|path| path.join("Startup"));
        if fallback_startup.is_none()
            && read(
                hive.0,
                "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders",
                "Startup",
            )?
            .is_none()
        {
            return Err("windows_uninstall_profile_unavailable".into());
        }
        let startup = folder(
            &hive,
            "Startup",
            &profile,
            &fallback_startup
                .unwrap_or_else(|| roaming.join("Microsoft/Windows/Start Menu/Programs/Startup")),
        )?;
        Ok(Self {
            sid: sid.into(),
            hive,
            local,
            desktop,
            programs,
            startup,
        })
    }

    pub fn close(self) -> Result<(), String> {
        self.hive.close()
    }
}

fn folder(hive: &Key, name: &str, profile: &Path, fallback: &Path) -> Result<PathBuf, String> {
    let value = read(
        hive.0,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders",
        name,
    )?;
    let path = match value {
        Some(value) => {
            let mut remaining = value.as_str();
            let mut value = String::new();
            // Expand only the target profile, never process-user environment.
            while let Some(index) = remaining.to_ascii_lowercase().find("%userprofile%") {
                value.push_str(&remaining[..index]);
                value.push_str(&profile.to_string_lossy());
                remaining = &remaining[index + 13..];
            }
            value.push_str(remaining);
            PathBuf::from(value)
        }
        None => fallback.to_owned(),
    };
    if !path.is_absolute() {
        return Err("windows_uninstall_profile_unavailable".into());
    }
    native::reject_reparse_chain(&path)?;
    Ok(path)
}

fn expand_machine_path(value: &str) -> Result<PathBuf, String> {
    use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
    // Absolute profile paths can contain a literal percent sign in a username.
    // Do not interpret those characters as the administrator's environment.
    if Path::new(value).is_absolute() {
        return Ok(PathBuf::from(value));
    }
    let input = native::wide(value);
    let mut output = vec![0u16; 32768];
    let size = unsafe {
        ExpandEnvironmentStringsW(input.as_ptr(), output.as_mut_ptr(), output.len() as u32)
    };
    if size == 0 || size as usize > output.len() {
        return Err("windows_uninstall_profile_unavailable".into());
    }
    let path = PathBuf::from(String::from_utf16_lossy(&output[..size as usize - 1]));
    if !path.is_absolute() || path.to_string_lossy().contains('%') {
        return Err("windows_uninstall_profile_unavailable".into());
    }
    Ok(path)
}

pub(super) struct Processes {
    raw: *mut WTS_PROCESS_INFOW,
    length: u32,
}
impl Processes {
    pub fn enumerate() -> Result<Self, String> {
        // WTS captures pUserSid while enumerating. Without SeDebugPrivilege it
        // can return NULL for another user's application even to an elevated
        // administrator. Enabling it later in OpenProcess is already too late
        // for both owner filtering and detecting the owner's logged-on session.
        let _debug = if native::current_elevation()? {
            Some(PrivilegeGuard::enable("SeDebugPrivilege").map_err(|_| {
                "windows_uninstall_process_access_denied:enumerate_debug_privilege".to_string()
            })?)
        } else {
            None
        };
        let mut value = Self {
            raw: null_mut(),
            length: 0,
        };
        if unsafe {
            WTSEnumerateProcessesW(
                WTS_CURRENT_SERVER_HANDLE,
                0,
                1,
                &mut value.raw,
                &mut value.length,
            )
        } == 0
        {
            return Err("windows_uninstall_process_access_denied".into());
        }
        Ok(value)
    }
    pub fn entries(&self) -> &[WTS_PROCESS_INFOW] {
        if self.length == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.raw, self.length as usize) }
    }
}
impl Drop for Processes {
    fn drop(&mut self) {
        unsafe { WTSFreeMemory(self.raw.cast()) };
    }
}

pub(super) fn process_sid(entry: &WTS_PROCESS_INFOW) -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    if entry.pUserSid.is_null() {
        return None;
    }
    let mut raw = null_mut();
    if unsafe { ConvertSidToStringSidW(entry.pUserSid, &mut raw) } == 0 {
        return None;
    }
    let mut length = 0;
    unsafe {
        while *raw.add(length) != 0 {
            length += 1;
        }
    }
    let sid = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(raw, length)) };
    unsafe { LocalFree(raw.cast()) };
    Some(sid)
}

pub(super) fn logged_on(sid: &str) -> Result<bool, String> {
    Ok(Processes::enumerate()?
        .entries()
        .iter()
        .any(|entry| entry.SessionId != 0 && process_sid(entry).as_deref() == Some(sid)))
}

struct ProfileMount {
    name: Option<String>,
    _privileges: Vec<PrivilegeGuard>,
}

impl ProfileMount {
    fn open(file: &Path) -> Result<Key, String> {
        if !file.is_file() {
            return Err("windows_uninstall_profile_unavailable".into());
        }
        let privileges = vec![
            PrivilegeGuard::enable("SeBackupPrivilege")?,
            PrivilegeGuard::enable("SeRestorePrivilege")?,
        ];
        let name = format!("LLRemote-Uninstall-{}", uuid::Uuid::new_v4());
        if unsafe {
            RegLoadKeyW(
                HKEY_USERS,
                native::wide(&name).as_ptr(),
                native::wide(file).as_ptr(),
            )
        } != 0
        {
            return Err("windows_uninstall_profile_unavailable".into());
        }
        let mount = Self {
            name: Some(name.clone()),
            _privileges: privileges,
        };
        let mut key = open(HKEY_USERS, &name, KEY_READ | KEY_WRITE)?
            .ok_or("windows_uninstall_profile_unavailable")?;
        key.1 = Some(mount);
        Ok(key)
    }

    fn unload(&mut self) -> Result<(), String> {
        if let Some(name) = &self.name {
            if unsafe { RegUnLoadKeyW(HKEY_USERS, native::wide(name).as_ptr()) } != 0 {
                return Err("windows_uninstall_profile_unavailable".into());
            }
            self.name = None;
        }
        Ok(())
    }
}
impl Drop for ProfileMount {
    fn drop(&mut self) {
        let _ = self.unload();
    }
}

pub(super) struct PrivilegeGuard {
    token: native::Handle,
    previous: windows_sys::Win32::Security::TOKEN_PRIVILEGES,
}
impl PrivilegeGuard {
    pub(super) fn enable(name: &str) -> Result<Self, String> {
        use windows_sys::Win32::Security::{
            AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES,
            SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let token = native::process_token(
            unsafe { GetCurrentProcess() },
            TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
        )?;
        let mut id = Default::default();
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), native::wide(name).as_ptr(), &mut id) }
            == 0
        {
            return Err("windows_uninstall_profile_unavailable".into());
        }
        let requested = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: id,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let mut previous = TOKEN_PRIVILEGES::default();
        let mut length = std::mem::size_of::<TOKEN_PRIVILEGES>() as u32;
        if unsafe {
            AdjustTokenPrivileges(token.0, 0, &requested, length, &mut previous, &mut length)
        } == 0
            || native::last_error() != 0
        {
            return Err("windows_uninstall_profile_unavailable".into());
        }
        Ok(Self { token, previous })
    }
}
impl Drop for PrivilegeGuard {
    fn drop(&mut self) {
        if unsafe {
            windows_sys::Win32::Security::AdjustTokenPrivileges(
                self.token.0,
                0,
                &self.previous,
                0,
                null_mut(),
                null_mut(),
            )
        } == 0
        {
            std::process::abort();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_uninstall_registry_tests.rs"]
mod tests;
