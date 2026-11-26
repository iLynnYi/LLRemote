use super::*;

#[test]
fn incomplete_capture_rows_are_rejected_instead_of_published_as_black_pixels() {
    let target = DisplaySize {
        width: 2,
        height: 2,
    };
    for (source, width, height, stride) in [
        (vec![10; 12], 2, 2, 8),
        (vec![10; 16], 2, 2, 4),
        (vec![10; 16], 0, 2, 8),
        (vec![10; 16], usize::MAX, 2, 8),
    ] {
        assert_eq!(
            convert_captured_framebuffer(
                &source,
                width,
                height,
                stride,
                target,
                CapturedPixelFormat::Bgra8888
            ),
            Err(CaptureFailure::InvalidFrame)
        );
    }
    // Stride padding does not become pixels and the last row needs no trailing padding.
    let source = [
        10, 20, 30, 0, 40, 50, 60, 0, 255, 255, 255, 255, 70, 80, 90, 0, 100, 110, 120, 0,
    ];
    let expected = [
        10, 20, 30, 0, 40, 50, 60, 0, 70, 80, 90, 0, 100, 110, 120, 0,
    ];
    assert_eq!(
        convert_captured_framebuffer(&source, 2, 2, 12, target, CapturedPixelFormat::Bgra8888)
            .unwrap(),
        expected
    );
    for target in [
        DisplaySize {
            width: 0,
            height: 2,
        },
        DisplaySize {
            width: 65536,
            height: 1,
        },
        DisplaySize {
            width: 65535,
            height: 65535,
        },
    ] {
        assert_eq!(
            convert_captured_framebuffer(&source, 2, 2, 12, target, CapturedPixelFormat::Bgra8888),
            Err(CaptureFailure::InvalidFrame)
        );
    }
    assert!(capture_buffer_length(3840, 2160).is_ok());
    assert!(capture_buffer_length(7680, 2160).is_ok());
}
