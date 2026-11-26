use super::{inbox::MessageInbox, layout::popup_limits, MessageHistory, MAX_HISTORY_BYTES};
use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Controls::{
    ShowScrollBar, DRAWITEMSTRUCT, EM_REPLACESEL, EM_SCROLLCARET, EM_SETSEL, NMHDR, ODS_FOCUS,
    ODS_SELECTED,
};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetActiveWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static RECEIVER: OnceLock<Result<Receiver, String>> = OnceLock::new();
static REGISTERED: OnceLock<bool> = OnceLock::new();
static RICH_EDIT_READY: OnceLock<bool> = OnceLock::new();
const APPEND_MESSAGE: u32 = WM_APP + 1;
const COPY_ID: usize = 100;
const CLOSE_ID: usize = 2;

struct Receiver {
    dispatcher: usize,
    inbox: Arc<Mutex<MessageInbox>>,
}

struct ReceiverState {
    inbox: Arc<Mutex<MessageInbox>>,
    window: Option<MessageWindow>,
}

struct MessageWindow {
    hwnd: HWND,
    data: Box<WindowData>,
}

// Match the app's light theme and #1f8a70 primary color without needing a WebView.
const PRIMARY: u32 = rgb(31, 138, 112);
const PRIMARY_PRESSED: u32 = rgb(23, 111, 91);
const TEXT: u32 = rgb(31, 41, 51);
const MUTED: u32 = rgb(112, 128, 144);
const CARD: u32 = rgb(246, 248, 250);
const BORDER: u32 = rgb(215, 222, 231);
const WHITE: u32 = rgb(255, 255, 255);
const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

struct GdiObject(HGDIOBJ);
impl Drop for GdiObject {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                DeleteObject(self.0);
            }
        }
    }
}

struct WindowData {
    history: RefCell<MessageHistory>,
    inbox: Arc<Mutex<MessageInbox>>,
    clipboard_owner: HWND,
    copy_button: HWND,
    close_button: HWND,
    body: HWND,
    card_rect: Cell<RECT>,
    measured_height: Cell<i32>,
    client_width: Cell<i32>,
    reference_bounds: RefCell<Option<(Instant, Option<(i32, i32)>, RECT)>>,
    scale: f64,
    font: GdiObject,
    title_font: GdiObject,
    caption_font: GdiObject,
    button_font: GdiObject,
    white_brush: GdiObject,
    card_brush: GdiObject,
}

impl WindowData {
    fn px(&self, value: i32) -> i32 {
        (value as f64 * self.scale).round() as i32
    }
}

unsafe fn make_font(size: i32, weight: i32, scale: f64) -> GdiObject {
    GdiObject(CreateFontW(
        -(size as f64 * scale).round() as i32,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide("Segoe UI").as_ptr(),
    ))
}

unsafe fn rounded_rect(dc: HDC, rect: &RECT, fill: u32, border: u32, radius: i32) {
    let brush = GdiObject(CreateSolidBrush(fill));
    let pen = GdiObject(CreatePen(PS_SOLID, 1, border));
    let old_brush = SelectObject(dc, brush.0);
    let old_pen = SelectObject(dc, pen.0);
    RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius * 2,
        radius * 2,
    );
    SelectObject(dc, old_brush);
    SelectObject(dc, old_pen);
}

