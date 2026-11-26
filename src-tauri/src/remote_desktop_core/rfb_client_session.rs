use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::memory::{BudgetedBytes, ClientMemoryBudget};
use super::rfb_client_handshake::framebuffer_update_request;
use super::rfb_client_handshake::set_encodings_message_with_quality;
use super::rfb_fence::{fence_message, FENCE_FLAG_REQUEST, FENCE_MESSAGE};
use super::rfb_limits::{check_receive_capacity, MAX_CLIPBOARD_BYTES, RECEIVE_CHUNK_BYTES};
use super::rfb_receive::RfbServerMessageFramer;

#[cfg(test)]
#[path = "../../tests/support/client_session_memory_tests.rs"]
mod memory_tests;
use super::{
    assemble_discovery_frames, decode_discovery_frame, encode_discovery_frames,
    ClientFramebufferCore, ClientFramebufferHub, ClientRfbPixelFormat, DiscoveryFrame,
    DiscoveryFrameType, FramebufferGeometry, RfbClientFramebufferDecoder, RfbClientHandshake,
    RfbClientHandshakeEvent, RfbCursorUpdate, RfbDesktopScreen,
};

const END_OF_CONTINUOUS_UPDATES_MESSAGE: u8 = 150;
const EXT_CLIPBOARD_TEXT: u32 = 1;
const EXT_CLIPBOARD_CAPS: u32 = 1 << 24;
const EXT_CLIPBOARD_REQUEST: u32 = 1 << 25;
const EXT_CLIPBOARD_NOTIFY: u32 = 1 << 27;
const EXT_CLIPBOARD_PROVIDE: u32 = 1 << 28;

/// Connects the byte-level RFB handshake, incremental server-message framing,
/// decoder, and canonical framebuffer. It deliberately has no UI or socket type.
pub struct RfbClientSessionCore {
    handshake: RfbClientHandshake,
    decoder: Option<RfbClientFramebufferDecoder>,
    geometry: Option<FramebufferGeometry>,
    server_bytes: BudgetedBytes,
    memory: ClientMemoryBudget,
    event_bytes: usize,
    framer: RfbServerMessageFramer,
    framebuffer: Arc<ClientFramebufferHub>,
    events: Vec<RfbClientSessionEvent>,
    discovery_frames: Vec<DiscoveryFrame>,
    discovery_disabled: bool,
    continuous_updates_supported: bool,
    continuous_updates_enabled: bool,
    extended_clipboard_enabled: bool,
    metrics: super::RfbPerformanceMetrics,
}

#[derive(Debug, Clone)]
pub enum RfbClientCommand {
    SecureAttention {
        request_id: u32,
    },
    Key {
        down: bool,
        keysym: u32,
    },
    Pointer {
        x: u16,
        y: u16,
        button_mask: u8,
    },
    Clipboard {
        text: String,
    },
    Refresh {
        incremental: bool,
    },
    SetQuality {
        quality_level: u8,
        compression_level: u8,
    },
}

/// Internal transport seam; the native owner supplies its bounded queue while
/// standalone protocol callers can keep using an ordinary mpsc receiver.
pub(crate) trait RfbCommandSource {
    fn try_recv_command(&self) -> Result<Option<RfbClientCommand>, String>;
}

