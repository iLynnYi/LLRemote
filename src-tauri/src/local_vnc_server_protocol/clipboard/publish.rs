pub(super) fn publish_changed_platform_clipboard<W: Write>(
    stream: &mut W,
    supports_extended_clipboard: bool,
    clipboard_publish_state: &mut ClipboardPublishState,
) -> Result<(), std::io::Error> {
    if !supports_extended_clipboard {
        return Ok(());
    }
    if !clipboard_publish_state.should_poll(Instant::now()) {
        return Ok(());
    }

    let text = match get_clipboard_text_for_vnc() {
        Ok(text) => text,
        Err(error) => {
            eprintln!("local VNC clipboard: {error}");
            return Ok(());
        }
    };

    if let Some(text) = clipboard_publish_state.next_changed_text(text) {
        write_extended_clipboard_notify(stream)?;
        eprintln!(
            "local VNC clipboard: notified local clipboard utf8_bytes={}",
            text.len(),
        );
    }
    Ok(())
}

fn notify_current_platform_clipboard<W: Write>(
    stream: &mut W,
    supports_extended_clipboard: bool,
    clipboard_publish_state: &mut ClipboardPublishState,
) -> Result<(), std::io::Error> {
    if !supports_extended_clipboard {
        return Ok(());
    }

    let text = match get_clipboard_text_for_vnc() {
        Ok(Some(text)) if !text.is_empty() => text,
        Ok(_) => return Ok(()),
        Err(error) => {
            eprintln!("local VNC clipboard: {error}");
            return Ok(());
        }
    };

    clipboard_publish_state.remember_text(&text);
    write_extended_clipboard_notify(stream)?;
    Ok(())
}

fn publish_current_platform_clipboard<W: Write>(
    stream: &mut W,
    supports_extended_clipboard: bool,
    clipboard_publish_state: &mut ClipboardPublishState,
) -> Result<(), std::io::Error> {
    if !supports_extended_clipboard {
        return Ok(());
    }

    let text = match get_clipboard_text_for_vnc() {
        Ok(Some(text)) if !text.is_empty() => text,
        Ok(_) => return Ok(()),
        Err(error) => {
            eprintln!("local VNC clipboard: {error}");
            return Ok(());
        }
    };

    clipboard_publish_state.remember_text(&text);
    write_extended_clipboard_provide(stream, &text)?;
    Ok(())
}

fn write_platform_clipboard(text: &str) {
    if let Err(error) = set_clipboard_text_from_vnc(text) {
        eprintln!("local VNC clipboard: {error}");
    }
}
