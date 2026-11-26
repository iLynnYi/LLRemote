use LLRemoteLib::remote_desktop_core::{
    FrameInterest, Framebuffer, FramebufferGeometry, FramebufferRect, PixelFormat,
    ServerFramebufferCore,
};

fn geometry() -> FramebufferGeometry {
    FramebufferGeometry {
        width: 128,
        height: 64,
        pixel_format: PixelFormat::Bgra8888,
    }
}

fn frame(pixels: Vec<u8>) -> Framebuffer {
    Framebuffer::new(geometry(), pixels).unwrap()
}

#[test]
fn selecting_a_frame_does_not_consume_undelivered_pixels() {
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    core.publish(frame(vec![0; geometry().byte_len().unwrap()]))
        .unwrap();
    let first = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    let retry = core
        .next_update("client", FrameInterest::incremental())
        .unwrap();
    assert_eq!(
        retry
            .expect("an unsent selection must remain available")
            .frame_id,
        first.frame_id
    );
}

#[test]
fn slow_client_gets_cumulative_changes_without_an_unrelated_full_frame() {
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let mut pixels = vec![0; geometry().byte_len().unwrap()];
    core.publish(frame(pixels.clone())).unwrap();
    let first = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    core.commit_delivery("client", &first).unwrap();
    pixels[0] = 9;
    core.publish(frame(pixels.clone())).unwrap();
    core.publish(frame(pixels.clone())).unwrap();
    pixels[34 * 4] = 7;
    core.publish(frame(pixels)).unwrap();
    let update = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    assert_eq!(
        update.dirty_rectangles,
        vec![
            FramebufferRect {
                x: 0,
                y: 0,
                width: 32,
                height: 32
            },
            FramebufferRect {
                x: 32,
                y: 0,
                width: 32,
                height: 32
            }
        ]
    );
}

#[test]
fn delivery_receipts_reject_superseded_and_replaced_owners() {
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    core.publish(frame(vec![0; geometry().byte_len().unwrap()]))
        .unwrap();
    let first = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    let newer = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    assert_eq!(
        core.commit_delivery("client", &first).unwrap_err(),
        "server_delivery_receipt_stale"
    );
    core.commit_delivery("client", &newer).unwrap();
    assert!(core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .is_none());
    core.detach_client("client").unwrap();
    core.attach_client("client").unwrap();
    assert_eq!(
        core.commit_delivery("client", &newer).unwrap_err(),
        "server_delivery_receipt_stale"
    );
    assert!(core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .is_some());
    let other = ServerFramebufferCore::default();
    other.attach_client("client").unwrap();
    assert_eq!(
        other.commit_delivery("client", &newer).unwrap_err(),
        "server_delivery_receipt_stale"
    );
}

#[test]
fn history_eviction_and_resize_require_complete_pixels() {
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let mut pixels = vec![0; geometry().byte_len().unwrap()];
    core.publish(frame(pixels.clone())).unwrap();
    let first = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    core.commit_delivery("client", &first).unwrap();
    for index in 0..65 {
        pixels[0] = 1 + (index % 2) as u8;
        core.publish(frame(pixels.clone())).unwrap();
    }
    let update = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    assert_eq!(
        update.dirty_rectangles,
        vec![FramebufferRect::full(geometry())]
    );
    core.commit_delivery("client", &update).unwrap();
    let resized = FramebufferGeometry {
        width: 64,
        ..geometry()
    };
    core.publish(Framebuffer::new(resized, vec![0; resized.byte_len().unwrap()]).unwrap())
        .unwrap();
    let update = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    assert_eq!(
        update.dirty_rectangles,
        vec![FramebufferRect::full(resized)]
    );
}

#[test]
fn unchanged_capture_does_not_evict_a_visible_baseline() {
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let pixels = vec![0; geometry().byte_len().unwrap()];
    let id = core.publish(frame(pixels.clone())).unwrap();
    let first = core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    core.commit_delivery("client", &first).unwrap();
    for _ in 0..128 {
        assert_eq!(core.publish(frame(pixels.clone())).unwrap(), id);
    }
    assert!(core
        .next_update("client", FrameInterest::incremental())
        .unwrap()
        .is_none());
}
