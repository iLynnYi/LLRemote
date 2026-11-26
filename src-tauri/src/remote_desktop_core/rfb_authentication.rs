use crate::local_vnc_server::auth::{create_vnc_auth_key, create_vnc_auth_response_from_key};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// Shared cancellation handle for bounded VNC auth material. A connection may
/// still be waiting in OS DNS; its owner can erase the key without waiting for
/// that worker to resume. Handles do not expose plaintext or key bytes.
#[derive(Clone, Default)]
pub(crate) struct RfbAuthentication(Arc<Mutex<Option<Zeroizing<[u8; 8]>>>>);

impl RfbAuthentication {
    pub(super) fn from_password(password: Option<String>) -> Self {
        let key = password.map(|password| {
            let password = Zeroizing::new(password);
            Zeroizing::new(create_vnc_auth_key(&password))
        });
        Self(Arc::new(Mutex::new(key)))
    }

    pub(crate) fn has_key(&self) -> Result<bool, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "rfb_auth_state_poisoned")?
            .is_some())
    }

    pub(super) fn respond(&self, challenge: &[u8; 16]) -> Result<[u8; 16], String> {
        let key = self
            .0
            .lock()
            .map_err(|_| "rfb_auth_state_poisoned")?
            .take()
            .ok_or("rfb_credentials_required")?;
        // The key has a single consumer; release the mutex before DES work so
        // cancellation never waits on crypto or network I/O.
        Ok(create_vnc_auth_response_from_key(&key, challenge))
    }

    pub(crate) fn clear(&self) {
        // Poison must not prevent secret cleanup after an unexpected panic.
        let mut key = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *key = None;
    }
}

#[cfg(test)]
#[path = "../../tests/support/rfb_authentication_tests.rs"]
mod tests;
