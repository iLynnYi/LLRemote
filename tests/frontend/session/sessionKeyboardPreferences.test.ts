import assert from "node:assert/strict";
import test from "node:test";
import { parseSessionKeyboardPreferences, normalizeSessionKeyboardPreferences } from "../../../src/session/input/sessionKeyboardPreferences.ts";

test("keyboard preferences preserve endpoint isolation and require explicit Command remapping", () => {
  assert.deepEqual(normalizeSessionKeyboardPreferences(undefined), { rightControlAsCommand: false });
  assert.deepEqual(parseSessionKeyboardPreferences(JSON.stringify({
    "mac:5900": { captureSystemShortcuts: true, rightControlAsCommand: true },
    "win:5900": { captureSystemShortcuts: false, rightControlAsCommand: false },
  })), {
    "mac:5900": { rightControlAsCommand: true },
    "win:5900": { rightControlAsCommand: false },
  });
  for (const value of [null, "bad", "[]", "null"]) assert.deepEqual(parseSessionKeyboardPreferences(value), {});
  assert.deepEqual(normalizeSessionKeyboardPreferences({ captureSystemShortcuts: "false", rightControlAsCommand: "true" }), { rightControlAsCommand: false });
});

test("legacy capture opt-out cannot disable automatic window capture", () => {
  assert.deepEqual(normalizeSessionKeyboardPreferences({captureSystemShortcuts: false}), {rightControlAsCommand: false});
});
