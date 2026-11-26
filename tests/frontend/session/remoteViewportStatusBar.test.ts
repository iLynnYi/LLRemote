import assert from "node:assert/strict";
import test from "node:test";
import { summarizeRemoteViewportStatusBar, summarizeRemoteViewportStatusHud } from "../../../src/session/viewport/remoteViewportStatusBar.ts";
import type { EventLogEntry } from "../../../src/diagnostics/eventLog.ts";
import type { RemoteViewportSummary } from "../../../src/session/viewport/remoteViewport.ts";

const viewport: RemoteViewportSummary = {
  id: "session-1", title: "10.0.0.8:5900", target: "10.0.0.8:5900", state: "connected", statusLabel: "connected",
  connectionModel: {
    baseTransport: { kind: "vnc" }, serverIdentity: { kind: "unknown", verification: "none" }, localEndpointMatch: { state: "not-matched" },
    enhancedChannel: { status: "unavailable", authentication: "not-attempted" }, capabilities: { items: [] },
  },
  displaySource: "vnc", inputSource: "vnc", tone: "connected", inputMode: "control", inputPolicy: "control",
  detail: "transport=transport-1", transportLabel: "transport-1", canRetry: false, canDisconnect: true, canClose: true, canFullscreen: true, canToggleInputMode: true,
};

const events: EventLogEntry[] = [
  { id: "event-1", time: "10:02:00", level: "success", source: "rfb-core", message: "Started transport-1 for 10.0.0.8:5900" },
  { id: "event-2", time: "10:01:00", level: "info", source: "session", message: "Activated session tab 10.0.0.9:5900." },
];

test("remote viewport status bar summarizes native transport and latest scoped event", () => {
  assert.deepEqual(summarizeRemoteViewportStatusBar({ viewport, session: { connectedAt: "2026-06-30T10:00:00.000Z" }, events, nowMs: Date.parse("2026-06-30T10:02:03.000Z") }), {
    state: "connected", duration: "00:02:03", transport: "transport-1", latestEvent: "rfb-core: Started transport-1 for 10.0.0.8:5900",
  });
});

test("remote viewport status bar uses stable placeholders without scoped diagnostics", () => {
  const summary = summarizeRemoteViewportStatusBar({
    viewport: { ...viewport, id: "session-3", target: "10.0.0.10:5900", transportLabel: "", state: "connecting", statusLabel: "connecting" },
    session: {}, events, nowMs: Date.parse("2026-06-30T10:02:03.000Z"),
  });
  assert.equal(summary.state, "connecting");
  assert.equal(summary.duration, "00:00:00");
  assert.equal(summary.transport, "-");
  assert.equal(summary.latestEvent, "-");
});

test("remote viewport status HUD keeps identity display and native transport in one summary", () => {
  assert.deepEqual(summarizeRemoteViewportStatusHud({
    viewport: { ...viewport, title: "Office Mac" }, session: { connectedAt: "2026-06-30T10:00:00.000Z" }, events,
    nowMs: Date.parse("2026-06-30T10:02:03.000Z"), display: { width: 828, height: 1792, label: "sessions.displaySource.vnc" },
  }), {
    title: "Office Mac", target: "10.0.0.8:5900", state: "connected", duration: "00:02:03", inputMode: "control",
    display: "828x1792", displaySource: "sessions.displaySource.vnc", transport: "transport-1",
    latestEvent: "rfb-core: Started transport-1 for 10.0.0.8:5900",
  });
});