impl RfbCommandSource for Receiver<RfbClientCommand> {
    fn try_recv_command(&self) -> Result<Option<RfbClientCommand>, String> {
        Ok(self.try_recv().ok())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RfbClientSessionEvent {
    ServerRuntime(super::ServerRuntimeMessage),
    Performance(super::RfbPerformanceMetrics),
    Ready(RfbClientHandshakeEvent),
    Bell,
    Clipboard(String),
    Cursor(RfbCursorUpdate),
    PointerPosition { x: u32, y: u32 },
    DesktopLayout(Vec<RfbDesktopScreen>),
    DiscoveryManifest(serde_json::Value),
    DiscoveryError(serde_json::Value),
    DiscoveryFailure(String),
}

impl Default for RfbClientSessionCore {
    fn default() -> Self {
        Self::with_shared_framebuffer(None, Arc::new(ClientFramebufferHub::default()))
    }
}

impl RfbClientSessionCore {
    pub(crate) fn authentication(&self) -> super::RfbAuthentication {
        self.handshake.authentication.clone()
    }

    pub fn with_password(password: Option<String>) -> Self {
        Self::with_shared_framebuffer(password, Arc::new(ClientFramebufferHub::default()))
    }
    pub fn with_shared_framebuffer(
        password: Option<String>,
        framebuffer: Arc<ClientFramebufferHub>,
    ) -> Self {
        let memory = framebuffer.memory_budget();
        Self {
            handshake: RfbClientHandshake::with_password(password),
            decoder: None,
            geometry: None,
            server_bytes: BudgetedBytes::empty(&memory),
            memory,
            event_bytes: 0,
            framer: RfbServerMessageFramer::new(ClientRfbPixelFormat::core_bgra32()),
            framebuffer,
            events: Vec::new(),
            discovery_frames: Vec::new(),
            discovery_disabled: false,
            continuous_updates_supported: false,
            continuous_updates_enabled: false,
            extended_clipboard_enabled: false,
            metrics: super::RfbPerformanceMetrics::default(),
        }
    }
    pub fn with_framebuffer<R>(
        &self,
        operation: impl FnOnce(&ClientFramebufferCore) -> R,
    ) -> Result<R, String> {
        let framebuffer = self.framebuffer.lock()?;
        Ok(operation(&framebuffer))
    }
    pub fn take_events(&mut self) -> Vec<RfbClientSessionEvent> {
        self.event_bytes = 0;
        std::mem::take(&mut self.events)
    }

    /// Drain numeric counters for one monotonic runtime window, including idle time.
    pub fn take_performance_metrics(&mut self, elapsed: Duration) -> super::RfbPerformanceMetrics {
        let mut metrics = std::mem::take(&mut self.metrics);
        metrics.window_ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        metrics
    }

    /// A client is transport-ready only after ServerInit supplied geometry and
    /// the canonical framebuffer decoder has been configured.
    pub fn is_ready(&self) -> bool {
        self.geometry.is_some() && self.decoder.is_some()
    }

    /// Native presentation may start only after a decoded framebuffer has been
    /// committed. ServerInit alone does not prove the peer uses a supported
    /// server-message or framebuffer encoding dialect.
    pub fn is_presentable(&self) -> bool {
        self.framebuffer
            .lock()
            .map(|framebuffer| framebuffer.snapshot().is_some())
            .unwrap_or(false)
    }

    /// Feeds arbitrary TCP chunks and returns each RFB client message to write.
    pub fn consume(&mut self, input: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let mut writes = Vec::new();
        for chunk in input.chunks(RECEIVE_CHUNK_BYTES) {
            writes.extend(self.consume_chunk(chunk)?);
            check_client_writes(&writes)?;
        }
        Ok(writes)
    }

    fn consume_chunk(&mut self, input: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        if self.is_ready() {
            self.metrics.received_bytes = self
                .metrics
                .received_bytes
                .saturating_add(input.len() as u64);
            check_receive_capacity(self.server_bytes.len(), input.len())?;
            self.server_bytes.extend_from_slice(input)?;
        }
        let mut writes = if self.is_ready() {
            Vec::new()
        } else {
            self.handshake.consume(input)?
        };
        for event in self.handshake.take_events() {
            let RfbClientHandshakeEvent::Ready { geometry, .. } = &event;
            self.geometry = Some(*geometry);
            self.decoder = Some(RfbClientFramebufferDecoder::with_memory(
                ClientRfbPixelFormat::core_bgra32(),
                self.memory.clone(),
            )?);
            self.push_event(RfbClientSessionEvent::Ready(event))?;
        }
        let pending = self.handshake.take_pending_bytes();
        check_receive_capacity(self.server_bytes.len(), pending.len())?;
        self.server_bytes.extend_from_slice(&pending)?;
        while let Some(length) = self.framer.next_message_length(&self.server_bytes)? {
            // Keep the completed message in the existing allocation. `drain().collect()`
            // copied every compressed framebuffer update before decoding, which is
            // especially costly for 4K Tight/ZRLE payloads.
            let trailing = self.server_bytes.split_off(length)?;
            let message = std::mem::replace(&mut self.server_bytes, trailing);
            match message[0] {
                0 => {
                    let geometry = self
                        .geometry
                        .ok_or_else(|| "rfb_framebuffer_before_server_init".to_string())?;
                    let decode_started = Instant::now();
                    let outcome = self
                        .decoder
                        .as_mut()
                        .expect("decoder follows Ready")
                        .decode_to_hub(&message, geometry, &self.framebuffer)?;
                    self.metrics.record_update(
                        outcome.frame_id.is_some(),
                        decode_started.elapsed(),
                        outcome.encodings,
                    );
                    if outcome.frame_id.is_some() {
                        self.framebuffer.notify_frame_committed();
                    }
                    if let Some(cursor) = outcome.cursor {
                        self.push_event(RfbClientSessionEvent::Cursor(cursor))?;
                    }
                    if let Some((x, y)) = outcome.pointer_position {
                        self.push_event(RfbClientSessionEvent::PointerPosition { x, y })?;
                    }
                    if let Some(layout) = outcome.desktop_layout {
                        self.push_event(RfbClientSessionEvent::DesktopLayout(layout))?;
                    }
                    let next_geometry = self.framebuffer.lock()?.geometry().unwrap_or(geometry);
                    self.geometry = Some(next_geometry);
                    // Standard peers confirm support with their first message 150;
                    // accept the old empty -313 rectangle through the same state.
                    let first_support =
                        outcome.continuous_updates_supported && !self.continuous_updates_supported;
                    if first_support
                        || (outcome.geometry_changed && self.continuous_updates_enabled)
                    {
                        writes.push(enable_continuous_updates(next_geometry));
                        self.continuous_updates_supported = true;
                        self.continuous_updates_enabled = true;
                    }
                    if outcome.full_refresh_required || !self.continuous_updates_enabled {
                        writes.push(framebuffer_update_request(
                            next_geometry,
                            !outcome.full_refresh_required,
                        ));
                    }
                }
                1 => {} // SetColorMapEntries is irrelevant to negotiated true-colour pixels.
                2 => self.push_event(RfbClientSessionEvent::Bell)?,
                3 => {
                    self.consume_server_cut_text(&message, &mut writes)?;
                }
                FENCE_MESSAGE => self.consume_server_fence(&message, &mut writes)?,
                END_OF_CONTINUOUS_UPDATES_MESSAGE => {
                    if let Some(geometry) = self.geometry {
                        if !self.continuous_updates_supported {
                            self.continuous_updates_supported = true;
                            self.continuous_updates_enabled = true;
                            writes.push(enable_continuous_updates(geometry));
                        } else {
                            self.continuous_updates_enabled = false;
                            writes.push(framebuffer_update_request(geometry, true));
                        }
                    }
                }
                message_type => {
                    return Err(format!(
                        "rfb_unsupported_server_message_type:{message_type}"
                    ))
                }
            }
            check_client_writes(&writes)?;
            if self.server_bytes.is_empty() {
                self.server_bytes = message;
                self.server_bytes.clear();
            }
        }
        Ok(writes)
    }

    fn consume_server_fence(
        &mut self,
        message: &[u8],
        writes: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        let flags = u32::from_be_bytes(message[4..8].try_into().expect("fence header"));
        let payload = &message[9..];
        if flags & FENCE_FLAG_REQUEST == 0 {
            return Ok(());
        }
        // Output is written after consume returns and following messages may
        // already have been decoded. Do not promise a synchronization barrier.
        writes.push(fence_message(0, payload)?);
        if let Some(notice) = super::ServerRuntimeMessage::decode(payload) {
            if !matches!(
                notice,
                super::ServerRuntimeMessage::SecureAttentionRequest { .. }
            ) {
                self.push_event(RfbClientSessionEvent::ServerRuntime(notice))?;
            }
            return Ok(());
        }
        if self.discovery_disabled {
            return Ok(());
        }
        let Some(frame) = decode_discovery_frame(payload) else {
            return Ok(());
        };
        if frame.frame_type == DiscoveryFrameType::Hello {
            let request = encode_discovery_frames(
                DiscoveryFrameType::Discover,
                frame.request_id,
                &serde_json::json!({ "version": "1.0" }),
            )?;
            for payload in request {
                writes.push(fence_message(FENCE_FLAG_REQUEST, &payload)?);
            }
            return Ok(());
        }
        let same_exchange = self.discovery_frames.first().is_none_or(|first| {
            first.frame_type == frame.frame_type
                && first.request_id == frame.request_id
                && first.total == frame.total
        });
        if !same_exchange && frame.sequence == 0 {
            self.discovery_frames.clear();
        }
        if !same_exchange && frame.sequence != 0
            || usize::from(frame.sequence) != self.discovery_frames.len()
        {
            self.discovery_frames.clear();
            self.discovery_disabled = true;
            self.push_event(RfbClientSessionEvent::DiscoveryFailure(
                "discovery_fragment_sequence_invalid".into(),
            ))?;
            return Ok(());
        }
        // One incomplete exchange, at most 255 unique fragments of 54 bytes.
        let frame_type = frame.frame_type;
        let complete = usize::from(frame.sequence) + 1 == usize::from(frame.total);
        self.discovery_frames.push(frame);
        if !complete {
            return Ok(());
        }
        let value = assemble_discovery_frames(&self.discovery_frames);
        self.discovery_frames.clear();
        let event = match value {
            Some(value) if frame_type == DiscoveryFrameType::Manifest => {
                RfbClientSessionEvent::DiscoveryManifest(value)
            }
            Some(value) if frame_type == DiscoveryFrameType::Error => {
                RfbClientSessionEvent::DiscoveryError(value)
            }
            Some(_) => {
                RfbClientSessionEvent::DiscoveryFailure("discovery_unexpected_frame_type".into())
            }
            None => RfbClientSessionEvent::DiscoveryFailure("discovery_manifest_invalid".into()),
        };
        self.push_event(event)
    }

    fn push_event(&mut self, event: RfbClientSessionEvent) -> Result<(), String> {
        let bytes = self
            .event_bytes
            .checked_add(event_owned_bytes(&event))
            .filter(|bytes| *bytes <= 8 * 1024 * 1024)
            .ok_or("rfb_resource_limit:events")?;
        if self.events.len() >= 256 {
            return Err("rfb_resource_limit:events".into());
        }
        self.events.push(event);
        self.event_bytes = bytes;
        Ok(())
    }

    fn consume_server_cut_text(
        &mut self,
        message: &[u8],
        writes: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        let signed_length = i32::from_be_bytes(message[4..8].try_into().expect("clipboard length"));
        let payload = &message[8..];
        if signed_length >= 0 {
            // Keep existing UTF-8 peers readable; invalid UTF-8 is the legacy
            // ISO-8859-1 byte encoding, not text to discard with replacement characters.
            let text = match std::str::from_utf8(payload) {
                Ok(text) => text.to_owned(),
                Err(_) => payload.iter().map(|byte| char::from(*byte)).collect(),
            };
            self.push_event(RfbClientSessionEvent::Clipboard(text))?;
            return Ok(());
        }
        if payload.len() < 4 {
            return Err("rfb_extended_clipboard_payload_too_short".into());
        }
        let flags = u32::from_be_bytes(payload[..4].try_into().expect("clipboard flags"));
        let action = flags & 0xff00_0000;
        if action & EXT_CLIPBOARD_CAPS != 0 {
            self.extended_clipboard_enabled = true;
            writes.push(extended_clipboard_message(
                EXT_CLIPBOARD_CAPS
                    | EXT_CLIPBOARD_REQUEST
                    | EXT_CLIPBOARD_NOTIFY
                    | EXT_CLIPBOARD_PROVIDE
                    | EXT_CLIPBOARD_TEXT,
                &1_048_576_u32.to_be_bytes(),
            )?);
        } else if action == EXT_CLIPBOARD_NOTIFY && flags & EXT_CLIPBOARD_TEXT != 0 {
            writes.push(extended_clipboard_message(
                EXT_CLIPBOARD_REQUEST | EXT_CLIPBOARD_TEXT,
                &[],
            )?);
        } else if action == EXT_CLIPBOARD_PROVIDE && flags & EXT_CLIPBOARD_TEXT != 0 {
            let mut inflated = Vec::new();
            ZlibDecoder::new(&payload[4..])
                .take((MAX_CLIPBOARD_BYTES + 5) as u64)
                .read_to_end(&mut inflated)
                .map_err(|_| "rfb_extended_clipboard_inflate_failed")?;
            if inflated.len() > MAX_CLIPBOARD_BYTES + 4 {
                return Err("rfb_resource_limit:clipboard_inflate".into());
            }
            if inflated.len() < 4 {
                return Err("rfb_extended_clipboard_text_size_missing".into());
            }
            let length =
                u32::from_be_bytes(inflated[..4].try_into().expect("clipboard text length"))
                    as usize;
            if length > MAX_CLIPBOARD_BYTES {
                return Err("rfb_resource_limit:clipboard_inflate".into());
            }
            if inflated.len() < 4 + length {
                return Err("rfb_extended_clipboard_text_truncated".into());
            }
            let mut text = String::from_utf8(inflated[4..4 + length].to_vec())
                .map_err(|_| "rfb_extended_clipboard_invalid_utf8")?;
            if text.ends_with('\0') {
                text.pop();
            }
            self.push_event(RfbClientSessionEvent::Clipboard(text.replace("\r\n", "\n")))?;
        }
        Ok(())
    }

    fn command_to_wire(&self, command: &RfbClientCommand) -> Result<Vec<u8>, String> {
        if let RfbClientCommand::Clipboard { text } = command {
            if text.len() > MAX_CLIPBOARD_BYTES {
                return Err("rfb_resource_limit:clipboard".into());
            }
            if self.extended_clipboard_enabled {
                let mut text_bytes = text.replace('\n', "\r\n").into_bytes();
                text_bytes.push(0);
                let mut source = Vec::with_capacity(4 + text_bytes.len());
                source.extend_from_slice(&(text_bytes.len() as u32).to_be_bytes());
                source.extend_from_slice(&text_bytes);
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
                encoder
                    .write_all(&source)
                    .map_err(|error| format!("rfb_extended_clipboard_deflate_failed:{error}"))?;
                let compressed = encoder
                    .finish()
                    .map_err(|error| format!("rfb_extended_clipboard_deflate_failed:{error}"))?;
                return extended_clipboard_message(
                    EXT_CLIPBOARD_PROVIDE | EXT_CLIPBOARD_TEXT,
                    &compressed,
                );
            }
        }
        command.to_wire(self.geometry)
    }
}

fn check_client_writes(writes: &[Vec<u8>]) -> Result<(), String> {
    let total = writes
        .iter()
        .try_fold(0usize, |total, bytes| total.checked_add(bytes.capacity()));
    if writes.len() > 4096 || total.is_none_or(|total| total > 1024 * 1024) {
        return Err("rfb_resource_limit:client_messages".into());
    }
    Ok(())
}

fn event_owned_bytes(event: &RfbClientSessionEvent) -> usize {
    std::mem::size_of::<RfbClientSessionEvent>()
        + match event {
            RfbClientSessionEvent::Ready(RfbClientHandshakeEvent::Ready {
                desktop_name, ..
            }) => desktop_name.capacity(),
            RfbClientSessionEvent::Clipboard(text)
            | RfbClientSessionEvent::DiscoveryFailure(text) => text.capacity(),
            RfbClientSessionEvent::Cursor(cursor) => cursor.pixels.capacity(),
            RfbClientSessionEvent::DesktopLayout(screens) => {
                screens.capacity() * std::mem::size_of::<RfbDesktopScreen>()
            }
            RfbClientSessionEvent::DiscoveryManifest(value)
            | RfbClientSessionEvent::DiscoveryError(value) => json_owned_bytes(value),
            _ => 0,
        }
}
fn json_owned_bytes(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(text) => text.capacity(),
        serde_json::Value::Array(values) => {
            values.capacity() * std::mem::size_of::<serde_json::Value>()
                + values.iter().map(json_owned_bytes).sum::<usize>()
        }
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(key, value)| 256 + key.capacity() + json_owned_bytes(value))
            .sum(),
        _ => 0,
    }
}

