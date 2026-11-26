use super::MAX_HISTORY_BYTES;
use std::collections::VecDeque;

/// Owns accepted messages independently of the native window's lifetime.
#[derive(Default)]
pub(super) struct MessageInbox {
    pending: VecDeque<String>,
    retained_bytes: usize,
    scheduled: bool,
}

impl MessageInbox {
    pub fn enqueue(
        &mut self,
        entry: &str,
        wake: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let entry = entry.replace("\r\n", "\n").replace('\r', "\n");
        let previous = self.retained_bytes;
        let next = previous + usize::from(previous > 0) * 2 + entry.len();
        if next > MAX_HISTORY_BYTES {
            return Err("remote_message_busy".into());
        }
        self.pending.push_back(entry);
        self.retained_bytes = next;
        if !self.scheduled {
            if let Err(error) = wake() {
                self.pending.pop_back();
                self.retained_bytes = previous;
                return Err(error);
            }
            self.scheduled = true;
        }
        Ok(())
    }

    pub fn take_pending(&mut self) -> Vec<String> {
        self.scheduled = false;
        self.pending.drain(..).collect()
    }

    /// Dismiss only displayed history; already accepted pending messages survive.
    pub fn window_closed(&mut self) {
        self.retained_bytes = self.pending.iter().map(String::len).sum::<usize>()
            + self.pending.len().saturating_sub(1) * 2;
    }

    pub fn restore_after_window_failure(&mut self, entries: Vec<String>) {
        for entry in entries.into_iter().rev() {
            self.pending.push_front(entry);
        }
        self.retained_bytes = self.pending.iter().map(String::len).sum::<usize>()
            + self.pending.len().saturating_sub(1) * 2;
        self.scheduled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledges_receipt_without_waiting_for_render_and_batches_wakeups() {
        let mut inbox = MessageInbox::default();
        let mut wakeups = 0;
        for entry in ["one", "two", "three"] {
            inbox
                .enqueue(entry, || {
                    wakeups += 1;
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(wakeups, 1);
        assert_eq!(inbox.take_pending(), ["one", "two", "three"]);
        inbox
            .enqueue("four", || {
                wakeups += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(wakeups, 2);
    }

    #[test]
    fn closing_keeps_pending_messages_and_releases_only_displayed_capacity() {
        let mut inbox = MessageInbox::default();
        inbox
            .enqueue(&"a".repeat(MAX_HISTORY_BYTES - 10), || Ok(()))
            .unwrap();
        inbox.take_pending();
        inbox.enqueue("pending", || Ok(())).unwrap();
        assert_eq!(
            inbox.enqueue("too much", || Ok(())).unwrap_err(),
            "remote_message_busy"
        );
        inbox.window_closed();
        inbox.enqueue("new", || Ok(())).unwrap();
        assert_eq!(inbox.take_pending(), ["pending", "new"]);
    }

    #[test]
    fn failed_wakeup_does_not_accept_or_reserve_a_message() {
        let mut inbox = MessageInbox::default();
        assert!(inbox
            .enqueue("failed", || Err("worker stopped".into()))
            .is_err());
        assert!(inbox.take_pending().is_empty());
        inbox
            .enqueue(&"a".repeat(MAX_HISTORY_BYTES), || Ok(()))
            .unwrap();
        assert_eq!(
            inbox.enqueue("b", || Ok(())).unwrap_err(),
            "remote_message_busy"
        );
    }

    #[test]
    fn a_window_creation_failure_keeps_the_batch_for_the_next_wakeup() {
        let mut inbox = MessageInbox::default();
        inbox.enqueue("one", || Ok(())).unwrap();
        let batch = inbox.take_pending();
        inbox.enqueue("two", || Ok(())).unwrap();
        inbox.restore_after_window_failure(batch);
        inbox.enqueue("three", || Ok(())).unwrap();
        assert_eq!(inbox.take_pending(), ["one", "two", "three"]);
    }
}
