use super::windows_desktop::{self, DesktopOwner, InputDesktop};
use crate::local_vnc_server::SharedDisplay;
use std::collections::BTreeSet;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTUP,
};

/// Each authenticated RFB reader owns its injected keys and desktop binding.
/// Cleanup releases only this reader's state; keys are never replayed on a new desktop.
#[derive(Default)]
pub(crate) struct ServiceInputOwner {
    desktop: DesktopOwner,
    keys: BTreeSet<u32>,
    buttons: u8,
    pending_release: bool,
}

impl ServiceInputOwner {
    pub(crate) fn prepare(&mut self, display: Option<&SharedDisplay>) -> bool {
        let Ok(next) = InputDesktop::open() else {
            self.release();
            return false;
        };
        let epoch = next.epoch;
        if !self.desktop.matches(&next) {
            self.release();
        }
        if self.desktop.bind(next).is_err() {
            return false;
        }
        if self.pending_release {
            self.release();
            if self.pending_release {
                return false;
            }
        }
        display.is_some_and(|display| display.desktop_epoch == epoch)
            && windows_desktop::frame_is_current(epoch)
    }

    pub(crate) fn key(&mut self, down: bool, keysym: u32) {
        if super::windows_key_input(down, keysym).is_none()
            || (down && !self.keys.contains(&keysym) && self.keys.len() >= 256)
        {
            return;
        }
        if super::inject_windows_key_event(down, keysym) {
            if down {
                self.keys.insert(keysym);
            } else {
                self.keys.remove(&keysym);
            }
        }
    }

    pub(crate) fn pointer(&mut self, next: u8, x: u16, y: u16, display: &SharedDisplay) {
        if super::inject_windows_pointer_event(next, x, y, &mut self.buttons, display) {
            self.buttons = next;
        } else {
            // SendInput can insert a prefix of the batch. Release every button
            // that could have gone down, rather than forgetting partial input.
            self.buttons |= next & 7;
            self.release();
        }
    }

    fn release(&mut self) {
        // Retain rejected releases across desktop rebinding. Only up events are
        // retried; a prior desktop's down events never reach a new desktop.
        self.keys
            .retain(|key| !super::inject_windows_key_event(false, *key));
        let mut inputs = Vec::new();
        let mut masks = Vec::new();
        self.buttons &= 7;
        for (mask, flag) in [
            (1, MOUSEEVENTF_LEFTUP),
            (2, MOUSEEVENTF_MIDDLEUP),
            (4, MOUSEEVENTF_RIGHTUP),
        ] {
            if self.buttons & mask != 0 {
                masks.push(mask);
                inputs.push(super::windows_mouse_input(0, 0, 0, flag));
            }
        }
        if !inputs.is_empty() {
            let inserted = unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_ptr(),
                    std::mem::size_of::<INPUT>() as i32,
                )
            };
            for mask in masks.into_iter().take(inserted as usize) {
                self.buttons &= !mask;
            }
        }
        self.pending_release = !self.keys.is_empty() || self.buttons != 0;
    }
}

impl Drop for ServiceInputOwner {
    fn drop(&mut self) {
        self.release();
        if self.pending_release {
            if let Ok(next) = InputDesktop::open() {
                if self.desktop.bind(next).is_ok() {
                    self.release();
                }
            }
        }
    }
}