fn extended_clipboard_message(flags: u32, data: &[u8]) -> Result<Vec<u8>, String> {
    let payload_length = 4usize
        .checked_add(data.len())
        .ok_or("rfb_clipboard_too_large")?;
    let signed_length = -i32::try_from(payload_length).map_err(|_| "rfb_clipboard_too_large")?;
    let mut bytes = vec![6, 0, 0, 0];
    bytes.extend_from_slice(&signed_length.to_be_bytes());
    bytes.extend_from_slice(&flags.to_be_bytes());
    bytes.extend_from_slice(data);
    Ok(bytes)
}

fn enable_continuous_updates(geometry: FramebufferGeometry) -> Vec<u8> {
    let mut bytes = vec![150, 1, 0, 0, 0, 0];
    bytes.extend_from_slice(
        &u16::try_from(geometry.width)
            .unwrap_or(u16::MAX)
            .to_be_bytes(),
    );
    bytes.extend_from_slice(
        &u16::try_from(geometry.height)
            .unwrap_or(u16::MAX)
            .to_be_bytes(),
    );
    bytes
}

/// Blocking transport adapter for the new Core. The application can own its
/// lifecycle in a dedicated connection task without exposing TCP to UI code.
pub fn run_rfb_client_tcp_loop<S: Read + Write>(
    stream: &mut S,
    session: &mut RfbClientSessionCore,
) -> Result<(), String> {
    run_rfb_client_tcp_loop_until(stream, session, &AtomicBool::new(false))
}

