use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use LLRemoteLib::local_vnc_server::{
    constrained_framebuffer_size, convert_rfb_framebuffer_to_pixel_format,
    create_vnc_auth_response, framebuffer_size_for_resolution, get_local_vnc_server_permissions,
    mac_keycode_for_keyboard_input, mac_keycode_for_keysym, map_vnc_point_to_display,
    map_vnc_point_to_screen, refresh_interval, scale_captured_framebuffer_to_rfb,
    scale_rgba_to_rfb_framebuffer, scroll_delta_for_pointer_transition,
    validate_required_capture_permissions, CapturedPixelFormat, DisplayFrame, DisplaySize,
    LocalVncServerCaptureStatus, LocalVncServerManager, LocalVncServerPermission,
    LocalVncServerPermissionKind, LocalVncServerPermissionStatus, LocalVncServerRefreshMode,
    LocalVncServerResolutionMode, LocalVncServerStartRequest, LocalVncServerStatus,
    LocalVncServerViewRequest, RfbPixelFormat, ScreenPoint,
};

#[test]
fn local_vnc_server_protocol_is_split_into_focused_modules() {
    let protocol_root = PathBuf::from("src/local_vnc_server_protocol");

    assert!(
        protocol_root.join("mod.rs").is_file(),
        "local VNC RFB protocol should live in a module directory"
    );
    assert!(
        protocol_root.join("tests.rs").is_file(),
        "large inline protocol tests should be split out of the implementation module"
    );
}

#[test]
fn local_vnc_server_platform_is_split_into_focused_modules() {
    let platform_root = PathBuf::from("src/local_vnc_server_platform");

    for module in [
        "mod.rs",
        "capture.rs",
        "clipboard.rs",
        "desktop.rs",
        "display.rs",
        "input.rs",
    ] {
        assert!(
            platform_root.join(module).is_file(),
            "local VNC Server platform module should include {module}"
        );
    }
}

fn start_request(port: u16) -> LocalVncServerStartRequest {
    LocalVncServerStartRequest {
        port,
        bind_address: "127.0.0.1".to_string(),
        password: "temporary-secret".to_string(),
        view_only: false,
    }
}

#[test]
fn local_vnc_server_requires_screen_recording_before_start() {
    let permissions = vec![LocalVncServerPermission {
        kind: LocalVncServerPermissionKind::ScreenRecording,
        status: LocalVncServerPermissionStatus::Missing,
        required_for: "screen_capture".to_string(),
    }];

    let error = validate_required_capture_permissions(&permissions)
        .expect_err("missing screen recording should block local VNC server start");

    assert!(error.contains("permission_missing"));
    assert!(error.contains("screen_recording"));
}

#[test]
fn local_vnc_server_starts_listener_and_serves_rfb_banner() {
    let manager = LocalVncServerManager::default();

    let overview = manager
        .start(start_request(0))
        .expect("local VNC server should start");

    assert_eq!(overview.status, LocalVncServerStatus::Running);
    assert_eq!(overview.bind_address, "127.0.0.1");
    assert!(overview.port > 0);
    assert_eq!(overview.security_type, "vnc_auth");
    assert!(!overview.view_only);
    assert_eq!(overview.active_connections, 0);
    assert_eq!(overview.agent.status.as_str(), "running");
    assert!(overview.agent.port.is_some());
    assert!(!overview.agent.capabilities.is_empty());
    assert!(!overview.addresses.is_empty());
    assert!(!overview.displays.is_empty());
    assert!(overview.framebuffer_width >= 640);
    assert!(overview.framebuffer_height >= 400);
    assert!(overview.framebuffer_width <= 1920);
    assert!(overview.framebuffer_height <= 1080);

    let mut stream =
        TcpStream::connect(("127.0.0.1", overview.port)).expect("connect local VNC server");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set read timeout");
    let mut banner = [0_u8; 12];
    stream.read_exact(&mut banner).expect("read RFB banner");

    assert_eq!(&banner, b"RFB 003.008\n");

    let status = manager.status().expect("status should be available");
    assert_eq!(status.status, LocalVncServerStatus::Running);
    assert_eq!(status.port, overview.port);
    assert!(!status.addresses.is_empty());

    let stopped = manager.stop().expect("local VNC server should stop");
    assert_eq!(stopped.status, LocalVncServerStatus::Stopped);
    assert_eq!(stopped.agent.status.as_str(), "stopped");
}

