fn run_server_writer(
    stream: &mut TcpStream,
    stop_requested: &AtomicBool,
    context: &RfbServerContext,
    client_id: String,
    initial_display: SharedDisplay,
    inbox: &ClientInbox,
    last_sent_display: &RwLock<Option<SharedDisplay>>,
) -> Result<(), std::io::Error> {
    let view_only = context.view_only;
    let shared_display = Arc::clone(&context.shared_display);
    let frame_cache = Arc::clone(&context.frame_cache);
    let framebuffer_core = Arc::clone(&context.framebuffer_core);
    let discovery_context = context.discovery.clone();
    let relay = initial_display.relay.clone();
    let mut supports_desktop_size = false;
    let mut supports_extended_clipboard = false;
    let mut discovery_hello_sent = false;
    let mut supports_fence = false;
    let mut supports_runtime_status = false;
    let mut discovery_revision = discovery_context.revision();
    let mut client_extended_clipboard_ready = false;
    let mut framebuffer_encoding = RfbFramebufferEncoding::Raw;
    let mut zlib_encoder = RfbZlibEncoder::new();
    let mut pixel_format = default_rfb_pixel_format();
    let mut last_framebuffer = initial_display.framebuffer;
    let mut clipboard_publish_state = ClipboardPublishState::default();
    let mut framebuffer_update_sent = false;
    let mut continuous_updates_announced = false;
    let mut continuous_updates_enabled = false;
    let mut continuous_updates_region = None;
    let mut pending_interest = None;
    let mut last_runtime_status = None;

    while !stop_requested.load(Ordering::SeqCst) && !inbox.closed.load(Ordering::SeqCst) {
        let observed = inbox.updates.version();
        {
            let mut output = RfbDeadlineIo::for_message(stream, stop_requested);
            let latest_revision = discovery_context.revision();
            if supports_fence && discovery_hello_sent && discovery_revision != latest_revision {
                discovery_revision = announce_server_discovery(&mut output, &discovery_context)?;
            }
            // Only this worker writes control messages and owns compression state.
            let interest = pending_interest.or_else(|| {
                continuous_updates_enabled.then_some(crate::remote_desktop_core::FrameInterest {
                    incremental: true,
                    region: continuous_updates_region,
                })
            });
            if let Some(interest) = interest {
                if let Some(display) = send_available_core_update(
                    &mut output,
                    &shared_display,
                    &frame_cache,
                    &framebuffer_core,
                    &client_id,
                    supports_desktop_size,
                    &mut last_framebuffer,
                    pixel_format,
                    framebuffer_encoding,
                    &mut zlib_encoder,
                    interest.incremental,
                    interest.region,
                )? {
                    #[cfg(target_os = "windows")]
                    if crate::local_vnc_server_platform::windows_desktop::is_service_worker() {
                        crate::local_vnc_server_platform::windows_desktop::frame_delivered(
                            display.desktop_epoch,
                        );
                    }
                    *last_sent_display
                        .write()
                        .map_err(|_| std::io::Error::other("sent display poisoned"))? =
                        Some(display);
                    pending_interest = None;
                    framebuffer_update_sent = true;
                }
            }
            if framebuffer_update_sent && relay.is_none() {
                publish_changed_platform_clipboard(
                    &mut output,
                    client_extended_clipboard_ready,
                    &mut clipboard_publish_state,
                )?;
            }
            if supports_runtime_status {
                let notice = ServerRuntimeMessage::Desktop {
                    desktop: frame_cache.desktop_state(),
                    secure_attention_available: relay
                        .as_ref()
                        .map(|relay| relay.sas_available())
                        .unwrap_or_else(service_secure_attention_available),
                };
                if last_runtime_status != Some(notice) {
                    write_runtime_message(&mut output, notice)?;
                    last_runtime_status = Some(notice);
                }
            }
            output.flush()?;
        }
        // Capture/control/close wake immediately. This fallback bounds stop flag
        // checks and polling for platform clipboard changes, which have no signal.
        let message = match inbox.next_after(observed, RFB_IO_POLL)? {
            ClientInboxItem::Message(message) => message,
            ClientInboxItem::Idle => continue,
            ClientInboxItem::Closed => return Ok(()),
        };
        let mut output = RfbDeadlineIo::for_message(stream, stop_requested);
        match message {
            ClientMessage::PixelFormat(payload) => {
                pixel_format = parse_set_pixel_format(payload);
            }
            ClientMessage::Encodings(encodings) => {
                let support = parse_client_encoding_support(&encodings);
                supports_fence = support.fence;
                if supports_runtime_status != support.runtime_status {
                    last_runtime_status = None;
                }
                supports_runtime_status = support.runtime_status;
                supports_desktop_size = support.desktop_size;
                framebuffer_encoding = support.framebuffer_encoding;
                if support.continuous_updates && !continuous_updates_announced {
                    write_end_of_continuous_updates(&mut output)?;
                    continuous_updates_announced = true;
                }
                if supports_extended_clipboard && !support.extended_clipboard {
                    client_extended_clipboard_ready = false;
                }
                supports_extended_clipboard = support.extended_clipboard;
                if relay.is_some() {
                    supports_extended_clipboard = false;
                }
                #[cfg(target_os = "windows")]
                if crate::local_vnc_server_platform::windows_desktop::is_service_worker() {
                    supports_extended_clipboard = false;
                }
                if support.fence && !discovery_hello_sent {
                    discovery_revision =
                        announce_server_discovery(&mut output, &discovery_context)?;
                    discovery_hello_sent = true;
                }
                if supports_extended_clipboard && !client_extended_clipboard_ready {
                    write_extended_clipboard_caps(&mut output)?;
                }
            }
            ClientMessage::FrameRequest(interest) => {
                if !continuous_updates_enabled || !interest.incremental {
                    pending_interest = Some(
                        pending_interest
                            .map(|previous| merge_frame_interest(previous, interest))
                            .unwrap_or(interest),
                    );
                }
            }
            ClientMessage::CutText {
                signed_length,
                payload,
            } => {
                if relay.is_some() {
                    #[cfg(target_os = "windows")]
                    forward_service_clipboard(signed_length, &payload, view_only);
                    continue;
                }
                #[cfg(target_os = "windows")]
                if crate::local_vnc_server_platform::windows_desktop::is_service_worker() {
                    continue;
                }
                let outcome = handle_client_cut_text(
                    &mut output,
                    signed_length,
                    &payload,
                    view_only,
                    supports_extended_clipboard,
                    &mut clipboard_publish_state,
                )?;
                if outcome.received_extended_clipboard_caps {
                    client_extended_clipboard_ready = true;
                }
            }
            ClientMessage::Continuous { enabled, region } => {
                continuous_updates_enabled = enabled;
                continuous_updates_region = enabled.then_some(region);
                if !enabled {
                    write_end_of_continuous_updates(&mut output)?;
                }
            }
            ClientMessage::Fence { flags, payload } => {
                if support_fence_request(flags, discovery_hello_sent) {
                    // The reader applies input independently of this writer, so
                    // synchronization flags are unsupported and must be cleared.
                    let response = crate::remote_desktop_core::rfb_fence::fence_message(
                        0, &payload,
                    )
                    .map_err(|code| std::io::Error::new(std::io::ErrorKind::InvalidData, code))?;
                    output.write_all(&response)?;
                    if supports_runtime_status {
                        if let Some(relay) = &relay {
                            handle_relay_runtime_request(&mut output, &payload, view_only, relay)?;
                        } else {
                            handle_runtime_request(&mut output, &payload, view_only)?;
                        }
                    }
                    crate::server_discovery::handle_discover_request(
                        &mut output,
                        &discovery_context,
                        &payload,
                    )?;
                }
            }
            ClientMessage::Key { .. } | ClientMessage::Pointer { .. } => {
                unreachable!("input belongs to the reader")
            }
        }
        output.flush()?;
    }
    // Preserve a reader protocol error even if it closed between writer iterations.
    if inbox.closed.load(Ordering::SeqCst) {
        let _ = inbox.next(Duration::ZERO)?;
    }
    Ok(())
}

