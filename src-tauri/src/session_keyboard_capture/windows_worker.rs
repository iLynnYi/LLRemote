//! Runs before Tauri/WebView initialization, in a child of the focused client.
//! The inherited pipes bind lifetime and events to that client; no listening port.
use super::{WorkerConfig, WorkerMessage};
use crate::session_keyboard_capture::{policy::CapturePolicy, NativeKey};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Read, Write},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyboardLayout, ToUnicodeEx},
        WindowsAndMessaging::*,
    },
};

struct HookContext {
    hwnd: isize,
    active: Arc<AtomicBool>,
    reason: Arc<AtomicU8>,
    sender: mpsc::SyncSender<NativeKey>,
    policy: CapturePolicy,
}
thread_local! { static CONTEXT: RefCell<Option<HookContext>> = const { RefCell::new(None) }; }

fn write_message(output: &mut impl Write, message: &WorkerMessage) -> std::io::Result<()> {
    serde_json::to_writer(&mut *output, message)?;
    output.write_all(b"\n")?;
    output.flush()
}

pub(super) fn run() -> Result<(), String> {
    let mut input = BufReader::new(std::io::stdin());
    let mut line = String::new();
    input
        .by_ref()
        .take(4097)
        .read_line(&mut line)
        .map_err(|_| "keyboard_capture_config_io")?;
    if line.len() > 4096 {
        return Err("keyboard_capture_config_size".into());
    }
    let config: WorkerConfig =
        serde_json::from_str(&line).map_err(|_| "keyboard_capture_config_invalid")?;
    let mut owner = 0;
    unsafe {
        GetWindowThreadProcessId(config.hwnd as _, &mut owner);
    }
    if owner != config.parent_pid || owner == 0 || !super::is_foreground(config.hwnd) {
        return Err("keyboard_capture_owner_changed".into());
    }
    let active = Arc::new(AtomicBool::new(true));
    let reason = Arc::new(AtomicU8::new(0));
    let (sender, receiver) = mpsc::sync_channel(128);
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        let thread_id = GetCurrentThreadId();
        CONTEXT.with(|context| {
            *context.borrow_mut() = Some(HookContext {
                hwnd: config.hwnd,
                active: active.clone(),
                reason: reason.clone(),
                sender,
                policy: CapturePolicy::with_modifiers(
                    (0xa0..=0xa5).filter(|key| GetAsyncKeyState(*key as i32) < 0),
                ),
            })
        });
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        if hook.is_null() {
            return Err("keyboard_capture_install_failed".into());
        }
        let input_active = active.clone();
        let input_thread = thread::Builder::new()
            .name("keyboard-owner-pipe".into())
            .spawn(move || {
                // EOF or an unexpected second request means this owner is gone.
                let mut extra = [0u8; 1];
                let _ = input.read(&mut extra);
                input_active.store(false, Ordering::Release);
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            });
        if input_thread.is_err() {
            UnhookWindowsHookEx(hook);
            return Err("keyboard_capture_owner_spawn".into());
        }
        let event_active = active.clone();
        let event_reason = reason.clone();
        let output_thread = thread::Builder::new()
            .name("keyboard-event-pipe".into())
            .spawn(move || {
                let mut output = std::io::stdout().lock();
                if write_message(&mut output, &WorkerMessage::Ready).is_err() {
                    event_active.store(false, Ordering::Release);
                }
                while event_active.load(Ordering::Acquire) {
                    match receiver.recv_timeout(Duration::from_millis(25)) {
                        Ok(key) => {
                            if event_active.load(Ordering::Acquire)
                                && write_message(&mut output, &WorkerMessage::Key(key)).is_err()
                            {
                                event_reason.store(4, Ordering::Release);
                                event_active.store(false, Ordering::Release);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            event_reason.store(4, Ordering::Release);
                            break;
                        }
                    }
                }
                let reason = match event_reason.load(Ordering::Acquire) {
                    1 => "focus",
                    2 => "overflow",
                    4 => "unavailable",
                    _ => "disabled",
                };
                let _ = write_message(&mut output, &WorkerMessage::Stopped(reason.into()));
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            });
        if output_thread.is_err() {
            active.store(false, Ordering::Release);
            UnhookWindowsHookEx(hook);
            return Err("keyboard_capture_events_spawn".into());
        }
        let result = loop {
            let result = GetMessageW(&mut message, std::ptr::null_mut(), 0, 0);
            if result <= 0 {
                break result;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        };
        if result < 0 || active.load(Ordering::Acquire) {
            reason.store(4, Ordering::Release);
        }
        active.store(false, Ordering::Release);
        UnhookWindowsHookEx(hook);
        CONTEXT.with(|context| *context.borrow_mut() = None);
        let _ = output_thread.unwrap().join();
    }
    Ok(())
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    }
    let captured = CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let Some(context) = context.as_mut() else {
            return false;
        };
        if !context.active.load(Ordering::Acquire) {
            return false;
        }
        if GetForegroundWindow() as isize != context.hwnd {
            context.reason.store(1, Ordering::Release);
            context.active.store(false, Ordering::Release);
            return false;
        }
        let input = &*(lparam as *const KBDLLHOOKSTRUCT);
        if crate::keyboard_input_source::is_own_input(input.dwExtraInfo as u64) {
            return false;
        }
        let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let vk = match input.vkCode {
            0x10 => {
                if input.scanCode == 0x36 {
                    0xa1
                } else {
                    0xa0
                }
            }
            0x11 => {
                if input.flags & LLKHF_EXTENDED != 0 {
                    0xa3
                } else {
                    0xa2
                }
            }
            0x12 => {
                if input.flags & LLKHF_EXTENDED != 0 {
                    0xa5
                } else {
                    0xa4
                }
            }
            key => key,
        };
        let capture = context.policy.capture(vk, down);
        let (control, alt, shift, meta) = context.policy.modifiers();
        if !capture {
            return false;
        }
        let key = native_key(input, down, control, alt, shift, meta);
        if context.sender.try_send(key).is_err() {
            // Never block the hook or silently drop a keyup. Release the entire
            // remote input state and fall back to local input on overload.
            context.reason.store(2, Ordering::Release);
            context.active.store(false, Ordering::Release);
        }
        true
    });
    if captured {
        1
    } else {
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }
}

