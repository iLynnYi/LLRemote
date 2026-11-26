use LLRemoteLib::local_agent_service::{LocalAgentOverview, LocalAgentStatus};
use LLRemoteLib::local_vnc_server::{
    LocalVncServerActiveConnection, LocalVncServerConnectionKind, LocalVncServerConnectionStatus,
    LocalVncServerFrameDiagnostics, LocalVncServerOverview, LocalVncServerRefreshMode,
    LocalVncServerResolutionMode, LocalVncServerStatus,
};
use LLRemoteLib::server::{LocalAddress, LocalAddressKind};
use LLRemoteLib::tray::{
    build_local_share_tray_snapshot, build_tray_menu_model, TrayIconState, TrayLocalConnection,
    TrayLocalShareAction, TrayLocalShareStatus, TrayMenuItemKind, TrayStateSnapshot,
};

#[test]
fn tray_snapshot_accepts_frontend_active_icon_state_values() {
    let payload = serde_json::json!({
        "status": "sharingActive",
        "iconState": "sharing-active-1",
        "localShareRunning": true,
        "localShareStatus": "running",
        "activeLocalConnectionCount": 1,
        "activeSessionCount": 0,
        "failedSessionCount": 0,
        "port": 5903,
        "tooltip": "LL Remote",
        "menuSummary": [],
        "activeLocalConnections": [],
        "canStartLocalShare": true
    });

    let snapshot: TrayStateSnapshot =
        serde_json::from_value(payload.clone()).expect("frontend tray snapshot should deserialize");

    assert_eq!(snapshot.icon_state, TrayIconState::SharingActive1);
    assert_eq!(snapshot.local_share_action, TrayLocalShareAction::Idle);

    let mut pending_payload = payload;
    pending_payload["localShareAction"] = serde_json::json!("awaiting_permission");
    let pending: TrayStateSnapshot = serde_json::from_value(pending_payload).unwrap();
    assert_eq!(
        pending.local_share_action,
        TrayLocalShareAction::AwaitingPermission
    );
}

fn snapshot(status: TrayLocalShareStatus, connections: u32) -> TrayStateSnapshot {
    TrayStateSnapshot {
        status: if status.is_error() {
            "error".to_string()
        } else if status == TrayLocalShareStatus::Running && connections > 0 {
            "sharingActive".to_string()
        } else if status == TrayLocalShareStatus::Running {
            "sharing".to_string()
        } else {
            "idle".to_string()
        },
        icon_state: TrayIconState::Idle,
        local_share_running: status == TrayLocalShareStatus::Running,
        local_share_status: status,
        active_local_connection_count: connections,
        active_session_count: 1,
        failed_session_count: 0,
        port: Some(5903),
        tooltip: "LL Remote".to_string(),
        menu_summary: Vec::new(),
        active_local_connections: if connections > 0 {
            vec![TrayLocalConnection {
                id: "device-1".to_string(),
                label: "MacBook Pro".to_string(),
                detail: "192.168.1.22".to_string(),
            }]
        } else {
            Vec::new()
        },
        can_start_local_share: true,
        local_share_action: TrayLocalShareAction::Idle,
    }
}

#[test]
fn tray_menu_model_starts_with_status_summary() {
    let model = build_tray_menu_model(&snapshot(TrayLocalShareStatus::Running, 2));

    assert_eq!(
        model.summary,
        vec![
            "● 本机共享：已启动",
            "活动连接：2 台",
            "监听端口：5903",
            "远程会话：1 个",
        ]
    );
    assert!(model
        .items
        .iter()
        .any(|item| item.kind == TrayMenuItemKind::StopLocalShare));
    assert!(model
        .items
        .iter()
        .any(|item| item.kind == TrayMenuItemKind::CopyLocalShare));
    let active_connections = model
        .items
        .iter()
        .find(|item| item.kind == TrayMenuItemKind::ActiveConnections)
        .expect("active connections submenu should exist");
    assert_eq!(active_connections.children.len(), 1);
    assert_eq!(
        active_connections.children[0].kind,
        TrayMenuItemKind::DisconnectLocalConnection
    );
    assert_eq!(
        active_connections.children[0].payload.as_deref(),
        Some("device-1")
    );
}

#[test]
fn tray_menu_model_direct_start_when_local_share_is_stopped() {
    let model = build_tray_menu_model(&snapshot(TrayLocalShareStatus::Stopped, 0));

    assert_eq!(model.summary, vec!["○ 本机共享：未启动", "远程会话：1 个",]);
    assert!(model
        .items
        .iter()
        .any(|item| item.kind == TrayMenuItemKind::StartLocalShare));
}

#[test]
fn tray_menu_model_routes_errors_to_existing_local_share_view() {
    let model = build_tray_menu_model(&snapshot(TrayLocalShareStatus::PortConflict, 2));

    assert_eq!(model.summary, vec!["! 本机共享：端口占用", "需要处理",]);
    assert!(model
        .items
        .iter()
        .any(|item| item.kind == TrayMenuItemKind::OpenLocalShare));
    assert!(model
        .items
        .iter()
        .any(|item| item.label == "查看本机共享异常"));
    assert!(!model
        .items
        .iter()
        .any(|item| item.kind == TrayMenuItemKind::StartLocalShare));
}

