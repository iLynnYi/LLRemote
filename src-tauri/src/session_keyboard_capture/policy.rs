use std::collections::HashSet;

#[derive(Default)]
pub(super) struct CapturePolicy {
    held: HashSet<u32>,
    captured: HashSet<u32>,
}

impl CapturePolicy {
    pub(super) fn with_modifiers(keys: impl IntoIterator<Item = u32>) -> Self {
        Self {
            held: keys.into_iter().collect(),
            captured: HashSet::new(),
        }
    }

    pub(super) fn modifiers(&self) -> (bool, bool, bool, bool) {
        let any = |keys: &[u32]| keys.iter().any(|key| self.held.contains(key));
        (
            any(&[0xa2, 0xa3]),
            any(&[0xa4, 0xa5]),
            any(&[0xa0, 0xa1]),
            any(&[0x5b, 0x5c]),
        )
    }

    /// Ordinary text and IME/AltGr input stay on the WebView path. Suppressed
    /// modifiers must use our physical state: Windows does not update its own
    /// keyboard state for a key swallowed by a low-level hook.
    pub(super) fn capture(&mut self, key: u32, down: bool) -> bool {
        if down {
            self.held.insert(key);
        } else {
            self.held.remove(&key);
        }
        let (control, alt, _, windows) = self.modifiers();
        let capture = self.captured.contains(&key)
            || matches!(key, 0x5b | 0x5c)
            || windows
            || (alt && matches!(key, 0x09 | 0x1b | 0x73))
            || ((control || alt)
                && !matches!(key, 0xa0..=0xa5)
                && !self.held.contains(&0xa5)
                && !(control && alt && key == 0x2e));
        if down && capture {
            self.captured.insert(key);
        } else if !down {
            self.captured.remove(&key);
        }
        capture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_and_registered_shortcuts_use_the_native_path() {
        let mut policy = CapturePolicy::with_modifiers([0xa2, 0xa4]);
        for key in [0x4d, 0x57, 0x55, 0x0d] {
            assert!(policy.capture(key, true));
            assert!(policy.capture(key, false));
        }
    }

    #[test]
    fn windows_chord_captures_both_edges_even_when_modifier_releases_first() {
        let mut policy = CapturePolicy::default();
        for (key, down) in [(0x5b, true), (0x44, true), (0x5b, false), (0x44, false)] {
            assert!(policy.capture(key, down));
        }
        assert!(!policy.capture(0x41, true));
    }

    #[test]
    fn leaves_typing_altgr_and_shift_input_on_the_text_path() {
        let mut policy = CapturePolicy::default();
        for key in [0xa2, 0xa5, 0xa0, 0x41, 0xe5] {
            assert!(!policy.capture(key, true));
        }
        for key in [0xe5, 0x41, 0xa0, 0xa5, 0xa2] {
            assert!(!policy.capture(key, false));
        }
    }

    #[test]
    fn captures_system_switching_keys_and_their_release() {
        for (key, modifier) in [(0x09, 0xa4), (0x73, 0xa4), (0x1b, 0xa2)] {
            let mut policy = CapturePolicy::default();
            assert!(!policy.capture(modifier, true));
            assert!(policy.capture(key, true));
            assert!(!policy.capture(modifier, false));
            assert!(policy.capture(key, false));
        }
        let mut policy = CapturePolicy::with_modifiers([0xa2, 0xa4]);
        assert!(!policy.capture(0x2e, true)); // Secure attention stays with Windows.
    }

    #[test]
    fn suppressed_modifier_state_and_repeats_do_not_depend_on_windows_state() {
        let mut policy = CapturePolicy::default();
        for (key, down) in [(0x5b, true), (0xa0, true), (0x43, true), (0x43, true)] {
            assert!(policy.capture(key, down));
        }
        assert_eq!(policy.modifiers(), (false, false, true, true));
        assert!(policy.capture(0x5b, false));
        assert!(policy.capture(0xa0, false));
        assert!(policy.capture(0x43, false));
        assert_eq!(policy.modifiers(), (false, false, false, false));
    }
}
