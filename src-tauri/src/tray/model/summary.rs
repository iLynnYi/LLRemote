fn fallback_summary(snapshot: &TrayStateSnapshot) -> Vec<String> {
    if snapshot.local_share_status.is_error() {
        return vec![
            format!(
                "! 本机共享：{}",
                local_share_status_label(snapshot.local_share_status)
            ),
            "需要处理".to_string(),
        ];
    }

    if !snapshot.local_share_running {
        let mut summary = vec![format!(
            "○ 本机共享：{}",
            local_share_status_label(snapshot.local_share_status)
        )];
        if snapshot.active_session_count > 0 {
            summary.push(format!("远程会话：{} 个", snapshot.active_session_count));
        }
        return summary;
    }

    let mut summary = vec![
        "● 本机共享：已启动".to_string(),
        format!("活动连接：{} 台", snapshot.active_local_connection_count),
    ];

    if let Some(port) = snapshot.port {
        summary.push(format!("监听端口：{port}"));
    }
    if snapshot.active_session_count > 0 {
        summary.push(format!("远程会话：{} 个", snapshot.active_session_count));
    }

    summary
}

fn local_share_status_label(status: TrayLocalShareStatus) -> &'static str {
    match status {
        TrayLocalShareStatus::Stopped => "未启动",
        TrayLocalShareStatus::Starting => "启动中",
        TrayLocalShareStatus::Running => "已启动",
        TrayLocalShareStatus::Stopping => "停止中",
        TrayLocalShareStatus::PermissionMissing => "权限缺失",
        TrayLocalShareStatus::PortConflict => "端口占用",
        TrayLocalShareStatus::Failed => "启动失败",
        TrayLocalShareStatus::Unknown => "状态未知",
    }
}
