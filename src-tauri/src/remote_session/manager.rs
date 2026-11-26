use super::connection_task::RfbConnectionTask;
use super::core::RemoteSessionCore;
use super::surface_stream::SurfaceStream;
use super::types::{
    RemoteSessionRfbConnectRequest, RemoteSessionRfbEvent, RemoteSessionRfbInput,
    RemoteSessionRfbInputRequest, RemoteSessionSnapshot, RemoteSessionStartRequest,
    RemoteSessionUpdateRequest, SessionSurfaceAttachRequest, SessionSurfaceSnapshot,
};
use crate::remote_desktop_core::{ClientFramebufferHub, RfbClientCommand};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct RemoteSessionManager {
    sessions: Mutex<HashMap<String, RemoteSessionCore>>,
    shutting_down: AtomicBool,
}

/// Captures the resources owned by one teardown before waiting outside the registry.
struct RfbShutdown {
    session_id: String,
    generation: u64,
    framebuffer: Arc<ClientFramebufferHub>,
    task: Option<Arc<RfbConnectionTask>>,
    surface_stream: Option<Arc<SurfaceStream>>,
    remove: bool,
}

impl RfbShutdown {
    fn join(&self) -> Result<(), String> {
        if let Some(task) = &self.task {
            task.join()?;
        }
        if let Some(surface_stream) = &self.surface_stream {
            surface_stream.shutdown()?;
        }
        Ok(())
    }
}

impl RemoteSessionManager {
    pub(crate) fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    /// Cancel every connection and Surface before waiting for any one worker.
    pub(crate) fn request_stop_all(&self) -> Result<(), String> {
        self.begin_shutdown();
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote_session_registry_lock_poisoned")?;
        for session in sessions.values_mut() {
            session.rfb_closing = true;
            session.rfb_removing = true;
            session.state = "stopping".into();
            if let Some(task) = &session.connection_task {
                task.request_stop();
            }
            session.surface_stream.request_stop();
        }
        Ok(())
    }

