use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfbEncodingCounts {
    pub rectangles: u64,
    pub copy_rect: u64,
    pub tight: u64,
    pub jpeg: u64,
}

/// Numeric window only: never retain wire bytes, desktop names or input values.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfbPerformanceMetrics {
    pub window_ms: u64,
    pub received_bytes: u64,
    pub updates: u64,
    pub committed_frames: u64,
    pub decode_total_us: u64,
    pub decode_max_us: u64,
    pub read_calls: u64,
    pub read_timeouts: u64,
    pub read_total_us: u64,
    pub consume_total_us: u64,
    pub write_total_us: u64,
    pub write_max_us: u64,
    pub event_total_us: u64,
    pub encodings: RfbEncodingCounts,
}

impl RfbPerformanceMetrics {
    pub(super) fn record_read(&mut self, elapsed: Duration, timed_out: bool) {
        self.read_calls = self.read_calls.saturating_add(1);
        self.read_timeouts = self.read_timeouts.saturating_add(u64::from(timed_out));
        self.read_total_us = self.read_total_us.saturating_add(micros(elapsed));
    }

    pub(super) fn record_write(&mut self, elapsed: Duration) {
        let elapsed = micros(elapsed);
        self.write_total_us = self.write_total_us.saturating_add(elapsed);
        self.write_max_us = self.write_max_us.max(elapsed);
    }

    pub(super) fn record_update(
        &mut self,
        committed: bool,
        elapsed: Duration,
        encodings: RfbEncodingCounts,
    ) {
        let micros = micros(elapsed);
        self.updates = self.updates.saturating_add(1);
        self.committed_frames = self.committed_frames.saturating_add(u64::from(committed));
        self.decode_total_us = self.decode_total_us.saturating_add(micros);
        self.decode_max_us = self.decode_max_us.max(micros);
        self.encodings.rectangles = self
            .encodings
            .rectangles
            .saturating_add(encodings.rectangles);
        self.encodings.copy_rect = self.encodings.copy_rect.saturating_add(encodings.copy_rect);
        self.encodings.tight = self.encodings.tight.saturating_add(encodings.tight);
        self.encodings.jpeg = self.encodings.jpeg.saturating_add(encodings.jpeg);
    }
}

pub(super) fn micros(elapsed: Duration) -> u64 {
    elapsed.as_micros().min(u128::from(u64::MAX)) as u64
}
