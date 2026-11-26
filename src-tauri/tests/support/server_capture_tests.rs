use super::*;
use crate::remote_desktop_core::{FrameInterest, ServerFramebufferCore};

#[test]
fn desktop_restriction_is_not_cleared_by_generic_capture_errors_only_a_valid_capture() {
    use crate::remote_desktop_core::DesktopAccessState;
    let cache = LocalVncServerFrameCache::default();
    cache.record_capture(Duration::ZERO);
    cache.mark_desktop_unavailable();
    assert_eq!(cache.desktop_state(), DesktopAccessState::Unavailable);
    cache.record_capture_failure(LocalVncServerCaptureStatus::Unavailable, Duration::ZERO);
    assert_eq!(cache.desktop_state(), DesktopAccessState::Unavailable);
    cache.record_capture(Duration::ZERO);
    assert_eq!(cache.desktop_state(), DesktopAccessState::Available);
}

#[test]
fn capture_from_replaced_display_is_discarded_even_when_dimensions_match() {
    let desktop_source = desktop_relay::test_support::SourceFixture::new(7);
    let old_display = SharedDisplay {
        relay: Some(desktop_source.relay.clone()),
        framebuffer: DisplaySize {
            width: 2,
            height: 1,
        },
        ..SharedDisplay::fallback_main()
    };
    let mut new_display = old_display.clone();
    new_display.id = "second-display".into();
    new_display.frame.x = 1920;
    let shared = RwLock::new(old_display.clone());
    shared.write().unwrap().replace_with(new_display).unwrap();
    let new_display = shared.read().unwrap().clone();
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let cache = LocalVncServerFrameCache::default();
    publish_current_capture_result(
        &shared,
        &old_display,
        Ok(vec![7; 8]),
        Duration::ZERO,
        &cache,
        &core,
    );
    assert!(core
        .next_update("client", FrameInterest::full())
        .unwrap()
        .is_none());
    assert_eq!(cache.diagnostics().capture_frames, 0);
    assert_eq!(cache.diagnostics().capture_failures, 0);
    publish_current_capture_result(
        &shared,
        &new_display,
        Ok(vec![9; 8]),
        Duration::ZERO,
        &cache,
        &core,
    );
    assert_eq!(
        core.next_update("client", FrameInterest::full())
            .unwrap()
            .unwrap()
            .framebuffer
            .pixels(),
        &[9; 8]
    );
    assert_eq!(cache.diagnostics().capture_frames, 1);
}

#[test]
fn a_detached_desktop_cannot_publish_a_late_capture() {
    let desktop_source = desktop_relay::test_support::SourceFixture::new(7);
    let display = SharedDisplay {
        relay: Some(desktop_source.relay.clone()),
        framebuffer: DisplaySize {
            width: 2,
            height: 1,
        },
        ..SharedDisplay::fallback_main()
    };
    let shared = RwLock::new(display.clone());
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let cache = LocalVncServerFrameCache::default();
    publish_current_capture_result(
        &shared,
        &display,
        Ok(vec![7; 8]),
        Duration::ZERO,
        &cache,
        &core,
    );
    desktop_source.relay.detach();
    publish_current_capture_result(
        &shared,
        &display,
        Ok(vec![9; 8]),
        Duration::ZERO,
        &cache,
        &core,
    );
    assert_eq!(
        core.next_update("client", FrameInterest::full())
            .unwrap()
            .unwrap()
            .framebuffer
            .pixels(),
        &[7; 8]
    );
    assert_eq!(cache.diagnostics().capture_frames, 1);
}

