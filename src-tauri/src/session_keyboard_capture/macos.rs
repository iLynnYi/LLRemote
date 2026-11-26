//! Session-scoped event tap. Ordinary typing remains in WKWebView so local IME
//! composition still works. Modifier shortcuts are delivered before AppKit or
//! the system can consume them. The tap only copies/enqueues events; AppKit
//! translation is dispatched asynchronously to the main thread.
use super::{macos_policy::CapturePolicy, Message, NativeKey};
use std::{
    ffi::{c_char, c_void, CStr},
    ptr,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};
use tauri::Emitter;

type CFRef = *const c_void;
type Event = *mut c_void;
type TapCallback = unsafe extern "C" fn(*mut c_void, u32, Event, *mut c_void) -> Event;

#[repr(C)]
struct ProcessSerialNumber {
    high: u32,
    low: u32,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFRef;
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        mask: u64,
        callback: TapCallback,
        context: *mut c_void,
    ) -> CFRef;
    fn CGEventTapEnable(tap: CFRef, enabled: bool);
    fn CGEventGetIntegerValueField(event: Event, field: u32) -> i64;
    fn CGEventGetFlags(event: Event) -> u64;
    fn CGEventCreateCopy(event: Event) -> Event;
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn GetFrontProcess(process: *mut ProcessSerialNumber) -> i16;
    fn GetProcessPID(process: *const ProcessSerialNumber, pid: *mut i32) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: CFRef;
    static kCFBooleanTrue: CFRef;
    fn CFDictionaryCreate(
        allocator: CFRef,
        keys: *const CFRef,
        values: *const CFRef,
        count: isize,
        key_callbacks: CFRef,
        value_callbacks: CFRef,
    ) -> CFRef;
    fn CFRelease(value: CFRef);
    fn CFMachPortCreateRunLoopSource(allocator: CFRef, port: CFRef, order: isize) -> CFRef;
    fn CFMachPortInvalidate(port: CFRef);
    fn CFRunLoopGetCurrent() -> CFRef;
    fn CFRunLoopAddSource(run_loop: CFRef, source: CFRef, mode: CFRef);
    fn CFRunLoopRemoveSource(run_loop: CFRef, source: CFRef, mode: CFRef);
    fn CFRunLoopRunInMode(mode: CFRef, seconds: f64, return_after_source: bool) -> i32;
}
#[link(name = "AppKit", kind = "framework")]
extern "C" {}
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend();
}

fn request_permission_once() {
    static REQUESTED: AtomicBool = AtomicBool::new(false);
    if REQUESTED.swap(true, Ordering::AcqRel) {
        return;
    }
    // Request only when the user enters an interactive remote session. The OS
    // remains responsible for consent; focus return retries the installation.
    unsafe {
        let options = CFDictionaryCreate(
            ptr::null(),
            &kAXTrustedCheckOptionPrompt,
            &kCFBooleanTrue,
            1,
            ptr::null(),
            ptr::null(),
        );
        if !options.is_null() {
            AXIsProcessTrustedWithOptions(options);
            CFRelease(options);
        }
    }
}

pub(super) struct CaptureHandle {
    pub token: String,
    active: Arc<AtomicBool>,
}
impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

struct HookContext {
    active: Arc<AtomicBool>,
    reason: Arc<AtomicU8>,
    sender: mpsc::SyncSender<CapturedKey>,
    policy: CapturePolicy,
}

/// An independent immutable event copy, owned across the capture/UI boundary.
struct EventCopy(std::ptr::NonNull<c_void>);
// SAFETY: CGEventCreateCopy returns an independently retained Quartz event. The
// copy is transferred to exactly one consumer and never accessed concurrently.
unsafe impl Send for EventCopy {}
impl Drop for EventCopy {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0.as_ptr());
        }
    }
}

