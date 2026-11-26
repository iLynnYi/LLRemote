import assert from "node:assert/strict";
import test from "node:test";
import { createSessionConnectAttempts } from "../../../src/features/sessions/composables/sessionConnectionLifecycle/connectAttempts.ts";
import { createSessionUserDisconnector } from "../../../src/features/sessions/composables/sessionConnectionLifecycle/disconnect.ts";
import type { ProtocolConnection } from "../../../src/session/protocol/protocolAdapter.ts";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((complete) => { resolve = complete; });
  return { promise, resolve };
}
const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

test("one startup owns late Core creation and replacement waits for confirmed cleanup", async () => {
  const core = deferred();
  const stopping = deferred();
  const steps: string[] = [];
  const attempts = createSessionConnectAttempts({
    canConnect: () => true,
    releaseSession: async () => { steps.push("release"); },
    stopCore: async () => { steps.push("stop"); await stopping.promise; },
    disconnectConnection: async () => { steps.push("connection-close"); },
  });
  const first = attempts.run("session", async (attempt) => {
    await attempt.startCore(async () => { steps.push("create"); await core.promise; });
    steps.push("continued");
    return true;
  });
  const repeated = attempts.run("session", async () => { steps.push("duplicate"); return true; });
  assert.equal(first, repeated);
  await flush();
  const closing = attempts.cancel("session", true);
  const replacement = attempts.run("session", async () => { steps.push("replacement"); return true; });
  await flush();
  assert.deepEqual(steps, ["create", "release"]);
  core.resolve();
  await flush();
  assert.deepEqual(steps, ["create", "release", "stop"]);
  stopping.resolve();
  await Promise.all([first, closing, replacement]);
  assert.deepEqual(steps, ["create", "release", "stop", "release", "replacement"]);
});

test("a failed release retains its resource gate and a retry reclaims it before reuse", async () => {
  let releaseFails = true;
  let operations = 0;
  let stops = 0;
  const connection = {} as ProtocolConnection;
  const attempts = createSessionConnectAttempts({
    canConnect: () => true,
    releaseSession: async () => { if (releaseFails) throw new Error("cleanup_failed"); },
    stopCore: async () => { stops += 1; },
    disconnectConnection: async (owner) => { assert.equal(owner, connection); },
  });
  await attempts.run("session", async (attempt) => {
    operations += 1;
    await attempt.startCore(async () => undefined);
    await attempt.connect(async () => connection);
    return true;
  });
  await assert.rejects(attempts.cancel("session", true), /cleanup_failed/);
  await assert.rejects(attempts.run("session", async () => { operations += 1; return true; }), /cleanup_failed/);
  assert.equal(operations, 1);
  assert.equal(stops, 0);
  releaseFails = false;
  await attempts.cancel("session", true);
  await attempts.run("session", async () => { operations += 1; return true; });
  assert.equal(operations, 2);
  assert.equal(stops, 1);
  await attempts.dispose();
  await attempts.run("session", async () => { operations += 1; return true; });
  assert.equal(operations, 2);
});

test("cancel before startup allocates resources still owns cleanup before a replacement", async () => {
  const release = deferred();
  const stop = deferred();
  const steps: string[] = [];
  const attempts = createSessionConnectAttempts({
    canConnect: () => true,
    releaseSession: async () => { steps.push("release"); await release.promise; },
    stopCore: async () => { steps.push("stop"); await stop.promise; },
    disconnectConnection: async () => undefined,
  });
  const first = attempts.run("session", async () => { steps.push("old-create"); return true; });
  const closing = attempts.cancel("session", true);
  const next = attempts.run("session", async () => { steps.push("new-create"); return true; });
  await flush();
  assert.deepEqual(steps, ["release"]);
  release.resolve();
  await flush();
  assert.deepEqual(steps, ["release", "stop"]);
  stop.resolve();
  await Promise.all([first, closing, next]);
  assert.deepEqual(steps, ["release", "stop", "release", "new-create"]);
});


test("explicit end notification completes before Agent cancellation and blocks reconnection while waiting", async () => {
  const acknowledgement = deferred();
  const steps: string[] = [];
  let channelAvailable = true;
  const connection = { notifyRemoteSessionEnd: async () => {
    assert.equal(channelAvailable, true);
    steps.push("lock-request");
    await acknowledgement.promise;
    assert.equal(channelAvailable, true);
    steps.push("lock-acknowledged");
  } } as ProtocolConnection;
  const attempts = createSessionConnectAttempts({
    canConnect: (id) => !user.isPending(id),
    releaseSession: async () => { steps.push("release"); },
    stopCore: async () => {}, disconnectConnection: async () => {},
  });
  const user = createSessionUserDisconnector({
    getConnection: () => connection, shouldNotify: () => true,
    onFailure: () => assert.fail("lock must precede Agent revocation"),
    cancel: (id, removeCore) => attempts.cancel(id, removeCore),
  });
  await attempts.run("session", async (attempt) => {
    attempt.signal.addEventListener("abort", () => { channelAvailable = false; steps.push("abort"); });
    await attempt.connect(async () => connection);
    return true;
  });
  const disconnect = user.disconnect("session");
  await flush();
  await attempts.run("session", async () => { assert.fail("cannot replace a connection awaiting lock"); });
  assert.deepEqual(steps, ["lock-request"]);
  acknowledgement.resolve();
  await disconnect;
  assert.deepEqual(steps, ["lock-request", "lock-acknowledged", "abort", "release", "release"]);
});