#[test]
fn local_vnc_server_accepts_a_second_client_while_the_first_client_is_negotiating() {
    let manager = LocalVncServerManager::default();
    let overview = manager
        .start(start_request(0))
        .expect("start local VNC server");

    let mut first = TcpStream::connect(("127.0.0.1", overview.port)).expect("connect first client");
    let mut first_banner = [0_u8; 12];
    first
        .read_exact(&mut first_banner)
        .expect("read first banner");

    let mut second =
        TcpStream::connect(("127.0.0.1", overview.port)).expect("connect second client");
    second
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set second timeout");
    let mut second_banner = [0_u8; 12];
    second
        .read_exact(&mut second_banner)
        .expect("read second banner while first awaits version");

    assert_eq!(&first_banner, b"RFB 003.008\n");
    assert_eq!(&second_banner, b"RFB 003.008\n");
    manager.stop().expect("stop local VNC server");
}

#[test]
fn local_vnc_server_status_includes_platform_permissions() {
    let manager = LocalVncServerManager::default();
    let overview = manager.status().expect("status should be available");
    let permissions = get_local_vnc_server_permissions();

    assert_eq!(overview.permissions, permissions);

    if std::env::consts::OS == "macos" {
        assert!(permissions
            .iter()
            .any(|permission| permission.kind == LocalVncServerPermissionKind::ScreenRecording));
        assert!(permissions
            .iter()
            .any(|permission| permission.kind == LocalVncServerPermissionKind::Accessibility));
    }
}

#[test]
fn local_vnc_server_maps_vnc_pointer_to_screen_coordinates() {
    assert_eq!(
        map_vnc_point_to_screen(
            80,
            50,
            DisplaySize {
                width: 160,
                height: 100,
            },
            DisplaySize {
                width: 3200,
                height: 2000,
            },
        ),
        ScreenPoint { x: 1600, y: 1000 },
    );
    assert_eq!(
        map_vnc_point_to_screen(
            999,
            999,
            DisplaySize {
                width: 160,
                height: 100,
            },
            DisplaySize {
                width: 3200,
                height: 2000,
            },
        ),
        ScreenPoint { x: 3180, y: 1980 },
    );
}

#[test]
fn local_vnc_server_constrains_framebuffer_size_without_upscaling() {
    assert_eq!(
        constrained_framebuffer_size(3024, 1964),
        DisplaySize {
            width: 1663,
            height: 1080,
        },
    );
    assert_eq!(
        constrained_framebuffer_size(1280, 800),
        DisplaySize {
            width: 1280,
            height: 800,
        },
    );
}

#[test]
fn local_vnc_server_resolution_modes_preserve_aspect_ratio() {
    assert_eq!(
        framebuffer_size_for_resolution(3840, 2160, LocalVncServerResolutionMode::Fhd),
        DisplaySize {
            width: 1920,
            height: 1080,
        },
    );
    assert_eq!(
        framebuffer_size_for_resolution(3840, 2160, LocalVncServerResolutionMode::Hd),
        DisplaySize {
            width: 1280,
            height: 720,
        },
    );
    assert_eq!(
        framebuffer_size_for_resolution(800, 600, LocalVncServerResolutionMode::Native),
        DisplaySize {
            width: 800,
            height: 600,
        },
    );
}

#[test]
fn local_vnc_server_resolution_modes_support_explicit_orientation() {
    assert_eq!(
        framebuffer_size_for_resolution(3840, 2160, LocalVncServerResolutionMode::FhdLandscape),
        DisplaySize {
            width: 1920,
            height: 1080,
        },
    );
    assert_eq!(
        framebuffer_size_for_resolution(3840, 2160, LocalVncServerResolutionMode::FhdPortrait),
        DisplaySize {
            width: 1080,
            height: 1920,
        },
    );
    assert_eq!(
        framebuffer_size_for_resolution(3840, 2160, LocalVncServerResolutionMode::HdLandscape),
        DisplaySize {
            width: 1280,
            height: 720,
        },
    );
    assert_eq!(
        framebuffer_size_for_resolution(3840, 2160, LocalVncServerResolutionMode::HdPortrait),
        DisplaySize {
            width: 720,
            height: 1280,
        },
    );
}

