import assert from "node:assert/strict";
import test from "node:test";

import {
  createBrowserFileTransferTaskStore,
} from "../../../src/session/transfers/fileTransferStore.ts";
import type { FileTransferTaskSnapshot } from "../../../src/session/transfers/fileTransferManager.ts";

function memoryStorage(): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    clear: () => data.clear(),
    getItem: (key) => data.get(key) ?? null,
    key: (index) => Array.from(data.keys())[index] ?? null,
    removeItem: (key) => data.delete(key),
    setItem: (key, value) => {
      data.set(key, value);
    },
  };
}

function snapshot(id: string, state: FileTransferTaskSnapshot["state"]): FileTransferTaskSnapshot {
  return {
    id,
    sessionId: "session-1",
    target: "mac.local:5900",
    direction: "upload",
    filePath: `/tmp/${id}.txt`,
    fileName: `${id}.txt`,
    state,
    transferredBytes: state === "completed" ? 1 : 0,
    totalBytes: 1,
    progress: state === "completed" ? 100 : 0,
    speedBytesPerSecond: 0,
    etaSeconds: null,
    createdAt: "2026-07-22T00:00:00.000Z",
    updatedAt: "2026-07-22T00:00:00.000Z",
  };
}

test("browser file transfer store persists pure task snapshots", () => {
  const storage = memoryStorage();
  const store = createBrowserFileTransferTaskStore(storage);

  store.save([snapshot("one", "paused"), snapshot("two", "completed"), snapshot("three", "skipped")]);

  assert.deepEqual(
    createBrowserFileTransferTaskStore(storage).load().map((task) => [task.id, task.state]),
    [["one", "paused"], ["two", "completed"], ["three", "skipped"]],
  );
});

test("malformed persisted containers report errors instead of restoring defaults", () => {
  for (const raw of ["{", "null", "{}", '"old-data"']) {
    const storage = memoryStorage();
    storage.setItem("llremote.file-transfer-tasks.v1", raw);
    const store = createBrowserFileTransferTaskStore(storage)!;
    assert.throws(() => store.load(), { message: "file_transfer_tasks_invalid" });
    assert.equal(storage.getItem("llremote.file-transfer-tasks.v1"), raw);
  }
});

test("a malformed task rejects the whole cache instead of silently filtering entries", () => {
  const storage = memoryStorage();
  const valid = snapshot("valid", "paused");
  const raw = JSON.stringify([valid, { ...valid, id: "bad-number", totalBytes: "10" }, { ...valid, state: "unknown" }, { id: "partial" }]);
  storage.setItem("llremote.file-transfer-tasks.v1", raw);
  const store = createBrowserFileTransferTaskStore(storage)!;
  assert.throws(() => store.load(), { message: "file_transfer_tasks_invalid" });
  assert.equal(storage.getItem("llremote.file-transfer-tasks.v1"), raw);
});

test("task persistence propagates storage failures", () => {
  const unavailable = { getItem() { throw new Error("denied"); }, setItem() { throw new Error("quota"); } };
  const store = createBrowserFileTransferTaskStore(unavailable)!;
  assert.throws(() => store.load(), /denied/);
  assert.throws(() => store.save([snapshot("new", "queued")]), /quota/);
});
