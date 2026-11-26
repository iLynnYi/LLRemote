import assert from "node:assert/strict";
import test from "node:test";
import { createServerRuntime } from "../../../src/session/protocol/protocolAdapter/serverRuntime.ts";
import { createExternalDesktopRecovery } from "../../../src/session/external-window/sessionExternalWindowState.ts";
import type { RemoteViewportStatusHudSummary } from "../../../src/session/viewport/remoteViewportStatusBar.ts";

test("restriction survives stale Surface commits and recovery waits for a refreshed frame", () => {
  let refreshes = 0;
  let changes = 0;
  const runtime = createServerRuntime({ sendRequest: async () => {}, refresh: () => { refreshes++; }, onChanged: () => { changes++; } });
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: false });
  runtime.consume({ kind: "desktop", desktop: "unavailable", secureAttentionAvailable: false });
  runtime.framePresented();
  assert.equal(runtime.state?.desktop, "unavailable");
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: false });
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: false });
  assert.equal(runtime.state?.desktop, "recovering");
  assert.equal(refreshes, 1);
  runtime.framePresented();
  assert.equal(runtime.state?.desktop, "available");
  const firstRecoveryId = runtime.state!.recoveryId;
  runtime.consume({ kind: "desktop", desktop: "unavailable", secureAttentionAvailable: false });
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: false });
  assert.equal(runtime.framePresented(firstRecoveryId), false, "late external frame acknowledgement cannot clear a later restriction");
  assert.equal(runtime.state?.desktop, "recovering");
  assert.equal(runtime.framePresented(runtime.state!.recoveryId), true);
  assert.equal(changes, 7);
});

test("unsupported SAS never sends a request; replies match ID and mean submitted only", async () => {
  const sent: number[] = [];
  const runtime = createServerRuntime({ sendRequest: async (id) => { sent.push(id); }, refresh() {} });
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: false });
  assert.equal(await runtime.request(), "unsupported");
  assert.deepEqual(sent, []);
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: true });
  const result = runtime.request();
  runtime.consume({ kind: "secureAttentionResponse", requestId: 99, result: "submitted" });
  assert.equal(runtime.request(), result);
  runtime.consume({ kind: "secureAttentionResponse", requestId: sent[0]!, result: "submitted" });
  assert.equal(await result, "submitted");
  const abandoned = runtime.request();
  runtime.dispose();
  assert.equal(await abandoned, "failed");
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: true });
  assert.equal(runtime.state, null);
});

test("timeout and late replies do not confirm a subsequent SAS request", async () => {
  const sent: number[] = [];
  const runtime = createServerRuntime({ sendRequest: async (id) => { sent.push(id); }, refresh() {}, timeoutMs: 5 });
  runtime.consume({ kind: "desktop", desktop: "available", secureAttentionAvailable: true });
  assert.equal(await runtime.request(), "unconfirmed");
  const next = runtime.request();
  runtime.consume({ kind: "secureAttentionResponse", requestId: sent[0]!, result: "submitted" });
  runtime.consume({ kind: "secureAttentionResponse", requestId: sent[1]!, result: "denied" });
  assert.equal(await next, "denied");
  runtime.dispose();
});

test("external recovery requests a refresh once, ignores restricted commits and acknowledges each recovery once", () => {
  const requests: Array<[number, boolean]> = [];
  const recovery = createExternalDesktopRecovery(async (id, presented) => { requests.push([id, presented]); });
  const hud: RemoteViewportStatusHudSummary = { title: "", target: "", inputMode: "", display: "", displaySource: "", state: "", duration: "", transport: "", latestEvent: "" };
  recovery.consume({ ...hud, runtimeDesktop: "unavailable", runtimeRecoveryId: 0 });
  recovery.framePresented();
  assert.deepEqual(requests, []);
  recovery.consume({ ...hud, runtimeDesktop: "recovering", runtimeRecoveryId: 1 });
  recovery.consume({ ...hud, runtimeDesktop: "recovering", runtimeRecoveryId: 1 });
  recovery.framePresented();
  recovery.framePresented();
  recovery.consume({ ...hud, runtimeDesktop: "unavailable", runtimeRecoveryId: 1 });
  recovery.framePresented();
  recovery.consume({ ...hud, runtimeDesktop: "recovering", runtimeRecoveryId: 2 });
  recovery.framePresented();
  assert.deepEqual(requests, [[1, false], [1, true], [2, false], [2, true]]);
});
