use std::io::Write;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use LLRemoteLib::remote_desktop_core::{
    encode_discovery_frames, ClientFramebufferCore, ClientRfbPixelFormat, DecodedFramebufferRect,
    DecodedFramebufferUpdate, DiscoveryFrameType, FrameInterest, Framebuffer, FramebufferGeometry,
    FramebufferRect, PixelFormat, RfbClientFramebufferDecoder, RfbClientHandshake,
    RfbClientHandshakeEvent, RfbClientSessionCore, RfbClientSessionEvent, ServerFramebufferCore,
    SurfaceFramePacket,
};

#[test]
fn eight_bit_channels_preserve_all_values_across_byte_orders_and_encodings() {
    for depth in [24, 32] {
        for high_channels in [false, true] {
            for big_endian in [false, true] {
                let mut format = ClientRfbPixelFormat {
                    depth,
                    big_endian,
                    ..ClientRfbPixelFormat::core_bgra32()
                };
                if high_channels {
                    format.red_shift = 24;
                    format.green_shift = 16;
                    format.blue_shift = 8;
                }
                let mut wire_pixels = Vec::new();
                let mut expected = Vec::new();
                for red in 0..=255_u8 {
                    let green = red ^ 0x55;
                    let blue = red ^ 0xaa;
                    let padding = red ^ 0x0f;
                    // Explicit byte layouts, independent of the decoder's bit-field helper.
                    wire_pixels.extend(match (high_channels, big_endian) {
                        (false, false) => [blue, green, red, padding],
                        (false, true) => [padding, red, green, blue],
                        (true, false) => [padding, blue, green, red],
                        (true, true) => [red, green, blue, padding],
                    });
                    expected.extend([blue, green, red, 255]);
                }
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
                encoder.write_all(&wire_pixels).unwrap();
                let compressed = encoder.finish().unwrap();
                let zlib = [
                    (compressed.len() as u32).to_be_bytes().as_slice(),
                    &compressed,
                ]
                .concat();
                let hextile = [&[1][..], &wire_pixels].concat(); // One complete 16x16 Raw tile.
                for (encoding, body) in [(0_i32, &wire_pixels), (5, &hextile), (6, &zlib)] {
                    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 16, 0, 16];
                    message.extend(encoding.to_be_bytes());
                    message.extend(body);
                    let geometry = FramebufferGeometry {
                        width: 16,
                        height: 16,
                        pixel_format: PixelFormat::Bgra8888,
                    };
                    let mut decoder = RfbClientFramebufferDecoder::new(format).unwrap();
                    let mut core = ClientFramebufferCore::default();
                    decoder
                        .decode_framebuffer_update(&message, geometry, &mut core)
                        .unwrap();
                    assert_eq!(core.pixels(), expected,
                        "depth={depth} high_channels={high_channels} big_endian={big_endian} encoding={encoding}");
                    assert_eq!(core.latest_frame_id(), Some(1));
                }
            }
        }
    }
}

fn geometry() -> FramebufferGeometry {
    FramebufferGeometry {
        width: 4,
        height: 2,
        pixel_format: PixelFormat::Bgra8888,
    }
}
fn frame(fill: u8) -> Framebuffer {
    Framebuffer::new(geometry(), vec![fill; geometry().byte_len().unwrap()]).unwrap()
}

