use std::fs;
use std::path::{Path, PathBuf};

use LLRemoteLib::local_agent_service::{
    agent_capabilities, default_agent_inbox_dir_for_home,
    resolve_agent_display_for_request_from_inventory, resolve_agent_path_for_home,
};
use LLRemoteLib::local_vnc_server::{LocalVncServerDisplay, LocalVncServerDisplayKind};

#[test]
fn local_agent_service_is_split_into_focused_modules() {
    let agent_root = PathBuf::from("src/local_agent_service");

    assert!(
        agent_root.join("mod.rs").is_file(),
        "local Agent implementation should live in a module directory"
    );
    assert!(
        agent_root.join("types.rs").is_file(),
        "public Agent command and response types should live outside the implementation module"
    );
    for module in [
        "capabilities.rs",
        "display.rs",
        "paths.rs",
        "protocol.rs",
        "transfers.rs",
    ] {
        assert!(
            agent_root.join(module).is_file(),
            "local Agent module should include {module}"
        );
    }
}

#[test]
fn default_agent_inbox_dir_uses_product_download_directory() {
    assert_eq!(
        default_agent_inbox_dir_for_home(Path::new("/Users/alice")),
        PathBuf::from("/Users/alice/Downloads/LL Remote")
    );
}

#[test]
fn agent_file_actions_expand_tilde_paths_against_agent_home() {
    assert_eq!(
        resolve_agent_path_for_home("~/Downloads/LLRemote", Path::new("/Users/alice")),
        PathBuf::from("/Users/alice/Downloads/LLRemote")
    );
    assert_eq!(
        resolve_agent_path_for_home("~", Path::new("/Users/alice")),
        PathBuf::from("/Users/alice")
    );
    assert_eq!(
        resolve_agent_path_for_home("/tmp/LLRemote", Path::new("/Users/alice")),
        PathBuf::from("/tmp/LLRemote")
    );
}

#[test]
fn agent_capabilities_exclude_remote_desktop_display_and_input_transport() {
    let capabilities = agent_capabilities();

    assert!(!capabilities
        .iter()
        .any(|capability| capability.starts_with("screen.stream.")));
    assert!(!capabilities.contains(&"input.pointer.inject".to_string()));
    assert!(!capabilities.contains(&"input.keyboard.inject".to_string()));
    assert!(capabilities.contains(&"agent.display.select".to_string()));
    assert_eq!(
        capabilities
            .iter()
            .any(|capability| capability == "display.wake"),
        cfg!(any(target_os = "macos", target_os = "windows"))
    );
    #[cfg(target_os = "windows")]
    for unsupported in ["remote.desktop.control", "agent.secure_input.recover"] {
        assert!(
            !capabilities
                .iter()
                .any(|capability| capability == unsupported),
            "Windows must not advertise unimplemented Agent action {unsupported}"
        );
    }
}

#[test]
fn agent_capabilities_include_runtime_file_resume_after_protocol_support_exists() {
    let capabilities = agent_capabilities();

    assert!(capabilities.contains(&"file.transfer.upload".to_string()));
    assert!(capabilities.contains(&"file.transfer.relative-path".to_string()));
    assert!(capabilities.contains(&"file.transfer.cancel".to_string()));
    assert!(capabilities.contains(&"file.transfer.checksum.sha256".to_string()));
    assert!(capabilities.contains(&"file.transfer.status".to_string()));
    assert!(capabilities.contains(&"file.transfer.resume".to_string()));
}

#[test]
fn agent_upload_uses_an_authenticated_remote_session() {
    let source = fs::read_to_string("src/commands/agent.rs").expect("read agent commands source");
    let lib_source = fs::read_to_string("src/lib.rs").expect("read lib.rs");
    assert!(source.contains("pub async fn upload_remote_agent_file"));
    assert!(source.contains("session_id: request.session_id"));
    assert!(lib_source.contains("commands::agent::upload_remote_agent_file,"));
}

