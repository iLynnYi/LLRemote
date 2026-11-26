use super::*;

#[test]
fn history_charges_retained_capacity_and_recovers_only_after_a_complete_range() {
    let geometry = FramebufferGeometry {
        width: 1,
        height: 1,
        pixel_format: crate::remote_desktop_core::PixelFormat::Bgra8888,
    };
    let mut history = ServerDirtyHistory::default();
    history.record(1, Some(0), geometry, vec![FramebufferRect::full(geometry)]);
    let oversized =
        Vec::with_capacity(MAX_HISTORY_BYTES / std::mem::size_of::<FramebufferRect>() + 1);
    history.record(2, Some(0), geometry, oversized);
    assert_eq!(history.bytes, 0);
    assert!(history.frames.is_empty());
    history.record(3, Some(0), geometry, vec![FramebufferRect::full(geometry)]);
    assert!(history.changes_since(1, 3, Some(0), geometry).is_none());
    assert_eq!(
        history.changes_since(2, 3, Some(0), geometry).unwrap(),
        vec![FramebufferRect::full(geometry)]
    );
    for id in 4..80 {
        history.record(id, Some(0), geometry, Vec::new());
    }
    assert!(history.frames.len() <= MAX_HISTORY_FRAMES);
    assert!(history.bytes <= MAX_HISTORY_BYTES);
}
