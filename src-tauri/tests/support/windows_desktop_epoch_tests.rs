use super::DesktopEpoch;

#[test]
fn unchanged_desktop_keeps_the_captured_baseline_valid() {
    let mut state = DesktopEpoch::default();
    let captured = state.observe(Some("Default".into()));
    assert_eq!(state.observe(Some("Default".into())), captured);
    assert!(state.accepts(captured));
}

#[test]
fn secure_transition_rejects_old_pixels_and_input_even_at_the_same_size() {
    let mut state = DesktopEpoch::default();
    let ordinary_frame = state.observe(Some("Default".into()));
    let secure_frame = state.observe(Some("Winlogon".into()));
    assert!(!state.accepts(ordinary_frame));
    assert!(state.accepts(secure_frame));
    state.observe(Some("Default".into()));
    assert!(!state.accepts(ordinary_frame));
    assert!(!state.accepts(secure_frame));
}

#[test]
fn lost_desktop_requires_a_fresh_baseline_when_the_same_name_returns() {
    let mut state = DesktopEpoch::default();
    let old = state.observe(Some("Default".into()));
    state.observe(None);
    assert!(!state.accepts(old));
    let restored = state.observe(Some("Default".into()));
    assert_ne!(old, restored);
    assert!(!state.accepts(old));
    assert!(state.accepts(restored));
}
