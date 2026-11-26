import assert from "node:assert/strict";
import test from "node:test";

import { shouldCleanupConnectionForSessionState } from "../../../src/session/core/sessionConnectionCleanup.ts";
import type { RemoteSessionState } from "../../../src/session/core/sessionManager.ts";

test("session connection cleanup is required for every terminal or failed state", () => {
  const cleanupStates: RemoteSessionState[] = [
    "authentication failed",
    "security failure",
    "disconnected",
    "disconnected unexpectedly",
    "failed",
    "closed",
  ];

  for (const state of cleanupStates) {
    assert.equal(shouldCleanupConnectionForSessionState(state), true, state);
  }
});

test("session connection cleanup keeps active connection states alive", () => {
  const activeStates: RemoteSessionState[] = [
    "idle",
    "probing",
    "connecting",
    "connected",
  ];

  for (const state of activeStates) {
    assert.equal(shouldCleanupConnectionForSessionState(state), false, state);
  }
});