#[test]
fn server_core_delivers_full_first_frame_and_incremental_dirty_tiles_per_client() {
    let core = ServerFramebufferCore::default();
    core.attach_client("one").unwrap();
    core.attach_client("two").unwrap();
    core.publish(frame(0)).unwrap();
    let first = core
        .next_update("one", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    assert_eq!(
        first.dirty_rectangles,
        vec![FramebufferRect::full(geometry())]
    );
    core.commit_delivery("one", &first).unwrap();
    assert!(core
        .next_update("one", FrameInterest::incremental())
        .unwrap()
        .is_none());

    let mut changed = vec![0; geometry().byte_len().unwrap()];
    changed[0] = 9;
    core.publish(Framebuffer::new(geometry(), changed).unwrap())
        .unwrap();
    assert!(!core
        .next_update("one", FrameInterest::incremental())
        .unwrap()
        .unwrap()
        .dirty_rectangles
        .is_empty());
    let second = core
        .next_update("two", FrameInterest::incremental())
        .unwrap()
        .unwrap();
    assert_eq!(
        second.dirty_rectangles,
        vec![FramebufferRect::full(geometry())]
    );
    core.commit_delivery("two", &second).unwrap();

    let mut newer = vec![0; geometry().byte_len().unwrap()];
    newer[31] = 3;
    core.publish(Framebuffer::new(geometry(), newer).unwrap())
        .unwrap();
    assert_eq!(
        core.next_update("two", FrameInterest::incremental())
            .unwrap()
            .unwrap()
            .dirty_rectangles,
        vec![FramebufferRect::full(geometry())]
    );
}

#[test]
fn server_core_clips_updates_to_interest_and_refreshes_when_region_changes() {
    let core = ServerFramebufferCore::default();
    core.attach_client("regional").unwrap();
    core.publish(frame(7)).unwrap();
    let left = FramebufferRect {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    };
    let right = FramebufferRect {
        x: 2,
        y: 0,
        width: 2,
        height: 2,
    };

    let first = core
        .next_update("regional", FrameInterest::incremental_region(left))
        .unwrap()
        .unwrap();
    assert_eq!(first.dirty_rectangles, vec![left]);
    core.commit_delivery("regional", &first).unwrap();
    assert!(core
        .next_update("regional", FrameInterest::incremental_region(left))
        .unwrap()
        .is_none());

    let switched = core
        .next_update("regional", FrameInterest::incremental_region(right))
        .unwrap()
        .unwrap();
    assert_eq!(switched.dirty_rectangles, vec![right]);
}

#[test]
fn recording_consumer_survives_display_switches_without_taking_display_ownership() {
    let mut core = ClientFramebufferCore::default();
    core.attach_surface("main", true).unwrap();
    core.attach_surface("external", false).unwrap();
    core.attach_recording_surface("recording").unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry()),
            pixels: vec![3; geometry().byte_len().unwrap()],
        }],
    })
    .unwrap();
    for display in ["main", "external", "main"] {
        core.set_active_surface(display).unwrap();
        assert!(core.next_surface_update(display).unwrap().is_some());
        assert!(core.next_surface_update("recording").unwrap().is_some());
        assert!(core.surfaces().iter().any(|s| s.id == display && s.active));
        assert!(
            !core
                .surfaces()
                .iter()
                .find(|s| s.id == "recording")
                .unwrap()
                .active
        );
    }
    assert!(core.set_active_surface("recording").is_err());
    core.detach_surface("recording").unwrap();
    assert!(core.next_surface_update("main").unwrap().is_some());
}

#[test]
fn client_core_applies_decoded_rectangles_once_and_keeps_surface_ownership_out_of_pixels() {
    let mut core = ClientFramebufferCore::default();
    core.attach_surface("main", true).unwrap();
    core.attach_surface("external", false).unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry()),
            pixels: vec![0; geometry().byte_len().unwrap()],
        }],
    })
    .unwrap();
    let initial = core.next_surface_update("main").unwrap().unwrap();
    core.mark_surface_presented("main", initial.frame_id)
        .unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect {
                x: 1,
                y: 1,
                width: 2,
                height: 1,
            },
            pixels: vec![7; 8],
        }],
    })
    .unwrap();
    assert_eq!(core.pixels()[(4 + 1) * 4..(4 + 3) * 4], [7; 8]);
    core.set_active_surface("external").unwrap();
    assert!(core
        .surfaces()
        .iter()
        .any(|surface| surface.id == "external" && surface.active));
    assert_eq!(core.pixels()[(4 + 1) * 4..(4 + 3) * 4], [7; 8]);
}

#[test]
fn client_core_versions_commits_and_gives_each_surface_its_own_dirty_cursor() {
    let mut core = ClientFramebufferCore::default();
    core.attach_surface("main", true).unwrap();
    core.attach_surface("external", false).unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry()),
            pixels: vec![1; geometry().byte_len().unwrap()],
        }],
    })
    .unwrap();

    let first = core.next_surface_update("main").unwrap().unwrap();
    assert_eq!(
        first.dirty_rectangles,
        vec![FramebufferRect::full(geometry())]
    );
    core.mark_surface_presented("main", first.frame_id).unwrap();

    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            pixels: vec![2; 4],
        }],
    })
    .unwrap();

    let main = core.next_surface_update("main").unwrap().unwrap();
    assert_eq!(
        main.dirty_rectangles,
        vec![FramebufferRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1
        }]
    );
    assert!(core.next_surface_update("external").unwrap().is_none());
    core.set_active_surface("external").unwrap();
    let external = core.next_surface_update("external").unwrap().unwrap();
    assert_eq!(
        external.dirty_rectangles,
        vec![FramebufferRect::full(geometry())]
    );
    let snapshot = core.snapshot().unwrap();
    assert_eq!(snapshot.frame_id, main.frame_id);
    assert_eq!(snapshot.pixels()[..4], [2; 4]);
}

