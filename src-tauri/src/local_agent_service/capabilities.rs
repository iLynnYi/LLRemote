pub fn agent_capabilities() -> Vec<String> {
    let mut capabilities = vec![
        CLIPBOARD_TEXT_WRITE.to_string(),
        CLIPBOARD_TEXT_READ.to_string(),
        FILE_TRANSFER_UPLOAD.to_string(),
        FILE_TRANSFER_RELATIVE_PATH.to_string(),
        FILE_TRANSFER_CANCEL_CAPABILITY.to_string(),
        FILE_TRANSFER_SHA256.to_string(),
        FILE_TRANSFER_STATUS.to_string(),
        FILE_TRANSFER_RESUME.to_string(),
        FILE_REVEAL.to_string(),
        FILE_OPEN.to_string(),
        AGENT_PERMISSIONS_DIAGNOSE.to_string(),
        AGENT_DISPLAY_ENUMERATE.to_string(),
        AGENT_DISPLAY_SELECT.to_string(),
        REMOTE_DESKTOP_CONTROL.to_string(),
        AGENT_SECURE_INPUT_RECOVER.to_string(),
    ];
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        capabilities.push(DISPLAY_WAKE.to_string());
        capabilities.push(SESSION_END.to_string());
        capabilities.push(MESSAGE_SHOW.to_string());
    }
    if cfg!(target_os = "windows") {
        capabilities.retain(|capability| {
            !matches!(
                capability.as_str(),
                REMOTE_DESKTOP_CONTROL | AGENT_SECURE_INPUT_RECOVER
            )
        });
    }
    capabilities
}

fn agent_capabilities_for_context(context: &AgentRuntimeContext) -> Vec<String> {
    let mut capabilities = agent_capabilities();
    if context.view_only {
        capabilities.retain(|capability| {
            matches!(
                capability.as_str(),
                CLIPBOARD_TEXT_READ
                    | AGENT_PERMISSIONS_DIAGNOSE
                    | AGENT_DISPLAY_ENUMERATE
                    | AGENT_DISPLAY_SELECT
                    | SESSION_END
            )
        });
    }
    if context
        .interactive
        .as_ref()
        .is_some_and(|active| !active.load(Ordering::SeqCst))
    {
        capabilities.retain(|capability| {
            !matches!(
                capability.as_str(),
                CLIPBOARD_TEXT_READ
                    | CLIPBOARD_TEXT_WRITE
                    | MESSAGE_SHOW
                    | FILE_OPEN
                    | FILE_REVEAL
                    | AGENT_DISPLAY_SELECT
                    | DISPLAY_WAKE
                    | REMOTE_DESKTOP_CONTROL
                    | SESSION_END
            )
        });
    }
    capabilities
}
