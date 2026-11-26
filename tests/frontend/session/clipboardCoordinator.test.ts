import assert from "node:assert/strict";
import test from "node:test";
import { createClipboardCoordinator } from "../../../src/session/clipboard/clipboardCoordinator.ts";

class Clipboard {
  text = "";
  writes: string[] = [];
  async readText() { return this.text; }
  async writeText(text: string) { this.text = text; this.writes.push(text); }
}

class DeferredWriteClipboard extends Clipboard {
  private resolveWrite: (() => void) | undefined;

  async writeText(text: string) {
    await new Promise<void>((resolve) => { this.resolveWrite = resolve; });
    await super.writeText(text);
  }

  finishWrite() { this.resolveWrite?.(); }
}

class ReadErrorClipboard extends Clipboard {
  private readonly error: Error;

  constructor(error: Error) {
    super();
    this.error = error;
  }

  async readText() {
    throw this.error;
  }
}

test("ignores an empty or unsupported system clipboard while registering a session", async () => {
  const clipboard = new ReadErrorClipboard(new Error("The clipboard contents were not available in the requested format or the clipboard is empty."));
  const warnings: string[] = [];
  const coordinator = createClipboardCoordinator(clipboard, { onWarning: (message) => warnings.push(message) });

  await assert.doesNotReject(() => coordinator.register("session", { sendText: () => undefined }, false));
  assert.deepEqual(warnings, []);
  coordinator.dispose();
});

test("logs other system clipboard read failures without interrupting session registration", async () => {
  const clipboard = new ReadErrorClipboard(new Error("clipboard permission denied"));
  const warnings: string[] = [];
  const coordinator = createClipboardCoordinator(clipboard, { onWarning: (message) => warnings.push(message) });

  await assert.doesNotReject(() => coordinator.register("session", { sendText: () => undefined }, false));
  assert.deepEqual(warnings, ["System clipboard read failed: Error: clipboard permission denied"]);
  coordinator.dispose();
});

test("broadcasts client text to every writable connected session", async () => {
  const clipboard = new Clipboard(); clipboard.text = "中文";
  const sent: string[] = [];
  const coordinator = createClipboardCoordinator(clipboard);
  await coordinator.register("a", { sendText: (text) => sent.push(`a:${text}`) }, false);
  await coordinator.register("b", { sendText: (text) => sent.push(`b:${text}`) }, false);
  await coordinator.register("readonly", { sendText: (text) => sent.push(`r:${text}`) }, true);
  assert.deepEqual(sent, ["a:中文", "b:中文"]);
  coordinator.dispose();
});

test("accepts remote text only from the active writable session and suppresses its echo", async () => {
  const clipboard = new Clipboard();
  const sent: string[] = [];
  const coordinator = createClipboardCoordinator(clipboard);
  await coordinator.register("a", { sendText: (text) => sent.push(`a:${text}`) }, false);
  await coordinator.register("b", { sendText: (text) => sent.push(`b:${text}`) }, false);
  await coordinator.setActiveSession("a");
  assert.equal(await coordinator.receiveRemoteText("b", "ignored"), false);
  assert.equal(await coordinator.receiveRemoteText("a", "来自远端"), true);
  await coordinator.syncClientClipboard();
  assert.deepEqual(clipboard.writes, ["来自远端"]);
  assert.deepEqual(sent, []);
  coordinator.dispose();
});

test("does not broadcast the pre-write Client clipboard while accepting remote text", async () => {
  const clipboard = new DeferredWriteClipboard(); clipboard.text = "Client 旧文本";
  const sent: string[] = [];
  const coordinator = createClipboardCoordinator(clipboard);
  await coordinator.register("active", { sendText: (text) => sent.push(`active:${text}`) }, false);
  await coordinator.register("other", { sendText: (text) => sent.push(`other:${text}`) }, false);
  sent.length = 0;
  await coordinator.setActiveSession("active");
  sent.length = 0;

  const incoming = coordinator.receiveRemoteText("active", "远端文本");
  await coordinator.syncClientClipboard();
  clipboard.finishWrite();
  assert.equal(await incoming, true);
  coordinator.dispose();
  assert.deepEqual(sent, []);
});

test("pushes the current Client clipboard when Active changes", async () => {
  const clipboard = new Clipboard(); clipboard.text = "切换时的中文";
  const sent: string[] = [];
  const coordinator = createClipboardCoordinator(clipboard);
  await coordinator.register("old", { sendText: () => undefined }, false);
  await coordinator.register("next", { sendText: (text) => sent.push(text) }, false);
  sent.length = 0;
  await coordinator.setActiveSession("next");
  assert.deepEqual(sent, ["切换时的中文"]);
  coordinator.dispose();
});

test("broadcasts a Client copy made immediately before an Active switch", async () => {
  const clipboard = new Clipboard(); clipboard.text = "旧文本";
  const sent: string[] = [];
  const coordinator = createClipboardCoordinator(clipboard);
  await coordinator.register("third-party", { sendText: (text) => sent.push(`third:${text}`) }, false);
  await coordinator.register("app", { sendText: (text) => sent.push(`app:${text}`) }, false);
  sent.length = 0;
  clipboard.text = "刚复制的中文";
  await coordinator.setActiveSession("app");
  assert.deepEqual(sent, ["third:刚复制的中文", "app:刚复制的中文"]);
  coordinator.dispose();
});
