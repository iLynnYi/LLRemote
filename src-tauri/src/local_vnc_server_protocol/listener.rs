/// Immutable server resources cloned for each owned connection worker.
#[derive(Clone)]
pub(crate) struct RfbServerContext {
    pub auth_key: [u8; 8],
    pub view_only: bool,
    pub shared_display: Arc<RwLock<SharedDisplay>>,
    pub frame_cache: Arc<LocalVncServerFrameCache>,
    pub framebuffer_core: Arc<crate::remote_desktop_core::ServerFramebufferCore>,
    pub discovery: crate::server_discovery::ServerDiscoveryContext,
}

pub(crate) fn run_listener(
    listener: TcpListener,
    stop_requested: Arc<AtomicBool>,
    connections: Arc<LocalVncServerConnectionRegistry>,
    context: RfbServerContext,
) {
    let mut clients = LocalServerClientWorkers::new(Arc::clone(&stop_requested), "vnc-server-rfb");
    while !stop_requested.load(Ordering::SeqCst) {
        clients.reap_finished();
        match listener.accept() {
            Ok((stream, peer_addr)) => {
                if stop_requested.load(Ordering::SeqCst) {
                    break;
                }
                if !clients.has_capacity() || stream.set_nonblocking(false).is_err() {
                    continue;
                }
                let Ok(shutdown_socket) = stream.try_clone().map(Arc::new) else {
                    continue;
                };
                let disconnect_stream = Arc::clone(&shutdown_socket);
                let disconnect = Arc::new(move || {
                    let _ = disconnect_stream.shutdown(Shutdown::Both);
                });
                let connection_guard = connections.register(
                    LocalVncServerConnectionKind::Rfb,
                    Some(peer_addr),
                    disconnect,
                );
                let client_id = connection_guard.id().to_string();
                let client_stop = Arc::clone(&stop_requested);
                let client_context = context.clone();
                let _ = clients.spawn(stream, shutdown_socket, move |mut stream| {
                    let _connection_guard = connection_guard;
                    let _ = serve_rfb_client(&mut stream, &client_stop, &client_context, client_id);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/server_worker_lifecycle_tests.rs"]
mod server_worker_lifecycle_tests;
