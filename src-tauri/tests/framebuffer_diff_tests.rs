use LLRemoteLib::remote_desktop_core::{
    diff_dirty_rectangles, Framebuffer, FramebufferGeometry, FramebufferRect, PixelFormat,
};

fn frame(width: u32, height: u32, pixels: Vec<u8>) -> Framebuffer {
    Framebuffer::new(
        FramebufferGeometry {
            width,
            height,
            pixel_format: PixelFormat::Bgra8888,
        },
        pixels,
    )
    .unwrap()
}

#[test]
fn dirty_rectangles_return_empty_when_framebuffer_is_unchanged() {
    let current = vec![7_u8; 8 * 8 * 4];

    assert!(diff_dirty_rectangles(
        Some(&frame(8, 8, current.clone())),
        &frame(8, 8, current),
        4
    )
    .is_empty());
}

#[test]
fn dirty_rectangles_cover_only_changed_tiles() {
    let previous = vec![0_u8; 8 * 8 * 4];
    let mut current = previous.clone();
    let changed_pixel = ((6 * 8) + 6) * 4;
    current[changed_pixel] = 255;

    assert_eq!(
        diff_dirty_rectangles(Some(&frame(8, 8, previous)), &frame(8, 8, current), 4),
        vec![FramebufferRect {
            x: 4,
            y: 4,
            width: 4,
            height: 4,
        }],
    );
}

#[test]
fn dirty_rectangles_merge_adjacent_changed_tiles_into_row_bands() {
    let previous = vec![0_u8; 16 * 8 * 4];
    let current = vec![255_u8; 16 * 8 * 4];

    assert_eq!(
        diff_dirty_rectangles(Some(&frame(16, 8, previous)), &frame(16, 8, current), 4),
        vec![
            FramebufferRect {
                x: 0,
                y: 0,
                width: 16,
                height: 4,
            },
            FramebufferRect {
                x: 0,
                y: 4,
                width: 16,
                height: 4,
            },
        ],
    );
}

#[test]
fn canonical_framebuffer_rejects_short_pixels_before_a_client_update_can_exist() {
    let error = Framebuffer::new(
        FramebufferGeometry {
            width: 2,
            height: 1,
            pixel_format: PixelFormat::Bgra8888,
        },
        vec![0; 4],
    )
    .unwrap_err();
    assert_eq!(error, "framebuffer_length_mismatch");
}