pub fn run_rfb_client_tcp_loop_until<S: Read + Write>(
    stream: &mut S,
    session: &mut RfbClientSessionCore,
    stop: &AtomicBool,
) -> Result<(), String> {
    run_rfb_client_tcp_loop_with_commands(stream, session, stop, None)
}

pub fn run_rfb_client_tcp_loop_with_commands<S: Read + Write>(
    stream: &mut S,
    session: &mut RfbClientSessionCore,
    stop: &AtomicBool,
    commands: Option<&Receiver<RfbClientCommand>>,
) -> Result<(), String> {
    run_rfb_client_tcp_loop_with_commands_and_ready(stream, session, stop, commands, || {})
}

/// Like `run_rfb_client_tcp_loop_with_commands`, but reports readiness exactly
/// once after the first decoded framebuffer commit. The caller still owns
/// lifecycle and error propagation outside the protocol core.
pub fn run_rfb_client_tcp_loop_with_commands_and_ready<S: Read + Write, F: FnMut()>(
    stream: &mut S,
    session: &mut RfbClientSessionCore,
    stop: &AtomicBool,
    commands: Option<&Receiver<RfbClientCommand>>,
    on_ready: F,
) -> Result<(), String> {
    run_rfb_client_tcp_loop_with_commands_ready_and_events(
        stream,
        session,
        stop,
        commands,
        on_ready,
        |_| {},
    )
}

