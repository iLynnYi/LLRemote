const failures = {
  rfb_credentials_required: { category: "credentials_required", retryable: true },
  rfb_security_failed: { category: "authentication_failed", retryable: true },
  rfb_authentication_cancelled: { category: "authentication_cancelled", retryable: true },
  rfb_cancelled: { category: "connection_cancelled", retryable: true },
  rfb_unsupported_security: { category: "unsupported_security", retryable: false },
  rfb_no_security_types: { category: "security_failure", retryable: false },
  rfb_invalid_version_banner: { category: "not_vnc_service", retryable: false },
  rfb_protocol_error: { category: "protocol_error", retryable: false },
  rfb_resource_limit: { category: "resource_limit", retryable: false },
  rfb_resolve_failed: { category: "target_unreachable", retryable: true },
  rfb_tcp_connect_failed: { category: "target_unreachable", retryable: true },
  rfb_startup_timeout: { category: "target_unreachable", retryable: true },
  rfb_transport_failed: { category: "target_unreachable", retryable: true },
  rfb_tcp_read_reset: { category: "target_unreachable", retryable: true },
  rfb_tcp_read_aborted: { category: "target_unreachable", retryable: true },
  rfb_tcp_read_failed: { category: "target_unreachable", retryable: true },
  rfb_tcp_write_timeout: { category: "target_unreachable", retryable: true },
  rfb_tcp_write_reset: { category: "target_unreachable", retryable: true },
  rfb_tcp_write_broken_pipe: { category: "target_unreachable", retryable: true },
  rfb_tcp_write_failed: { category: "target_unreachable", retryable: true },
  rfb_tcp_flush_failed: { category: "target_unreachable", retryable: true },
  rfb_remote_closed: { category: "remote_closed", retryable: true },
  rfb_transport_closed: { category: "session_unavailable", retryable: true },
  rfb_session_unavailable: { category: "session_unavailable", retryable: true },
  rfb_internal_error: { category: "unknown_error", retryable: false },
} as const;

// Keep the IPC allowlist aligned with Rust's protocol_failure_code. The UI still
// chooses localized copy from the category; diagnostics retain the exact code.
const protocolFailureCodes = [
  "rfb_clipboard_too_large",
  "rfb_cursor_length_mismatch",
  "rfb_cursor_size_overflow",
  "rfb_desktop_name_length_overflow",
  "rfb_desktop_name_not_utf8",
  "rfb_expected_framebuffer_update",
  "rfb_extended_clipboard_deflate_failed",
  "rfb_extended_clipboard_inflate_failed",
  "rfb_extended_clipboard_payload_too_short",
  "rfb_extended_clipboard_text_size_missing",
  "rfb_extended_clipboard_text_truncated",
  "rfb_framebuffer_before_server_init",
  "rfb_framebuffer_rect_out_of_bounds",
  "rfb_framebuffer_rectangle_size_overflow",
  "rfb_framebuffer_update_trailing_bytes",
  "rfb_framebuffer_update_truncated",
  "rfb_handshake_failed",
  "rfb_hextile_size_overflow",
  "rfb_hextile_background_missing",
  "rfb_hextile_foreground_missing",
  "rfb_hextile_invalid_flags",
  "rfb_hextile_subrect_out_of_bounds",
  "rfb_hextile_tile_length_mismatch",
  "rfb_hextile_tile_out_of_bounds",
  "rfb_invalid_desktop_size",
  "rfb_invalid_extended_desktop_size",
  "rfb_invalid_pixel_format",
  "rfb_quality_profile_out_of_range",
  "rfb_tight_copy_length_mismatch",
  "rfb_tight_decoded_length_mismatch",
  "rfb_tight_decompression_failed",
  "rfb_tight_gradient_length_mismatch",
  "rfb_tight_image_decode_failed",
  "rfb_tight_image_geometry_mismatch",
  "rfb_tight_palette_index_out_of_bounds",
  "rfb_tight_palette_length_mismatch",
  "rfb_tight_size_overflow",
  "rfb_tight_rectangle_width_limit",
  "rfb_tight_invalid_palette",
  "rfb_unsupported_framebuffer_encoding",
  "rfb_unsupported_pixel_format",
  "rfb_unsupported_protocol_version",
  "rfb_unsupported_server_message_type",
  "rfb_unsupported_tight_control",
  "rfb_unsupported_tight_filter",
  "rfb_zlib_decoded_length_mismatch",
  "rfb_zlib_decompression_failed",
  "rfb_zrle_compressed_length_mismatch",
  "rfb_zrle_decoded_trailing_bytes",
  "rfb_zrle_decompression_failed",
  "rfb_zrle_invalid_packed_palette",
  "rfb_zrle_invalid_subencoding",
  "rfb_zrle_palette_index_out_of_bounds",
  "rfb_zrle_rectangle_size_overflow",
  "rfb_zrle_run_exceeds_tile",
  "rfb_zrle_run_length_overflow",
  "rfb_zrle_tile_length_mismatch",
  "rfb_zrle_tile_size_overflow",
] as const;

