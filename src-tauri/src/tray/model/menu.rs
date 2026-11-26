pub fn build_tray_menu_model(snapshot: &TrayStateSnapshot) -> TrayMenuModel {
    let summary = if snapshot.menu_summary.is_empty() {
        fallback_summary(snapshot)
    } else {
        snapshot.menu_summary.clone()
    };
    let mut items = Vec::new();

    let action = match snapshot.local_share_action {
        TrayLocalShareAction::Idle => match snapshot.local_share_status {
            TrayLocalShareStatus::Starting => TrayLocalShareAction::Starting,
            TrayLocalShareStatus::Stopping => TrayLocalShareAction::Stopping,
            _ => TrayLocalShareAction::Idle,
        },
        action => action,
    };

    if action != TrayLocalShareAction::Idle {
        let (kind, label) = match action {
            TrayLocalShareAction::Starting => {
                (TrayMenuItemKind::StartLocalShare, "正在启动本机共享…")
            }
            TrayLocalShareAction::Stopping => {
                (TrayMenuItemKind::StopLocalShare, "正在停止本机共享…")
            }
            TrayLocalShareAction::AwaitingPermission => {
                (TrayMenuItemKind::StartLocalShare, "等待权限确认")
            }
            TrayLocalShareAction::AwaitingPassword => {
                (TrayMenuItemKind::StartLocalShare, "等待输入密码")
            }
            _ => (TrayMenuItemKind::StartLocalShare, "正在处理本机共享…"),
        };
        let mut pending = item(kind, label);
        pending.enabled = false;
        items.push(pending);
        items.push(item(
            TrayMenuItemKind::OpenLocalShare,
            match action {
                TrayLocalShareAction::AwaitingPermission => "继续权限设置…",
                TrayLocalShareAction::AwaitingPassword => "输入共享密码…",
                _ => "打开本机共享…",
            },
        ));
    } else if snapshot.local_share_status.is_error() {
        items.push(item(TrayMenuItemKind::OpenLocalShare, "查看本机共享异常"));
    } else if snapshot.local_share_running {
        items.push(item(TrayMenuItemKind::StopLocalShare, "停止本机共享"));
        if !snapshot.active_local_connections.is_empty() {
            items.push(TrayMenuItemModel {
                kind: TrayMenuItemKind::ActiveConnections,
                label: "活动连接".to_string(),
                payload: None,
                children: snapshot
                    .active_local_connections
                    .iter()
                    .map(|connection| TrayMenuItemModel {
                        kind: TrayMenuItemKind::DisconnectLocalConnection,
                        label: format!("断开 {} ({})", connection.label, connection.detail),
                        payload: Some(connection.id.clone()),
                        children: Vec::new(),
                        enabled: true,
                    })
                    .collect(),
                enabled: true,
            });
        } else {
            items.push(item(TrayMenuItemKind::OpenLocalShare, "查看活动连接"));
        }
        items.push(item(TrayMenuItemKind::CopyLocalShare, "复制连接信息"));
    } else {
        items.push(item(TrayMenuItemKind::StartLocalShare, "启动本机共享"));
        items.push(item(TrayMenuItemKind::OpenLocalShare, "打开本机共享..."));
    }

    items.push(item(TrayMenuItemKind::OpenMain, "打开主窗口"));
    items.push(item(TrayMenuItemKind::OpenSettings, "设置"));

    items.push(item(
        TrayMenuItemKind::Quit,
        if snapshot.local_share_running {
            "退出应用（共享继续）"
        } else {
            "退出应用"
        },
    ));

    TrayMenuModel { summary, items }
}
