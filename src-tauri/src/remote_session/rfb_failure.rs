use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RfbFailureCategory {
    CredentialsRequired,
    AuthenticationFailed,
    ConnectionCancelled,
    UnsupportedSecurity,
    SecurityFailure,
    NotVncService,
    ProtocolError,
    ResourceLimit,
    TargetUnreachable,
    RemoteClosed,
    SessionUnavailable,
    UnknownError,
}

/// Stable command/event boundary. Peer strings and OS error descriptions never
/// enter the serialized failure; the frontend selects copy from the category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfbFailure {
    pub category: RfbFailureCategory,
    pub code: &'static str,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<TightLengthDiagnostic>,
}

/// Only this fixed numeric shape may accompany a Tight length failure over IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TightLengthDiagnostic {
    pub phase: &'static str,
    pub stream: u8,
    pub width: u32,
    pub height: u32,
    pub payload_bytes: u32,
    pub consumed_bytes: u32,
    pub expected_bytes: u32,
    pub decoded_bytes: u32,
}

impl TightLengthDiagnostic {
    fn from_numeric_suffix(suffix: &str) -> Option<Self> {
        let mut fields = suffix.split(',');
        let phase = match fields.next()? {
            "framing" => "framing",
            "no_zlib_decode" => "no_zlib_decode",
            "inflate" => "inflate",
            _ => return None,
        };
        let stream = fields.next()?.parse::<u8>().ok()?;
        let width = fields.next()?.parse::<u32>().ok()?;
        let height = fields.next()?.parse::<u32>().ok()?;
        let payload_bytes = fields.next()?.parse::<u32>().ok()?;
        let consumed_bytes = fields.next()?.parse::<u32>().ok()?;
        let expected_bytes = fields.next()?.parse::<u32>().ok()?;
        let decoded_bytes = fields.next()?.parse::<u32>().ok()?;
        if fields.next().is_some()
            || stream > 3
            || !(1..=u16::MAX as u32).contains(&width)
            || !(1..=u16::MAX as u32).contains(&height)
        {
            return None;
        }
        Some(Self {
            phase,
            stream,
            width,
            height,
            payload_bytes,
            consumed_bytes,
            expected_bytes,
            decoded_bytes,
        })
    }
}

