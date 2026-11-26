import assert from "node:assert/strict";
import test from "node:test";
import { summarizeRemoteViewport } from "../../../src/session/viewport/remoteViewport.ts";
import { summarizeRemoteViewportStatusHud } from "../../../src/session/viewport/remoteViewportStatusBar.ts";

import {
  createSessionManager,
  createRemoteSession,
  summarizeSessionTabs,
  type RemoteSessionConfig,
} from "../../../src/session/core/sessionManager.ts";

const baseConfig: RemoteSessionConfig = {
  protocol: "vnc",
  host: "192.168.2.2",
  port: 5901,
  qualityProfile: "balanced",
};

test("createRemoteSession creates an idle VNC session with a stable target", () => {
  const session = createRemoteSession(baseConfig, {
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => "session-1",
  });

  assert.equal(session.id, "session-1");
  assert.equal(session.state, "idle");
  assert.equal(session.target, "192.168.2.2:5901");
  assert.equal(session.createdAt, "2026-06-28T12:00:00.000Z");
  assert.equal(session.qualityProfile, "balanced");
});

test("session manager creates one active session and records state transitions", () => {
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => "session-1",
  });

  const session = manager.create(baseConfig);

  manager.transition(session.id, "probing");
  manager.transition(session.id, "connecting");
  manager.transition(session.id, "connected");

  assert.equal(manager.activeSession?.id, "session-1");
  assert.equal(manager.activeSession?.state, "connected");
  assert.deepEqual(manager.activeSession?.stateHistory.map((entry) => entry.state), [
    "idle",
    "probing",
    "connecting",
    "connected",
  ]);
});

test("session manager attaches native Core transport ownership to a session", () => {
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => "session-1",
  });

  const session = manager.create(baseConfig);
  const updated = manager.attachTransport(session.id, {
    id: "transport-1",
    surfaceUrl: "ws://127.0.0.1:61001/surface",
  });

  assert.equal(updated?.transport?.id, "transport-1");
  assert.equal(updated?.transport?.surfaceUrl, "ws://127.0.0.1:61001/surface");
});

test("session manager updates quality profile for one session without changing other sessions", () => {
  const ids = ["session-1", "session-2"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  const first = manager.create(baseConfig);
  const second = manager.create({ ...baseConfig, host: "10.0.0.8" });

  const updated = manager.updateQualityProfile(first.id, "sharp");

  assert.equal(updated?.qualityProfile, "sharp");
  assert.equal(first.qualityProfile, "sharp");
  assert.equal(second.qualityProfile, "balanced");
  assert.equal(manager.activeSession?.id, second.id);
});

test("session manager clears native Core transport ownership when a session disconnects or closes", () => {
  const ids = ["session-1", "session-2"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  const disconnected = manager.create(baseConfig);
  manager.attachTransport(disconnected.id, {
    id: "transport-1",
    surfaceUrl: "ws://127.0.0.1:61001/surface",
  });

  manager.transition(disconnected.id, "disconnected");

  const closed = manager.create({ ...baseConfig, host: "10.0.0.8" });
  manager.attachTransport(closed.id, {
    id: "transport-2",
    surfaceUrl: "ws://127.0.0.1:61002/surface",
  });

  manager.close(closed.id);

  assert.equal(disconnected.transport, undefined);
  assert.equal(closed.transport, undefined);
});

test("session manager records connected and closed timestamps", () => {
  const times = [
    new Date("2026-06-28T12:00:00.000Z"),
    new Date("2026-06-28T12:00:05.000Z"),
    new Date("2026-06-28T12:03:00.000Z"),
  ];
  const manager = createSessionManager({
    now: () => times.shift() ?? new Date("2026-06-28T12:10:00.000Z"),
    id: () => "session-1",
  });

  const session = manager.create(baseConfig);

  manager.transition(session.id, "connected");
  manager.closeActive();

  assert.equal(session.connectedAt, "2026-06-28T12:00:05.000Z");
  assert.equal(session.closedAt, "2026-06-28T12:03:00.000Z");
});

test("session manager records failures without leaking sensitive values", () => {
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => "session-1",
  });

  const session = manager.create(baseConfig);
  const failed = manager.fail(session.id, "connection refused password=secret");

  assert.equal(failed?.state, "failed");
  assert.equal(failed?.lastError, "connection refused password=[redacted]");
});

test("session manager records structured connection issues and disconnect reasons", () => {
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => "session-1",
  });

  const session = manager.create(baseConfig);
  const failed = manager.fail(session.id, {
    code: "service_unavailable",
    phase: "tcpConnecting",
    title: "VNC service unavailable",
    messageKey: "connectionErrors.service_unavailable",
    retryable: true,
    autoReconnect: "never",
    logLevel: "error",
    technicalMessage: "connection refused password=secret",
  });

  assert.equal(failed?.lastIssue?.code, "service_unavailable");
  assert.equal(failed?.lastIssue?.technicalMessage, "connection refused password=[redacted]");
  assert.equal(failed?.lastError, "connectionErrors.service_unavailable");

  const disconnected = manager.transition(session.id, "disconnected", {
    disconnectReason: "idle_timeout",
  });

  assert.equal(disconnected?.disconnectReason, "idle_timeout");
});

