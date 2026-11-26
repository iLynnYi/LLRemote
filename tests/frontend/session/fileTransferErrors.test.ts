import assert from "node:assert/strict";
import test from "node:test";
import { fileTransferErrorMessageKey, isFileTransferConflictSkipped } from "../../../src/session/transfers/fileTransferErrors.ts";
import { zhCN } from "../../../src/i18n/messages/zh-CN.ts";

function message(error: unknown): string {
  const key = fileTransferErrorMessageKey(error);
  const entry = Object.entries(zhCN.sessions.fileTransfers.errors).find(([name]) => key === `sessions.fileTransfers.errors.${name}`);
  assert.ok(entry, `missing translation: ${key}`);
  return entry[1];
}

test("duplicate-file skips are explained as the selected policy, including native string rejections", () => {
  for (const error of ["agent_file_conflict_skipped", new Error("agent_file_conflict_skipped: report.txt")]) {
    assert.equal(isFileTransferConflictSkipped(error), true);
    assert.equal(message(error), "远端已有同名文件，已按“跳过同名文件”设置跳过。");
  }
  assert.equal(isFileTransferConflictSkipped("agent_file_create_failed: agent_file_conflict_skipped"), false);
});

test("transfer errors explain connection, permission, validation and disk failures in Chinese", () => {
  for (const [code, text] of [
    ["agent_channel_unavailable", "远端文件传输连接已断开，请重新连接会话后重试。"],
    ["agent_authentication_required", "远端文件传输授权已失效，请重新连接会话后重试。"],
    ["agent_file_part_write_failed: Access denied", "无法写入远端接收目录，请检查目录权限和磁盘空间后重试。"],
    ["agent_file_checksum_mismatch", "文件校验未通过，请重试上传。"],
    ["agent_file_conflict_requires_choice", "远端已有同名文件，请在“设置 → 文件与权限”调整同名文件处理方式后重试。"],
  ]) assert.equal(message(code), text);
});

test("unknown failures never expose internal error codes or raw system details", () => {
  for (const error of ["agent_future_error: internal details", new Error("some_native_code"), undefined]) {
    assert.equal(message(error), "文件传输未成功，请检查连接、文件权限和远端磁盘空间后重试。");
  }
});