unsafe fn draw_button(item: &DRAWITEMSTRUCT, data: &WindowData) {
    let primary = item.CtlID as usize == CLOSE_ID;
    let pressed = item.itemState & ODS_SELECTED != 0;
    let focused = item.itemState & ODS_FOCUS != 0;
    FillRect(item.hDC, &item.rcItem, data.white_brush.0);
    let fill = if primary {
        if pressed {
            PRIMARY_PRESSED
        } else {
            PRIMARY
        }
    } else if pressed {
        CARD
    } else {
        WHITE
    };
    let border = if primary || focused {
        PRIMARY_PRESSED
    } else {
        BORDER
    };
    let mut rect = item.rcItem;
    rect.left += data.px(1);
    rect.top += data.px(1);
    rect.right -= data.px(1);
    rect.bottom -= data.px(1);
    rounded_rect(item.hDC, &rect, fill, border, data.px(6));
    if focused {
        let mut ring = rect;
        ring.left += data.px(3);
        ring.top += data.px(3);
        ring.right -= data.px(3);
        ring.bottom -= data.px(3);
        let pen = GdiObject(CreatePen(PS_DOT, 1, if primary { WHITE } else { PRIMARY }));
        let old_pen = SelectObject(item.hDC, pen.0);
        let old_brush = SelectObject(item.hDC, GetStockObject(NULL_BRUSH));
        RoundRect(
            item.hDC,
            ring.left,
            ring.top,
            ring.right,
            ring.bottom,
            data.px(6),
            data.px(6),
        );
        SelectObject(item.hDC, old_brush);
        SelectObject(item.hDC, old_pen);
    }
    let mut title = [0u16; 32];
    let length = GetWindowTextW(item.hwndItem, title.as_mut_ptr(), title.len() as i32);
    SetBkMode(item.hDC, TRANSPARENT as i32);
    SetTextColor(item.hDC, if primary { WHITE } else { TEXT });
    let old_font = SelectObject(item.hDC, data.button_font.0);
    DrawTextW(
        item.hDC,
        title.as_ptr(),
        length,
        &mut rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(item.hDC, old_font);
}

// Win32 richedit.h's CHARFORMATW/CHARRANGE and messages. RichEdit is not
// exposed by windows-sys; keep the small C ABI surface next to its one caller.
#[repr(C)]
#[derive(Default)]
struct CharacterFormat {
    size: u32,
    mask: u32,
    effects: u32,
    height: i32,
    offset: i32,
    color: u32,
    charset: u8,
    pitch: u8,
    face: [u16; 32],
}
#[repr(C)]
struct CharacterRange {
    start: i32,
    end: i32,
}
const _: () = assert!(std::mem::size_of::<CharacterFormat>() == 92);
const EM_SETCHARFORMAT: u32 = WM_USER + 68;
const EM_SETBKGNDCOLOR: u32 = WM_USER + 67;
const EM_EXSETSEL: u32 = WM_USER + 55;
const EM_EXLIMITTEXT: u32 = WM_USER + 53;
const CFM_COLOR: u32 = 0x4000_0000;
const CFM_SIZE: u32 = 0x8000_0000;
const EM_REQUESTRESIZE: u32 = WM_USER + 65;
const EM_SETEVENTMASK: u32 = WM_USER + 69;
const ENM_REQUESTRESIZE: isize = 0x40000;
const EN_REQUESTRESIZE: u32 = 0x0701;
#[repr(C, packed(4))]
struct ResizeRequest {
    header: NMHDR,
    rect: RECT,
}
const SCF_SELECTION: usize = 1;

unsafe fn append_transcript(data: &WindowData, entries: &[String]) {
    SendMessageW(data.body, WM_SETREDRAW, 0, 0);
    for entry in entries {
        let appended = data
            .history
            .borrow_mut()
            .append(entry)
            .expect("the inbox reserves capacity before accepting messages");
        let range = CharacterRange {
            start: appended.start,
            end: appended.start,
        };
        SendMessageW(
            data.body,
            EM_EXSETSEL,
            0,
            (&range as *const CharacterRange) as isize,
        );
        let mut format = CharacterFormat {
            size: std::mem::size_of::<CharacterFormat>() as u32,
            mask: CFM_COLOR | CFM_SIZE,
            height: 14 * 15,
            color: TEXT,
            ..Default::default()
        };
        SendMessageW(
            data.body,
            EM_SETCHARFORMAT,
            SCF_SELECTION,
            (&format as *const CharacterFormat) as isize,
        );
        // RichEdit paragraph breaks occupy one UTF-16 unit, matching normalized LF.
        SendMessageW(
            data.body,
            EM_REPLACESEL,
            0,
            wide(&appended.text.replace('\n', "\r")).as_ptr() as isize,
        );
        let range = CharacterRange {
            start: appended.time_range.0,
            end: appended.time_range.1,
        };
        SendMessageW(
            data.body,
            EM_EXSETSEL,
            0,
            (&range as *const CharacterRange) as isize,
        );
        format.height = 11 * 15;
        format.color = rgb(138, 151, 163);
        SendMessageW(
            data.body,
            EM_SETCHARFORMAT,
            SCF_SELECTION,
            (&format as *const CharacterFormat) as isize,
        );
        SendMessageW(
            data.body,
            EM_SETSEL,
            appended.end as usize,
            appended.end as isize,
        );
    }
    SetWindowTextW(
        data.copy_button,
        wide(if data.history.borrow().time_ranges.len() > 1 {
            "复制全部"
        } else {
            "复制"
        })
        .as_ptr(),
    );
    SendMessageW(data.body, WM_SETREDRAW, 1, 0);
    InvalidateRect(data.body, null(), 1);
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn show(entry: &str) -> Result<(), String> {
    let receiver = RECEIVER
        .get_or_init(|| {
            let inbox = Arc::new(Mutex::new(MessageInbox::default()));
            let worker_inbox = Arc::clone(&inbox);
            let (ready, receive) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("remote-message".into())
                .spawn(move || unsafe { run_receiver(worker_inbox, ready) })
                .map_err(|_| "remote_message_display_failed".to_string())?;
            let dispatcher = receive
                .recv_timeout(Duration::from_secs(3))
                .map_err(|_| "remote_message_display_failed".to_string())?;
            if dispatcher == 0 {
                return Err("remote_message_display_failed".into());
            }
            Ok(Receiver { dispatcher, inbox })
        })
        .as_ref()
        .map_err(Clone::clone)?;
    receiver
        .inbox
        .lock()
        .map_err(|_| "remote_message_display_failed".to_string())?
        .enqueue(entry, || {
            if unsafe { PostMessageW(receiver.dispatcher as HWND, APPEND_MESSAGE, 0, 0) } == 0 {
                Err("remote_message_display_failed".into())
            } else {
                Ok(())
            }
        })
}

unsafe fn run_receiver(inbox: Arc<Mutex<MessageInbox>>, ready: mpsc::SyncSender<usize>) {
    // A message-only HWND survives popup close/reopen and receives wakeups even
    // inside Windows' modal move/resize loops, which discard thread messages.
    let instance = GetModuleHandleW(null());
    let class = wide("LLRemoteMessageReceiver");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(receiver_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&wc) == 0 {
        let _ = ready.send(0);
        return;
    }
    let dispatcher = CreateWindowExW(
        0,
        class.as_ptr(),
        wide("").as_ptr(),
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        null_mut(),
        instance,
        null(),
    );
    if dispatcher.is_null() {
        let _ = ready.send(0);
        return;
    }
    let mut state = Box::new(ReceiverState {
        inbox,
        window: None,
    });
    // windows-sys uses i32 on x86 and isize on x64 and ARM64.
    SetWindowLongPtrW(
        dispatcher,
        GWLP_USERDATA,
        std::ptr::from_mut(state.as_mut()) as _,
    );
    if ready.send(dispatcher as usize).is_err() {
        DestroyWindow(dispatcher);
        return;
    }
    let mut message: MSG = std::mem::zeroed();
    while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
        let popup = state
            .window
            .as_ref()
            .map(|window| window.hwnd)
            .unwrap_or(null_mut());
        if popup.is_null() || IsWindow(popup) == 0 || IsDialogMessageW(popup, &message) == 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if state
            .window
            .as_ref()
            .is_some_and(|window| IsWindow(window.hwnd) == 0)
        {
            state.window = None;
        }
    }
    if let Some(window) = &state.window {
        if IsWindow(window.hwnd) != 0 {
            DestroyWindow(window.hwnd);
        }
    }
    DestroyWindow(dispatcher);
}

unsafe extern "system" fn receiver_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ReceiverState;
    if state.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if message == APPEND_MESSAGE {
        if SetTimer(hwnd, 1, 16, None) != 0 {
            return 0;
        }
    } else if message == WM_TIMER && wparam == 1 {
        KillTimer(hwnd, 1);
    } else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let state = &mut *state;
    if state
        .window
        .as_ref()
        .is_some_and(|window| IsWindow(window.hwnd) == 0)
    {
        state.window = None;
    }
    let entries = state.inbox.lock().unwrap().take_pending();
    if entries.is_empty() {
        return 0;
    }
    if let Some(window) = &state.window {
        append_transcript(&window.data, &entries);
        resize_to_contents(window.hwnd, &window.data, false);
        SendMessageW(window.data.body, EM_SCROLLCARET, 0, 0);
        focus_message_window(window.hwnd, &window.data);
    } else {
        match create_window(&entries, Arc::clone(&state.inbox), hwnd) {
            Ok(created) => state.window = Some(created),
            Err(_) => {
                state
                    .inbox
                    .lock()
                    .unwrap()
                    .restore_after_window_failure(entries);
                SetTimer(hwnd, 1, 1000, None);
            }
        }
    }
    0
}

