import assert from "node:assert/strict";
import test from "node:test";
import { editMessageDraftClipboard, messageDraftEditShortcut } from "../../../src/session/messages/messageDraftClipboard.ts";

function editor(value: string, selectionStart: number, selectionEnd = selectionStart) {
  return {
    value, selectionStart, selectionEnd,
    replaceSelection(text: string) {
      this.value = this.value.slice(0, this.selectionStart) + text + this.value.slice(this.selectionEnd);
      this.selectionStart += text.length;
      this.selectionEnd = this.selectionStart;
    },
    selectAll() { this.selectionStart = 0; this.selectionEnd = this.value.length; },
  };
}

test("recognizes local Ctrl/Command edit shortcuts without claiming remote Alt or IME combinations", () => {
  for (const modifier of ["ctrlKey", "metaKey"]) {
    for (const [key, action] of [["c", "copy"], ["x", "cut"], ["v", "paste"], ["a", "selectAll"]]) {
      const event = { key, code: `Key${key.toUpperCase()}`, ctrlKey: false, metaKey: false,
        altKey: false, shiftKey: false, isComposing: false, [modifier]: true };
      assert.equal(messageDraftEditShortcut(event), action);
      assert.equal(messageDraftEditShortcut({ ...event, altKey: true }), null);
      assert.equal(messageDraftEditShortcut({ ...event, isComposing: true }), null);
    }
  }
});

test("copy writes only the selection and empty copy leaves the clipboard alone", async () => {
  const writes: string[] = [];
  const input = editor("前缀你好🐱后缀", 2, 6);
  const clipboard = { writeText: async (text: string) => { writes.push(text); } };
  await editMessageDraftClipboard("copy", input, clipboard, () => true);
  assert.deepEqual(writes, ["你好🐱"]);
  assert.equal(input.value, "前缀你好🐱后缀");
  input.selectionEnd = input.selectionStart;
  await editMessageDraftClipboard("copy", input, clipboard, () => true);
  assert.equal(writes.length, 1);
});

test("paste inserts at the caret or replaces selected text including Unicode and newlines", async () => {
  const clipboard = { readText: async () => "你好🐱\n第二行", writeText: async () => {} };
  for (const [start, end, expected] of [[1, 1, "a你好🐱\n第二行bc"], [1, 2, "a你好🐱\n第二行c"]] as const) {
    const input = editor("abc", start, end);
    await editMessageDraftClipboard("paste", input, clipboard, () => true);
    assert.equal(input.value, expected);
    assert.equal(input.selectionStart, 1 + "你好🐱\n第二行".length);
    assert.equal(input.selectionEnd, input.selectionStart);
  }
});

test("empty or unavailable clipboard does not erase the selected draft", async () => {
  const input = editor("selected draft", 0, 14);
  await editMessageDraftClipboard("paste", input, { readText: async () => "", writeText: async () => {} }, () => true);
  assert.equal(input.value, "selected draft");
  await assert.rejects(editMessageDraftClipboard("paste", input, undefined, () => true), /clipboard_unavailable/);
  assert.equal(input.value, "selected draft");
});

test("cut removes text only after a successful write and select-all needs no clipboard access", async () => {
  const input = editor("abc", 1, 2);
  await assert.rejects(editMessageDraftClipboard("cut", input, { writeText: async () => { throw new Error("denied"); } }, () => true), /denied/);
  assert.equal(input.value, "abc");
  await editMessageDraftClipboard("cut", input, { writeText: async (text) => { assert.equal(text, "b"); } }, () => true);
  assert.equal(input.value, "ac");
  await editMessageDraftClipboard("selectAll", input, undefined, () => true);
  assert.equal(input.selectionStart, 0);
  assert.equal(input.selectionEnd, 2);
});

test("late paste cannot change a moved caret, edited draft, closed dialog or another focused input", async () => {
  for (const change of ["selection", "value", "inactive"]) {
    const input = editor("abc", 1);
    let active = true;
    let resolve!: (text: string) => void;
    const text = new Promise<string>((done) => { resolve = done; });
    const pending = editMessageDraftClipboard("paste", input, { readText: () => text, writeText: async () => {} }, () => active);
    if (change === "selection") input.selectionStart = input.selectionEnd = 2;
    if (change === "value") input.value = "changed";
    if (change === "inactive") active = false;
    resolve("late paste");
    await pending;
    assert.equal(input.value, change === "value" ? "changed" : "abc");
  }
});
