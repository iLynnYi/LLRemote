use super::{Message, NativeKey};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tauri::Emitter;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
use windows_core_webview::Interface;
use windows_sys::Win32::{
    System::Threading::CREATE_NO_WINDOW, UI::WindowsAndMessaging::GetForegroundWindow,
};
#[path = "windows_worker.rs"]
mod worker;

#[derive(Serialize, Deserialize)]
struct WorkerConfig {
    hwnd: isize,
    parent_pid: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
enum WorkerMessage {
    // Confirms hook installation and its live worker, not every OS shortcut.
    Ready,
    Key(NativeKey),
    Stopped(String),
}

pub(super) fn run_worker_if_requested() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--session-keyboard-worker") {
        return false;
    }
    if let Err(reason) = worker::run() {
        if let Ok(message) = serde_json::to_string(&WorkerMessage::Stopped(reason)) {
            let _ = writeln!(std::io::stdout(), "{message}");
        }
    }
    true
}

pub(super) struct CaptureHandle {
    pub token: String,
    active: Arc<AtomicBool>,
    child: Arc<Mutex<Child>>,
    input: Option<ChildStdin>,
}
impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        self.input.take();
        // A stopped or wedged reader cannot leave a system hook behind.
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

pub(super) fn is_foreground(hwnd: isize) -> bool {
    unsafe { GetForegroundWindow() as isize == hwnd }
}

pub(super) fn browser_accelerators(
    window: &tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    window
        .with_webview(move |webview| unsafe {
            if let Ok(settings) = webview
                .controller()
                .CoreWebView2()
                .and_then(|view| view.Settings())
            {
                if let Ok(settings) = settings.cast::<ICoreWebView2Settings3>() {
                    let _ = settings.SetAreBrowserAcceleratorKeysEnabled(enabled);
                }
            }
        })
        .map_err(|error| format!("keyboard_capture_webview:{error}"))
}

pub(super) fn start(
    window: &tauri::WebviewWindow,
    token: String,
    hwnd: isize,
) -> Result<CaptureHandle, String> {
    // The GUI host on Windows ARM can register a hook without receiving direct
    // callbacks. The same executable before GUI initialization receives them.
    let executable =
        std::env::current_exe().map_err(|error| format!("keyboard_capture_binary:{error}"))?;
    let mut child = Command::new(executable)
        .arg("--session-keyboard-worker")
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("keyboard_capture_spawn:{error}"))?;
    let input = child.stdin.take();
    let output = child.stdout.take();
    let active = Arc::new(AtomicBool::new(true));
    let child = Arc::new(Mutex::new(child));
    let mut handle = CaptureHandle {
        token: token.clone(),
        active: active.clone(),
        child,
        input,
    };
    let output = output.ok_or("keyboard_capture_pipe_missing")?;
    let config = serde_json::to_string(&WorkerConfig {
        hwnd,
        parent_pid: std::process::id(),
    })
    .map_err(|_| "keyboard_capture_config_failed")?;
    writeln!(
        handle
            .input
            .as_mut()
            .ok_or("keyboard_capture_pipe_missing")?,
        "{config}"
    )
    .map_err(|_| "keyboard_capture_config_io")?;
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let event_window = window.clone();
    let event_child = handle.child.clone();
    thread::Builder::new()
        .name("session-keyboard-events".into())
        .spawn(move || {
            let mut output = BufReader::new(output);
            let mut ready = false;
            let mut stopped = "unavailable";
            loop {
                let mut line = String::new();
                match output.by_ref().take(4097).read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    _ => (),
                }
                if line.len() > 4096 {
                    break;
                }
                match serde_json::from_str::<WorkerMessage>(&line) {
                    Ok(WorkerMessage::Ready) if !ready => {
                        ready = true;
                        let _ = ready_sender.send(Ok(()));
                    }
                    Ok(WorkerMessage::Key(key)) if ready && active.load(Ordering::Acquire) => {
                        if event_window
                            .emit(
                                "session-native-keyboard",
                                Message {
                                    token: token.clone(),
                                    key: Some(key),
                                    reason: None,
                                },
                            )
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(WorkerMessage::Stopped(reason)) => {
                        stopped = match reason.as_str() {
                            "focus" => "focus",
                            "overflow" => "overflow",
                            _ => "unavailable",
                        };
                        if !ready {
                            let _ = ready_sender.send(Err(reason));
                        }
                        break;
                    }
                    _ => break,
                }
            }
            if let Ok(mut child) = event_child.lock() {
                let _ = child.kill();
            }
            if !ready {
                let _ = ready_sender.try_send(Err("keyboard_capture_worker_stopped".into()));
            }
            if active.swap(false, Ordering::AcqRel) {
                let _ = event_window.emit(
                    "session-native-keyboard",
                    Message {
                        token,
                        key: None,
                        reason: Some(stopped),
                    },
                );
            }
        })
        .map_err(|error| format!("keyboard_capture_events_spawn:{error}"))?;
    ready_receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|_| "keyboard_capture_start_timeout")??;
    Ok(handle)
}
