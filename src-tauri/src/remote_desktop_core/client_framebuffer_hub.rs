use super::{ClientFramebufferCore, ClientMemoryBudget};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// Synchronization seam between the protocol thread and renderer transports.
/// Pixel ownership stays in `ClientFramebufferCore`; the condition variable only
/// wakes Surface publishers after a committed frame instead of polling the Core.
pub struct ClientFramebufferHub {
    framebuffer: Mutex<ClientFramebufferCore>,
    changed: Condvar,
    memory: ClientMemoryBudget,
}

impl Default for ClientFramebufferHub {
    fn default() -> Self {
        Self::with_memory(ClientMemoryBudget::default())
    }
}

impl ClientFramebufferHub {
    pub(super) fn with_memory(memory: ClientMemoryBudget) -> Self {
        Self {
            framebuffer: Mutex::new(ClientFramebufferCore::with_memory(memory.clone())),
            changed: Condvar::new(),
            memory,
        }
    }

    pub(crate) fn memory_budget(&self) -> ClientMemoryBudget {
        self.memory.clone()
    }
    pub fn lock(&self) -> Result<MutexGuard<'_, ClientFramebufferCore>, String> {
        self.framebuffer
            .lock()
            .map_err(|_| "client_framebuffer_hub_lock_poisoned".to_string())
    }

    pub fn notify_frame_committed(&self) {
        self.changed.notify_all();
    }

    pub fn notify_surface_state_changed(&self) {
        self.changed.notify_all();
    }

    pub fn wait_for_surface_update(
        &self,
        surface_id: &str,
        timeout: Duration,
    ) -> Result<bool, String> {
        let framebuffer = self.lock()?;
        if framebuffer.next_surface_update(surface_id)?.is_some() {
            return Ok(true);
        }
        let (framebuffer, _) = self
            .changed
            .wait_timeout_while(framebuffer, timeout, |framebuffer| {
                framebuffer
                    .next_surface_update(surface_id)
                    .map(|update| update.is_none())
                    .unwrap_or(false)
            })
            .map_err(|_| "client_framebuffer_hub_wait_poisoned".to_string())?;
        Ok(framebuffer.next_surface_update(surface_id)?.is_some())
    }
}
