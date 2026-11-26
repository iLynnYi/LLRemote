import assert from "node:assert/strict";
import test from "node:test";

import { matchRemoteDesktopShortcut } from "../../../src/session/input/sessionRemoteDesktopShortcuts.ts";

function keyboardEvent(key: string, options: KeyboardEventInit = {}): KeyboardEvent {
  return {
    key,
    altKey: Boolean(options.altKey),
    ctrlKey: Boolean(options.ctrlKey),
    metaKey: Boolean(options.metaKey),
    shiftKey: Boolean(options.shiftKey),
  } as KeyboardEvent;
}

test("reserves only dedicated desktop shortcuts and preserves native remote arrow shortcuts", () => {
  assert.equal(matchRemoteDesktopShortcut({
    event: keyboardEvent("ArrowLeft", { ctrlKey: true, altKey: true }),
    externalWindow: true,
  }), "previous-desktop");

  assert.equal(matchRemoteDesktopShortcut({
    event: keyboardEvent("ArrowRight", { ctrlKey: true }),
    externalWindow: true,
  }), null);

  assert.equal(matchRemoteDesktopShortcut({
    event: keyboardEvent("ArrowUp", { metaKey: true }),
    externalWindow: true,
  }), null);

  assert.equal(matchRemoteDesktopShortcut({
    event: keyboardEvent("ArrowUp", { metaKey: true }),
    externalWindow: false,
  }), null);

  assert.equal(matchRemoteDesktopShortcut({
    event: keyboardEvent("ArrowDown"),
    externalWindow: true,
  }), null);
});

test("capture sends dedicated desktop aliases as original remote keys", () => {
  const shortcuts = [
    ["ArrowLeft", "previous-desktop"],
    ["ArrowRight", "next-desktop"],
    ["ArrowUp", "overview"],
    ["ArrowDown", "show-current-desktop"],
  ];
  for (const [key, action] of shortcuts) {
    const input = { event: keyboardEvent(key, { ctrlKey: true, altKey: true }), externalWindow: true };
    assert.equal(matchRemoteDesktopShortcut(input), action);
    assert.equal(matchRemoteDesktopShortcut({ ...input, remotePriority: true }), null);
  }
});
