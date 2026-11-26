use super::desktop_epoch::DesktopEpoch;
use serde::Serialize;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, IsWellKnownSid, TokenUser, WinLocalSystemSid, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, GetThreadDesktop, GetUserObjectInformationW, OpenInputDesktop, SetThreadDesktop,
    DESKTOP_JOURNALPLAYBACK, DESKTOP_READOBJECTS, DESKTOP_WRITEOBJECTS, HDESK, UOI_NAME,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThreadId, OpenProcessToken,
};

static SERVICE_WORKER: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<DesktopState> = Mutex::new(DesktopState {
    epoch: DesktopEpoch {
        desktop: None,
        epoch: 0,
    },
    input_status: String::new(),
    input_failures: 0,
    last_error_code: None,
});

struct DesktopState {
    epoch: DesktopEpoch,
    input_status: String,
    input_failures: u64,
    last_error_code: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopDiagnostics {
    pub desktop: Option<String>,
    pub desktop_epoch: u64,
    pub input_status: String,
    pub input_failures: u64,
    pub last_error_code: Option<String>,
}

/// Only the broker-created SYSTEM process may enable desktop switching. The
/// Tauri process continues to use the ordinary platform path.
pub(crate) fn enable_service_worker() -> Result<(), String> {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err("windows_service_worker_identity_failed".into());
        }
        let mut buffer = [0usize; 32];
        let mut needed = 0;
        let queried = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            std::mem::size_of_val(&buffer) as u32,
            &mut needed,
        );
        let system = queried != 0
            && IsWellKnownSid(
                (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid,
                WinLocalSystemSid,
            ) != 0;
        CloseHandle(token);
        if !system {
            return Err("windows_service_worker_identity_failed".into());
        }
    }
    SERVICE_WORKER.store(true, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn is_service_worker() -> bool {
    SERVICE_WORKER.load(Ordering::SeqCst)
}

pub(crate) fn diagnostics() -> DesktopDiagnostics {
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    DesktopDiagnostics {
        desktop: state.epoch.desktop.clone(),
        desktop_epoch: state.epoch.epoch,
        input_status: state.input_status.clone(),
        input_failures: state.input_failures,
        last_error_code: state.last_error_code.clone(),
    }
}

fn observe_desktop(name: Option<String>) -> u64 {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let unavailable = name.is_none();
    let previous = state.epoch.epoch;
    let epoch = state.epoch.observe(name);
    if epoch != previous {
        state.input_status = "waiting_for_frame".into();
    }
    if unavailable {
        state.input_status = "blocked".into();
        state.last_error_code = Some("windows_input_desktop_unavailable".into());
    }
    epoch
}

pub(crate) fn frame_is_current(epoch: u64) -> bool {
    if !is_service_worker() {
        return true;
    }
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.epoch.accepts(epoch)
}

pub(crate) fn observed_frame_is_current(epoch: u64) -> bool {
    if is_service_worker() {
        InputDesktop::open().is_ok_and(|desktop| desktop.epoch == epoch && frame_is_current(epoch))
    } else {
        ordinary_desktop_epoch().is_ok_and(|current| current == epoch)
    }
}

pub(crate) fn frame_delivered(epoch: u64) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.epoch.accepts(epoch) && state.input_status == "waiting_for_frame" {
        state.input_status = "ready".into();
        state.last_error_code = None;
    }
}

pub(crate) fn record_input_result(expected: u32, inserted: u32) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if expected == inserted {
        state.input_status = "injected".into();
        state.last_error_code = None;
    } else {
        state.input_status = "blocked".into();
        state.input_failures = state.input_failures.saturating_add(1);
        state.last_error_code = Some("windows_input_injection_failed".into());
    }
}

pub(crate) struct InputDesktop {
    handle: HDESK,
    name: String,
    pub epoch: u64,
}

impl InputDesktop {
    pub(crate) fn open() -> Result<Self, String> {
        Self::open_with_access(DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS | DESKTOP_JOURNALPLAYBACK)
    }

    fn open_with_access(access: u32) -> Result<Self, String> {
        unsafe {
            // Input replay access is required when SendInput uses a desktop
            // explicitly assigned to this SYSTEM worker thread.
            let handle = OpenInputDesktop(0, 0, access);
            if handle.is_null() {
                observe_desktop(None);
                return Err("windows_input_desktop_unavailable".into());
            }
            let mut name = [0u16; 256];
            let mut needed = 0;
            if GetUserObjectInformationW(
                handle,
                UOI_NAME,
                name.as_mut_ptr().cast(),
                std::mem::size_of_val(&name) as u32,
                &mut needed,
            ) == 0
            {
                CloseDesktop(handle);
                observe_desktop(None);
                return Err("windows_input_desktop_unavailable".into());
            }
            let length = name
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(name.len());
            let name = String::from_utf16_lossy(&name[..length]);
            let epoch = observe_desktop(Some(name.clone()));
            Ok(Self {
                handle,
                name,
                epoch,
            })
        }
    }
}

/// Observe access without switching an ordinary user thread onto a protected
/// desktop. Failure is reported as unavailable, never inferred to be a lock.
pub(crate) fn ordinary_desktop_epoch() -> Result<u64, String> {
    let next = InputDesktop::open_with_access(DESKTOP_READOBJECTS)?;
    let mut name = [0u16; 256];
    let mut needed = 0;
    let ok = unsafe {
        GetUserObjectInformationW(
            GetThreadDesktop(GetCurrentThreadId()),
            UOI_NAME,
            name.as_mut_ptr().cast(),
            std::mem::size_of_val(&name) as u32,
            &mut needed,
        )
    };
    let length = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    if ok == 0 || String::from_utf16_lossy(&name[..length]) != next.name {
        observe_desktop(None);
        return Err("windows_input_desktop_unavailable".into());
    }
    Ok(next.epoch)
}

impl Drop for InputDesktop {
    fn drop(&mut self) {
        unsafe {
            CloseDesktop(self.handle);
        }
    }
}

/// A desktop handle belongs to one window-free thread. Restore the borrowed
/// original desktop before closing the handle currently assigned to that thread.
pub(crate) struct DesktopOwner {
    original: HDESK,
    current: Option<InputDesktop>,
}

impl Default for DesktopOwner {
    fn default() -> Self {
        Self {
            original: unsafe { GetThreadDesktop(GetCurrentThreadId()) },
            current: None,
        }
    }
}

impl DesktopOwner {
    pub(crate) fn matches(&self, next: &InputDesktop) -> bool {
        self.current
            .as_ref()
            .is_some_and(|current| current.name == next.name && current.epoch == next.epoch)
    }

    pub(crate) fn bind(&mut self, next: InputDesktop) -> Result<u64, String> {
        let epoch = next.epoch;
        if self.matches(&next) {
            return Ok(epoch);
        }
        if unsafe { SetThreadDesktop(next.handle) } == 0 {
            let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            state.input_status = "blocked".into();
            state.last_error_code = Some("windows_input_desktop_bind_failed".into());
            return Err("windows_input_desktop_bind_failed".into());
        }
        self.current = Some(next);
        Ok(epoch)
    }
}

impl Drop for DesktopOwner {
    fn drop(&mut self) {
        if self.current.is_some() && unsafe { SetThreadDesktop(self.original) } == 0 {
            // The OS closes handles at thread exit. Never close an assigned
            // desktop handle while SetThreadDesktop has failed.
            if let Some(current) = self.current.take() {
                std::mem::forget(current);
            }
        }
    }
}
