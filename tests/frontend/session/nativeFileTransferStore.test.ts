import assert from "node:assert/strict";
import test from "node:test";
import { createFileTransferManager, type FileTransferTaskSnapshot } from "../../../src/session/transfers/fileTransferManager.ts";
import { prepareNativeFileTransferTaskStore, preparedFileTransferTaskStore, setFileTransferStoreErrorHandler } from "../../../src/session/transfers/fileTransferStore.ts";

test("native persistence restores tasks and writes row changes without using browser storage", async () => {
  const existing: FileTransferTaskSnapshot = {
    id: "old", sessionId: "s", target: "host:5900", direction: "upload",
    filePath: "/tmp/old", fileName: "old", state: "completed", transferredBytes: 1,
    totalBytes: 1, progress: 100, speedBytesPerSecond: 0, etaSeconds: null,
    createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z",
  };
  const rows = new Map([[existing.id, existing]]);
  let browserWrites = 0;
  let nativeWrites = 0;
  const browser = {
    getItem: () => { throw new Error("native tasks should not read browser storage"); },
    setItem: () => { browserWrites++; throw new DOMException("quota", "QuotaExceededError"); },
  };
  const invoke = async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
    if (command === "load_file_transfer_tasks") return [...rows.values()];
    if (command === "apply_file_transfer_task_changes") {
      nativeWrites++;
      for (const task of args!.upserts as FileTransferTaskSnapshot[]) rows.set(task.id, task);
      for (const id of args!.removes as string[]) rows.delete(id);
      return;
    }
    throw new Error(command);
  };
  await prepareNativeFileTransferTaskStore(invoke);
  const manager = createFileTransferManager({ store: preparedFileTransferTaskStore(browser) });
  assert.equal(manager.tasks.value[0].id, "old");
  await manager.enqueueUploadsDurably(Array.from({ length: 500 }, (_, index) => ({
    sessionId: "s", target: "host:5900", filePath: `/tmp/${index}`,
    upload: async () => ({ fileName: String(index), remotePath: `/remote/${index}`, bytesWritten: 1, sha256: "a" }),
  })));
  await manager.waitForIdle();
  await new Promise(resolve => setTimeout(resolve, 200));
  assert.equal(manager.summary.value.completed, 501);
  assert.equal(rows.size, 501);
  assert.equal(browserWrites, 0);
  assert.ok(nativeWrites > 0);
});

test("failed native insert does not create stalled in-memory tasks", async () => {
  const manager = createFileTransferManager({ store: {
    load: () => [], save: () => {}, insert: async () => { throw new Error("disk full"); },
    applyChanges: () => {},
  } });
  await assert.rejects(manager.enqueueUploadsDurably([{
    sessionId: "s", target: "host:5900", filePath: "/tmp/file",
    upload: async () => ({ fileName: "file", remotePath: "/file", bytesWritten: 1, sha256: "a" }),
  }]), /disk full/);
  assert.deepEqual(manager.tasks.value, []);
});

test("failed native load reports the error and leaves task history untouched", async () => {
  const error = new Error("disk unavailable");
  await prepareNativeFileTransferTaskStore(async command => {
    assert.equal(command, "load_file_transfer_tasks");
    throw error;
  });

  const errors: unknown[] = [];
  setFileTransferStoreErrorHandler(value => errors.push(value));
  assert.deepEqual(errors, [error]);
  assert.deepEqual(preparedFileTransferTaskStore(null)?.load(), []);
  setFileTransferStoreErrorHandler(() => {});
});
