//! Start outside a launcher's Job while retaining the same interactive user.
use super::windows_context::{launch_mode, LaunchMode, UserContext};
use crate::windows_service::native::{self, Handle};
use crate::windows_service::user_token;
use std::ffi::{c_void, OsStr};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Security::{
    DuplicateTokenEx, GetTokenInformation, SecurityImpersonation, TokenElevation, TokenPrimary,
    TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    CreateProcessAsUserW, CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess,
    GetExitCodeProcess, GetProcessId, InitializeProcThreadAttributeList, OpenProcess, ResumeThread,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NEW_PROCESS_GROUP,
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    PROCESS_CREATE_PROCESS, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

fn failed(stage: &str) -> String {
    format!(
        "background_share_startup_unavailable: stage={stage};win32={}",
        unsafe { GetLastError() }
    )
}

fn elevated(token: HANDLE) -> Result<bool, String> {
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut used = 0;
    if unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut used,
        )
    } == 0
    {
        return Err(failed("token_elevation"));
    }
    Ok(elevation.TokenIsElevated != 0)
}

fn validate_parent(parent: HANDLE) -> Result<LaunchMode, String> {
    let denied = |_| "background_share_access_denied".to_string();
    let current =
        native::process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY).map_err(denied)?;
    let selected = native::process_token(parent, TOKEN_QUERY).map_err(denied)?;
    let current_sid = native::token_user_sid(current.0).map_err(denied)?;
    let selected_sid = native::token_user_sid(selected.0).map_err(denied)?;
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetProcessId(parent), &mut session) } == 0 {
        return Err(failed("parent_session"));
    }
    launch_mode(
        &UserContext {
            sid: &current_sid,
            session: native::current_session().map_err(denied)?,
            elevated: elevated(current.0)?,
        },
        &UserContext {
            sid: &selected_sid,
            session,
            elevated: elevated(selected.0)?,
        },
    )
}

struct UserEnvironment(*mut c_void);

impl UserEnvironment {
    fn new(token: HANDLE) -> Result<Self, String> {
        let mut environment = null_mut();
        if unsafe { CreateEnvironmentBlock(&mut environment, token, 0) } == 0 {
            return Err(failed("user_environment"));
        }
        Ok(Self(environment))
    }
}

impl Drop for UserEnvironment {
    fn drop(&mut self) {
        unsafe { DestroyEnvironmentBlock(self.0) };
    }
}

fn restricted_launch_token() -> Result<Handle, String> {
    let source = native::process_token(
        unsafe { GetCurrentProcess() },
        TOKEN_QUERY | TOKEN_DUPLICATE,
    )
    .map_err(|_| "background_share_access_denied".to_string())?;
    let selected = user_token::for_session(
        source,
        native::current_session().map_err(user_token::background_error)?,
    )
    .map_err(user_token::background_error)?;
    let mut primary = null_mut();
    if unsafe {
        DuplicateTokenEx(
            selected.0,
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY,
            null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut primary,
        )
    } == 0
    {
        return Err(failed("primary_token"));
    }
    Handle::new(primary, "background_share_user_context_required")
}

fn validate_child(process: HANDLE) -> Result<(), String> {
    let token = native::process_token(process, TOKEN_QUERY)
        .map_err(|_| "background_share_access_denied".to_string())?;
    let sid = native::token_user_sid(token.0)
        .map_err(|_| "background_share_access_denied".to_string())?;
    if sid != native::current_user_sid().map_err(user_token::background_error)? {
        return Err("background_share_access_denied".into());
    }
    user_token::validate(
        token.0,
        native::current_session().map_err(user_token::background_error)?,
    )
    .map_err(user_token::background_error)
}

struct Attributes {
    _storage: Vec<usize>,
    list: *mut c_void,
}

impl Attributes {
    fn new(parent: &Handle) -> Result<Self, String> {
        let mut size = 0;
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size) };
        if size == 0 || size > 65536 {
            return Err(failed("attribute_size"));
        }
        let mut storage = vec![0_usize; size.div_ceil(size_of::<usize>())];
        let list = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut size) } == 0 {
            return Err(failed("attribute_initialize"));
        }
        let attributes = Self {
            _storage: storage,
            list,
        };
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.list,
                0,
                PROC_THREAD_ATTRIBUTE_PARENT_PROCESS as usize,
                (&parent.0 as *const HANDLE).cast(),
                size_of::<HANDLE>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(failed("attribute_parent"));
        }
        Ok(attributes)
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.list) };
    }
}

pub(super) struct Process {
    handle: Handle,
}

impl Process {
    pub(super) fn check_running(&self) -> Result<(), String> {
        match unsafe { WaitForSingleObject(self.handle.0, 0) } {
            WAIT_TIMEOUT => Ok(()),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.handle.0, &mut code) } == 0 {
                    return Err(failed("process_exit_code"));
                }
                Err(format!(
                    "background_share_runtime_failed: stage=process_exit;exit_code={code}"
                ))
            }
            _ => Err(failed("process_wait")),
        }
    }
}

