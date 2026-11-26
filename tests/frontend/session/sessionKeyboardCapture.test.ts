import assert from "node:assert/strict";
import test from "node:test";

import { shouldCaptureSessionKeyboardEvent } from "../../../src/session/input/sessionKeyboardCapture.ts";

test("captures session keyboard events only when the active session allows it", () => {
  assert.equal(shouldCaptureSessionKeyboardEvent({
    readonly: false,
    active: true,
    visible: true,
    editableTarget: false,
    reservedShortcut: false,
  }), true);

  assert.equal(shouldCaptureSessionKeyboardEvent({
    readonly: false,
    active: true,
    visible: true,
    editableTarget: false,
    reservedShortcut: false,
  }), true);

  assert.equal(shouldCaptureSessionKeyboardEvent({
    readonly: false,
    active: true,
    visible: true,
    editableTarget: false,
    reservedShortcut: false,
  }), true);
});

test("does not capture editable targets reserved shortcuts or read-only sessions", () => {
  for (const blocked of [
    { editableTarget: true },
    { reservedShortcut: true },
    { readonly: true },
    { active: false },
    { visible: false },
  ]) {
    assert.equal(shouldCaptureSessionKeyboardEvent({
      readonly: false,
      active: true,
      visible: true,
      editableTarget: false,
      reservedShortcut: false,
      ...blocked,
    }), false);
  }
});