unsafe fn focus_message_window(hwnd: HWND, data: &WindowData) {
    ShowWindow(
        hwnd,
        if IsIconic(hwnd) != 0 {
            SW_RESTORE
        } else {
            SW_SHOW
        },
    );
    SetWindowPos(
        hwnd,
        HWND_TOP,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
    );
    SetForegroundWindow(hwnd);
    if GetForegroundWindow() == hwnd {
        SetActiveWindow(hwnd);
        SetFocus(data.body);
    } else {
        // Windows can deny background activation. Request attention without
        // attaching unrelated input queues or leaving the window always on top.
        let flash = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd,
            dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
            uCount: 3,
            dwTimeout: 0,
        };
        FlashWindowEx(&flash);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let data = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowData).as_ref();
    match message {
        WM_NOTIFY if data.is_some() && lparam != 0 => {
            let data = data.unwrap();
            let header = std::ptr::read_unaligned(lparam as *const NMHDR);
            if header.hwndFrom == data.body && header.code == EN_REQUESTRESIZE {
                let requested = std::ptr::read_unaligned(lparam as *const ResizeRequest);
                data.measured_height
                    .set((requested.rect.bottom - requested.rect.top).max(1));
            }
            0
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT if data.is_some() => {
            let data = data.unwrap();
            let dc = wparam as HDC;
            let body = lparam as HWND == data.body;
            SetBkMode(dc, TRANSPARENT as i32);
            SetBkColor(dc, WHITE);
            SetTextColor(dc, if body { TEXT } else { MUTED });
            // Heading uses the app's normal text color; caption uses muted gray.
            if GetDlgCtrlID(lparam as HWND) == 102 {
                SetTextColor(dc, TEXT);
            }
            (if body {
                data.card_brush.0
            } else {
                data.white_brush.0
            }) as LRESULT
        }
        WM_DRAWITEM if data.is_some() => {
            let item = &*(lparam as *const DRAWITEMSTRUCT);
            if item.CtlID as usize == COPY_ID || item.CtlID as usize == CLOSE_ID {
                draw_button(item, data.unwrap());
                return 1;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_PAINT if data.is_some() => {
            let data = data.unwrap();
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            rounded_rect(dc, &data.card_rect.get(), WHITE, BORDER, data.px(8));
            let dot = RECT {
                left: data.px(20),
                top: data.px(24),
                right: data.px(26),
                bottom: data.px(30),
            };
            rounded_rect(dc, &dot, PRIMARY, PRIMARY, data.px(3));
            EndPaint(hwnd, &paint);
            0
        }
        WM_COMMAND => match wparam & 0xffff {
            COPY_ID => {
                let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowData;
                if let Some(data) = data.as_ref() {
                    let copied = copy_message(data.clipboard_owner, &data.history.borrow().text);
                    SetWindowTextW(
                        data.copy_button,
                        wide(if copied { "已复制" } else { "重试复制" }).as_ptr(),
                    );
                }
                0
            }
            1 | CLOSE_ID => {
                DestroyWindow(hwnd);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        },
        WM_DESTROY => {
            if let Some(data) = data {
                if let Ok(mut inbox) = data.inbox.lock() {
                    inbox.window_closed();
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn copy_message(owner: HWND, text: &str) -> bool {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    // Use the persistent receiver HWND, not the dismissible popup. Publish real
    // CF_UNICODETEXT memory immediately; no later paste needs the popup's data.
    if OpenClipboard(owner) == 0 {
        return false;
    }
    let copied = (|| {
        let text = wide(text);
        let handle = GlobalAlloc(GMEM_MOVEABLE, text.len() * std::mem::size_of::<u16>());
        if handle.is_null() {
            return false;
        }
        let target = GlobalLock(handle) as *mut u16;
        if target.is_null() {
            GlobalFree(handle);
            return false;
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), target, text.len());
        GlobalUnlock(handle);
        if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
            GlobalFree(handle);
            return false;
        }
        true // The system now owns the global allocation.
    })();
    // Finish the entire clipboard transaction before the button reports success.
    // The system owns the published memory; neither paste nor closing a window
    // calls back into this message history to obtain the text.
    let closed = CloseClipboard() != 0;
    copied && closed
}

unsafe fn create_window(
    entries: &[String],
    inbox: Arc<Mutex<MessageInbox>>,
    clipboard_owner: HWND,
) -> Result<MessageWindow, String> {
    if !*RICH_EDIT_READY.get_or_init(|| !LoadLibraryW(wide("Msftedit.dll").as_ptr()).is_null()) {
        return Err("remote_message_display_failed".into());
    }
    let instance = GetModuleHandleW(null());
    let class = wide("LLRemoteReceivedMessage");
    let registered = *REGISTERED.get_or_init(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: GetStockObject(WHITE_BRUSH) as _,
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc) != 0
    });
    if !registered {
        return Err("remote_message_display_failed".into());
    }
    let dpi = GetDpiForSystem().max(96);
    let scale = dpi as f64 / 96.0;
    let px = |value: i32| (value as f64 * scale).round() as i32;
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let extended = WS_EX_DLGMODALFRAME;
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: px(420),
        bottom: px(360),
    };
    if AdjustWindowRectExForDpi(&mut frame, style, 0, extended, dpi) == 0 {
        return Err("remote_message_display_failed".into());
    }
    let hwnd = CreateWindowExW(
        extended,
        class.as_ptr(),
        wide("LL Remote").as_ptr(),
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        frame.right - frame.left,
        frame.bottom - frame.top,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return Err("remote_message_display_failed".into());
    }
    let mut data = Box::new(WindowData {
        history: RefCell::new(MessageHistory::default()),
        inbox,
        clipboard_owner,
        copy_button: null_mut(),
        close_button: null_mut(),
        body: null_mut(),
        card_rect: Cell::new(std::mem::zeroed()),
        measured_height: Cell::new(0),
        client_width: Cell::new(px(420)),
        reference_bounds: RefCell::new(None),
        scale,
        font: make_font(14, 400, scale),
        title_font: make_font(16, 600, scale),
        caption_font: make_font(12, 400, scale),
        button_font: make_font(14, 600, scale),
        white_brush: GdiObject(CreateSolidBrush(WHITE)),
        card_brush: GdiObject(CreateSolidBrush(WHITE)),
    });
    if [
        data.font.0,
        data.title_font.0,
        data.caption_font.0,
        data.button_font.0,
        data.white_brush.0,
        data.card_brush.0,
    ]
    .iter()
    .any(|handle| handle.is_null())
    {
        DestroyWindow(hwnd);
        return Err("remote_message_display_failed".into());
    }
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, std::ptr::from_ref(data.as_ref()) as _);
    let child = |class: &str,
                 text: &str,
                 style: u32,
                 x: i32,
                 y: i32,
                 w: i32,
                 h: i32,
                 id: usize,
                 font: HGDIOBJ| {
        let control = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            px(x),
            px(y),
            px(w),
            px(h),
            hwnd,
            id as _,
            instance,
            null(),
        );
        SendMessageW(control, WM_SETFONT, font as usize, 1);
        control
    };
    let heading = child(
        "STATIC",
        "收到远程消息",
        0,
        34,
        18,
        340,
        24,
        102,
        data.title_font.0,
    );
    let caption = child(
        "STATIC",
        "来自远程协助连接",
        0,
        20,
        44,
        380,
        18,
        103,
        data.caption_font.0,
    );
    // Measure the control's actual wrapped lines, including unbroken strings.
    let body = child(
        "RICHEDIT50W",
        "",
        WS_TABSTOP | WS_VSCROLL | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_READONLY as u32,
        32,
        82,
        356,
        200,
        101,
        data.font.0,
    );
    data.body = body;
    SendMessageW(body, EM_SETEVENTMASK, 0, ENM_REQUESTRESIZE);
    SendMessageW(body, EM_EXLIMITTEXT, 0, (MAX_HISTORY_BYTES * 2) as isize);
    SendMessageW(body, EM_SETBKGNDCOLOR, 0, WHITE as isize);
    let copy = child(
        "BUTTON",
        "复制",
        WS_TABSTOP | BS_OWNERDRAW as u32,
        240,
        300,
        76,
        32,
        COPY_ID,
        data.button_font.0,
    );
    let close = child(
        "BUTTON",
        "关闭",
        WS_TABSTOP | BS_OWNERDRAW as u32,
        324,
        300,
        76,
        32,
        CLOSE_ID,
        data.button_font.0,
    );
    data.copy_button = copy;
    data.close_button = close;
    if [heading, caption, body, copy, close]
        .iter()
        .any(|control| control.is_null())
    {
        DestroyWindow(hwnd);
        return Err("remote_message_display_failed".into());
    }
    append_transcript(&data, entries);
    resize_to_contents(hwnd, &data, true);
    focus_message_window(hwnd, &data);
    // data outlives HWND and all callbacks using GWLP_USERDATA.
    Ok(MessageWindow { hwnd, data })
}

// Look up the GUI across processes: the sharing host/user Agent may run while
// the main app is in another process. Ignore our own message-window class.
unsafe extern "system" fn find_main_window(hwnd: HWND, context: LPARAM) -> i32 {
    let mut title = [0u16; 128];
    let length = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
    let title = String::from_utf16_lossy(&title[..length.max(0) as usize]);
    if title != "LL Remote" && title != "LLRemote" {
        return 1;
    }
    let mut class = [0u16; 128];
    let count = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
    if String::from_utf16_lossy(&class[..count.max(0) as usize]) == "LLRemoteReceivedMessage" {
        return 1;
    }
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return 1;
    }
    let mut image = [0u16; 32768];
    let mut count = image.len() as u32;
    let queried = QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut count);
    CloseHandle(process);
    if queried == 0 {
        return 1;
    }
    let image = String::from_utf16_lossy(&image[..count as usize]);
    if !image
        .rsplit('\\')
        .next()
        .unwrap_or_default()
        .eq_ignore_ascii_case("LLRemote.exe")
    {
        return 1;
    }
    *(context as *mut HWND) = hwnd;
    0
}

