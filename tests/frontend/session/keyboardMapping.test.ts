import assert from "node:assert/strict";
import test from "node:test";

import {
  keyboardEventToRemoteKeyEvent,
  createRemoteKeyboardMapper,
} from "../../../src/session/input/keyboardMapping.ts";

function keyEvent(input: Partial<KeyboardEvent>): KeyboardEvent {
  return input as KeyboardEvent;
}

test("maps printable DOM keyboard events to RFB key events", () => {
  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "a",
    code: "KeyA",
  })), {
    keysym: 0x61,
    code: "KeyA",
  });

  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "é",
    code: "KeyE",
  })), {
    keysym: 0xe9,
    code: "KeyE",
  });
});

test("maps special DOM keyboard events to RFB key events", () => {
  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "ArrowLeft",
    code: "ArrowLeft",
  })), {
    keysym: 0xff51,
    code: "ArrowLeft",
  });

  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "F12",
    code: "F12",
  })), {
    keysym: 0xffc9,
    code: "F12",
  });
});

test("maps left and right modifier DOM keyboard events to side-specific keysyms", () => {
  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "Shift",
    code: "ShiftLeft",
    location: 1,
  })), {
    keysym: 0xffe1,
    code: "ShiftLeft",
  });

  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "Shift",
    code: "ShiftRight",
    location: 2,
  })), {
    keysym: 0xffe2,
    code: "ShiftRight",
  });

  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "Meta",
    code: "MetaRight",
    location: 2,
  })), {
    keysym: 0xffe8,
    code: "MetaRight",
  });
});

test("preserves local control as the standard RFB Control keysym", () => {
  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "Control",
    code: "ControlLeft",
    location: 1,
  })), {
    keysym: 0xffe3,
    code: "ControlLeft",
  });

  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "Control",
    code: "ControlRight",
    location: 2,
  })), {
    keysym: 0xffe4,
    code: "ControlRight",
  });
});

test("maps non Latin-1 printable characters to Unicode keysyms", () => {
  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "雪",
    code: "KeyX",
  })), {
    keysym: 0x01000000 + 0x96ea,
    code: "KeyX",
  });
});

test("optionally maps only right Control to Command without changing left Control", () => {
  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "Control", code: "ControlRight", location: 2,
  }), { rightControlAsCommand: true }), { keysym: 0xffe8, code: "MetaRight" });
  assert.deepEqual(keyboardEventToRemoteKeyEvent(keyEvent({
    key: "Control", code: "ControlLeft", location: 1,
  }), { rightControlAsCommand: true }), { keysym: 0xffe3, code: "ControlLeft" });
});

test("modifier and printable key releases use their original keydown mapping", () => {
  const mapper = createRemoteKeyboardMapper();
  const right = keyEvent({ key: "Control", code: "ControlRight", location: 2 });
  const down = mapper.map(right, true, { rightControlAsCommand: true });
  assert.deepEqual(mapper.map(right, true), down);
  assert.deepEqual(mapper.map(right, false), down);
  assert.equal(mapper.map(right, true)?.keysym, 0xffe4);
  mapper.map(keyEvent({ key: "A", code: "KeyA" }), true);
  assert.equal(mapper.map(keyEvent({ key: "a", code: "KeyA" }), false)?.keysym, 0x41);
  mapper.clear();
  assert.equal(mapper.map(right, true)?.keysym, 0xffe4);
});

test("late key releases after capture reset cannot enter a new owner", () => {
  const mapper = createRemoteKeyboardMapper();
  const meta = keyEvent({key:"Meta",code:"MetaLeft",location:1});
  mapper.map(meta, true);
  mapper.clear();
  assert.equal(mapper.map(meta, false), null);
  const fresh = createRemoteKeyboardMapper();
  assert.equal(fresh.map(meta, false), null);
  assert.ok(fresh.map(meta, true));
  assert.ok(fresh.map(meta, false));
});