#[test]
fn local_vnc_server_converts_rgba_capture_to_rfb_bgrx_pixels() {
    let source = [
        250, 140, 170, 255, // pink-ish RGBA
        20, 80, 230, 255, // blue-ish RGBA
    ];

    assert_eq!(
        scale_rgba_to_rfb_framebuffer(&source, 2, 1, 8, 2, 1),
        vec![
            170, 140, 250, 0, // RFB little-endian 0x00RRGGBB => B,G,R,0
            230, 80, 20, 0,
        ],
    );
}

#[test]
fn local_vnc_server_converts_bgra_capture_to_rfb_bgrx_pixels() {
    let source = [
        170, 140, 250, 255, // pink-ish BGRA
        230, 80, 20, 255, // blue-ish BGRA
    ];

    assert_eq!(
        scale_captured_framebuffer_to_rfb(&source, 2, 1, 8, 2, 1, CapturedPixelFormat::Bgra8888,),
        vec![
            170, 140, 250, 0, // RFB little-endian 0x00RRGGBB => B,G,R,0
            230, 80, 20, 0,
        ],
    );
}

#[test]
fn local_vnc_server_converts_default_bgrx_framebuffer_to_requested_rgbx_pixel_format() {
    let server_framebuffer = vec![
        10, 20, 30, 0, // RFB default little-endian 0x00RRGGBB => B,G,R,0
        80, 90, 100, 0,
    ];
    let rgbx_format = RfbPixelFormat {
        bits_per_pixel: 32,
        depth: 24,
        big_endian: false,
        true_color: true,
        red_max: 255,
        green_max: 255,
        blue_max: 255,
        red_shift: 0,
        green_shift: 8,
        blue_shift: 16,
    };

    assert_eq!(
        convert_rfb_framebuffer_to_pixel_format(&server_framebuffer, rgbx_format),
        vec![
            30, 20, 10, 0, // requested little-endian 0x00BBGGRR => R,G,B,padding
            100, 90, 80, 0,
        ],
    );
}

#[test]
fn local_vnc_server_refresh_modes_have_ordered_intervals() {
    assert!(
        refresh_interval(LocalVncServerRefreshMode::Responsive)
            < refresh_interval(LocalVncServerRefreshMode::Balanced)
    );
    assert!(
        refresh_interval(LocalVncServerRefreshMode::Balanced)
            < refresh_interval(LocalVncServerRefreshMode::LowPower)
    );
}

#[test]
fn local_vnc_server_responsive_refresh_targets_sixty_fps() {
    assert!(refresh_interval(LocalVncServerRefreshMode::Responsive) <= Duration::from_millis(17));
}

#[test]
fn local_vnc_server_writes_dirty_rectangles_without_rect_buffer_copy() {
    let source = [
        fs::read_to_string("src/local_vnc_server_protocol/framebuffer/writer.rs")
            .expect("read local server framebuffer writer source"),
        fs::read_to_string("src/local_vnc_server_protocol/framebuffer/pixels.rs")
            .expect("read local server framebuffer pixels source"),
    ]
    .join("\n");

    assert!(!source.contains("extract_framebuffer_rect(raw_framebuffer"));
    assert!(source.contains("write_framebuffer_row"));
}

#[test]
fn local_vnc_server_set_encodings_can_disable_extended_clipboard_for_live_connection() {
    let source = fs::read_to_string("src/local_vnc_server_protocol/client_loop.rs")
        .expect("read local server client loop source");
    let set_encodings = source
        .split("let support = parse_client_encoding_support(&encodings);")
        .nth(1)
        .and_then(|value| {
            value
                .split("if supports_extended_clipboard && !client_extended_clipboard_ready")
                .next()
        })
        .expect("SetEncodings support handling");

    assert!(set_encodings.contains("supports_extended_clipboard = support.extended_clipboard;"));
    assert!(set_encodings.contains("client_extended_clipboard_ready = false;"));
}

