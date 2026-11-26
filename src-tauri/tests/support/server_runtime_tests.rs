#[test]
fn runtime_sas_reply_denies_readonly_and_reports_unsupported_without_task_manager_fallback() {
    use crate::remote_desktop_core::{SecureAttentionResult, ServerRuntimeMessage};
    let payload = ServerRuntimeMessage::SecureAttentionRequest { request_id: 42 }.encode();
    let mut wire = Vec::new();
    handle_runtime_request(&mut wire, &payload, true).unwrap();
    assert_eq!(
        ServerRuntimeMessage::decode(&wire[9..]),
        Some(ServerRuntimeMessage::SecureAttentionResponse {
            request_id: 42,
            result: SecureAttentionResult::Denied,
        })
    );
    // Unit tests never run in a service worker, including on Windows.
    wire.clear();
    handle_runtime_request(&mut wire, &payload, false).unwrap();
    assert_eq!(
        ServerRuntimeMessage::decode(&wire[9..]),
        Some(ServerRuntimeMessage::SecureAttentionResponse {
            request_id: 42,
            result: SecureAttentionResult::Unsupported,
        })
    );
    wire.clear();
    handle_runtime_request(&mut wire, b"unrelated opaque fence", false).unwrap();
    assert!(wire.is_empty());
}

#[test]
fn runtime_status_requires_explicit_negotiation_and_reports_readonly_sas_result() {
    use crate::remote_desktop_core::{
        DesktopAccessState, SecureAttentionResult, ServerRuntimeMessage,
        RUNTIME_STATUS_PSEUDO_ENCODING,
    };
    let fence = crate::server_discovery::FENCE_PSEUDO_ENCODING;
    assert!(!parse_client_encoding_support(&fence.to_be_bytes()).runtime_status);
    assert!(
        !parse_client_encoding_support(&RUNTIME_STATUS_PSEUDO_ENCODING.to_be_bytes())
            .runtime_status
    );
    let encodings = [
        fence.to_be_bytes(),
        RUNTIME_STATUS_PSEUDO_ENCODING.to_be_bytes(),
    ]
    .concat();
    assert!(parse_client_encoding_support(&encodings).runtime_status);
    let request = ServerRuntimeMessage::SecureAttentionRequest { request_id: 7 }.encode();
    let wire = continuous_control_wire(vec![
        ClientMessage::Encodings(encodings),
        ClientMessage::Fence {
            flags: 1 << 31,
            payload: request.to_vec(),
        },
    ]);
    let mut notices = Vec::new();
    let mut position = 0;
    while position < wire.len() {
        assert_eq!(wire[position], 248);
        let length = wire[position + 8] as usize;
        let flags = u32::from_be_bytes(wire[position + 4..position + 8].try_into().unwrap());
        if flags & (1 << 31) != 0 {
            if let Some(notice) =
                ServerRuntimeMessage::decode(&wire[position + 9..position + 9 + length])
            {
                notices.push(notice);
            }
        }
        position += 9 + length;
    }
    assert!(
        notices.contains(&ServerRuntimeMessage::SecureAttentionResponse {
            request_id: 7,
            result: SecureAttentionResult::Denied
        })
    );
    assert!(notices.contains(&ServerRuntimeMessage::Desktop {
        desktop: DesktopAccessState::Waiting,
        secure_attention_available: false
    }));
}
