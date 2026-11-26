import assert from "node:assert/strict";
import test from "node:test";
import { summarizeAgentPermissionDiagnostics } from "../../../src/session/diagnostics/agentPermissionDiagnostics.ts";

test("permission diagnostics explain every check and leave unreported items unknown", () => {
  const result = summarizeAgentPermissionDiagnostics([]);
  assert.equal(result.length, 5);
  assert.ok(result.every((item) => item.status === "unknown" && item.purpose && item.actionHint));
  assert.equal(result.find((item) => item.kind === "screen_recording")?.label, "屏幕录制权限");
});

test("permission diagnostics distinguish command availability, system grants and unsupported capabilities", () => {
  const rawMessage = crypto.randomUUID();
  const result = summarizeAgentPermissionDiagnostics([
    { kind: "clipboard", status: "granted", requiredFor: "clipboard.text.write", message: rawMessage },
    { kind: "accessibility", status: "missing", requiredFor: "input", message: rawMessage },
    { kind: "file_system", status: "unsupported", requiredFor: "files", message: rawMessage },
  ]);
  assert.equal(result.find((item) => item.kind === "clipboard")?.statusText, "命令可用");
  const accessibility = result.find((item) => item.kind === "accessibility")!;
  assert.equal(accessibility.statusText, "未授权");
  assert.match(accessibility.actionHint, /辅助功能/);
  const files = result.find((item) => item.kind === "file_system")!;
  assert.equal(files.statusText, "暂不支持");
  assert.doesNotMatch(files.actionHint, /授权/);
  assert.equal(JSON.stringify(result).includes(rawMessage), false);
});
