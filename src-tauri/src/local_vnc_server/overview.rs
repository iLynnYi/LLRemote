impl LocalVncServerRuntime {
    fn overview(&self, status: LocalVncServerStatus) -> LocalVncServerOverview {
        self.overview_with_permissions(status, collect_local_vnc_server_permissions())
    }

    fn overview_with_permissions(
        &self,
        status: LocalVncServerStatus,
        permissions: Vec<LocalVncServerPermission>,
    ) -> LocalVncServerOverview {
        let active_connection_devices = self.connections.list();
        let active_connections = active_connection_devices.len();

        LocalVncServerOverview {
            status,
            bind_address: self.bind_address.clone(),
            port: self.port,
            addresses: local_address_candidates(),
            displays: self
                .shared_display
                .read()
                .map(|display| display.source_displays())
                .unwrap_or_default(),
            selected_display_id: self
                .shared_display
                .read()
                .map(|display| display.id.clone())
                .unwrap_or_else(|_| "main".to_string()),
            selected_display_label: self
                .shared_display
                .read()
                .map(|display| display.label.clone())
                .unwrap_or_else(|_| "Main display".to_string()),
            resolution_mode: self
                .shared_display
                .read()
                .map(|display| display.resolution_mode)
                .unwrap_or(LocalVncServerResolutionMode::Fit),
            refresh_mode: self
                .shared_display
                .read()
                .map(|display| display.refresh_mode)
                .unwrap_or(DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE),
            framebuffer_width: self
                .shared_display
                .read()
                .map(|display| display.framebuffer.width)
                .unwrap_or(DEFAULT_FRAMEBUFFER_WIDTH as u32),
            framebuffer_height: self
                .shared_display
                .read()
                .map(|display| display.framebuffer.height)
                .unwrap_or(DEFAULT_FRAMEBUFFER_HEIGHT as u32),
            security_type: "vnc_auth".to_string(),
            view_only: self.view_only,
            active_connections,
            active_connection_devices,
            permissions,
            agent: self
                .agent
                .as_ref()
                .map(|agent| agent.overview())
                .unwrap_or_else(|| self.agent_overview.clone()),
            frame_diagnostics: self.frame_cache.diagnostics(),
            started_at: Some(self.started_at.clone()),
            updated_at: now_unix_seconds(),
        }
    }
}