#[test]
fn agent_file_transfer_routes_runtime_pause_status_and_resume() {
    let constants_source = fs::read_to_string("src/local_agent_service/mod.rs")
        .expect("read local_agent_service/mod.rs");
    let protocol_source = fs::read_to_string("src/local_agent_service/protocol.rs")
        .expect("read local_agent_service/protocol.rs");
    let transfer_source = fs::read_to_string("src/local_agent_service/transfers/receive.rs")
        .expect("read local_agent_service/transfers/receive.rs");

    assert!(constants_source.contains("const FILE_TRANSFER_PAUSE: &str = \"file.transfer.pause\""));
    assert!(
        constants_source.contains("const FILE_TRANSFER_STATUS: &str = \"file.transfer.status\"")
    );
    assert!(
        constants_source.contains("const FILE_TRANSFER_RESUME: &str = \"file.transfer.resume\"")
    );
    assert!(protocol_source
        .contains("FILE_TRANSFER_RESUME => resume_file_transfer(request, transfers)"));
    assert!(protocol_source
        .contains("FILE_TRANSFER_STATUS => status_file_transfer(request, transfers)"));
    assert!(
        protocol_source.contains("FILE_TRANSFER_PAUSE => pause_file_transfer(request, transfers)")
    );
    assert!(transfer_source.contains("existing_transfer_response"));
    assert!(transfer_source.contains("agent_transfer_fingerprint_mismatch"));
}

#[test]
fn agent_listener_handles_control_requests_concurrently_with_upload_chunks() {
    let source = fs::read_to_string("src/local_agent_service/protocol.rs")
        .expect("read local_agent_service/protocol.rs");
    let listener_source = source
        .split("fn run_agent_listener")
        .nth(1)
        .and_then(|value| value.split("fn handle_agent_stream").next())
        .expect("run_agent_listener source slice");

    assert!(listener_source.contains("LocalServerClientWorkers::new"));
    assert!(listener_source.contains("clients.spawn(stream, shutdown_socket"));
    assert!(listener_source.contains("Arc::clone(&sessions)"));
    assert!(listener_source.contains("Arc::clone(&transfers)"));
    assert!(listener_source.contains("bootstrap.clone()"));
}

#[test]
fn agent_upload_commit_uses_large_file_response_timeout() {
    let source = fs::read_to_string("src/local_agent_service/transfers/upload.rs")
        .expect("read local_agent_service/transfers/upload.rs");
    let upload_source = source
        .split("pub fn upload_agent_file_with_progress")
        .nth(1)
        .and_then(|value| value.split("fn agent_file_commit_timeout").next())
        .expect("upload_agent_file_with_progress source slice");

    assert!(upload_source.contains("agent_file_commit_timeout(metadata.len())"));
    assert!(source.contains("fn agent_file_commit_timeout"));
}

#[test]
fn agent_display_wake_is_routed_and_reported_as_display_capability() {
    let constants_source = fs::read_to_string("src/local_agent_service/mod.rs")
        .expect("read local_agent_service/mod.rs");
    let protocol_source = fs::read_to_string("src/local_agent_service/protocol.rs")
        .expect("read local_agent_service/protocol.rs");
    let display_handlers_source = fs::read_to_string("src/local_agent_service/display/handlers.rs")
        .expect("read local_agent_service/display/handlers.rs");
    let capability_source = fs::read_to_string("src/local_agent_service/capabilities.rs")
        .expect("read local_agent_service/capabilities.rs");

    assert!(constants_source.contains("const DISPLAY_WAKE: &str = \"display.wake\""));
    assert!(protocol_source.contains("DISPLAY_WAKE => wake_display_response()"));
    assert!(display_handlers_source.contains("wake_remote_display()?"));
    assert!(capability_source.contains("DISPLAY_WAKE.to_string()"));
}

