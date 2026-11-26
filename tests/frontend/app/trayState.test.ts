import assert from "node:assert/strict";
import test from "node:test";

import {
  createTrayStartFailureNotification,
  createTrayStateSnapshot,
  getTrayNotifications,
  refreshLocalShareTrayState,
  type TrayStateInput,
} from "../../../src/app/trayState.ts";
import type { LocalVncServerOverview } from "../../../src/server/localShare.ts";

function serverOverview(
  status: LocalVncServerOverview["status"],
  activeConnections = 0,
  deviceName = "MacBook Pro",
): LocalVncServerOverview {
  return {
    status,
    bindAddress: "0.0.0.0",
    port: 5903,
    addresses: [
      { label: "en0", address: "192.168.1.10", kind: "lan_candidate" },
    ],
    displays: [],
    selectedDisplayId: "main",
    selectedDisplayLabel: "Main",
    resolutionMode: "fit",
    refreshMode: "balanced",
    framebufferWidth: 1280,
    framebufferHeight: 800,
    securityType: "vnc_auth",
    viewOnly: false,
    activeConnections,
    activeConnectionDevices: activeConnections > 0
      ? [
        {
          id: "device-1",
          kind: "rfb",
          deviceName,
          platform: "macOS",
          ipAddress: "192.168.1.22",
          connectedAt: "1782813600",
          status: "online",
        },
      ]
      : [],
    permissions: [],
    agent: {
      status: status === "running" ? "running" : "stopped",
      bindAddress: "0.0.0.0",
      port: status === "running" ? 6903 : null,
      capabilities: [],
      lastError: null,
      updatedAt: "1782813601",
    },
    startedAt: status === "running" ? "1782813600" : null,
    updatedAt: "1782813601",
  };
}

function trayInput(patch: Partial<TrayStateInput> = {}): TrayStateInput {
  return {
    localVncServerOverview: null,
    activeSessionCount: 0,
    failedSessionCount: 0,
    canStartLocalShare: false,
    ...patch,
  };
}

test("tray state prioritizes local share state over remote session activity", () => {
  const snapshot = createTrayStateSnapshot(trayInput({
    localVncServerOverview: null,
    activeSessionCount: 3,
  }));

  assert.equal(snapshot.status, "idle");
  assert.equal(snapshot.iconState, "idle");
  assert.equal(snapshot.activeSessionCount, 3);
  assert.match(snapshot.tooltip, /远程会话：3 个/);
});

test("tray state exposes local share running with active connection count", () => {
  const snapshot = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("running", 2),
    activeSessionCount: 1,
  }));

  assert.equal(snapshot.status, "sharingActive");
  assert.equal(snapshot.iconState, "sharing-active-2");
  assert.equal(snapshot.localShareRunning, true);
  assert.equal(snapshot.activeLocalConnectionCount, 2);
  assert.deepEqual(snapshot.menuSummary, [
    "● 本机共享：已启动",
    "活动连接：2 台",
    "监听端口：5903",
    "远程会话：1 个",
  ]);
  assert.deepEqual(snapshot.activeLocalConnections, [
    {
      id: "device-1",
      label: "MacBook Pro",
      detail: "192.168.1.22",
    },
  ]);
});

test("tray state caps icon connection count while keeping exact menu count", () => {
  const snapshot = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("running", 12),
  }));

  assert.equal(snapshot.status, "sharingActive");
  assert.equal(snapshot.iconState, "sharing-active-9-plus");
  assert.match(snapshot.tooltip, /活动连接：12 台/);
  assert.equal(snapshot.menuSummary[1], "活动连接：12 台");
});

test("tray state marks local share service errors above active connections", () => {
  const snapshot = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("port_conflict", 2),
  }));

  assert.equal(snapshot.status, "error");
  assert.equal(snapshot.iconState, "error");
  assert.deepEqual(snapshot.menuSummary, [
    "! 本机共享：端口占用",
    "需要处理",
  ]);
});

test("tray action shows pending permission and password steps without changing runtime status", () => {
  for (const [action, label] of [
    ["awaiting_permission", "等待权限确认"],
    ["awaiting_password", "等待输入密码"],
  ] as const) {
    const snapshot = createTrayStateSnapshot(trayInput({ localShareAction: action }));
    assert.equal(snapshot.localShareStatus, "stopped");
    assert.equal(snapshot.localShareAction, action);
    assert.ok(snapshot.menuSummary.includes(label));
    assert.ok(snapshot.tooltip.includes(label));
  }
});

test("tray runtime transitions disable actions even when the frontend action is idle", () => {
  for (const status of ["starting", "stopping"] as const) {
    const snapshot = createTrayStateSnapshot(trayInput({
      localVncServerOverview: serverOverview(status),
      localShareAction: "idle",
    }));
    assert.equal(snapshot.localShareAction, status);
  }
});

test("tray start failure notification includes the concrete start error", () => {
  assert.deepEqual(createTrayStartFailureNotification("访问密码不能为空。"), {
    kind: "local-share-start-failed",
    title: "本机共享启动失败",
    body: "访问密码不能为空。",
  });
  assert.deepEqual(createTrayStartFailureNotification(""), {
    kind: "local-share-start-failed",
    title: "本机共享启动失败",
    body: "请打开本机共享查看配置和权限状态。",
  });
});

test("tray notifications include new device connections and local share errors", () => {
  const before = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("running", 0),
  }));
  const afterConnection = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("running", 1, "Ops Mac"),
  }));
  const portConflict = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("port_conflict", 0),
  }));
  const permissionMissing = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("permission_missing", 0),
  }));

  assert.deepEqual(getTrayNotifications(before, afterConnection, { intentionalStop: false }), [
    {
      kind: "local-share-connection",
      title: "新设备已连接",
      body: "Ops Mac 已连接本机共享。",
    },
  ]);
  assert.equal(getTrayNotifications(before, portConflict, { intentionalStop: false })[0]?.kind, "local-share-port-conflict");
  assert.equal(getTrayNotifications(before, permissionMissing, { intentionalStop: false })[0]?.kind, "local-share-permission-missing");
});

test("tray notifications suppress intentional stop but report unexpected stop", () => {
  const running = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("running", 0),
  }));
  const stopped = createTrayStateSnapshot(trayInput({
    localVncServerOverview: serverOverview("stopped", 0),
  }));

  assert.deepEqual(getTrayNotifications(running, stopped, { intentionalStop: true }), []);
  assert.equal(getTrayNotifications(running, stopped, { intentionalStop: false })[0]?.kind, "local-share-stopped-unexpectedly");
});

test("local share tray refresh syncs after the latest server status is loaded", async () => {
  const calls: string[] = [];

  await refreshLocalShareTrayState({
    refreshLocalVncServerStatus: async () => {
      calls.push("refresh:start");
      await Promise.resolve();
      calls.push("refresh:done");
    },
    syncTrayState: async () => {
      calls.push("sync");
    },
  });

  assert.deepEqual(calls, ["refresh:start", "refresh:done", "sync"]);
});