struct CapturedKey {
    event: EventCopy,
    key: u16,
    flags: u64,
    down: bool,
}
impl CapturedKey {
    unsafe fn copy(event: Event, key: u16, flags: u64, down: bool) -> Option<Self> {
        Some(Self {
            event: EventCopy(std::ptr::NonNull::new(CGEventCreateCopy(event))?),
            key,
            flags,
            down,
        })
    }
    unsafe fn translate(self) -> Result<NativeKey, &'static str> {
        native_key_on_main(self.event.0.as_ptr(), self.key, self.flags, self.down)
    }
}

/// One UI task in flight preserves modifier/key ordering and keeps the bounded
/// native queue effective while the UI is busy. Only the worker waits for ACK;
/// neither the event tap nor the main thread waits on the other thread.
fn deliver_on_main(
    window: &tauri::WebviewWindow,
    key: CapturedKey,
    token: &str,
    active: &Arc<AtomicBool>,
    reason: &Arc<AtomicU8>,
) -> Result<(), ()> {
    let event_window = window.clone();
    let event_active = active.clone();
    let event_reason = reason.clone();
    let token = token.to_owned();
    let (done, acknowledgement) = mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            if event_active.load(Ordering::Acquire) {
                let result = unsafe { key.translate() }.map_err(|_| ()).and_then(|key| {
                    event_window
                        .emit(
                            "session-native-keyboard",
                            Message {
                                token,
                                key: Some(key),
                                reason: None,
                            },
                        )
                        .map_err(|_| ())
                });
                if result.is_err() {
                    let _ =
                        event_reason.compare_exchange(0, 4, Ordering::AcqRel, Ordering::Acquire);
                    event_active.store(false, Ordering::Release);
                }
            }
            let _ = done.send(());
        })
        .map_err(|_| ())?;
    while active.load(Ordering::Acquire) {
        match acknowledgement.recv_timeout(Duration::from_millis(25)) {
            Ok(()) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(()),
        }
    }
    Ok(())
}

