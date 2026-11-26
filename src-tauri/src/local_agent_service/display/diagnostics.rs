fn agent_display_inventory() -> Vec<AgentDisplayInventoryItem> {
    list_local_vnc_server_displays()
        .into_iter()
        .map(|display| AgentDisplayInventoryItem {
            id: display.id,
            label: display.label,
            is_primary: display.is_primary,
            x: display.x,
            y: display.y,
            width: display.width,
            height: display.height,
            pixel_width: display.pixel_width,
            pixel_height: display.pixel_height,
            scale_factor: display.scale_factor,
        })
        .collect()
}

fn agent_permission_diagnostics(inbox_dir: &Path) -> Vec<AgentPermissionDiagnosticItem> {
    let mut items = vec![
        AgentPermissionDiagnosticItem {
            kind: "agent_service".to_string(),
            status: "granted".to_string(),
            required_for: "agent.lifecycle".to_string(),
            message: "Agent service is reachable and authenticated.".to_string(),
        },
        AgentPermissionDiagnosticItem {
            kind: "clipboard".to_string(),
            status: "granted".to_string(),
            required_for: CLIPBOARD_TEXT_WRITE.to_string(),
            message: "Agent clipboard write command is available.".to_string(),
        },
        file_system_permission_item(inbox_dir),
    ];

    for permission in get_local_vnc_server_permissions() {
        let kind = match permission.kind {
            LocalVncServerPermissionKind::ScreenRecording => "screen_recording",
            LocalVncServerPermissionKind::Accessibility => "accessibility",
        };
        items.push(AgentPermissionDiagnosticItem {
            kind: kind.to_string(),
            status: serde_json::to_value(permission.status)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
            required_for: permission.required_for,
            message: format!("{kind} permission state reported by target Agent."),
        });
    }

    items
}

fn file_system_permission_item(inbox_dir: &Path) -> AgentPermissionDiagnosticItem {
    match fs::create_dir_all(inbox_dir) {
        Ok(()) => AgentPermissionDiagnosticItem {
            kind: "file_system".to_string(),
            status: "granted".to_string(),
            required_for: FILE_TRANSFER_UPLOAD.to_string(),
            message: "Agent receive directory is writable.".to_string(),
        },
        Err(error) => AgentPermissionDiagnosticItem {
            kind: "file_system".to_string(),
            status: "missing".to_string(),
            required_for: FILE_TRANSFER_UPLOAD.to_string(),
            message: format!("Agent receive directory is not writable: {error}"),
        },
    }
}