fn command_line(executable: &Path, argument: &str) -> Vec<u16> {
    [b'"' as u16]
        .into_iter()
        .chain(executable.as_os_str().encode_wide())
        .chain([b'"' as u16, b' ' as u16])
        .chain(OsStr::new(argument).encode_wide())
        .chain([0])
        .collect()
}

fn spawn(
    parent: &Handle,
    executable: &Path,
    argument: &str,
    mode: LaunchMode,
) -> Result<Process, String> {
    let attributes = Attributes::new(parent)?;
    let explicit_token = if mode == LaunchMode::RestrictedToken {
        Some(restricted_launch_token()?)
    } else {
        None
    };
    let desktop_token = if explicit_token.is_none() {
        Some(
            native::process_token(parent.0, TOKEN_QUERY | TOKEN_DUPLICATE)
                .map_err(|_| "background_share_access_denied".to_string())?,
        )
    } else {
        None
    };
    let environment_token = explicit_token
        .as_ref()
        .or(desktop_token.as_ref())
        .ok_or("background_share_user_context_required")?;
    let environment = UserEnvironment::new(environment_token.0)?;
    // Do not inherit an elevated launcher's administrator-only object DACL.
    // This is needed for both Explorer-token inheritance and explicit tokens.
    let object_security =
        user_token::object_security(environment_token.0).map_err(user_token::background_error)?;
    let object_attributes = object_security.attributes();
    let application = executable
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let mut command = command_line(executable, argument);
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.lpAttributeList = attributes.list;
    let mut desktop = native::wide("WinSta0\\Default");
    if explicit_token.is_some()
        && native::current_session().map_err(user_token::background_error)? != 0
    {
        startup.StartupInfo.lpDesktop = desktop.as_mut_ptr();
    }
    let directory = executable.parent().map(native::wide);
    let directory = directory.as_ref().map_or(null(), |value| value.as_ptr());
    let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
    let flags = CREATE_NO_WINDOW
        | CREATE_NEW_PROCESS_GROUP
        | EXTENDED_STARTUPINFO_PRESENT
        | CREATE_SUSPENDED
        | CREATE_UNICODE_ENVIRONMENT;
    let created = if let Some(token) = &explicit_token {
        unsafe {
            CreateProcessAsUserW(
                token.0,
                application.as_ptr(),
                command.as_mut_ptr(),
                &object_attributes,
                &object_attributes,
                0,
                flags,
                environment.0,
                directory,
                &startup.StartupInfo,
                &mut process,
            )
        }
    } else {
        unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                &object_attributes,
                &object_attributes,
                0,
                flags,
                environment.0,
                directory,
                &startup.StartupInfo,
                &mut process,
            )
        }
    };
    if created == 0 {
        return Err(failed("create_process"));
    }
    let handle = Handle::new(process.hProcess, "background_share_startup_unavailable")?;
    // No background code runs until its real process token has been checked.
    // Closing a handle alone would leave a failed child suspended forever.
    let started = (|| {
        let thread = Handle::new(process.hThread, "background_share_startup_unavailable")?;
        validate_child(handle.0)?;
        if unsafe { ResumeThread(thread.0) } == u32::MAX {
            return Err(failed("resume_process"));
        }
        Ok(())
    })();
    if let Err(error) = started {
        unsafe {
            TerminateProcess(handle.0, 1);
            WaitForSingleObject(handle.0, 5000);
        }
        return Err(error);
    }
    Ok(Process { handle })
}

/// Explorer's token and Job are inherited only after verifying user and Session.
/// This keeps the host alive when pnpm/Tauri closes its own kill-on-close Job.
pub(super) fn launch(executable: &Path, argument: &str) -> Result<Process, String> {
    let shell = unsafe { GetShellWindow() };
    if shell.is_null() {
        return Err("background_share_startup_unavailable: stage=desktop_shell".into());
    }
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(shell, &mut pid) } == 0 || pid == 0 {
        return Err(failed("desktop_pid"));
    }
    let parent = Handle::new(
        unsafe {
            OpenProcess(
                PROCESS_CREATE_PROCESS | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        },
        "background_share_startup_unavailable",
    )?;
    let mode = validate_parent(parent.0)?;
    spawn(&parent, executable, argument, mode)
}

/// Login registrations can enter the binary directly with an elevated desktop
/// token. Hand off before opening IPC, reading credentials, or starting Agent.
pub(super) fn relaunch_if_elevated(argument: &str) -> Result<bool, String> {
    let token = native::process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY)
        .map_err(|_| "background_share_access_denied".to_string())?;
    if !elevated(token.0)? {
        return Ok(false);
    }
    let executable =
        std::env::current_exe().map_err(|_| "background_share_binary_unavailable".to_string())?;
    launch(&executable, argument)?.check_running()?;
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/support/background_share_windows_launch_tests.rs"]
mod tests;
