import assert from "node:assert/strict";
import test from "node:test";
import { classifyConnectionFailure, createConnectionIssue } from "../../../src/session/diagnostics/sessionDiagnostics.ts";
import { nativeRfbError, readNativeRfbFailure } from "../../../src/session/protocol/rfbFailure.ts";

test("native authentication failures use the category rather than raw text", () => {
  const failure = nativeRfbError({ category: "authentication_failed", code: "rfb_security_failed", retryable: true });
  const parsed = readNativeRfbFailure(failure);
  assert.ok(parsed);
  assert.equal(classifyConnectionFailure(parsed).code, "authentication_failed");
  const issue = createConnectionIssue({ reason: parsed, phase: "authenticating" });
  assert.equal(issue.messageKey, "connectionErrors.authentication_failed");
  assert.equal(issue.autoReconnect, "never");
  assert.equal(issue.technicalMessage, "rfb_security_failed");
});

test("resource limits are localized and never automatically retried", () => {
  const failure = readNativeRfbFailure({ category: "resource_limit", code: "rfb_resource_limit", retryable: false });
  assert.ok(failure);
  const issue = createConnectionIssue({ reason: failure });
  assert.equal(issue.code, "resource_limit");
  assert.equal(issue.messageKey, "connectionErrors.resource_limit");
  assert.equal(issue.retryable, false);
  assert.equal(issue.autoReconnect, "never");
});

test("native unknown failures and extra payload fields never expose raw input", () => {
  const privateText = crypto.randomUUID();
  const failure = nativeRfbError(new Error(privateText));
  assert.equal(failure.message, "rfb_internal_error");
  assert.ok(!JSON.stringify(failure).includes(privateText));
  const parsed = readNativeRfbFailure({ category: "remote_closed", code: "rfb_remote_closed", retryable: true, raw: privateText });
  assert.deepEqual(parsed, { category: "remote_closed", code: "rfb_remote_closed", retryable: true });
});

test("malformed native failure objects and raw credential strings cannot request a prompt", () => {
  for (const value of [null, "rfb_credentials_required", { code: "rfb_credentials_required" },
    { category: "credentials_required", code: "rfb_credentials_required", retryable: false },
    { category: "credentials_required", code: "rfb_remote_closed", retryable: true }]) {
    assert.equal(readNativeRfbFailure(value), null);
  }
});

test("transport operation and known IO kind survive while raw OS details are discarded", () => {
  const privateText = crypto.randomUUID();
  for (const code of ["rfb_tcp_read_reset", "rfb_tcp_read_aborted", "rfb_tcp_read_failed",
    "rfb_tcp_write_timeout", "rfb_tcp_write_reset", "rfb_tcp_write_broken_pipe",
    "rfb_tcp_write_failed", "rfb_tcp_flush_failed"]) {
    const failure = readNativeRfbFailure({ category: "target_unreachable", code, retryable: true, raw: privateText });
    assert.ok(failure);
    const issue = createConnectionIssue({ reason: failure });
    assert.equal(issue.technicalMessage, code);
    assert.equal(issue.messageKey, "connectionErrors.target_unreachable");
    assert.ok(!JSON.stringify(issue).includes(privateText));
  }
  assert.equal(readNativeRfbFailure({ category: "target_unreachable", code: `rfb_tcp_read_${privateText}`, retryable: true }), null);
});

test("known protocol diagnostic codes preserve localized copy and discard extra details", () => {
  const privateText = crypto.randomUUID();
  for (const code of ["rfb_desktop_name_not_utf8", "rfb_tight_rectangle_width_limit",
    "rfb_tight_invalid_palette", "rfb_tight_decoded_length_mismatch", "rfb_zlib_decompression_failed",
    "rfb_hextile_background_missing", "rfb_hextile_foreground_missing", "rfb_hextile_invalid_flags",
    "rfb_framebuffer_update_truncated", "rfb_unsupported_server_message_type"]) {
    const failure = readNativeRfbFailure({ category: "protocol_error", code, retryable: false, raw: privateText });
    assert.ok(failure);
    const issue = createConnectionIssue({ reason: failure });
    assert.equal(issue.technicalMessage, code);
    assert.equal(issue.messageKey, "connectionErrors.protocol_error");
    assert.equal(issue.retryable, false);
    assert.ok(!JSON.stringify(issue).includes(privateText));
  }
  assert.equal(readNativeRfbFailure({ category: "protocol_error", code: `rfb_unknown_${privateText}`, retryable: false }), null);
});

test("Tight length diagnostics keep only bounded numeric evidence through the failure log", () => {
  const privateText = crypto.randomUUID();
  const base = { category: "protocol_error", code: "rfb_tight_decoded_length_mismatch", retryable: false };
  const diagnostic = {
    phase: "inflate", stream: 3, width: 65535, height: 1,
    payloadBytes: 0xffffffff, consumedBytes: 0, expectedBytes: 196605, decodedBytes: 196604,
  };
  const parsed = readNativeRfbFailure(nativeRfbError({
    ...base, diagnostic: { ...diagnostic, raw: privateText }, raw: privateText,
  }));
  assert.deepEqual(parsed, { ...base, diagnostic });
  assert.ok(parsed);
  const issue = createConnectionIssue({ reason: parsed, phase: "disconnected" });
  assert.equal(issue.technicalMessage, "rfb_tight_decoded_length_mismatch; tight_length phase=inflate stream=3 width=65535 height=1 payload_bytes=4294967295 consumed_bytes=0 expected_bytes=196605 decoded_bytes=196604");
  assert.equal(issue.messageKey, "connectionErrors.protocol_error");
  assert.equal(issue.retryable, false);
  assert.equal(issue.autoReconnect, "never");
  assert.ok(!JSON.stringify(issue).includes(privateText));

  for (const invalid of [null, privateText, { ...diagnostic, phase: privateText },
    { ...diagnostic, stream: 4 }, { ...diagnostic, width: 0 }, { ...diagnostic, height: 65536 },
    { ...diagnostic, payloadBytes: -1 }, { ...diagnostic, consumedBytes: "0" },
    { ...diagnostic, expectedBytes: 0.5 }, { ...diagnostic, decodedBytes: Infinity },
    { ...diagnostic, decodedBytes: 0x100000000 }, { ...diagnostic, decodedBytes: undefined }]) {
    const failure = readNativeRfbFailure({ ...base, diagnostic: invalid });
    assert.deepEqual(failure, base);
    assert.ok(failure);
    assert.equal(createConnectionIssue({ reason: failure }).technicalMessage, base.code);
    assert.ok(!JSON.stringify(failure).includes(privateText));
  }
  assert.deepEqual(readNativeRfbFailure({ ...base, code: "rfb_protocol_error", diagnostic }), {
    ...base, code: "rfb_protocol_error",
  });
});
