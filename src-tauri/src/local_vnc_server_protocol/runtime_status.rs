use crate::remote_desktop_core::{SecureAttentionResult, ServerRuntimeMessage};

fn service_secure_attention_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        crate::local_vnc_server_platform::windows_desktop::is_service_worker()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

fn write_runtime_message<W: Write>(
    output: &mut W,
    notice: ServerRuntimeMessage,
) -> std::io::Result<()> {
    let bytes = crate::remote_desktop_core::rfb_fence::fence_message(
        crate::remote_desktop_core::rfb_fence::FENCE_FLAG_REQUEST,
        &notice.encode(),
    )
    .map_err(std::io::Error::other)?;
    output.write_all(&bytes)
}

fn handle_runtime_request<W: Write>(
    output: &mut W,
    payload: &[u8],
    view_only: bool,
) -> std::io::Result<()> {
    let Some(ServerRuntimeMessage::SecureAttentionRequest { request_id }) =
        ServerRuntimeMessage::decode(payload)
    else {
        return Ok(());
    };
    let result = if view_only {
        SecureAttentionResult::Denied
    } else if !service_secure_attention_available() {
        SecureAttentionResult::Unsupported
    } else {
        match send_secure_attention_sequence() {
            Ok(()) => SecureAttentionResult::Submitted,
            Err(code)
                if code == "windows_service_sas_policy_denied"
                    || code == "windows_service_access_denied" =>
            {
                SecureAttentionResult::Denied
            }
            Err(_) => SecureAttentionResult::Failed,
        }
    };
    write_runtime_message(
        output,
        ServerRuntimeMessage::SecureAttentionResponse { request_id, result },
    )
}

fn handle_relay_runtime_request<W: Write>(
    output: &mut W,
    payload: &[u8],
    view_only: bool,
    relay: &crate::local_vnc_server::desktop_relay::DesktopRelay,
) -> std::io::Result<()> {
    let Some(ServerRuntimeMessage::SecureAttentionRequest { request_id }) =
        ServerRuntimeMessage::decode(payload)
    else {
        return Ok(());
    };
    let result = if view_only {
        SecureAttentionResult::Denied
    } else {
        relay.secure_attention()
    };
    write_runtime_message(
        output,
        ServerRuntimeMessage::SecureAttentionResponse { request_id, result },
    )
}