#[test]
fn identical_pixels_on_a_new_display_still_deliver_a_full_frame() {
    use crate::remote_desktop_core::{FramebufferGeometry, FramebufferRect, PixelFormat};
    let display = SharedDisplay {
        framebuffer: DisplaySize {
            width: 2,
            height: 1,
        },
        ..SharedDisplay::fallback_main()
    };
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let cache = LocalVncServerFrameCache::default();
    let geometry = FramebufferGeometry {
        width: 2,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    publish_capture_result(&display, Ok(vec![9; 8]), Duration::ZERO, &cache, &core);
    let first = core
        .next_update_for_source("client", FrameInterest::incremental(), geometry, 0)
        .unwrap()
        .unwrap();
    core.commit_delivery("client", &first).unwrap();
    let mut next = display.clone();
    next.id = "second-display".into();
    let mut current = display;
    current.replace_with(next).unwrap();
    publish_capture_result(&current, Ok(vec![9; 8]), Duration::ZERO, &cache, &core);
    let frame = core
        .next_update_for_source("client", FrameInterest::incremental(), geometry, 1)
        .unwrap()
        .expect("source switch must deliver even identical pixels");
    assert_eq!(
        frame.dirty_rectangles,
        vec![FramebufferRect::full(geometry)]
    );
}

#[test]
fn failed_capture_does_not_publish_a_synthetic_frame_or_count_as_success() {
    let display = SharedDisplay {
        framebuffer: DisplaySize {
            width: 2,
            height: 1,
        },
        ..SharedDisplay::fallback_main()
    };
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let cache = LocalVncServerFrameCache::default();
    publish_capture_result(
        &display,
        Err(CaptureFailure::Unavailable),
        Duration::from_millis(7),
        &cache,
        &core,
    );
    assert!(core
        .next_update("client", FrameInterest::full())
        .unwrap()
        .is_none());
    let diagnostics = cache.diagnostics();
    assert_eq!(diagnostics.capture_frames, 0);
    assert_eq!(diagnostics.capture_failures, 1);
    assert_eq!(
        diagnostics.capture_status,
        LocalVncServerCaptureStatus::Unavailable
    );
    assert_eq!(diagnostics.last_frame_age_ms, None);
}

#[test]
fn capture_failure_preserves_valid_pixels_and_recovery_resumes_publication() {
    let display = SharedDisplay {
        framebuffer: DisplaySize {
            width: 2,
            height: 1,
        },
        ..SharedDisplay::fallback_main()
    };
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let cache = LocalVncServerFrameCache::default();
    let pixels = vec![10, 20, 30, 0, 40, 50, 60, 0];
    publish_capture_result(&display, Ok(pixels.clone()), Duration::ZERO, &cache, &core);
    let first = core
        .next_update("client", FrameInterest::full())
        .unwrap()
        .unwrap();
    for result in [Err(CaptureFailure::PermissionDenied), Ok(vec![0; 4])] {
        publish_capture_result(&display, result, Duration::from_millis(9), &cache, &core);
        let retained = core
            .next_update("client", FrameInterest::full())
            .unwrap()
            .unwrap();
        assert_eq!(retained.frame_id, first.frame_id);
        assert_eq!(retained.framebuffer.pixels(), pixels);
        assert_eq!(cache.diagnostics().capture_frames, 1);
    }
    let diagnostics = cache.diagnostics();
    assert_eq!(
        diagnostics.capture_status,
        LocalVncServerCaptureStatus::InvalidFrame
    );
    assert_eq!(diagnostics.capture_failures, 2);
    assert_eq!(diagnostics.last_capture_duration_ms, 9);
    assert!(diagnostics.last_frame_age_ms.is_some());
    let recovered_pixels = vec![100; 8];
    publish_capture_result(
        &display,
        Ok(recovered_pixels.clone()),
        Duration::ZERO,
        &cache,
        &core,
    );
    let recovered = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    assert!(recovered.frame_id > first.frame_id);
    assert_eq!(recovered.framebuffer.pixels(), recovered_pixels);
    let diagnostics = cache.diagnostics();
    assert_eq!(
        diagnostics.capture_status,
        LocalVncServerCaptureStatus::Ready
    );
    assert_eq!(diagnostics.capture_frames, 2);
    assert_eq!(diagnostics.capture_failures, 2);
}

#[test]
fn changed_display_geometry_updates_view_without_overwriting_a_newer_selection() {
    let mut overview = LocalVncServerDisplay {
        id: "display:7".into(),
        label: "selected".into(),
        kind: LocalVncServerDisplayKind::Main,
        is_primary: true,
        x: 0,
        y: 0,
        width: 640,
        height: 480,
        pixel_width: 640,
        pixel_height: 480,
        scale_factor: 1.0,
    };
    let initial = shared_display_from_overview(
        &overview,
        LocalVncServerResolutionMode::Native,
        DEFAULT_LOCAL_VNC_SERVER_REFRESH_MODE,
    );
    let shared = RwLock::new(initial.clone());
    overview.x = -800;
    overview.width = 800;
    overview.height = 600;
    overview.pixel_width = 1600;
    overview.pixel_height = 1200;
    overview.scale_factor = 2.0;
    refresh_shared_display_geometry(&shared, &initial, &[overview.clone()]).unwrap();
    let updated = shared.read().unwrap().clone();
    assert_eq!(updated.revision, 1);
    assert_eq!(updated.frame.x, -800);
    assert_eq!(updated.frame.width, 800);
    assert_eq!(
        updated.framebuffer,
        DisplaySize {
            width: 1600,
            height: 1200
        }
    );
    refresh_shared_display_geometry(&shared, &updated, &[overview.clone()]).unwrap();
    assert_eq!(shared.read().unwrap().revision, 1);
    let mut manual = updated.clone();
    manual.id = "display:8".into();
    manual.frame.x = 2000;
    shared.write().unwrap().replace_with(manual).unwrap();
    refresh_shared_display_geometry(&shared, &updated, &[overview]).unwrap();
    let retained = shared.read().unwrap();
    assert_eq!(retained.id, "display:8");
    assert_eq!(retained.frame.x, 2000);
    assert_eq!(retained.revision, 2);
}
