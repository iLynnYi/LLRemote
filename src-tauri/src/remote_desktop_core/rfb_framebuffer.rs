use super::memory::{BudgetedBytes, ClientMemoryBudget};
use super::{
    client::coalesce_surface_dirty, rfb_limits::framebuffer_bytes, ClientFramebufferCore, FrameId,
    FramebufferGeometry, FramebufferRect, PixelFormat,
};

/// Decoder-owned back buffer. Only completed messages swap into the published
/// Core; the next message repairs the previous dirty regions instead of cloning
/// the full desktop. A failed message forces a fresh baseline on the next use.
pub(super) struct RfbFramebufferTransaction {
    pixels: BudgetedBytes,
    buffer_geometry: Option<FramebufferGeometry>,
    geometry: Option<FramebufferGeometry>,
    base_geometry: Option<FramebufferGeometry>,
    base_frame: Option<FrameId>,
    expected_frame: Option<FrameId>,
    repair: Vec<FramebufferRect>,
    dirty: Vec<FramebufferRect>,
    reusable: bool,
    has_frame: bool,
    resized: bool,
}

impl Default for RfbFramebufferTransaction {
    fn default() -> Self {
        Self::with_memory(&ClientMemoryBudget::default())
    }
}

impl RfbFramebufferTransaction {
    pub(super) fn with_memory(memory: &ClientMemoryBudget) -> Self {
        Self {
            pixels: BudgetedBytes::empty(memory),
            buffer_geometry: None,
            geometry: None,
            base_geometry: None,
            base_frame: None,
            expected_frame: None,
            repair: Vec::new(),
            dirty: Vec::new(),
            reusable: false,
            has_frame: false,
            resized: false,
        }
    }
    // Allocate before acquiring the publication lock. The lock only protects
    // baseline copies and the final swap, never compression or image decoding.
    pub fn reserve(&mut self, geometry: FramebufferGeometry) -> Result<(), String> {
        if geometry.pixel_format != PixelFormat::Bgra8888 {
            return Err("client_framebuffer_noncanonical_pixel_format".into());
        }
        let length = framebuffer_bytes(geometry.width as usize, geometry.height as usize)?;
        if self.pixels.len() != length {
            self.pixels.resize_zeroed(length)?;
            self.reusable = false;
        }
        Ok(())
    }

    pub fn prepare(&mut self, core: &ClientFramebufferCore, geometry: FramebufferGeometry) {
        let incremental = self.reusable
            && self.buffer_geometry == Some(geometry)
            && core.geometry() == Some(geometry)
            && self.expected_frame == core.latest_frame_id();
        if incremental {
            for rect in &self.repair {
                copy_rows(core.pixels(), &mut self.pixels, geometry, *rect);
            }
        } else if core.geometry() == Some(geometry) && core.pixels().len() == self.pixels.len() {
            self.pixels.copy_from_slice(core.pixels());
        } else {
            self.pixels.fill(0);
        }
        self.base_geometry = core.geometry();
        self.base_frame = core.latest_frame_id();
        self.buffer_geometry = Some(geometry);
        self.geometry = Some(geometry);
        self.has_frame = core.geometry() == Some(geometry) && core.latest_frame_id().is_some();
        self.dirty.clear();
        self.resized = false;
        self.reusable = false;
    }

    pub fn resize(&mut self, geometry: FramebufferGeometry) -> Result<(), String> {
        self.reserve(geometry)?;
        self.pixels.fill(0);
        self.geometry = Some(geometry);
        self.buffer_geometry = Some(geometry);
        self.dirty.clear();
        self.has_frame = false;
        self.resized = true;
        Ok(())
    }

    /// ExtendedDesktopSize retains the overlapping desktop. A full update
    /// request would ask the peer for another layout notice indefinitely.
    pub fn resize_preserving_pixels(
        &mut self,
        geometry: FramebufferGeometry,
    ) -> Result<(), String> {
        let old = self.geometry.ok_or("client_framebuffer_geometry_missing")?;
        let length = framebuffer_bytes(geometry.width as usize, geometry.height as usize)?;
        let mut next = BudgetedBytes::zeroed(&self.pixels.budget(), length)?;
        if self.has_frame {
            let row_bytes = old.width.min(geometry.width) as usize * 4;
            for row in 0..old.height.min(geometry.height) as usize {
                let source = row * old.width as usize * 4;
                let target = row * geometry.width as usize * 4;
                next[target..target + row_bytes]
                    .copy_from_slice(&self.pixels[source..source + row_bytes]);
            }
        }
        self.pixels = next;
        self.geometry = Some(geometry);
        self.buffer_geometry = Some(geometry);
        self.dirty.clear();
        if self.has_frame {
            self.dirty.push(FramebufferRect::full(geometry));
        }
        self.resized = true;
        Ok(())
    }

