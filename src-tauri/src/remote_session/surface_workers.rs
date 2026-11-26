use super::surface_transport::SurfaceConnectionPermit;
use crate::remote_desktop_core::{ClientFramebufferHub, FrameId};
use std::collections::HashMap;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};

pub(super) struct SurfaceConnection {
    socket: TcpStream,
    stop: AtomicBool,
}

impl SurfaceConnection {
    pub(super) fn stop_flag(&self) -> &AtomicBool {
        &self.stop
    }
    fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

#[derive(Default)]
struct SurfaceConnections {
    sockets: Vec<Weak<SurfaceConnection>>,
    owners: HashMap<String, Weak<SurfaceConnection>>,
}

/// Registered before spawning, so cancellation also interrupts a worker that
/// has not entered HTTP/WebSocket processing yet. Weak entries retain no socket.
pub(super) struct SurfaceWorkerControl {
    stop: AtomicBool,
    connections: Mutex<SurfaceConnections>,
    framebuffer: Arc<ClientFramebufferHub>,
}

impl SurfaceWorkerControl {
    pub(super) fn new(framebuffer: Arc<ClientFramebufferHub>) -> Arc<Self> {
        Arc::new(Self {
            stop: AtomicBool::new(false),
            connections: Mutex::new(SurfaceConnections::default()),
            framebuffer,
        })
    }

    pub(super) fn stop_flag(&self) -> &AtomicBool {
        &self.stop
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        let connections = self
            .connections
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for connection in connections.sockets.iter().filter_map(Weak::upgrade) {
            connection.request_stop();
        }
        self.framebuffer.notify_surface_state_changed();
    }

    fn register(&self, stream: &TcpStream) -> Result<Arc<SurfaceConnection>, String> {
        let connection = Arc::new(SurfaceConnection {
            socket: stream.try_clone().map_err(|_| "rfb_socket_clone_failed")?,
            stop: AtomicBool::new(false),
        });
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| "rfb_socket_lock_poisoned")?;
        if self.stop.load(Ordering::Acquire) {
            return Err("rfb_cancelled".into());
        }
        connections
            .sockets
            .retain(|socket| socket.strong_count() != 0);
        connections.sockets.push(Arc::downgrade(&connection));
        Ok(connection)
    }

    pub(super) fn claim_surface(
        &self,
        id: &str,
        connection: &Arc<SurfaceConnection>,
    ) -> Result<(), String> {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| "rfb_socket_lock_poisoned")?;
        if self.stop.load(Ordering::Acquire) || connection.stop.load(Ordering::Acquire) {
            return Err("rfb_cancelled".into());
        }
        // Owner changes and ACK commits use the same lock order: registry -> Core.
        self.framebuffer.lock()?.reset_surface_delivery(id)?;
        connections
            .owners
            .retain(|_, owner| owner.strong_count() != 0);
        if let Some(previous) = connections
            .owners
            .insert(id.into(), Arc::downgrade(connection))
            .and_then(|owner| owner.upgrade())
        {
            previous.request_stop();
        }
        self.framebuffer.notify_surface_state_changed();
        Ok(())
    }

    pub(super) fn mark_presented(
        &self,
        id: &str,
        connection: &Arc<SurfaceConnection>,
        frame_id: FrameId,
    ) -> Result<(), String> {
        let connections = self
            .connections
            .lock()
            .map_err(|_| "rfb_socket_lock_poisoned")?;
        let current = connections.owners.get(id).and_then(Weak::upgrade);
        if connection.stop.load(Ordering::Acquire)
            || current.is_none_or(|owner| !Arc::ptr_eq(&owner, connection))
        {
            return Err("rfb_cancelled".into());
        }
        self.framebuffer
            .lock()?
            .mark_surface_presented(id, frame_id)
    }

    pub(super) fn detach_surface(&self, id: &str) {
        let mut connections = self
            .connections
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(owner) = connections
            .owners
            .remove(id)
            .and_then(|owner| owner.upgrade())
        {
            owner.request_stop();
        }
    }
}

/// The listener owns every connection JoinHandle; completed workers are reaped
/// during service, and cancellation/unwind joins the remaining workers.
pub(super) struct SurfaceWorkers {
    control: Arc<SurfaceWorkerControl>,
    workers: Vec<JoinHandle<()>>,
}

impl SurfaceWorkers {
    pub(super) fn new(control: Arc<SurfaceWorkerControl>) -> Self {
        Self {
            control,
            workers: Vec::new(),
        }
    }

    pub(super) fn spawn<F>(
        &mut self,
        stream: TcpStream,
        permit: SurfaceConnectionPermit,
        serve: F,
    ) -> Result<(), String>
    where
        F: FnOnce(TcpStream, Arc<SurfaceWorkerControl>, Arc<SurfaceConnection>) + Send + 'static,
    {
        let socket = self.control.register(&stream)?;
        let control = Arc::clone(&self.control);
        let worker = thread::Builder::new()
            .name("vnc-surface".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let _permit = permit;
                serve(stream, control, socket);
            })
            .map_err(|_| "rfb_worker_spawn_failed")?;
        self.workers.push(worker);
        Ok(())
    }

    pub(super) fn reap_finished(&mut self) -> Result<(), String> {
        let mut index = 0;
        while index < self.workers.len() {
            if self.workers[index].is_finished() {
                self.workers
                    .swap_remove(index)
                    .join()
                    .map_err(|_| "rfb_worker_panicked")?;
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<(), String> {
        self.control.request_stop();
        let mut result = Ok(());
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                result = Err("rfb_worker_panicked".into());
            }
        }
        result
    }
}

impl Drop for SurfaceWorkers {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