type ProtocolFailureCode = (typeof protocolFailureCodes)[number];

export type TightLengthDiagnostic = {
  phase: "framing" | "no_zlib_decode" | "inflate";
  stream: number;
  width: number;
  height: number;
  payloadBytes: number;
  consumedBytes: number;
  expectedBytes: number;
  decodedBytes: number;
};

export type NativeRfbFailure = {
  category: (typeof failures)[keyof typeof failures]["category"];
  code: keyof typeof failures | ProtocolFailureCode;
  retryable: boolean;
  diagnostic?: TightLengthDiagnostic;
};

export class NativeRfbError extends Error {
  readonly failure: NativeRfbFailure;
  constructor(failure: NativeRfbFailure) {
    super(failure.code);
    this.failure = failure;
  }
}

function isFailureCode(code: unknown): code is NativeRfbFailure["code"] {
  return typeof code === "string"
    && (Object.prototype.hasOwnProperty.call(failures, code) || isProtocolFailureCode(code));
}

function isProtocolFailureCode(code: string): code is ProtocolFailureCode {
  return protocolFailureCodes.some((known) => known === code);
}

function isBoundedInteger(value: unknown, minimum: number, maximum: number): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= minimum && value <= maximum;
}

function readTightLengthDiagnostic(value: unknown): TightLengthDiagnostic | null {
  if (typeof value !== "object" || value === null
    || !("phase" in value) || !("stream" in value) || !("width" in value) || !("height" in value)
    || !("payloadBytes" in value) || !("consumedBytes" in value)
    || !("expectedBytes" in value) || !("decodedBytes" in value)) return null;
  const { phase, stream, width, height, payloadBytes, consumedBytes, expectedBytes, decodedBytes } = value;
  if ((phase !== "framing" && phase !== "no_zlib_decode" && phase !== "inflate")
    || !isBoundedInteger(stream, 0, 3) || !isBoundedInteger(width, 1, 65535) || !isBoundedInteger(height, 1, 65535)
    || !isBoundedInteger(payloadBytes, 0, 0xffffffff) || !isBoundedInteger(consumedBytes, 0, 0xffffffff)
    || !isBoundedInteger(expectedBytes, 0, 0xffffffff) || !isBoundedInteger(decodedBytes, 0, 0xffffffff)) return null;
  return { phase, stream, width, height, payloadBytes, consumedBytes, expectedBytes, decodedBytes };
}

/** Validate IPC data and discard extra properties; no raw exception text enters
 * native connection diagnostics, including for malformed or unknown failures. */
export function readNativeRfbFailure(value: unknown): NativeRfbFailure | null {
  if (value instanceof NativeRfbError) return readNativeRfbFailure(value.failure);
  if (typeof value !== "object" || value === null
    || !("code" in value) || !("category" in value) || !("retryable" in value)
    || !isFailureCode(value.code)) return null;
  const expected = isProtocolFailureCode(value.code)
    ? { category: "protocol_error", retryable: false } as const
    : failures[value.code];
  if (value.category !== expected.category || value.retryable !== expected.retryable) return null;
  const diagnostic = value.code === "rfb_tight_decoded_length_mismatch" && "diagnostic" in value
    ? readTightLengthDiagnostic(value.diagnostic) : null;
  return { code: value.code, ...expected, ...(diagnostic ? { diagnostic } : {}) };
}

/** Keep fixed numeric Tight evidence in the existing connection failure log. */
export function formatNativeRfbFailure(failure: NativeRfbFailure): string {
  const checked = readNativeRfbFailure(failure);
  if (!checked) return "rfb_internal_error";
  const diagnostic = checked.diagnostic;
  if (!diagnostic) return checked.code;
  return `${checked.code}; tight_length phase=${diagnostic.phase} stream=${diagnostic.stream}`
    + ` width=${diagnostic.width} height=${diagnostic.height} payload_bytes=${diagnostic.payloadBytes}`
    + ` consumed_bytes=${diagnostic.consumedBytes} expected_bytes=${diagnostic.expectedBytes}`
    + ` decoded_bytes=${diagnostic.decodedBytes}`;
}

export function nativeRfbError(value: unknown): NativeRfbError {
  return new NativeRfbError(readNativeRfbFailure(value) ?? {
    code: "rfb_internal_error", ...failures.rfb_internal_error,
  });
}