pub fn run_rfb_client_tcp_loop_with_commands_ready_and_events<
    S: Read + Write,
    F: FnMut(),
    E: FnMut(RfbClientSessionEvent),
>(
    stream: &mut S,
    session: &mut RfbClientSessionCore,
    stop: &AtomicBool,
    commands: Option<&Receiver<RfbClientCommand>>,
    on_ready: F,
    on_event: E,
) -> Result<(), String> {
    run_rfb_client_tcp_loop_with_command_source(
        stream,
        session,
        stop,
        commands.map(|commands| commands as &dyn RfbCommandSource),
        on_ready,
        on_event,
    )
}

pub(crate) fn run_rfb_client_tcp_loop_with_command_source<
    S: Read + Write,
    F: FnMut(),
    E: FnMut(RfbClientSessionEvent),
>(
    stream: &mut S,
    session: &mut RfbClientSessionCore,
    stop: &AtomicBool,
    commands: Option<&dyn RfbCommandSource>,
    mut on_ready: F,
    mut on_event: E,
) -> Result<(), String> {
    let mut read_buffer = [0_u8; 16 * 1024];
    let mut ready_reported = false;
    let mut metrics_started: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        if session.is_ready() {
            let started = metrics_started.get_or_insert_with(Instant::now);
            let elapsed = started.elapsed();
            if elapsed >= Duration::from_secs(5) {
                on_event(RfbClientSessionEvent::Performance(
                    session.take_performance_metrics(elapsed),
                ));
                *started = Instant::now();
            }
        }
        if let Some(commands) = commands {
            // Bound each input batch so a busy producer cannot starve reads.
            for _ in 0..64 {
                if stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                let Some(command) = commands.try_recv_command()? else {
                    break;
                };
                let bytes = session.command_to_wire(&command)?;
                timed_write(stream, &bytes, session)?;
            }
            timed_flush(stream, session)?;
        }
        let read_started = Instant::now();
        let result = stream.read(&mut read_buffer);
        if session.is_ready() {
            session.metrics.record_read(
                read_started.elapsed(),
                result.as_ref().is_err_and(|error| {
                    matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    )
                }),
            );
        }
        let read = match result {
            Ok(read) => read,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                continue
            }
            Err(error) => return Err(read_failure_code(error.kind()).into()),
        };
        if read == 0 {
            return Ok(());
        }
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let consume_started = Instant::now();
        let writes = session.consume(&read_buffer[..read]);
        if session.is_ready() {
            session.metrics.consume_total_us = session
                .metrics
                .consume_total_us
                .saturating_add(super::rfb_metrics::micros(consume_started.elapsed()));
        }
        for write in writes? {
            timed_write(stream, &write, session)?;
        }
        let event_started = Instant::now();
        for event in session.take_events() {
            on_event(event);
        }
        if !ready_reported && session.is_presentable() {
            ready_reported = true;
            on_ready();
        }
        if session.is_ready() {
            session.metrics.event_total_us = session
                .metrics
                .event_total_us
                .saturating_add(super::rfb_metrics::micros(event_started.elapsed()));
        }
        timed_flush(stream, session)?;
    }
    Ok(())
}

