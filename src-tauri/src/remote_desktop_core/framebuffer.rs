use std::sync::Arc;

pub type FrameId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra8888,
}

impl PixelFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramebufferGeometry {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
}

impl FramebufferGeometry {
    pub fn byte_len(self) -> Result<usize, String> {
        let pixels = usize::try_from(self.width)
            .map_err(|_| "framebuffer_width_overflow")?
            .checked_mul(usize::try_from(self.height).map_err(|_| "framebuffer_height_overflow")?)
            .ok_or_else(|| "framebuffer_pixel_count_overflow".to_string())?;
        pixels
            .checked_mul(self.pixel_format.bytes_per_pixel())
            .ok_or_else(|| "framebuffer_byte_count_overflow".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramebufferRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl FramebufferRect {
    pub fn full(geometry: FramebufferGeometry) -> Self {
        Self {
            x: 0,
            y: 0,
            width: geometry.width,
            height: geometry.height,
        }
    }
    pub fn is_within(self, geometry: FramebufferGeometry) -> bool {
        self.width > 0
            && self.height > 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|end| end <= geometry.width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|end| end <= geometry.height)
    }
}

#[derive(Debug, Clone)]
pub struct Framebuffer {
    geometry: FramebufferGeometry,
    pixels: Arc<Vec<u8>>,
}

impl Framebuffer {
    pub fn new(geometry: FramebufferGeometry, pixels: Vec<u8>) -> Result<Self, String> {
        if pixels.len() != geometry.byte_len()? {
            return Err("framebuffer_length_mismatch".into());
        }
        Ok(Self {
            geometry,
            pixels: Arc::new(pixels),
        })
    }
    pub fn geometry(&self) -> FramebufferGeometry {
        self.geometry
    }
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_slice()
    }
    /// Shares immutable canonical pixels with a per-client encoder without a frame copy.
    pub fn shared_pixels(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.pixels)
    }
}

pub fn diff_dirty_rectangles(
    previous: Option<&Framebuffer>,
    current: &Framebuffer,
    tile_size: u32,
) -> Vec<FramebufferRect> {
    let geometry = current.geometry;
    let Some(previous) = previous else {
        return vec![FramebufferRect::full(geometry)];
    };
    if previous.geometry != geometry || tile_size == 0 {
        return vec![FramebufferRect::full(geometry)];
    }
    let mut rects = Vec::new();
    for y in (0..geometry.height).step_by(tile_size as usize) {
        let height = tile_size.min(geometry.height - y);
        let mut pending: Option<FramebufferRect> = None;
        for x in (0..geometry.width).step_by(tile_size as usize) {
            let width = tile_size.min(geometry.width - x);
            if tile_changed(
                previous.pixels(),
                current.pixels(),
                geometry,
                x,
                y,
                width,
                height,
            ) {
                let next = FramebufferRect {
                    x,
                    y,
                    width,
                    height,
                };
                if let Some(existing) = pending
                    .as_mut()
                    .filter(|rect| rect.x + rect.width == next.x && rect.height == next.height)
                {
                    existing.width += next.width;
                } else {
                    if let Some(rect) = pending.take() {
                        rects.push(rect);
                    }
                    pending = Some(next);
                }
            }
        }
        if let Some(rect) = pending {
            rects.push(rect);
        }
    }
    rects
}

fn tile_changed(
    previous: &[u8],
    current: &[u8],
    geometry: FramebufferGeometry,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> bool {
    let bpp = geometry.pixel_format.bytes_per_pixel();
    for row in y..y + height {
        let start = (row as usize * geometry.width as usize + x as usize) * bpp;
        let end = start + width as usize * bpp;
        if previous[start..end] != current[start..end] {
            return true;
        }
    }
    false
}

/// Caps accumulated pixel cost and rectangle count while preserving small separate changes.
pub(crate) fn coalesce_dirty_rectangles(
    rectangles: Vec<FramebufferRect>,
    geometry: FramebufferGeometry,
    max_rectangles: usize,
) -> Vec<FramebufferRect> {
    let payload_bytes = rectangles.iter().try_fold(0usize, |total, rect| {
        (rect.width as usize)
            .checked_mul(rect.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| total.checked_add(bytes))
    });
    // A slow consumer may accumulate the same region across many frames. Sending
    // duplicate pixels must never cost more than sending the current full frame.
    if rectangles.len() <= max_rectangles
        && payload_bytes
            .zip(geometry.byte_len().ok())
            .is_some_and(|(payload, full)| payload <= full)
    {
        return rectangles;
    }
    let Some(first) = rectangles.first().copied() else {
        return Vec::new();
    };
    let bounds = rectangles.into_iter().skip(1).fold(first, |bounds, rect| {
        let left = bounds.x.min(rect.x);
        let top = bounds.y.min(rect.y);
        let right = (bounds.x + bounds.width).max(rect.x + rect.width);
        let bottom = (bounds.y + bounds.height).max(rect.y + rect.height);
        FramebufferRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    });
    if bounds.is_within(geometry) {
        vec![bounds]
    } else {
        vec![FramebufferRect::full(geometry)]
    }
}
