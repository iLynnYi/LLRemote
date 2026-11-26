fn start_framebuffer_capture_worker(
    shared_display: Arc<RwLock<SharedDisplay>>,
    frame_cache: Arc<LocalVncServerFrameCache>,
    framebuffer_core: Arc<crate::remote_desktop_core::ServerFramebufferCore>,
    stop_requested: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        run_framebuffer_capture_loop(
            shared_display,
            frame_cache,
            framebuffer_core,
            stop_requested,
        );
    })
}

fn run_framebuffer_capture_loop(
    shared_display: Arc<RwLock<SharedDisplay>>,
    frame_cache: Arc<LocalVncServerFrameCache>,
    framebuffer_core: Arc<crate::remote_desktop_core::ServerFramebufferCore>,
    stop_requested: Arc<AtomicBool>,
) {
    if let Some(relay) = shared_display
        .read()
        .ok()
        .and_then(|display| display.relay.clone())
    {
        run_relay_capture_loop(
            shared_display.clone(),
            relay,
            frame_cache,
            framebuffer_core,
            stop_requested,
        );
        return;
    }
    let mut capture = PlatformFrameCapture::default();
    let mut next_display_refresh = Instant::now();
    while !stop_requested.load(Ordering::SeqCst) {
        match capture.prepare_desktop() {
            Ok(Some(epoch)) => {
                let Ok(mut current) = shared_display.write() else {
                    break;
                };
                if current.desktop_epoch != epoch {
                    let mut next = current.clone();
                    next.desktop_epoch = epoch;
                    if current.replace_with(next).is_err() {
                        break;
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                #[cfg(target_os = "windows")]
                if matches!(error, CaptureFailure::DesktopUnavailable) {
                    frame_cache.mark_desktop_unavailable();
                }
                #[cfg(not(target_os = "windows"))]
                let _ = error;
                frame_cache.record_capture_failure(
                    LocalVncServerCaptureStatus::Unavailable,
                    Duration::ZERO,
                );
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        }
        let Ok(display) = shared_display.read().map(|display| display.clone()) else {
            break;
        };
        if Instant::now() >= next_display_refresh {
            // Enumerate native displays without holding the requested-view lock.
            let displays = list_local_vnc_server_displays();
            if stop_requested.load(Ordering::SeqCst) {
                break;
            }
            if refresh_shared_display_geometry(&shared_display, &display, &displays).is_err() {
                break;
            }
            next_display_refresh = Instant::now() + Duration::from_millis(500);
        }
        let Ok(display) = shared_display.read().map(|display| display.clone()) else {
            break;
        };
        let capture_started_at = Instant::now();
        let result = capture.capture(&display);
        if stop_requested.load(Ordering::SeqCst) {
            break;
        }
        if let Some(result) = result.transpose() {
            publish_current_capture_result(
                &shared_display,
                &display,
                result,
                capture_started_at.elapsed(),
                &frame_cache,
                &framebuffer_core,
            );
        }

        let interval = capture_worker_refresh_interval(display.refresh_mode);
        let mut slept = Duration::from_millis(0);
        while slept < interval && !stop_requested.load(Ordering::SeqCst) {
            let remaining = interval - slept;
            let slice = remaining.min(Duration::from_millis(5));
            thread::sleep(slice);
            slept += slice;
        }
    }
}

fn run_relay_capture_loop(
    shared: Arc<RwLock<SharedDisplay>>,
    relay: Arc<desktop_relay::DesktopRelay>,
    cache: Arc<LocalVncServerFrameCache>,
    core: Arc<crate::remote_desktop_core::ServerFramebufferCore>,
    stop: Arc<AtomicBool>,
) {
    let mut delivered = None;
    while !stop.load(Ordering::Acquire) {
        let started = Instant::now();
        let frame = relay.with_frame(|epoch, frame| {
            let mut display = shared.write().unwrap_or_else(|e| e.into_inner());
            if display.desktop_epoch != epoch
                || display.frame.pixel_width != frame.geometry.width
                || display.frame.pixel_height != frame.geometry.height
            {
                let mut next = display.clone();
                next.desktop_epoch = epoch;
                next.frame = DisplayFrame {
                    x: 0,
                    y: 0,
                    width: frame.geometry.width,
                    height: frame.geometry.height,
                    pixel_width: frame.geometry.width,
                    pixel_height: frame.geometry.height,
                };
                next.framebuffer = framebuffer_size_for_resolution(
                    frame.geometry.width,
                    frame.geometry.height,
                    next.resolution_mode,
                );
                let _ = display.replace_with(next);
            }
            let key = (epoch, frame.frame_id, display.revision);
            if delivered == Some(key) {
                return None;
            }
            delivered = Some(key);
            let pixels = scale_captured_framebuffer_to_rfb(
                frame.pixels(),
                frame.geometry.width as usize,
                frame.geometry.height as usize,
                frame.geometry.width as usize * 4,
                display.framebuffer.width as usize,
                display.framebuffer.height as usize,
                CapturedPixelFormat::Bgra8888,
            );
            Some((display.clone(), pixels))
        });
        match frame {
            Some(Some((display, pixels))) => publish_current_capture_result(
                &shared,
                &display,
                Ok(pixels),
                started.elapsed(),
                &cache,
                &core,
            ),
            Some(None) => {}
            None => {
                cache.mark_desktop_unavailable();
                cache.record_capture_failure(
                    LocalVncServerCaptureStatus::Unavailable,
                    Duration::ZERO,
                );
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn refresh_shared_display_geometry(
    shared: &RwLock<SharedDisplay>,
    observed: &SharedDisplay,
    displays: &[LocalVncServerDisplay],
) -> Result<(), &'static str> {
    // A disconnected selection remains selected; do not switch to another screen.
    let Some(selected) = displays.iter().find(|display| display.id == observed.id) else {
        return Ok(());
    };
    let mut next =
        shared_display_from_overview(selected, observed.resolution_mode, observed.refresh_mode);
    next.desktop_epoch = observed.desktop_epoch;
    if next.frame == observed.frame && next.framebuffer == observed.framebuffer {
        return Ok(());
    }
    let mut current = shared
        .write()
        .map_err(|_| "local_vnc_server_state_poisoned")?;
    if current.revision == observed.revision {
        current.replace_with(next)?;
    }
    Ok(())
}

fn capture_worker_refresh_interval(refresh_mode: LocalVncServerRefreshMode) -> Duration {
    refresh_interval(refresh_mode)
}

fn publish_current_capture_result(
    shared_display: &RwLock<SharedDisplay>,
    captured_display: &SharedDisplay,
    result: Result<Vec<u8>, CaptureFailure>,
    duration: Duration,
    frame_cache: &LocalVncServerFrameCache,
    framebuffer_core: &crate::remote_desktop_core::ServerFramebufferCore,
) {
    if !captured_display.desktop_is_current() {
        return;
    }
    // Native capture runs outside this lock; serialize only the commit with view
    // replacement so stale pixels and stale errors cannot overwrite current state.
    let Ok(current) = shared_display.read() else {
        frame_cache.record_capture_failure(LocalVncServerCaptureStatus::Unavailable, duration);
        return;
    };
    if current.revision != captured_display.revision {
        return;
    }
    publish_capture_result(
        captured_display,
        result,
        duration,
        frame_cache,
        framebuffer_core,
    );
}

// Validates and publishes one platform capture; failed captures must never
// become synthetic pixels or advance successful-capture diagnostics.
fn publish_capture_result(
    display: &SharedDisplay,
    result: Result<Vec<u8>, CaptureFailure>,
    duration: Duration,
    frame_cache: &LocalVncServerFrameCache,
    framebuffer_core: &crate::remote_desktop_core::ServerFramebufferCore,
) {
    let framebuffer = match result {
        Ok(framebuffer) => framebuffer,
        Err(error) => {
            let status = match error {
                #[cfg(target_os = "windows")]
                CaptureFailure::DesktopUnavailable => LocalVncServerCaptureStatus::Unavailable,
                #[cfg(any(target_os = "macos", test))]
                CaptureFailure::PermissionDenied => LocalVncServerCaptureStatus::PermissionDenied,
                CaptureFailure::Unavailable => LocalVncServerCaptureStatus::Unavailable,
                CaptureFailure::InvalidFrame => LocalVncServerCaptureStatus::InvalidFrame,
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                CaptureFailure::Unsupported => LocalVncServerCaptureStatus::Unsupported,
            };
            frame_cache.record_capture_failure(status, duration);
            return;
        }
    };
    let geometry = crate::remote_desktop_core::FramebufferGeometry {
        width: display.framebuffer.width,
        height: display.framebuffer.height,
        pixel_format: crate::remote_desktop_core::PixelFormat::Bgra8888,
    };
    if geometry.width == 0 || geometry.height == 0 {
        frame_cache.record_capture_failure(LocalVncServerCaptureStatus::InvalidFrame, duration);
        return;
    }
    let Ok(frame) = crate::remote_desktop_core::Framebuffer::new(geometry, framebuffer) else {
        frame_cache.record_capture_failure(LocalVncServerCaptureStatus::InvalidFrame, duration);
        return;
    };
    if framebuffer_core
        .publish_for_source(frame, display.revision)
        .is_err()
    {
        frame_cache.record_capture_failure(LocalVncServerCaptureStatus::Unavailable, duration);
        return;
    }
    frame_cache.record_capture(duration);
}

#[cfg(test)]
#[path = "../../tests/support/server_capture_tests.rs"]
mod server_capture_tests;
