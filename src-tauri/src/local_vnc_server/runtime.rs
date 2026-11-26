#[derive(Default)]
pub struct LocalVncServerManager {
    runtime: Mutex<LocalVncServerState>,
}

#[derive(Default)]
enum LocalVncServerState {
    #[default]
    Stopped,
    Running(Box<LocalVncServerRuntime>),
    Stopping(Box<LocalVncServerOverview>),
}

impl LocalVncServerState {
    fn running(&self) -> Option<&LocalVncServerRuntime> {
        match self {
            Self::Running(runtime) => Some(runtime),
            Self::Stopped | Self::Stopping(_) => None,
        }
    }
}

struct LocalVncServerRuntime {
    bind_address: String,
    port: u16,
    view_only: bool,
    started_at: String,
    stop_requested: Arc<AtomicBool>,
    connections: Arc<LocalVncServerConnectionRegistry>,
    shared_display: Arc<RwLock<SharedDisplay>>,
    frame_cache: Arc<LocalVncServerFrameCache>,
    agent: Option<LocalAgentRuntime>,
    agent_overview: LocalAgentOverview,
    worker: Option<JoinHandle<()>>,
    capture_worker: Option<JoinHandle<()>>,
    #[cfg(target_os = "windows")]
    discovery_revision: Arc<std::sync::atomic::AtomicU64>,
}

impl LocalVncServerRuntime {
    fn shutdown_workers(&mut self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        if let Some(agent) = self.agent.as_mut() {
            agent.stop();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.capture_worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for LocalVncServerRuntime {
    fn drop(&mut self) {
        self.shutdown_workers();
    }
}

/// Keeps the stopping gate closed even when a notification callback unwinds.
struct LocalVncServerStoppingRuntime<'a> {
    runtime: Box<LocalVncServerRuntime>,
    state: &'a Mutex<LocalVncServerState>,
}

impl Drop for LocalVncServerStoppingRuntime<'_> {
    fn drop(&mut self) {
        self.runtime.shutdown_workers();
        if let Ok(mut state) = self.state.lock() {
            *state = LocalVncServerState::Stopped;
        }
    }
}
