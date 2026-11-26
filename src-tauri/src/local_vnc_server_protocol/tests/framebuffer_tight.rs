#[test]
fn framebuffer_rect_writer_uses_tight_fill_for_solid_regions() {
    let framebuffer = DisplaySize { width: 2, height: 1 };
    let rect = FramebufferRect { x: 0, y: 0, width: 2, height: 1 };
    let pixels = [10, 20, 30, 255, 10, 20, 30, 255];
    let mut output = Vec::new();
    let mut encoder = RfbZlibEncoder::new();

    write_framebuffer_rect(
        &mut output,
        &pixels,
        framebuffer,
        &rect,
        core_bgra32_pixel_format(),
        RfbFramebufferEncoding::Tight,
        &mut encoder,
    ).unwrap();

    assert_eq!(&output[8..12], &TIGHT_ENCODING.to_be_bytes());
    assert_eq!(&output[12..], &[0x80, 30, 20, 10]);
}

#[test]
fn framebuffer_rect_writer_rotates_across_four_tight_streams() {
    let framebuffer = DisplaySize { width: 2, height: 2 };
    let rect = FramebufferRect { x: 0, y: 0, width: 2, height: 2 };
    let pixels = [
        0, 0, 255, 255, 0, 255, 0, 255,
        255, 0, 0, 255, 20, 40, 60, 255,
    ];
    let mut encoder = RfbZlibEncoder::new();
    let mut controls = Vec::new();
    for _ in 0..4 {
        let mut output = Vec::new();
        write_framebuffer_rect(
            &mut output,
            &pixels,
            framebuffer,
            &rect,
            core_bgra32_pixel_format(),
            RfbFramebufferEncoding::Tight,
            &mut encoder,
        ).unwrap();
        controls.push(output[12] >> 4);
    }
    assert_eq!(controls, vec![0, 1, 2, 3]);
}

#[test]
fn tight_server_writer_round_trips_through_native_client_core() {
    use crate::remote_desktop_core::{
        ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
        RfbClientFramebufferDecoder,
    };

    let framebuffer = DisplaySize { width: 2, height: 2 };
    let rect = FramebufferRect { x: 0, y: 0, width: 2, height: 2 };
    let pixels = [
        0, 0, 255, 255, 0, 255, 0, 255,
        255, 0, 0, 255, 20, 40, 60, 255,
    ];
    let mut encoded_rect = Vec::new();
    write_framebuffer_rect(
        &mut encoded_rect,
        &pixels,
        framebuffer,
        &rect,
        core_bgra32_pixel_format(),
        RfbFramebufferEncoding::Tight,
        &mut RfbZlibEncoder::new(),
    )
    .unwrap();
    let mut update = vec![0, 0, 0, 1];
    update.extend_from_slice(&encoded_rect);

    let geometry = FramebufferGeometry { width: 2, height: 2, pixel_format: PixelFormat::Bgra8888 };
    let mut core = ClientFramebufferCore::default();
    let mut decoder = RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    decoder.decode_framebuffer_update(&update, geometry, &mut core).unwrap();

    assert_eq!(core.snapshot().unwrap().pixels(), pixels);
}
