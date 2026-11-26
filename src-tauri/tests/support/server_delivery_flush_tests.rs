#[test]
fn a_failed_server_flush_keeps_the_selected_pixels_undelivered() {
    use crate::remote_desktop_core::{
        FrameInterest, Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut sender, _) = listener.accept().unwrap();
    let size = DisplaySize {
        width: 2,
        height: 1,
    };
    let desktop_source =
        crate::local_vnc_server::desktop_relay::test_support::SourceFixture::new(7);
    let display = Arc::new(RwLock::new(SharedDisplay {
        relay: Some(desktop_source.relay.clone()),
        framebuffer: size,
        revision: 0,
        desktop_epoch: 0,
        id: "delivery-fixture".into(),
        label: "delivery-fixture".into(),
        frame: crate::local_vnc_server::types::DisplayFrame {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
            pixel_width: 2,
            pixel_height: 1,
        },
        resolution_mode: crate::local_vnc_server::types::LocalVncServerResolutionMode::Native,
        refresh_mode: crate::local_vnc_server::types::LocalVncServerRefreshMode::Responsive,
    }));
    let geometry = FramebufferGeometry {
        width: 2,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    let core = Arc::new(ServerFramebufferCore::default());
    core.attach_client("client").unwrap();
    core.publish_for_source(Framebuffer::new(geometry, vec![7; 8]).unwrap(), 0)
        .unwrap();
    sender.shutdown(Shutdown::Write).unwrap();
    let result = send_available_core_update(
        &mut sender,
        &display,
        &Arc::new(LocalVncServerFrameCache::default()),
        &core,
        "client",
        false,
        &mut size.clone(),
        default_rfb_pixel_format(),
        RfbFramebufferEncoding::Raw,
        &mut RfbZlibEncoder::new(),
        true,
        None,
    );
    assert!(
        result.is_err(),
        "a closed write side cannot flush a framebuffer"
    );
    assert!(core
        .next_update_for_source("client", FrameInterest::incremental(), geometry, 0)
        .unwrap()
        .is_some());
}
