use LLRemoteLib::remote_desktop_core::{
    ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
    RfbClientFramebufferDecoder,
};

fn geometry(width: u32, height: u32) -> FramebufferGeometry {
    FramebufferGeometry {
        width,
        height,
        pixel_format: PixelFormat::Bgra8888,
    }
}
fn decoder() -> RfbClientFramebufferDecoder {
    RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32()).unwrap()
}

#[test]
fn zlib_excess_output_is_rejected_without_publishing_a_partial_rectangle() {
    use std::io::Write;
    for extra in 1..=64 {
        let mut decoder = decoder();
        let mut core = ClientFramebufferCore::default();
        decoder
            .decode_framebuffer_update(
                &message(&[raw(0, 0, 2, 1, &[1, 2])]),
                geometry(2, 1),
                &mut core,
            )
            .unwrap();
        let previous = core.pixels().to_vec();
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&vec![7; 8 + extra]).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut payload = (compressed.len() as u32).to_be_bytes().to_vec();
        payload.extend(compressed);
        let wire = message(&[raw(0, 0, 1, 1, &[42]), rect(0, 0, 2, 1, 6, &payload)]);
        assert_eq!(
            decoder
                .decode_framebuffer_update(&wire, geometry(2, 1), &mut core)
                .unwrap_err(),
            "rfb_zlib_decoded_length_mismatch",
            "excess={extra}"
        );
        assert_eq!(core.pixels(), previous);
        assert_eq!(core.latest_frame_id(), Some(1));
    }
}

#[test]
fn zlib_persistent_rectangles_keep_exact_pixels_across_output_sizes() {
    use std::io::Write;
    let mut decoder = decoder();
    let mut core = ClientFramebufferCore::default();
    let geometry = geometry(65, 65);
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut offset = 0;
    let mut random = 0x12345678_u32;
    for (index, (width, height)) in [(2_u16, 1_u16), (65, 65), (17, 3), (65, 65)]
        .into_iter()
        .enumerate()
    {
        let mut source = vec![255; width as usize * height as usize * 4];
        for pixel in source.as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                *channel = random as u8;
            }
        }
        // Generic persistent zlib, independent of our RFB Server writer.
        encoder.write_all(&source).unwrap();
        encoder.flush().unwrap();
        let compressed = &encoder.get_ref()[offset..];
        let mut payload = (compressed.len() as u32).to_be_bytes().to_vec();
        payload.extend(compressed);
        offset = encoder.get_ref().len();
        decoder
            .decode_framebuffer_update(
                &message(&[rect(0, 0, width, height, 6, &payload)]),
                geometry,
                &mut core,
            )
            .unwrap();
        for row in 0..height as usize {
            let length = width as usize * 4;
            let start = row * geometry.width as usize * 4;
            assert_eq!(
                &core.pixels()[start..start + length],
                &source[row * length..(row + 1) * length]
            );
        }
        assert_eq!(core.latest_frame_id(), Some(index as u64 + 1));
    }
}
fn rect(x: u16, y: u16, w: u16, h: u16, encoding: i32, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for n in [x, y, w, h] {
        out.extend(n.to_be_bytes());
    }
    out.extend(encoding.to_be_bytes());
    out.extend(bytes);
    out
}
fn raw(x: u16, y: u16, w: u16, h: u16, values: &[u8]) -> Vec<u8> {
    rect(
        x,
        y,
        w,
        h,
        0,
        &values
            .iter()
            .flat_map(|v| [*v, 0, 0, 0])
            .collect::<Vec<_>>(),
    )
}
fn copy(x: u16, y: u16, w: u16, h: u16, sx: u16, sy: u16) -> Vec<u8> {
    rect(
        x,
        y,
        w,
        h,
        1,
        &[sx.to_be_bytes(), sy.to_be_bytes()].concat(),
    )
}
fn message(rectangles: &[Vec<u8>]) -> Vec<u8> {
    let mut wire = vec![0, 0];
    wire.extend((rectangles.len() as u16).to_be_bytes());
    for r in rectangles {
        wire.extend(r);
    }
    wire
}
fn values(core: &ClientFramebufferCore) -> Vec<u8> {
    core.pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0])
        .collect()
}

#[test]
fn raw_and_chained_copyrects_use_previous_rectangles_in_the_same_update() {
    let mut d = decoder();
    let mut core = ClientFramebufferCore::default();
    let g = geometry(3, 1);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 3, 1, &[1, 2, 3])]), g, &mut core)
        .unwrap();
    let id = core.latest_frame_id().unwrap();
    let outcome = d
        .decode_framebuffer_update(
            &message(&[
                raw(0, 0, 1, 1, &[9]),
                copy(1, 0, 1, 1, 0, 0),
                copy(2, 0, 1, 1, 1, 0),
            ]),
            g,
            &mut core,
        )
        .unwrap();
    assert_eq!(values(&core), [9, 9, 9]);
    assert_eq!(outcome.frame_id, Some(id + 1));
}