#[test]
fn local_vnc_server_captures_frames_on_background_worker() {
    let server_source = [
        fs::read_to_string("src/local_vnc_server/capture.rs")
            .expect("read local server capture source"),
        fs::read_to_string("src/local_vnc_server/manager.rs")
            .expect("read local server manager source"),
    ]
    .join("\n");
    let frame_cache_source = fs::read_to_string("src/local_vnc_server/frame_cache.rs")
        .expect("read local server frame cache source");
    let protocol_source = fs::read_to_string("src/local_vnc_server_protocol/client_loop.rs")
        .expect("read local protocol client loop source");
    let protocol_resources = fs::read_to_string("src/local_vnc_server_protocol/listener.rs")
        .expect("read local protocol resources");

    assert!(frame_cache_source.contains("struct LocalVncServerFrameCache"));
    assert!(server_source.contains("run_framebuffer_capture_loop"));
    assert!(server_source.contains("capture_worker: Some(capture_worker)"));
    assert!(protocol_resources.contains("frame_cache: Arc<LocalVncServerFrameCache>"));
    assert!(protocol_source.contains("Arc::clone(&context.frame_cache)"));
    assert!(!protocol_source.contains("capture_framebuffer(shared_display)"));
}

#[test]
fn local_vnc_server_frame_diagnostics_report_rfb_update_metrics() {
    let types_source = fs::read_to_string("src/local_vnc_server/types.rs")
        .expect("read local server types source");
    let local_server_source = fs::read_to_string("src/local_vnc_server/frame_cache.rs")
        .expect("read local server frame cache source");
    let protocol_source = fs::read_to_string("src/local_vnc_server_protocol/client_connection.rs")
        .expect("read local protocol client loop source");

    assert!(types_source.contains("pub framebuffer_update_requests: u64"));
    assert!(types_source.contains("pub framebuffer_updates: u64"));
    assert!(types_source.contains("pub last_framebuffer_encoding: String"));
    assert!(types_source.contains("pub last_raw_bytes_written: u64"));
    assert!(types_source.contains("pub last_encoded_bytes_written: u64"));
    assert!(local_server_source.contains("framebuffer_update_requests"));
    assert!(local_server_source.contains("last_framebuffer_encoding"));
    assert!(protocol_source.contains("record_framebuffer_update_request"));
}

#[test]
fn local_vnc_server_frame_cache_keeps_metrics_but_not_captured_pixels() {
    let source = fs::read_to_string("src/local_vnc_server/frame_cache.rs")
        .expect("read local server diagnostics cache source");
    assert!(source.contains("record_capture"));
    assert!(source.contains("record_send"));
    assert!(!source.contains("SharedFramebufferFrame"));
    assert!(!source.contains("latest_for_display"));
    assert!(!source.contains("Arc<Vec<u8>>"));
}

#[test]
fn local_vnc_server_updates_running_view_resolution_and_refresh_mode() {
    let manager = LocalVncServerManager::default();
    let overview = manager
        .start(start_request(0))
        .expect("local VNC server should start");

    let updated = manager
        .update_view(LocalVncServerViewRequest {
            display_id: Some(overview.selected_display_id.clone()),
            resolution_mode: Some(LocalVncServerResolutionMode::Hd),
            refresh_mode: Some(LocalVncServerRefreshMode::LowPower),
        })
        .expect("view should update while server is running");

    assert_eq!(updated.status, LocalVncServerStatus::Running);
    assert_eq!(updated.selected_display_id, overview.selected_display_id);
    assert_eq!(updated.resolution_mode, LocalVncServerResolutionMode::Hd);
    assert_eq!(updated.refresh_mode, LocalVncServerRefreshMode::LowPower);
    assert!(updated.framebuffer_width <= 1280);
    assert!(updated.framebuffer_height <= 720);

    manager.stop().expect("local VNC server should stop");
}

#[test]
fn local_vnc_server_defaults_to_responsive_refresh_mode() {
    let source =
        fs::read_to_string("src/local_vnc_server/mod.rs").expect("read local server source");

    assert!(
        source.contains("const DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE: LocalVncServerRefreshMode")
    );
    assert!(source.contains("LocalVncServerRefreshMode::Responsive;"));
    assert!(!source.contains("resolve_shared_display(\n            None,\n            LocalVncServerResolutionMode::Fit,\n            LocalVncServerRefreshMode::Balanced,"));
}

