//! A replaceable local RFB desktop source. Public client sockets and encoder state
//! stay with the broker; only this authenticated loopback connection is replaced.
use crate::remote_desktop_core::{
    run_rfb_client_tcp_loop_with_commands_ready_and_events, ClientFramebufferHub,
    ClientFramebufferSnapshot, DesktopAccessState, RfbClientCommand, RfbClientSessionCore,
    RfbClientSessionEvent, SecureAttentionResult, ServerRuntimeMessage,
};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct DesktopRelay {
    state: Mutex<State>,
    task: Mutex<Option<Task>>,
}

impl std::fmt::Debug for DesktopRelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopRelay").finish_non_exhaustive()
    }
}

#[derive(Default)]
struct State {
    epoch: u64,
    next_request: u32,
    source: Option<Source>,
    pending: BTreeMap<u32, mpsc::SyncSender<SecureAttentionResult>>,
}

struct Source {
    frames: Arc<ClientFramebufferHub>,
    commands: mpsc::SyncSender<RfbClientCommand>,
    control: Arc<Control>,
    desktop: DesktopAccessState,
    frame_floor: u64,
    sas: bool,
}

struct Control {
    stop: AtomicBool,
    socket: TcpStream,
}
impl Control {
    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}
struct Task {
    control: Arc<Control>,
    worker: JoinHandle<()>,
}
impl Task {
    fn stop(self) {
        self.control.stop();
        let _ = self.worker.join();
    }
}

impl DesktopRelay {
    /// Invalidate pixels and queued input before terminating the old source.
    pub(crate) fn detach(&self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.epoch = state.epoch.saturating_add(1);
            state.source = None;
            state.pending.clear();
        }
        let task = self.task.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(task) = task {
            task.stop();
        }
    }

    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    pub(crate) fn attach(self: &Arc<Self>, port: u16, password: String) -> Result<(), String> {
        self.detach();
        let frames = Arc::new(ClientFramebufferHub::default());
        let mut session =
            RfbClientSessionCore::with_shared_framebuffer(Some(password), Arc::clone(&frames));
        let mut stream = TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            Duration::from_millis(500),
        )
        .map_err(|_| "windows_service_desktop_relay_unavailable".to_string())?;
        stream
            .set_nodelay(true)
            .map_err(|_| "windows_service_desktop_relay_unavailable")?;
        stream
            .set_read_timeout(Some(Duration::from_millis(25)))
            .map_err(|_| "windows_service_desktop_relay_unavailable")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|_| "windows_service_desktop_relay_unavailable")?;
        let control = Arc::new(Control {
            stop: AtomicBool::new(false),
            socket: stream
                .try_clone()
                .map_err(|_| "windows_service_desktop_relay_unavailable")?,
        });
        let (commands, receiver) = mpsc::sync_channel(256);
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.source = Some(Source {
                frames,
                commands,
                control: Arc::clone(&control),
                desktop: DesktopAccessState::Waiting,
                frame_floor: 0,
                sas: false,
            });
        }
        let relay = Arc::downgrade(self);
        let worker_control = Arc::clone(&control);
        let worker = std::thread::Builder::new()
            .name("service-desktop-relay".into())
            .spawn(move || {
                let _ = run_rfb_client_tcp_loop_with_commands_ready_and_events(
                    &mut stream,
                    &mut session,
                    &worker_control.stop,
                    Some(&receiver),
                    || {},
                    |event| {
                        if let Some(relay) = relay.upgrade() {
                            relay.consume(event);
                        }
                    },
                );
                if let Some(relay) = relay.upgrade() {
                    let mut state = relay.state.lock().unwrap_or_else(|e| e.into_inner());
                    if state
                        .source
                        .as_ref()
                        .is_some_and(|source| Arc::ptr_eq(&source.control, &worker_control))
                    {
                        state.epoch = state.epoch.saturating_add(1);
                        state.source = None;
                        state.pending.clear();
                    }
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(_) => {
                control.stop();
                self.detach();
                return Err("windows_service_desktop_relay_unavailable".into());
            }
        };
        *self.task.lock().unwrap_or_else(|e| e.into_inner()) = Some(Task { control, worker });
        Ok(())
    }

    fn consume(&self, event: RfbClientSessionEvent) {
        let RfbClientSessionEvent::ServerRuntime(notice) = event else {
            return;
        };
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match notice {
            ServerRuntimeMessage::Desktop {
                desktop,
                secure_attention_available,
            } => {
                let Some(source) = state.source.as_mut() else {
                    return;
                };
                let invalidating = desktop != DesktopAccessState::Available
                    && source.desktop == DesktopAccessState::Available;
                if invalidating {
                    source.frame_floor = source
                        .frames
                        .lock()
                        .ok()
                        .and_then(|frames| frames.snapshot().map(|frame| frame.frame_id))
                        .unwrap_or(0);
                }
                source.desktop = desktop;
                source.sas = secure_attention_available;
                if invalidating {
                    state.epoch = state.epoch.saturating_add(1);
                    state.pending.clear();
                }
            }
            ServerRuntimeMessage::SecureAttentionResponse { request_id, result } => {
                if let Some(pending) = state.pending.remove(&request_id) {
                    let _ = pending.try_send(result);
                }
            }
            _ => {}
        }
    }

    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    pub(crate) fn attached(&self) -> bool {
        self.state.lock().is_ok_and(|state| {
            state
                .source
                .as_ref()
                .is_some_and(|source| !source.control.stop.load(Ordering::Acquire))
        })
    }

    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    pub(crate) fn ready(&self) -> bool {
        self.with_frame(|_, _| ()).is_some()
    }

    /// Borrow canonical pixels only while they belong to the active desktop and
    /// follow its recovery floor. Old pixels never serve as a new desktop frame.
    pub(super) fn with_frame<R>(
        &self,
        read: impl FnOnce(u64, &ClientFramebufferSnapshot<'_>) -> R,
    ) -> Option<R> {
        let state = self.state.lock().ok()?;
        let source = state.source.as_ref()?;
        if source.desktop != DesktopAccessState::Available
            || source.control.stop.load(Ordering::Acquire)
        {
            return None;
        }
        let frames = source.frames.lock().ok()?;
        let frame = frames.snapshot()?;
        if frame.frame_id <= source.frame_floor {
            return None;
        }
        Some(read(state.epoch, &frame))
    }

    pub(crate) fn accepts(&self, epoch: u64) -> bool {
        self.with_frame(|current, _| current == epoch)
            .unwrap_or(false)
    }

    pub(crate) fn send(&self, epoch: u64, command: RfbClientCommand) {
        if !self.accepts(epoch) {
            return;
        }
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.epoch != epoch {
            return;
        }
        if let Some(source) = &state.source {
            if source.commands.try_send(command).is_err() {
                // Never drop a release behind a full input queue. Replacing the
                // local source releases its held input; public sockets stay up.
                source.control.stop();
            }
        }
    }

    pub(crate) fn sas_available(&self) -> bool {
        self.state.lock().is_ok_and(|state| {
            state
                .source
                .as_ref()
                .is_some_and(|source| source.sas && !source.control.stop.load(Ordering::Acquire))
        })
    }

    pub(crate) fn secure_attention(&self) -> SecureAttentionResult {
        let (request_id, receiver) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.pending.len() >= 16 {
                return SecureAttentionResult::Failed;
            }
            state.next_request = state.next_request.wrapping_add(1).max(1);
            let request_id = state.next_request;
            let Some(source) = state
                .source
                .as_ref()
                .filter(|source| source.sas && !source.control.stop.load(Ordering::Acquire))
            else {
                return SecureAttentionResult::Unsupported;
            };
            let (sender, receiver) = mpsc::sync_channel(1);
            if source
                .commands
                .try_send(RfbClientCommand::SecureAttention { request_id })
                .is_err()
            {
                return SecureAttentionResult::Failed;
            }
            state.pending.insert(request_id, sender);
            (request_id, receiver)
        };
        let result = receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or(SecureAttentionResult::Failed);
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending
            .remove(&request_id);
        result
    }
}

