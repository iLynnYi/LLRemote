import assert from "node:assert/strict";
import test from "node:test";
import { createRemoteSession } from "../../../src/session/core/sessionManager.ts";
import { summarizeRemoteViewport } from "../../../src/session/viewport/remoteViewport.ts";
import { summarizeSessionDiagnosticsPanel } from "../../../src/session/diagnostics/sessionDiagnosticsPanel.ts";
import { buildSessionDiagnosticView } from "../../../src/session/diagnostics/sessionDiagnosticView.ts";
import type { EventLogEntry } from "../../../src/diagnostics/eventLog.ts";

test("diagnostic overview uses Chinese messages while the technical report retains redacted detail", () => {
  const session = {
    ...createRemoteSession(
      { protocol: "vnc", host: "192.168.85.128", port: 5904, qualityProfile: "balanced" },
      { id: () => "session-a", now: () => new Date("2026-11-01T10:00:00Z") },
    ),
    state: "connected" as const,
    connectedAt: "2026-11-01T10:00:00Z",
  };
  const tab = {
    id: session.id, label: session.target, target: session.target, state: session.state,
    active: true, liveCanvas: true, closable: true,
  };
  const viewport = summarizeRemoteViewport(tab, session);
  const events: EventLogEntry[] = [
    { id: "warning", time: "18:00:09", level: "warning", source: "会话", message: "fallback; password=secret", userMessage: "增强画面暂不可用，已回退到 VNC 画面。", origin: "conversation", conversationId: "session-a" },
    { id: "ready", time: "18:00:08", level: "success", source: "会话", message: "Native RFB Core transport ready; transport=native:internal-id", userMessage: "已建立 VNC 连接", eventKey: "vnc_connected", origin: "conversation", conversationId: "session-a" },
    { id: "other", time: "18:00:08", level: "error", source: "会话", message: "另一会话失败", origin: "conversation", conversationId: "session-b", target: session.target },
    { id: "routine", time: "18:00:07", level: "info", source: "connection-model", message: "Connection model agent: identity_id=internal-id", origin: "conversation", conversationId: "session-a" },
    { id: "warning-old", time: "18:00:06", level: "warning", source: "会话", message: "fallback again", userMessage: "增强画面暂不可用，已回退到 VNC 画面。", origin: "conversation", conversationId: "session-a" },
    { id: "discovery", time: "18:00:05", level: "warning", source: "discovery", message: "Server Discovery unavailable", userMessage: "增强能力发现失败，基础 VNC 连接不受影响。", eventKey: "discovery", origin: "conversation", conversationId: "session-a" },
  ];
  const summary = summarizeSessionDiagnosticsPanel({ viewport, events });
  const view = buildSessionDiagnosticView({ session, viewport, summary, events, nowMs: Date.parse("2026-11-01T10:00:10Z"), t: (key) => key === "connection.connected" ? "已连接" : key });

  assert.equal(view.duration, "00:00:10");
  assert.equal(view.enhanced, "未启用（可选）");
  assert.equal(view.stateText, "已连接");
  assert.deepEqual(view.userEvents.map((event) => event.id), ["warning", "ready", "discovery"]);
  assert.equal(view.userEvents[0].count, 2);
  assert.deepEqual(view.recentEvents.map((event) => event.id), ["warning", "ready", "routine", "warning-old", "discovery"]);
  assert.match(view.summaryReport, /已建立 VNC 连接/);
  assert.match(view.summaryReport, /增强能力发现失败，基础 VNC 连接不受影响/);
  assert.match(view.summaryReport, /重复 2 次/);
  assert.doesNotMatch(view.summaryReport, /Native RFB Core|Connection model|Server Discovery|internal-id|password|另一会话失败/);
  assert.match(view.technicalReport, /password=\[redacted\]/);
  assert.match(view.technicalReport, /Connection model agent/);
  assert.doesNotMatch(view.technicalReport, /secret|另一会话失败/);

  const olderEvents: EventLogEntry[] = Array.from({ length: 35 }, (_, index) => ({
    id: `older-${index}`, time: "17:59:59", level: "info", source: "rfb-performance",
    message: `rfb_metrics sample=${index}`, origin: "conversation", conversationId: "session-a",
  }));
  const fullView = buildSessionDiagnosticView({
    session, viewport, summary, events: [...events, ...olderEvents],
    nowMs: Date.parse("2026-11-01T10:00:10Z"), t: (key) => key === "connection.connected" ? "已连接" : key,
  });
  assert.equal(fullView.recentEvents.length, 40);
  assert.match(fullView.technicalReport, /rfb_metrics sample=34/);
  assert.deepEqual(fullView.userEvents.map((event) => event.id), ["warning", "ready", "discovery"]);
});
