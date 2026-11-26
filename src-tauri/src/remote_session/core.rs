use super::connection_task::RfbConnectionTask;
use super::surface_stream::SurfaceStream;
use super::types::{RemoteSessionSnapshot, SessionConnectionModel, SessionSurfaceSnapshot};
use crate::remote_desktop_core::ClientFramebufferHub;
use std::sync::{Arc, Mutex};

pub(super) struct RemoteSessionCore {
    pub(super) session_id: String,
    pub(super) target: String,
    pub(super) state: String,
    pub(super) connection_model: SessionConnectionModel,
    pub(super) display_source: String,
    pub(super) input_source: String,
    pub(super) surfaces: Vec<SessionSurfaceSnapshot>,
    pub(super) framebuffer: Arc<ClientFramebufferHub>,
    pub(super) surface_stream: Arc<SurfaceStream>,
    pub(super) connection_task: Option<Arc<RfbConnectionTask>>,
    /// Registry-owned lifecycle gate; frontend presentation state cannot open it.
    pub(super) rfb_closing: bool,
    /// Full stop is sticky until this Core is removed; a concurrent disconnect
    /// cannot reopen the transport while its Surface owner is being torn down.
    pub(super) rfb_removing: bool,
    pub(super) rfb_generation: u64,
    /// The terminal Rust TCP-loop error, retained only for the active session
    /// so the next command can report a meaningful transport failure.
    pub(super) rfb_failure: Arc<Mutex<Option<String>>>,
}

impl RemoteSessionCore {
    pub(super) fn set_active_surface(&mut self, surface_id: &str) {
        for surface in &mut self.surfaces {
            surface.active = surface.surface_id == surface_id;
        }
    }

    fn active_surface_id(&self) -> Option<String> {
        self.surfaces
            .iter()
            .find(|surface| surface.active)
            .map(|surface| surface.surface_id.clone())
    }

    pub(super) fn snapshot(&self) -> RemoteSessionSnapshot {
        RemoteSessionSnapshot {
            session_id: self.session_id.clone(),
            target: self.target.clone(),
            state: self.state.clone(),
            connection_model: self.connection_model.clone(),
            display_source: self.display_source.clone(),
            input_source: self.input_source.clone(),
            active_surface_id: self.active_surface_id(),
            surfaces: self.surfaces.clone(),
        }
    }
}
