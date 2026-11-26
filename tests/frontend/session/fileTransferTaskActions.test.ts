import assert from "node:assert/strict";
import test from "node:test";
import type { TransferTask } from "../../../src/session/transfers/fileTransferManager.ts";
import { formatTransferSessionLabel, resolveTransferBatchAction, resolveTransferHistorySelection, resolveTransferTaskPathAction } from "../../../src/session/transfers/fileTransferTaskActions.ts";

const completed: TransferTask = {
  id: "upload-1", sessionId: "session-1", target: "example.local:5900", direction: "upload",
  filePath: "/local/report.txt", fileName: "report.txt", state: "completed",
  transferredBytes: 1, totalBytes: 1, progress: 100, speedBytesPerSecond: 0, etaSeconds: 0,
  remotePath: "/remote/downloads/report.txt", createdAt: "", updatedAt: "",
};

test("batch control pauses running or queued work, resumes paused work and hides for terminal tasks", () => {
  assert.equal(resolveTransferBatchAction([]), null);
  const terminal = (["completed", "failed", "canceled", "skipped"] as const).map(state => ({ ...completed, state }));
  assert.equal(resolveTransferBatchAction(terminal), null);
  const paused = { ...completed, state: "paused" as const };
  assert.equal(resolveTransferBatchAction([...terminal, paused]), "resume");
  for (const state of ["transferring", "queued"] as const) {
    assert.equal(resolveTransferBatchAction([...terminal, paused, { ...completed, state }]), "pause");
  }
});

test("session choices show identical names and addresses only once and retain custom names", () => {
  assert.equal(formatTransferSessionLabel({ label: "192.168.85.128:5903", target: "192.168.85.128:5903" }), "192.168.85.128:5903");
  assert.equal(formatTransferSessionLabel({ label: " 192.168.85.128:5903 ", target: "192.168.85.128:5903" }), "192.168.85.128:5903");
  assert.equal(formatTransferSessionLabel({ label: "", target: "server:5900" }), "server:5900");
  assert.equal(formatTransferSessionLabel({ label: "办公电脑", target: "server:5900" }), "办公电脑 · server:5900");
});

test("history cleanup selects only removable tasks and counts paused transfers for confirmation", () => {
  const states = ["queued", "transferring", "paused", "completed", "failed", "skipped", "canceled"] as const;
  const tasks = states.map(state => ({ id: state, state }));
  assert.deepEqual(resolveTransferHistorySelection(tasks), {
    taskIds: ["paused", "completed", "failed", "skipped", "canceled"], pausedCount: 1,
  });
  assert.deepEqual(resolveTransferHistorySelection([]), { taskIds: [], pausedCount: 0 });
  assert.deepEqual(resolveTransferHistorySelection(tasks.slice(0, 2)), { taskIds: [], pausedCount: 0 });
});

test("history cleanup freezes the supplied filter results without including later tasks", () => {
  const visible = [{ id: "paused-1", state: "paused" as const }, { id: "done-1", state: "completed" as const }];
  const selected = resolveTransferHistorySelection(visible);
  visible.push({ id: "paused-2", state: "paused" });
  assert.deepEqual(selected, { taskIds: ["paused-1", "done-1"], pausedCount: 1 });
});

test("copies the actual completed upload path and resolves its containing directory", () => {
  assert.deepEqual(resolveTransferTaskPathAction([completed], "session-1", "upload-1", "copy-path"), {
    sessionId: "session-1", path: "/remote/downloads/report.txt",
  });
  assert.deepEqual(resolveTransferTaskPathAction([completed], "session-1", "upload-1", "open-directory"), {
    sessionId: "session-1", path: "/remote/downloads",
  });
});

test("rejects another session's task, missing paths and unfinished uploads", () => {
  assert.equal(resolveTransferTaskPathAction([completed], "session-2", "upload-1", "open-directory"), null);
  assert.equal(resolveTransferTaskPathAction([completed], "session-1", "missing", "copy-path"), null);
  assert.equal(resolveTransferTaskPathAction([{ ...completed, remotePath: undefined }], "session-1", "upload-1", "copy-path"), null);
  for (const state of ["queued", "transferring", "paused", "failed", "canceled"] as const) {
    assert.equal(resolveTransferTaskPathAction([{ ...completed, state }], "session-1", "upload-1", "open-directory"), null);
  }
});

test("preserves filesystem roots and resolves path separators independently of the client platform", () => {
  for (const [remotePath, directory] of [["/report.txt", "/"], ["C:\\report.txt", "C:\\"], ["C:\\Downloads\\report.txt", "C:\\Downloads"]]) {
    assert.deepEqual(resolveTransferTaskPathAction([{ ...completed, remotePath }], "session-1", "upload-1", "open-directory"), {
      sessionId: "session-1", path: directory,
    });
  }
});
