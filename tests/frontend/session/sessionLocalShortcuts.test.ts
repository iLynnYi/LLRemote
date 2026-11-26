import assert from "node:assert/strict";
import test from "node:test";

import {
  matchSessionLocalShortcut,
  sessionLocalShortcutLabel,
  SESSION_LOCAL_SHORTCUTS,
} from "../../../src/session/input/sessionLocalShortcuts.ts";

function keyboardEvent(key: string, options: KeyboardEventInit = {}): KeyboardEvent {
  return {
    key,
    altKey: Boolean(options.altKey),
    ctrlKey: Boolean(options.ctrlKey),
    metaKey: Boolean(options.metaKey),
    shiftKey: Boolean(options.shiftKey),
  } as KeyboardEvent;
}

test("matches focused session viewport shortcuts", () => {
  assert.equal(matchSessionLocalShortcut({
    event: keyboardEvent("w", { ctrlKey: true, altKey: true }),
    state: "connected",
    active: true,
    visible: true,
    editableTarget: false,
  }), "toggle-external-window");

  assert.equal(matchSessionLocalShortcut({
    event: keyboardEvent("Enter", { ctrlKey: true, altKey: true }),
    state: "connected",
    active: true,
    visible: true,
    editableTarget: false,
  }), "toggle-fullscreen");

  for (const [key, actionId] of [["u", "upload-file"], ["t", "open-file-transfer-center"], ["m", "open-message-dialog"]] as const) {
    assert.equal(matchSessionLocalShortcut({
      event: keyboardEvent(key, { ctrlKey: true, altKey: true }),
      state: "connected",
      active: true,
      visible: true,
      editableTarget: false,
    }), actionId);
  }
});

test("leaves retired local shortcut combinations available for remote keyboard input", () => {
  for (const key of ["s", "f", "1", "0", "x"]) {
    assert.equal(matchSessionLocalShortcut({
      event: keyboardEvent(key, { ctrlKey: true, altKey: true }),
      state: "connected",
      active: true,
      visible: true,
      editableTarget: false,
    }), null, `Control+Alt+${key} must not trigger a local action`);
  }
});

test("keeps local session shortcuts scoped to active non-editable viewports", () => {
  for (const blocked of [
    { active: false },
    { visible: false },
    { editableTarget: true },
  ]) {
    assert.equal(matchSessionLocalShortcut({
      event: keyboardEvent("w", { ctrlKey: true, altKey: true }),
      state: "connected",
      active: true,
      visible: true,
      editableTarget: false,
      ...blocked,
    }), null);
  }
});

test("matches retry only for recoverable connection states", () => {
  for (const state of ["failed", "authentication failed", "disconnected"] as const) {
    assert.equal(matchSessionLocalShortcut({
      event: keyboardEvent("Enter"),
      state,
      active: true,
      visible: true,
      editableTarget: false,
    }), "retry-session");
    assert.equal(matchSessionLocalShortcut({
      event: keyboardEvent("r"),
      state,
      active: true,
      visible: true,
      editableTarget: false,
    }), null);
  }

  assert.equal(matchSessionLocalShortcut({
    event: keyboardEvent("Enter"),
    state: "connected",
    active: true,
    visible: true,
    editableTarget: false,
  }), null);
});

test("does not match window shortcuts in failed states", () => {
  assert.equal(matchSessionLocalShortcut({
    event: keyboardEvent("w", { ctrlKey: true, altKey: true }),
    state: "failed",
    active: true,
    visible: true,
    editableTarget: false,
  }), null);
});

test("exposes user-facing labels for toolbox hints", () => {
  assert.equal(sessionLocalShortcutLabel("open-message-dialog", "windows"), "Ctrl+Alt+M");
  assert.equal(sessionLocalShortcutLabel("open-message-dialog", "macos"), "Control+Option+M");
  assert.equal(sessionLocalShortcutLabel("toggle-fullscreen", "macos"), "Control+Option+Enter");
  assert.equal(sessionLocalShortcutLabel("toggle-external-window", "macos"), "Control+Option+W");
  assert.equal(sessionLocalShortcutLabel("toggle-external-window", "windows"), "Ctrl+Alt+W");
  assert.equal(sessionLocalShortcutLabel("upload-file", "macos"), "Control+Option+U");
  assert.equal(sessionLocalShortcutLabel("open-file-transfer-center", "macos"), "Control+Option+T");
  assert.equal(sessionLocalShortcutLabel("retry-session", "macos"), "Enter");
});

test("message shortcut stays local to a connected, focused viewport and appears in settings", () => {
  const definition = SESSION_LOCAL_SHORTCUTS.find((item) => item.actionId === "open-message-dialog");
  assert.equal(definition?.scope, "session-viewport");
  assert.equal(definition?.nameKey, "settings.sessionShortcuts.sendMessage");
  assert.equal(new Set(SESSION_LOCAL_SHORTCUTS.map((item) => item.shortcut)).size, SESSION_LOCAL_SHORTCUTS.length);
  for (const blocked of [{ state: "failed" as const }, { state: "connecting" as const }, { active: false }, { visible: false }, { editableTarget: true }]) {
    assert.equal(matchSessionLocalShortcut({
      event: keyboardEvent("m", { ctrlKey: true, altKey: true }), state: "connected",
      active: true, visible: true, editableTarget: false, ...blocked,
    }), null);
  }
  assert.equal(matchSessionLocalShortcut({
    event: keyboardEvent("m", { ctrlKey: true }), state: "connected",
    active: true, visible: true, editableTarget: false,
  }), null);
});

test("remote capture takes priority over every connected local shortcut", () => {
  for (const key of ["w", "Enter", "u", "t", "m"]) {
    assert.equal(matchSessionLocalShortcut({
      event: keyboardEvent(key, {ctrlKey: true, altKey: true}), state: "connected",
      active: true, visible: true, editableTarget: false, remotePriority: true,
    }), null);
  }
});
