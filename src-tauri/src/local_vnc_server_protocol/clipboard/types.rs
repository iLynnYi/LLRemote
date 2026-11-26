pub(super) const EXTENDED_CLIPBOARD_PSEUDO_ENCODING: u32 = 0xc0a1e5ce;
pub(super) const EXTENDED_CLIPBOARD_FORMAT_TEXT: u32 = 1;
pub(super) const EXTENDED_CLIPBOARD_ACTION_CAPS: u32 = 1 << 24;
pub(super) const EXTENDED_CLIPBOARD_ACTION_REQUEST: u32 = 1 << 25;
pub(super) const EXTENDED_CLIPBOARD_ACTION_PEEK: u32 = 1 << 26;
pub(super) const EXTENDED_CLIPBOARD_ACTION_NOTIFY: u32 = 1 << 27;
pub(super) const EXTENDED_CLIPBOARD_ACTION_PROVIDE: u32 = 1 << 28;
const EXTENDED_CLIPBOARD_ACTION_MASK: u32 = 0xff00_0000;
const EXTENDED_CLIPBOARD_FORMAT_MASK: u32 = 0x0000_ffff;
const EXTENDED_CLIPBOARD_MAX_TEXT_BYTES: u32 = 1024 * 1024;
pub(super) const CLIPBOARD_PUBLISH_POLL_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Default)]
pub(super) struct ClipboardPublishState {
    last_text: Option<String>,
    last_poll_at: Option<Instant>,
}

#[derive(Default)]
pub(super) struct ClientCutTextOutcome {
    pub(super) received_extended_clipboard_caps: bool,
}

impl ClipboardPublishState {
    pub(super) fn remember_text(&mut self, text: &str) {
        self.last_text = Some(text.to_string());
    }

    pub(super) fn next_changed_text(&mut self, text: Option<String>) -> Option<String> {
        let text = text?;
        if text.is_empty() {
            return None;
        }

        if self.last_text.as_deref() == Some(text.as_str()) {
            return None;
        }

        self.last_text = Some(text.clone());
        Some(text)
    }

    pub(super) fn should_poll(&mut self, now: Instant) -> bool {
        if self.last_poll_at.is_some_and(|last_poll_at| {
            now.duration_since(last_poll_at) < CLIPBOARD_PUBLISH_POLL_INTERVAL
        }) {
            return false;
        }

        self.last_poll_at = Some(now);
        true
    }
}
