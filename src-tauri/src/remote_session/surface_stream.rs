use super::surface_transport::{SurfaceAdmission, SurfaceTransport, ACK_TIMEOUT};
use super::surface_workers::{SurfaceConnection, SurfaceWorkerControl, SurfaceWorkers};
use crate::remote_desktop_core::{ClientFramebufferHub, FrameId, SurfaceFramePacket};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::{Error as WebSocketError, Message};

const SURFACE_ACK_MAGIC: [u8; 4] = *b"VNCA";
const SURFACE_ACK_LEN: usize = 12;

#[cfg(test)]
#[path = "../../tests/support/surface_lifecycle_tests.rs"]
mod lifecycle_tests;

pub(super) struct SurfaceStream {
    port: u16,
    token: String,
    control: Arc<SurfaceWorkerControl>,
    listener: Mutex<Option<thread::JoinHandle<Result<(), String>>>>,
}

impl SurfaceStream {
    pub(super) fn start(
        session_id: String,
        framebuffer: Arc<ClientFramebufferHub>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("surface_stream_bind_failed:{error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("surface_stream_nonblocking_failed:{error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("surface_stream_address_failed:{error}"))?
            .port();
        let token = uuid::Uuid::new_v4().to_string();
        let control = SurfaceWorkerControl::new(Arc::clone(&framebuffer));
        let thread_control = Arc::clone(&control);
        let thread_token = token.clone();
        let admission = SurfaceAdmission::new(framebuffer.memory_budget());
        let listener = thread::Builder::new()
            .name("vnc-surface-listener".into())
            .spawn(move || {
                let mut workers = SurfaceWorkers::new(Arc::clone(&thread_control));
                while !thread_control.stop_flag().load(Ordering::Acquire) {
                    workers.reap_finished()?;
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let Ok(permit) = admission.acquire() else {
                                continue;
                            };
                            let framebuffer = Arc::clone(&framebuffer);
                            let session_id = session_id.clone();
                            let token = thread_token.clone();
                            workers.spawn(stream, permit, move |stream, control, connection| {
                                serve_surface(
                                    stream,
                                    session_id,
                                    token,
                                    framebuffer,
                                    control,
                                    connection,
                                )
                            })?;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(8))
                        }
                        Err(_) => break,
                    }
                }
                workers.finish()
            })
            .map_err(|_| "rfb_worker_spawn_failed")?;
        Ok(Self {
            port,
            token,
            control,
            listener: Mutex::new(Some(listener)),
        })
    }

    pub(super) fn request_stop(&self) {
        self.control.request_stop();
    }

    pub(super) fn detach_surface(&self, id: &str) {
        self.control.detach_surface(id);
    }

    /// Registry callers wait outside their lock. A timed-out join keeps the
    /// listener handle and the Session owner so teardown can be retried.
    pub(super) fn shutdown(&self) -> Result<(), String> {
        self.request_stop();
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let mut listener = self
                .listener
                .lock()
                .map_err(|_| "rfb_worker_lock_poisoned")?;
            if listener
                .as_ref()
                .is_none_or(thread::JoinHandle::is_finished)
            {
                return match listener.take() {
                    Some(listener) => listener.join().map_err(|_| "rfb_worker_panicked")?,
                    None => Ok(()),
                };
            }
            drop(listener);
            if Instant::now() >= deadline {
                return Err("rfb_shutdown_pending".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn url_for(&self, session_id: &str, surface_id: &str) -> String {
        format!(
            "ws://127.0.0.1:{}/surface/{session_id}/{surface_id}?token={}",
            self.port, self.token
        )
    }
}

impl Drop for SurfaceStream {
    fn drop(&mut self) {
        self.request_stop();
        // Final destruction cannot discard live handles. Normal stop has already
        // joined outside the registry; this also covers direct owner destruction.
        if let Some(listener) = self
            .listener
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            let _ = listener.join();
        }
    }
}

struct SurfaceHandshake {
    session_id: String,
    token: String,
    requested_surface: Arc<Mutex<Option<String>>>,
}

impl tungstenite::handshake::server::Callback for SurfaceHandshake {
    fn on_request(
        self,
        request: &Request,
        response: Response,
    ) -> Result<Response, tungstenite::handshake::server::ErrorResponse> {
        let path = request.uri().path();
        let expected_prefix = format!("/surface/{}/", self.session_id);
        let requested = path
            .strip_prefix(&expected_prefix)
            .filter(|value| !value.is_empty() && value.len() <= 128);
        let valid_token = request.uri().query().and_then(|query| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("token="))
        }) == Some(self.token.as_str());
        if !valid_token || requested.is_none() {
            return Err(tungstenite::http::Response::builder()
                .status(403)
                .body(Some("surface token rejected".into()))
                .expect("static HTTP response"));
        }
        *self.requested_surface.lock().expect("surface request lock") =
            requested.map(str::to_owned);
        Ok(response)
    }
}