#[test]
fn client_surface_accumulates_dirty_regions_while_a_previous_frame_is_in_flight() {
    let mut core = ClientFramebufferCore::default();
    core.attach_surface("main", true).unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry()),
            pixels: vec![0; geometry().byte_len().unwrap()],
        }],
    })
    .unwrap();
    let initial = core.next_surface_update("main").unwrap().unwrap();
    core.mark_surface_presented("main", initial.frame_id)
        .unwrap();
    let first_dirty = FramebufferRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    let second_dirty = FramebufferRect {
        x: 3,
        y: 1,
        width: 1,
        height: 1,
    };
    for rect in [first_dirty, second_dirty] {
        core.apply(DecodedFramebufferUpdate {
            geometry: geometry(),
            rectangles: vec![DecodedFramebufferRect {
                rect,
                pixels: vec![1; 4],
            }],
        })
        .unwrap();
    }
    let update = core.next_surface_update("main").unwrap().unwrap();
    assert_eq!(update.dirty_rectangles, vec![first_dirty, second_dirty]);
}

#[test]
fn surface_packet_contains_only_the_surface_dirty_rectangle_in_canvas_rgba_order() {
    let mut core = ClientFramebufferCore::default();
    core.attach_surface("main", true).unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry()),
            pixels: vec![0; geometry().byte_len().unwrap()],
        }],
    })
    .unwrap();
    let initial = core.next_surface_update("main").unwrap().unwrap();
    core.mark_surface_presented("main", initial.frame_id)
        .unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect {
                x: 1,
                y: 0,
                width: 1,
                height: 1,
            },
            pixels: vec![9, 8, 7, 255],
        }],
    })
    .unwrap();
    let update = core.next_surface_update("main").unwrap().unwrap();
    let packet = SurfaceFramePacket::from_core(&core, &update).unwrap();
    assert_eq!(&packet.bytes()[..4], b"VNCF");
    assert_eq!(
        u16::from_be_bytes(packet.bytes()[24..26].try_into().unwrap()),
        1
    );
    assert_eq!(&packet.bytes()[44..48], &[7, 8, 9, 255]);
}

#[test]
fn server_core_notifies_waiters_when_capture_publishes_requested_geometry() {
    let core = Arc::new(ServerFramebufferCore::default());
    let waiting_core = Arc::clone(&core);
    let expected_geometry = geometry();
    let waiter = thread::spawn(move || {
        waiting_core
            .wait_for_geometry(expected_geometry, Duration::from_millis(100))
            .unwrap()
    });
    thread::sleep(Duration::from_millis(5));
    core.publish(frame(3)).unwrap();
    assert!(waiter.join().unwrap());
}

#[test]
fn rfb_client_decoder_commits_raw_rectangles_as_canonical_bgra() {
    let mut core = ClientFramebufferCore::default();
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let message = [
        0, 0, 0, 1, // FramebufferUpdate: one rectangle
        0, 0, 0, 0, 0, 2, 0, 1, // x, y, width, height
        0, 0, 0, 0, // Raw encoding
        30, 20, 10, 0, 60, 50, 40, 0, // BGRX pixels from the wire
    ];

    let outcome = decoder
        .decode_framebuffer_update(&message, geometry(), &mut core)
        .unwrap();

    let snapshot = core.snapshot().unwrap();
    assert_eq!(snapshot.frame_id, outcome.frame_id.unwrap());
    assert_eq!(&snapshot.pixels()[..8], &[30, 20, 10, 255, 60, 50, 40, 255]);
}

#[test]
fn rfb_desktop_size_discards_the_old_frame_until_new_geometry_pixels_arrive() {
    let mut core = ClientFramebufferCore::default();
    core.apply(DecodedFramebufferUpdate {
        geometry: geometry(),
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry()),
            pixels: vec![1; geometry().byte_len().unwrap()],
        }],
    })
    .unwrap();
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let desktop_size = [
        0, 0, 0, 1, // FramebufferUpdate / one rectangle
        0, 0, 0, 0, 0, 6, 0, 3, // new framebuffer width/height
        255, 255, 255, 33, // DesktopSize pseudo encoding (-223)
    ];

    let outcome = decoder
        .decode_framebuffer_update(&desktop_size, geometry(), &mut core)
        .unwrap();
    assert!(outcome.geometry_changed);
    assert!(outcome.frame_id.is_none());
    assert_eq!(
        core.geometry(),
        Some(FramebufferGeometry {
            width: 6,
            height: 3,
            pixel_format: PixelFormat::Bgra8888,
        })
    );
    assert!(
        core.snapshot().is_none(),
        "old geometry pixels must never be displayed after resize"
    );
}

