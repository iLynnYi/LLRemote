import assert from "node:assert/strict";
import test from "node:test";
import { sessionExternalWindowStateEvent, sessionExternalWindowStateRequestEvent } from "../../../src/session/external-window/sessionExternalWindowState.ts";

test("external window state events are scoped to one Core Surface session", () => {
  assert.equal(sessionExternalWindowStateEvent("session-1"), "session-window-state:session-1");
  assert.equal(sessionExternalWindowStateRequestEvent("session-1"), "session-window-state-request:session-1");
});
