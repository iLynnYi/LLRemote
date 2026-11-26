fn try_handle_secure_attention_key_event(
    down: bool,
    keysym: u32,
    state: &mut SecureAttentionKeyState,
) -> bool {
    if keysym_is_control(keysym) {
        state.control_down = down;
        if !down {
            state.delete_sequence_active = false;
        }
        return false;
    }

    if keysym_is_alt(keysym) {
        state.alt_down = down;
        if !down {
            state.delete_sequence_active = false;
        }
        return false;
    }

    if keysym != KEYSYM_DELETE {
        return false;
    }

    if down && state.control_down && state.alt_down {
        state.delete_sequence_active = true;
        if let Err(error) = send_secure_attention_sequence() {
            eprintln!("local VNC secure attention: {error}");
        }
        return true;
    }

    if !down && state.delete_sequence_active {
        state.delete_sequence_active = false;
        return true;
    }

    false
}

fn keysym_is_control(keysym: u32) -> bool {
    matches!(keysym, KEYSYM_CONTROL_LEFT | KEYSYM_CONTROL_RIGHT)
}

fn keysym_is_alt(keysym: u32) -> bool {
    matches!(keysym, KEYSYM_ALT_LEFT | KEYSYM_ALT_RIGHT)
}