#[test]
fn rfb_client_decoder_rejects_unknown_or_incomplete_messages_before_frame_commit() {
    let mut core = ClientFramebufferCore::default();
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let unsupported = [0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 8];
    assert_eq!(
        decoder
            .decode_framebuffer_update(&unsupported, geometry(), &mut core)
            .unwrap_err(),
        "rfb_unsupported_framebuffer_encoding:8"
    );
    assert!(core.snapshot().is_none());

    let truncated_raw = [0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 30, 20];
    assert_eq!(
        decoder
            .decode_framebuffer_update(&truncated_raw, geometry(), &mut core)
            .unwrap_err(),
        "rfb_framebuffer_update_truncated"
    );
    assert!(core.snapshot().is_none());
}

#[test]
fn rfb_client_decoder_commits_zlib_rectangles_into_the_same_canonical_framebuffer() {
    let wire_pixels = [30_u8, 20, 10, 0];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&wire_pixels).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 6];
    message.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    message.extend_from_slice(&compressed);
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    decoder
        .decode_framebuffer_update(&message, geometry(), &mut core)
        .unwrap();
    assert_eq!(core.snapshot().unwrap().pixels()[..4], [30, 20, 10, 255]);
}

#[test]
fn rfb_client_decoder_commits_standard_bgr_cpixel_raw_zrle() {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&[0, 30, 20, 10]).unwrap(); // raw ZRLE tile: subencoding, B, G, R
    let compressed = encoder.finish().unwrap();
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 16];
    message.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    message.extend_from_slice(&compressed);
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    decoder
        .decode_framebuffer_update(&message, geometry(), &mut core)
        .unwrap();
    assert_eq!(core.snapshot().unwrap().pixels()[..4], [30, 20, 10, 255]);
}

fn decode_single_zrle(width: u16, height: u16, zrle: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(zrle).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0];
    message.extend_from_slice(&width.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message.extend_from_slice(&16_i32.to_be_bytes());
    message.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    message.extend_from_slice(&compressed);
    let geometry = FramebufferGeometry {
        width: u32::from(width),
        height: u32::from(height),
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    decoder
        .decode_framebuffer_update(&message, geometry, &mut core)
        .unwrap();
    core.snapshot().unwrap().pixels().to_vec()
}

#[test]
fn rfb_client_decoder_places_multiple_zrle_palette_tiles_in_framebuffer_rows() {
    let zrle = vec![1, 30, 20, 10, 1, 60, 50, 40]; // solid tiles carry no indices
    let pixels = decode_single_zrle(65, 1, &zrle);
    assert_eq!(&pixels[..4], &[30, 20, 10, 255]);
    assert_eq!(&pixels[64 * 4..65 * 4], &[60, 50, 40, 255]);
}

#[test]
fn rfb_client_decoder_supports_zrle_plain_rle() {
    let pixels = decode_single_zrle(3, 1, &[128, 30, 20, 10, 2]);
    assert_eq!(pixels, [30, 20, 10, 255, 30, 20, 10, 255, 30, 20, 10, 255]);
}

#[test]
fn rfb_client_decoder_supports_zrle_palette_rle() {
    let pixels = decode_single_zrle(4, 1, &[130, 30, 20, 10, 60, 50, 40, 0x80, 1, 0x81, 1]);
    assert_eq!(
        pixels,
        [30, 20, 10, 255, 30, 20, 10, 255, 60, 50, 40, 255, 60, 50, 40, 255,]
    );
}

#[test]
fn rfb_client_decoder_supports_hextile_background_and_colored_subrects() {
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 4, 0, 2];
    message.extend_from_slice(&5_i32.to_be_bytes());
    message.extend_from_slice(&[
        2 | 8 | 16, // background specified, colored subrects
        3,
        2,
        1,
        0, // BGRX background
        1, // one subrect
        30,
        20,
        10,
        0, // BGRX foreground
        0x11,
        0x10, // x=1,y=1,w=2,h=1
    ]);
    let geometry = FramebufferGeometry {
        width: 4,
        height: 2,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    decoder
        .decode_framebuffer_update(&message, geometry, &mut core)
        .unwrap();
    let snapshot = core.snapshot().unwrap();
    let pixels = snapshot.pixels();
    assert_eq!(&pixels[..4], &[3, 2, 1, 255]);
    assert_eq!(
        &pixels[(4 + 1) * 4..(4 + 3) * 4],
        &[30, 20, 10, 255, 30, 20, 10, 255]
    );
}

#[test]
fn rfb_client_decoder_supports_copyrect_from_the_canonical_framebuffer() {
    let geometry = FramebufferGeometry {
        width: 3,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut core = ClientFramebufferCore::default();
    core.apply(DecodedFramebufferUpdate {
        geometry,
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect::full(geometry),
            pixels: vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255],
        }],
    })
    .unwrap();
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut message = vec![0, 0, 0, 1, 0, 1, 0, 0, 0, 2, 0, 1];
    message.extend_from_slice(&1_i32.to_be_bytes());
    message.extend_from_slice(&[0, 0, 0, 0]);
    decoder
        .decode_framebuffer_update(&message, geometry, &mut core)
        .unwrap();
    assert_eq!(
        core.snapshot().unwrap().pixels(),
        &[1, 2, 3, 255, 1, 2, 3, 255, 4, 5, 6, 255]
    );
}