#[test]
fn agent_remote_desktop_control_is_routed_and_reported_as_capability() {
    let constants_source = fs::read_to_string("src/local_agent_service/mod.rs")
        .expect("read local_agent_service/mod.rs");
    let protocol_source = fs::read_to_string("src/local_agent_service/protocol.rs")
        .expect("read local_agent_service/protocol.rs");
    let display_handlers_source = fs::read_to_string("src/local_agent_service/display/handlers.rs")
        .expect("read local_agent_service/display/handlers.rs");
    let capability_source = fs::read_to_string("src/local_agent_service/capabilities.rs")
        .expect("read local_agent_service/capabilities.rs");
    let commands_source =
        fs::read_to_string("src/commands/agent.rs").expect("read agent commands source");
    let lib_source = fs::read_to_string("src/lib.rs").expect("read lib.rs");
    let platform_source = fs::read_to_string("src/local_vnc_server_platform/desktop.rs")
        .expect("read local_vnc_server_platform/desktop.rs");
    let platform_impl_source = [
        fs::read_to_string("src/local_vnc_server_platform/ffi_macos.rs")
            .expect("read local_vnc_server_platform/ffi_macos.rs"),
        fs::read_to_string("src/local_vnc_server_platform/keyboard_macos.rs")
            .expect("read local_vnc_server_platform/keyboard_macos.rs"),
    ]
    .join("\n");

    assert!(constants_source
        .contains("const REMOTE_DESKTOP_CONTROL: &str = \"remote.desktop.control\""));
    assert!(protocol_source
        .contains("REMOTE_DESKTOP_CONTROL => remote_desktop_control_response(request)"));
    assert!(display_handlers_source.contains("control_remote_desktop("));
    assert!(capability_source.contains("REMOTE_DESKTOP_CONTROL.to_string()"));
    assert!(capability_source.contains("REMOTE_DESKTOP_CONTROL.to_string()"));
    assert!(commands_source.contains("pub async fn invoke_remote_agent"));
    assert!(lib_source.contains("commands::agent::invoke_remote_agent,"));
    assert!(platform_source.contains(
        "inject_macos_key_event_with_flags(true, arrow_keysym, None, K_CG_EVENT_FLAG_MASK_CONTROL)"
    ));
    assert!(platform_impl_source.contains("CGEventSetFlags(event, flags)"));
    assert!(platform_source.contains("osascript"));
    assert!(platform_source.contains("System Events"));
    assert!(platform_source.contains("using control down"));
}

#[test]
fn windows_clipboard_imports_match_windows_sys_api_layout() {
    let platform_source = fs::read_to_string("src/local_vnc_server_platform/clipboard.rs")
        .expect("read local_vnc_server_platform/clipboard.rs");
    let display_source = fs::read_to_string("src/local_vnc_server_platform/display.rs")
        .expect("read local_vnc_server_platform/display.rs");
    let mod_source = fs::read_to_string("src/local_vnc_server_platform/mod.rs")
        .expect("read local_vnc_server_platform/mod.rs");

    assert!(platform_source.contains("windows_sys::Win32::System::Ole::CF_UNICODETEXT"));
    assert!(platform_source.contains("windows_sys::Win32::Foundation::GlobalFree"));
    assert!(platform_source.contains("handle.is_null()"));
    assert!(display_source
        .contains("#[cfg(not(target_os = \"windows\"))]\nfn fallback_display_overview()"));
    assert!(mod_source.contains("#[cfg(any(target_os = \"windows\", all(target_os = \"macos\", test)))]\nuse crate::local_vnc_server::types::CapturedPixelFormat;"));
    assert!(!platform_source.contains("CapturedPixelFormat, DisplayFrame"));
    assert!(!platform_source.contains("Win32::System::DataExchange::{\n        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData, CF_UNICODETEXT"));
    assert!(!platform_source.contains("Win32::System::Memory::{\n        GlobalAlloc, GlobalFree, GlobalLock, GlobalUnlock, GMEM_MOVEABLE"));
    assert!(!platform_source.contains("if handle == 0"));
    assert!(!platform_source.contains("fn inject_macos_key_event_with_flags(_down: bool"));
    assert!(!platform_source.contains("fn infer_captured_pixel_format(_alpha_info: u32"));
    assert!(!platform_source
        .contains("#[cfg(target_os = \"macos\")]\n#[cfg(not(target_os = \"macos\"))]"));
}

#[test]
fn windows_local_server_uses_real_framebuffer_capture_path() {
    let platform_source = fs::read_to_string("src/local_vnc_server_platform/capture_windows.rs")
        .expect("read local_vnc_server_platform/capture_windows.rs");
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");

    assert!(cargo_toml.contains("\"Win32_Graphics_Gdi\""));
    assert!(platform_source.contains("pub(super) struct WindowsCapture"));
    assert!(platform_source.contains("GetSystemMetrics(SM_CXVIRTUALSCREEN)"));
    assert!(platform_source.contains("GetSystemMetrics(SM_CYVIRTUALSCREEN)"));
    assert!(platform_source.contains("GetSystemMetrics(SM_XVIRTUALSCREEN)"));
    assert!(platform_source.contains("GetSystemMetrics(SM_YVIRTUALSCREEN)"));
    assert!(!platform_source.contains("GetDeviceCaps(screen_dc, HORZRES"));
    assert!(platform_source.contains("BitBlt("));
    assert!(platform_source.contains("GetDIBits("));
    assert!(platform_source.contains("CapturedPixelFormat::Bgra8888"));
    assert!(!platform_source
        .contains("#[cfg(not(target_os = \"macos\"))]\nfn capture_macos_framebuffer"));
}