#[test]
fn first_update_can_copy_its_own_raw_pixels_without_an_older_published_frame() {
    let mut core = ClientFramebufferCore::default();
    decoder()
        .decode_framebuffer_update(
            &message(&[raw(0, 0, 1, 1, &[7]), copy(1, 0, 1, 1, 0, 0)]),
            geometry(2, 1),
            &mut core,
        )
        .unwrap();
    assert_eq!(values(&core), [7, 7]);
}

#[test]
fn overlapping_copyrect_preserves_source_rows_before_any_destination_write() {
    let mut core = ClientFramebufferCore::default();
    let mut d = decoder();
    let g = geometry(3, 3);
    d.decode_framebuffer_update(
        &message(&[raw(0, 0, 3, 3, &[1, 2, 3, 4, 5, 6, 7, 8, 9])]),
        g,
        &mut core,
    )
    .unwrap();
    d.decode_framebuffer_update(
        &message(&[raw(0, 0, 1, 1, &[10]), copy(1, 1, 2, 2, 0, 0)]),
        g,
        &mut core,
    )
    .unwrap();
    assert_eq!(values(&core), [10, 2, 3, 4, 10, 2, 7, 4, 5]);
}

#[test]
fn copyrect_all_overlap_directions_match_an_independent_source_snapshot() {
    let source: Vec<u8> = (1..=12).collect();
    for (x, y, w, h, sx, sy) in [
        (1, 0, 3, 3, 0, 0),
        (0, 0, 3, 3, 1, 0),
        (0, 1, 4, 2, 0, 0),
        (0, 0, 4, 2, 0, 1),
        (1, 1, 3, 2, 0, 0),
        (0, 0, 3, 2, 1, 1),
        (1, 0, 3, 2, 0, 1),
        (0, 1, 3, 2, 1, 0),
        (0, 0, 4, 3, 0, 0),
    ] {
        let mut core = ClientFramebufferCore::default();
        let mut d = decoder();
        let g = geometry(4, 3);
        d.decode_framebuffer_update(&message(&[raw(0, 0, 4, 3, &source)]), g, &mut core)
            .unwrap();
        let mut expected = source.clone();
        for row in 0..h as usize {
            for column in 0..w as usize {
                expected[(y as usize + row) * 4 + x as usize + column] =
                    source[(sy as usize + row) * 4 + sx as usize + column];
            }
        }
        d.decode_framebuffer_update(&message(&[copy(x, y, w, h, sx, sy)]), g, &mut core)
            .unwrap();
        assert_eq!(values(&core), expected);
        assert_eq!(core.latest_frame_id(), Some(2));
        let previous = core.pixels().to_vec();
        assert!(d
            .decode_framebuffer_update(
                &message(&[copy(x, y, w, h, sx, sy), rect(0, 0, 1, 1, 987, &[])]),
                g,
                &mut core
            )
            .is_err());
        assert_eq!(core.pixels(), previous);
        assert_eq!(core.latest_frame_id(), Some(2));
        expected[11] = 99;
        d.decode_framebuffer_update(&message(&[raw(3, 2, 1, 1, &[99])]), g, &mut core)
            .unwrap();
        assert_eq!(values(&core), expected);
        assert_eq!(core.latest_frame_id(), Some(3));
    }
}

#[test]
fn a_late_error_rolls_back_pixels_and_reused_working_buffer_is_repaired() {
    let mut core = ClientFramebufferCore::default();
    let mut d = decoder();
    let g = geometry(3, 1);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 3, 1, &[1, 2, 3])]), g, &mut core)
        .unwrap();
    let original = core.pixels().to_vec();
    let id = core.latest_frame_id();
    for invalid in [rect(2, 0, 1, 1, 0, &[]), rect(0, 0, 1, 1, 987, &[])] {
        assert!(d
            .decode_framebuffer_update(&message(&[raw(0, 0, 1, 1, &[9]), invalid]), g, &mut core)
            .is_err());
        assert_eq!(core.pixels(), original);
        assert_eq!(core.latest_frame_id(), id);
    }
    let mut trailing = message(&[raw(0, 0, 1, 1, &[8])]);
    trailing.push(0);
    assert!(d
        .decode_framebuffer_update(&trailing, g, &mut core)
        .is_err());
    assert_eq!(core.pixels(), original);
    d.decode_framebuffer_update(&message(&[raw(2, 0, 1, 1, &[6])]), g, &mut core)
        .unwrap();
    assert_eq!(values(&core), [1, 2, 6]);
    d.decode_framebuffer_update(&message(&[raw(1, 0, 1, 1, &[5])]), g, &mut core)
        .unwrap();
    assert_eq!(values(&core), [1, 5, 6]);
}