// Observe before I/O: changes during a write still need a subsequent announcement.
fn announce_server_discovery<W: Write>(
    output: &mut W,
    context: &crate::server_discovery::ServerDiscoveryContext,
) -> Result<u64, std::io::Error> {
    let revision = context.revision();
    write_server_discovery_hello(output)?;
    Ok(revision)
}

#[allow(clippy::too_many_arguments)]
fn send_available_core_update<W: Write>(
    stream: &mut W,
    shared_display: &Arc<RwLock<SharedDisplay>>,
    frame_cache: &Arc<LocalVncServerFrameCache>,
    framebuffer_core: &Arc<crate::remote_desktop_core::ServerFramebufferCore>,
    client_id: &str,
    supports_desktop_size: bool,
    last_framebuffer: &mut DisplaySize,
    pixel_format: RfbPixelFormat,
    framebuffer_encoding: RfbFramebufferEncoding,
    zlib_encoder: &mut RfbZlibEncoder,
    incremental: bool,
    requested_region: Option<crate::remote_desktop_core::FramebufferRect>,
) -> Result<Option<SharedDisplay>, std::io::Error> {
    let display = shared_display
        .read()
        .map_err(|_| std::io::Error::other("display state poisoned"))?
        .clone();
    if !display.desktop_is_current() {
        return Ok(None);
    }
    let framebuffer_resized = display.framebuffer != *last_framebuffer;
    if framebuffer_resized && !supports_desktop_size {
        return Ok(None);
    }
    let Some(frame) = framebuffer_core
        .next_update_for_source(
            client_id,
            if framebuffer_resized {
                crate::remote_desktop_core::FrameInterest::full()
            } else if incremental {
                requested_region
                    .map(crate::remote_desktop_core::FrameInterest::incremental_region)
                    .unwrap_or_else(crate::remote_desktop_core::FrameInterest::incremental)
            } else {
                requested_region
                    .map(crate::remote_desktop_core::FrameInterest::full_region)
                    .unwrap_or_else(crate::remote_desktop_core::FrameInterest::full)
            },
            crate::remote_desktop_core::FramebufferGeometry {
                width: display.framebuffer.width,
                height: display.framebuffer.height,
                pixel_format: crate::remote_desktop_core::PixelFormat::Bgra8888,
            },
            display.revision,
        )
        .map_err(std::io::Error::other)?
    else {
        return Ok(None);
    };
    // Announce only after owning matching pixels. No state lock is held during I/O.
    if framebuffer_resized {
        write_desktop_size_update(stream, display.framebuffer)?;
        *last_framebuffer = display.framebuffer;
    }
    match write_core_framebuffer_update(
        stream,
        &frame,
        frame_cache,
        pixel_format,
        framebuffer_encoding,
        zlib_encoder,
    ) {
        Ok(()) => {
            framebuffer_core
                .commit_delivery(client_id, &frame)
                .map_err(std::io::Error::other)?;
            Ok(Some(display))
        }
        Err(error) if is_recoverable_framebuffer_source_error(&error) => {
            eprintln!("local VNC framebuffer: skipped invalid captured frame: {error}");
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn support_fence_request(flags: u32, discovery_hello_sent: bool) -> bool {
    const REQUEST_BIT: u32 = 1 << 31;
    discovery_hello_sent && flags & REQUEST_BIT != 0
}
