import assert from "node:assert/strict";
import test from "node:test";

import { createSessionConnectionRegistry } from "../../../src/session/core/sessionConnectionRegistry.ts";
import type { ProtocolConnection } from "../../../src/session/protocol/protocolAdapter.ts";

function fakeConnection(id: string): ProtocolConnection {
  return {
    protocol: "vnc",
    bridge: {
      id: `bridge-${id}`,
      websocketUrl: `ws://127.0.0.1:${id}`,
    },
    disconnect: () => {},
    focus: () => {},
    refreshDisplay: () => {},
    setInputEnabled: () => {},
    setReadonlyMode: () => {},
    forwardExternalInput: () => {},
    getDisplayInfo: () => null,
    getRuntimeEvidence: () => null,
    setQualityProfile: () => {},
  };
}

test("session connection registry stores one connection per session", () => {
  const registry = createSessionConnectionRegistry();
  const first = fakeConnection("61001");
  const second = fakeConnection("61002");

  registry.attach("session-1", first);
  registry.attach("session-2", second);

  assert.equal(registry.get("session-1")?.connection, first);
  assert.equal(registry.get("session-2")?.connection, second);
  assert.deepEqual(registry.listLiveSessionIds(), ["session-1", "session-2"]);
});

test("session connection registry detaches only the targeted session", () => {
  const registry = createSessionConnectionRegistry();
  const first = fakeConnection("61001");
  const second = fakeConnection("61002");

  registry.attach("session-1", first);
  registry.attach("session-2", second);

  const detached = registry.detach("session-1");

  assert.equal(detached?.connection, first);
  assert.equal(registry.get("session-1"), undefined);
  assert.equal(registry.get("session-2")?.connection, second);
  assert.deepEqual(registry.listLiveSessionIds(), ["session-2"]);
});

test("session connection registry tracks the focused canvas without requiring other live sessions to disconnect", () => {
  const registry = createSessionConnectionRegistry();
  const first = fakeConnection("61001");
  const second = fakeConnection("61002");

  registry.attach("session-1", first);
  registry.attach("session-2", second);
  registry.activateCanvas("session-1");

  assert.equal(registry.activeCanvasSessionId, "session-1");
  assert.equal(registry.activeCanvasConnection?.connection, first);
  assert.equal(registry.shouldDisconnectBeforeActivating("session-2"), false);
  assert.equal(registry.shouldDisconnectBeforeActivating("session-1"), false);
});

test("session connection registry does not replace the focused canvas when another session finishes connecting", () => {
  const registry = createSessionConnectionRegistry();
  const first = fakeConnection("61001");

  registry.attach("session-1", first);
  registry.activateCanvas("session-1");
  registry.attach("session-2", fakeConnection("61002"));

  assert.equal(registry.activeCanvasSessionId, "session-1");
  assert.equal(registry.activeCanvasConnection?.connection, first);
});

test("session connection registry clears active canvas when its owner detaches", () => {
  const registry = createSessionConnectionRegistry();

  registry.attach("session-1", fakeConnection("61001"));
  registry.attach("session-2", fakeConnection("61002"));
  registry.activateCanvas("session-1");

  registry.detach("session-1");

  assert.equal(registry.activeCanvasSessionId, null);
  assert.equal(registry.activeCanvasConnection, null);
  assert.deepEqual(registry.listLiveSessionIds(), ["session-2"]);
});

test("session connection registry can clear active canvas without detaching live sessions", () => {
  const registry = createSessionConnectionRegistry();

  registry.attach("session-1", fakeConnection("61001"));
  registry.attach("session-2", fakeConnection("61002"));

  registry.clearActiveCanvas();

  assert.equal(registry.activeCanvasSessionId, null);
  assert.equal(registry.activeCanvasConnection, null);
  assert.deepEqual(registry.listLiveSessionIds(), ["session-1", "session-2"]);
});

test("session connection registry resolves the requested session independently of the focused canvas", () => {
  const registry = createSessionConnectionRegistry();
  const first = fakeConnection("61001");
  const second = fakeConnection("61002");

  registry.attach("session-1", first);
  registry.attach("session-2", second);
  registry.activateCanvas("session-2");

  assert.equal(registry.activeCanvasSessionId, "session-2");
  assert.equal(registry.get("session-1")?.connection, first);
});

test("session connection registry detects zombie connections outside allowed sessions", () => {
  const registry = createSessionConnectionRegistry();

  registry.attach("session-1", fakeConnection("61001"));
  registry.attach("session-2", fakeConnection("61002"));
  registry.attach("session-3", fakeConnection("61003"));

  assert.deepEqual(registry.findZombieSessionIds(["session-1", "session-3"]), ["session-2"]);
});

test("session connection registry detaches zombie connections without touching allowed sessions", () => {
  const registry = createSessionConnectionRegistry();
  const first = fakeConnection("61001");
  const second = fakeConnection("61002");
  const third = fakeConnection("61003");

  registry.attach("session-1", first);
  registry.attach("session-2", second);
  registry.attach("session-3", third);
  registry.activateCanvas("session-2");

  const detached = registry.detachZombieConnections(["session-1", "session-3"]);

  assert.deepEqual(detached.map((entry) => entry.sessionId), ["session-2"]);
  assert.equal(detached[0]?.connection, second);
  assert.equal(registry.get("session-1")?.connection, first);
  assert.equal(registry.get("session-2"), undefined);
  assert.equal(registry.get("session-3")?.connection, third);
  assert.equal(registry.activeCanvasSessionId, null);
});