#[test]
fn local_vnc_server_uses_native_resolution_by_default_on_windows() {
    let source = [
        fs::read_to_string("src/local_vnc_server/display.rs")
            .expect("read local server display source"),
        fs::read_to_string("src/local_vnc_server/manager.rs")
            .expect("read local server manager source"),
    ]
    .join(
        "
",
    );

    assert!(source.contains("fn default_local_vnc_server_resolution_mode()"));
    assert!(source.contains("cfg!(target_os = \"windows\")"));
    assert!(source.contains("LocalVncServerResolutionMode::Native"));
    assert!(source.contains("default_local_vnc_server_resolution_mode(),"));
    assert!(!source.contains("resolve_shared_display(\n            None,\n            LocalVncServerResolutionMode::Fit,\n            DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE,"));
}

#[test]
fn local_vnc_server_maps_vnc_pointer_to_offset_display_coordinates() {
    assert_eq!(
        map_vnc_point_to_display(
            960,
            540,
            DisplaySize {
                width: 1920,
                height: 1080,
            },
            DisplayFrame {
                x: -1920,
                y: 120,
                width: 1920,
                height: 1080,
                pixel_width: 1920,
                pixel_height: 1080,
            },
        ),
        ScreenPoint { x: -960, y: 660 },
    );
}

#[test]
fn local_vnc_server_maps_common_keysyms_to_macos_keycodes() {
    assert_eq!(mac_keycode_for_keysym(0x61), Some(0));
    assert_eq!(mac_keycode_for_keysym(0x41), Some(0));
    assert_eq!(mac_keycode_for_keysym(0xff0d), Some(36));
    assert_eq!(mac_keycode_for_keysym(0xff51), Some(123));
    assert_eq!(mac_keycode_for_keysym(0xff52), Some(126));
    assert_eq!(mac_keycode_for_keysym(0xff53), Some(124));
    assert_eq!(mac_keycode_for_keysym(0xff54), Some(125));
    assert_eq!(mac_keycode_for_keysym(0xffff), Some(117));
    assert_eq!(mac_keycode_for_keysym(0xffe3), Some(59));
    assert_eq!(mac_keycode_for_keysym(0xffeb), Some(55));
    assert_eq!(mac_keycode_for_keysym(0xffe9), Some(58));
    assert_eq!(mac_keycode_for_keysym(0xffbe), Some(122));
    assert_eq!(mac_keycode_for_keysym(0x2603), None);
}

#[test]
fn local_vnc_server_uses_dom_code_for_macos_numpad_keycodes() {
    assert_eq!(
        mac_keycode_for_keyboard_input(0x31, Some("Digit1")),
        Some(18)
    );
    assert_eq!(
        mac_keycode_for_keyboard_input(0x31, Some("Numpad1")),
        Some(83)
    );
    assert_eq!(
        mac_keycode_for_keyboard_input(0xff0d, Some("NumpadEnter")),
        Some(76)
    );
    assert_eq!(
        mac_keycode_for_keyboard_input(0x2b, Some("NumpadAdd")),
        Some(69)
    );
}

#[test]
fn local_vnc_server_maps_shifted_printable_keysyms_to_macos_keycodes() {
    assert_eq!(mac_keycode_for_keysym(0x21), Some(18)); // !
    assert_eq!(mac_keycode_for_keysym(0x40), Some(19)); // @
    assert_eq!(mac_keycode_for_keysym(0x23), Some(20)); // #
    assert_eq!(mac_keycode_for_keysym(0x24), Some(21)); // $
    assert_eq!(mac_keycode_for_keysym(0x25), Some(23)); // %
    assert_eq!(mac_keycode_for_keysym(0x5e), Some(22)); // ^
    assert_eq!(mac_keycode_for_keysym(0x26), Some(26)); // &
    assert_eq!(mac_keycode_for_keysym(0x2a), Some(28)); // *
    assert_eq!(mac_keycode_for_keysym(0x28), Some(25)); // (
    assert_eq!(mac_keycode_for_keysym(0x29), Some(29)); // )
}

#[test]
fn local_vnc_server_detects_rfb_wheel_button_transitions() {
    assert_eq!(scroll_delta_for_pointer_transition(0, 8), Some(1));
    assert_eq!(scroll_delta_for_pointer_transition(8, 0), None);
    assert_eq!(scroll_delta_for_pointer_transition(0, 16), Some(-1));
    assert_eq!(scroll_delta_for_pointer_transition(16, 0), None);
    assert_eq!(scroll_delta_for_pointer_transition(1, 1 | 8), Some(1));
}