test("session manager keeps previous sessions open when creating another active session", () => {
  const ids = ["session-1", "session-2"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  const first = manager.create(baseConfig);
  const second = manager.create({ ...baseConfig, host: "10.0.0.8" });

  assert.equal(first.state, "idle");
  assert.equal(second.state, "idle");
  assert.equal(manager.activeSession?.id, "session-2");
  assert.equal(manager.sessions.length, 2);
});

test("session manager can activate an existing open session", () => {
  const ids = ["session-1", "session-2"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  const first = manager.create(baseConfig);
  manager.create({ ...baseConfig, host: "10.0.0.8" });

  const active = manager.activate(first.id);

  assert.equal(active?.id, "session-1");
  assert.equal(manager.activeSession?.id, "session-1");
});

test("session manager finds a reusable session for an already open target", () => {
  const ids = ["session-1", "session-2", "session-3"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  const first = manager.create(baseConfig);
  assert.equal(manager.findReusableByTarget(baseConfig), undefined);
  manager.transition(first.id, "connected");
  manager.create({ ...baseConfig, host: "10.0.0.8" });

  assert.equal(manager.findReusableByTarget(baseConfig)?.id, first.id);

  manager.transition(first.id, "disconnected");
  assert.equal(manager.findReusableByTarget(baseConfig), undefined);
});

test("session manager finds a recoverable failed session for a manual retry target", () => {
  const ids = ["session-1", "session-2", "session-3"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  const failed = manager.create(baseConfig);
  manager.transition(failed.id, "authentication failed");
  manager.create({ ...baseConfig, host: "10.0.0.8" });

  assert.equal(manager.findRecoverableByTarget(baseConfig)?.id, failed.id);
  assert.equal(manager.findRecoverableByTarget({ ...baseConfig, host: "10.0.0.9" }), undefined);
});

test("session manager removes a specific session tab and activates a remaining session", () => {
  const ids = ["session-1", "session-2"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  const first = manager.create(baseConfig);
  const second = manager.create({ ...baseConfig, host: "10.0.0.8" });

  const closed = manager.close(second.id);

  assert.equal(closed?.id, second.id);
  assert.equal(manager.activeSession?.id, first.id);
  assert.deepEqual(manager.sessions.map((session) => session.id), [first.id]);
});

test("session manager removes the last session tab and clears active session", () => {
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => "session-1",
  });

  const session = manager.create(baseConfig);

  manager.close(session.id);

  assert.equal(manager.activeSession, null);
  assert.deepEqual(manager.sessions, []);
});

test("summarizeSessionTabs returns compact tab records with active and live-canvas state", () => {
  const ids = ["session-1", "session-2"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  manager.create(baseConfig);
  const second = manager.create({ ...baseConfig, host: "10.0.0.8", port: 5900 });
  manager.transition(second.id, "connected");

  assert.deepEqual(summarizeSessionTabs(manager.sessions, manager.activeSession?.id, second.id), [
    {
      id: "session-1",
      label: "192.168.2.2:5901",
      railLabel: "192.168.2.2:5901",
      target: "192.168.2.2:5901",
      state: "idle",
      active: false,
      liveCanvas: false,
      closable: true,
    },
    {
      id: "session-2",
      label: "10.0.0.8:5900",
      railLabel: "10.0.0.8:5900",
      target: "10.0.0.8:5900",
      state: "connected",
      active: true,
      liveCanvas: true,
      closable: true,
    },
  ]);
});

test("saved-host names affect only the session rail and leave the detail card unchanged", () => {
  const saved = createRemoteSession({ ...baseConfig, savedHostName: "办公室电脑" });
  const temporary = createRemoteSession(baseConfig);
  const unnamed = createRemoteSession({ ...baseConfig, savedHostName: "  " });
  const tabs = summarizeSessionTabs([saved, temporary, unnamed]);
  assert.deepEqual(tabs.map(tab => tab.railLabel), ["办公室电脑", saved.target, saved.target]);
  for (const tab of tabs) {
    assert.equal(tab.label, saved.target);
    assert.equal(tab.target, saved.target);
    const hud = summarizeRemoteViewportStatusHud({
      viewport: summarizeRemoteViewport(tab), events: [], nowMs: 0,
    });
    assert.equal(hud.title, baseConfig.host);
    assert.equal(hud.target, saved.target);
  }
});

test("summarizeSessionTabs supports multiple live canvas sessions", () => {
  const ids = ["session-1", "session-2"];
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => ids.shift() ?? "session-fallback",
  });

  manager.create(baseConfig);
  manager.create({ ...baseConfig, host: "10.0.0.8", port: 5900 });

  assert.deepEqual(
    summarizeSessionTabs(manager.sessions, manager.activeSession?.id, ["session-1", "session-2"]).map((tab) => ({
      id: tab.id,
      liveCanvas: tab.liveCanvas,
    })),
    [
      { id: "session-1", liveCanvas: true },
      { id: "session-2", liveCanvas: true },
    ],
  );
});

test("session manager ignores stale state transitions for missing sessions", () => {
  const manager = createSessionManager({
    now: () => new Date("2026-06-28T12:00:00.000Z"),
    id: () => "session-1",
  });

  assert.equal(manager.transition("missing", "connected"), undefined);
});
