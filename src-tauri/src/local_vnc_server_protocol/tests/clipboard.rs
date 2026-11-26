#[test]
fn legacy_cut_text_decodes_valid_utf8_chinese_by_byte_payload() {
    let decoded = decode_legacy_cut_text("中文剪贴板".as_bytes());

    assert_eq!(decoded.text, "中文剪贴板");
    assert_eq!(decoded.encoding, "utf-8");
}

#[test]
fn legacy_cut_text_falls_back_to_latin1_for_non_utf8_payload() {
    let decoded = decode_legacy_cut_text(&[0x48, 0xe9, 0x21]);

    assert_eq!(decoded.text, "Hé!");
    assert_eq!(decoded.encoding, "latin-1");
}

#[test]
fn server_cut_text_extended_payload_uses_negative_byte_length() {
    let mut wire = Vec::new();

    write_extended_clipboard_request(&mut wire).expect("write request");

    assert_eq!(wire[0], 3);
    assert_eq!(&wire[1..4], &[0, 0, 0]);
    assert_eq!(i32::from_be_bytes([wire[4], wire[5], wire[6], wire[7]]), -4);
    assert_eq!(
        u32::from_be_bytes([wire[8], wire[9], wire[10], wire[11]]),
        EXTENDED_CLIPBOARD_ACTION_REQUEST | EXTENDED_CLIPBOARD_FORMAT_TEXT,
    );
}

#[test]
fn server_cut_text_extended_notify_advertises_text_change() {
    let mut wire = Vec::new();

    write_extended_clipboard_notify(&mut wire).expect("write notify");

    assert_eq!(wire[0], 3);
    assert_eq!(&wire[1..4], &[0, 0, 0]);
    assert_eq!(i32::from_be_bytes([wire[4], wire[5], wire[6], wire[7]]), -4);
    assert_eq!(
        u32::from_be_bytes([wire[8], wire[9], wire[10], wire[11]]),
        EXTENDED_CLIPBOARD_ACTION_NOTIFY | EXTENDED_CLIPBOARD_FORMAT_TEXT,
    );
}

#[test]
fn server_cut_text_extended_provide_encodes_utf8_text() {
    let mut wire = Vec::new();

    write_extended_clipboard_provide(&mut wire, "Server 中文剪贴板").expect("write provide");

    assert_eq!(wire[0], 3);
    assert_eq!(&wire[1..4], &[0, 0, 0]);
    let signed_length = i32::from_be_bytes([wire[4], wire[5], wire[6], wire[7]]);
    assert!(signed_length < 0);
    let decoded = decode_extended_clipboard_provide(&wire[8..])
        .expect("decode provide")
        .expect("text payload");
    assert_eq!(decoded, "Server 中文剪贴板");
}

#[test]
fn client_extended_clipboard_caps_mark_publish_ready() {
    let mut wire = Vec::new();
    let mut publish_state = ClipboardPublishState::default();
    let flags = EXTENDED_CLIPBOARD_ACTION_CAPS
        | EXTENDED_CLIPBOARD_ACTION_REQUEST
        | EXTENDED_CLIPBOARD_ACTION_NOTIFY
        | EXTENDED_CLIPBOARD_ACTION_PROVIDE
        | EXTENDED_CLIPBOARD_FORMAT_TEXT;

    let outcome = handle_client_cut_text(
        &mut wire,
        -4,
        &flags.to_be_bytes(),
        false,
        true,
        &mut publish_state,
    )
    .expect("handle client caps");

    assert!(outcome.received_extended_clipboard_caps);
    assert!(wire.is_empty());
}

#[test]
fn client_extended_clipboard_messages_before_negotiation_do_not_mark_publish_ready() {
    let mut wire = Vec::new();
    let mut publish_state = ClipboardPublishState::default();
    let flags = EXTENDED_CLIPBOARD_ACTION_CAPS | EXTENDED_CLIPBOARD_FORMAT_TEXT;

    let outcome = handle_client_cut_text(
        &mut wire,
        -4,
        &flags.to_be_bytes(),
        false,
        false,
        &mut publish_state,
    )
    .expect("handle early client caps");

    assert!(!outcome.received_extended_clipboard_caps);
    assert!(wire.is_empty());
}

#[test]
fn view_only_client_cut_text_does_not_update_publish_state() {
    let mut wire = Vec::new();
    let mut publish_state = ClipboardPublishState::default();

    handle_client_cut_text(
        &mut wire,
        "blocked".len() as i32,
        b"blocked",
        true,
        true,
        &mut publish_state,
    )
    .expect("handle view-only legacy cut text");

    let payload = build_extended_clipboard_provide_payload("blocked");
    handle_client_cut_text(
        &mut wire,
        -(payload.len() as i32),
        &payload,
        true,
        true,
        &mut publish_state,
    )
    .expect("handle view-only extended cut text");

    assert_eq!(
        publish_state.next_changed_text(Some("blocked".to_string())),
        Some("blocked".to_string())
    );
    assert!(wire.is_empty());
}

#[test]
fn clipboard_publish_state_only_publishes_changed_text() {
    let mut state = ClipboardPublishState::default();

    assert_eq!(
        state.next_changed_text(Some("first".to_string())),
        Some("first".to_string())
    );
    assert_eq!(state.next_changed_text(Some("first".to_string())), None);
    assert_eq!(
        state.next_changed_text(Some("second".to_string())),
        Some("second".to_string())
    );
    assert_eq!(state.next_changed_text(Some("".to_string())), None);
    assert_eq!(state.next_changed_text(None), None);
}

#[test]
fn clipboard_publish_state_throttles_platform_reads() {
    let mut state = ClipboardPublishState::default();
    let now = Instant::now();

    assert!(state.should_poll(now));
    assert!(!state.should_poll(now + Duration::from_millis(250)));
    assert!(state.should_poll(now + CLIPBOARD_PUBLISH_POLL_INTERVAL));
}

#[test]
fn extended_clipboard_provide_decodes_utf8_text_and_strips_nul() {
    let payload = build_extended_clipboard_provide_payload("中文剪贴板\r\nsecond line");

    let decoded = decode_extended_clipboard_provide(&payload)
        .expect("decode provide")
        .expect("text payload");

    assert_eq!(decoded, "中文剪贴板\nsecond line");
}
