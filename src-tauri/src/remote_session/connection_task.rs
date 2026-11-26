use super::input_queue::RfbInputQueue;
use super::types::{RemoteSessionRfbConnectRequest, RemoteSessionRfbEvent};
use crate::remote_desktop_core::{
    run_rfb_client_tcp_loop_with_command_source, ClientFramebufferHub, RfbClientCommand,
    RfbClientSessionCore, RfbClientSessionEvent,
};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "../../tests/support/rfb_auth_cancel_tests.rs"]
mod auth_cancel_tests;

#[cfg(test)]
#[path = "../../tests/support/rfb_input_queue_tests.rs"]
mod input_queue_tests;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(6);

struct ConnectionControl {
    stop: AtomicBool,
    socket: Mutex<Option<TcpStream>>,
    authentication: crate::remote_desktop_core::RfbAuthentication,
    inputs: RfbInputQueue,
}

struct InputQueueCleanup<'a>(&'a RfbInputQueue);

impl Drop for InputQueueCleanup<'_> {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl ConnectionControl {
    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.authentication.clear();
        self.inputs.close();
        if let Ok(socket) = self.socket.lock() {
            if let Some(socket) = socket.as_ref() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
    }

    fn register_socket(&self, stream: &TcpStream) -> Result<(), String> {
        let mut socket = self.socket.lock().map_err(|_| "rfb_socket_lock_poisoned")?;
        if self.stop.load(Ordering::Acquire) {
            return Err("rfb_cancelled".into());
        }
        *socket = Some(stream.try_clone().map_err(|_| "rfb_socket_clone_failed")?);
        Ok(())
    }
}

/// Owns one connection worker and its shutdown socket. Timed-out DNS workers
/// keep their slot and JoinHandle until they exit; retry cannot overlap them.
pub(super) struct RfbConnectionTask {
    control: Arc<ConnectionControl>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

type ConnectionStartup = (Arc<RfbConnectionTask>, Receiver<Result<(), String>>);

impl RfbConnectionTask {
    pub(super) fn spawn<F>(
        mut request: RemoteSessionRfbConnectRequest,
        framebuffer: Arc<ClientFramebufferHub>,
        failure: Arc<Mutex<Option<String>>>,
        on_event: F,
    ) -> Result<ConnectionStartup, String>
    where
        F: Fn(RemoteSessionRfbEvent) + Send + 'static,
    {
        // Derive bounded auth material and erase the owned plaintext before DNS
        // or thread creation can block/fail. Drop also clears a cancelled key.
        let session =
            RfbClientSessionCore::with_shared_framebuffer(request.password.take(), framebuffer);
        let control = Arc::new(ConnectionControl {
            stop: AtomicBool::new(false),
            socket: Mutex::new(None),
            authentication: session.authentication(),
            inputs: RfbInputQueue::default(),
        });
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let worker_control = Arc::clone(&control);
        let worker = thread::Builder::new()
            .name("vnc-rfb-client".into())
            .spawn(move || {
                // A worker unwind must also release queued input retained by its owner.
                let _input_cleanup = InputQueueCleanup(&worker_control.inputs);
                let mut startup_tx = Some(startup_tx);
                let result = run_connection(
                    request,
                    session,
                    &worker_control,
                    || {
                        if let Some(sender) = startup_tx.take() {
                            let _ = sender.send(Ok(()));
                        }
                    },
                    |event| {
                        if !worker_control.stop.load(Ordering::Acquire) {
                            if let Some(event) = map_event(event) {
                                on_event(event);
                            }
                        }
                    },
                );
                let cancelled = worker_control.stop.load(Ordering::Acquire);
                worker_control.inputs.close();
                let terminal = if cancelled {
                    "rfb_cancelled".into()
                } else {
                    result.err().unwrap_or_else(|| "rfb_remote_closed".into())
                };
                let terminal_failure = super::RfbFailure::from_internal(&terminal);
                if terminal_failure.category == super::RfbFailureCategory::ProtocolError {
                    // A fixed allowlisted code is sufficient for decoder diagnosis;
                    // never log the internal error's peer or OS detail suffix.
                    eprintln!("VNC RFB protocol failure: {}", terminal_failure.code);
                }
                if let Ok(mut failure) = failure.lock() {
                    *failure = Some(terminal_failure.code.into());
                }
                if let Ok(mut socket) = worker_control.socket.lock() {
                    *socket = None;
                }
                if let Some(sender) = startup_tx.take() {
                    let _ = sender.send(Err(terminal));
                } else if !cancelled {
                    on_event(RemoteSessionRfbEvent::Disconnected {
                        reason: terminal_failure.code.into(),
                        failure: terminal_failure,
                    });
                }
            })
            .map_err(|_| "rfb_worker_spawn_failed")?;
        Ok((
            Arc::new(Self {
                control,
                worker: Mutex::new(Some(worker)),
            }),
            startup_rx,
        ))
    }

