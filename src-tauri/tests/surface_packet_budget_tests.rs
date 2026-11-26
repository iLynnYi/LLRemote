use LLRemoteLib::remote_desktop_core::{
    ClientFramebufferCore, DecodedFramebufferRect, DecodedFramebufferUpdate, FramebufferGeometry,
    FramebufferRect, PixelFormat, SurfaceFramePacket, SurfaceFrameUpdate,
};

fn geometry() -> FramebufferGeometry {
    FramebufferGeometry {
        width: 4,
        height: 2,
        pixel_format: PixelFormat::Bgra8888,
    }
}
fn core() -> ClientFramebufferCore {
    let mut core = ClientFramebufferCore::default();
    core.attach_surface("main", true).unwrap();
    publish(&mut core, 0);
    core.mark_surface_presented("main", 1).unwrap();
    core
}
fn publish(core: &mut ClientFramebufferCore, value: u8) {
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry()),
            pixels: vec![value; geometry().byte_len().unwrap()],
        }],
    })
    .unwrap();
}

#[test]
fn a_slow_surface_receives_one_current_frame_after_repeated_full_updates() {
    let mut core = core();
    for value in 1..=64 {
        publish(&mut core, value);
    }
    let update = core.next_surface_update("main").unwrap().unwrap();
    assert_eq!(update.frame_id, 65);
    assert_eq!(update.dirty_rectangles, [FramebufferRect::full(geometry())]);
    let packet = SurfaceFramePacket::from_core(&core, &update).unwrap();
    assert_eq!(packet.bytes().len(), 28 + 16 + 32);
    assert_eq!(&packet.bytes()[44..], &[64; 32]);
}

#[test]
fn packet_building_rejects_excess_payload_and_rect_count_before_allocation() {
    let core = core();
    for rectangles in [
        vec![FramebufferRect::full(geometry()); 2],
        vec![
            FramebufferRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1
            };
            65
        ],
    ] {
        let update = SurfaceFrameUpdate {
            frame_id: 1,
            geometry: geometry(),
            dirty_rectangles: rectangles,
        };
        assert_eq!(
            SurfaceFramePacket::from_core(&core, &update).err().unwrap(),
            "surface_packet_resource_limit"
        );
    }
}

#[test]
fn packet_building_rejects_huge_out_of_bounds_rectangles_before_reserving_pixels() {
    let core = core();
    let update = SurfaceFrameUpdate {
        frame_id: 1,
        geometry: geometry(),
        dirty_rectangles: vec![FramebufferRect {
            x: 0,
            y: 0,
            width: u16::MAX as u32,
            height: u16::MAX as u32,
        }],
    };
    assert_eq!(
        SurfaceFramePacket::from_core(&core, &update).err().unwrap(),
        "surface_rect_out_of_bounds"
    );
}
