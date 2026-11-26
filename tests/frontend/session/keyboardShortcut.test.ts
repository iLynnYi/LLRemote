import assert from "node:assert/strict";
import test from "node:test";
import {
  findKeyboardShortcutConflict,
  formatKeyboardShortcutForPlatform,
  formatKeyboardShortcutSymbolsForPlatform,
  normalizeKeyboardShortcut,
  shortcutFromKeyboardEvent,
  validateKeyboardShortcut,
} from "../../../src/session/input/keyboardShortcut.ts";

function keyboardEvent(input: Partial<KeyboardEvent>): KeyboardEvent {
  return input as KeyboardEvent;
}

test("normalizeKeyboardShortcut normalizes aliases and modifier order", () => {
  assert.equal(normalizeKeyboardShortcut("cmd+shift+f"), "Shift+Meta+F");
  assert.equal(normalizeKeyboardShortcut("Ctrl+Option+esc"), "Control+Alt+Escape");
  assert.equal(normalizeKeyboardShortcut("shift"), "Shift");
});

test("validateKeyboardShortcut accepts standalone function keys but requires modifiers for plain keys", () => {
  assert.deepEqual(validateKeyboardShortcut("F"), {
    valid: false,
    shortcut: "F",
    reason: "missing-modifier",
  });
  assert.deepEqual(validateKeyboardShortcut("1"), {
    valid: false,
    shortcut: "1",
    reason: "missing-modifier",
  });
  assert.deepEqual(validateKeyboardShortcut("Escape"), {
    valid: true,
    shortcut: "Escape",
  });
  assert.deepEqual(validateKeyboardShortcut("F11"), {
    valid: true,
    shortcut: "F11",
  });
  assert.deepEqual(validateKeyboardShortcut("Control"), {
    valid: true,
    shortcut: "Control",
  });
  assert.deepEqual(validateKeyboardShortcut("Control+Shift+F"), {
    valid: true,
    shortcut: "Control+Shift+F",
  });
});

test("findKeyboardShortcutConflict ignores current action and reports duplicate shortcuts", () => {
  const records = [
    { id: "fullscreenExit", labelKey: "settings.shortcutNames.fullscreenExit", shortcut: "Control+Shift+F" },
    { id: "commandPalette", labelKey: "settings.shortcutNames.commandPalette", shortcut: "Control+Shift+P" },
  ];

  assert.equal(findKeyboardShortcutConflict("Control+Shift+F", records, "fullscreenExit"), null);
  assert.deepEqual(findKeyboardShortcutConflict("ctrl+shift+p", records, "fullscreenExit"), records[1]);
});

test("formatKeyboardShortcutForPlatform maps modifier names for the local OS", () => {
  assert.equal(formatKeyboardShortcutForPlatform("Control+Alt+W", "macos"), "Control+Option+W");
  assert.equal(formatKeyboardShortcutForPlatform("Meta+Control+ArrowLeft", "macos"), "Control+Command+←");
  assert.equal(formatKeyboardShortcutForPlatform("Control+Alt+W", "windows"), "Ctrl+Alt+W");
  assert.equal(formatKeyboardShortcutForPlatform("Meta+Control+ArrowRight", "windows"), "Ctrl+Win+→");
});

test("shortcutFromKeyboardEvent uses physical letter codes when option changes event key text", () => {
  assert.equal(shortcutFromKeyboardEvent(keyboardEvent({
    key: "ß",
    code: "KeyS",
    ctrlKey: true,
    altKey: true,
  })), "Control+Alt+S");

  assert.equal(shortcutFromKeyboardEvent(keyboardEvent({
    key: "¨",
    code: "KeyU",
    ctrlKey: true,
    altKey: true,
    shiftKey: true,
  })), "Control+Alt+Shift+U");
});

test("formatKeyboardShortcutSymbolsForPlatform uses compact macOS modifier glyphs", () => {
  assert.equal(formatKeyboardShortcutSymbolsForPlatform("Control+Alt+Enter", "macos"), "⌃⌥↩︎");
  assert.equal(formatKeyboardShortcutSymbolsForPlatform("Control+Alt+Shift+U", "macos"), "⌃⌥⇧U");
  assert.equal(formatKeyboardShortcutSymbolsForPlatform("Meta+Control+ArrowLeft", "macos"), "⌃⌘←");
  assert.equal(formatKeyboardShortcutSymbolsForPlatform("Control+Alt+W", "windows"), "Ctrl+Alt+W");
});
