import assert from "node:assert/strict";
import test from "node:test";
import { createLocalVncServerDiagnosticsRefresh } from "../../../src/app/composables/vncAppState/localDiagnostics.ts";

test("local diagnostics sample only visible connected self sessions and serialize slow reads", async (t) => {
  t.mock.timers.enable({ apis: ["setInterval"] });
  let calls = 0;
  let release: (() => void) | undefined;
  const session = { host: "127.0.0.1", port: 5900, state: "connected" };
  const options = {
    activeRemoteSession: { value: session }, activeWorkspace: { value: "sessions" },
    diagnosticsVisible: { value: false },
    localVncServerOverview: { value: { status: "running", port: 5900 } },
    refreshRuntimeStatus: () => { calls++; return new Promise<void>(resolve => { release = resolve; }); },
  };
  const sampler = createLocalVncServerDiagnosticsRefresh(options);
  sampler.sync();
  t.mock.timers.tick(2000);
  assert.equal(calls, 0);
  options.diagnosticsVisible.value = true;
  sampler.sync();
  assert.equal(calls, 1);
  t.mock.timers.tick(3000);
  assert.equal(calls, 1);
  release!();
  await Promise.resolve();
  t.mock.timers.tick(1000);
  assert.equal(calls, 2);
  options.diagnosticsVisible.value = false;
  sampler.sync();
  release!();
  await Promise.resolve();
  t.mock.timers.tick(3000);
  assert.equal(calls, 2);
  session.state = "disconnected";
  options.diagnosticsVisible.value = true;
  sampler.sync();
  t.mock.timers.tick(2000);
  assert.equal(calls, 2);
  options.activeRemoteSession.value = { ...session, state: "connected", port: 5901 };
  sampler.sync();
  t.mock.timers.tick(2000);
  assert.equal(calls, 2);
  sampler.stop();
});