impl RfbFailure {
    /// Bridge the existing internal protocol codes while decoders are migrated.
    /// Only known constant codes cross IPC, including for unexpected errors.
    pub fn from_internal(error: &str) -> Self {
        use RfbFailureCategory::*;
        let prefix = error.split(':').next().unwrap_or_default();
        let (category, code, retryable) = match prefix {
            "rfb_credentials_required" => (CredentialsRequired, "rfb_credentials_required", true),
            "rfb_security_failed" => (AuthenticationFailed, "rfb_security_failed", true),
            "rfb_cancelled" => (ConnectionCancelled, "rfb_cancelled", true),
            "rfb_unsupported_security"
            | "rfb_unsupported_security_type"
            | "rfb_unsupported_security_types" => {
                (UnsupportedSecurity, "rfb_unsupported_security", false)
            }
            "rfb_no_security_types" | "rfb_server_offered_no_security_types" => {
                (SecurityFailure, "rfb_no_security_types", false)
            }
            "rfb_invalid_version_banner" => (NotVncService, "rfb_invalid_version_banner", false),
            "rfb_resource_limit" | "rfb_command_queue_full" => {
                (ResourceLimit, "rfb_resource_limit", false)
            }
            "rfb_resolve_failed" => (TargetUnreachable, "rfb_resolve_failed", true),
            "rfb_tcp_connect_failed" => (TargetUnreachable, "rfb_tcp_connect_failed", true),
            "rfb_startup_timeout" => (TargetUnreachable, "rfb_startup_timeout", true),
            "rfb_remote_closed" => (RemoteClosed, "rfb_remote_closed", true),
            "rfb_tcp_read_reset" => (TargetUnreachable, "rfb_tcp_read_reset", true),
            "rfb_tcp_read_aborted" => (TargetUnreachable, "rfb_tcp_read_aborted", true),
            "rfb_tcp_read_failed" => (TargetUnreachable, "rfb_tcp_read_failed", true),
            "rfb_tcp_write_timeout" => (TargetUnreachable, "rfb_tcp_write_timeout", true),
            "rfb_tcp_write_reset" => (TargetUnreachable, "rfb_tcp_write_reset", true),
            "rfb_tcp_write_broken_pipe" => (TargetUnreachable, "rfb_tcp_write_broken_pipe", true),
            "rfb_tcp_write_failed" => (TargetUnreachable, "rfb_tcp_write_failed", true),
            "rfb_tcp_flush_failed" => (TargetUnreachable, "rfb_tcp_flush_failed", true),
            "rfb_transport_failed" => (TargetUnreachable, "rfb_transport_failed", true),
            "rfb_transport_closed" | "rfb_shutdown_pending" => {
                (SessionUnavailable, "rfb_transport_closed", true)
            }
            "rfb_session_unavailable" => (SessionUnavailable, "rfb_session_unavailable", true),
            "rfb_internal_error" => (UnknownError, "rfb_internal_error", false),
            code if code.starts_with("remote_session_") => {
                (SessionUnavailable, "rfb_session_unavailable", true)
            }
            "rfb_auth_state_poisoned"
            | "rfb_worker_spawn_failed"
            | "rfb_worker_panicked"
            | "rfb_worker_lock_poisoned"
            | "rfb_socket_lock_poisoned"
            | "rfb_socket_clone_failed"
            | "rfb_socket_options_failed"
            | "rfb_startup_channel_closed"
            | "rfb_connect_dispatch_failed"
            | "rfb_disconnect_dispatch_failed"
            | "rfb_stop_dispatch_failed" => (UnknownError, "rfb_internal_error", false),
            code if code.starts_with("rfb_") => (ProtocolError, protocol_failure_code(code), false),
            _ => (UnknownError, "rfb_internal_error", false),
        };
        Self {
            category,
            code,
            retryable,
            diagnostic: error
                .strip_prefix("rfb_tight_decoded_length_mismatch:")
                .and_then(TightLengthDiagnostic::from_numeric_suffix),
        }
    }
}