#[test]
fn windows_local_server_injects_pointer_and_keyboard_input() {
    let platform_source = fs::read_to_string("src/local_vnc_server_platform/input.rs")
        .expect("read local_vnc_server_platform/input.rs");
    let platform_impl_source = [
        fs::read_to_string("src/local_vnc_server_platform/pointer_windows.rs")
            .expect("read local_vnc_server_platform/pointer_windows.rs"),
        fs::read_to_string("src/local_vnc_server_platform/keyboard_windows.rs")
            .expect("read local_vnc_server_platform/keyboard_windows.rs"),
    ]
    .join("\n");
    let display_source = fs::read_to_string("src/local_vnc_server_platform/display.rs")
        .expect("read local_vnc_server_platform/display.rs");
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");

    assert!(cargo_toml.contains("\"Win32_UI_Input_KeyboardAndMouse\""));
    assert!(cargo_toml.contains("\"Win32_UI_WindowsAndMessaging\""));
    assert!(platform_impl_source.contains("fn inject_windows_pointer_event("));
    assert!(platform_impl_source.contains("fn inject_windows_key_event("));
    assert!(display_source.contains("fn list_windows_displays("));
    assert!(platform_impl_source.contains("SM_CXVIRTUALSCREEN"));
    assert!(platform_impl_source.contains("SM_XVIRTUALSCREEN"));
    assert!(platform_impl_source.contains("SendInput("));
    assert!(platform_impl_source.contains("MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK"));
    assert!(platform_impl_source.contains("KEYEVENTF_UNICODE"));
    assert!(
        platform_impl_source.contains("-> windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT")
    );
    assert!(platform_impl_source.contains("windows_vk_for_keysym"));
    assert!(platform_impl_source.contains("0x41..=0x5a => Some(keysym as u16)"));
    assert!(platform_impl_source.contains("0x61..=0x7a => Some((keysym - 0x20) as u16)"));
    assert!(platform_impl_source.contains("0x30..=0x39 => Some(keysym as u16)"));
    assert!(!platform_source
        .contains("#[cfg(not(target_os = \"macos\"))]\nfn inject_macos_pointer_event"));
    assert!(!platform_source
        .contains("#[cfg(not(target_os = \"macos\"))]\nfn inject_macos_key_event(_down"));
}

#[test]
fn windows_local_server_handles_secure_attention_as_special_sequence() {
    let platform_source = fs::read_to_string("src/local_vnc_server_platform/desktop.rs")
        .expect("read local_vnc_server_platform/desktop.rs");
    let protocol_source = [
        fs::read_to_string("src/local_vnc_server_protocol/secure_attention.rs")
            .expect("read local_vnc_server_protocol/secure_attention.rs"),
        fs::read_to_string("src/local_vnc_server_protocol/secure_attention_handler.rs")
            .expect("read local_vnc_server_protocol/secure_attention_handler.rs"),
        fs::read_to_string("src/local_vnc_server_protocol/client_loop.rs")
            .expect("read local_vnc_server_protocol/client_loop.rs"),
    ]
    .join("\n");
    let local_server_source =
        fs::read_to_string("src/local_vnc_server/mod.rs").expect("read local_vnc_server/mod.rs");
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");

    assert!(cargo_toml.contains("\"Win32_System_LibraryLoader\""));
    assert!(local_server_source.contains("send_secure_attention_sequence"));
    assert!(protocol_source.contains("SecureAttentionKeyState"));
    assert!(protocol_source.contains("try_handle_secure_attention_key_event"));
    assert!(protocol_source.contains("send_secure_attention_sequence()"));
    assert!(protocol_source.contains("keysym_is_control"));
    assert!(protocol_source.contains("keysym_is_alt"));
    assert!(protocol_source.contains("KEYSYM_DELETE"));
    assert!(platform_source.contains("fn send_windows_secure_attention_sequence()"));
    assert!(platform_source.contains("request_secure_attention(session)"));
    assert!(platform_source.contains("Err(\"secure_attention_unsupported\".into())"));
}