unsafe fn native_key(
    input: &KBDLLHOOKSTRUCT,
    down: bool,
    control: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> NativeKey {
    let vk = input.vkCode;
    let extended = input.flags & LLKHF_EXTENDED != 0;
    let (key, code, location) = match vk {
        0x5b => ("Meta".into(), "MetaLeft".into(), 1),
        0x5c => ("Meta".into(), "MetaRight".into(), 2),
        0x08 => ("Backspace".into(), "Backspace".into(), 0),
        0x09 => ("Tab".into(), "Tab".into(), 0),
        0x0d => (
            "Enter".into(),
            if extended { "NumpadEnter" } else { "Enter" }.into(),
            if extended { 3 } else { 0 },
        ),
        0x1b => ("Escape".into(), "Escape".into(), 0),
        0x13 => ("Pause".into(), "Pause".into(), 0),
        0x14 => ("CapsLock".into(), "CapsLock".into(), 0),
        0x2c => ("PrintScreen".into(), "PrintScreen".into(), 0),
        0x5d => ("ContextMenu".into(), "ContextMenu".into(), 0),
        0x90 => ("NumLock".into(), "NumLock".into(), 0),
        0x91 => ("ScrollLock".into(), "ScrollLock".into(), 0),
        0x20 => (" ".into(), "Space".into(), 0),
        0x21..=0x28 => {
            let key = [
                "PageUp",
                "PageDown",
                "End",
                "Home",
                "ArrowLeft",
                "ArrowUp",
                "ArrowRight",
                "ArrowDown",
            ][(vk - 0x21) as usize];
            (key.into(), key.into(), 0)
        }
        0x2d => ("Insert".into(), "Insert".into(), 0),
        0x2e => ("Delete".into(), "Delete".into(), 0),
        0x70..=0x87 => {
            let key = format!("F{}", vk - 0x6f);
            (key.clone(), key, 0)
        }
        0x10 | 0xa0 | 0xa1 => {
            let right = vk == 0xa1 || input.scanCode == 0x36;
            (
                "Shift".into(),
                if right { "ShiftRight" } else { "ShiftLeft" }.into(),
                if right { 2 } else { 1 },
            )
        }
        0x11 | 0xa2 | 0xa3 => {
            let right = vk == 0xa3 || extended;
            (
                "Control".into(),
                if right { "ControlRight" } else { "ControlLeft" }.into(),
                if right { 2 } else { 1 },
            )
        }
        0x12 | 0xa4 | 0xa5 => {
            let right = vk == 0xa5 || extended;
            (
                "Alt".into(),
                if right { "AltRight" } else { "AltLeft" }.into(),
                if right { 2 } else { 1 },
            )
        }
        _ => {
            // Translate with the foreground layout without modifying its dead-key
            // state. Ctrl/Alt/Win are omitted to avoid control-character output.
            let mut state = [0u8; 256];
            if shift {
                state[0x10] = 0x80;
            }
            state[0x14] =
                (windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x14) & 1) as u8;
            let mut chars = [0u16; 8];
            let layout_thread =
                GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
            let count = ToUnicodeEx(
                vk,
                input.scanCode,
                state.as_ptr(),
                chars.as_mut_ptr(),
                chars.len() as i32,
                4,
                GetKeyboardLayout(layout_thread),
            );
            let key = if count > 0 {
                String::from_utf16_lossy(&chars[..(count as usize).min(chars.len())])
            } else {
                "Unidentified".into()
            };
            let code = match vk {
                0x41..=0x5a => format!("Key{}", char::from_u32(vk).unwrap_or(' ')),
                0x30..=0x39 => format!("Digit{}", vk - 0x30),
                _ => key.clone(),
            };
            (key, code, 0)
        }
    };
    NativeKey {
        key,
        code,
        location,
        down,
        ctrl_key: control,
        alt_key: alt,
        shift_key: shift,
        meta_key: meta,
    }
}
