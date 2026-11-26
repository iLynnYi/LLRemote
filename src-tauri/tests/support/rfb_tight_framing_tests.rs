#[test]
fn tight_framing_uses_four_byte_tpixel_for_non_rgb24_formats_at_every_split() {
    let format = crate::remote_desktop_core::ClientRfbPixelFormat {
        depth: 32,
        ..crate::remote_desktop_core::ClientRfbPixelFormat::core_bgra32()
    };
    for (width, encoding, payload) in [
        (1_u16, 7_i32, vec![0, 0x56, 0x34, 0x12, 0]),
        (2, -260, vec![0x80, 0x56, 0x34, 0x12, 0]),
        (
            2,
            7,
            vec![0x40, 1, 1, 0x56, 0x34, 0x12, 0, 0, 0, 0, 0, 0x40],
        ),
    ] {
        let mut wire = vec![0, 0, 0, 1, 0, 0, 0, 0];
        wire.extend(width.to_be_bytes());
        wire.extend(1_u16.to_be_bytes());
        wire.extend(encoding.to_be_bytes());
        wire.extend(payload);
        for split in 0..wire.len() {
            let mut framer = RfbServerMessageFramer::new(format);
            assert_eq!(framer.next_message_length(&wire[..split]).unwrap(), None);
            assert_eq!(framer.next_message_length(&wire).unwrap(), Some(wire.len()));
        }
    }
}
