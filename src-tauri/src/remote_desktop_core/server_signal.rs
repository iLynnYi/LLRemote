use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// Per-attachment wake state shared by capture and the connection's control inbox.
/// A version observed before checking work also covers events arriving before wait.
#[derive(Default)]
pub(crate) struct ServerUpdateSignal {
    version: Mutex<u64>,
    changed: Condvar,
}

impl ServerUpdateSignal {
    pub(crate) fn version(&self) -> u64 {
        *self
            .version
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn notify(&self) {
        let mut version = self
            .version
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *version = version.wrapping_add(1);
        self.changed.notify_all();
    }

    pub(crate) fn wait_since(&self, observed: u64, timeout: Duration) {
        let version = self
            .version
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _guard = self
            .changed
            .wait_timeout_while(version, timeout, |version| *version == observed)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}
