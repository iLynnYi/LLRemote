pub(super) fn write_extended_clipboard_caps<W: Write>(
    stream: &mut W,
) -> Result<(), std::io::Error> {
    let flags = EXTENDED_CLIPBOARD_ACTION_CAPS
        | EXTENDED_CLIPBOARD_ACTION_REQUEST
        | EXTENDED_CLIPBOARD_ACTION_NOTIFY
        | EXTENDED_CLIPBOARD_ACTION_PROVIDE
        | EXTENDED_CLIPBOARD_FORMAT_TEXT;
    let mut payload = Vec::with_capacity(8);
    payload.extend_from_slice(&flags.to_be_bytes());
    payload.extend_from_slice(&EXTENDED_CLIPBOARD_MAX_TEXT_BYTES.to_be_bytes());
    write_server_cut_text_payload(stream, &payload, true)
}

pub(super) fn write_extended_clipboard_request<W: Write>(
    stream: &mut W,
) -> Result<(), std::io::Error> {
    let flags = EXTENDED_CLIPBOARD_ACTION_REQUEST | EXTENDED_CLIPBOARD_FORMAT_TEXT;
    write_server_cut_text_payload(stream, &flags.to_be_bytes(), true)
}

pub(super) fn write_extended_clipboard_notify<W: Write>(
    stream: &mut W,
) -> Result<(), std::io::Error> {
    let flags = EXTENDED_CLIPBOARD_ACTION_NOTIFY | EXTENDED_CLIPBOARD_FORMAT_TEXT;
    write_server_cut_text_payload(stream, &flags.to_be_bytes(), true)
}

pub(super) fn write_extended_clipboard_provide<W: Write>(
    stream: &mut W,
    text: &str,
) -> Result<(), std::io::Error> {
    let mut text_payload = text.as_bytes().to_vec();
    text_payload.push(0);

    let mut deflated_source = Vec::with_capacity(4 + text_payload.len());
    deflated_source.extend_from_slice(&(text_payload.len() as u32).to_be_bytes());
    deflated_source.extend_from_slice(&text_payload);

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&deflated_source)?;
    let compressed = encoder.finish()?;

    let flags = EXTENDED_CLIPBOARD_ACTION_PROVIDE | EXTENDED_CLIPBOARD_FORMAT_TEXT;
    let mut payload = Vec::with_capacity(4 + compressed.len());
    payload.extend_from_slice(&flags.to_be_bytes());
    payload.extend_from_slice(&compressed);
    write_server_cut_text_payload(stream, &payload, true)
}

fn write_server_cut_text_payload<W: Write>(
    stream: &mut W,
    payload: &[u8],
    extended: bool,
) -> Result<(), std::io::Error> {
    let signed_length = if extended {
        -(payload.len() as i32)
    } else {
        payload.len() as i32
    };
    stream.write_all(&[3, 0, 0, 0])?;
    stream.write_all(&signed_length.to_be_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}
