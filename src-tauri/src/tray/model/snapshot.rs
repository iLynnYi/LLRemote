pub fn build_local_share_tray_snapshot(overview: &LocalVncServerOverview) -> TrayStateSnapshot {
    let local_share_status = tray_local_share_status(overview.status.clone());
    let active_connection_count = overview.active_connections as u32;
    let icon_state = tray_icon_state(local_share_status, active_connection_count);
    let menu_summary = local_share_menu_summary(
        local_share_status,
        active_connection_count,
        Some(overview.port),
    );

    TrayStateSnapshot {
        status: tray_status(local_share_status, active_connection_count),
        icon_state,
        local_share_running: local_share_status == TrayLocalShareStatus::Running,
        local_share_status,
        active_local_connection_count: active_connection_count,
        active_session_count: 0,
        failed_session_count: 0,
        port: Some(overview.port),
        tooltip: ["LL Remote".to_string(), menu_summary.join("\n")].join("\n"),
        menu_summary,
        active_local_connections: overview
            .active_connection_devices
            .iter()
            .map(tray_local_connection)
            .collect(),
        can_start_local_share: false,
        local_share_action: TrayLocalShareAction::Idle,
    }
}

fn tray_status(status: TrayLocalShareStatus, active_connection_count: u32) -> String {
    if status.is_error() {
        "error".to_string()
    } else if status == TrayLocalShareStatus::Running && active_connection_count > 0 {
        "sharingActive".to_string()
    } else if status == TrayLocalShareStatus::Running {
        "sharing".to_string()
    } else {
        "idle".to_string()
    }
}

fn tray_icon_state(status: TrayLocalShareStatus, active_connection_count: u32) -> TrayIconState {
    if status.is_error() {
        return TrayIconState::Error;
    }

    if status != TrayLocalShareStatus::Running {
        return TrayIconState::Idle;
    }

    match active_connection_count {
        0 => TrayIconState::Sharing,
        1 => TrayIconState::SharingActive1,
        2 => TrayIconState::SharingActive2,
        3 => TrayIconState::SharingActive3,
        _ => TrayIconState::SharingActive9Plus,
    }
}

fn tray_local_share_status(status: LocalVncServerStatus) -> TrayLocalShareStatus {
    match status {
        LocalVncServerStatus::Stopped => TrayLocalShareStatus::Stopped,
        LocalVncServerStatus::Starting => TrayLocalShareStatus::Starting,
        LocalVncServerStatus::Running => TrayLocalShareStatus::Running,
        LocalVncServerStatus::Stopping => TrayLocalShareStatus::Stopping,
        LocalVncServerStatus::PermissionMissing => TrayLocalShareStatus::PermissionMissing,
        LocalVncServerStatus::PortConflict => TrayLocalShareStatus::PortConflict,
        LocalVncServerStatus::Failed => TrayLocalShareStatus::Failed,
        LocalVncServerStatus::Unknown => TrayLocalShareStatus::Unknown,
    }
}

fn local_share_menu_summary(
    status: TrayLocalShareStatus,
    active_connection_count: u32,
    port: Option<u16>,
) -> Vec<String> {
    if status.is_error() {
        return vec![
            format!("! 本机共享：{}", local_share_status_label(status)),
            "需要处理".to_string(),
        ];
    }

    if status != TrayLocalShareStatus::Running {
        return vec![format!("○ 本机共享：{}", local_share_status_label(status))];
    }

    let mut summary = vec![
        "● 本机共享：已启动".to_string(),
        format!("活动连接：{active_connection_count} 台"),
    ];
    if let Some(port) = port {
        summary.push(format!("监听端口：{port}"));
    }
    summary
}

fn tray_local_connection(connection: &LocalVncServerActiveConnection) -> TrayLocalConnection {
    let detail = connection
        .ip_address
        .clone()
        .unwrap_or_else(|| "未知地址".to_string());

    TrayLocalConnection {
        id: connection.id.clone(),
        label: connection
            .device_name
            .clone()
            .unwrap_or_else(|| "未知设备".to_string()),
        detail,
    }
}
