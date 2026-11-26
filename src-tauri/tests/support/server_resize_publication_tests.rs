#[test]
fn desktop_resize_waits_for_matching_pixels_and_never_sends_old_geometry() {
    use crate::remote_desktop_core::{
        Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let (mut sender, _) = listener.accept().unwrap();
    let old_size = DisplaySize {
        width: 1,
        height: 1,
    };
    let new_size = DisplaySize {
        width: 2,
        height: 1,
    };
    let desktop_source =
        crate::local_vnc_server::desktop_relay::test_support::SourceFixture::new(7);
    let display = Arc::new(RwLock::new(SharedDisplay {
        relay: Some(desktop_source.relay.clone()),
        revision: 0,
        desktop_epoch: 0,
        id: "resize-fixture".into(),
        label: "resize-fixture".into(),
        frame: crate::local_vnc_server::types::DisplayFrame {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
            pixel_width: 2,
            pixel_height: 1,
        },
        framebuffer: new_size,
        resolution_mode: crate::local_vnc_server::types::LocalVncServerResolutionMode::Native,
        refresh_mode: crate::local_vnc_server::types::LocalVncServerRefreshMode::Balanced,
    }));
    let core = Arc::new(ServerFramebufferCore::default());
    core.attach_client("resize-client").unwrap();
    let publish = |size: DisplaySize, value: u8, revision: u64| {
        let geometry = FramebufferGeometry {
            width: size.width,
            height: size.height,
            pixel_format: PixelFormat::Bgra8888,
        };
        core.publish_for_source(
            Framebuffer::new(geometry, vec![value; geometry.byte_len().unwrap()]).unwrap(),
            revision,
        )
        .unwrap();
    };
    publish(old_size, 7, 0);
    let cache = Arc::new(LocalVncServerFrameCache::default());
    let mut encoder = RfbZlibEncoder::new();
    let mut last_size = old_size;
    let send =
        |last_size: &mut DisplaySize, encoder: &mut RfbZlibEncoder, sender: &mut TcpStream| {
            send_available_core_update(
                sender,
                &display,
                &cache,
                &core,
                "resize-client",
                true,
                last_size,
                default_rfb_pixel_format(),
                RfbFramebufferEncoding::Raw,
                encoder,
                false,
                None,
            )
            .unwrap()
        };

    assert!(send(&mut last_size, &mut encoder, &mut sender).is_none());
    let mut probe = [0; 1];
    let error = peer
        .read(&mut probe)
        .expect_err("a failed capture must not announce the requested size");
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    assert_eq!(last_size, old_size);

    publish(new_size, 9, 0);
    let sent_display = send(&mut last_size, &mut encoder, &mut sender).unwrap();
    let mut wire = [0; 40];
    peer.read_exact(&mut wire).unwrap();
    assert_eq!(
        &wire[..16],
        &[0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 255, 255, 255, 33]
    );
    assert_eq!(
        &wire[16..32],
        &[0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0]
    );
    assert_eq!(last_size, new_size);

    publish(old_size, 11, 0);
    assert!(send(&mut last_size, &mut encoder, &mut sender).is_none());
    let error = peer
        .read(&mut probe)
        .expect_err("old-size pixels must not follow the new DesktopSize");
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));

    // Matching dimensions alone cannot identify the display that produced pixels.
    let mut next_display = sent_display.clone();
    next_display.id = "second-display".into();
    next_display.frame.x = 1920;
    display.write().unwrap().replace_with(next_display).unwrap();
    publish(new_size, 12, 0);
    assert!(send(&mut last_size, &mut encoder, &mut sender).is_none());
    assert_eq!(
        sent_display.frame.x, 0,
        "the client's last sent input mapping stays on the old display"
    );
    let error = peer
        .read(&mut probe)
        .expect_err("same-size old-display pixels must not be sent");
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    publish(new_size, 13, 1);
    let switched = send(&mut last_size, &mut encoder, &mut sender).unwrap();
    assert_eq!(switched.frame.x, 1920);
    assert_eq!(switched.revision, 1);
    let mut wire = [0; 24];
    peer.read_exact(&mut wire).unwrap();
    assert_eq!(
        &wire[..16],
        &[0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0]
    );
    desktop_source.relay.detach();
    publish(new_size, 14, 1);
    assert!(send(&mut last_size, &mut encoder, &mut sender).is_none());
    assert!(
        peer.read(&mut probe).is_err(),
        "a detached desktop cannot send retained pixels"
    );
}
