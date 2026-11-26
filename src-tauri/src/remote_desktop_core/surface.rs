use super::memory::{BudgetedBytes, MemoryReservation};
use super::{ClientFramebufferCore, FramebufferGeometry, FramebufferRect, SurfaceFrameUpdate};

const SURFACE_PACKET_MAGIC: [u8; 4] = *b"VNCF";
const SURFACE_PACKET_VERSION: u8 = 1;
const HEADER_LEN: usize = 28;
const RECT_HEADER_LEN: usize = 16;

/// Binary, renderer-facing packet. RFB never crosses this boundary: a Surface
/// receives Canvas-native RGBA pixels for only the rectangles it must present.
/// The native publisher captures BGRA rows under the Core lock and swizzles the
/// owned packet to RGBA after unlocking, without another pixel allocation.
pub struct SurfaceFramePacket {
    bytes: BudgetedBytes,
}

pub(crate) struct SurfaceFrameDraft {
    bytes: BudgetedBytes,
    payload_offset: usize,
}

impl SurfaceFrameDraft {
    pub(crate) fn finish(mut self) -> SurfaceFramePacket {
        for pixel in self.bytes[self.payload_offset..].as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        SurfaceFramePacket { bytes: self.bytes }
    }
}

impl SurfaceFramePacket {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.into_parts().0
    }

    pub(crate) fn into_transport_parts(self) -> (Vec<u8>, MemoryReservation) {
        self.bytes.into_parts()
    }

    pub fn from_core(
        framebuffer: &ClientFramebufferCore,
        update: &SurfaceFrameUpdate,
    ) -> Result<Self, String> {
        Ok(Self::capture_from_core(framebuffer, update)?.finish())
    }

    /// Owns a consistent copy of just the requested rows. No Core borrow escapes
    /// capture, so conversion and socket I/O can run after releasing the lock.
    pub(crate) fn capture_from_core(
        framebuffer: &ClientFramebufferCore,
        update: &SurfaceFrameUpdate,
    ) -> Result<SurfaceFrameDraft, String> {
        let snapshot = framebuffer
            .snapshot()
            .ok_or_else(|| "surface_framebuffer_not_ready".to_string())?;
        if snapshot.frame_id != update.frame_id || snapshot.geometry != update.geometry {
            return Err("surface_framebuffer_update_stale".into());
        }
        let frame_len = update.geometry.byte_len()?;
        if frame_len == 0
            || frame_len > super::rfb_limits::MAX_FRAMEBUFFER_BYTES
            || update.dirty_rectangles.len() > super::client::MAX_SURFACE_DIRTY_RECTS
        {
            return Err("surface_packet_resource_limit".into());
        }
        let payload_len = update
            .dirty_rectangles
            .iter()
            .try_fold(0usize, |total, rect| {
                if !rect.is_within(update.geometry) {
                    return Err("surface_rect_out_of_bounds".into());
                }
                total
                    .checked_add(rect_byte_len(*rect)?)
                    .ok_or_else(|| "surface_packet_size_overflow".to_string())
            })?;
        if payload_len > frame_len {
            return Err("surface_packet_resource_limit".into());
        }
        let headers_len = update
            .dirty_rectangles
            .len()
            .checked_mul(RECT_HEADER_LEN)
            .ok_or_else(|| "surface_packet_size_overflow".to_string())?;
        let packet_len = HEADER_LEN
            .checked_add(headers_len)
            .and_then(|length| length.checked_add(payload_len))
            .ok_or("surface_packet_size_overflow")?;
        let mut bytes = BudgetedBytes::with_capacity(&framebuffer.memory_budget(), packet_len)?;
        bytes.extend_from_slice(&SURFACE_PACKET_MAGIC)?;
        bytes.extend_from_slice(&[SURFACE_PACKET_VERSION, 0])?;
        bytes.extend_from_slice(&(HEADER_LEN as u16).to_be_bytes())?;
        bytes.extend_from_slice(&update.frame_id.to_be_bytes())?;
        bytes.extend_from_slice(&update.geometry.width.to_be_bytes())?;
        bytes.extend_from_slice(&update.geometry.height.to_be_bytes())?;
        bytes.extend_from_slice(&(update.dirty_rectangles.len() as u16).to_be_bytes())?;
        bytes.extend_from_slice(&[0, 0])?;
        for rect in &update.dirty_rectangles {
            bytes.extend_from_slice(&rect.x.to_be_bytes())?;
            bytes.extend_from_slice(&rect.y.to_be_bytes())?;
            bytes.extend_from_slice(&rect.width.to_be_bytes())?;
            bytes.extend_from_slice(&rect.height.to_be_bytes())?;
        }
        for rect in &update.dirty_rectangles {
            append_rect_pixels(&mut bytes, snapshot.pixels(), update.geometry, *rect)?;
        }
        Ok(SurfaceFrameDraft {
            bytes,
            payload_offset: HEADER_LEN + headers_len,
        })
    }
}

fn rect_byte_len(rect: FramebufferRect) -> Result<usize, String> {
    (rect.width as usize)
        .checked_mul(rect.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "surface_packet_size_overflow".to_string())
}

fn append_rect_pixels(
    output: &mut BudgetedBytes,
    pixels: &[u8],
    geometry: FramebufferGeometry,
    rect: FramebufferRect,
) -> Result<(), String> {
    if !rect.is_within(geometry) {
        return Err("surface_rect_out_of_bounds".into());
    }
    for y in rect.y..rect.y + rect.height {
        let start = ((y as usize * geometry.width as usize) + rect.x as usize) * 4;
        let end = start + rect.width as usize * 4;
        output.extend_from_slice(&pixels[start..end])?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/surface_capture_tests.rs"]
mod tests;