// Preserve only operation and known ErrorKind constants, never the OS payload.
fn read_failure_code(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::ConnectionReset => "rfb_tcp_read_reset",
        std::io::ErrorKind::ConnectionAborted => "rfb_tcp_read_aborted",
        _ => "rfb_tcp_read_failed",
    }
}

fn timed_write<S: Write>(
    stream: &mut S,
    bytes: &[u8],
    session: &mut RfbClientSessionCore,
) -> Result<(), String> {
    let started = Instant::now();
    let result = stream.write_all(bytes);
    if session.is_ready() {
        session.metrics.record_write(started.elapsed());
    }
    result.map_err(|error| {
        match error.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                "rfb_tcp_write_timeout"
            }
            std::io::ErrorKind::ConnectionReset => "rfb_tcp_write_reset",
            std::io::ErrorKind::BrokenPipe => "rfb_tcp_write_broken_pipe",
            _ => "rfb_tcp_write_failed",
        }
        .to_string()
    })
}

fn timed_flush<S: Write>(stream: &mut S, session: &mut RfbClientSessionCore) -> Result<(), String> {
    let started = Instant::now();
    let result = stream.flush();
    if session.is_ready() {
        session.metrics.record_write(started.elapsed());
    }
    result.map_err(|_| "rfb_tcp_flush_failed".to_string())
}

