use super::types::{Reply, Request};
use crate::windows_service::{ipc, native};
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Pipes::{DisconnectNamedPipe, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};

fn pipe_name(sid: &str) -> String {
    format!(r"\\.\pipe\LLRemoteSharing-v1-{sid}")
}

fn verify_server(
    pipe: windows_sys::Win32::Foundation::HANDLE,
) -> Result<std::path::PathBuf, String> {
    server_process(pipe, PROCESS_QUERY_LIMITED_INFORMATION).map(|(_, image)| image)
}

fn server_process(
    pipe: windows_sys::Win32::Foundation::HANDLE,
    access: u32,
) -> Result<(native::Handle, std::path::PathBuf), String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .ok_or("background_share_user_directory_unavailable")?;
    server_process_in(pipe, access, &local)
}

fn server_process_in(
    pipe: windows_sys::Win32::Foundation::HANDLE,
    access: u32,
    local: &std::path::Path,
) -> Result<(native::Handle, std::path::PathBuf), String> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
        return Err("background_share_access_denied".into());
    }
    let process = native::Handle::new(
        unsafe { OpenProcess(access, 0, pid) },
        "background_share_access_denied",
    )?;
    let mut path = [0_u16; 32768];
    let mut length = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length) } == 0 {
        return Err("background_share_access_denied".into());
    }
    let image = std::path::PathBuf::from(String::from_utf16_lossy(&path[..length as usize]));
    let binaries = local.join("LLRemote/Sharing/bin");
    if image.parent() != Some(binaries.as_path())
        || !image
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("LLRemoteService-"))
    {
        return Err("background_share_access_denied".into());
    }
    Ok((process, image))
}

/// Probe existence without connecting or sending requests.
pub(super) fn installer_has_runtime() -> Result<bool, String> {
    use windows_sys::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_SEM_TIMEOUT,
    };
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
    let name = native::wide(pipe_name(&native::current_user_sid()?));
    if unsafe { WaitNamedPipeW(name.as_ptr(), 1) } != 0 {
        return Ok(true);
    }
    match native::last_error() {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(false),
        ERROR_PIPE_BUSY | ERROR_SEM_TIMEOUT => Ok(true),
        error => Err(format!("background_share_probe_failed:{error}")),
    }
}

/// Stop only the authenticated user's helper and wait on that exact process.
/// Missing IPC is harmless; busy/denied IPC must not masquerade as a stopped host.
pub(super) fn stop_for_uninstall() -> Result<(), String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .ok_or("background_share_user_directory_unavailable")?;
    stop_for_uninstall_user(&native::current_user_sid()?, &local)
}

pub(super) fn stop_for_uninstall_user(sid: &str, local: &std::path::Path) -> Result<(), String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
    use windows_sys::Win32::System::Pipes::{
        SetNamedPipeHandleState, PIPE_NOWAIT, PIPE_READMODE_MESSAGE,
    };
    use windows_sys::Win32::System::Threading::{WaitForSingleObject, PROCESS_SYNCHRONIZE};
    let name = native::wide(pipe_name(sid));
    let deadline = Instant::now() + Duration::from_secs(8);
    let pipe = loop {
        let raw = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if raw != INVALID_HANDLE_VALUE {
            break native::Handle::new(raw, "background_share_access_denied")?;
        }
        match native::last_error() {
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(()),
            ERROR_PIPE_BUSY if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => return Err("background_share_uninstall_stop_failed".into()),
        }
    };
    let mode = PIPE_READMODE_MESSAGE | PIPE_NOWAIT;
    if unsafe { SetNamedPipeHandleState(pipe.0, &mode, null(), null()) } == 0 {
        return Err("background_share_uninstall_stop_failed".into());
    }
    let (process, _) = server_process_in(
        pipe.0,
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
        local,
    )?;
    ipc::write_message(pipe.0, &Request::Stop, deadline)?;
    let reply: Reply = ipc::read_message(pipe.0, deadline)?;
    ipc::write_message(pipe.0, &true, deadline)?;
    reply.into_result()?;
    drop(pipe);
    if unsafe { WaitForSingleObject(process.0, 8_000) } != WAIT_OBJECT_0 {
        return Err("background_share_uninstall_stop_failed".into());
    }
    Ok(())
}

pub(super) fn call(request: &Request) -> Result<Reply, String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let pipe = ipc::open_client(
        &pipe_name(&native::current_user_sid()?),
        Duration::from_millis(300),
    )
    .map_err(|_| "background_share_ipc_unavailable".to_string())?;
    verify_server(pipe.0)?;
    ipc::write_message(pipe.0, request, deadline)
        .map_err(|_| "background_share_ipc_unavailable".to_string())?;
    let reply = ipc::read_message(pipe.0, deadline)
        .map_err(|_| "background_share_invalid_reply".to_string())?;
    ipc::write_message(pipe.0, &true, deadline)
        .map_err(|_| "background_share_ipc_unavailable".to_string())?;
    Ok(reply)
}

pub(super) struct Server {
    pipe: native::Handle,
}
impl Server {
    pub fn new() -> Result<Self, String> {
        let sid = native::current_user_sid()?;
        let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})");
        Ok(Self {
            pipe: ipc::create_server(&pipe_name(&sid), &sddl)
                .map_err(|_| "background_share_already_running".to_string())?,
        })
    }

    pub fn poll(&self, handler: impl FnOnce(Request) -> Reply) -> Result<(), String> {
        if !ipc::connected(self.pipe.0)
            .map_err(|_| "background_share_ipc_unavailable".to_string())?
        {
            return Ok(());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let reply = ipc::read_message(self.pipe.0, deadline)
            .map(handler)
            .unwrap_or_else(|_| Reply {
                overview: None,
                error_code: Some("background_share_invalid_request".into()),
                permission: None,
            });
        let response_deadline = Instant::now() + Duration::from_secs(3);
        if ipc::write_message(self.pipe.0, &reply, response_deadline).is_ok() {
            let _ = ipc::read_message::<bool>(self.pipe.0, response_deadline);
        }
        unsafe { DisconnectNamedPipe(self.pipe.0) };
        Ok(())
    }
}
