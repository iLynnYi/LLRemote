import assert from "node:assert/strict";
import test from "node:test";
import { createSessionFileUploadActions } from "../../../src/session/transfers/sessionFileUploads.ts";
import { createInitialConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";
import type { TransferUploadRequest } from "../../../src/session/transfers/fileTransferManager.ts";
import type { FileTransferRenamePolicy } from "../../../src/session/settings/sessionSettings.ts";

function fixture() {
  const queued: TransferUploadRequest[] = [];
  const calls: unknown[] = [];
  let sources: unknown = [{ filePath: "/one.txt", relativePath: "folder/one.txt" }, { filePath: "/two.txt" }];
  const model = createInitialConnectionModel({ state: "not-matched" });
  model.capabilities.items.push({ id: "file.transfer.upload", state: "supported", source: "agent" });
  let connected = true;
  let renamePolicy: FileTransferRenamePolicy = "skip";
  const actions = createSessionFileUploadActions({
    findSession: () => ({ target: "host:5900" }),
    getConnection: () => connected ? {
      connectionModel: model,
      async controlRemoteFileTransfer(id, action) { calls.push([id, action]); },
      async uploadRemoteFile(path, options) {
        if (options?.signal?.aborted) throw new Error("agent_file_paused");
        calls.push([path, options?.targetDirectory, options?.renamePolicy, options?.transferId, options?.relativePath]);
        options?.onProgress?.({ transferId: "one", fileName: path, transferredBytes: 3, totalBytes: 3, phase: "verifying" });
        if (path === "/two.txt") throw new Error("upload-failed");
        if (path === "/duplicate.txt") throw new Error("agent_file_conflict_skipped");
        return { fileName: "one.txt", remotePath: "/receive/one.txt", bytesWritten: 3, sha256: "checksum" };
      },
    } : undefined,
    getRenamePolicy: () => renamePolicy,
    expandSources: async () => sources,
    enqueueUploads: requests => { queued.push(...requests); return []; },
    t: key => key,
    onUploaded: result => calls.push(result.remotePath),
  });
  return { actions, queued, calls, model, setRenamePolicy: (value: FileTransferRenamePolicy) => { renamePolicy = value; }, disconnect: () => { connected = false; }, sources: (value: unknown) => { sources = value; } };
}

test("shared upload actions preserve source metadata, settings, progress, control and batch results", async () => {
  const f = fixture();
  const batches: number[][] = [], completed: unknown[] = [], progress: unknown[] = [];
  assert.equal(await f.actions.enqueue("session", ["/folder"], {
    onFileCompleted: (path, count) => completed.push([path, count]),
    onBatchFinished: (success, failed) => batches.push([success, failed]),
  }), 2);
  assert.equal(f.queued[0].relativePath, "folder/one.txt");
  assert.equal(f.queued[0].renamePolicy, "skip");
  await f.queued[0].upload({ taskId: "task-one", signal: new AbortController().signal, onProgress: update => progress.push(update) });
  await f.queued[0].control?.("task-one", "pause");
  await assert.rejects(f.queued[1].upload({ taskId: "task-two", signal: new AbortController().signal, onProgress() {} }), /upload-failed/);
  assert.deepEqual(completed, [["/receive/one.txt", 2]]);
  assert.deepEqual(progress, [{ transferredBytes: 3, totalBytes: 3, phase: "verifying" }]);
  assert.deepEqual(batches, [[1, 1]]);
  assert.deepEqual(f.calls[0], ["/one.txt", undefined, "skip", "task-one", "folder/one.txt"]);
  assert.ok(f.calls.some(value => Array.isArray(value) && value[0] === "task-one" && value[1] === "pause"));
});

test("batch feedback counts skipped duplicates separately without reporting upload success", async () => {
  const f = fixture();
  f.sources([{ filePath: "/duplicate.txt" }, { filePath: "/two.txt" }, { filePath: "/one.txt" }]);
  const batches: number[][] = [], completed: string[] = [];
  await f.actions.enqueue("session", ["/folder"], {
    onFileCompleted: path => completed.push(path),
    onBatchFinished: (success, failed, skipped) => batches.push([success, failed, skipped]),
  });
  for (const [index, task] of f.queued.entries()) {
    try { await task.upload({ taskId: String(index), signal: new AbortController().signal, onProgress() {} }); }
    catch { /* Expected skip and failure both settle this batch. */ }
  }
  assert.deepEqual(batches, [[1, 1, 1]]);
  assert.deepEqual(completed, ["/receive/one.txt"]);
});

test("each upload attempt reads the current global conflict policy and uses the remote default directory", async () => {
  const f = fixture();
  f.sources([{ filePath: "/one.txt" }]);
  await f.actions.enqueue("session", ["/one.txt"]);
  assert.equal(f.queued[0].targetDirectory, undefined);
  f.setRenamePolicy("overwrite");
  await f.queued[0].upload({ taskId: "first", signal: new AbortController().signal, onProgress() {} });
  assert.deepEqual(f.calls[0], ["/one.txt", undefined, "overwrite", "first", undefined]);
  f.setRenamePolicy("autoRename");
  await f.queued[0].upload({ taskId: "second", signal: new AbortController().signal, onProgress() {} });
  assert.ok(f.calls.some(value => Array.isArray(value) && value[2] === "autoRename" && value[3] === "second"));
});

test("shared actions reject missing capabilities and malformed source expansion without queueing", async () => {
  const f = fixture();
  for (const value of [null, {}, [], [{ filePath: 3 }]]) {
    f.sources(value);
    await assert.rejects(f.actions.enqueue("session", ["/source"]), /noFileSelected/);
  }
  f.model.capabilities.items = [];
  await assert.rejects(f.actions.enqueue("session", ["/source"]), /enhancedCapability/);
  assert.equal(f.queued.length, 0);
});

test("queued callbacks recheck the current connection and finish a failed batch when disconnected", async () => {
  const f = fixture();
  f.sources([{ filePath: "/one.txt" }]);
  const batches: number[][] = [];
  await f.actions.enqueue("session", ["/one.txt"], { onBatchFinished: (success, failed) => batches.push([success, failed]) });
  f.disconnect();
  await assert.rejects(f.queued[0].upload({ taskId: "one", signal: new AbortController().signal, onProgress() {} }), /disconnected/);
  await assert.rejects(f.queued[0].control!("one", "cancel"), /disconnected/);
  assert.deepEqual(batches, [[0, 1]]);
});

test("pausing an upload does not report batch failure and continuing settles it once", async () => {
  const f = fixture();
  f.sources([{ filePath: "/one.txt" }]);
  const batches: number[][] = [];
  await f.actions.enqueue("session", ["/one.txt"], { onBatchFinished: (success, failed) => batches.push([success, failed]) });
  const paused = new AbortController();
  paused.abort("paused");
  await assert.rejects(f.queued[0].upload({ taskId: "one", signal: paused.signal, onProgress() {} }), /agent_file_paused/);
  assert.deepEqual(batches, []);
  await f.queued[0].upload({ taskId: "one", signal: new AbortController().signal, onProgress() {} });
  assert.deepEqual(batches, [[1, 0]]);
});