    pub fn apply(&mut self, rect: FramebufferRect, pixels: &[u8]) -> Result<(), String> {
        let geometry = self.geometry.ok_or("client_framebuffer_geometry_missing")?;
        if !rect.is_within(geometry) {
            return Err("rfb_framebuffer_rect_out_of_bounds".into());
        }
        if pixels.len() != rect.width as usize * rect.height as usize * 4 {
            return Err("client_framebuffer_rect_length_mismatch".into());
        }
        let row_bytes = rect.width as usize * 4;
        for row in 0..rect.height as usize {
            let target = ((rect.y as usize + row) * geometry.width as usize + rect.x as usize) * 4;
            self.pixels[target..target + row_bytes]
                .copy_from_slice(&pixels[row * row_bytes..(row + 1) * row_bytes]);
        }
        self.mark_dirty(rect, geometry);
        Ok(())
    }

    /// Writes canonical pixels straight into the private transaction buffer.
    pub fn apply_pixels(
        &mut self,
        rect: FramebufferRect,
        mut pixels: impl ExactSizeIterator<Item = [u8; 4]>,
    ) -> Result<(), String> {
        let geometry = self.geometry.ok_or("client_framebuffer_geometry_missing")?;
        if !rect.is_within(geometry) {
            return Err("rfb_framebuffer_rect_out_of_bounds".into());
        }
        if pixels.len() != rect.width as usize * rect.height as usize {
            return Err("client_framebuffer_rect_length_mismatch".into());
        }
        for row in 0..rect.height as usize {
            let start = ((rect.y as usize + row) * geometry.width as usize + rect.x as usize) * 4;
            let end = start + rect.width as usize * 4;
            for output in self.pixels[start..end].as_chunks_mut::<4>().0 {
                output.copy_from_slice(
                    &pixels
                        .next()
                        .ok_or("client_framebuffer_rect_length_mismatch")?,
                );
            }
        }
        if pixels.next().is_some() {
            return Err("client_framebuffer_rect_length_mismatch".into());
        }
        self.mark_dirty(rect, geometry);
        Ok(())
    }

    fn mark_dirty(&mut self, rect: FramebufferRect, geometry: FramebufferGeometry) {
        self.has_frame = true;
        self.dirty.push(rect);
        // Bound both delivery history and next-buffer repair metadata.
        if self.dirty.len() > 64 {
            self.dirty = coalesce_surface_dirty(std::mem::take(&mut self.dirty), geometry);
        }
    }

    pub fn copy_rect(
        &mut self,
        source_x: u32,
        source_y: u32,
        target: FramebufferRect,
    ) -> Result<(), String> {
        if !self.has_frame {
            return Err("rfb_copyrect_without_framebuffer".into());
        }
        let geometry = self.geometry.ok_or("client_framebuffer_geometry_missing")?;
        if !target.is_within(geometry) {
            return Err("rfb_framebuffer_rect_out_of_bounds".into());
        }
        let source = FramebufferRect {
            x: source_x,
            y: source_y,
            width: target.width,
            height: target.height,
        };
        if !source.is_within(geometry) {
            return Err("rfb_copyrect_source_out_of_bounds".into());
        }
        // Copy bottom-up when moving down; each row handles horizontal overlap.
        let height = source.height as usize;
        let row_bytes = source.width as usize * 4;
        for step in 0..height {
            let row = if target.y > source.y {
                height - step - 1
            } else {
                step
            };
            let start =
                ((source.y as usize + row) * geometry.width as usize + source.x as usize) * 4;
            let destination =
                ((target.y as usize + row) * geometry.width as usize + target.x as usize) * 4;
            self.pixels
                .copy_within(start..start + row_bytes, destination);
        }
        self.mark_dirty(target, geometry);
        Ok(())
    }

    pub fn commit(&mut self, core: &mut ClientFramebufferCore) -> Result<Option<FrameId>, String> {
        let geometry = self.geometry.ok_or("client_framebuffer_geometry_missing")?;
        if self.dirty.is_empty() && !self.resized {
            return Ok(None);
        }
        let old_geometry = core.geometry();
        let id = core.commit_transaction(
            geometry,
            &mut self.pixels,
            &self.dirty,
            self.resized,
            self.base_geometry,
            self.base_frame,
        )?;
        self.buffer_geometry = old_geometry;
        self.expected_frame = core.latest_frame_id();
        if self.resized {
            // Even a same-size DesktopSize invalidates untouched old pixels.
            self.repair.clear();
            self.repair.push(FramebufferRect::full(geometry));
        } else {
            self.repair.clone_from(&self.dirty);
        }
        self.reusable = true;
        Ok(id)
    }
}

fn copy_rows(
    source: &[u8],
    target: &mut [u8],
    geometry: FramebufferGeometry,
    rect: FramebufferRect,
) {
    for row in 0..rect.height as usize {
        let start = ((rect.y as usize + row) * geometry.width as usize + rect.x as usize) * 4;
        let end = start + rect.width as usize * 4;
        target[start..end].copy_from_slice(&source[start..end]);
    }
}
