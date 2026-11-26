import assert from "node:assert/strict";
import test from "node:test";
import { createSessionWindowInputFocus } from "../../../src/session/input/sessionWindowInputFocus.ts";

test("window focus controls input without changing the main tab selection", () => {
  let mainSessionId = "main-a";
  const owners: Array<string | null> = [];
  const focus = createSessionWindowInputFocus({
    getMainSessionId: () => mainSessionId,
    onChange: (sessionId) => owners.push(sessionId),
  });

  assert.equal(focus.sessionId(), "main-a");
  focus.mainFocusChanged(false);
  focus.externalFocusChanged("child-b", true);
  assert.equal(focus.sessionId(), "child-b");
  mainSessionId = "main-c";
  focus.mainSessionChanged();
  assert.equal(focus.sessionId(), "child-b");
  focus.externalFocusChanged("child-d", true);
  focus.externalFocusChanged("child-b", false);
  assert.equal(focus.sessionId(), "child-d", "a late blur must not clear another focused window");
  focus.externalFocusChanged("child-d", false);
  focus.mainFocusChanged(true);
  assert.equal(focus.sessionId(), "main-c");
  assert.deepEqual(owners, [null, "child-b", "child-d", null, "main-c"]);
});

test("changing the selected main tab updates input only while the main window owns focus", () => {
  let mainSessionId = "main-a";
  const owners: Array<string | null> = [];
  const focus = createSessionWindowInputFocus({
    getMainSessionId: () => mainSessionId,
    onChange: (sessionId) => owners.push(sessionId),
  });
  mainSessionId = "main-b";
  focus.mainSessionChanged();
  assert.deepEqual(owners, ["main-b"]);
  focus.externalFocusChanged("child-c", true);
  mainSessionId = "main-d";
  focus.mainSessionChanged();
  assert.deepEqual(owners, ["main-b", "child-c"]);
});
