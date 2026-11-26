import assert from "node:assert/strict";
import test from "node:test";

import {
  createExternalKeyboardInputPayload,
  createExternalPointerInputPayload,
  createExternalWheelInputPayload,
  startSessionExternalInputForwarder,
} from "../../../src/features/sessions/composables/useSessionExternalInputBridge.ts";
import { sessionExternalInputEvent, type SessionExternalInputPayload } from "../../../src/session/external-window/sessionExternalInput.ts";
import type { ProtocolConnection } from "../../../src/session/protocol/protocolAdapter.ts";
import { createInitialConnectionModel, type SessionConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";

function agentModel(capabilities: string[]): SessionConnectionModel {
  return {
    ...createInitialConnectionModel(),
    enhancedChannel: { status: "available", authentication: "verified" },
    capabilities: { items: capabilities.map((id) => ({ id, state: "supported", source: "agent" })) },
  };
}

function keyEvent(input: Partial<KeyboardEvent>): KeyboardEvent {
  return input as KeyboardEvent;
}

test("external keyboard payload carries the standard key mapping without remote OS inference", () => {
  assert.deepEqual(createExternalKeyboardInputPayload({
    sessionId: "session-1",
    event: keyEvent({ key: "ArrowLeft", code: "ArrowLeft" }),
    down: true,
  }), {
    sessionId: "session-1",
    type: "keyboard",
    key: "ArrowLeft",
    code: "ArrowLeft",
    down: true,
    keysym: 0xff51,
  });
});

test("external keyboard payload skips unmapped keyboard events", () => {
  assert.equal(createExternalKeyboardInputPayload({
    sessionId: "session-1",
    event: keyEvent({ key: "Unidentified", code: "" }),
    down: true,
  }), null);
});

test("external windows apply the explicitly selected right Control mapping", () => {
  assert.equal(createExternalKeyboardInputPayload({
    sessionId: "mac", event: keyEvent({ key: "Control", code: "ControlRight", location: 2 }),
    down: true, rightControlAsCommand: true,
  })?.keysym, 0xffe8);
});

test("external pointer and wheel payloads are session scoped and contain no target secrets", () => {
  assert.deepEqual(createExternalPointerInputPayload({
    sessionId: "session-1",
    action: "down",
    clientX: 150,
    clientY: 80,
    button: 0,
    buttons: 1,
    frameRect: { left: 50, top: 30, width: 200, height: 100 },
    framebufferWidth: 1000,
    framebufferHeight: 500,
  }), {
    sessionId: "session-1",
    type: "pointer",
    action: "down",
    x: 500,
    y: 250,
    buttonMask: 1,
  });

  assert.deepEqual(createExternalWheelInputPayload({
    sessionId: "session-1",
    clientX: 150,
    clientY: 80,
    buttons: 1,
    deltaX: 0,
    deltaY: -100,
    frameRect: { left: 50, top: 30, width: 200, height: 100 },
    framebufferWidth: 1000,
    framebufferHeight: 500,
  }), {
    sessionId: "session-1",
    type: "wheel",
    x: 500,
    y: 250,
    deltaX: 0,
    deltaY: -100,
    buttonMask: 1,
  });
});

test("external pointer release clears the remote mouse button mask", () => {
  assert.deepEqual(createExternalPointerInputPayload({
    sessionId: "session-1",
    action: "up",
    clientX: 150,
    clientY: 80,
    button: 0,
    buttons: 0,
    frameRect: { left: 50, top: 30, width: 200, height: 100 },
    framebufferWidth: 1000,
    framebufferHeight: 500,
  }), {
    sessionId: "session-1",
    type: "pointer",
    action: "up",
    x: 500,
    y: 250,
    buttonMask: 0,
  });
});

test("external input forwarder listens on the session scoped event and forwards to the live connection", async () => {
  const forwarded: SessionExternalInputPayload[] = [];
  const order: string[] = [];
  const listenedEvents: string[] = [];
  const unlistenCalls: string[] = [];
  const connection = {
    forwardExternalInput: (payload: SessionExternalInputPayload) => { order.push("forward"); forwarded.push(payload); },
  } as ProtocolConnection;

  const unlisten = await startSessionExternalInputForwarder({
    sessionId: "session-1",
    listen: async (eventName, handler) => {
      listenedEvents.push(eventName);
      handler({
        payload: {
          sessionId: "session-1",
          type: "keyboard",
          key: "Enter",
          code: "Enter",
          down: true,
          keysym: 0xff0d,
        },
      });
      handler({
        payload: {
          sessionId: "session-2",
          type: "keyboard",
          key: "Enter",
          code: "Enter",
          down: true,
          keysym: 0xff0d,
        },
      });
      return () => unlistenCalls.push(eventName);
    },
    getConnection: () => connection,
    activateInput: () => { order.push("activate"); },
  });

  assert.deepEqual(listenedEvents, [sessionExternalInputEvent("session-1")]);
  assert.deepEqual(forwarded, [{
    sessionId: "session-1",
    type: "keyboard",
    key: "Enter",
    code: "Enter",
    down: true,
    keysym: 0xff0d,
  }]);
  assert.deepEqual(order, ["activate", "forward"]);

  unlisten();
  assert.deepEqual(unlistenCalls, [sessionExternalInputEvent("session-1")]);
});

test("a replaced child window cannot reclaim input through a late event", async () => {
  const forwarded: SessionExternalInputPayload[] = [];
  let activated = 0;
  let handler!: (event: { payload: SessionExternalInputPayload }) => void;
  const cleanup = await startSessionExternalInputForwarder({
    sessionId: "session-1",
    windowInstanceId: "current-window",
    listen: async (_name, listener) => { handler = listener; return () => undefined; },
    getConnection: () => ({ forwardExternalInput: (payload: SessionExternalInputPayload) => forwarded.push(payload) }) as ProtocolConnection,
    activateInput: () => { activated++; },
  });
  const key = { sessionId: "session-1", type: "keyboard", key: "a", code: "KeyA", down: true, keysym: 97 } as const;
  handler({ payload: { ...key, windowInstanceId: "old-window" } });
  handler({ payload: { ...key, windowInstanceId: "current-window" } });
  assert.equal(activated, 1);
  assert.equal(forwarded.length, 1);
  cleanup();
});

test("external input release and cleanup release only held state on its original connection once", async () => {
  const firstInputs: SessionExternalInputPayload[] = [];
  const secondInputs: SessionExternalInputPayload[] = [];
  const first = { forwardExternalInput: (payload: SessionExternalInputPayload) => firstInputs.push(payload) } as ProtocolConnection;
  const second = { forwardExternalInput: (payload: SessionExternalInputPayload) => secondInputs.push(payload) } as ProtocolConnection;
  let connection = first;
  let handler!: (event: { payload: SessionExternalInputPayload }) => void;
  let unlistenCalls = 0;
  const records: string[] = [];
  const cleanup = await startSessionExternalInputForwarder({
    sessionId: "session-1",
    listen: async (_eventName, listener) => {
      handler = listener;
      return () => { unlistenCalls += 1; };
    },
    getConnection: () => connection,
    recordInput: (_sessionId, type) => { records.push(type); },
  });
  const key = { sessionId: "session-1", type: "keyboard", key: "Shift", code: "ShiftLeft", down: true, keysym: 0xffe1 } as const;
  const releasedKey = { ...key, down: false };
  const pointer = { sessionId: "session-1", type: "pointer", action: "down", x: 10, y: 20, buttonMask: 1 } as const;
  const release = { sessionId: "session-1", type: "releaseInput" } as const;
  const send = (payload: SessionExternalInputPayload) => handler({ payload });

  send(key);
  const control = { ...key, key: "Control", code: "ControlLeft", keysym: 0xffe3 };
  send(control);
  send({ ...control, down: false });
  send(pointer);
  send({ sessionId: "session-1", type: "wheel", x: 30, y: 40, deltaX: 0, deltaY: -100, buttonMask: 1 });
  const beforeRelease = firstInputs.length;
  const recordedBeforeRelease = records.length;
  send(release);
  send(release);
  assert.deepEqual(firstInputs.slice(beforeRelease), [releasedKey, {
    sessionId: "session-1", type: "pointer", action: "up", x: 30, y: 40, buttonMask: 0,
  }]);
  assert.equal(records.length, recordedBeforeRelease);

  send(key);
  send(pointer);
  connection = second;
  const beforeIdentityRelease = firstInputs.length;
  send(release);
  assert.deepEqual(firstInputs.slice(beforeIdentityRelease), [releasedKey, { ...pointer, action: "up", buttonMask: 0 }]);
  assert.deepEqual(secondInputs, []);

  send(key);
  send(releasedKey);
  send({ sessionId: "session-1", type: "wheel", x: 50, y: 60, deltaX: 0, deltaY: -100, buttonMask: 0 });
  const beforeEmptyRelease = secondInputs.length;
  send(release);
  assert.equal(secondInputs.length, beforeEmptyRelease);
  send(key);
  send(pointer);
  const beforeCleanup = secondInputs.length;
  cleanup();
  cleanup();
  send(key);
  assert.deepEqual(secondInputs.slice(beforeCleanup), [releasedKey, { ...pointer, action: "up", buttonMask: 0 }]);
  assert.equal(unlistenCalls, 1);
});

test("external desktop shortcuts still dispatch to the Agent without an RFB fallback", async () => {
  const calls: string[] = [];
  const connection = {
    connectionModel: agentModel(["remote.desktop.control"]),
    controlRemoteDesktop: async (actionId: string) => { calls.push(`agent:${actionId}`); },
    forwardExternalInput: () => calls.push("rfb:input"),
  } as unknown as ProtocolConnection;
  await startSessionExternalInputForwarder({
    sessionId: "session-1", getConnection: () => connection,
    listen: async (_name, handler) => {
      handler({ payload: { sessionId: "session-1", type: "remoteDesktopShortcut", actionId: "overview" } });
      return () => undefined;
    },
  });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(calls, ["agent:overview"]);
});

test("rejected desktop shortcuts settle silently without an unhandled rejection", async () => {
  const connection = {
    connectionModel: agentModel(["remote.desktop.control"]),
    controlRemoteDesktop: async () => { throw new Error("remote_desktop_control_permission_missing"); },
  } as unknown as ProtocolConnection;
  await startSessionExternalInputForwarder({
    sessionId: "session-1", getConnection: () => connection,
    listen: async (_name, handler) => {
      handler({ payload: { sessionId: "session-1", type: "remoteDesktopShortcut", actionId: "overview" } });
      return () => undefined;
    },
  });
  await new Promise(resolve => setImmediate(resolve));
});

test("unsupported desktop shortcuts retain the capability gate without an RFB fallback", async () => {
  const calls: string[] = [];
  const connection = {
    connectionModel: createInitialConnectionModel(),
    controlRemoteDesktop: async () => { calls.push("agent"); },
    forwardExternalInput: () => { calls.push("rfb"); },
  } as unknown as ProtocolConnection;
  await startSessionExternalInputForwarder({
    sessionId: "session-1", getConnection: () => connection,
    onRemoteDesktopShortcutUnavailable: (id, action) => calls.push(`${id}:${action}`),
    listen: async (_name, handler) => {
      handler({ payload: { sessionId: "session-1", type: "remoteDesktopShortcut", actionId: "next-desktop" } });
      return () => undefined;
    },
  });
  assert.deepEqual(calls, ["session-1:next-desktop"]);
});