    /// Pending workers remain registry-owned so application exit can retry.
    pub(crate) fn shutdown(&self) -> Result<(), String> {
        self.request_stop_all()?;
        let ids = self
            .sessions
            .lock()
            .map_err(|_| "remote_session_registry_lock_poisoned")?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut failure = None;
        for id in ids {
            if let Err(error) = self.stop(&id) {
                if failure.as_deref() == Some("rfb_shutdown_pending") || failure.is_none() {
                    failure = Some(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    pub fn start(
        &self,
        request: RemoteSessionStartRequest,
    ) -> Result<RemoteSessionSnapshot, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote_session_registry_lock_poisoned")?;
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err("rfb_cancelled".into());
        }
        if let Some(session) = sessions.get(&request.session_id) {
            if session.target != request.target {
                return Err("remote_session_already_started".into());
            }
            // Retry reuses the same session and its attached Surface owners.
            // Starting that identity again must not replace a live worker.
            return Ok(session.snapshot());
        }
        let framebuffer = Arc::new(ClientFramebufferHub::default());
        let surface_stream = Arc::new(SurfaceStream::start(
            request.session_id.clone(),
            Arc::clone(&framebuffer),
        )?);
        let session = RemoteSessionCore {
            session_id: request.session_id.clone(),
            target: request.target,
            connection_model: request.connection_model,
            display_source: request.display_source,
            input_source: request.input_source,
            state: "starting".into(),
            surfaces: Vec::new(),
            framebuffer,
            surface_stream,
            connection_task: None,
            rfb_closing: false,
            rfb_removing: false,
            rfb_generation: 0,
            rfb_failure: Arc::new(Mutex::new(None)),
        };
        let snapshot = session.snapshot();
        sessions.insert(request.session_id, session);
        Ok(snapshot)
    }

    /// Owns the native TCP worker and waits for a frame from this generation.
    /// Tauri dispatches this synchronous service on its blocking executor.
    pub fn connect_rfb<F>(
        &self,
        request: RemoteSessionRfbConnectRequest,
        on_event: F,
    ) -> Result<(), String>
    where
        F: Fn(RemoteSessionRfbEvent) + Send + 'static,
    {
        let session_id = request.session_id.clone();
        let (task, startup_rx, generation, framebuffer) = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| "remote_session_registry_lock_poisoned")?;
            if self.shutting_down.load(Ordering::SeqCst) {
                return Err("rfb_cancelled".into());
            }
            let session = sessions
                .get_mut(&session_id)
                .ok_or("remote_session_not_found")?;
            if session.rfb_closing {
                return Err("rfb_cancelled".into());
            }
            if let Some(task) = &session.connection_task {
                if !task.is_finished() {
                    return Err("remote_session_rfb_already_started".into());
                }
                // Already finished: reaping cannot wait on a worker holding registry state.
                task.join()?;
            }
            session.framebuffer.lock()?.clear_connection();
            session.framebuffer.notify_surface_state_changed();
            session.rfb_generation = session
                .rfb_generation
                .checked_add(1)
                .ok_or("rfb_generation_overflow")?;
            let failure = Arc::new(Mutex::new(None));
            session.rfb_failure = Arc::clone(&failure);
            let (task, startup_rx) = RfbConnectionTask::spawn(
                request,
                Arc::clone(&session.framebuffer),
                failure,
                on_event,
            )?;
            session.connection_task = Some(Arc::clone(&task));
            session.state = "connecting".into();
            (
                task,
                startup_rx,
                session.rfb_generation,
                Arc::clone(&session.framebuffer),
            )
        };
        let result = match startup_rx.recv_timeout(Duration::from_secs(8)) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err("rfb_startup_timeout".into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("rfb_startup_channel_closed".into()),
        };
        let result = if task.is_cancelled() {
            Err("rfb_cancelled".into())
        } else {
            result
        };
        if let Err(error) = &result {
            task.request_stop();
            // Keep the handle in the session if an OS resolver has not exited.
            let _ = task.join();
            self.clear_failed_rfb_start(&session_id, generation, &framebuffer, error);
        }
        result
    }

    fn clear_failed_rfb_start(
        &self,
        session_id: &str,
        generation: u64,
        framebuffer: &Arc<ClientFramebufferHub>,
        error: &str,
    ) {
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                if !Arc::ptr_eq(&session.framebuffer, framebuffer)
                    || session.rfb_generation != generation
                    || session.rfb_closing
                {
                    return;
                }
                session.state = "failed".into();
                if let Ok(mut failure) = session.rfb_failure.lock() {
                    *failure = Some(super::RfbFailure::from_internal(error).code.into());
                }
            }
        }
    }

    pub fn send_rfb_input(&self, request: RemoteSessionRfbInputRequest) -> Result<(), String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote_session_registry_lock_poisoned")?;
        let session = sessions
            .get(&request.session_id)
            .ok_or("remote_session_not_found")?;
        let command = match request.input {
            RemoteSessionRfbInput::SecureAttention { request_id } => {
                RfbClientCommand::SecureAttention { request_id }
            }
            RemoteSessionRfbInput::Key { down, keysym } => RfbClientCommand::Key { down, keysym },
            RemoteSessionRfbInput::Pointer { x, y, button_mask } => {
                RfbClientCommand::Pointer { x, y, button_mask }
            }
            RemoteSessionRfbInput::Clipboard { text } => RfbClientCommand::Clipboard { text },
            RemoteSessionRfbInput::Refresh { incremental } => {
                RfbClientCommand::Refresh { incremental }
            }
            RemoteSessionRfbInput::SetQuality {
                quality_level,
                compression_level,
            } => RfbClientCommand::SetQuality {
                quality_level,
                compression_level,
            },
        };
        let task = session
            .connection_task
            .as_ref()
            .ok_or("remote_session_rfb_not_started")?;
        task.send(command).map_err(|error| {
            if error == "rfb_transport_closed" {
                session
                    .rfb_failure
                    .lock()
                    .ok()
                    .and_then(|failure| failure.clone())
                    .map(|reason| format!("rfb_transport_closed:{reason}"))
                    .unwrap_or(error)
            } else {
                error
            }
        })
    }

    pub fn disconnect_rfb(&self, session_id: &str) -> Result<(), String> {
        self.shutdown_connection(session_id, false)
    }

    fn shutdown_connection(&self, session_id: &str, remove: bool) -> Result<(), String> {
        if let Some(shutdown) = self.begin_rfb_shutdown(session_id, remove)? {
            shutdown.join()?;
            self.finish_rfb_shutdown(shutdown)?;
        }
        Ok(())
    }

    fn begin_rfb_shutdown(
        &self,
        session_id: &str,
        remove: bool,
    ) -> Result<Option<RfbShutdown>, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote_session_registry_lock_poisoned")?;
        let Some(session) = sessions.get_mut(session_id) else {
            return if remove {
                Ok(None)
            } else {
                Err("remote_session_not_found".into())
            };
        };
        session.rfb_removing |= remove;
        session.state = if session.rfb_removing {
            "stopping"
        } else {
            "disconnecting"
        }
        .into();
        session.rfb_closing = true;
        if let Some(task) = &session.connection_task {
            task.request_stop();
        }
        let surface_stream = remove.then(|| Arc::clone(&session.surface_stream));
        if let Some(surface_stream) = &surface_stream {
            surface_stream.request_stop();
        }
        Ok(Some(RfbShutdown {
            session_id: session_id.into(),
            task: session.connection_task.clone(),
            surface_stream,
            generation: session.rfb_generation,
            framebuffer: Arc::clone(&session.framebuffer),
            remove,
        }))
    }

    fn finish_rfb_shutdown(&self, shutdown: RfbShutdown) -> Result<(), String> {
        let RfbShutdown {
            session_id,
            generation,
            framebuffer,
            remove,
            ..
        } = shutdown;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote_session_registry_lock_poisoned")?;
        let Some(session) = sessions.get_mut(&session_id) else {
            return Ok(());
        };
        // An old completion must not touch a reconnected generation or a new
        // Core created with the same ID (whose generation may also start at zero).
        if !Arc::ptr_eq(&session.framebuffer, &framebuffer) || session.rfb_generation != generation
        {
            return Ok(());
        }
        if !remove && session.rfb_removing {
            return Ok(());
        }
        session.connection_task = None;
        session.rfb_generation = session
            .rfb_generation
            .checked_add(1)
            .ok_or("rfb_generation_overflow")?;
        session.rfb_closing = false;
        session.framebuffer.lock()?.clear_connection();
        session.framebuffer.notify_surface_state_changed();
        session.state = "disconnected".into();
        if remove {
            // Surface Drop must also run outside the registry lock.
            let session = sessions.remove(&session_id);
            drop(sessions);
            drop(session);
        }
        Ok(())
    }

    pub fn update(
        &self,
        request: RemoteSessionUpdateRequest,
    ) -> Result<RemoteSessionSnapshot, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote session registry lock poisoned".to_string())?;
        let session = sessions
            .get_mut(&request.session_id)
            .ok_or_else(|| format!("remote session not found: {}", request.session_id))?;

        if let Some(state) = request.state {
            session.state = state;
        }
        if let Some(connection_model) = request.connection_model {
            session.connection_model = connection_model;
        }
        if let Some(display_source) = request.display_source {
            session.display_source = display_source;
        }
        if let Some(input_source) = request.input_source {
            session.input_source = input_source;
        }

        Ok(session.snapshot())
    }

    pub fn attach_surface(
        &self,
        request: SessionSurfaceAttachRequest,
    ) -> Result<SessionSurfaceSnapshot, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote session registry lock poisoned".to_string())?;
        let session = sessions
            .get_mut(&request.session_id)
            .ok_or_else(|| format!("remote session not found: {}", request.session_id))?;

        if !matches!(
            request.owner.as_str(),
            "embedded" | "external" | "recording"
        ) || (request.owner == "recording" && request.active)
        {
            return Err("invalid_surface_owner".into());
        }

        let known_surface = session
            .surfaces
            .iter()
            .any(|item| item.surface_id == request.surface_id);
        if !known_surface {
            let mut framebuffer = session.framebuffer.lock()?;
            if request.owner == "recording" {
                framebuffer.attach_recording_surface(request.surface_id.clone())?;
            } else {
                framebuffer.attach_surface(request.surface_id.clone(), request.active)?;
            }
        } else if session.surfaces.iter().any(|surface| {
            surface.surface_id == request.surface_id && surface.owner != request.owner
        }) {
            return Err("surface_owner_mismatch".into());
        }
        let surface_url = session
            .surface_stream
            .url_for(&session.session_id, &request.surface_id);
        let surface = SessionSurfaceSnapshot {
            surface_id: request.surface_id,
            owner: request.owner,
            window_label: request.window_label,
            active: request.active,
            surface_url,
        };

        match session
            .surfaces
            .iter_mut()
            .find(|item| item.surface_id == surface.surface_id)
        {
            Some(existing) => *existing = surface.clone(),
            None => session.surfaces.push(surface.clone()),
        }

        if surface.active {
            session.set_active_surface(&surface.surface_id);
            session
                .framebuffer
                .lock()?
                .set_active_surface(&surface.surface_id)?;
            session.framebuffer.notify_surface_state_changed();
        }

        Ok(surface)
    }

    pub fn set_active_surface(
        &self,
        session_id: &str,
        surface_id: &str,
    ) -> Result<RemoteSessionSnapshot, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote session registry lock poisoned".to_string())?;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("remote session not found: {session_id}"))?;

        if !session
            .surfaces
            .iter()
            .any(|surface| surface.surface_id == surface_id && surface.owner != "recording")
        {
            return Err(format!("session surface not found: {surface_id}"));
        }

        session.set_active_surface(surface_id);
        session.framebuffer.lock()?.set_active_surface(surface_id)?;
        session.framebuffer.notify_surface_state_changed();
        Ok(session.snapshot())
    }

    pub fn detach_surface(
        &self,
        session_id: &str,
        surface_id: &str,
    ) -> Result<RemoteSessionSnapshot, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote session registry lock poisoned".to_string())?;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("remote session not found: {session_id}"))?;

        let detached_was_active = session
            .surfaces
            .iter()
            .any(|surface| surface.surface_id == surface_id && surface.active);
        session
            .surfaces
            .retain(|surface| surface.surface_id != surface_id);
        session.framebuffer.lock()?.detach_surface(surface_id)?;
        session.surface_stream.detach_surface(surface_id);
        if detached_was_active {
            if let Some(next_surface_id) = session
                .surfaces
                .iter()
                .find(|surface| surface.owner != "recording")
                .map(|surface| surface.surface_id.clone())
            {
                session.set_active_surface(&next_surface_id);
                session
                    .framebuffer
                    .lock()?
                    .set_active_surface(&next_surface_id)?;
            }
        }
        session.framebuffer.notify_surface_state_changed();
        Ok(session.snapshot())
    }

    pub fn stop(&self, session_id: &str) -> Result<(), String> {
        self.shutdown_connection(session_id, true)
    }

    pub fn get_snapshot(&self, session_id: &str) -> Result<Option<RemoteSessionSnapshot>, String> {
        Ok(self
            .sessions
            .lock()
            .map_err(|_| "remote session registry lock poisoned".to_string())?
            .get(session_id)
            .map(RemoteSessionCore::snapshot))
    }

    pub fn list_snapshots(&self) -> Result<Vec<RemoteSessionSnapshot>, String> {
        let mut snapshots = self
            .sessions
            .lock()
            .map_err(|_| "remote session registry lock poisoned".to_string())?
            .values()
            .map(RemoteSessionCore::snapshot)
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        Ok(snapshots)
    }

    pub fn active_session_count(&self) -> Result<usize, String> {
        Ok(self
            .sessions
            .lock()
            .map_err(|_| "remote session registry lock poisoned".to_string())?
            .len())
    }
}

#[cfg(test)]
#[path = "../../tests/support/rfb_shutdown_tests.rs"]
mod shutdown_tests;
