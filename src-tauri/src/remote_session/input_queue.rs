use crate::remote_desktop_core::{RfbClientCommand, RfbCommandSource};
use std::collections::VecDeque;
use std::sync::Mutex;

const MAX_COMMANDS: usize = 256;
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

#[derive(Default)]
struct InputState {
    commands: VecDeque<RfbClientCommand>,
    text_bytes: usize,
    closed: bool,
}

/// Bounded native input ownership. Count limits preserve key/pointer ordering;
/// byte limits include String capacity, and cancellation releases pending input
/// without waiting for a worker blocked in DNS or socket I/O.
#[derive(Default)]
pub(super) struct RfbInputQueue {
    state: Mutex<InputState>,
}

impl RfbInputQueue {
    pub(super) fn push(&self, command: RfbClientCommand) -> Result<(), String> {
        if matches!(&command, RfbClientCommand::Clipboard { text } if text.len() > MAX_CLIPBOARD_BYTES)
        {
            return Err("rfb_resource_limit:clipboard".into());
        }
        let mut state = self.state.lock().map_err(|_| "rfb_internal_error")?;
        if state.closed {
            return Err("rfb_transport_closed".into());
        }
        if state.commands.len() == MAX_COMMANDS {
            return Err("rfb_command_queue_full".into());
        }
        let bytes = text_capacity(&command);
        let total = state
            .text_bytes
            .checked_add(bytes)
            .filter(|total| *total <= MAX_TEXT_BYTES)
            .ok_or("rfb_resource_limit:command_queue")?;
        state.commands.push_back(command);
        state.text_bytes = total;
        Ok(())
    }

    pub(super) fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        state.commands = VecDeque::new();
        state.text_bytes = 0;
    }
}

impl RfbCommandSource for RfbInputQueue {
    fn try_recv_command(&self) -> Result<Option<RfbClientCommand>, String> {
        let mut state = self.state.lock().map_err(|_| "rfb_internal_error")?;
        let command = state.commands.pop_front();
        if let Some(command) = &command {
            state.text_bytes -= text_capacity(command);
        }
        Ok(command)
    }
}

fn text_capacity(command: &RfbClientCommand) -> usize {
    match command {
        RfbClientCommand::Clipboard { text } => text.capacity(),
        _ => 0,
    }
}
