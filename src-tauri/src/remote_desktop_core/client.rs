use super::memory::{BudgetedBytes, ClientMemoryBudget};
use super::rfb_limits::framebuffer_bytes;
use super::{FrameId, FramebufferGeometry, FramebufferRect, PixelFormat};
use std::collections::VecDeque;

#[cfg(test)]
#[path = "../../tests/support/client_memory_budget_tests.rs"]
mod memory_tests;

const DIRTY_HISTORY_FRAMES: usize = 120;
pub(super) const MAX_SURFACE_DIRTY_RECTS: usize = 64;

struct FrameDirtyRecord {
    frame_id: FrameId,
    rectangles: Vec<FramebufferRect>,
}

pub type SurfaceId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceSnapshot {
    pub id: SurfaceId,
    pub active: bool,
    pub recording: bool,
    pub last_presented_frame: Option<FrameId>,
}

#[derive(Debug, Clone)]
pub struct DecodedFramebufferRect {
    pub rect: FramebufferRect,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DecodedFramebufferUpdate {
    pub geometry: FramebufferGeometry,
    pub rectangles: Vec<DecodedFramebufferRect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceFrameUpdate {
    pub frame_id: FrameId,
    pub geometry: FramebufferGeometry,
    pub dirty_rectangles: Vec<FramebufferRect>,
}

/// Borrowed view of the current canonical framebuffer. Renderers must finish using
/// it before the next `apply`; the Core never clones the full pixel buffer merely
/// to hand a frame to another Surface.
pub struct ClientFramebufferSnapshot<'a> {
    pub frame_id: FrameId,
    pub geometry: FramebufferGeometry,
    pixels: &'a [u8],
}

impl ClientFramebufferSnapshot<'_> {
    pub fn pixels(&self) -> &[u8] {
        self.pixels
    }
}

/// Owns canonical decoded pixels and one delivery cursor per Surface. RFB decoding
/// remains outside this module, so any protocol adapter can submit BGRA rectangles
/// without exposing pixel buffers to UI state.
pub struct ClientFramebufferCore {
    geometry: Option<FramebufferGeometry>,
    pixels: BudgetedBytes,
    memory: ClientMemoryBudget,
    surfaces: Vec<SurfaceSnapshot>,
    latest_frame_id: Option<FrameId>,
    dirty_history: VecDeque<FrameDirtyRecord>,
    next_frame_id: FrameId,
    presentation_floor: FrameId,
}

impl Default for ClientFramebufferCore {
    fn default() -> Self {
        Self::with_memory(ClientMemoryBudget::default())
    }
}

impl ClientFramebufferCore {
    pub(crate) fn with_memory(memory: ClientMemoryBudget) -> Self {
        Self {
            pixels: BudgetedBytes::empty(&memory),
            memory,
            geometry: None,
            surfaces: Vec::new(),
            latest_frame_id: None,
            dirty_history: VecDeque::new(),
            next_frame_id: 0,
            presentation_floor: 1,
        }
    }

    pub(crate) fn memory_budget(&self) -> ClientMemoryBudget {
        self.memory.clone()
    }
    /// Starts a connection generation without carrying over pixels or Surface
    /// ACK cursors. Frame ids remain monotonic so late old ACKs cannot win.
    pub fn clear_connection(&mut self) {
        self.geometry = None;
        self.pixels = BudgetedBytes::empty(&self.memory);
        self.latest_frame_id = None;
        self.invalidate_presentation();
    }

    fn invalidate_presentation(&mut self) {
        self.presentation_floor = self.next_frame_id.saturating_add(1);
        self.dirty_history.clear();
        for surface in &mut self.surfaces {
            surface.last_presented_frame = None;
        }
    }

