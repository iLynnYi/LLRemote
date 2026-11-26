//! Distinguish our Server's injected input from other legitimate keyboard sources
//! (remote control, accessibility tools, and virtual keyboards).
pub(crate) const OWN_INPUT_TAG: usize = 0x4c4c_524b;

pub(crate) fn is_own_input(tag: u64) -> bool {
    tag == OWN_INPUT_TAG as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_server_events_bypass_client_capture() {
        assert!(is_own_input(OWN_INPUT_TAG as u64));
        // Neither hardware nor another remote-control application's events may
        // bypass capture simply because the OS marks them as injected.
        for tag in [0, 1, 0x1234, u64::MAX] {
            assert!(!is_own_input(tag));
        }
    }
}