#[test]
fn tray_menu_disables_lifecycle_actions_while_starting_or_stopping() {
    for (status, label) in [
        (TrayLocalShareStatus::Starting, "正在启动本机共享…"),
        (TrayLocalShareStatus::Stopping, "正在停止本机共享…"),
    ] {
        let model = build_tray_menu_model(&snapshot(status, 0));
        assert!(model.items.iter().any(|item| item.label == label));
        assert!(model
            .items
            .iter()
            .any(|item| item.kind == TrayMenuItemKind::OpenLocalShare));
        assert!(!model.items.iter().any(|item| {
            item.label == "启动本机共享" || item.label == "停止本机共享"
        }));
        assert!(!model.items.iter().any(|item| {
            matches!(
                item.kind,
                TrayMenuItemKind::StartLocalShare | TrayMenuItemKind::StopLocalShare
            ) && item.enabled
        }));
    }
}

#[test]
fn tray_quit_explains_that_running_sharing_continues() {
    for (status, label) in [
        (TrayLocalShareStatus::Running, "退出应用（共享继续）"),
        (TrayLocalShareStatus::Stopped, "退出应用"),
    ] {
        let model = build_tray_menu_model(&snapshot(status, 0));
        let quit = model
            .items
            .iter()
            .find(|item| item.kind == TrayMenuItemKind::Quit)
            .unwrap();
        assert_eq!(quit.label, label);
    }
}

#[test]
fn tray_menu_disables_actions_during_frontend_intents_before_runtime_status_changes() {
    for (status, action, label, open_label) in [
        (
            TrayLocalShareStatus::Stopped,
            TrayLocalShareAction::Starting,
            "正在启动本机共享…",
            "打开本机共享…",
        ),
        (
            TrayLocalShareStatus::Running,
            TrayLocalShareAction::Stopping,
            "正在停止本机共享…",
            "打开本机共享…",
        ),
        (
            TrayLocalShareStatus::Stopped,
            TrayLocalShareAction::AwaitingPermission,
            "等待权限确认",
            "继续权限设置…",
        ),
        (
            TrayLocalShareStatus::Stopped,
            TrayLocalShareAction::AwaitingPassword,
            "等待输入密码",
            "输入共享密码…",
        ),
        (
            TrayLocalShareStatus::Running,
            TrayLocalShareAction::Busy,
            "正在处理本机共享…",
            "打开本机共享…",
        ),
    ] {
        let mut current = snapshot(status, 0);
        current.local_share_action = action;
        let model = build_tray_menu_model(&current);
        let pending = model.items.iter().find(|item| item.label == label).unwrap();
        assert!(!pending.enabled);
        let open = model
            .items
            .iter()
            .find(|item| item.kind == TrayMenuItemKind::OpenLocalShare)
            .unwrap();
        assert!(open.enabled);
        assert_eq!(open.label, open_label);
        assert!(!model.items.iter().any(|item| {
            matches!(
                item.kind,
                TrayMenuItemKind::StartLocalShare | TrayMenuItemKind::StopLocalShare
            ) && item.enabled
        }));
    }
}

#[test]
fn local_share_tray_snapshot_uses_backend_connection_count() {
    let snapshot = build_local_share_tray_snapshot(&local_share_overview(
        LocalVncServerStatus::Running,
        vec![LocalVncServerActiveConnection {
            id: "device-1".to_string(),
            kind: LocalVncServerConnectionKind::Rfb,
            device_name: None,
            platform: None,
            ip_address: Some("192.168.64.1".to_string()),
            connected_at: "1782813600".to_string(),
            status: LocalVncServerConnectionStatus::Online,
        }],
    ));

    assert_eq!(snapshot.icon_state, TrayIconState::SharingActive1);
    assert_eq!(snapshot.active_local_connection_count, 1);
    assert_eq!(
        snapshot.menu_summary,
        vec!["● 本机共享：已启动", "活动连接：1 台", "监听端口：5903",]
    );
    assert_eq!(snapshot.active_local_connections[0].label, "未知设备");
    assert_eq!(snapshot.active_local_connections[0].detail, "192.168.64.1");
}

fn local_share_overview(
    status: LocalVncServerStatus,
    active_connection_devices: Vec<LocalVncServerActiveConnection>,
) -> LocalVncServerOverview {
    LocalVncServerOverview {
        status,
        bind_address: "0.0.0.0".to_string(),
        port: 5903,
        addresses: vec![LocalAddress {
            label: "en0".to_string(),
            address: "192.168.64.6".to_string(),
            kind: LocalAddressKind::LanCandidate,
        }],
        displays: Vec::new(),
        selected_display_id: "main".to_string(),
        selected_display_label: "Main".to_string(),
        resolution_mode: LocalVncServerResolutionMode::Fit,
        refresh_mode: LocalVncServerRefreshMode::Balanced,
        framebuffer_width: 1280,
        framebuffer_height: 800,
        security_type: "vnc_auth".to_string(),
        view_only: false,
        active_connections: active_connection_devices.len(),
        active_connection_devices,
        permissions: Vec::new(),
        agent: LocalAgentOverview {
            status: LocalAgentStatus::Running,
            bind_address: "0.0.0.0".to_string(),
            port: Some(6903),
            capabilities: Vec::new(),
            last_error: None,
            updated_at: "1782813601".to_string(),
        },
        frame_diagnostics: LocalVncServerFrameDiagnostics {
            capture_frames: 0,
            framebuffer_update_requests: 0,
            framebuffer_updates: 0,
            last_capture_duration_ms: 0,
            last_frame_age_ms: None,
            last_diff_duration_ms: 0,
            last_write_duration_ms: 0,
            last_dirty_rects: 0,
            last_framebuffer_encoding: "-".to_string(),
            last_raw_bytes_written: 0,
            last_encoded_bytes_written: 0,
            last_bytes_written: 0,
            ..LocalVncServerFrameDiagnostics::default()
        },
        started_at: Some("1782813600".to_string()),
        updated_at: "1782813601".to_string(),
    }
}
