use super::server_history::ServerDirtyHistory;
use super::{diff_dirty_rectangles, FrameId, Framebuffer, FramebufferRect};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const DIRTY_TILE_SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInterest {
    pub incremental: bool,
    pub region: Option<FramebufferRect>,
}

impl FrameInterest {
    pub const fn incremental() -> Self {
        Self {
            incremental: true,
            region: None,
        }
    }
    pub const fn full() -> Self {
        Self {
            incremental: false,
            region: None,
        }
    }
    pub const fn incremental_region(region: FramebufferRect) -> Self {
        Self {
            incremental: true,
            region: Some(region),
        }
    }
    pub const fn full_region(region: FramebufferRect) -> Self {
        Self {
            incremental: false,
            region: Some(region),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientFrameUpdate {
    pub frame_id: FrameId,
    pub framebuffer: Arc<Framebuffer>,
    pub dirty_rectangles: Vec<FramebufferRect>,
    delivery: PreparedDelivery,
}

#[derive(Debug, Clone)]
struct PreparedDelivery {
    attachment: Arc<()>,
    ticket: u64,
    frame_id: FrameId,
    region: Option<FramebufferRect>,
    source_revision: Option<u64>,
}

#[derive(Default)]
struct ClientCursor {
    attachment: Arc<()>,
    updates: Arc<super::ServerUpdateSignal>,
    issued_ticket: u64,
    last_delivered: Option<FrameId>,
    last_region: Option<FramebufferRect>,
    last_source_revision: Option<u64>,
}

#[derive(Default)]
struct ServerState {
    latest: Option<PublishedFrame>,
    clients: HashMap<String, ClientCursor>,
    next_frame_id: FrameId,
    history: ServerDirtyHistory,
}

#[derive(Clone)]
struct PublishedFrame {
    id: FrameId,
    source_revision: Option<u64>,
    framebuffer: Arc<Framebuffer>,
}

/// Owns the shared captured frame and per-client delivery cursors. Socket, RFB, and
/// encoder code prepare with `next_update` and commit only after a successful flush.
#[derive(Default)]
pub struct ServerFramebufferCore {
    state: Mutex<ServerState>,
    published: Condvar,
}

impl ServerFramebufferCore {
    pub fn attach_client(&self, client_id: impl Into<String>) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "server_framebuffer_state_poisoned")?;
        let id = client_id.into();
        if state.clients.contains_key(&id) {
            return Err("server_client_already_attached".into());
        }
        state.clients.insert(id, ClientCursor::default());
        Ok(())
    }
    pub fn detach_client(&self, client_id: &str) -> Result<(), String> {
        // Resource removal still runs if a publication unwind poisoned the state.
        let removed = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clients
            .remove(client_id);
        if let Some(cursor) = removed {
            cursor.updates.notify();
        }
        Ok(())
    }

    /// Returns this attachment's wake state without injecting protocol callbacks into Core.
    pub(crate) fn client_update_signal(
        &self,
        client_id: &str,
    ) -> Result<Arc<super::ServerUpdateSignal>, String> {
        self.state
            .lock()
            .map_err(|_| "server_framebuffer_state_poisoned")?
            .clients
            .get(client_id)
            .map(|cursor| Arc::clone(&cursor.updates))
            .ok_or_else(|| "server_client_not_attached".into())
    }
    pub fn publish(&self, framebuffer: Framebuffer) -> Result<FrameId, String> {
        self.publish_from_source(framebuffer, None)
    }

    /// Tags a platform capture with its opaque view revision without putting
    /// display inventory or input mapping into the framebuffer core.
    pub(crate) fn publish_for_source(
        &self,
        framebuffer: Framebuffer,
        source_revision: u64,
    ) -> Result<FrameId, String> {
        self.publish_from_source(framebuffer, Some(source_revision))
    }

    fn publish_from_source(
        &self,
        framebuffer: Framebuffer,
        source_revision: Option<u64>,
    ) -> Result<FrameId, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "server_framebuffer_state_poisoned")?;
        let framebuffer = Arc::new(framebuffer);
        let dirty_rectangles = diff_dirty_rectangles(
            state
                .latest
                .as_ref()
                .map(|frame| frame.framebuffer.as_ref()),
            framebuffer.as_ref(),
            DIRTY_TILE_SIZE,
        );
        if dirty_rectangles.is_empty() {
            if let Some(previous) = state.latest.as_ref().filter(|previous| {
                previous.source_revision == source_revision
                    && previous.framebuffer.geometry() == framebuffer.geometry()
            }) {
                return Ok(previous.id);
            }
        }
        state.next_frame_id = state
            .next_frame_id
            .checked_add(1)
            .ok_or("frame_id_overflow")?;
        let id = state.next_frame_id;
        state.history.record(
            id,
            source_revision,
            framebuffer.geometry(),
            dirty_rectangles,
        );
        state.latest = Some(PublishedFrame {
            id,
            source_revision,
            framebuffer: Arc::clone(&framebuffer),
        });
        let updates = state
            .clients
            .values()
            .map(|cursor| Arc::clone(&cursor.updates))
            .collect::<Vec<_>>();
        drop(state);
        self.published.notify_all();
        for updates in updates {
            updates.notify();
        }
        Ok(id)
    }

    /// Waits until capture has published a frame for the requested display geometry.
    pub fn wait_for_geometry(
        &self,
        geometry: super::FramebufferGeometry,
        timeout: Duration,
    ) -> Result<bool, String> {
        let started_at = Instant::now();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "server_framebuffer_state_poisoned")?;
        loop {
            if state
                .latest
                .as_ref()
                .is_some_and(|frame| frame.framebuffer.geometry() == geometry)
            {
                return Ok(true);
            }
            let Some(remaining) = timeout.checked_sub(started_at.elapsed()) else {
                return Ok(false);
            };
            let (next_state, timeout_result) = self
                .published
                .wait_timeout(state, remaining)
                .map_err(|_| "server_framebuffer_state_poisoned")?;
            state = next_state;
            if timeout_result.timed_out() {
                return Ok(state
                    .latest
                    .as_ref()
                    .is_some_and(|frame| frame.framebuffer.geometry() == geometry));
            }
        }
    }
    /// Prepares immutable pixels without consuming them. The sole sender calls
    /// `commit_delivery` after complete encoding and flush, never after a partial write.
    pub fn next_update(
        &self,
        client_id: &str,
        interest: FrameInterest,
    ) -> Result<Option<ClientFrameUpdate>, String> {
        self.next_update_matching(client_id, interest, None, None)
    }

    /// Selects matching immutable pixels without advancing delivery. A source
    /// switch forces a full frame even when its pixels are identical.
    pub(crate) fn next_update_for_source(
        &self,
        client_id: &str,
        interest: FrameInterest,
        geometry: super::FramebufferGeometry,
        source_revision: u64,
    ) -> Result<Option<ClientFrameUpdate>, String> {
        self.next_update_matching(client_id, interest, Some(geometry), Some(source_revision))
    }

    fn next_update_matching(
        &self,
        client_id: &str,
        interest: FrameInterest,
        geometry: Option<super::FramebufferGeometry>,
        source_revision: Option<u64>,
    ) -> Result<Option<ClientFrameUpdate>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "server_framebuffer_state_poisoned")?;
        let latest = state.latest.clone();
        let cursor = state
            .clients
            .get(client_id)
            .ok_or_else(|| "server_client_not_attached".to_string())?;
        let Some(frame) = latest else {
            return Ok(None);
        };
        if geometry.is_some_and(|geometry| frame.framebuffer.geometry() != geometry)
            || source_revision.is_some_and(|revision| frame.source_revision != Some(revision))
        {
            return Ok(None);
        }
        let region_changed = cursor.last_region != interest.region;
        if interest.incremental && !region_changed && cursor.last_delivered == Some(frame.id) {
            return Ok(None);
        }
        let changes = if interest.incremental
            && !region_changed
            && cursor.last_source_revision == frame.source_revision
        {
            cursor.last_delivered.and_then(|delivered| {
                state.history.changes_since(
                    delivered,
                    frame.id,
                    frame.source_revision,
                    frame.framebuffer.geometry(),
                )
            })
        } else {
            None
        };
        let dirty_rectangles = changes
            .unwrap_or_else(|| vec![FramebufferRect::full(frame.framebuffer.geometry())])
            .into_iter()
            .filter_map(|rect| match interest.region {
                Some(region) => intersect_rectangles(rect, region),
                None => Some(rect),
            })
            .collect::<Vec<_>>();
        if dirty_rectangles.is_empty() {
            return Ok(None);
        }
        let cursor = state
            .clients
            .get_mut(client_id)
            .ok_or("server_client_not_attached")?;
        cursor.issued_ticket = cursor
            .issued_ticket
            .checked_add(1)
            .ok_or("server_delivery_ticket_overflow")?;
        let delivery = PreparedDelivery {
            attachment: Arc::clone(&cursor.attachment),
            ticket: cursor.issued_ticket,
            frame_id: frame.id,
            region: interest.region,
            source_revision: frame.source_revision,
        };
        Ok(Some(ClientFrameUpdate {
            frame_id: frame.id,
            framebuffer: Arc::clone(&frame.framebuffer),
            dirty_rectangles,
            delivery,
        }))
    }

    /// Commits the prepared sender receipt. Detached/replaced owners and older
    /// selections cannot move a current connection's delivery cursor backwards.
    pub fn commit_delivery(
        &self,
        client_id: &str,
        update: &ClientFrameUpdate,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "server_framebuffer_state_poisoned")?;
        let cursor = state
            .clients
            .get_mut(client_id)
            .ok_or("server_client_not_attached")?;
        let delivery = &update.delivery;
        if !Arc::ptr_eq(&cursor.attachment, &delivery.attachment)
            || cursor.issued_ticket != delivery.ticket
            || update.frame_id != delivery.frame_id
        {
            return Err("server_delivery_receipt_stale".into());
        }
        cursor.last_delivered = Some(delivery.frame_id);
        cursor.last_region = delivery.region;
        cursor.last_source_revision = delivery.source_revision;
        Ok(())
    }
}

fn intersect_rectangles(left: FramebufferRect, right: FramebufferRect) -> Option<FramebufferRect> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let end_x = left
        .x
        .checked_add(left.width)?
        .min(right.x.checked_add(right.width)?);
    let end_y = left
        .y
        .checked_add(left.height)?
        .min(right.y.checked_add(right.height)?);
    (end_x > x && end_y > y).then_some(FramebufferRect {
        x,
        y,
        width: end_x - x,
        height: end_y - y,
    })
}