#[test]
fn local_vnc_server_authenticates_and_reports_actual_capture_result() {
    let manager = LocalVncServerManager::default();
    let overview = manager
        .start(start_request(0))
        .expect("local VNC server should start");
    let mut stream =
        TcpStream::connect(("127.0.0.1", overview.port)).expect("connect local VNC server");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("set write timeout");

    let mut banner = [0_u8; 12];
    stream.read_exact(&mut banner).expect("read server version");
    assert_eq!(&banner, b"RFB 003.008\n");
    stream
        .write_all(b"RFB 003.008\n")
        .expect("write client version");

    let mut security_types = [0_u8; 2];
    stream
        .read_exact(&mut security_types)
        .expect("read security types");
    assert_eq!(security_types, [1, 2]);
    stream.write_all(&[2]).expect("select VNC auth");

    let mut challenge = [0_u8; 16];
    stream.read_exact(&mut challenge).expect("read challenge");
    let response = create_vnc_auth_response("temporary-secret", &challenge);
    stream.write_all(&response).expect("write auth response");

    let mut security_result = [0_u8; 4];
    stream
        .read_exact(&mut security_result)
        .expect("read security result");
    assert_eq!(u32::from_be_bytes(security_result), 0);

    stream.write_all(&[1]).expect("write client init");

    let mut init_head = [0_u8; 24];
    stream.read_exact(&mut init_head).expect("read server init");
    let width = u16::from_be_bytes([init_head[0], init_head[1]]);
    let height = u16::from_be_bytes([init_head[2], init_head[3]]);
    let name_len = u32::from_be_bytes([init_head[20], init_head[21], init_head[22], init_head[23]]);
    assert_eq!(
        (width, height),
        (
            overview.framebuffer_width as u16,
            overview.framebuffer_height as u16,
        ),
    );
    assert!(width >= 640);
    assert!(height >= 400);
    assert_eq!(init_head[4], 32);
    assert_eq!(init_head[5], 24);
    assert_eq!(init_head[6], 0);
    assert_eq!(init_head[7], 1);
    assert!(name_len > 0);
    let mut name = vec![0_u8; name_len as usize];
    stream.read_exact(&mut name).expect("read desktop name");
    assert!(String::from_utf8(name)
        .expect("desktop name is utf8")
        .contains("LL Remote"));

    // The native Core advertises 32bpp/depth-24 and consumes Raw pixels as 32-bit BGRA values.
    stream
        .write_all(&[
            0, // SetPixelFormat
            0, 0, 0, // padding
            32, 24, 0, 1, // bpp, depth, little-endian, true-color
            0, 255, 0, 255, 0, 255, // max values
            0, 8, 16, // red, green, blue shifts
            0, 0, 0, // padding
        ])
        .expect("write Core pixel format");

    let encodings = [
        0_i32.to_be_bytes(),
        (-223_i32).to_be_bytes(),
        (-224_i32).to_be_bytes(),
        (-307_i32).to_be_bytes(),
        (-308_i32).to_be_bytes(),
        0xc0a1e5ce_u32.to_be_bytes(),
    ]
    .concat();
    let encoding_count = (encodings.len() / 4) as u16;
    let encoding_count_bytes = encoding_count.to_be_bytes();
    stream
        .write_all(&[2, 0, encoding_count_bytes[0], encoding_count_bytes[1]])
        .expect("write SetEncodings header");
    stream
        .write_all(&encodings)
        .expect("write native Core SetEncodings");

    let mut caps = [0_u8; 16];
    stream
        .read_exact(&mut caps)
        .expect("read Extended Clipboard caps");
    assert_eq!(caps[0], 3);
    assert_eq!(i32::from_be_bytes([caps[4], caps[5], caps[6], caps[7]]), -8);

    let client_clipboard_caps = 0x1f00_0001_u32;
    stream
        .write_all(&[6, 0, 0, 0])
        .expect("write ClientCutText caps header");
    stream
        .write_all(&(-8_i32).to_be_bytes())
        .expect("write ClientCutText caps length");
    stream
        .write_all(&client_clipboard_caps.to_be_bytes())
        .expect("write ClientCutText caps flags");
    stream
        .write_all(&0_u32.to_be_bytes())
        .expect("write ClientCutText caps text size");

    // Capture is asynchronous and may be unavailable in a headless/unauthorized
    // process. An unavailable capture leaves the request pending without fake pixels.
    let capture_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let diagnostics = manager.status().unwrap().frame_diagnostics;
        if diagnostics.capture_status != LocalVncServerCaptureStatus::Waiting {
            break;
        }
        assert!(
            Instant::now() < capture_deadline,
            "capture worker did not report a result"
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    // FramebufferUpdateRequest: incremental=false, x=0, y=0, w=framebuffer width/height.
    let width_bytes = width.to_be_bytes();
    let height_bytes = height.to_be_bytes();
    stream
        .write_all(&[
            3,
            0,
            0,
            0,
            0,
            0,
            width_bytes[0],
            width_bytes[1],
            height_bytes[0],
            height_bytes[1],
        ])
        .expect("write framebuffer update request");

    let mut update_head = [0_u8; 16];
    if let Err(error) = stream.read_exact(&mut update_head[..4]) {
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ));
        let diagnostics = manager.status().unwrap().frame_diagnostics;
        assert_eq!(diagnostics.capture_frames, 0);
        assert!(diagnostics.capture_failures > 0);
        assert_ne!(
            diagnostics.capture_status,
            LocalVncServerCaptureStatus::Ready
        );
        assert_eq!(diagnostics.last_frame_age_ms, None);
        eprintln!(
            "capture unavailable: {:?}; no real-frame acceptance",
            diagnostics.capture_status
        );
        manager.stop().expect("stop unavailable capture server");
        return;
    }
    assert_eq!(update_head[0], 0);
    stream
        .read_exact(&mut update_head[4..])
        .expect("read captured rectangle header");
    assert_eq!(u16::from_be_bytes([update_head[2], update_head[3]]), 1);
    assert_eq!(u16::from_be_bytes([update_head[8], update_head[9]]), width);
    assert_eq!(
        u16::from_be_bytes([update_head[10], update_head[11]]),
        height
    );
    assert_eq!(
        i32::from_be_bytes([
            update_head[12],
            update_head[13],
            update_head[14],
            update_head[15]
        ]),
        0
    );

    let mut framebuffer = vec![0_u8; width as usize * height as usize * 4];
    stream
        .read_exact(&mut framebuffer)
        .expect("read framebuffer payload");
    // Black pixels are valid captured content; success is proved by publication,
    // not by assuming a specific color at the corner of the user's desktop.
    assert!(manager.status().unwrap().frame_diagnostics.capture_frames > 0);

    stream
        .write_all(&[
            3,
            1,
            0,
            0,
            0,
            0,
            width_bytes[0],
            width_bytes[1],
            height_bytes[0],
            height_bytes[1],
        ])
        .expect("write incremental framebuffer update request");
    let mut next_message_type = [0_u8; 1];
    match stream.read_exact(&mut next_message_type) {
        Ok(()) => assert!(matches!(next_message_type[0], 0 | 3)),
        Err(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        )),
    }
    // A static desktop may keep an incremental request pending indefinitely.
    assert!(
        manager
            .status()
            .unwrap()
            .frame_diagnostics
            .framebuffer_update_requests
            >= 2
    );

    manager.stop().expect("local VNC server should stop");
}

