impl LocalVncServerManager {
    #[cfg(target_os = "windows")]
    pub(crate) fn is_running(&self) -> bool {
        self.runtime
            .lock()
            .is_ok_and(|state| state.running().is_some())
    }
    const DEFAULT_AGENT_PORT: u16 = 6200;

    pub fn start(
        &self,
        request: LocalVncServerStartRequest,
    ) -> Result<LocalVncServerOverview, String> {
        self.start_with_active_connections_callback(request, None)
    }

    pub fn start_with_active_connections_callback(
        &self,
        request: LocalVncServerStartRequest,
        active_connections_changed: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<LocalVncServerOverview, String> {
        self.start_runtime(request, active_connections_changed, true, None, None)
    }

    /// The privileged worker exposes only RFB; user-side Agent actions must not
    /// inherit SYSTEM privileges.
    #[cfg(target_os = "windows")]
    pub(crate) fn start_service_worker(
        &self,
        request: LocalVncServerStartRequest,
        server_id: String,
    ) -> Result<LocalVncServerOverview, String> {
        if !crate::local_vnc_server_platform::windows_desktop::is_service_worker() {
            return Err("windows_service_worker_not_authorized".into());
        }
        self.start_runtime(request, None, false, Some(server_id), None)
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn start_service_broker(
        &self,
        request: LocalVncServerStartRequest,
        server_id: String,
        relay: Arc<desktop_relay::DesktopRelay>,
    ) -> Result<LocalVncServerOverview, String> {
        self.start_runtime(request, None, false, Some(server_id), Some(relay))
    }

    fn start_runtime(
        &self,
        mut request: LocalVncServerStartRequest,
        active_connections_changed: Option<Arc<dyn Fn() + Send + Sync>>,
        enable_agent: bool,
        server_id: Option<String>,
        relay: Option<Arc<desktop_relay::DesktopRelay>>,
    ) -> Result<LocalVncServerOverview, String> {
        let bind_address = normalize_bind_address(&request.bind_address);
        validate_start_request(&request)?;
        let password = zeroize::Zeroizing::new(std::mem::take(&mut request.password));

        let mut runtime_guard = self
            .runtime
            .lock()
            .map_err(|_| "local_vnc_server_state_poisoned".to_string())?;

        match &*runtime_guard {
            LocalVncServerState::Running(_) => {
                return Err("already_running: local VNC server is already running".to_string());
            }
            LocalVncServerState::Stopping(_) => {
                return Err("local_vnc_server_stopping".to_string());
            }
            LocalVncServerState::Stopped => {}
        }

        let listener =
            TcpListener::bind((bind_address.as_str(), request.port)).map_err(|error| {
                format!(
                    "port_conflict: unable to bind local VNC server on {}:{}: {}",
                    bind_address, request.port, error
                )
            })?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("listener_setup_failed: {error}"))?;

        let effective_port = listener
            .local_addr()
            .map_err(|error| format!("listener_addr_failed: {error}"))?
            .port();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(LocalVncServerConnectionRegistry::new(
            active_connections_changed.clone(),
        ));
        let worker_stop = Arc::clone(&stop_requested);
        let worker_connections = Arc::clone(&connections);
        let auth_key = create_vnc_auth_key(&password);
        drop(password);
        let view_only = request.view_only;
        let shared_display = Arc::new(RwLock::new(resolve_shared_display(
            None,
            default_local_vnc_server_resolution_mode(),
            DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE,
        )));
        if let Some(relay) = relay {
            let mut display = shared_display
                .write()
                .map_err(|_| "local_vnc_server_display_state_poisoned")?;
            *display = SharedDisplay::fallback_main();
            display.id = "windows:virtual-screen".into();
            display.label = "Windows desktop".into();
            display.resolution_mode = LocalVncServerResolutionMode::Native;
            display.relay = Some(relay);
        }
        let frame_cache = Arc::new(LocalVncServerFrameCache::default());
        let framebuffer_core =
            Arc::new(crate::remote_desktop_core::ServerFramebufferCore::default());
        let server_id = server_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        // The enhanced service is part of the shared server.  Its listener is
        // application-managed: start at the preferred port and move upward on
        // conflicts.  The selected port is disclosed only through Discovery.
        let mut agent_port = Self::DEFAULT_AGENT_PORT;
        let (agent, agent_overview) = if !enable_agent {
            (None, stopped_agent_overview())
        } else {
            loop {
                match start_local_agent(
                    bind_address.as_str(),
                    agent_port,
                    server_id.clone(),
                    Arc::clone(&shared_display),
                    request.view_only,
                    None,
                ) {
                    Ok(runtime) => {
                        let overview = runtime.overview();
                        break (Some(runtime), overview);
                    }
                    Err(error)
                        if error.starts_with("agent_listener_port_conflict:")
                            && agent_port < u16::MAX =>
                    {
                        agent_port = agent_port.saturating_add(1);
                    }
                    Err(error) => break (None, failed_agent_overview(bind_address.clone(), error)),
                }
            }
        };
        let agent_port = agent.as_ref().and_then(|runtime| runtime.overview().port);
        let agent_ticket_store = agent.as_ref().map(|runtime| runtime.ticket_store());
        let worker_display = Arc::clone(&shared_display);
        let worker_frame_cache = Arc::clone(&frame_cache);
        let worker_framebuffer_core = Arc::clone(&framebuffer_core);
        let discovery_context = crate::server_discovery::ServerDiscoveryContext::new(
            server_id,
            view_only,
            agent_port,
            agent_ticket_store,
        );
        #[cfg(target_os = "windows")]
        let discovery_context = discovery_context.with_service_agent(!enable_agent);
        #[cfg(target_os = "windows")]
        let discovery_revision = discovery_context.revision_handle();

        let worker = thread::spawn(move || {
            run_listener(
                listener,
                worker_stop,
                worker_connections,
                RfbServerContext {
                    auth_key,
                    view_only,
                    shared_display: worker_display,
                    frame_cache: worker_frame_cache,
                    framebuffer_core: worker_framebuffer_core,
                    discovery: discovery_context,
                },
            );
        });
        let capture_worker = start_framebuffer_capture_worker(
            Arc::clone(&shared_display),
            Arc::clone(&frame_cache),
            Arc::clone(&framebuffer_core),
            Arc::clone(&stop_requested),
        );

        // The runtime port is only ever declared by Discovery and is never
        // derived from the RFB port or its password.

        let started_at = now_unix_seconds();
        let runtime = LocalVncServerRuntime {
            bind_address,
            port: effective_port,
            view_only: request.view_only,
            started_at,
            stop_requested,
            connections,
            shared_display,
            frame_cache,
            agent,
            agent_overview,
            worker: Some(worker),
            capture_worker: Some(capture_worker),
            #[cfg(target_os = "windows")]
            discovery_revision,
        };
        let overview = runtime.overview(LocalVncServerStatus::Running);
        *runtime_guard = LocalVncServerState::Running(Box::new(runtime));

        Ok(overview)
    }

    /// Reannounce optional user Agent availability without restarting RFB.
    #[cfg(target_os = "windows")]
    pub(crate) fn update_service_agent_revision(&self, revision: u64) {
        if let Ok(state) = self.runtime.lock() {
            if let Some(runtime) = state.running() {
                runtime.discovery_revision.store(revision, Ordering::SeqCst);
            }
        }
    }

    pub fn stop(&self) -> Result<LocalVncServerOverview, String> {
        let runtime = {
            let mut runtime_guard = self
                .runtime
                .lock()
                .map_err(|_| "local_vnc_server_state_poisoned".to_string())?;

            let overview = match &*runtime_guard {
                LocalVncServerState::Stopped => return Ok(stopped_overview()),
                LocalVncServerState::Stopping(_) => {
                    return Err("local_vnc_server_stopping".to_string());
                }
                LocalVncServerState::Running(runtime) => {
                    runtime.overview(LocalVncServerStatus::Stopping)
                }
            };
            // Keep the restart gate closed while the stop call owns and joins
            // workers. Callbacks and joins run outside the manager lock.
            let LocalVncServerState::Running(runtime) = std::mem::replace(
                &mut *runtime_guard,
                LocalVncServerState::Stopping(Box::new(overview)),
            ) else {
                unreachable!("running state checked under the same lock");
            };
            runtime
        };
        let mut stopping = LocalVncServerStoppingRuntime {
            runtime,
            state: &self.runtime,
        };
        let runtime = &mut stopping.runtime;

        runtime.stop_requested.store(true, Ordering::SeqCst);
        runtime.connections.disconnect_all();
        runtime.shutdown_workers();
        runtime.connections.clear();

        let overview = LocalVncServerOverview {
            status: LocalVncServerStatus::Stopped,
            bind_address: runtime.bind_address.clone(),
            port: runtime.port,
            addresses: local_address_candidates(),
            displays: list_local_vnc_server_displays(),
            selected_display_id: runtime
                .shared_display
                .read()
                .map(|display| display.id.clone())
                .unwrap_or_else(|_| "main".to_string()),
            selected_display_label: runtime
                .shared_display
                .read()
                .map(|display| display.label.clone())
                .unwrap_or_else(|_| "Main display".to_string()),
            resolution_mode: runtime
                .shared_display
                .read()
                .map(|display| display.resolution_mode)
                .unwrap_or(LocalVncServerResolutionMode::Fit),
            refresh_mode: runtime
                .shared_display
                .read()
                .map(|display| display.refresh_mode)
                .unwrap_or(DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE),
            framebuffer_width: runtime
                .shared_display
                .read()
                .map(|display| display.framebuffer.width)
                .unwrap_or(DEFAULT_FRAMEBUFFER_WIDTH as u32),
            framebuffer_height: runtime
                .shared_display
                .read()
                .map(|display| display.framebuffer.height)
                .unwrap_or(DEFAULT_FRAMEBUFFER_HEIGHT as u32),
            security_type: "vnc_auth".to_string(),
            view_only: runtime.view_only,
            active_connections: 0,
            active_connection_devices: Vec::new(),
            permissions: collect_local_vnc_server_permissions(),
            agent: stopped_agent_overview(),
            frame_diagnostics: runtime.frame_cache.diagnostics(),
            started_at: None,
            updated_at: now_unix_seconds(),
        };
        drop(stopping);
        Ok(overview)
    }

    pub fn status(&self) -> Result<LocalVncServerOverview, String> {
        self.status_with_permissions(collect_local_vnc_server_permissions())
    }

    /// Runtime polling supplies the last permission snapshot without querying TCC.
    pub(crate) fn status_with_permissions(
        &self,
        permissions: Vec<LocalVncServerPermission>,
    ) -> Result<LocalVncServerOverview, String> {
        let runtime_guard = self
            .runtime
            .lock()
            .map_err(|_| "local_vnc_server_state_poisoned".to_string())?;

        Ok(match &*runtime_guard {
            LocalVncServerState::Running(runtime) => {
                runtime.overview_with_permissions(LocalVncServerStatus::Running, permissions)
            }
            LocalVncServerState::Stopping(overview) => {
                let mut result = overview.as_ref().clone();
                result.permissions = permissions;
                result
            }
            LocalVncServerState::Stopped => stopped_overview_with_permissions(permissions),
        })
    }

    pub fn update_view(
        &self,
        request: LocalVncServerViewRequest,
    ) -> Result<LocalVncServerOverview, String> {
        let runtime_guard = self
            .runtime
            .lock()
            .map_err(|_| "local_vnc_server_state_poisoned".to_string())?;

        let runtime = runtime_guard
            .running()
            .ok_or_else(|| "not_running: local VNC server is not running".to_string())?;

        let current = runtime
            .shared_display
            .read()
            .map_err(|_| "local_vnc_server_display_state_poisoned".to_string())?
            .clone();
        let display_id = request.display_id.as_deref().unwrap_or(&current.id);
        let resolution_mode = request.resolution_mode.unwrap_or(current.resolution_mode);
        let refresh_mode = request.refresh_mode.unwrap_or(current.refresh_mode);
        let next_display = if current.relay.is_some() {
            let mut next = current.clone();
            next.resolution_mode = resolution_mode;
            next.refresh_mode = refresh_mode;
            next.framebuffer = framebuffer_size_for_resolution(
                current.frame.pixel_width,
                current.frame.pixel_height,
                resolution_mode,
            );
            next
        } else {
            resolve_shared_display(Some(display_id), resolution_mode, refresh_mode)
        };

        runtime
            .shared_display
            .write()
            .map_err(|_| "local_vnc_server_display_state_poisoned".to_string())?
            .replace_with(next_display)
            .map_err(str::to_string)?;

        Ok(runtime.overview(LocalVncServerStatus::Running))
    }

    pub fn disconnect_connection(
        &self,
        connection_id: String,
    ) -> Result<LocalVncServerOverview, String> {
        let runtime_guard = self
            .runtime
            .lock()
            .map_err(|_| "local_vnc_server_state_poisoned".to_string())?;

        let runtime = runtime_guard
            .running()
            .ok_or_else(|| "not_running: local VNC server is not running".to_string())?;

        runtime.connections.disconnect(&connection_id)?;
        Ok(runtime.overview(LocalVncServerStatus::Running))
    }
}

impl Drop for LocalVncServerManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