#[test]
fn rfb_client_decoder_accepts_cursor_only_updates_without_mutating_pixels() {
    let geometry = FramebufferGeometry {
        width: 2,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1];
    message.extend_from_slice(&(-239_i32).to_be_bytes());
    message.extend_from_slice(&[0; 5]); // one BGRX pixel and one mask byte
    let outcome = decoder
        .decode_framebuffer_update(&message, geometry, &mut core)
        .unwrap();
    assert_eq!(outcome.frame_id, None);
    let cursor = outcome.cursor.expect("cursor event");
    assert_eq!((cursor.width, cursor.height), (1, 1));
    assert_eq!(cursor.pixels, [0, 0, 0, 0]);
    assert!(core.snapshot().is_none());
}

#[test]
fn rfb_client_decoder_exposes_extended_desktop_topology() {
    let geometry = FramebufferGeometry {
        width: 2,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0];
    message.extend_from_slice(&32770_u16.to_be_bytes());
    message.extend_from_slice(&1_u16.to_be_bytes());
    message.extend_from_slice(&(-308_i32).to_be_bytes());
    message.extend_from_slice(&[2, 0, 0, 0]);
    for (id, x) in [(1_u32, 0_u16), (2_u32, 32768_u16)] {
        message.extend_from_slice(&id.to_be_bytes());
        message.extend_from_slice(&x.to_be_bytes());
        message.extend_from_slice(&0_i16.to_be_bytes());
        message.extend_from_slice(&2_u16.to_be_bytes());
        message.extend_from_slice(&1_u16.to_be_bytes());
        message.extend_from_slice(&0_u32.to_be_bytes());
    }
    let outcome = decoder
        .decode_framebuffer_update(&message, geometry, &mut core)
        .unwrap();
    let screens = outcome.desktop_layout.expect("desktop layout");
    assert_eq!(screens.len(), 2);
    assert_eq!((screens[1].x, screens[1].width), (32768, 2));
}

fn decode_single_tight(width: u16, height: u16, payload: &[u8]) -> Vec<u8> {
    decode_single_tight_encoding(width, height, 7, payload)
}

fn decode_single_tight_encoding(width: u16, height: u16, encoding: i32, payload: &[u8]) -> Vec<u8> {
    let geometry = FramebufferGeometry {
        width: u32::from(width),
        height: u32::from(height),
        pixel_format: PixelFormat::Bgra8888,
    };
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0];
    message.extend_from_slice(&width.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message.extend_from_slice(&encoding.to_be_bytes());
    message.extend_from_slice(payload);
    let mut decoder =
        RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap();
    let mut core = ClientFramebufferCore::default();
    decoder
        .decode_framebuffer_update(&message, geometry, &mut core)
        .unwrap();
    core.snapshot().unwrap().pixels().to_vec()
}

#[test]
fn rfb_client_decoder_supports_tight_fill_and_basic_copy() {
    assert_eq!(
        decode_single_tight(2, 1, &[0x80, 10, 20, 30]),
        [30, 20, 10, 255, 30, 20, 10, 255]
    );
    assert_eq!(
        decode_single_tight(1, 1, &[0, 10, 20, 30]),
        [30, 20, 10, 255]
    );
}

#[test]
fn rfb_client_decoder_supports_tight_palette_filter() {
    let pixels = decode_single_tight(2, 1, &[0x40, 1, 1, 1, 2, 3, 10, 20, 30, 0b0100_0000]);
    assert_eq!(pixels, [3, 2, 1, 255, 30, 20, 10, 255]);
}