#[test]
fn local_vnc_server_ignores_unsupported_set_desktop_size_without_disconnect() {
    let manager = LocalVncServerManager::default();
    let overview = manager
        .start(start_request(0))
        .expect("local VNC server should start");
    let mut stream =
        TcpStream::connect(("127.0.0.1", overview.port)).expect("connect local VNC server");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("set write timeout");

    let mut banner = [0_u8; 12];
    stream.read_exact(&mut banner).expect("read server version");
    stream
        .write_all(b"RFB 003.008\n")
        .expect("write client version");
    let mut security_types = [0_u8; 2];
    stream
        .read_exact(&mut security_types)
        .expect("read security types");
    stream.write_all(&[2]).expect("select VNC auth");
    let mut challenge = [0_u8; 16];
    stream.read_exact(&mut challenge).expect("read challenge");
    let response = create_vnc_auth_response("temporary-secret", &challenge);
    stream.write_all(&response).expect("write auth response");
    let mut security_result = [0_u8; 4];
    stream
        .read_exact(&mut security_result)
        .expect("read security result");
    stream.write_all(&[1]).expect("write client init");

    let mut init_head = [0_u8; 24];
    stream.read_exact(&mut init_head).expect("read server init");
    let width = u16::from_be_bytes([init_head[0], init_head[1]]);
    let height = u16::from_be_bytes([init_head[2], init_head[3]]);
    let name_len = u32::from_be_bytes([init_head[20], init_head[21], init_head[22], init_head[23]]);
    let mut name = vec![0_u8; name_len as usize];
    stream.read_exact(&mut name).expect("read desktop name");

    let width_bytes = width.to_be_bytes();
    let height_bytes = height.to_be_bytes();
    let mut set_desktop_size = vec![
        251,
        0,
        width_bytes[0],
        width_bytes[1],
        height_bytes[0],
        height_bytes[1],
        1,
        0,
    ];
    set_desktop_size.extend_from_slice(&0_u32.to_be_bytes());
    set_desktop_size.extend_from_slice(&0_u16.to_be_bytes());
    set_desktop_size.extend_from_slice(&0_u16.to_be_bytes());
    set_desktop_size.extend_from_slice(&width.to_be_bytes());
    set_desktop_size.extend_from_slice(&height.to_be_bytes());
    set_desktop_size.extend_from_slice(&0_u32.to_be_bytes());
    stream
        .write_all(&set_desktop_size)
        .expect("write SetDesktopSize");

    // A control response proves the next message remains aligned even when
    // native capture is unavailable to this command-line test process.
    stream
        .write_all(&[
            150,
            0,
            0,
            0,
            0,
            0,
            width_bytes[0],
            width_bytes[1],
            height_bytes[0],
            height_bytes[1],
        ])
        .expect("disable continuous updates after unsupported SetDesktopSize");
    let mut reply = [0];
    stream
        .read_exact(&mut reply)
        .expect("read EndOfContinuousUpdates");
    assert_eq!(reply, [150]);

    manager.stop().expect("local VNC server should stop");
}

