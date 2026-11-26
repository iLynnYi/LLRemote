use crate::remote_desktop_core::{FrameInterest, FramebufferRect as InterestRect};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

const MAX_CLIENT_INBOX_MESSAGES: usize = 64;
const MAX_CLIENT_INBOX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct ClientInboxState {
    queue: VecDeque<ClientMessage>,
    bytes: usize,
    error: Option<std::io::Error>,
}

#[derive(Default)]
struct ClientInbox {
    state: Mutex<ClientInboxState>,
    changed: Condvar,
    updates: Arc<crate::remote_desktop_core::ServerUpdateSignal>,
    closed: AtomicBool,
}

enum ClientInboxItem {
    Message(ClientMessage),
    Idle,
    Closed,
}

impl ClientInbox {
    fn new(updates: Arc<crate::remote_desktop_core::ServerUpdateSignal>) -> Self {
        Self {
            updates,
            ..Self::default()
        }
    }

    fn push(&self, message: ClientMessage, stop: &AtomicBool) -> Result<bool, std::io::Error> {
        if message.retained_bytes() > MAX_CLIENT_INBOX_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "rfb_resource_limit:client_inbox",
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("client inbox poisoned"))?;
        loop {
            if self.closed.load(Ordering::SeqCst) || stop.load(Ordering::SeqCst) {
                return Ok(false);
            }
            if let (
                Some(ClientMessage::FrameRequest(previous)),
                ClientMessage::FrameRequest(next),
            ) = (state.queue.back_mut(), &message)
            {
                *previous = merge_frame_interest(*previous, *next);
                self.changed.notify_all();
                self.updates.notify();
                return Ok(true);
            }
            let bytes = message.retained_bytes();
            if state.queue.len() < MAX_CLIENT_INBOX_MESSAGES
                && bytes <= MAX_CLIENT_INBOX_BYTES.saturating_sub(state.bytes)
            {
                state.bytes += bytes;
                state.queue.push_back(message);
                self.changed.notify_all();
                self.updates.notify();
                return Ok(true);
            }
            state = self
                .changed
                .wait_timeout(state, RFB_IO_POLL)
                .map_err(|_| std::io::Error::other("client inbox poisoned"))?
                .0;
        }
    }

    fn next(&self, timeout: Duration) -> Result<ClientInboxItem, std::io::Error> {
        self.next_after(self.updates.version(), timeout)
    }

    // The writer observes before checking pixels, so a capture between that check
    // and this wait cannot be lost. Inbox locks are never held across signal waits.
    fn next_after(
        &self,
        observed: u64,
        timeout: Duration,
    ) -> Result<ClientInboxItem, std::io::Error> {
        match self.try_next()? {
            ClientInboxItem::Idle => {
                self.updates.wait_since(observed, timeout);
                self.try_next()
            }
            item => Ok(item),
        }
    }

    fn try_next(&self) -> Result<ClientInboxItem, std::io::Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("client inbox poisoned"))?;
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        if self.closed.load(Ordering::SeqCst) {
            return Ok(ClientInboxItem::Closed);
        }
        if let Some(message) = state.queue.pop_front() {
            state.bytes -= message.retained_bytes();
            self.changed.notify_all();
            return Ok(ClientInboxItem::Message(message));
        }
        Ok(ClientInboxItem::Idle)
    }

    fn close(&self, error: Option<std::io::Error>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.closed.swap(true, Ordering::SeqCst) {
            state.error = error;
            state.queue.clear();
            state.bytes = 0;
        }
        self.changed.notify_all();
        self.updates.notify();
    }
}

// Coalescing preserves a full request and covers every requested region.
fn merge_frame_interest(left: FrameInterest, right: FrameInterest) -> FrameInterest {
    let region = match (left.region, right.region) {
        (Some(left), Some(right)) => {
            let x = left.x.min(right.x);
            let y = left.y.min(right.y);
            Some(InterestRect {
                x,
                y,
                width: left
                    .x
                    .saturating_add(left.width)
                    .max(right.x.saturating_add(right.width))
                    - x,
                height: left
                    .y
                    .saturating_add(left.height)
                    .max(right.y.saturating_add(right.height))
                    - y,
            })
        }
        _ => None,
    };
    FrameInterest {
        incremental: left.incremental && right.incremental,
        region,
    }
}