#[test]
fn rfb_client_decoder_resets_and_uses_selected_tight_zlib_stream() {
    let rgb = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&rgb).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < 128);
    let mut payload = vec![0x24, compressed.len() as u8]; // reset stream 2, then basic stream 2
    payload.extend_from_slice(&compressed);
    let pixels = decode_single_tight(2, 2, &payload);
    assert_eq!(&pixels[..4], &[30, 20, 10, 255]);
    assert_eq!(&pixels[12..16], &[120, 110, 100, 255]);
}

fn tight_compact_length(length: usize) -> Vec<u8> {
    let mut bytes = vec![(length & 0x7f) as u8];
    if length > 0x7f {
        bytes[0] |= 0x80;
        bytes.push(((length >> 7) & 0x7f) as u8);
        if length > 0x3fff {
            bytes[1] |= 0x80;
            bytes.push((length >> 14) as u8);
        }
    }
    bytes
}

#[test]
fn rfb_client_decoder_supports_tight_png_and_jpeg_images() {
    use image::ImageEncoder;

    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[10, 20, 30, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    let mut png_payload = vec![0xa0];
    png_payload.extend(tight_compact_length(png.len()));
    png_payload.extend(png);
    assert_eq!(
        decode_single_tight_encoding(1, 1, -260, &png_payload),
        [30, 20, 10, 255]
    );

    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .encode(&[10, 20, 30], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut jpeg_payload = vec![0x90];
    jpeg_payload.extend(tight_compact_length(jpeg.len()));
    jpeg_payload.extend(jpeg);
    let decoded = decode_single_tight(1, 1, &jpeg_payload);
    assert_eq!(decoded[3], 255);
    assert!(
        decoded[0].abs_diff(30) <= 3
            && decoded[1].abs_diff(20) <= 3
            && decoded[2].abs_diff(10) <= 3
    );
}

#[test]
fn rfb_client_handshake_buffers_fragmented_messages_and_emits_core_ready_server_init() {
    let mut handshake = RfbClientHandshake::default();
    assert_eq!(
        handshake.consume(b"RFB 003.").unwrap(),
        Vec::<Vec<u8>>::new()
    );
    assert_eq!(
        handshake.consume(b"008\n\x01\x01").unwrap(),
        vec![b"RFB 003.008\n".to_vec(), vec![1]],
    );
    assert_eq!(handshake.consume(&[0, 0, 0, 0]).unwrap(), vec![vec![1]]);

    let mut server_init = vec![0, 4, 0, 2];
    server_init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    server_init.extend_from_slice(&[0, 0, 0, 4]);
    server_init.extend_from_slice(b"test");
    let writes = handshake.consume(&server_init).unwrap();
    assert_eq!(
        writes.len(),
        3,
        "SetPixelFormat, SetEncodings, full update request"
    );
    assert_eq!(
        handshake.take_events(),
        vec![RfbClientHandshakeEvent::Ready {
            geometry: FramebufferGeometry {
                width: 4,
                height: 2,
                pixel_format: PixelFormat::Bgra8888,
            },
            desktop_name: "test".into(),
            pixel_format: ClientRfbPixelFormat::core_bgra32(),
        }]
    );
}

#[test]
fn rfb_client_session_frames_fragmented_tcp_raw_updates_and_requests_the_next_frame() {
    let mut session = RfbClientSessionCore::default();
    session.consume(b"RFB 003.008\n\x01\x01\0\0\0\0").unwrap();
    let mut init = vec![0, 2, 0, 1];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 1, b'x']);
    session.consume(&init).unwrap();
    assert!(
        session.is_ready(),
        "ServerInit must precede command dispatch"
    );
    let update_head = [0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0, 9, 8];
    assert!(session.consume(&update_head).unwrap().is_empty());
    let writes = session.consume(&[7, 0, 6, 5, 4, 0]).unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0][..2], [3, 1], "next update must be incremental");
    assert_eq!(
        session
            .with_framebuffer(|framebuffer| framebuffer.snapshot().unwrap().pixels()[..8].to_vec())
            .unwrap(),
        [9, 8, 7, 255, 6, 5, 4, 255]
    );
    assert!(
        session.is_presentable(),
        "a committed first frame enables native presentation"
    );
}

