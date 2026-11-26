use LLRemoteLib::remote_session::{RfbFailure, RfbFailureCategory};

#[test]
fn tight_length_failure_serializes_only_valid_numeric_decoder_metadata() {
    use std::io::Write;
    use LLRemoteLib::remote_desktop_core::{
        ClientFramebufferCore, ClientRfbPixelFormat, FramebufferGeometry, PixelFormat,
        RfbClientFramebufferDecoder,
    };
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&[1, 2, 3].repeat(3)).unwrap(); // Three pixels for an announced four.
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < 128);
    let mut message = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 4, 0, 1];
    message.extend(7_i32.to_be_bytes());
    message.extend([0, compressed.len() as u8]);
    message.extend(&compressed);
    let error = RfbClientFramebufferDecoder::new(ClientRfbPixelFormat::core_bgra32())
        .unwrap()
        .decode_framebuffer_update(
            &message,
            FramebufferGeometry {
                width: 4,
                height: 1,
                pixel_format: PixelFormat::Bgra8888,
            },
            &mut ClientFramebufferCore::default(),
        )
        .unwrap_err();
    let failure = RfbFailure::from_internal(&error);
    assert_eq!(failure.code, "rfb_tight_decoded_length_mismatch");
    assert_eq!(failure.category, RfbFailureCategory::ProtocolError);
    assert!(!failure.retryable);
    let wire = serde_json::to_value(failure).unwrap();
    assert_eq!(
        wire["diagnostic"],
        serde_json::json!({
            "phase": "inflate", "stream": 0, "width": 4, "height": 1,
            "payloadBytes": compressed.len(), "consumedBytes": compressed.len(),
            "expectedBytes": 12, "decodedBytes": 9,
        })
    );
    for phase in ["framing", "no_zlib_decode"] {
        let input = format!("rfb_tight_decoded_length_mismatch:{phase},0,4,1,9,0,12,9");
        assert_eq!(
            serde_json::to_value(RfbFailure::from_internal(&input)).unwrap()["diagnostic"]["phase"],
            phase
        );
    }
    let private = uuid::Uuid::new_v4().to_string();
    for suffix in [
        private.clone(),
        format!("inflate,0,4,1,9,9,12,9,{private}"),
        "unknown,0,4,1,9,9,12,9".into(),
        "inflate,4,4,1,9,9,12,9".into(),
        "inflate,0,65536,1,9,9,12,9".into(),
        "inflate,0,4,0,9,9,12,9".into(),
        "inflate,0,4,1,4294967296,9,12,9".into(),
        "inflate,0,4,1,9,9,12,-1".into(),
    ] {
        let failure =
            RfbFailure::from_internal(&format!("rfb_tight_decoded_length_mismatch:{suffix}"));
        let wire = serde_json::to_value(failure).unwrap();
        assert!(wire.get("diagnostic").is_none());
        assert!(!wire.to_string().contains(&private));
    }
}

#[test]
fn native_failure_categories_and_retry_policy_survive_serialization_and_reclassification() {
    use RfbFailureCategory::*;
    for (input, category, retryable) in [
        ("rfb_credentials_required", CredentialsRequired, true),
        ("rfb_security_failed:1", AuthenticationFailed, true),
        ("rfb_cancelled", ConnectionCancelled, true),
        (
            "rfb_unsupported_security_type:99",
            UnsupportedSecurity,
            false,
        ),
        (
            "rfb_server_offered_no_security_types",
            SecurityFailure,
            false,
        ),
        ("rfb_invalid_version_banner", NotVncService, false),
        ("rfb_resource_limit:framebuffer", ResourceLimit, false),
        ("rfb_command_queue_full", ResourceLimit, false),
        ("rfb_tcp_connect_failed", TargetUnreachable, true),
        ("rfb_startup_timeout", TargetUnreachable, true),
        ("rfb_tcp_read_failed:OS detail", TargetUnreachable, true),
        ("rfb_remote_closed", RemoteClosed, true),
        ("remote_session_not_found", SessionUnavailable, true),
        ("rfb_shutdown_pending", SessionUnavailable, true),
        ("rfb_zrle_invalid_subencoding", ProtocolError, false),
        ("rfb_worker_panicked", UnknownError, false),
        ("rfb_auth_state_poisoned", UnknownError, false),
    ] {
        let failure = RfbFailure::from_internal(input);
        assert_eq!(failure.category, category);
        assert_eq!(failure.retryable, retryable);
        assert_eq!(RfbFailure::from_internal(failure.code), failure);
        let wire = serde_json::to_value(&failure).unwrap();
        assert_eq!(wire["retryable"], retryable);
        assert_eq!(wire.as_object().unwrap().len(), 3);
    }
}

#[test]
fn native_failures_discard_raw_peer_and_os_details_even_for_unknown_errors() {
    let private = uuid::Uuid::new_v4().to_string();
    for prefix in [
        "rfb_tcp_write_failed",
        "rfb_security_failed",
        "rfb_resource_limit",
        "unexpected",
    ] {
        let failure = RfbFailure::from_internal(&format!("{prefix}:{private}"));
        let wire = serde_json::to_string(&failure).unwrap();
        assert!(!wire.contains(&private));
        assert!(!format!("{failure:?}").contains(&private));
    }
}

#[test]
fn known_protocol_failures_keep_safe_diagnostic_codes_without_peer_details() {
    let private = uuid::Uuid::new_v4().to_string();
    for code in [
        "rfb_desktop_name_not_utf8",
        "rfb_tight_rectangle_width_limit",
        "rfb_tight_invalid_palette",
        "rfb_tight_decoded_length_mismatch",
        "rfb_hextile_background_missing",
        "rfb_hextile_foreground_missing",
        "rfb_hextile_invalid_flags",
        "rfb_zlib_decompression_failed",
        "rfb_framebuffer_update_truncated",
        "rfb_unsupported_server_message_type",
    ] {
        let failure = RfbFailure::from_internal(&format!("{code}:{private}"));
        assert_eq!(failure.category, RfbFailureCategory::ProtocolError);
        assert_eq!(failure.code, code);
        assert!(!failure.retryable);
        assert!(!serde_json::to_string(&failure).unwrap().contains(&private));
        assert_eq!(RfbFailure::from_internal(failure.code), failure);
    }
    let unknown = RfbFailure::from_internal(&format!("rfb_unrecognized_{private}"));
    assert_eq!(unknown.code, "rfb_protocol_error");
}