impl RfbClientCommand {
    fn to_wire(&self, geometry: Option<FramebufferGeometry>) -> Result<Vec<u8>, String> {
        match self {
            Self::SecureAttention { request_id } => fence_message(
                FENCE_FLAG_REQUEST,
                &super::ServerRuntimeMessage::SecureAttentionRequest {
                    request_id: *request_id,
                }
                .encode(),
            ),
            Self::Key { down, keysym } => {
                let mut bytes = vec![4, u8::from(*down), 0, 0];
                bytes.extend_from_slice(&keysym.to_be_bytes());
                Ok(bytes)
            }
            Self::Pointer { x, y, button_mask } => {
                let mut bytes = vec![5, *button_mask];
                bytes.extend_from_slice(&x.to_be_bytes());
                bytes.extend_from_slice(&y.to_be_bytes());
                Ok(bytes)
            }
            Self::Clipboard { text } => {
                let mut bytes = vec![6, 0, 0, 0];
                bytes.extend_from_slice(
                    &(u32::try_from(text.len()).map_err(|_| "rfb_clipboard_too_large")?)
                        .to_be_bytes(),
                );
                bytes.extend_from_slice(text.as_bytes());
                Ok(bytes)
            }
            Self::Refresh { incremental } => Ok(framebuffer_update_request(
                geometry.ok_or_else(|| "rfb_framebuffer_before_server_init".to_string())?,
                *incremental,
            )),
            Self::SetQuality {
                quality_level,
                compression_level,
            } => set_encodings_message_with_quality(*quality_level, *compression_level),
        }
    }
}

#[cfg(test)]
mod quality_tests {
    use super::RfbClientCommand;

    #[test]
    fn quality_command_resends_set_encodings_with_tight_pseudo_levels() {
        let wire = RfbClientCommand::SetQuality {
            quality_level: 9,
            compression_level: 0,
        }
        .to_wire(None)
        .unwrap();
        assert_eq!(&wire[..2], &[2, 0]);
        let encodings = wire[4..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| i32::from_be_bytes(*bytes))
            .collect::<Vec<_>>();
        assert_eq!(
            usize::from(u16::from_be_bytes([wire[2], wire[3]])),
            encodings.len()
        );
        assert!(encodings.contains(&-256));
        assert!(encodings.contains(&-23));
        assert!(encodings.contains(&-317));
        assert!(encodings.contains(&-312));
    }

    #[test]
    fn quality_command_rejects_levels_outside_tight_range() {
        assert!(RfbClientCommand::SetQuality {
            quality_level: 10,
            compression_level: 0,
        }
        .to_wire(None)
        .is_err());
    }
}
