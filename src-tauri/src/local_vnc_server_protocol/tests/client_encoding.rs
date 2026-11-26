#[test]
fn client_encoding_support_prefers_zlib_over_raw_for_bounded_rect_payloads() {
    let encodings = [
        1_i32.to_be_bytes(),
        ZLIB_ENCODING.to_be_bytes(),
        RAW_ENCODING.to_be_bytes(),
        DESKTOP_SIZE_PSEUDO_ENCODING.to_be_bytes(),
        (-224_i32).to_be_bytes(),
        (-308_i32).to_be_bytes(),
        (-307_i32).to_be_bytes(),
        EXTENDED_CLIPBOARD_PSEUDO_ENCODING.to_be_bytes(),
    ]
    .concat();

    let support = parse_client_encoding_support(&encodings);

    assert_eq!(support.framebuffer_encoding, RfbFramebufferEncoding::Zlib);
    assert!(support.desktop_size);
}

#[test]
fn client_encoding_support_prefers_zrle_for_local_app_bounded_rect_payloads() {
    let encodings = [
        ZRLE_ENCODING.to_be_bytes(),
        RAW_ENCODING.to_be_bytes(),
        EXTENDED_CLIPBOARD_PSEUDO_ENCODING.to_be_bytes(),
    ]
    .concat();

    let support = parse_client_encoding_support(&encodings);

    assert_eq!(support.framebuffer_encoding, RfbFramebufferEncoding::Zrle);
    assert!(support.extended_clipboard);
}

#[test]
fn client_encoding_support_prefers_zrle_for_native_core_profile() {
    let encodings = [
        1_i32.to_be_bytes(),
        ZRLE_ENCODING.to_be_bytes(),
        ZLIB_ENCODING.to_be_bytes(),
        HEXTILE_ENCODING.to_be_bytes(),
        RAW_ENCODING.to_be_bytes(),
        DESKTOP_SIZE_PSEUDO_ENCODING.to_be_bytes(),
        CONTINUOUS_UPDATES_PSEUDO_ENCODING.to_be_bytes(),
    ]
    .concat();

    let support = parse_client_encoding_support(&encodings);

    assert_eq!(support.framebuffer_encoding, RfbFramebufferEncoding::Zrle);
    assert!(support.desktop_size);
    assert!(support.continuous_updates);
}

#[test]
fn framebuffer_update_region_preserves_client_requested_bounds() {
    let payload = [
        1, // incremental
        0, 10, // x
        0, 20, // y
        2, 0, // width = 512
        1, 0, // height = 256
    ];

    assert_eq!(
        parse_update_region(&payload),
        crate::remote_desktop_core::FramebufferRect {
            x: 10,
            y: 20,
            width: 512,
            height: 256,
        }
    );
}
