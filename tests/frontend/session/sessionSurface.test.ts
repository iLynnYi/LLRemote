import assert from "node:assert/strict";
import test from "node:test";

import { createSessionSurfaceId } from "../../../src/session/viewport/sessionSurface.ts";

test("createSessionSurfaceId is scoped by session owner and window label", () => {
  assert.equal(createSessionSurfaceId({
    sessionId: "session-1",
    owner: "embedded",
    windowLabel: "main",
  }), "surface:session-1:embedded:main");

  assert.equal(createSessionSurfaceId({
    sessionId: "session-1",
    owner: "external",
    windowLabel: "session-session-1",
  }), "surface:session-1:external:session-session-1");
});
