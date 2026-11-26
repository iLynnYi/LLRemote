pub fn validate_required_capture_permissions(
    permissions: &[LocalVncServerPermission],
) -> Result<(), String> {
    let screen_recording_missing = permissions.iter().any(|permission| {
        permission.kind == LocalVncServerPermissionKind::ScreenRecording
            && permission.status == LocalVncServerPermissionStatus::Missing
    });

    if screen_recording_missing {
        return Err(
            "permission_missing: screen_recording permission is required for screen capture"
                .to_string(),
        );
    }

    Ok(())
}

fn resolve_shared_display(
    display_id: Option<&str>,
    resolution_mode: LocalVncServerResolutionMode,
    refresh_mode: LocalVncServerRefreshMode,
) -> SharedDisplay {
    let displays = list_local_vnc_server_displays();
    let selected = display_id
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| displays.iter().find(|display| display.id == value))
        .or_else(|| displays.iter().find(|display| display.is_primary))
        .or_else(|| displays.first());

    selected
        .map(|display| shared_display_from_overview(display, resolution_mode, refresh_mode))
        .unwrap_or_else(SharedDisplay::fallback_main)
}

fn shared_display_from_overview(
    display: &LocalVncServerDisplay,
    resolution_mode: LocalVncServerResolutionMode,
    refresh_mode: LocalVncServerRefreshMode,
) -> SharedDisplay {
    SharedDisplay {
        relay: None,
        revision: 0,
        desktop_epoch: 0,
        id: display.id.clone(),
        label: display.label.clone(),
        frame: DisplayFrame {
            x: display.x,
            y: display.y,
            width: display.width,
            height: display.height,
            pixel_width: display.pixel_width,
            pixel_height: display.pixel_height,
        },
        framebuffer: framebuffer_size_for_resolution(
            display.pixel_width,
            display.pixel_height,
            resolution_mode,
        ),
        resolution_mode,
        refresh_mode,
    }
}

fn validate_start_request(request: &LocalVncServerStartRequest) -> Result<(), String> {
    if request.password.trim().is_empty() {
        return Err("password_required: VNC access password is required".to_string());
    }

    Ok(())
}

fn normalize_bind_address(bind_address: &str) -> String {
    let trimmed = bind_address.trim();

    if trimmed.is_empty() {
        return "0.0.0.0".to_string();
    }

    trimmed.to_string()
}