#[test]
fn rfb_client_session_switches_to_continuous_updates_after_server_support_marker() {
    let mut session = RfbClientSessionCore::default();
    session.consume(b"RFB 003.008\n\x01\x01\0\0\0\0").unwrap();
    let mut init = vec![0, 2, 0, 1];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 1, b'x']);
    session.consume(&init).unwrap();

    let mut support = vec![0, 0, 0, 1];
    support.extend_from_slice(&[0; 8]);
    support.extend_from_slice(&(-313_i32).to_be_bytes());
    let writes = session.consume(&support).unwrap();
    assert_eq!(writes, vec![vec![150, 1, 0, 0, 0, 0, 0, 2, 0, 1]]);
    assert!(session.consume(&support).unwrap().is_empty());

    let update = [
        0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0, 9, 8, 7, 0, 6, 5, 4, 0,
    ];
    assert!(
        session.consume(&update).unwrap().is_empty(),
        "continuous mode must not queue a request per frame"
    );
}

#[test]
fn rfb_client_session_negotiates_extended_clipboard_caps() {
    let mut session = RfbClientSessionCore::default();
    session.consume(b"RFB 003.008\n\x01\x01\0\0\0\0").unwrap();
    let mut init = vec![0, 2, 0, 1];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 1, b'x']);
    session.consume(&init).unwrap();

    let flags = (1_u32 << 24) | (1_u32 << 25) | (1_u32 << 27) | (1_u32 << 28) | 1;
    let mut caps = vec![3, 0, 0, 0];
    caps.extend_from_slice(&(-8_i32).to_be_bytes());
    caps.extend_from_slice(&flags.to_be_bytes());
    caps.extend_from_slice(&1_048_576_u32.to_be_bytes());
    let writes = session.consume(&caps).unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0][0], 6);
    assert!(i32::from_be_bytes(writes[0][4..8].try_into().unwrap()) < 0);
}

#[test]
fn rfb_client_session_handles_fragmented_vwsd_fence_discovery() {
    let mut session = RfbClientSessionCore::default();
    session.consume(b"RFB 003.008\n\x01\x01\0\0\0\0").unwrap();
    let mut init = vec![0, 2, 0, 1];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 1, b'x']);
    session.consume(&init).unwrap();
    let _ = session.take_events();

    let hello = [b'V', b'W', b'S', b'D', 1, 1, 9, 0, 0, 1, b'{', b'}'];
    let mut hello_message = vec![248, 0, 0, 0];
    hello_message.extend_from_slice(&(1_u32 << 31).to_be_bytes());
    hello_message.push(hello.len() as u8);
    hello_message.extend_from_slice(&hello);
    let writes = session.consume(&hello_message).unwrap();
    assert_eq!(writes.len(), 2);
    let mut acknowledgement = hello_message.clone();
    acknowledgement[4..8].copy_from_slice(&0_u32.to_be_bytes());
    assert_eq!(
        writes[0], acknowledgement,
        "Discovery must still acknowledge the standard Fence"
    );
    assert_eq!(writes[1][0], 248);
    assert_eq!(
        u32::from_be_bytes(writes[1][4..8].try_into().unwrap()),
        1_u32 << 31
    );
    assert_eq!(&writes[1][9..13], b"VWSD");
    assert_eq!(writes[1][14], 2, "native client responds with Discover");

    let manifest = serde_json::json!({ "version": "1.0", "server": { "id": "peer" }, "padding": "x".repeat(128) });
    for payload in encode_discovery_frames(DiscoveryFrameType::Manifest, 9, &manifest).unwrap() {
        let mut message = vec![248, 0, 0, 0];
        message.extend_from_slice(&(1_u32 << 31).to_be_bytes());
        message.push(payload.len() as u8);
        message.extend_from_slice(&payload);
        session.consume(&message).unwrap();
    }
    assert_eq!(
        session.take_events(),
        vec![RfbClientSessionEvent::DiscoveryManifest(manifest)]
    );
}

#[test]
fn rfb_runtime_status_preserves_fence_ack_and_reports_restriction_recovery_and_action_result() {
    use LLRemoteLib::remote_desktop_core::{
        DesktopAccessState, SecureAttentionResult, ServerRuntimeMessage,
    };
    let mut session = RfbClientSessionCore::default();
    session.consume(b"RFB 003.008\n\x01\x01\0\0\0\0").unwrap();
    let mut init = vec![0, 2, 0, 1];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 1, b'x']);
    session.consume(&init).unwrap();
    session.take_events();
    for notice in [
        ServerRuntimeMessage::Desktop {
            desktop: DesktopAccessState::Unavailable,
            secure_attention_available: false,
        },
        ServerRuntimeMessage::Desktop {
            desktop: DesktopAccessState::Available,
            secure_attention_available: false,
        },
        ServerRuntimeMessage::SecureAttentionResponse {
            request_id: 17,
            result: SecureAttentionResult::Unsupported,
        },
    ] {
        let mut wire = vec![248, 0, 0, 0, 128, 0, 0, 0, 12];
        wire.extend(notice.encode());
        assert!(session.consume(&wire[..11]).unwrap().is_empty());
        let writes = session.consume(&wire[11..]).unwrap();
        let mut ack = wire.clone();
        ack[4..8].fill(0);
        assert_eq!(writes, vec![ack]);
        assert_eq!(
            session.take_events(),
            vec![RfbClientSessionEvent::ServerRuntime(notice)]
        );
    }
    let malformed = b"VWRS\x01\x01\xff\0\0\0\0\0";
    assert_eq!(ServerRuntimeMessage::decode(malformed), None);
}

