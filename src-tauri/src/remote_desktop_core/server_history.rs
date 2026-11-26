use super::{FrameId, FramebufferGeometry, FramebufferRect};
use std::collections::VecDeque;

const MAX_HISTORY_FRAMES: usize = 64;
const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;

struct FrameDelta {
    id: FrameId,
    source_revision: Option<u64>,
    geometry: FramebufferGeometry,
    rectangles: Vec<FramebufferRect>,
}

// Retains change metadata, never old pixel snapshots. A missing range requires full pixels.
#[derive(Default)]
pub(super) struct ServerDirtyHistory {
    frames: VecDeque<FrameDelta>,
    bytes: usize,
}

impl ServerDirtyHistory {
    pub(super) fn record(
        &mut self,
        id: FrameId,
        source_revision: Option<u64>,
        geometry: FramebufferGeometry,
        rectangles: Vec<FramebufferRect>,
    ) {
        let bytes = rectangles
            .capacity()
            .saturating_mul(std::mem::size_of::<FramebufferRect>());
        if bytes > MAX_HISTORY_BYTES {
            self.frames.clear();
            self.bytes = 0;
            return;
        }
        while self.frames.len() >= MAX_HISTORY_FRAMES
            || bytes > MAX_HISTORY_BYTES.saturating_sub(self.bytes)
        {
            let Some(old) = self.frames.pop_front() else {
                break;
            };
            self.bytes -= old.rectangles.capacity() * std::mem::size_of::<FramebufferRect>();
        }
        self.bytes += bytes;
        self.frames.push_back(FrameDelta {
            id,
            source_revision,
            geometry,
            rectangles,
        });
    }

    pub(super) fn changes_since(
        &self,
        delivered: FrameId,
        latest: FrameId,
        source_revision: Option<u64>,
        geometry: FramebufferGeometry,
    ) -> Option<Vec<FramebufferRect>> {
        if delivered == latest {
            return Some(Vec::new());
        }
        let mut covered = delivered;
        let mut rectangles = Vec::new();
        for frame in self
            .frames
            .iter()
            .filter(|frame| frame.id > delivered && frame.id <= latest)
        {
            if covered.checked_add(1) != Some(frame.id)
                || frame.source_revision != source_revision
                || frame.geometry != geometry
            {
                return None;
            }
            rectangles.extend_from_slice(&frame.rectangles);
            covered = frame.id;
        }
        (covered == latest)
            .then(|| super::coalesce_dirty_rectangles(rectangles, geometry, usize::MAX))
    }
}

#[cfg(test)]
#[path = "../../tests/support/server_dirty_history_tests.rs"]
mod tests;