#[test]
fn desktop_size_starts_a_new_epoch_and_cannot_copy_old_pixels() {
    let mut core = ClientFramebufferCore::default();
    let mut d = decoder();
    let old = geometry(3, 1);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 3, 1, &[1, 2, 3])]), old, &mut core)
        .unwrap();
    let id = core.latest_frame_id();
    let resize = rect(0, 0, 2, 1, -223, &[]);
    assert!(d
        .decode_framebuffer_update(
            &message(&[resize.clone(), copy(1, 0, 1, 1, 0, 0)]),
            old,
            &mut core
        )
        .is_err());
    assert_eq!(core.geometry(), Some(old));
    assert_eq!(core.latest_frame_id(), id);
    assert_eq!(values(&core), [1, 2, 3]);
    d.decode_framebuffer_update(
        &message(&[
            raw(2, 0, 1, 1, &[9]),
            resize,
            raw(0, 0, 1, 1, &[7]),
            copy(1, 0, 1, 1, 0, 0),
        ]),
        old,
        &mut core,
    )
    .unwrap();
    assert_eq!(core.geometry(), Some(geometry(2, 1)));
    assert_eq!(values(&core), [7, 7]);
}

#[test]
fn repeated_partial_updates_preserve_every_untouched_pixel() {
    let mut core = ClientFramebufferCore::default();
    let mut d = decoder();
    let g = geometry(8, 1);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 8, 1, &[1; 8])]), g, &mut core)
        .unwrap();
    let mut expected = vec![1; 8];
    let mut allocations = std::collections::BTreeSet::new();
    allocations.insert(core.pixels().as_ptr() as usize);
    for n in 2..100u8 {
        let x = n as usize % 8;
        d.decode_framebuffer_update(&message(&[raw(x as u16, 0, 1, 1, &[n])]), g, &mut core)
            .unwrap();
        expected[x] = n;
        assert_eq!(values(&core), expected);
        allocations.insert(core.pixels().as_ptr() as usize);
    }
    assert_eq!(
        allocations.len(),
        2,
        "steady updates reuse the same two frame buffers"
    );
}

#[test]
fn external_core_updates_are_used_as_the_next_transaction_baseline() {
    use LLRemoteLib::remote_desktop_core::{
        DecodedFramebufferRect, DecodedFramebufferUpdate, FramebufferRect,
    };
    let mut core = ClientFramebufferCore::default();
    let mut d = decoder();
    let g = geometry(3, 1);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 3, 1, &[1, 2, 3])]), g, &mut core)
        .unwrap();
    core.apply(DecodedFramebufferUpdate {
        geometry: g,
        rectangles: vec![DecodedFramebufferRect {
            rect: FramebufferRect {
                x: 1,
                y: 0,
                width: 1,
                height: 1,
            },
            pixels: vec![8, 0, 0, 255],
        }],
    })
    .unwrap();
    d.decode_framebuffer_update(&message(&[copy(2, 0, 1, 1, 1, 0)]), g, &mut core)
        .unwrap();
    assert_eq!(values(&core), [1, 8, 8]);
}

#[test]
fn same_size_desktop_reset_does_not_restore_old_pixels_from_the_back_buffer() {
    let mut core = ClientFramebufferCore::default();
    let mut d = decoder();
    let g = geometry(3, 1);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 3, 1, &[1, 2, 3])]), g, &mut core)
        .unwrap();
    // Warm the second allocation, then reset the desktop without changing dimensions.
    d.decode_framebuffer_update(&message(&[raw(0, 0, 1, 1, &[4])]), g, &mut core)
        .unwrap();
    d.decode_framebuffer_update(
        &message(&[rect(0, 0, 3, 1, -223, &[]), raw(0, 0, 1, 1, &[7])]),
        g,
        &mut core,
    )
    .unwrap();
    d.decode_framebuffer_update(&message(&[raw(1, 0, 1, 1, &[8])]), g, &mut core)
        .unwrap();
    assert_eq!(values(&core), [7, 8, 0]);
    d.decode_framebuffer_update(&message(&[rect(0, 0, 3, 1, -223, &[])]), g, &mut core)
        .unwrap();
    assert_eq!(core.latest_frame_id(), None);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 1, 1, &[9])]), g, &mut core)
        .unwrap();
    assert_eq!(values(&core), [9, 0, 0]);
}

#[test]
fn extended_resize_is_atomic_and_repairs_the_old_baseline_after_a_late_error() {
    let mut core = ClientFramebufferCore::default();
    let mut d = decoder();
    let g = geometry(3, 1);
    d.decode_framebuffer_update(&message(&[raw(0, 0, 3, 1, &[1, 2, 3])]), g, &mut core)
        .unwrap();
    let id = core.latest_frame_id();
    // A malformed update must not publish a resize before a later pixel error.
    let layout = rect(
        0,
        0,
        2,
        1,
        -308,
        &[
            1, 0, 0, 0, // one screen
            0, 0, 0, 1, // id
            0, 0, 0, 0, // origin
            0, 2, 0, 1, // dimensions
            0, 0, 0, 0, // flags
        ],
    );
    assert!(d
        .decode_framebuffer_update(&message(&[layout, raw(2, 0, 1, 1, &[9])]), g, &mut core,)
        .is_err());
    assert_eq!(core.geometry(), Some(g));
    assert_eq!(core.latest_frame_id(), id);
    assert_eq!(values(&core), [1, 2, 3]);
    d.decode_framebuffer_update(&message(&[raw(1, 0, 1, 1, &[8])]), g, &mut core)
        .unwrap();
    assert_eq!(values(&core), [1, 8, 3]);
}