// Only audited parser/decoder constants are diagnostic codes. Never forward an
// arbitrary prefix, a dynamic encoding number, or a peer/OS error description.
fn protocol_failure_code(prefix: &str) -> &'static str {
    match prefix {
        "rfb_clipboard_too_large" => "rfb_clipboard_too_large",
        "rfb_cursor_length_mismatch" => "rfb_cursor_length_mismatch",
        "rfb_cursor_size_overflow" => "rfb_cursor_size_overflow",
        "rfb_desktop_name_length_overflow" => "rfb_desktop_name_length_overflow",
        "rfb_desktop_name_not_utf8" => "rfb_desktop_name_not_utf8",
        "rfb_expected_framebuffer_update" => "rfb_expected_framebuffer_update",
        "rfb_extended_clipboard_deflate_failed" => "rfb_extended_clipboard_deflate_failed",
        "rfb_extended_clipboard_inflate_failed" => "rfb_extended_clipboard_inflate_failed",
        "rfb_extended_clipboard_payload_too_short" => "rfb_extended_clipboard_payload_too_short",
        "rfb_extended_clipboard_text_size_missing" => "rfb_extended_clipboard_text_size_missing",
        "rfb_extended_clipboard_text_truncated" => "rfb_extended_clipboard_text_truncated",
        "rfb_framebuffer_before_server_init" => "rfb_framebuffer_before_server_init",
        "rfb_framebuffer_rect_out_of_bounds" => "rfb_framebuffer_rect_out_of_bounds",
        "rfb_framebuffer_rectangle_size_overflow" => "rfb_framebuffer_rectangle_size_overflow",
        "rfb_framebuffer_update_trailing_bytes" => "rfb_framebuffer_update_trailing_bytes",
        "rfb_framebuffer_update_truncated" => "rfb_framebuffer_update_truncated",
        "rfb_handshake_failed" => "rfb_handshake_failed",
        "rfb_hextile_size_overflow" => "rfb_hextile_size_overflow",
        "rfb_hextile_background_missing" => "rfb_hextile_background_missing",
        "rfb_hextile_foreground_missing" => "rfb_hextile_foreground_missing",
        "rfb_hextile_invalid_flags" => "rfb_hextile_invalid_flags",
        "rfb_hextile_subrect_out_of_bounds" => "rfb_hextile_subrect_out_of_bounds",
        "rfb_hextile_tile_length_mismatch" => "rfb_hextile_tile_length_mismatch",
        "rfb_hextile_tile_out_of_bounds" => "rfb_hextile_tile_out_of_bounds",
        "rfb_invalid_desktop_size" => "rfb_invalid_desktop_size",
        "rfb_invalid_extended_desktop_size" => "rfb_invalid_extended_desktop_size",
        "rfb_invalid_pixel_format" => "rfb_invalid_pixel_format",
        "rfb_quality_profile_out_of_range" => "rfb_quality_profile_out_of_range",
        "rfb_tight_copy_length_mismatch" => "rfb_tight_copy_length_mismatch",
        "rfb_tight_decoded_length_mismatch" => "rfb_tight_decoded_length_mismatch",
        "rfb_tight_decompression_failed" => "rfb_tight_decompression_failed",
        "rfb_tight_gradient_length_mismatch" => "rfb_tight_gradient_length_mismatch",
        "rfb_tight_image_decode_failed" => "rfb_tight_image_decode_failed",
        "rfb_tight_image_geometry_mismatch" => "rfb_tight_image_geometry_mismatch",
        "rfb_tight_palette_index_out_of_bounds" => "rfb_tight_palette_index_out_of_bounds",
        "rfb_tight_palette_length_mismatch" => "rfb_tight_palette_length_mismatch",
        "rfb_tight_size_overflow" => "rfb_tight_size_overflow",
        "rfb_tight_rectangle_width_limit" => "rfb_tight_rectangle_width_limit",
        "rfb_tight_invalid_palette" => "rfb_tight_invalid_palette",
        "rfb_unsupported_framebuffer_encoding" => "rfb_unsupported_framebuffer_encoding",
        "rfb_unsupported_pixel_format" => "rfb_unsupported_pixel_format",
        "rfb_unsupported_protocol_version" => "rfb_unsupported_protocol_version",
        "rfb_unsupported_server_message_type" => "rfb_unsupported_server_message_type",
        "rfb_unsupported_tight_control" => "rfb_unsupported_tight_control",
        "rfb_unsupported_tight_filter" => "rfb_unsupported_tight_filter",
        "rfb_zlib_decoded_length_mismatch" => "rfb_zlib_decoded_length_mismatch",
        "rfb_zlib_decompression_failed" => "rfb_zlib_decompression_failed",
        "rfb_zrle_compressed_length_mismatch" => "rfb_zrle_compressed_length_mismatch",
        "rfb_zrle_decoded_trailing_bytes" => "rfb_zrle_decoded_trailing_bytes",
        "rfb_zrle_decompression_failed" => "rfb_zrle_decompression_failed",
        "rfb_zrle_invalid_packed_palette" => "rfb_zrle_invalid_packed_palette",
        "rfb_zrle_invalid_subencoding" => "rfb_zrle_invalid_subencoding",
        "rfb_zrle_palette_index_out_of_bounds" => "rfb_zrle_palette_index_out_of_bounds",
        "rfb_zrle_rectangle_size_overflow" => "rfb_zrle_rectangle_size_overflow",
        "rfb_zrle_run_exceeds_tile" => "rfb_zrle_run_exceeds_tile",
        "rfb_zrle_run_length_overflow" => "rfb_zrle_run_length_overflow",
        "rfb_zrle_tile_length_mismatch" => "rfb_zrle_tile_length_mismatch",
        "rfb_zrle_tile_size_overflow" => "rfb_zrle_tile_size_overflow",
        _ => "rfb_protocol_error",
    }
}