fn serve_surface(
    stream: TcpStream,
    session_id: String,
    token: String,
    framebuffer: Arc<ClientFramebufferHub>,
    control: Arc<SurfaceWorkerControl>,
    connection: Arc<SurfaceConnection>,
) {
    let surface_id = Arc::new(Mutex::new(None::<String>));
    let requested_surface = Arc::clone(&surface_id);
    let callback = SurfaceHandshake {
        session_id,
        token,
        requested_surface,
    };
    let Ok(mut websocket) = SurfaceTransport::accept(stream, callback, framebuffer.memory_budget())
    else {
        return;
    };
    let Some(surface_id) = surface_id.lock().ok().and_then(|value| value.clone()) else {
        return;
    };
    if control.claim_surface(&surface_id, &connection).is_err() {
        return;
    }
    let stop = connection.stop_flag();
    while !stop.load(Ordering::Acquire) {
        let core = match framebuffer.lock() {
            Ok(core) => core,
            Err(_) => return,
        };
        let update = match core.next_surface_update(&surface_id) {
            Ok(Some(update)) => update,
            Ok(None) => {
                drop(core);
                if websocket.poll_idle().is_err() {
                    return;
                }
                if framebuffer
                    .wait_for_surface_update(&surface_id, Duration::from_millis(250))
                    .is_err()
                {
                    return;
                }
                continue;
            }
            Err(_) => return,
        };
        let draft = match SurfaceFramePacket::capture_from_core(&core, &update) {
            Ok(packet) => packet,
            Err(_) => return,
        };
        drop(core);
        let packet = draft.finish();
        if websocket.send_packet(packet).is_err() {
            return;
        }
        if wait_for_present_ack(&mut websocket, update.frame_id, stop).is_err() {
            return;
        }
        websocket.presented();
        if control
            .mark_presented(&surface_id, &connection, update.frame_id)
            .is_err()
        {
            return;
        }
    }
}

fn wait_for_present_ack(
    websocket: &mut SurfaceTransport,
    expected_frame_id: FrameId,
    stop: &AtomicBool,
) -> Result<(), ()> {
    let deadline = Instant::now() + ACK_TIMEOUT;
    websocket.begin_ack(deadline);
    while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
        match websocket.read() {
            Ok(Message::Binary(bytes)) if decode_surface_ack(&bytes) == Some(expected_frame_id) => {
                return Ok(())
            }
            Ok(Message::Close(_)) => return Err(()),
            Ok(_) => continue,
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(()),
        }
    }
    Err(())
}

fn decode_surface_ack(bytes: &[u8]) -> Option<FrameId> {
    if bytes.len() != SURFACE_ACK_LEN || bytes[..4] != SURFACE_ACK_MAGIC {
        return None;
    }
    Some(u64::from_be_bytes(bytes[4..12].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::decode_surface_ack;

    #[test]
    fn surface_ack_requires_magic_and_exact_frame_id() {
        let mut bytes = Vec::from(*b"VNCA");
        bytes.extend_from_slice(&42_u64.to_be_bytes());
        assert_eq!(decode_surface_ack(&bytes), Some(42));
        bytes.push(0);
        assert_eq!(decode_surface_ack(&bytes), None);
    }
}