unsafe fn main_window_and_work_area(fallback: HWND) -> (Option<(i32, i32)>, RECT) {
    let mut main: HWND = null_mut();
    EnumWindows(Some(find_main_window), (&mut main as *mut HWND) as isize);
    let mut bounds: RECT = std::mem::zeroed();
    let reference = if !main.is_null() && GetWindowRect(main, &mut bounds) != 0 {
        if IsIconic(main) != 0 {
            let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(main, &mut placement) != 0 {
                bounds = placement.rcNormalPosition;
            }
        }
        Some((bounds.right - bounds.left, bounds.bottom - bounds.top))
    } else {
        None
    };
    let monitor = MonitorFromWindow(
        if main.is_null() { fallback } else { main },
        MONITOR_DEFAULTTONEAREST,
    );
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    let work = if GetMonitorInfoW(monitor, &mut info) != 0 {
        info.rcWork
    } else {
        RECT {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        }
    };
    (reference, work)
}

unsafe fn measure_body(data: &WindowData, width: i32, height: i32) -> i32 {
    MoveWindow(
        data.body,
        data.px(32),
        data.px(82),
        width,
        height.max(data.px(24)),
        1,
    );
    data.measured_height.set(0);
    // Ask RichEdit for real wrapped, mixed-font content height. Its line-count
    // API can be stale while a previously hidden control is being laid out.
    // https://learn.microsoft.com/windows/win32/controls/em-requestresize
    SendMessageW(data.body, EM_REQUESTRESIZE, 0, 0);
    let measured = data.measured_height.get();
    if measured > 0 {
        measured
    } else {
        height.max(data.px(24)) + data.px(1)
    }
}

