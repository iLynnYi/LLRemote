use super::*;
use crate::remote_desktop_core::{
    ClientRfbPixelFormat, RfbClientFramebufferDecoder, SurfaceFramePacket,
};

fn geometry() -> FramebufferGeometry {
    FramebufferGeometry {
        width: 4,
        height: 2,
        pixel_format: PixelFormat::Bgra8888,
    }
}

#[test]
fn connection_clear_releases_the_old_pixel_allocation() {
    let mut core = ClientFramebufferCore::default();
    core.reset_geometry(geometry()).unwrap();
    assert!(core.pixels.capacity() >= 32);
    core.clear_connection();
    assert_eq!(core.pixels.capacity(), 0);
}

#[test]
fn invalid_geometry_is_rejected_before_replacing_the_previous_frame() {
    let mut core = ClientFramebufferCore::default();
    core.reset_geometry(geometry()).unwrap();
    let mut invalid = geometry();
    invalid.width = 0;
    assert_eq!(
        core.reset_geometry(invalid).unwrap_err(),
        "rfb_invalid_framebuffer_geometry"
    );
    assert_eq!(core.geometry(), Some(geometry()));
    assert_eq!(core.pixels().len(), 32);
}

fn raw(value: u8) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 4, 0, 2, 0, 0, 0, 0];
    bytes.extend_from_slice(&[value; 32]);
    bytes
}

#[test]
fn raw_and_copyrect_updates_fit_exactly_two_framebuffer_allocations() {
    for raw_updates in [true, false] {
        let memory = ClientMemoryBudget::new(64, 0);
        let mut core = ClientFramebufferCore::with_memory(memory.clone());
        let mut decoder =
            RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
        decoder
            .decode_framebuffer_update(&raw(7), geometry(), &mut core)
            .unwrap();
        for value in 8..28 {
            let message = if raw_updates {
                raw(value)
            } else {
                // Move a three-pixel source row into the overlapping desktop.
                vec![0, 0, 0, 1, 0, 1, 0, 1, 0, 3, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0]
            };
            decoder
                .decode_framebuffer_update(&message, geometry(), &mut core)
                .unwrap();
            assert_eq!(memory.in_use(), 64);
            assert_eq!(
                &core.pixels()[..3],
                &[if raw_updates { value } else { 7 }; 3]
            );
        }
        drop(decoder);
        core.clear_connection();
        assert_eq!(memory.in_use(), 0);
    }
}

#[test]
fn tight_fill_updates_fit_exactly_two_framebuffer_allocations() {
    for encoding in [7_i32, -260] {
        for (format, pixel) in [
            (ClientRfbPixelFormat::core_bgra32(), vec![0x12, 0x34, 0x56]),
            (
                ClientRfbPixelFormat {
                    depth: 32,
                    big_endian: true,
                    ..ClientRfbPixelFormat::core_bgra32()
                },
                vec![0, 0x12, 0x34, 0x56],
            ),
        ] {
            let memory = ClientMemoryBudget::new(64, 0);
            let mut core = ClientFramebufferCore::with_memory(memory.clone());
            let mut decoder = RfbClientFramebufferDecoder::new(format).unwrap();
            let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 4, 0, 2];
            message.extend(encoding.to_be_bytes());
            message.push(0x8f);
            message.extend(pixel);
            for frame in 1..=20 {
                let outcome = decoder
                    .decode_framebuffer_update(&message, geometry(), &mut core)
                    .unwrap();
                assert_eq!(core.pixels(), [0x56, 0x34, 0x12, 255].repeat(8));
                assert_eq!(outcome.frame_id, Some(frame));
                assert_eq!(memory.in_use(), if frame == 1 { 32 } else { 64 });
                assert_eq!(outcome.encodings.rectangles, 1);
                assert_eq!(outcome.encodings.tight, 1);
                assert_eq!(outcome.encodings.jpeg, 0);
            }
            drop(decoder);
            core.clear_connection();
            assert_eq!(memory.in_use(), 0);
        }
    }
}

#[test]
fn allocation_failure_preserves_geometry_pixels_and_frame_version() {
    let memory = ClientMemoryBudget::new(128, 0);
    let mut core = ClientFramebufferCore::with_memory(memory.clone());
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    decoder
        .decode_framebuffer_update(&raw(7), geometry(), &mut core)
        .unwrap();
    let before = core.pixels().to_vec();
    let mut larger = geometry();
    larger.height = 8;
    assert_eq!(
        core.reset_geometry(larger).unwrap_err(),
        "rfb_resource_limit:session_memory"
    );
    assert_eq!(core.geometry(), Some(geometry()));
    assert_eq!(core.pixels(), before);
    assert_eq!(core.latest_frame_id(), Some(1));
    assert_eq!(memory.in_use(), 32);
}

#[test]
fn swapping_reused_framebuffers_keeps_exactly_two_capacity_charges() {
    let memory = ClientMemoryBudget::new(1024, 0);
    let mut core = ClientFramebufferCore::with_memory(memory.clone());
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    for value in 0..100 {
        decoder
            .decode_framebuffer_update(&raw(value), geometry(), &mut core)
            .unwrap();
        assert_eq!(memory.in_use(), if value == 0 { 32 } else { 64 });
        assert_eq!(&core.pixels()[..3], &[value; 3]);
    }
    core.clear_connection();
    assert_eq!(memory.in_use(), 32);
    drop(decoder);
    assert_eq!(memory.in_use(), 0);
}

#[test]
fn a_surface_packet_competes_with_decoder_storage_and_keeps_its_transport_charge() {
    let memory = ClientMemoryBudget::new(128, 0);
    let mut core = ClientFramebufferCore::with_memory(memory.clone());
    core.attach_surface("main", true).unwrap();
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    decoder
        .decode_framebuffer_update(&raw(1), geometry(), &mut core)
        .unwrap();
    decoder
        .decode_framebuffer_update(&raw(2), geometry(), &mut core)
        .unwrap();
    let update = core.next_surface_update("main").unwrap().unwrap();
    assert_eq!(
        SurfaceFramePacket::from_core(&core, &update).err().unwrap(),
        "rfb_resource_limit:session_memory"
    );
    assert_eq!(core.latest_frame_id(), Some(2));
    drop(decoder);
    let packet = SurfaceFramePacket::from_core(&core, &update).unwrap();
    assert_eq!(memory.in_use(), 32 + 76);
    let (bytes, reservation) = packet.into_transport_parts();
    core.clear_connection();
    assert_eq!(memory.in_use(), 76);
    assert_eq!(bytes.len(), 76);
    drop(bytes);
    drop(reservation);
    assert_eq!(memory.in_use(), 0);
}