#[test]
fn rfb_client_handshake_enables_fence_and_discovers_the_server() {
    use LLRemoteLib::server_discovery::{
        handle_discover_request, write_hello, ServerDiscoveryContext, FENCE_PSEUDO_ENCODING,
    };

    let mut session = RfbClientSessionCore::default();
    session.consume(b"RFB 003.008\n\x01\x01\0\0\0\0").unwrap();
    let mut init = vec![0, 2, 0, 1];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 1, b'x']);
    let writes = session.consume(&init).unwrap();
    let encodings = writes.iter().find(|message| message[0] == 2).unwrap();
    assert_eq!(
        encodings.len(),
        4 + usize::from(u16::from_be_bytes(encodings[2..4].try_into().unwrap())) * 4,
    );
    assert!(
        encodings[4..]
            .as_chunks::<4>()
            .0
            .iter()
            .any(|bytes| i32::from_be_bytes(*bytes) == FENCE_PSEUDO_ENCODING),
        "the server sends Discovery Hello only after the client advertises Fence",
    );
    session.take_events();

    let mut hello = Vec::new();
    write_hello(&mut hello).unwrap();
    let writes = session.consume(&hello).unwrap();
    let discover = writes.iter().find(|message| message[14] == 2).unwrap();
    let context = ServerDiscoveryContext::new("discovered-peer".into(), false, None, None);
    let mut manifest = Vec::new();
    handle_discover_request(&mut manifest, &context, &discover[9..]).unwrap();
    session.consume(&manifest).unwrap();
    assert!(matches!(
        session.take_events().as_slice(),
        [RfbClientSessionEvent::DiscoveryManifest(manifest)]
            if manifest["server"]["id"] == "discovered-peer"
                && manifest["server"]["identityVerification"] == "server-discovery"
    ));
}

#[test]
fn rfb_client_handshake_uses_the_standard_vnc_password_challenge_response() {
    let mut handshake = RfbClientHandshake::with_password(Some("secret".into()));
    let writes = handshake.consume(b"RFB 003.008\n\x01\x02").unwrap();
    assert_eq!(writes, vec![b"RFB 003.008\n".to_vec(), vec![2]]);
    let challenge = [7_u8; 16];
    let writes = handshake.consume(&challenge).unwrap();
    assert_eq!(
        writes[0],
        LLRemoteLib::local_vnc_server::create_vnc_auth_response("secret", &challenge)
    );
    assert_eq!(handshake.consume(&[0, 0, 0, 0]).unwrap(), vec![vec![1]]);
}

#[test]
fn rfb_33_client_handshake_supports_vnc_authentication_without_security_selection() {
    let mut handshake = RfbClientHandshake::with_password(Some("secret".into()));
    assert_eq!(
        handshake.consume(b"RFB 003.003\n").unwrap(),
        vec![b"RFB 003.003\n".to_vec()]
    );
    assert!(handshake.consume(&2_u32.to_be_bytes()).unwrap().is_empty());

    let challenge = [0x5a; 16];
    let response = handshake.consume(&challenge).unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0].len(), 16);
    assert_ne!(response[0], challenge);

    assert_eq!(
        handshake.consume(&0_u32.to_be_bytes()).unwrap(),
        vec![vec![1]]
    );
}

#[test]
fn rfb_37_none_security_skips_security_result_and_sends_client_init() {
    let mut handshake = RfbClientHandshake::default();
    assert_eq!(
        handshake.consume(b"RFB 003.007\n").unwrap(),
        vec![b"RFB 003.007\n".to_vec()]
    );
    assert_eq!(
        handshake.consume(&[1, 1]).unwrap(),
        vec![vec![1], vec![1]],
        "RFB 3.7 sends security selection then ClientInit without waiting for SecurityResult"
    );
}
