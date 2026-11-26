import assert from "node:assert/strict";
import test from "node:test";

import {
  mapExternalPointerToFramebuffer,
  sessionExternalInputEvent,
  toExternalMouseButtonMask,
  toExternalMouseButtonsMask,
  type SessionExternalInputPayload,
} from "../../../src/session/external-window/sessionExternalInput.ts";

test("session external input event is scoped per session", () => {
  assert.equal(sessionExternalInputEvent("session-1"), "session-window-input:session-1");
});

test("external pointer coordinates map rendered fit frame coordinates into framebuffer coordinates", () => {
  assert.deepEqual(mapExternalPointerToFramebuffer({
    clientX: 450,
    clientY: 250,
    frameRect: { left: 50, top: 25, width: 800, height: 450 },
    framebufferWidth: 1600,
    framebufferHeight: 900,
  }), { x: 800, y: 450 });
});

test("external pointer coordinates clamp outside rendered frame bounds", () => {
  assert.deepEqual(mapExternalPointerToFramebuffer({
    clientX: -20,
    clientY: 1200,
    frameRect: { left: 100, top: 100, width: 400, height: 300 },
    framebufferWidth: 1920,
    framebufferHeight: 1080,
  }), { x: 0, y: 1079 });
});

test("external pointer mapping supports actual and fill by using the rendered image rect", () => {
  assert.deepEqual(mapExternalPointerToFramebuffer({
    clientX: 980,
    clientY: 560,
    frameRect: { left: 20, top: 20, width: 1920, height: 1080 },
    framebufferWidth: 1920,
    framebufferHeight: 1080,
  }), { x: 960, y: 540 });
});

test("external mouse buttons use RFB button masks", () => {
  assert.equal(toExternalMouseButtonMask(0), 1);
  assert.equal(toExternalMouseButtonMask(1), 2);
  assert.equal(toExternalMouseButtonMask(2), 4);
  assert.equal(toExternalMouseButtonMask(99), 0);
  assert.equal(toExternalMouseButtonsMask(1 | 2 | 4), 1 | 4 | 2);
});

test("external input payload carries only session scoped input data", () => {
  const payload = {
    sessionId: "session-secret",
    type: "pointer",
    action: "move",
    x: 10,
    y: 20,
    buttonMask: 0,
  } satisfies SessionExternalInputPayload;

  assert.deepEqual(Object.keys(payload).sort(), ["action", "buttonMask", "sessionId", "type", "x", "y"]);
});