#[test]
fn agent_display_select_updates_local_vnc_server_shared_display_only() {
    let constants_source = fs::read_to_string("src/local_agent_service/mod.rs")
        .expect("read local_agent_service/mod.rs");
    let protocol_source = fs::read_to_string("src/local_agent_service/protocol.rs")
        .expect("read local_agent_service/protocol.rs");
    let source = fs::read_to_string("src/local_agent_service/display/handlers.rs")
        .expect("read local_agent_service/display/handlers.rs");
    let commands_source =
        fs::read_to_string("src/commands/agent.rs").expect("read agent commands source");
    let lib_source = fs::read_to_string("src/lib.rs").expect("read lib.rs");

    assert!(
        constants_source.contains("const AGENT_DISPLAY_SELECT: &str = \"agent.display.select\"")
    );
    assert!(protocol_source
        .contains("AGENT_DISPLAY_SELECT => select_display_response(request, context)"));
    assert!(source.contains("fn select_display_response("));
    assert!(source.contains("shared.replace_with(next_display)"));
    assert!(source.contains("\"agent_display_not_found\""));
    assert!(commands_source.contains("pub async fn invoke_remote_agent"));
    assert!(lib_source.contains("commands::agent::invoke_remote_agent,"));
    assert!(!source.contains("start_agent_screen_stream"));
    assert!(!source.contains("send_agent_input_event"));
}

#[test]
fn agent_capabilities_require_authenticated_session() {
    let source = fs::read_to_string("src/local_agent_service/protocol.rs")
        .expect("read local_agent_service/protocol.rs");
    let auth_gate_source = source
        .split("fn handle_agent_request")
        .nth(1)
        .and_then(|value| value.split("fn create_file_transfer").next())
        .expect("handle_agent_request source slice");

    assert!(auth_gate_source.contains("agent.session.open"));
    assert!(auth_gate_source.contains("agent_session_required"));
    assert!(auth_gate_source.contains("agent.session.describe"));
}

#[test]
fn agent_enhanced_display_request_uses_selected_local_share_display() {
    let displays = vec![
        LocalVncServerDisplay {
            id: "display:main".to_string(),
            label: "Main".to_string(),
            kind: LocalVncServerDisplayKind::Main,
            is_primary: true,
            x: 0,
            y: 0,
            width: 1512,
            height: 982,
            pixel_width: 3024,
            pixel_height: 1964,
            scale_factor: 2.0,
        },
        LocalVncServerDisplay {
            id: "display:secondary".to_string(),
            label: "Secondary".to_string(),
            kind: LocalVncServerDisplayKind::Display,
            is_primary: false,
            x: 1512,
            y: 0,
            width: 1280,
            height: 720,
            pixel_width: 1280,
            pixel_height: 720,
            scale_factor: 1.0,
        },
    ];

    let shared =
        resolve_agent_display_for_request_from_inventory(displays, Some("display:secondary"), None);

    assert_eq!(shared.id, "display:secondary");
    assert_eq!(shared.x, 1512);
    assert_eq!(shared.width, 1280);
    assert_eq!(shared.framebuffer_width, 1280);
    assert_eq!(shared.framebuffer_height, 720);
}

#[test]
fn agent_enhanced_display_request_uses_local_share_framebuffer_size() {
    let displays = vec![LocalVncServerDisplay {
        id: "display:main".to_string(),
        label: "Main".to_string(),
        kind: LocalVncServerDisplayKind::Main,
        is_primary: true,
        x: 0,
        y: 0,
        width: 1512,
        height: 982,
        pixel_width: 3024,
        pixel_height: 1964,
        scale_factor: 2.0,
    }];

    let shared = resolve_agent_display_for_request_from_inventory(
        displays,
        Some("display:main"),
        Some((1428, 1010)),
    );

    assert_eq!(shared.id, "display:main");
    assert_eq!(shared.framebuffer_width, 1428);
    assert_eq!(shared.framebuffer_height, 1010);
}
