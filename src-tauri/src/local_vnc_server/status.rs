fn stopped_overview() -> LocalVncServerOverview {
    stopped_overview_with_permissions(collect_local_vnc_server_permissions())
}

/// An absent capture process has no permission result to report.
pub(crate) fn pending_local_vnc_server_overview() -> LocalVncServerOverview {
    stopped_overview_with_permissions(permissions::pending_local_vnc_server_permissions())
}

fn stopped_overview_with_permissions(
    permissions: Vec<LocalVncServerPermission>,
) -> LocalVncServerOverview {
    let shared_display = resolve_shared_display(
        None,
        default_local_vnc_server_resolution_mode(),
        DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE,
    );

    LocalVncServerOverview {
        status: LocalVncServerStatus::Stopped,
        bind_address: "0.0.0.0".to_string(),
        port: 5903,
        addresses: local_address_candidates(),
        displays: list_local_vnc_server_displays(),
        selected_display_id: shared_display.id,
        selected_display_label: shared_display.label,
        resolution_mode: shared_display.resolution_mode,
        refresh_mode: shared_display.refresh_mode,
        framebuffer_width: shared_display.framebuffer.width,
        framebuffer_height: shared_display.framebuffer.height,
        security_type: "vnc_auth".to_string(),
        view_only: false,
        active_connections: 0,
        active_connection_devices: Vec::new(),
        permissions,
        agent: stopped_agent_overview(),
        frame_diagnostics: empty_frame_diagnostics(),
        started_at: None,
        updated_at: now_unix_seconds(),
    }
}

fn empty_frame_diagnostics() -> LocalVncServerFrameDiagnostics {
    LocalVncServerFrameDiagnostics {
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
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn now_unix_seconds() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