unsafe fn resize_to_contents(hwnd: HWND, data: &WindowData, center: bool) {
    let px = |value| data.px(value);
    let (reference, work) = {
        let mut cached = data.reference_bounds.borrow_mut();
        if cached
            .as_ref()
            .is_none_or(|(time, _, _)| time.elapsed() > Duration::from_secs(1))
        {
            let (reference, work) = main_window_and_work_area(hwnd);
            *cached = Some((Instant::now(), reference, work));
        }
        let (_, reference, work) = cached.as_ref().unwrap();
        (*reference, *work)
    };
    let limits = popup_limits(
        reference,
        (work.right - work.left, work.bottom - work.top),
        data.scale,
    );
    let mut decoration: RECT = std::mem::zeroed();
    AdjustWindowRectExForDpi(
        &mut decoration,
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        0,
        WS_EX_DLGMODALFRAME,
        GetDpiForSystem().max(96),
    );
    let extra_width = decoration.right - decoration.left;
    let extra_height = decoration.bottom - decoration.top;
    let max_width = (limits.width - extra_width).max(1);
    let max_card_height = (limits.height - extra_height - px(140)).max(px(32));
    let mut width = data.client_width.get().min(max_width);
    ShowScrollBar(data.body, SB_VERT, 0);
    let mut measured = measure_body(data, width - px(64), max_card_height - px(20));
    while measured > px(160).min(max_card_height - px(24)) && width < max_width {
        width = (width + px(80)).min(max_width);
        measured = measure_body(data, width - px(64), max_card_height - px(20));
    }
    data.client_width.set(width);
    let card_height = (measured + px(24)).clamp(px(44).min(max_card_height), max_card_height);
    let scroll = measured + px(24) > max_card_height;
    ShowScrollBar(data.body, SB_VERT, i32::from(scroll));
    MoveWindow(
        data.body,
        px(32),
        px(82),
        width - px(64),
        card_height - px(20),
        1,
    );
    data.card_rect.set(RECT {
        left: px(20),
        top: px(72),
        right: width - px(20),
        bottom: px(72) + card_height,
    });
    let footer_y = data.card_rect.get().bottom + px(16);
    MoveWindow(
        data.copy_button,
        width - px(180),
        footer_y,
        px(76),
        px(32),
        1,
    );
    MoveWindow(
        data.close_button,
        width - px(96),
        footer_y,
        px(76),
        px(32),
        1,
    );
    let outer_width = (width + extra_width).min(limits.width);
    let outer_height = (footer_y + px(52) + extra_height).min(limits.height);
    let mut old: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut old);
    let x = if center {
        work.left + (work.right - work.left - outer_width) / 2
    } else {
        old.left
    };
    let y = if center {
        work.top + (work.bottom - work.top - outer_height) / 2
    } else {
        old.top
    };
    SetWindowPos(
        hwnd,
        null_mut(),
        x.clamp(work.left, (work.right - outer_width).max(work.left)),
        y.clamp(work.top, (work.bottom - outer_height).max(work.top)),
        outer_width,
        outer_height,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    InvalidateRect(hwnd, null(), 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, GetClipboardOwner, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    unsafe fn read_copied_text(owner: HWND) -> String {
        assert_ne!(OpenClipboard(owner), 0);
        let handle = GetClipboardData(CF_UNICODETEXT as u32);
        assert!(!handle.is_null());
        let pointer = GlobalLock(handle) as *const u16;
        assert!(!pointer.is_null());
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let text = String::from_utf16(std::slice::from_raw_parts(pointer, length)).unwrap();
        GlobalUnlock(handle);
        CloseClipboard();
        text
    }

    #[test]
    #[ignore = "requires a Windows desktop and replaces the system clipboard"]
    fn copy_survives_close_and_close_does_not_restore_stale_text() {
        unsafe {
            let owner = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!owner.is_null());
            let text = "10:00:00  你好🐱\n第二行";
            let inbox = Arc::new(Mutex::new(MessageInbox::default()));
            let window = create_window(&[text.into()], Arc::clone(&inbox), owner).unwrap();
            SendMessageW(window.hwnd, WM_COMMAND, COPY_ID, 0);
            assert_eq!(GetClipboardOwner(), owner);
            // The first read happens after dismissal, as in the reported failure.
            DestroyWindow(window.hwnd);
            drop(window);
            assert_eq!(GetClipboardOwner(), owner);
            assert_eq!(read_copied_text(owner), text);

            let window = create_window(&[text.into()], inbox, owner).unwrap();
            SendMessageW(window.hwnd, WM_COMMAND, COPY_ID, 0);
            assert_eq!(read_copied_text(owner), text);
            // A newer copy must also survive closing the older message window.
            assert!(copy_message(owner, "newer clipboard text"));
            DestroyWindow(window.hwnd);
            drop(window);
            assert_eq!(read_copied_text(owner), "newer clipboard text");

            // No read/paste has occurred for this copy. Destroy every receiver
            // HWND before the first read to verify immediate system ownership,
            // rather than text supplied later by a living window.
            let final_text = "10:00:01  copied directly to Windows 🐱";
            assert!(copy_message(owner, final_text));
            DestroyWindow(owner);
            assert_eq!(read_copied_text(null_mut()), final_text);
        }
    }
}
