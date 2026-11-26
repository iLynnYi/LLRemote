import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { computed, ref } from "vue";
import { useSessionClipboardReports } from "../../../src/features/sessions/composables/useSessionClipboardReports.ts";

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith("@session/")) {
    return next(new URL(`../../../src/session/${specifier.slice(9)}.ts`, import.meta.url).href, context);
  }
  return next(specifier, context);
} });
const { dispatchFileTransferAction } = await import("../../../src/features/sessions/components/sessionToolbox/fileTransfer.ts");

test("visible transfers stay quiet on success while hidden completion and partial failures remain visible", async t => {
  const descriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: {
    writeText: async () => { throw new Error("clipboard denied"); },
  } });
  t.after(() => {
    if (descriptor) Object.defineProperty(navigator, "clipboard", descriptor);
    else Reflect.deleteProperty(navigator, "clipboard");
  });
  type Options = Parameters<typeof dispatchFileTransferAction>[0];
  let feedback!: NonNullable<Parameters<Options["enqueueSessionFileUploads"]>[2]>;
  let visible = false, copyFailures = 0;
  const messages: Array<{ content: string; tone: string }> = [];
  await dispatchFileTransferAction({
    commandKey: () => "send-file", consumeExternalFileTransferSourcePaths: () => ["/local/file"],
    currentFileTransferReceiveDirectory: () => "/remote", emitExternalToolboxCommand: async () => {},
    externalToolboxFeedbackTarget: null, sessionId: "s1",
    enqueueSessionFileUploads: async (_id, _paths, callbacks) => { feedback = callbacks!; return 1; },
    invoke: async () => [], openTransferCenter: () => { visible = true; },
    isTransferCenterVisible: () => visible, recordFailure: () => { copyFailures++; },
    showMessage: (content, tone) => { messages.push({ content, tone }); }, t: key => key,
  });
  assert.deepEqual(messages, [], "opening the drawer needs no queued Toast");
  feedback.onFileCompleted?.("/remote/file", 1);
  feedback.onBatchFinished?.(1, 0, 0);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(copyFailures, 1);
  assert.deepEqual(messages, [], "a failed automatic copy must not claim success");
  visible = false;
  feedback.onBatchFinished?.(1, 0, 0);
  visible = true;
  feedback.onBatchFinished?.(0, 1, 0);
  feedback.onBatchFinished?.(0, 0, 1);
  assert.deepEqual(messages.map(message => message.tone), ["success", "error", "warning"]);
});

test("copying diagnostics reports only the actual clipboard result to the requesting window", async t => {
  const descriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");
  let failCopy = true;
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: {
    writeText: async () => { if (failCopy) throw new Error("clipboard denied"); },
  } });
  t.after(() => {
    if (descriptor) Object.defineProperty(navigator, "clipboard", descriptor);
    else Reflect.deleteProperty(navigator, "clipboard");
  });
  const messages: Array<{ content: string; tone: string }> = [];
  const owner = useSessionClipboardReports({
    diagnosticReport: computed(() => "report"), errorMessage: ref(""), logEvent() {},
    notify() { assert.fail("feedback must reach the requesting window"); }, t: key => key,
  });
  const notify = (content: string, tone: "success" | "error") => { messages.push({ content, tone }); };
  await owner.copyDiagnosticReport(notify);
  failCopy = false;
  await owner.copyDiagnosticReport(notify);
  assert.deepEqual(messages, [
    { content: "log.diagnosticCopyFailed", tone: "error" },
    { content: "log.diagnosticCopied", tone: "success" },
  ]);
});
