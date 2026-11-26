use std::collections::HashSet;

#[derive(Default)]
pub(super) struct CapturePolicy {
    held: HashSet<u16>,
    captured: HashSet<u16>,
}

impl CapturePolicy {
    pub(super) fn with_modifiers(keys: impl IntoIterator<Item = u16>) -> Self {
        Self {
            held: keys.into_iter().collect(),
            captured: HashSet::new(),
        }
    }

    pub(super) fn modifier_down(&self, key: u16, flags: u64) -> bool {
        let mask = match key {
            54 | 55 => 1 << 20,
            59 | 62 => 1 << 18,
            58 | 61 => 1 << 19,
            56 | 60 => 1 << 17,
            57 => 1 << 16,
            _ => return false,
        };
        flags & mask != 0 && !self.held.contains(&key)
    }

    pub(super) fn capture(&mut self, key: u16, down: bool) -> bool {
        if down {
            self.held.insert(key);
        } else {
            self.held.remove(&key);
        }
        let capture = self.captured.contains(&key)
            || matches!(key, 54 | 55 | 58 | 59 | 61 | 62)
            || [54, 55, 58, 59, 61, 62]
                .iter()
                .any(|key| self.held.contains(key));
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
    fn captures_shortcuts_and_pairs_keyup_after_modifier_release() {
        for modifier in [54, 55, 58, 59, 61, 62] {
            let mut policy = CapturePolicy::default();
            for (key, down) in [(modifier, true), (0, true), (modifier, false), (0, false)] {
                assert!(policy.capture(key, down));
            }
            assert!(!policy.capture(0, true));
        }
    }

    #[test]
    fn leaves_plain_text_shift_and_ime_to_webview() {
        let mut policy = CapturePolicy::default();
        for key in [56, 0, 102, 104] {
            assert!(!policy.capture(key, true));
        }
    }

    #[test]
    fn flags_changed_tracks_both_sides_and_initial_modifiers() {
        let mut policy = CapturePolicy::with_modifiers([55]);
        assert!(!policy.modifier_down(55, 1 << 20));
        assert!(policy.modifier_down(54, 1 << 20));
        policy.capture(54, true);
        policy.capture(55, false);
        assert!(policy.capture(0, true));
        assert!(!policy.modifier_down(54, 0));
        policy.capture(54, false);
        assert!(policy.capture(0, false));
    }
}
