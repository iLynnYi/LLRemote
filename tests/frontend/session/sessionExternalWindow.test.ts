import assert from "node:assert/strict";
import test from "node:test";

import {
  createSessionExternalWindowRequest,
  isSessionExternalWindowLocation,
  isSessionExternalWindowDockRequest,
  sessionExternalWindowDockEvent,
  sessionExternalWindowLabel,
} from "../../../src/session/external-window/sessionExternalWindow.ts";

test("session external window labels are stable and scoped to a session", () => {
  assert.equal(sessionExternalWindowLabel("session-1"), "session-session-1");
});

test("session external window request carries shell URL title and conservative options", () => {
  assert.deepEqual(createSessionExternalWindowRequest({
    sessionId: "session-1",
    target: "10.0.0.1:5900",
    host: "10.0.0.1",
    port: 5900,
    remoteOsProfile: "generic",
    readonly: false,
  }), {
    sessionId: "session-1",
    target: "10.0.0.1:5900",
    host: "10.0.0.1",
    port: 5900,
    remoteOsProfile: "generic",
    readonly: false,
    label: "session-session-1",
    title: "VNC Session 10.0.0.1:5900",
    url: "/?workspace=session-window&sessionId=session-1&target=10.0.0.1%3A5900&host=10.0.0.1&port=5900&readonly=false&fullscreen=false",
    width: 960,
    height: 640,
    minWidth: 320,
    minHeight: 240,
    resizable: true,
    fullscreen: false,
  });
});

test("session external window location is detected from query parameters", () => {
  assert.deepEqual(isSessionExternalWindowLocation("http://localhost:1420/?workspace=session-window&sessionId=session-1&target=10.0.0.1%3A5900&host=10.0.0.1&port=5900&readonly=true"), {
    sessionId: "session-1",
    target: "10.0.0.1:5900",
    host: "10.0.0.1",
    port: 5900,
    readonly: true,
  });

  assert.equal(isSessionExternalWindowLocation("http://localhost:1420/"), null);
});

test("session external window instance survives URL roundtrip and scopes dock requests", () => {
  const location = {
    sessionId: "session-1", target: "10.0.0.1:5900", host: "10.0.0.1", port: 5900,
    readonly: false, windowInstanceId: "window-instance-1",
  };
  const request = createSessionExternalWindowRequest(location);
  const url = new URL(request.url, "http://localhost:1420");
  assert.equal(url.searchParams.get("windowInstanceId"), location.windowInstanceId);
  assert.equal(request.label, "session-session-1");
  assert.deepEqual(isSessionExternalWindowLocation(url.href), location);
  assert.equal(isSessionExternalWindowDockRequest({
    sessionId: location.sessionId, windowInstanceId: location.windowInstanceId, extra: "ignored",
  }), true);
  for (const payload of [
    location.sessionId, null, {}, { sessionId: location.sessionId },
    { sessionId: "", windowInstanceId: location.windowInstanceId },
    { sessionId: location.sessionId, windowInstanceId: " " },
    { sessionId: location.sessionId, windowInstanceId: 1 },
  ]) {
    assert.equal(isSessionExternalWindowDockRequest(payload), false);
  }
});

test("session external window dock event is scoped per session", () => {
  assert.equal(sessionExternalWindowDockEvent("session-1"), "session-window-dock:session-1");
});