    /// A replacement Canvas has no pixels from its predecessor, even when both
    /// sockets use the same Surface identity and frame id.
    pub(crate) fn reset_surface_delivery(&mut self, id: &str) -> Result<(), String> {
        let surface = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == id)
            .ok_or("client_surface_not_attached")?;
        surface.last_presented_frame = None;
        Ok(())
    }

    pub fn attach_surface(&mut self, id: impl Into<SurfaceId>, active: bool) -> Result<(), String> {
        let id = id.into();
        if id.is_empty() || id.len() > 128 || self.surfaces.len() >= 8 {
            return Err("rfb_resource_limit:surfaces".into());
        }
        if self.surfaces.iter().any(|surface| surface.id == id) {
            return Err("client_surface_already_attached".into());
        }
        if active {
            for surface in &mut self.surfaces {
                surface.active = false;
            }
        }
        self.surfaces.push(SurfaceSnapshot {
            id,
            active,
            recording: false,
            last_presented_frame: None,
        });
        Ok(())
    }

    /// Recording consumes the canonical pixels independently of the selected display.
    pub fn attach_recording_surface(&mut self, id: impl Into<SurfaceId>) -> Result<(), String> {
        if self.surfaces.iter().any(|surface| surface.recording) {
            return Err("recording_already_attached".into());
        }
        self.attach_surface(id, false)?;
        self.surfaces
            .last_mut()
            .expect("attached surface")
            .recording = true;
        Ok(())
    }

    pub fn set_active_surface(&mut self, id: &str) -> Result<(), String> {
        if !self
            .surfaces
            .iter()
            .any(|surface| surface.id == id && !surface.recording)
        {
            return Err("client_surface_not_attached".into());
        }
        for surface in &mut self.surfaces {
            surface.active = surface.id == id;
        }
        Ok(())
    }

    pub fn detach_surface(&mut self, id: &str) -> Result<(), String> {
        let before = self.surfaces.len();
        self.surfaces.retain(|surface| surface.id != id);
        if self.surfaces.len() == before {
            return Err("client_surface_not_attached".into());
        }
        Ok(())
    }

    pub fn surfaces(&self) -> &[SurfaceSnapshot] {
        &self.surfaces
    }

    pub fn geometry(&self) -> Option<FramebufferGeometry> {
        self.geometry
    }

    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_slice()
    }

    pub fn snapshot(&self) -> Option<ClientFramebufferSnapshot<'_>> {
        Some(ClientFramebufferSnapshot {
            frame_id: self.latest_frame_id?,
            geometry: self.geometry?,
            pixels: &self.pixels,
        })
    }

    pub fn latest_frame_id(&self) -> Option<FrameId> {
        self.latest_frame_id
    }

    pub fn next_surface_update(&self, id: &str) -> Result<Option<SurfaceFrameUpdate>, String> {
        let surface = self
            .surfaces
            .iter()
            .find(|surface| surface.id == id)
            .ok_or_else(|| "client_surface_not_attached".to_string())?;
        if !surface.active && !surface.recording {
            return Ok(None);
        }
        let Some(frame_id) = self.latest_frame_id else {
            return Ok(None);
        };
        if surface.last_presented_frame == Some(frame_id) {
            return Ok(None);
        }
        let geometry = self
            .geometry
            .ok_or_else(|| "client_framebuffer_geometry_missing".to_string())?;
        let dirty_rectangles = self.surface_dirty_since(surface.last_presented_frame, geometry);
        Ok(Some(SurfaceFrameUpdate {
            frame_id,
            geometry,
            dirty_rectangles,
        }))
    }

    fn surface_dirty_since(
        &self,
        last_presented_frame: Option<FrameId>,
        geometry: FramebufferGeometry,
    ) -> Vec<FramebufferRect> {
        let Some(last_presented_frame) = last_presented_frame else {
            return vec![FramebufferRect::full(geometry)];
        };
        let Some(first) = self.dirty_history.front() else {
            return vec![FramebufferRect::full(geometry)];
        };
        if last_presented_frame.saturating_add(1) < first.frame_id {
            return vec![FramebufferRect::full(geometry)];
        }
        let rectangles = self
            .dirty_history
            .iter()
            .filter(|record| record.frame_id > last_presented_frame)
            .flat_map(|record| record.rectangles.iter().copied())
            .collect::<Vec<_>>();
        coalesce_surface_dirty(rectangles, geometry)
    }

    pub fn mark_surface_presented(&mut self, id: &str, frame_id: FrameId) -> Result<(), String> {
        let surface = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == id)
            .ok_or_else(|| "client_surface_not_attached".to_string())?;
        if frame_id < self.presentation_floor {
            return Ok(());
        }
        let latest_frame_id = self
            .latest_frame_id
            .ok_or_else(|| "client_surface_presented_without_frame".to_string())?;
        if frame_id > latest_frame_id {
            return Err("client_surface_presented_future_frame".into());
        }
        if surface
            .last_presented_frame
            .is_some_and(|presented| frame_id <= presented)
        {
            return Ok(());
        }
        surface.last_presented_frame = Some(frame_id);
        Ok(())
    }

    /// Invalidates pixels after an RFB DesktopSize transition. A renderer cannot
    /// borrow the previous geometry while the server is preparing the first frame
    /// for the new desktop.
    pub fn reset_geometry(&mut self, geometry: FramebufferGeometry) -> Result<(), String> {
        if geometry.pixel_format != PixelFormat::Bgra8888 {
            return Err("client_framebuffer_noncanonical_pixel_format".into());
        }
        let length = framebuffer_bytes(geometry.width as usize, geometry.height as usize)?;
        let next = BudgetedBytes::zeroed(&self.memory, length)?;
        self.pixels = next;
        self.geometry = Some(geometry);
        self.latest_frame_id = None;
        self.invalidate_presentation();
        Ok(())
    }

    pub fn apply(&mut self, update: DecodedFramebufferUpdate) -> Result<FrameId, String> {
        if update.geometry.pixel_format != PixelFormat::Bgra8888 {
            return Err("client_framebuffer_noncanonical_pixel_format".into());
        }
        let length = framebuffer_bytes(
            update.geometry.width as usize,
            update.geometry.height as usize,
        )?;
        if update.rectangles.len() > usize::from(u16::MAX) {
            return Err("rfb_resource_limit:decoded_update".into());
        }
        if update.rectangles.is_empty() {
            return self
                .latest_frame_id
                .ok_or_else(|| "client_framebuffer_empty_update".to_string());
        }
        let bpp = update.geometry.pixel_format.bytes_per_pixel();
        for decoded in &update.rectangles {
            if !decoded.rect.is_within(update.geometry) {
                return Err("client_framebuffer_rect_out_of_bounds".into());
            }
            let expected = decoded.rect.width as usize * decoded.rect.height as usize * bpp;
            if decoded.pixels.len() != expected {
                return Err("client_framebuffer_rect_length_mismatch".into());
            }
        }
        let supplied_bytes = update.rectangles.iter().try_fold(
            update
                .rectangles
                .capacity()
                .checked_mul(std::mem::size_of::<DecodedFramebufferRect>())
                .ok_or("rfb_resource_limit:session_memory")?,
            |total, decoded| {
                total
                    .checked_add(decoded.pixels.capacity())
                    .ok_or("rfb_resource_limit:session_memory")
            },
        )?;
        let _supplied = self.memory.reserve(supplied_bytes)?;
        let frame_id = self
            .next_frame_id
            .checked_add(1)
            .ok_or("client_frame_id_overflow")?;
        let geometry_changed = self.geometry != Some(update.geometry);
        if geometry_changed {
            let next = BudgetedBytes::zeroed(&self.memory, length)?;
            self.geometry = Some(update.geometry);
            self.pixels = next;
            self.invalidate_presentation();
        }
        let mut dirty_rectangles = Vec::with_capacity(update.rectangles.len());
        for decoded in update.rectangles {
            for row in 0..decoded.rect.height as usize {
                let source_start = row * decoded.rect.width as usize * bpp;
                let target_start = ((decoded.rect.y as usize + row)
                    * update.geometry.width as usize
                    + decoded.rect.x as usize)
                    * bpp;
                let row_len = decoded.rect.width as usize * bpp;
                self.pixels[target_start..target_start + row_len]
                    .copy_from_slice(&decoded.pixels[source_start..source_start + row_len]);
            }
            dirty_rectangles.push(decoded.rect);
        }
        self.record_frame(frame_id, dirty_rectangles, update.geometry);
        Ok(frame_id)
    }

    /// Publish the decoder's complete back buffer in constant time after all
    /// validation. The version check prevents a stale transaction replacing a
    /// cleared or externally updated Core.
    pub(super) fn commit_transaction(
        &mut self,
        geometry: FramebufferGeometry,
        pixels: &mut BudgetedBytes,
        dirty: &[FramebufferRect],
        resized: bool,
        base_geometry: Option<FramebufferGeometry>,
        base_frame: Option<FrameId>,
    ) -> Result<Option<FrameId>, String> {
        if self.geometry != base_geometry || self.latest_frame_id != base_frame {
            return Err("client_framebuffer_transaction_stale".into());
        }
        if !self.memory.same_owner(&pixels.budget())
            || pixels.len() != geometry.byte_len()?
            || dirty.iter().any(|r| !r.is_within(geometry))
        {
            return Err("client_framebuffer_transaction_invalid".into());
        }
        let frame_id = if dirty.is_empty() {
            None
        } else {
            Some(
                self.next_frame_id
                    .checked_add(1)
                    .ok_or("client_frame_id_overflow")?,
            )
        };
        if resized || self.geometry != Some(geometry) {
            self.invalidate_presentation();
        }
        self.geometry = Some(geometry);
        std::mem::swap(&mut self.pixels, pixels);
        self.latest_frame_id = frame_id;
        if let Some(id) = frame_id {
            self.record_frame(id, dirty.to_vec(), geometry);
        }
        Ok(frame_id)
    }

    fn record_frame(
        &mut self,
        frame_id: FrameId,
        dirty: Vec<FramebufferRect>,
        geometry: FramebufferGeometry,
    ) {
        self.next_frame_id = frame_id;
        self.latest_frame_id = Some(frame_id);
        self.dirty_history.push_back(FrameDirtyRecord {
            frame_id,
            rectangles: coalesce_surface_dirty(dirty, geometry),
        });
        while self.dirty_history.len() > DIRTY_HISTORY_FRAMES {
            self.dirty_history.pop_front();
        }
    }
}

pub(super) fn coalesce_surface_dirty(
    rectangles: Vec<FramebufferRect>,
    geometry: FramebufferGeometry,
) -> Vec<FramebufferRect> {
    super::coalesce_dirty_rectangles(rectangles, geometry, MAX_SURFACE_DIRTY_RECTS)
}