impl Drop for DesktopRelay {
    fn drop(&mut self) {
        if let Some(task) = self
            .task
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            task.stop();
        }
    }
}

/// Input belongs to one public reader and one desktop epoch. Never replay held
/// input across replacement workers; reader teardown releases only its own keys.
pub(crate) struct RelayInputOwner {
    relay: Arc<DesktopRelay>,
    epoch: Option<u64>,
    keys: std::collections::BTreeSet<u32>,
    pointer: Option<(u16, u16)>,
}
impl RelayInputOwner {
    pub(crate) fn new(relay: Arc<DesktopRelay>) -> Self {
        Self {
            relay,
            epoch: None,
            keys: Default::default(),
            pointer: None,
        }
    }
    fn prepare(&mut self, display: Option<&super::SharedDisplay>) -> Option<u64> {
        let epoch = display
            .filter(|display| self.relay.accepts(display.desktop_epoch))
            .map(|display| display.desktop_epoch);
        if epoch != self.epoch {
            self.keys.clear();
            self.pointer = None;
            self.epoch = epoch;
        }
        epoch
    }
    pub(crate) fn key(&mut self, display: Option<&super::SharedDisplay>, down: bool, keysym: u32) {
        let Some(epoch) = self.prepare(display) else {
            return;
        };
        if down && self.keys.len() >= 256 && !self.keys.contains(&keysym) {
            return;
        }
        self.relay
            .send(epoch, RfbClientCommand::Key { down, keysym });
        if down {
            self.keys.insert(keysym);
        } else {
            self.keys.remove(&keysym);
        }
    }
    pub(crate) fn pointer(
        &mut self,
        display: &super::SharedDisplay,
        x: u16,
        y: u16,
        button_mask: u8,
    ) {
        let Some(epoch) = self.prepare(Some(display)) else {
            return;
        };
        let point =
            super::map_vnc_point_to_display(x, y, display.framebuffer, display.frame.clone());
        let x = point.x.clamp(0, u16::MAX as i32) as u16;
        let y = point.y.clamp(0, u16::MAX as i32) as u16;
        self.relay
            .send(epoch, RfbClientCommand::Pointer { x, y, button_mask });
        self.pointer = (button_mask & 7 != 0).then_some((x, y));
    }
}
impl Drop for RelayInputOwner {
    fn drop(&mut self) {
        let Some(epoch) = self.epoch else {
            return;
        };
        for &keysym in &self.keys {
            self.relay.send(
                epoch,
                RfbClientCommand::Key {
                    down: false,
                    keysym,
                },
            );
        }
        if let Some((x, y)) = self.pointer {
            self.relay.send(
                epoch,
                RfbClientCommand::Pointer {
                    x,
                    y,
                    button_mask: 0,
                },
            );
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/desktop_relay_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/support/desktop_relay_fixture.rs"]
pub(crate) mod test_support;