    pub(super) fn is_finished(&self) -> bool {
        self.worker
            .lock()
            .map(|worker| worker.as_ref().is_none_or(JoinHandle::is_finished))
            .unwrap_or(false)
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.control.stop.load(Ordering::Acquire)
    }

    pub(super) fn request_stop(&self) {
        self.control.stop();
    }

    pub(super) fn send(&self, command: RfbClientCommand) -> Result<(), String> {
        if self.is_cancelled() || self.is_finished() {
            return Err("rfb_transport_closed".into());
        }
        self.control.inputs.push(command)
    }

    /// Called outside the session registry lock. A pending OS DNS call cannot
    /// be interrupted by std, so a deadline returns without discarding ownership.
    pub(super) fn join(&self) -> Result<(), String> {
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            let mut worker = self.worker.lock().map_err(|_| "rfb_worker_lock_poisoned")?;
            if worker.as_ref().is_none_or(JoinHandle::is_finished) {
                if let Some(worker) = worker.take() {
                    worker.join().map_err(|_| "rfb_worker_panicked")?;
                }
                return Ok(());
            }
            drop(worker);
            if Instant::now() >= deadline {
                return Err("rfb_shutdown_pending".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Drop for RfbConnectionTask {
    fn drop(&mut self) {
        self.control.stop();
    }
}

fn run_connection<F, E>(
    request: RemoteSessionRfbConnectRequest,
    mut session: RfbClientSessionCore,
    control: &ConnectionControl,
    on_ready: F,
    on_event: E,
) -> Result<(), String>
where
    F: FnMut(),
    E: FnMut(RfbClientSessionEvent),
{
    if control.stop.load(Ordering::Acquire) {
        return Err("rfb_cancelled".into());
    }
    let addresses = (request.host.as_str(), request.port)
        .to_socket_addrs()
        .map_err(|_| "rfb_resolve_failed")?;
    let mut stream =
        super::rfb_connect::connect_addresses(addresses, &control.stop, CONNECT_TIMEOUT)?;
    control.register_socket(&stream)?;
    stream
        .set_nodelay(true)
        .map_err(|_| "rfb_socket_options_failed")?;
    stream
        .set_read_timeout(Some(Duration::from_millis(8)))
        .map_err(|_| "rfb_socket_options_failed")?;
    let mut transport = super::rfb_transport::RfbTcpTransport::new(&mut stream, &control.stop);
    run_rfb_client_tcp_loop_with_command_source(
        &mut transport,
        &mut session,
        &control.stop,
        Some(&control.inputs),
        on_ready,
        on_event,
    )
}

fn map_event(event: RfbClientSessionEvent) -> Option<RemoteSessionRfbEvent> {
    Some(match event {
        RfbClientSessionEvent::ServerRuntime(notice) => {
            RemoteSessionRfbEvent::ServerRuntime { notice }
        }
        RfbClientSessionEvent::Performance(metrics) => {
            RemoteSessionRfbEvent::Performance { metrics }
        }
        RfbClientSessionEvent::Ready(_) => return None,
        RfbClientSessionEvent::Bell => RemoteSessionRfbEvent::Bell,
        RfbClientSessionEvent::Clipboard(text) => RemoteSessionRfbEvent::Clipboard { text },
        RfbClientSessionEvent::Cursor(cursor) => RemoteSessionRfbEvent::Cursor { cursor },
        RfbClientSessionEvent::PointerPosition { x, y } => {
            RemoteSessionRfbEvent::PointerPosition { x, y }
        }
        RfbClientSessionEvent::DesktopLayout(screens) => {
            RemoteSessionRfbEvent::DesktopLayout { screens }
        }
        RfbClientSessionEvent::DiscoveryManifest(manifest) => {
            RemoteSessionRfbEvent::DiscoveryManifest { manifest }
        }
        RfbClientSessionEvent::DiscoveryError(error) => {
            RemoteSessionRfbEvent::DiscoveryError { error }
        }
        RfbClientSessionEvent::DiscoveryFailure(code) => {
            RemoteSessionRfbEvent::DiscoveryFailure { code }
        }
    })
}
