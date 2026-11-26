import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyConnectionFailure,
  createConnectionIssue,
  createDisconnectIssue,
  elapsedRemoteSessionSeconds,
  formatDuration,
  formatStabilityCheckpointEvent,
  stabilityLabel,
  shouldIgnoreFollowupRemoteDisconnectIssue,
  shouldReplaceConnectionIssue,
  shouldRecordStabilityCheckpoint,
} from "../../../src/session/diagnostics/sessionDiagnostics.ts";

test("formatDuration renders seconds as hh:mm:ss", () => {
  assert.equal(formatDuration(0), "00:00:00");
  assert.equal(formatDuration(65), "00:01:05");
  assert.equal(formatDuration(3661), "01:01:01");
});

test("elapsedRemoteSessionSeconds uses the active session timestamps", () => {
  assert.equal(elapsedRemoteSessionSeconds({
    connectedAt: "2026-06-28T12:00:00.000Z",
  }, Date.parse("2026-06-28T12:01:05.000Z")), 65);
  assert.equal(elapsedRemoteSessionSeconds({
    connectedAt: "2026-06-28T12:00:00.000Z",
    closedAt: "2026-06-28T12:03:00.000Z",
  }, Date.parse("2026-06-28T12:10:00.000Z")), 180);
  assert.equal(elapsedRemoteSessionSeconds(null, Date.parse("2026-06-28T12:10:00.000Z")), 0);
});

test("stabilityLabel marks sessions under 30 minutes as collecting evidence", () => {
  assert.equal(stabilityLabel(29 * 60 + 59), "collecting");
  assert.equal(stabilityLabel(30 * 60), "30m-ready");
});

test("shouldRecordStabilityCheckpoint only triggers when crossing 30 minutes once", () => {
  assert.equal(shouldRecordStabilityCheckpoint({
    previousSeconds: 29 * 60 + 59,
    currentSeconds: 30 * 60,
    alreadyRecorded: false,
  }), true);
  assert.equal(shouldRecordStabilityCheckpoint({
    previousSeconds: 30 * 60,
    currentSeconds: 30 * 60 + 1,
    alreadyRecorded: false,
  }), false);
  assert.equal(shouldRecordStabilityCheckpoint({
    previousSeconds: 29 * 60 + 59,
    currentSeconds: 30 * 60,
    alreadyRecorded: true,
  }), false);
});

test("formatStabilityCheckpointEvent creates a diagnostic-safe checkpoint message", () => {
  const message = formatStabilityCheckpointEvent({
    target: "192.168.2.2:5901",
    duration: "00:30:00",
  });

  assert.equal(
    message,
    "stability_check=30m_ready; target=192.168.2.2:5901; duration=00:30:00; verify framebuffer refresh, mouse input, keyboard input, disconnect, and reconnect.",
  );
});

test("classifyConnectionFailure identifies timeout errors", () => {
  assert.deepEqual(classifyConnectionFailure("operation timed out while opening socket"), {
    code: "target_unreachable",
    title: "Target unreachable",
    messageKey: "connectionErrors.target_unreachable",
  });
});

test("classifyConnectionFailure identifies unsupported security type errors", () => {
  assert.deepEqual(classifyConnectionFailure("unsupported security type 19"), {
    code: "unsupported_security",
    title: "Unsupported security type",
    messageKey: "connectionErrors.unsupported_security",
  });
});

test("classifyConnectionFailure identifies non-VNC and native transport failures", () => {
  assert.equal(classifyConnectionFailure("target is not a VNC service").code, "not_vnc_service");
  assert.equal(classifyConnectionFailure("rfb_tcp_connect_failed: connection refused").code, "service_unavailable");
  assert.equal(classifyConnectionFailure("connection refused").code, "service_unavailable");
  assert.equal(classifyConnectionFailure("Connection method auto requires a VNC service, but probe found unknown; reachable=false; error=tcp_unreachable.").code, "target_unreachable");
});

test("classifyConnectionFailure groups network and socket details by user action", () => {
  assert.equal(classifyConnectionFailure("dns resolution failed for lab-host.local").code, "target_unreachable");
  assert.equal(classifyConnectionFailure("failed to lookup address information: nodename nor servname provided").code, "target_unreachable");
  assert.equal(classifyConnectionFailure("network is unreachable").code, "target_unreachable");
  assert.equal(classifyConnectionFailure("no route to host").code, "target_unreachable");
  assert.equal(classifyConnectionFailure("connection reset by peer").code, "remote_closed");
  assert.equal(classifyConnectionFailure("socket closed before RFB handshake").code, "remote_closed");
  assert.equal(classifyConnectionFailure("server closed connection before framebuffer init").code, "remote_closed");
});

