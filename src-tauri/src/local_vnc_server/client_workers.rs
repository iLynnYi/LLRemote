use std::io;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

const MAX_CLIENT_WORKERS: usize = 8;

struct ClientWorker {
    socket: Arc<TcpStream>,
    worker: JoinHandle<()>,
}

/// Listener-owned Server tasks. A socket remains cancellable until its worker has
/// actually exited; finished tasks are joined before their slots are reused.
pub(crate) struct LocalServerClientWorkers {
    stop: Arc<AtomicBool>,
    thread_name: &'static str,
    workers: Vec<ClientWorker>,
}

impl LocalServerClientWorkers {
    pub(crate) fn new(stop: Arc<AtomicBool>, thread_name: &'static str) -> Self {
        Self {
            stop,
            thread_name,
            workers: Vec::new(),
        }
    }

    pub(crate) fn has_capacity(&self) -> bool {
        self.workers.len() < MAX_CLIENT_WORKERS
    }

    pub(crate) fn spawn<F>(
        &mut self,
        stream: TcpStream,
        socket: Arc<TcpStream>,
        serve: F,
    ) -> io::Result<()>
    where
        F: FnOnce(TcpStream) + Send + 'static,
    {
        if self.stop.load(Ordering::SeqCst) || !self.has_capacity() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "local_server_client_limit",
            ));
        }
        let worker = thread::Builder::new()
            .name(self.thread_name.into())
            .spawn(move || serve(stream))?;
        self.workers.push(ClientWorker { socket, worker });
        Ok(())
    }

    pub(crate) fn reap_finished(&mut self) {
        let mut index = 0;
        while index < self.workers.len() {
            if self.workers[index].worker.is_finished() {
                let task = self.workers.swap_remove(index);
                let _ = task.worker.join();
            } else {
                index += 1;
            }
        }
    }
}

impl Drop for LocalServerClientWorkers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Cancel every socket before waiting for any worker: a slow peer must
        // not postpone cancellation of the other clients.
        for task in &self.workers {
            let _ = task.socket.shutdown(Shutdown::Both);
        }
        for task in self.workers.drain(..) {
            let _ = task.worker.join();
        }
    }
}