#[test]
fn local_vnc_server_rejects_wrong_vnc_auth_password() {
    let manager = LocalVncServerManager::default();
    let overview = manager
        .start(start_request(0))
        .expect("local VNC server should start");
    let mut stream =
        TcpStream::connect(("127.0.0.1", overview.port)).expect("connect local VNC server");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set read timeout");

    let mut banner = [0_u8; 12];
    stream.read_exact(&mut banner).expect("read server version");
    stream
        .write_all(b"RFB 003.008\n")
        .expect("write client version");
    let mut security_types = [0_u8; 2];
    stream
        .read_exact(&mut security_types)
        .expect("read security types");
    stream.write_all(&[2]).expect("select VNC auth");
    let mut challenge = [0_u8; 16];
    stream.read_exact(&mut challenge).expect("read challenge");
    let response = create_vnc_auth_response("wrong-password", &challenge);
    stream.write_all(&response).expect("write auth response");

    let mut security_result = [0_u8; 4];
    stream
        .read_exact(&mut security_result)
        .expect("read security result");
    assert_eq!(u32::from_be_bytes(security_result), 1);
    let mut reason_len = [0_u8; 4];
    stream
        .read_exact(&mut reason_len)
        .expect("read reason length");
    let reason_len = u32::from_be_bytes(reason_len);
    let mut reason = vec![0_u8; reason_len as usize];
    stream
        .read_exact(&mut reason)
        .expect("read auth failure reason");
    assert_eq!(
        String::from_utf8(reason).expect("reason utf8"),
        "Authentication failed"
    );

    manager.stop().expect("local VNC server should stop");
}

#[test]
fn local_vnc_server_rejects_occupied_port_without_leaking_password() {
    let reserved = TcpListener::bind("127.0.0.1:0").expect("reserve local port");
    let port = reserved.local_addr().expect("local addr").port();
    let manager = LocalVncServerManager::default();

    let error = manager
        .start(start_request(port))
        .expect_err("occupied port should fail");

    assert!(error.contains("port_conflict"));
    assert!(!error.contains("temporary-secret"));
    assert_eq!(
        manager.status().expect("status should be available").status,
        LocalVncServerStatus::Stopped
    );
}

#[test]
fn local_vnc_server_requires_temporary_password() {
    let manager = LocalVncServerManager::default();
    let mut request = start_request(0);
    request.password = "  ".to_string();

    let error = manager
        .start(request)
        .expect_err("empty temporary password should fail");

    assert!(error.contains("password_required"));
    assert_eq!(
        manager.status().expect("status should be available").status,
        LocalVncServerStatus::Stopped
    );
}