test("createConnectionIssue attaches phase retry policy and redacted technical details", () => {
  assert.deepEqual(createConnectionIssue({
    phase: "tcpConnecting",
    reason: "connection refused password=secret",
  }), {
    code: "service_unavailable",
    phase: "tcpConnecting",
    title: "VNC service unavailable",
    messageKey: "connectionErrors.service_unavailable",
    retryable: true,
    autoReconnect: "never",
    logLevel: "error",
    technicalMessage: "connection refused password=[redacted]",
  });
});

test("createDisconnectIssue distinguishes user and idle disconnect reasons", () => {
  assert.deepEqual(createDisconnectIssue("idle_timeout"), {
    code: "idle_timeout",
    phase: "disconnected",
    title: "Idle timeout",
    messageKey: "connectionDisconnectReasons.idle_timeout",
    retryable: true,
    autoReconnect: "never",
    logLevel: "none",
  });
  assert.equal(createDisconnectIssue("user_requested").messageKey, "connectionDisconnectReasons.user_requested");
});

test("classifyConnectionFailure preserves authentication and security failure messages", () => {
  assert.deepEqual(classifyConnectionFailure("VNC authentication failed. Check the password."), {
    code: "authentication_failed",
    title: "Authentication failed",
    messageKey: "connectionErrors.authentication_failed",
  });
  assert.deepEqual(classifyConnectionFailure("VNC security negotiation failed."), {
    code: "security_failure",
    title: "Security negotiation failed",
    messageKey: "connectionErrors.security_failure",
  });
  assert.equal(classifyConnectionFailure("connectionErrors.authentication_failed").code, "authentication_failed");
});

test("classifyConnectionFailure distinguishes missing credentials from wrong passwords", () => {
  assert.deepEqual(classifyConnectionFailure("VNC server requires credentials. missing password credential."), {
    code: "credentials_required",
    title: "Password required",
    messageKey: "connectionErrors.credentials_required",
  });
  assert.equal(classifyConnectionFailure("rfb_credentials_required").code, "credentials_required");
});

test("follow-up remote disconnect does not replace a more specific failed issue", () => {
  const authenticationIssue = createConnectionIssue({
    phase: "authenticating",
    reason: "VNC authentication failed. Check the password.",
  });
  const remoteDisconnectIssue = createDisconnectIssue("remote_closed", "RFB session disconnected unexpectedly.");

  assert.equal(shouldIgnoreFollowupRemoteDisconnectIssue({
    state: "authentication failed",
    lastIssue: authenticationIssue,
    incomingIssue: remoteDisconnectIssue,
  }), true);
  assert.equal(shouldIgnoreFollowupRemoteDisconnectIssue({
    state: "connected",
    lastIssue: undefined,
    incomingIssue: remoteDisconnectIssue,
  }), false);
});

test("connection issue priority keeps specific failures over later generic disconnects", () => {
  const authenticationIssue = createConnectionIssue({
    phase: "authenticating",
    reason: "VNC authentication failed. Check the password.",
  });
  const remoteDisconnectIssue = createDisconnectIssue("remote_closed", "RFB session disconnected unexpectedly.");
  const targetIssue = createConnectionIssue({
    phase: "tcpConnecting",
    reason: "operation timed out while opening socket",
  });

  assert.equal(shouldReplaceConnectionIssue(authenticationIssue, remoteDisconnectIssue), false);
  assert.equal(shouldReplaceConnectionIssue(remoteDisconnectIssue, authenticationIssue), true);
  assert.equal(shouldReplaceConnectionIssue(targetIssue, remoteDisconnectIssue), false);
});

test("connection issue priority follows the user-facing failure type order", () => {
  const orderedIssues = [
    createConnectionIssue({ phase: "tcpConnecting", reason: "operation timed out while opening socket" }),
    createConnectionIssue({ phase: "protocolHandshake", reason: "unsupported protocol version" }),
    createConnectionIssue({ phase: "protocolHandshake", reason: "target is not a VNC service" }),
    createConnectionIssue({ phase: "securityNegotiating", reason: "unsupported security type 19" }),
    createConnectionIssue({ phase: "authenticating", reason: "VNC authentication failed. Check the password." }),
    createConnectionIssue({ phase: "connecting", reason: "session destroyed" }),
    createConnectionIssue({ phase: "connected", reason: "agent enhanced stream failed" }),
    createDisconnectIssue("remote_closed", "RFB session disconnected unexpectedly."),
    createConnectionIssue({ phase: "failed", reason: "unexpected VNC failure" }),
  ];

  for (let index = 0; index < orderedIssues.length - 1; index += 1) {
    const higherPriorityIssue = orderedIssues[index];
    const lowerPriorityIssue = orderedIssues[index + 1];

    assert.equal(shouldReplaceConnectionIssue(lowerPriorityIssue, higherPriorityIssue), true);
    assert.equal(shouldReplaceConnectionIssue(higherPriorityIssue, lowerPriorityIssue), false);
  }
});