pub(super) fn start(window: &tauri::WebviewWindow, token: String) -> Result<CaptureHandle, String> {
    if !unsafe { AXIsProcessTrusted() } {
        request_permission_once();
        if !unsafe { AXIsProcessTrusted() } {
            return Err("keyboard_capture_permission_required".into());
        }
    }
    let active = Arc::new(AtomicBool::new(true));
    let reason = Arc::new(AtomicU8::new(0));
    let (sender, receiver) = mpsc::sync_channel(128);
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let hook_active = active.clone();
    let hook_reason = reason.clone();
    thread::Builder::new()
        .name("session-keyboard-tap".into())
        .spawn(move || unsafe {
            let mut context = Box::new(HookContext {
                active: hook_active.clone(),
                reason: hook_reason,
                sender,
                policy: CapturePolicy::with_modifiers(
                    (54..=62).filter(|key| CGEventSourceKeyState(1, *key)),
                ),
            });
            // Session-level head insertion precedes application keyboard equivalents.
            let tap = CGEventTapCreate(
                1,
                0,
                0,
                (1 << 10) | (1 << 11) | (1 << 12),
                callback,
                (&mut *context as *mut HookContext).cast(),
            );
            if tap.is_null() {
                hook_active.store(false, Ordering::Release);
                let _ = ready_sender.send(Err("keyboard_capture_install_failed".to_owned()));
                return;
            }
            let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
            if source.is_null() {
                CFMachPortInvalidate(tap);
                CFRelease(tap);
                hook_active.store(false, Ordering::Release);
                let _ = ready_sender.send(Err("keyboard_capture_runloop_failed".to_owned()));
                return;
            }
            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopDefaultMode);
            CGEventTapEnable(tap, true);
            let _ = ready_sender.send(Ok(()));
            while hook_active.load(Ordering::Acquire) {
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.025, false);
            }
            CGEventTapEnable(tap, false);
            CFRunLoopRemoveSource(run_loop, source, kCFRunLoopDefaultMode);
            CFMachPortInvalidate(tap);
            CFRelease(source);
            CFRelease(tap);
        })
        .map_err(|error| format!("keyboard_capture_spawn:{error}"))?;
    let handle = CaptureHandle {
        token: token.clone(),
        active: active.clone(),
    };
    ready_receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|_| "keyboard_capture_start_timeout")??;
    let event_window = window.clone();
    thread::Builder::new()
        .name("session-keyboard-events".into())
        .spawn(move || {
            while active.load(Ordering::Acquire) {
                match receiver.recv_timeout(Duration::from_millis(25)) {
                    Ok(key) => {
                        if !active.load(Ordering::Acquire) {
                            break;
                        }
                        if deliver_on_main(&event_window, key, &token, &active, &reason).is_err() {
                            let _ =
                                reason.compare_exchange(0, 4, Ordering::AcqRel, Ordering::Acquire);
                            active.store(false, Ordering::Release);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            let reason = match reason.load(Ordering::Acquire) {
                1 => "focus",
                2 => "overflow",
                4 => "unavailable",
                _ => "disabled",
            };
            let _ = event_window.emit(
                "session-native-keyboard",
                Message {
                    token,
                    key: None,
                    reason: Some(reason),
                },
            );
        })
        .map_err(|error| format!("keyboard_capture_events_spawn:{error}"))?;
    Ok(handle)
}

unsafe extern "C" fn callback(_: *mut c_void, kind: u32, event: Event, data: *mut c_void) -> Event {
    let context = &mut *data.cast::<HookContext>();
    if !context.active.load(Ordering::Acquire) {
        return event;
    }
    if kind == u32::MAX || kind == u32::MAX - 1 {
        // A disabled tap must release remote modifiers, not leave stuck keys.
        context.reason.store(4, Ordering::Release);
        context.active.store(false, Ordering::Release);
        return event;
    }
    let mut process = ProcessSerialNumber { high: 0, low: 0 };
    let mut pid = 0;
    if GetFrontProcess(&mut process) != 0
        || GetProcessPID(&process, &mut pid) != 0
        || pid as u32 != std::process::id()
    {
        context.reason.store(1, Ordering::Release);
        context.active.store(false, Ordering::Release);
        return event;
    }
    if event.is_null() || !matches!(kind, 10..=12) {
        return event;
    }
    // Other remote-control clients may post keyboard events too. Only our own
    // Server injection bypasses this Client tap to avoid same-machine loops.
    if crate::keyboard_input_source::is_own_input(CGEventGetIntegerValueField(event, 42) as u64) {
        return event;
    }
    let key = CGEventGetIntegerValueField(event, 9) as u16;
    let flags = CGEventGetFlags(event);
    let down = if kind == 12 {
        context.policy.modifier_down(key, flags)
    } else {
        kind == 10
    };
    if !context.policy.capture(key, down) {
        return event;
    }
    let Some(key) = CapturedKey::copy(event, key, flags, down) else {
        context.reason.store(4, Ordering::Release);
        context.active.store(false, Ordering::Release);
        return event;
    };
    if context.sender.try_send(key).is_err() {
        context.reason.store(2, Ordering::Release);
        context.active.store(false, Ordering::Release);
    }
    ptr::null_mut()
}

unsafe fn native_key_on_main(
    event: Event,
    key: u16,
    flags: u64,
    down: bool,
) -> Result<NativeKey, &'static str> {
    if libc::pthread_main_np() == 0 {
        return Err("keyboard_capture_translation_requires_main_thread");
    }
    let (code, special, location) = key_definition(key);
    let text = special
        .map(str::to_owned)
        .unwrap_or_else(|| characters(event));
    Ok(NativeKey {
        key: if text.is_empty() {
            "Unidentified".into()
        } else {
            text
        },
        code: code.into(),
        location,
        down,
        ctrl_key: flags & (1 << 18) != 0,
        alt_key: flags & (1 << 19) != 0,
        shift_key: flags & (1 << 17) != 0,
        meta_key: flags & (1 << 20) != 0,
    })
}

/// NSEvent preserves the active keyboard layout and Shift, while removing
/// Command/Control/Option. Remote modifier flags carry those modifiers separately.
unsafe fn characters(event: Event) -> String {
    type Send = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
    type SendEvent = unsafe extern "C" fn(*mut c_void, *mut c_void, Event) -> *mut c_void;
    let send: Send = std::mem::transmute(objc_msgSend as *const ());
    let send_event: SendEvent = std::mem::transmute(objc_msgSend as *const ());
    let pool = send(
        objc_getClass(c"NSAutoreleasePool".as_ptr()),
        sel_registerName(c"new".as_ptr()),
    );
    let native = send_event(
        objc_getClass(c"NSEvent".as_ptr()),
        sel_registerName(c"eventWithCGEvent:".as_ptr()),
        event,
    );
    let string = send(
        native,
        sel_registerName(c"charactersIgnoringModifiers".as_ptr()),
    );
    let utf8 = send(string, sel_registerName(c"UTF8String".as_ptr())).cast::<c_char>();
    let result = if utf8.is_null() {
        String::new()
    } else {
        CStr::from_ptr(utf8).to_string_lossy().into_owned()
    };
    // drain returns void, so use the correct objc_msgSend signature.
    let drain: unsafe extern "C" fn(*mut c_void, *mut c_void) =
        std::mem::transmute(objc_msgSend as *const ());
    drain(pool, sel_registerName(c"drain".as_ptr()));
    result
}

fn key_definition(key: u16) -> (&'static str, Option<&'static str>, u32) {
    let (code, special, location) = match key {
        36 => ("Enter", Some("Enter"), 0),
        48 => ("Tab", Some("Tab"), 0),
        49 => ("Space", Some(" "), 0),
        51 => ("Backspace", Some("Backspace"), 0),
        53 => ("Escape", Some("Escape"), 0),
        54 => ("MetaRight", Some("Meta"), 2),
        55 => ("MetaLeft", Some("Meta"), 1),
        56 => ("ShiftLeft", Some("Shift"), 1),
        60 => ("ShiftRight", Some("Shift"), 2),
        58 => ("AltLeft", Some("Alt"), 1),
        61 => ("AltRight", Some("Alt"), 2),
        59 => ("ControlLeft", Some("Control"), 1),
        62 => ("ControlRight", Some("Control"), 2),
        57 => ("CapsLock", Some("CapsLock"), 0),
        63 => ("Fn", Some("Fn"), 0),
        123 => ("ArrowLeft", Some("ArrowLeft"), 0),
        124 => ("ArrowRight", Some("ArrowRight"), 0),
        125 => ("ArrowDown", Some("ArrowDown"), 0),
        126 => ("ArrowUp", Some("ArrowUp"), 0),
        115 => ("Home", Some("Home"), 0),
        119 => ("End", Some("End"), 0),
        116 => ("PageUp", Some("PageUp"), 0),
        121 => ("PageDown", Some("PageDown"), 0),
        117 => ("Delete", Some("Delete"), 0),
        114 => ("Insert", Some("Insert"), 0),
        122 => ("F1", Some("F1"), 0),
        120 => ("F2", Some("F2"), 0),
        99 => ("F3", Some("F3"), 0),
        118 => ("F4", Some("F4"), 0),
        96 => ("F5", Some("F5"), 0),
        97 => ("F6", Some("F6"), 0),
        98 => ("F7", Some("F7"), 0),
        100 => ("F8", Some("F8"), 0),
        101 => ("F9", Some("F9"), 0),
        109 => ("F10", Some("F10"), 0),
        103 => ("F11", Some("F11"), 0),
        111 => ("F12", Some("F12"), 0),
        105 => ("F13", Some("F13"), 0),
        107 => ("F14", Some("F14"), 0),
        113 => ("F15", Some("F15"), 0),
        106 => ("F16", Some("F16"), 0),
        64 => ("F17", Some("F17"), 0),
        79 => ("F18", Some("F18"), 0),
        80 => ("F19", Some("F19"), 0),
        90 => ("F20", Some("F20"), 0),
        76 => ("NumpadEnter", Some("Enter"), 3),
        71 => ("NumLock", Some("Clear"), 3),
        _ => (
            printable_code(key),
            None,
            if matches!(key, 65 | 67 | 69 | 75 | 78 | 81..=89 | 91 | 92) {
                3
            } else {
                0
            },
        ),
    };
    (code, special, location)
}

fn printable_code(key: u16) -> &'static str {
    match key {
        0 => "KeyA",
        1 => "KeyS",
        2 => "KeyD",
        3 => "KeyF",
        4 => "KeyH",
        5 => "KeyG",
        6 => "KeyZ",
        7 => "KeyX",
        8 => "KeyC",
        9 => "KeyV",
        11 => "KeyB",
        12 => "KeyQ",
        13 => "KeyW",
        14 => "KeyE",
        15 => "KeyR",
        16 => "KeyY",
        17 => "KeyT",
        31 => "KeyO",
        32 => "KeyU",
        34 => "KeyI",
        35 => "KeyP",
        37 => "KeyL",
        38 => "KeyJ",
        40 => "KeyK",
        45 => "KeyN",
        46 => "KeyM",
        18 => "Digit1",
        19 => "Digit2",
        20 => "Digit3",
        21 => "Digit4",
        22 => "Digit6",
        23 => "Digit5",
        25 => "Digit9",
        26 => "Digit7",
        28 => "Digit8",
        29 => "Digit0",
        24 => "Equal",
        27 => "Minus",
        30 => "BracketRight",
        33 => "BracketLeft",
        39 => "Quote",
        41 => "Semicolon",
        42 => "Backslash",
        43 => "Comma",
        44 => "Slash",
        47 => "Period",
        50 => "Backquote",
        10 => "IntlBackslash",
        93 => "IntlYen",
        94 => "IntlRo",
        95 => "NumpadComma",
        65 => "NumpadDecimal",
        67 => "NumpadMultiply",
        69 => "NumpadAdd",
        75 => "NumpadDivide",
        78 => "NumpadSubtract",
        81 => "NumpadEqual",
        82 => "Numpad0",
        83 => "Numpad1",
        84 => "Numpad2",
        85 => "Numpad3",
        86 => "Numpad4",
        87 => "Numpad5",
        88 => "Numpad6",
        89 => "Numpad7",
        91 => "Numpad8",
        92 => "Numpad9",
        _ => "Unidentified",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_worker_cannot_enter_appkit_translation() {
        let result = thread::spawn(|| unsafe {
            // Meta uses no event payload. The translator must reject this call
            // before any AppKit access, for printable and modifier keys alike.
            native_key_on_main(ptr::null_mut(), 55, 1 << 20, true)
        })
        .join()
        .unwrap();
        assert!(matches!(
            result,
            Err("keyboard_capture_translation_requires_main_thread")
        ));
    }

    #[test]
    fn maps_mac_keycodes_without_losing_modifier_sides_or_numpad() {
        assert_eq!(key_definition(54), ("MetaRight", Some("Meta"), 2));
        assert_eq!(key_definition(59), ("ControlLeft", Some("Control"), 1));
        assert_eq!(key_definition(111), ("F12", Some("F12"), 0));
        assert_eq!(key_definition(76), ("NumpadEnter", Some("Enter"), 3));
        assert_eq!(key_definition(82), ("Numpad0", None, 3));
        assert_eq!(key_definition(0), ("KeyA", None, 0));
    }
}
