import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { summarizeClipboard } from "../../../src/session/diagnostics/sessionDiagnosticsPanel/clipboard.ts";
import { summarizeSessionDiagnosticsPanel } from "../../../src/session/diagnostics/sessionDiagnosticsPanel.ts";

test("diagnostics panel exposes native Core transport and Surface evidence", () => {
  const summary = summarizeSessionDiagnosticsPanel({
    events: [],
    viewport: {
      id: "session-1", title: "10.0.0.8:5900", target: "10.0.0.8:5900", state: "connected", statusLabel: "connected",
      connectionModel: {
        baseTransport: { kind: "vnc" }, serverIdentity: { kind: "unknown", verification: "none" }, localEndpointMatch: { state: "not-matched" },
        enhancedChannel: { status: "unavailable", authentication: "not-attempted" }, capabilities: { items: [] },
      },
      displaySource: "vnc", inputSource: "vnc", tone: "connected", inputMode: "control", inputPolicy: "control",
      detail: "transport=native:session-1", transportLabel: "native:session-1", canRetry: false, canDisconnect: true, canClose: true,
      canFullscreen: true, canToggleInputMode: true,
    },
    runtimeEvidence: {
      canvasCount: 1,
      canvasSizes: [{ width: 1280, height: 720 }],
      framebuffer: { width: 1280, height: 720 },
      transportId: "native:session-1",
      latestCoreEvent: "surface_frame:42",
      latestCoreEventAtMs: 1782813600000,
      capturedAtMs: 1782813601000,
      displaySource: "core_framebuffer",
      inputPath: "native_rfb_input",
    },
  });
  assert.equal(summary.runtimeEvidence.transportId, "native:session-1");
  assert.equal(summary.runtimeEvidence.latestCoreEvent, "surface_frame:42");
  assert.equal(summary.runtimeEvidence.displaySource, "core_framebuffer");
  assert.equal(summary.runtimeEvidence.inputPath, "native_rfb_input");
});

test("diagnostics component renders Core transport and Surface fields", () => {
  const source = readFileSync(new URL("../../../src/features/sessions/components/SessionDeveloperDiagnostics.vue", import.meta.url), "utf8");
  assert.match(source, /runtimeEvidence\.transportId/);
  assert.match(source, /runtimeEvidence\.latestCoreEvent/);
  assert.match(source, /runtimeEvidence\.surface/);
  assert.match(source, /remoteSessionCore\.activeSurfaceOwner/);
});

test("clipboard diagnostics retain metadata and discard text from legacy activity objects", () => {
  const marker = crypto.randomUUID();
  const activity = {
    direction: "client-to-server" as const, atMs: 1, text: marker, accepted: true,
    reason: "manual-sent", mode: "bidirectional-active" as const, bytes: 42, channel: "rfb" as const,
  };
  const summary = summarizeClipboard({
    mode: "bidirectional-active", enabled: true, readonly: false, clientToServer: activity, serverToClient: null,
  });
  assert.equal(summary.clientToServer?.bytes, "42");
  assert.equal(summary.clientToServer?.channel, "rfb");
  assert.equal(summary.clientToServer?.statusText, "已发送");
  assert.equal(JSON.stringify(summary).includes(marker), false);
});
