pub(super) fn handle_client_cut_text<W: Write>(
    stream: &mut W,
    signed_length: i32,
    payload: &[u8],
    view_only: bool,
    supports_extended_clipboard: bool,
    clipboard_publish_state: &mut ClipboardPublishState,
) -> Result<ClientCutTextOutcome, std::io::Error> {
    if signed_length < 0 {
        let outcome = handle_extended_client_cut_text(
            stream,
            payload,
            view_only,
            supports_extended_clipboard,
            clipboard_publish_state,
        )?;
        return Ok(outcome);
    }

    if view_only {
        eprintln!("local VNC clipboard: ignored legacy ClientCutText while sharing is view-only");
        return Ok(ClientCutTextOutcome::default());
    }

    let decoded = decode_legacy_cut_text(payload);
    eprintln!(
        "local VNC clipboard: received legacy ClientCutText bytes={} encoding={}",
        payload.len(),
        decoded.encoding,
    );
    write_platform_clipboard(&decoded.text);
    clipboard_publish_state.remember_text(&decoded.text);
    Ok(ClientCutTextOutcome::default())
}

fn handle_extended_client_cut_text<W: Write>(
    stream: &mut W,
    payload: &[u8],
    view_only: bool,
    supports_extended_clipboard: bool,
    clipboard_publish_state: &mut ClipboardPublishState,
) -> Result<ClientCutTextOutcome, std::io::Error> {
    if payload.len() < 4 {
        eprintln!("local VNC clipboard: ignored malformed Extended Clipboard message");
        return Ok(ClientCutTextOutcome::default());
    }

    if !supports_extended_clipboard {
        eprintln!("local VNC clipboard: ignored Extended Clipboard before capability negotiation");
        return Ok(ClientCutTextOutcome::default());
    }

    let flags = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let actions = flags & EXTENDED_CLIPBOARD_ACTION_MASK;
    let formats = flags & EXTENDED_CLIPBOARD_FORMAT_MASK;

    if (actions & EXTENDED_CLIPBOARD_ACTION_CAPS) != 0 {
        eprintln!("local VNC clipboard: received client Extended Clipboard capabilities");
        return Ok(ClientCutTextOutcome {
            received_extended_clipboard_caps: true,
        });
    }

    if view_only && actions == EXTENDED_CLIPBOARD_ACTION_PROVIDE {
        eprintln!(
            "local VNC clipboard: ignored Extended Clipboard Provide while sharing is view-only"
        );
        return Ok(ClientCutTextOutcome::default());
    }

    if actions == EXTENDED_CLIPBOARD_ACTION_NOTIFY {
        if (formats & EXTENDED_CLIPBOARD_FORMAT_TEXT) != 0 {
            eprintln!("local VNC clipboard: received Notify, requesting UTF-8 text");
            write_extended_clipboard_request(stream)?;
        }
        return Ok(ClientCutTextOutcome::default());
    }

    if actions == EXTENDED_CLIPBOARD_ACTION_REQUEST {
        eprintln!("local VNC clipboard: received Request; providing current local clipboard");
        publish_current_platform_clipboard(
            stream,
            supports_extended_clipboard,
            clipboard_publish_state,
        )?;
        return Ok(ClientCutTextOutcome::default());
    }

    if actions == EXTENDED_CLIPBOARD_ACTION_PEEK {
        eprintln!("local VNC clipboard: received Peek; notifying current local clipboard");
        notify_current_platform_clipboard(
            stream,
            supports_extended_clipboard,
            clipboard_publish_state,
        )?;
        return Ok(ClientCutTextOutcome::default());
    }

    if actions == EXTENDED_CLIPBOARD_ACTION_PROVIDE {
        match decode_extended_clipboard_provide(payload) {
            Ok(Some(text)) => {
                eprintln!(
                    "local VNC clipboard: received Extended Clipboard text utf8_bytes={}",
                    text.len()
                );
                write_platform_clipboard(&text);
                clipboard_publish_state.remember_text(&text);
            }
            Ok(None) => {
                eprintln!("local VNC clipboard: Extended Clipboard Provide had no text format");
            }
            Err(message) => {
                eprintln!(
                    "local VNC clipboard: failed to decode Extended Clipboard Provide: {message}"
                );
            }
        }
        return Ok(ClientCutTextOutcome::default());
    }

    eprintln!(
        "local VNC clipboard: ignored unsupported Extended Clipboard action flags=0x{actions:08x}"
    );
    Ok(ClientCutTextOutcome::default())
}
