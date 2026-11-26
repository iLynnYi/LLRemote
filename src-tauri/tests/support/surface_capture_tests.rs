use super::*;
use crate::remote_desktop_core::{
    ClientFramebufferHub, DecodedFramebufferRect, DecodedFramebufferUpdate, PixelFormat,
};

#[test]
fn captured_rows_finish_after_core_mutation_without_tearing_or_an_extra_allocation() {
    let hub = ClientFramebufferHub::default();
    let geometry = FramebufferGeometry {
        width: 4,
        height: 2,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut core = hub.lock().unwrap();
    core.attach_surface("main", true).unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry,
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry),
            pixels: [10, 20, 30, 255].repeat(8),
        }],
    })
    .unwrap();
    let update = core.next_surface_update("main").unwrap().unwrap();
    let draft = SurfaceFramePacket::capture_from_core(&core, &update).unwrap();
    let allocation = draft.bytes.as_ptr();
    let memory = hub.memory_budget();
    let captured_usage = memory.in_use();
    drop(core);
    // Capture retains neither a lock nor a borrow of pixels that the decoder
    // can change/recycle before packet conversion and sending.
    hub.lock().unwrap().clear_connection();
    let packet = draft.finish();
    assert_eq!(packet.bytes().as_ptr(), allocation);
    assert_eq!(memory.in_use(), captured_usage - 32);
    assert_eq!(&packet.bytes()[44..], [30, 20, 10, 255].repeat(8));
    drop(packet);
    assert_eq!(memory.in_use(), 64 * 1024 * 1024);
}
