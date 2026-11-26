#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", test))]
mod macos_policy;
#[cfg(any(windows, test))]
mod policy;
#[cfg(windows)]
mod windows;

#[cfg(any(windows, target_os = "macos"))]
use serde::{Deserialize, Serialize};

#[cfg(any(windows, target_os = "macos"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativeKey {
    key: String,
    code: String,
    location: u32,
    down: bool,
    ctrl_key: bool,
    alt_key: bool,
    shift_key: bool,
    meta_key: bool,
}

#[cfg(any(windows, target_os = "macos"))]
#[derive(Clone, Serialize)]
pub(super) struct Message {
    token: String,
    key: Option<NativeKey>,
    reason: Option<&'static str>,
}

#[derive(Default)]
pub struct SessionKeyboardCapture {
    #[cfg(windows)]
    windows: std::sync::Mutex<std::collections::HashMap<String, windows::CaptureHandle>>,
    #[cfg(target_os = "macos")]
    macos: std::sync::Mutex<std::collections::HashMap<String, macos::CaptureHandle>>,
}

impl SessionKeyboardCapture {
    pub fn disable_unfocused_window(&self, window: &tauri::Window) {
        #[cfg(windows)]
        if window
            .hwnd()
            .is_ok_and(|hwnd| windows::is_foreground(hwnd.0 as isize))
        {
            return;
        }
        self.disable_window(window);
    }

    pub fn disable_window(&self, window: &tauri::Window) {
        #[cfg(windows)]
        {
            use tauri::Manager;
            if let Ok(mut windows) = self.windows.lock() {
                windows.remove(window.label());
            }
            if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                let _ = windows::browser_accelerators(&webview, true);
            }
        }
        #[cfg(target_os = "macos")]
        if let Ok(mut captures) = self.macos.lock() {
            captures.remove(window.label());
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        let _ = window;
    }
}

#[tauri::command]
pub async fn is_session_keyboard_capture_focused(
    window: tauri::WebviewWindow,
) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        {
            let hwnd = window
                .hwnd()
                .map_err(|error| format!("keyboard_capture_window:{error}"))?
                .0 as isize;
            // Tao's active-window query can miss a focused WebView. Shortcut
            // ownership follows the actual foreground top-level window.
            Ok(windows::is_foreground(hwnd))
        }
        #[cfg(not(windows))]
        window
            .is_focused()
            .map_err(|error| format!("keyboard_capture_focus:{error}"))
    })
    .await
    .map_err(|_| "keyboard_capture_dispatch_failed".to_owned())?
}

#[tauri::command]
pub async fn set_session_keyboard_capture(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, SessionKeyboardCapture>,
    enabled: bool,
    token: String,
) -> Result<bool, String> {
    if token.is_empty() || token.len() > 128 {
        return Err("keyboard_capture_invalid_token".into());
    }
    #[cfg(windows)]
    {
        use tauri::Manager;
        let _ = state;
        // Hook installation waits for its message loop on a worker, never on the UI thread.
        tauri::async_runtime::spawn_blocking(move || {
            // hwnd() synchronously queries the UI thread. Resolve it before
            // taking the mutex also used by the UI focus-loss callback.
            let hwnd = window
                .hwnd()
                .map_err(|error| format!("keyboard_capture_window:{error}"))?
                .0 as isize;
            if enabled && !windows::is_foreground(hwnd) {
                return Ok(false);
            }
            let state = window.state::<SessionKeyboardCapture>();
            let mut windows = state
                .windows
                .lock()
                .map_err(|_| "keyboard_capture_state_poisoned")?;
            if enabled {
                // Main and external windows share one native input owner.
                for label in windows.keys() {
                    if let Some(previous) = window.app_handle().get_webview_window(label) {
                        let _ = windows::browser_accelerators(&previous, true);
                    }
                }
                windows.clear();
                windows::browser_accelerators(&window, false)?;
                let handle = match windows::start(&window, token, hwnd) {
                    Ok(handle) => handle,
                    Err(error) => {
                        let _ = windows::browser_accelerators(&window, true);
                        return Err(error);
                    }
                };
                windows.insert(window.label().to_owned(), handle);
            } else if windows
                .get(window.label())
                .is_some_and(|handle| handle.token == token)
            {
                windows.remove(window.label());
                windows::browser_accelerators(&window, true)?;
            }
            Ok(enabled)
        })
        .await
        .map_err(|_| "keyboard_capture_dispatch_failed".to_owned())?
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        let _ = state;
        tauri::async_runtime::spawn_blocking(move || {
            // Query the UI before acquiring the focus-loss callback's mutex.
            if enabled
                && !window
                    .is_focused()
                    .map_err(|error| format!("keyboard_capture_focus:{error}"))?
            {
                return Ok(false);
            }
            let state = window.state::<SessionKeyboardCapture>();
            let mut captures = state
                .macos
                .lock()
                .map_err(|_| "keyboard_capture_state_poisoned")?;
            if enabled {
                // A process-wide tap has one focused window owner at a time.
                captures.clear();
                let handle = macos::start(&window, token)?;
                captures.insert(window.label().to_owned(), handle);
            } else if captures
                .get(window.label())
                .is_some_and(|handle| handle.token == token)
            {
                captures.remove(window.label());
            }
            Ok(enabled)
        })
        .await
        .map_err(|_| "keyboard_capture_dispatch_failed".to_owned())?
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (window, state, enabled);
        Ok(false)
    }
}

#[cfg(windows)]
pub(crate) fn run_worker_if_requested() -> bool {
    windows::run_worker_if_requested()
}
