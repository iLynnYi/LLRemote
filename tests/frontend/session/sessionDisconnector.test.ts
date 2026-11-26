import assert from "node:assert/strict";
import test from "node:test";
import type { ProtocolConnection } from "../../../src/session/protocol/protocolAdapter.ts";
import { createSessionDisconnector, createSessionUserDisconnector } from "../../../src/features/sessions/composables/sessionConnectionLifecycle/disconnect.ts";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((complete) => { resolve = complete; });
  return { promise, resolve };
}
const flushPromises = () => new Promise<void>((resolve) => setImmediate(resolve));
// The disconnector passes this owner through without reading protocol fields.
const connection = {} as ProtocolConnection;

test("repeated disconnects wait for pending clear after registry removal, including sessions without a connection", async () => {
  const pendingClear = deferred();
  const missingClear = deferred();
  let current: ProtocolConnection | undefined = connection;
  const steps: string[] = [];
  const disconnected: ProtocolConnection[] = [];
  const disconnect = createSessionDisconnector({
    getConnection: () => current,
    clearConnection: (sessionId) => {
      steps.push(`clear:${sessionId}`);
      current = undefined;
      return sessionId === "session-1" ? pendingClear.promise : missingClear.promise;
    },
    disconnectConnection: async (owner) => { assert.equal(owner, connection); steps.push("adapter"); },
    onDisconnected: (_, owner) => { disconnected.push(owner); },
  });

  const first = disconnect("session-1");
  // A status cleanup may detach the registry before the operation's microtask.
  current = undefined;
  let completed = false;
  const completion = first.then(() => { completed = true; });
  await flushPromises();
  const repeated = disconnect("session-1");
  assert.equal(repeated, first);
  assert.deepEqual(steps, ["clear:session-1", "adapter"]);
  assert.equal(completed, false);
  assert.deepEqual(disconnected, []);
  pendingClear.resolve();
  await Promise.all([first, repeated, completion]);
  assert.deepEqual(disconnected, [connection]);

  const missing = disconnect("missing");
  let missingCompleted = false;
  const missingCompletion = missing.then(() => { missingCompleted = true; });
  await flushPromises();
  assert.deepEqual(steps, ["clear:session-1", "adapter", "clear:missing"]);
  assert.equal(missingCompleted, false);
  missingClear.resolve();
  await Promise.all([missing, missingCompletion]);
  assert.deepEqual(disconnected, [connection]);
});

test("failed cleanup prevents disconnected continuation and releases the gate for retry", async () => {
  const pendingAdapter = deferred();
  const failure = new Error("surface_cleanup_failed");
  let clearCalls = 0;
  let adapterCalls = 0;
  let successfulCalls = 0;
  const disconnect = createSessionDisconnector({
    getConnection: () => undefined,
    clearConnection: async () => {
      clearCalls += 1;
      if (clearCalls === 1) throw failure;
    },
    disconnectConnection: (owner) => {
      assert.equal(owner, connection);
      adapterCalls += 1;
      return adapterCalls === 1 ? pendingAdapter.promise : Promise.resolve();
    },
    onDisconnected: (_, owner) => { assert.equal(owner, connection); successfulCalls += 1; },
  });

  const failed = disconnect("session-1", connection);
  const rejection = assert.rejects(failed, (error) => error === failure);
  let settled = false;
  const settlement = failed.then(() => { settled = true; }, () => { settled = true; });
  await flushPromises();
  assert.equal(disconnect("session-1"), failed);
  assert.deepEqual({ clearCalls, adapterCalls, successfulCalls }, { clearCalls: 1, adapterCalls: 1, successfulCalls: 0 });
  assert.equal(settled, false);
  pendingAdapter.resolve();
  await Promise.all([rejection, settlement]);
  await disconnect("session-1", connection);
  assert.deepEqual({ clearCalls, adapterCalls, successfulCalls }, { clearCalls: 2, adapterCalls: 2, successfulCalls: 1 });
});


test("user disconnect waits for one end notification before cancellation and tab close", async () => {
  const lock = deferred();
  const steps: string[] = [];
  const owner = { notifyRemoteSessionEnd: async () => { steps.push("lock"); await lock.promise; } } as ProtocolConnection;
  const user = createSessionUserDisconnector({
    getConnection: () => owner, shouldNotify: () => true,
    onFailure: () => assert.fail("unexpected lock failure"),
    cancel: async (_, removeCore) => { steps.push(removeCore ? "close" : "disconnect"); return true; },
  });
  const first = user.disconnect("session-1");
  assert.equal(user.isPending("session-1"), true);
  assert.equal(user.disconnect("session-1"), first);
  const close = user.disconnect("session-1", true);
  await flushPromises();
  assert.deepEqual(steps, ["lock"]);
  lock.resolve();
  await Promise.all([first, close]);
  assert.deepEqual(steps, ["lock", "disconnect", "close"]);
  assert.equal(user.isPending("session-1"), false);
});

test("nonconnected sessions and missing owners skip end notification and still disconnect", async () => {
  for (const hasOwner of [true, false]) {
    const steps: string[] = [];
    const user = createSessionUserDisconnector({
      getConnection: () => hasOwner ? { notifyRemoteSessionEnd: async () => { steps.push("lock"); } } as ProtocolConnection : undefined,
      shouldNotify: () => false,
      onFailure: () => assert.fail("unexpected lock failure"),
      cancel: async () => { steps.push("disconnect"); return true; },
    });
    await user.disconnect("session-1");
    assert.deepEqual(steps, ["disconnect"]);
  }
});

test("rejected end notifications report failure and release the connection", async () => {
  for (const owner of [{ notifyRemoteSessionEnd: async () => { throw new Error("permission_denied"); } } as ProtocolConnection]) {
    const steps: string[] = [];
    const user = createSessionUserDisconnector({
      getConnection: () => owner, shouldNotify: () => true,
      onFailure: (id) => { steps.push(`warning:${id}`); },
      cancel: async () => { steps.push("disconnect"); return true; },
    });
    await user.disconnect("session-1");
    assert.deepEqual(steps, ["warning:session-1", "disconnect"]);
    assert.equal(user.isPending("session-1"), false);
  }
});

test("passive cleanup never invokes remote locking", async () => {
  let locks = 0;
  const owner = { notifyRemoteSessionEnd: async () => { locks += 1; } } as ProtocolConnection;
  const disconnect = createSessionDisconnector({
    getConnection: () => owner,
    clearConnection: async () => {}, disconnectConnection: async () => {}, onDisconnected: () => {},
  });
  await disconnect("session-1");
  assert.equal(locks, 0);
});
