import assert from "node:assert/strict";
import test from "node:test";
import {
  summarizeRemoteViewport,
  type RemoteViewportSession,
} from "../../../src/session/viewport/remoteViewport.ts";
import type { RemoteSessionTab } from "../../../src/session/core/sessionManager.ts";

const baseTab: RemoteSessionTab = {
  id: "session-1",
  label: "10.0.0.8:5900",
  target: "10.0.0.8:5900",
  state: "idle",
  active: true,
  liveCanvas: false,
  closable: true,
};

const baseSession: RemoteViewportSession = {
  id: "session-1",
  target: "10.0.0.8:5900",
  state: "idle",
};

test("remote viewport summary makes failed sessions recoverable without disconnect actions", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "authentication failed",
    },
    {
      ...baseSession,
      state: "authentication failed",
      lastError: "failure_code=authentication_failed; password=[redacted]",
    },
  );

  assert.equal(summary.tone, "failed");
  assert.equal(summary.canRetry, true);
  assert.equal(summary.canDisconnect, false);
  assert.equal(summary.canClose, true);
  assert.equal(summary.canFullscreen, false);
  assert.equal(summary.inputMode, "locked");
  assert.equal(summary.detail, "failure_code=authentication_failed; password=[redacted]");
});

test("remote viewport summary displays the prioritized issue over a stale session state", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "authentication failed",
    },
    {
      ...baseSession,
      state: "authentication failed",
      lastIssue: {
        code: "service_unavailable",
        phase: "tcpConnecting",
        title: "VNC service unavailable",
        messageKey: "connectionErrors.service_unavailable",
        message: "未检测到可用的 VNC 服务，请确认 VNC Server 已启动并监听该端口。",
        retryable: true,
        autoReconnect: "never",
        logLevel: "error",
      },
    },
  );

  assert.equal(summary.state, "authentication failed");
  assert.equal(summary.statusLabel, "service unavailable");
  assert.equal(summary.tone, "failed");
  assert.equal(summary.canRetry, true);
});

test("remote viewport summary exposes target unreachable as a failed issue status", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "failed",
    },
    {
      ...baseSession,
      state: "failed",
      lastIssue: {
        code: "target_unreachable",
        phase: "tcpConnecting",
        title: "Target unreachable",
        messageKey: "connectionErrors.target_unreachable",
        message: "无法连接到目标主机，请检查主机地址、端口、网络、VPN 或防火墙。",
        retryable: true,
        autoReconnect: "never",
        logLevel: "error",
      },
    },
  );

  assert.equal(summary.statusLabel, "target unreachable");
  assert.equal(summary.tone, "failed");
  assert.equal(summary.canRetry, true);
});

test("remote viewport summary exposes connected session controls and input mode", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "connected",
      liveCanvas: true,
    },
    {
      ...baseSession,
      state: "connected",
      transport: {
        id: "transport-1",
        surfaceUrl: "ws://127.0.0.1:49152/surface",
      },
      connectedAt: "2026-06-30T10:00:00.000Z",
    },
  );

  assert.equal(summary.tone, "connected");
  assert.equal(summary.canRetry, false);
  assert.equal(summary.canDisconnect, true);
  assert.equal(summary.canClose, true);
  assert.equal(summary.canFullscreen, true);
  assert.equal(summary.inputMode, "control");
  assert.equal(summary.transportLabel, "transport-1");
});

test("remote viewport summary carries the unified connection model", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "connected",
      liveCanvas: true,
    },
    {
      ...baseSession,
      state: "connected",
      connectionModel: {
        baseTransport: { kind: "vnc" },
        serverIdentity: { kind: "managed", verification: "authenticated-agent" },
        localEndpointMatch: { state: "not-matched" },
        enhancedChannel: { status: "available", authentication: "verified", endpoint: { hostMode: "rfb-peer", port: 6901, source: "server-discovery" } },
        capabilities: { items: [{ id: "clipboard.text.write", state: "supported", source: "agent" }] },
      },
    },
  );

  assert.equal(summary.connectionModel.baseTransport.kind, "vnc");
  assert.equal(summary.connectionModel.enhancedChannel.status, "available");
  assert.equal(summary.connectionModel.capabilities.items[0]?.id, "clipboard.text.write");
});

test("remote viewport summary defaults to VNC base transport with unavailable sidecar", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "connected",
      liveCanvas: true,
    },
    {
      ...baseSession,
      state: "connected",
    },
  );

  assert.equal(summary.connectionModel.baseTransport.kind, "vnc");
  assert.equal(summary.connectionModel.enhancedChannel.status, "unavailable");
  assert.deepEqual(summary.connectionModel.capabilities.items, []);
});

test("remote viewport summary exposes read-only connected sessions as non-controllable", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "connected",
      liveCanvas: true,
    },
    {
      ...baseSession,
      state: "connected",
      readonly: true,
      transport: {
        id: "transport-1",
        surfaceUrl: "ws://127.0.0.1:49152/surface",
      },
    },
  );

  assert.equal(summary.tone, "connected");
  assert.equal(summary.inputMode, "readonly");
  assert.equal(summary.canToggleInputMode, true);
});

test("remote viewport summary treats view-only shared sessions as a locked read-only policy", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "connected",
      liveCanvas: true,
    },
    {
      ...baseSession,
      state: "connected",
      readonly: true,
      viewOnlyShared: true,
    },
  );

  assert.equal(summary.inputMode, "readonly");
  assert.equal(summary.inputPolicy, "viewOnlyShared");
  assert.equal(summary.canToggleInputMode, false);
});

test("remote viewport summary locks input for non-selected connected sessions", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "connected",
      active: false,
      liveCanvas: true,
    },
    {
      ...baseSession,
      state: "connected",
      transport: {
        id: "transport-1",
        surfaceUrl: "ws://127.0.0.1:49152/surface",
      },
    },
  );

  assert.equal(summary.tone, "connected");
  assert.equal(summary.inputMode, "locked");
  assert.equal(summary.canDisconnect, true);
});

test("remote viewport summary marks connecting sessions as busy and input locked", () => {
  const summary = summarizeRemoteViewport(
    {
      ...baseTab,
      state: "connecting",
    },
    {
      ...baseSession,
      state: "connecting",
    },
  );

  assert.equal(summary.tone, "working");
  assert.equal(summary.canRetry, false);
  assert.equal(summary.canDisconnect, true);
  assert.equal(summary.canClose, true);
  assert.equal(summary.canFullscreen, false);
  assert.equal(summary.inputMode, "locked");
});
