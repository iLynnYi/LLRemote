import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";
import { messageDraftEditShortcut } from "../../../src/session/messages/messageDraftClipboard.ts";

// Exercise the actual stopped-propagation handler without mounting a complete
// connected session or adding a second implementation of its keyboard policy.
const component = readFileSync(new URL("../../../src/features/sessions/components/SessionToolbox.vue", import.meta.url), "utf8");
const source = component.split('<script setup lang="ts">')[1]!.split("</script>")[0]!;
const ast = ts.createSourceFile("toolbox.ts", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
const handler = ast.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "handleMessageDraftKeydown");
assert.ok(handler);
const code = ts.transpileModule(handler.getText(ast), { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;

test("Escape closes the message draft locally, with the same sending and IME guards as modal close", async () => {
  for (const [sending, composing, keyCode, key, open] of [
    [false, false, 27, "Escape", false],
    [true, false, 27, "Escape", true],
    [false, true, 229, "Escape", true],
    [false, false, 229, "Escape", true],
    [false, false, 13, "Enter", true],
  ] as const) {
    const dialog = { value: true };
    let prevented = false;
    let stopped = false;
    const run = runInNewContext(`${code}; handleMessageDraftKeydown`, {
      textSending: { value: sending }, textDialogOpen: dialog,
      messageDraftEditShortcut, HTMLTextAreaElement: class {},
    });
    await run({ key, code: key, keyCode, isComposing: composing,
      ctrlKey: false, metaKey: false, altKey: false, shiftKey: false,
      preventDefault() { prevented = true; }, stopPropagation() { stopped = true; },
    });
    assert.equal(dialog.value, open, `sending=${sending}, composing=${composing}, keyCode=${keyCode}, key=${key}`);
    if (!open) { assert.equal(prevented, true); assert.equal(stopped, true); }
  }
});
