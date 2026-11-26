use super::{
    paths,
    types::{Reply, Request},
};
use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;
use zeroize::Zeroizing;

const MAX_MESSAGE_BYTES: usize = 64 * 1024;

fn denied(stage: &str) -> String {
    format!("background_share_access_denied:{stage}")
}

unsafe extern "C" {
    fn geteuid() -> u32;
    fn getpeereid(socket: i32, uid: *mut u32, gid: *mut u32) -> i32;
    fn getsockopt(
        socket: i32,
        level: i32,
        option: i32,
        value: *mut std::ffi::c_void,
        length: *mut u32,
    ) -> i32;
    fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, size: u32) -> i32;
}

fn socket_path() -> PathBuf {
    paths::runtime_directory().join("control.sock")
}

pub(super) fn process_running(pid: u32) -> bool {
    let mut path = [0_u8; 4096];
    i32::try_from(pid).is_ok_and(|pid| unsafe {
        proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) > 0
    })
}

fn verify_user(stream: &UnixStream) -> Result<(), String> {
    let mut uid = 0;
    let mut gid = 0;
    if unsafe { getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0
        || uid != unsafe { geteuid() }
    {
        return Err(denied("peer_user"));
    }
    Ok(())
}

fn verify_server(stream: &UnixStream) -> Result<(), String> {
    verify_user(stream)?;
    let mut pid: i32 = 0;
    let mut length = std::mem::size_of::<i32>() as u32;
    // Darwin SOL_LOCAL / LOCAL_PEERPID, from Apple's sys/un.h.
    if unsafe {
        getsockopt(
            stream.as_raw_fd(),
            0,
            2,
            (&mut pid as *mut i32).cast(),
            &mut length,
        )
    } != 0
        || pid <= 0
    {
        return Err(denied("peer_pid"));
    }
    let mut path = [0_u8; 4096];
    let count = unsafe { proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    if count <= 0 {
        return Err(denied("peer_process_path"));
    }
    let end = path
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(path.len());
    let actual = std::fs::canonicalize(std::path::Path::new(
        std::str::from_utf8(&path[..end]).map_err(|_| denied("peer_path_encoding"))?,
    ))
    .map_err(|_| denied("peer_path_unavailable"))?;
    let expected = std::fs::canonicalize(
        std::env::current_exe().map_err(|_| "background_share_binary_unavailable".to_string())?,
    )
    .map_err(|_| "background_share_binary_unavailable".to_string())?;
    let service_peer = super::bundle_macos::packaged_executable(&expected)
        .into_iter()
        .chain(super::bundle_macos::development_executable())
        .filter_map(|path| std::fs::canonicalize(path).ok())
        .any(|path| path == actual);
    if !service_peer {
        return Err(denied("peer_path_mismatch"));
    }
    Ok(())
}

fn write_message<T: Serialize>(stream: &mut UnixStream, value: &T) -> Result<(), String> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(value).map_err(|_| "background_share_invalid_request".to_string())?,
    );
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("background_share_invalid_request".into());
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|_| "background_share_ipc_unavailable".into())
}

fn read_message<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T, String> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).map_err(ipc_io_error)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE_BYTES {
        return Err("background_share_invalid_request".into());
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    stream.read_exact(&mut bytes).map_err(ipc_io_error)?;
    serde_json::from_slice(&bytes).map_err(|_| "background_share_invalid_request".into())
}

fn ipc_io_error(error: std::io::Error) -> String {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        "background_share_ipc_busy".into()
    } else {
        "background_share_ipc_unavailable".into()
    }
}

pub(super) fn call(request: &Request) -> Result<Reply, String> {
    let mut stream = UnixStream::connect(socket_path())
        .map_err(|_| "background_share_ipc_unavailable".to_string())?;
    verify_server(&stream)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(|_| "background_share_ipc_unavailable".to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "background_share_ipc_unavailable".to_string())?;
    write_message(&mut stream, request)?;
    read_message(&mut stream)
}

pub(super) struct Server {
    listener: UnixListener,
    path: PathBuf,
}
impl Server {
    pub fn new() -> Result<Self, String> {
        let directory = paths::runtime_directory();
        paths::ensure_private_directory(&directory)?;
        let path = socket_path();
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                return Err("background_share_already_running".into());
            }
            std::fs::remove_file(&path)
                .map_err(|_| "background_share_ipc_unavailable".to_string())?;
        }
        let listener = UnixListener::bind(&path)
            .map_err(|_| "background_share_ipc_unavailable".to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "background_share_ipc_unavailable".to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "background_share_ipc_unavailable".to_string())?;
        Ok(Self { listener, path })
    }

    pub fn poll(&self, handler: impl FnOnce(Request) -> Reply) -> Result<(), String> {
        let (mut stream, _) = match self.listener.accept() {
            Ok(peer) => peer,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(_) => return Err("background_share_ipc_unavailable".into()),
        };
        if verify_user(&stream).is_err() {
            return Ok(());
        }
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| "background_share_ipc_unavailable".to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .map_err(|_| "background_share_ipc_unavailable".to_string())?;
        let reply = read_message(&mut stream)
            .map(handler)
            .unwrap_or_else(|code| Reply {
                overview: None,
                error_code: Some(code),
                permission: None,
            });
        let _ = write_message(&mut stream, &reply);
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "../../tests/support/background_share_ipc_tests.rs"]
mod tests;
