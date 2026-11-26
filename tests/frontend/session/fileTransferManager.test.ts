import assert from "node:assert/strict";
import test from "node:test";

import {
  createFileTransferManager,
  type FileTransferTaskStore,
  type FileTransferTaskSnapshot,
  type TransferProgressUpdate,
  type TransferUploadRequest,
} from "../../../src/session/transfers/fileTransferManager.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((innerResolve, innerReject) => {
    resolve = innerResolve;
    reject = innerReject;
  });
  return { promise, resolve, reject };
}

function request(filePath: string, sessionId = "session-1"): TransferUploadRequest {
  return {
    sessionId,
    target: "mac.local:5900",
    filePath,
    upload: async () => ({
      fileName: filePath.split("/").pop() ?? filePath,
      remotePath: `/remote/${filePath.split("/").pop() ?? filePath}`,
      bytesWritten: 1,
      sha256: "a".repeat(64),
    }),
  };
}

function memoryTaskStore(): FileTransferTaskStore {
  let snapshots: ReturnType<FileTransferTaskStore["load"]> = [];
  return {
    load: () => snapshots.map((task) => ({ ...task })),
    save: (tasks) => {
      snapshots = tasks.map((task) => ({ ...task }));
    },
  };
}

test("shows upload preparation and verification without treating either as transfer speed", async () => {
  let timestamp = 0;
  const pending = deferred<Awaited<ReturnType<TransferUploadRequest["upload"]>>>();
  let reportProgress!: (progress: TransferProgressUpdate) => void;
  const manager = createFileTransferManager({ now: () => new Date(timestamp).toISOString() });
  manager.enqueueUploads([{
    ...request("/tmp/large.bin"),
    upload: ({ onProgress }) => { reportProgress = onProgress; return pending.promise; },
  }]);
  await manager.waitForIdleTick();
  assert.equal(manager.tasks.value[0].phase, "preparing");
  reportProgress({ transferredBytes: 0, totalBytes: 100, phase: "preparing" });
  timestamp = 10_000;
  reportProgress({ transferredBytes: 0, totalBytes: 100, phase: "transferring" });
  assert.equal(manager.tasks.value[0].speedBytesPerSecond, 0);
  assert.equal(manager.tasks.value[0].etaSeconds, null);
  timestamp = 11_000;
  reportProgress({ transferredBytes: 50, totalBytes: 100, phase: "transferring" });
  assert.equal(manager.tasks.value[0].speedBytesPerSecond, 50);
  reportProgress({ transferredBytes: 100, totalBytes: 100, phase: "verifying" });
  assert.equal(manager.tasks.value[0].phase, "verifying");
  assert.equal(manager.tasks.value[0].speedBytesPerSecond, 0);
  assert.equal(manager.tasks.value[0].etaSeconds, null);
  pending.resolve({ fileName: "large.bin", remotePath: "/remote/large.bin", bytesWritten: 100, sha256: "a".repeat(64) });
  await manager.waitForIdle();
  assert.equal(manager.tasks.value[0].state, "completed");
  assert.equal(manager.tasks.value[0].phase, undefined);
});

test("duplicate files skipped by policy are terminal history, not failed uploads", async () => {
  const store = memoryTaskStore();
  const manager = createFileTransferManager({ store, maxConcurrent: 1 });
  manager.enqueueUploads([
    { ...request("/duplicate.txt"), upload: async () => { throw new Error("agent_file_conflict_skipped"); } },
    request("/next.txt"),
  ]);
  await manager.waitForIdle();
  assert.deepEqual(manager.tasks.value.map(task => task.state), ["skipped", "completed"]);
  assert.equal(manager.summary.value.failed, 0);
  assert.equal(manager.summary.value.skipped, 1);
  assert.equal(manager.history.value.length, 2);
  manager.cancelAll();
  assert.equal(manager.tasks.value[0].state, "skipped");

  const restored = createFileTransferManager({ store });
  assert.equal(restored.tasks.value[0].state, "skipped");
  await restored.clearHistory("session-1", [restored.tasks.value[0].id]);
  assert.deepEqual(restored.tasks.value.map(task => task.state), ["completed"]);
});

test("storage quota failure leaves no unscheduled upload tasks", async () => {
  const manager = createFileTransferManager({
    store: { load: () => [], save: tasks => {
      if (tasks.length) throw new DOMException("The quota has been exceeded.", "QuotaExceededError");
    } },
  });
  assert.throws(() => manager.enqueueUploads([request("/folder/one.txt")]), { name: "QuotaExceededError" });
  assert.deepEqual(manager.tasks.value, []);
  assert.equal(manager.summary.value.queued, 0);
});

test("legacy duplicate-file failures restore as skipped instead of resumable tasks", async () => {
  const manager = createFileTransferManager();
  manager.enqueueUploads([request("/legacy.txt")]);
  await manager.waitForIdle();
  const restored = createFileTransferManager({ restoredTasks: [{
    ...manager.tasks.value[0], state: "failed", transferredBytes: 0, totalBytes: 0, progress: 0,
    errorMessage: "agent_file_conflict_skipped: existing.txt",
  }] });
  assert.equal(restored.tasks.value[0].state, "skipped");
  restored.resumeAll();
  assert.equal(restored.tasks.value[0].state, "skipped");
  assert.equal(restored.summary.value.failed, 0);
});

test("tracks progress speed and eta from chunk progress updates", async () => {
  let timestamp = 0;
  const manager = createFileTransferManager({
    now: () => new Date(timestamp).toISOString(),
  });

  manager.enqueueUploads([{
    ...request("/tmp/progress.txt"),
    upload: async ({ onProgress }) => {
      timestamp = 1_000;
      onProgress({ transferredBytes: 25, totalBytes: 100 });
      timestamp = 2_000;
      onProgress({ transferredBytes: 50, totalBytes: 100 });
      return {
        fileName: "progress.txt",
        remotePath: "/remote/progress.txt",
        bytesWritten: 100,
        sha256: "c".repeat(64),
      };
    },
  }]);

  await manager.waitForIdle();

  const task = manager.tasks.value[0];
  assert.equal(task.progress, 100);
  assert.equal(task.transferredBytes, 100);
  assert.equal(task.totalBytes, 100);
  assert.equal(task.speedBytesPerSecond, 25);
  assert.equal(task.etaSeconds, 0);
  assert.equal(manager.summary.value.totalBytes, 100);
  assert.equal(manager.summary.value.transferredBytes, 100);
  assert.equal(manager.summary.value.totalProgress, 100);
});

test("speed and ETA do not oscillate with subsecond chunk acknowledgements", async () => {
  let timestamp = 0;
  const samples: Array<{ bytes: number; speed: number; eta: number | null }> = [];
  const completed = deferred<Awaited<ReturnType<TransferUploadRequest["upload"]>>>();
  const manager = createFileTransferManager({ now: () => new Date(timestamp).toISOString() });
  manager.enqueueUploads([{
    ...request("/tmp/chunks.bin"),
    upload: ({ onProgress }) => {
      const totalBytes = 3_300_000;
      for (const [at, transferredBytes] of [[0, 0], [100, 64_000], [120, 128_000], [220, 192_000], [240, 256_000], [1000, 320_000], [1100, 384_000], [2000, 1_024_000]]) {
        timestamp = at;
        onProgress({ transferredBytes, totalBytes });
        const task = manager.tasks.value[0];
        samples.push({ bytes: task.transferredBytes, speed: task.speedBytesPerSecond, eta: task.etaSeconds });
      }
      return completed.promise;
    },
  }]);
  await manager.waitForIdleTick();
  assert.deepEqual(samples.map(sample => sample.bytes), [0, 64_000, 128_000, 192_000, 256_000, 320_000, 384_000, 1_024_000]);
  assert.deepEqual(samples.slice(0, 5).map(sample => sample.speed), [0, 0, 0, 0, 0]);
  assert.deepEqual(samples.slice(0, 5).map(sample => sample.eta), [null, null, null, null, null]);
  assert.ok(samples[5].speed > 0);
  assert.equal(samples[6].speed, samples[5].speed);
  assert.equal(samples[6].eta, samples[5].eta);
  assert.ok(samples[7].speed > samples[5].speed && samples[7].speed < 704_000, "a new sample moves the displayed speed gradually");
  assert.ok(samples[7].eta !== samples[6].eta);
  completed.resolve({ fileName: "chunks.bin", remotePath: "/remote/chunks.bin", bytesWritten: 3_300_000, sha256: "a" });
  await manager.waitForIdle();
});

test("pauses an active upload and resumes it with the same task id", async () => {
  const started: string[] = [];
  const controls: Array<[string, string]> = [];
  const firstStop = deferred<void>();
  const manager = createFileTransferManager({
    maxConcurrent: 1,
    maxConcurrentPerTarget: 1,
    id: () => "transfer-1",
  });

  manager.enqueueUploads([{
    ...request("/tmp/resume.txt"),
    control: async (taskId, action) => {
      controls.push([taskId, action]);
    },
    upload: async ({ taskId, signal, onProgress }) => {
      started.push(taskId);
      onProgress({ transferredBytes: started.length === 1 ? 40 : 100, totalBytes: 100 });
      if (started.length === 1) {
        await new Promise((resolve) => setTimeout(resolve, 0));
        if (signal.aborted) {
          await firstStop.promise;
          throw new Error("agent_file_paused");
        }
      }
      return {
        fileName: "resume.txt",
        remotePath: "/remote/resume.txt",
        bytesWritten: 100,
        sha256: "d".repeat(64),
      };
    },
  }]);

  await manager.waitForIdleTick();
  manager.pauseTask("transfer-1");
  await manager.waitForIdleTick();

  assert.equal(manager.tasks.value[0].state, "paused");
  assert.equal(manager.tasks.value[0].transferredBytes, 40);

  manager.resumeTask("transfer-1");
  await manager.waitForIdleTick();
  assert.deepEqual(started, ["transfer-1"], "resume must wait until the paused worker exits");
  firstStop.resolve();
  await manager.waitForIdle();

  assert.deepEqual(started, ["transfer-1", "transfer-1"]);
  assert.deepEqual(controls, [["transfer-1", "pause"]]);
  assert.equal(manager.tasks.value[0].state, "completed");
  assert.equal(manager.tasks.value[0].progress, 100);
});

test("does not start a resumed task while its paused worker is still active", async () => {
  const started: string[] = [];
  const firstStop = deferred<void>();
  const manager = createFileTransferManager({
    maxConcurrent: 3,
    maxConcurrentPerTarget: 3,
    id: () => "transfer-parallel-resume",
  });

  manager.enqueueUploads([{
    ...request("/tmp/parallel-resume.txt"),
    upload: async ({ taskId, signal, onProgress }) => {
      started.push(taskId);
      onProgress({ transferredBytes: started.length === 1 ? 32 : 100, totalBytes: 100 });
      if (started.length === 1) {
        await new Promise((resolve) => setTimeout(resolve, 0));
        if (signal.aborted) {
          await firstStop.promise;
          throw new Error("agent_file_paused");
        }
      }
      return {
        fileName: "parallel-resume.txt",
        remotePath: "/remote/parallel-resume.txt",
        bytesWritten: 100,
        sha256: "f".repeat(64),
      };
    },
  }]);

  await manager.waitForIdleTick();
  manager.pauseTask("transfer-parallel-resume");
  manager.resumeTask("transfer-parallel-resume");
  await manager.waitForIdleTick();

  assert.deepEqual(started, ["transfer-parallel-resume"], "resume must not duplicate an active paused worker");
  firstStop.resolve();
  await manager.waitForIdle();

  assert.deepEqual(started, ["transfer-parallel-resume", "transfer-parallel-resume"]);
  assert.equal(manager.tasks.value[0].state, "completed");
});

test("batch pause and resume respect the selected session and include all sessions when unfiltered", async () => {
  const stop = deferred<void>();
  const pausedRemotely: string[] = [];
  const manager = createFileTransferManager({ maxConcurrent: 1, maxConcurrentPerTarget: 1 });
  manager.enqueueUploads([request("/completed.txt")]);
  await manager.waitForIdle();

  const pending = (path: string, sessionId: string): TransferUploadRequest => ({
    ...request(path, sessionId),
    control: async (taskId, action) => { if (action === "pause") pausedRemotely.push(taskId); },
    upload: async context => {
      await stop.promise;
      return request(path, sessionId).upload(context);
    },
  });
  manager.enqueueUploads([
    pending("/one.txt", "session-1"),
    pending("/two.txt", "session-1"),
    pending("/other.txt", "session-2"),
  ]);
  await manager.waitForIdleTick();
  const states = () => manager.tasks.value.map(task => task.state);
  assert.deepEqual(states(), ["completed", "transferring", "queued", "queued"]);

  manager.pauseAll("session-1");
  assert.deepEqual(states(), ["completed", "paused", "paused", "queued"]);
  assert.deepEqual(pausedRemotely, [manager.tasks.value[1].id], "queued tasks have no remote transfer to pause");
  manager.resumeAll("session-2");
  assert.deepEqual(states(), ["completed", "paused", "paused", "queued"]);
  manager.resumeAll("session-1");
  assert.deepEqual(states(), ["completed", "queued", "queued", "queued"]);

  manager.pauseAll();
  assert.deepEqual(states(), ["completed", "paused", "paused", "paused"]);
  manager.resumeAll("session-2");
  assert.deepEqual(states(), ["completed", "paused", "paused", "queued"]);
  manager.resumeAll();
  assert.deepEqual(states(), ["completed", "queued", "queued", "queued"]);

  manager.cancelAll();
  stop.resolve();
  await manager.waitForIdle();
  assert.equal(manager.tasks.value[0].state, "completed");
});

test("failed uploads keep their offset when retried for runtime resume", async () => {
  const manager = createFileTransferManager({ id: () => "transfer-failed-resume" });
  let attempts = 0;

  manager.enqueueUploads([{
    ...request("/tmp/interrupted.txt"),
    upload: async ({ onProgress }) => {
      attempts += 1;
      if (attempts === 1) {
        onProgress({ transferredBytes: 64, totalBytes: 128 });
        throw new Error("agent_unreachable");
      }
      onProgress({ transferredBytes: 128, totalBytes: 128 });
      return {
        fileName: "interrupted.txt",
        remotePath: "/remote/interrupted.txt",
        bytesWritten: 128,
        sha256: "e".repeat(64),
      };
    },
  }]);

  await manager.waitForIdle();
  assert.equal(manager.tasks.value[0].state, "failed");
  assert.equal(manager.tasks.value[0].transferredBytes, 64);

  manager.retryTask("transfer-failed-resume");
  assert.equal(manager.tasks.value[0].transferredBytes, 64);
  assert.equal(manager.tasks.value[0].progress, 50);

  await manager.waitForIdle();
  assert.equal(attempts, 2);
  assert.equal(manager.tasks.value[0].state, "completed");
  assert.equal(manager.tasks.value[0].progress, 100);
});

test("persists unfinished tasks and restores them as paused after restart", async () => {
  const store = memoryTaskStore();
  let timestamp = 0;
  const firstManager = createFileTransferManager({
    id: () => "persistent-transfer",
    now: () => new Date(timestamp).toISOString(),
    store,
  });

  firstManager.enqueueUploads([{
    ...request("/tmp/restart.txt"),
    hostId: "host-1",
    relativePath: "Project/sub/restart.txt",
    targetDirectory: "/Users/pro/Receive",
    renamePolicy: "overwrite",
    upload: async ({ onProgress }) => {
      timestamp = 1_000;
      onProgress({ transferredBytes: 40, totalBytes: 100 });
      throw new Error("agent_unreachable");
    },
  }]);

  await firstManager.waitForIdle();
  assert.equal(firstManager.tasks.value[0].state, "failed");

  const secondManager = createFileTransferManager({ store });
  const restored = secondManager.tasks.value[0];
  assert.equal(restored.id, "persistent-transfer");
  assert.equal(restored.state, "paused");
  assert.equal(restored.transferredBytes, 40);
  assert.equal(restored.totalBytes, 100);
  assert.equal(restored.hostId, "host-1");
  assert.equal(restored.relativePath, "Project/sub/restart.txt");
  assert.equal(restored.targetDirectory, "/Users/pro/Receive");
  assert.equal(restored.renamePolicy, "overwrite");
});

test("restored paused tasks can resume when an upload resolver is available", async () => {
  const store = memoryTaskStore();
  createFileTransferManager({
    store,
    restoredTasks: [{
      id: "restored-transfer",
      sessionId: "session-1",
      target: "mac.local:5900",
      direction: "upload",
      filePath: "/tmp/restored.txt",
      fileName: "restored.txt",
      state: "transferring",
      transferredBytes: 64,
      totalBytes: 128,
      progress: 50,
      speedBytesPerSecond: 0,
      etaSeconds: null,
      createdAt: "2026-07-22T00:00:00.000Z",
      updatedAt: "2026-07-22T00:00:01.000Z",
    }],
  });
  let resumedWithOffset = 0;
  const manager = createFileTransferManager({
    store,
    restoreUpload: (task) => async ({ onProgress }) => {
      resumedWithOffset = task.transferredBytes;
      onProgress({ transferredBytes: 128, totalBytes: 128 });
      return {
        fileName: task.fileName,
        remotePath: "/remote/restored.txt",
        bytesWritten: 128,
        sha256: "f".repeat(64),
      };
    },
  });

  assert.equal(manager.tasks.value[0].state, "paused");
  manager.resumeTask("restored-transfer");
  await manager.waitForIdle();

  assert.equal(resumedWithOffset, 64);
  assert.equal(manager.tasks.value[0].state, "completed");
  assert.equal(manager.tasks.value[0].progress, 100);
});

test("keeps completed failed and canceled transfers as history that can be cleared", async () => {
  const manager = createFileTransferManager({ maxConcurrent: 1, maxConcurrentPerTarget: 1 });
  const [completed, failed, canceled] = manager.enqueueUploads([
    request("/tmp/completed.txt"),
    {
      ...request("/tmp/failed.txt"),
      upload: async () => {
        throw new Error("agent_file_read_failed");
      },
    },
    request("/tmp/canceled.txt"),
  ]);

  manager.cancelTask(canceled.id);
  await manager.waitForIdle();

  assert.deepEqual(
    manager.history.value.map((task) => [task.id, task.state]),
    [
      [completed.id, "completed"],
      [failed.id, "failed"],
      [canceled.id, "canceled"],
    ],
  );

  await manager.clearHistory();
  assert.equal(manager.tasks.value.some((task) => task.id === completed.id), false);
  assert.equal(manager.tasks.value.some((task) => task.id === failed.id), false);
  assert.equal(manager.tasks.value.some((task) => task.id === canceled.id), false);
});

test("clears terminal history only for the selected session when requested", async () => {
  const manager = createFileTransferManager({ maxConcurrent: 1, maxConcurrentPerTarget: 1 });
  const [first, second] = manager.enqueueUploads([
    request("/tmp/first.txt"),
    { ...request("/tmp/second.txt"), sessionId: "session-2" },
  ]);

  await manager.waitForIdle();
  await manager.clearHistory(first.sessionId);

  assert.deepEqual(manager.tasks.value.map((task) => task.id), [second.id]);
});

test("batch controls intersect filtered task ids with session ownership and empty results do nothing", async () => {
  const manager = createFileTransferManager();
  const [first, second, other] = manager.enqueueUploads([
    request("/one.txt", "session-a"), request("/two.txt", "session-a"), request("/other.txt", "session-b"),
  ]);
  manager.pauseAll(undefined, [first.id]);
  assert.deepEqual(manager.tasks.value.map(task => task.state), ["paused", "queued", "queued"]);
  manager.pauseAll();
  manager.resumeAll("session-a", [first.id, other.id]);
  assert.deepEqual(manager.tasks.value.map(task => task.state), ["queued", "paused", "paused"]);
  manager.pauseAll(undefined, []);
  manager.resumeAll(undefined, []);
  const noCleanup = manager.clearHistory(undefined, []);
  assert.deepEqual(manager.tasks.value.map(task => [task.id, task.state]), [
    [first.id, "queued"], [second.id, "paused"], [other.id, "paused"],
  ]);
  await noCleanup;
  manager.cancelAll();
  await manager.waitForIdle();
});

test("clearing history removes selected paused tasks and their remote partials while retaining active and other filtered tasks", async () => {
  const store = memoryTaskStore();
  const controls: Array<[string, string]> = [];
  const manager = createFileTransferManager({ store, maxConcurrent: 1, maxConcurrentPerTarget: 1 });
  const [completed] = manager.enqueueUploads([request("/done.txt", "session-a")]);
  await manager.waitForIdle();
  const pending = (path: string, sessionId: string): TransferUploadRequest => ({
    ...request(path, sessionId),
    control: async (id, action) => { controls.push([id, action]); },
    upload: ({ signal }) => new Promise((_resolve, reject) => {
      signal.addEventListener("abort", () => reject(new Error("interrupted")), { once: true });
    }),
  });
  const [paused, other] = manager.enqueueUploads([
    pending("/paused.txt", "session-a"), pending("/other.txt", "session-b"),
  ]);
  await manager.waitForIdleTick();
  manager.pauseAll();
  await manager.waitForIdle();
  controls.length = 0;
  await manager.clearHistory("session-a", [paused.id, other.id]);
  assert.deepEqual(manager.tasks.value.map(task => task.id), [completed.id, other.id]);
  assert.deepEqual(controls, [[paused.id, "cancel"]]);
  assert.deepEqual(store.load().map(task => task.id), [completed.id, other.id]);

  const [running] = manager.enqueueUploads([pending("/running.txt", "session-a")]);
  await manager.waitForIdleTick();
  await manager.clearHistory();
  assert.deepEqual(manager.tasks.value.map(task => [task.id, task.state]), [[running.id, "transferring"]]);
  assert.deepEqual(controls, [[paused.id, "cancel"], [other.id, "cancel"]]);
  manager.cancelAll();
  await manager.waitForIdle();
});

function pausedSnapshot(id: string, sessionId = "session-1"): FileTransferTaskSnapshot {
  return {
    id, sessionId, target: "host:5900", direction: "upload", filePath: `/tmp/${id}.txt`, fileName: `${id}.txt`,
    state: "paused", transferredBytes: 0, totalBytes: 100, progress: 0, speedBytesPerSecond: 0,
    etaSeconds: null, createdAt: "", updatedAt: "",
  };
}

test("history cleanup yields to the UI, persists once and limits remote cleanup concurrency", async () => {
  const stop = deferred<void>();
  let calls = 0, inFlight = 0, peak = 0, writes = 0;
  const manager = createFileTransferManager({
    restoredTasks: Array.from({ length: 205 }, (_, index) => pausedSnapshot(String(index))),
    store: { load: () => [], save: () => { writes++; } },
    restoreControl: () => async () => {
      calls++;
      peak = Math.max(peak, ++inFlight);
      await stop.promise;
      inFlight--;
    },
  });
  writes = 0;
  const cleanup = manager.clearHistory();
  assert.equal(calls, 0, "remote requests must not run in the click handler");
  assert.equal(writes, 0, "cache writes must not run in the click handler");
  assert.equal(manager.clearingHistory.value, true);
  assert.equal(manager.clearHistory(), cleanup, "repeated clicks share the pending cleanup");
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.ok(manager.tasks.value.length > 0, "large cleanups yield between local batches");
  for (let tick = 0; manager.tasks.value.length && tick < 100; tick++) {
    await new Promise(resolve => setTimeout(resolve, 0));
  }
  assert.equal(manager.tasks.value.length, 0);
  assert.equal(writes, 1, "persist after local removal, without rewriting the cache per batch");
  assert.ok(calls > 0 && calls <= 3);
  assert.equal(manager.clearingHistory.value, true, "remote work continues after records disappear");
  stop.resolve();
  await cleanup;
  assert.equal(calls, 205);
  assert.equal(peak, 3);
  assert.equal(manager.clearingHistory.value, false);
});

test("background cleanup preserves tasks resumed or created after the click and keeps the captured scope", async () => {
  const canceled: string[] = [];
  const store = memoryTaskStore();
  const manager = createFileTransferManager({
    store,
    restoredTasks: [pausedSnapshot("remove"), pausedSnapshot("resume"), pausedSnapshot("outside", "session-2")],
    restoreUpload: task => request(task.filePath, task.sessionId).upload,
    restoreControl: () => async id => { canceled.push(id); },
  });
  const cleanup = manager.clearHistory("session-1", ["remove", "resume", "outside"]);
  manager.resumeTask("resume");
  const [added] = manager.enqueueUploads([request("/new.txt")]);
  manager.pauseTask(added.id);
  await cleanup;
  await manager.waitForIdle();
  assert.deepEqual(manager.tasks.value.map(task => [task.id, task.state]), [
    ["resume", "completed"], ["outside", "paused"], [added.id, "paused"],
  ]);
  assert.deepEqual(canceled, ["remove"]);
  assert.deepEqual(store.load().map(task => task.id), ["resume", "outside", added.id]);
});

test("history cleanup releases its busy state when cache persistence fails and tolerates remote cleanup failures", async () => {
  let failSave = false;
  let persisted: FileTransferTaskSnapshot[] = [];
  const manager = createFileTransferManager({
    restoredTasks: [pausedSnapshot("first")],
    store: { load: () => [], save: tasks => { if (failSave) throw new Error("quota"); persisted = tasks; } },
    restoreControl: () => async () => { throw new Error("disconnected"); },
  });
  failSave = true;
  await assert.rejects(manager.clearHistory(), /quota/);
  assert.equal(manager.clearingHistory.value, false);
  assert.equal(persisted.length, 1);
  failSave = false;
  await manager.clearHistory(undefined, []);
  assert.deepEqual(persisted, [], "retry persists the deletion even when the filtered list is already empty");

  const remoteFailure = createFileTransferManager({
    restoredTasks: [pausedSnapshot("second")],
    restoreControl: () => async () => { throw new Error("disconnected"); },
  });
  await remoteFailure.clearHistory();
  assert.equal(remoteFailure.tasks.value.length, 0);
  assert.equal(remoteFailure.clearingHistory.value, false);
});

test("canceling a paused upload still calls transfer control for remote cleanup", async () => {
  const controls: Array<[string, string]> = [];
  const manager = createFileTransferManager({ id: () => "transfer-cleanup" });

  manager.enqueueUploads([{
    ...request("/tmp/cleanup.txt"),
    control: async (taskId, action) => {
      controls.push([taskId, action]);
    },
    upload: async () => new Promise(() => {}),
  }]);

  await manager.waitForIdleTick();
  manager.pauseTask("transfer-cleanup");
  manager.cancelTask("transfer-cleanup");

  assert.deepEqual(controls, [
    ["transfer-cleanup", "pause"],
    ["transfer-cleanup", "cancel"],
  ]);
  assert.equal(manager.tasks.value[0].state, "canceled");
});

test("removes all transfer records for a closed session and cancels active work", async () => {
  const controls: Array<[string, string]> = [];
  const manager = createFileTransferManager({ maxConcurrent: 1, maxConcurrentPerTarget: 1 });
  const [running, queued] = manager.enqueueUploads([
    {
      ...request("/tmp/running.txt", "session-closing"),
      control: async (taskId, action) => {
        controls.push([taskId, action]);
      },
      upload: async () => new Promise(() => {}),
    },
    {
      ...request("/tmp/queued.txt", "session-closing"),
      control: async (taskId, action) => {
        controls.push([taskId, action]);
      },
    },
    request("/tmp/other.txt", "session-other"),
  ]);

  await manager.waitForIdleTick();
  manager.removeSessionTasks("session-closing");

  assert.deepEqual(controls, [[running.id, "cancel"], [queued.id, "cancel"]]);
  assert.deepEqual(manager.tasks.value.map((task) => task.sessionId), ["session-other"]);
});

test("removes paused orphan transfers when their session no longer exists", () => {
  const manager = createFileTransferManager({
    restoredTasks: [
      {
        id: "orphan-paused",
        sessionId: "missing-session",
        target: "mac.local:5900",
        direction: "upload",
        filePath: "/tmp/orphan.txt",
        fileName: "orphan.txt",
        state: "paused",
        transferredBytes: 34,
        totalBytes: 195,
        progress: 17,
        speedBytesPerSecond: 0,
        etaSeconds: null,
        createdAt: "2026-07-26T00:00:00.000Z",
        updatedAt: "2026-07-26T00:00:01.000Z",
      },
      {
        id: "live-paused",
        sessionId: "live-session",
        target: "mac.local:5900",
        direction: "upload",
        filePath: "/tmp/live.txt",
        fileName: "live.txt",
        state: "paused",
        transferredBytes: 10,
        totalBytes: 100,
        progress: 10,
        speedBytesPerSecond: 0,
        etaSeconds: null,
        createdAt: "2026-07-26T00:00:00.000Z",
        updatedAt: "2026-07-26T00:00:01.000Z",
      },
    ],
  });

  manager.removeMissingSessionTasks(new Set(["live-session"]));

  assert.deepEqual(manager.tasks.value.map((task) => task.id), ["live-paused"]);
});

test("keeps directory-relative names when queueing expanded directory uploads", () => {
  const manager = createFileTransferManager();

  manager.enqueueUploads([
    {
      ...request("/tmp/project/src/main.ts"),
      relativePath: "project/src/main.ts",
    },
    {
      ...request("/tmp/project/README.md"),
      relativePath: "project/README.md",
    },
  ]);

  assert.deepEqual(
    manager.tasks.value.map((task) => [task.filePath, task.fileName, task.relativePath]),
    [
      ["/tmp/project/src/main.ts", "project/src/main.ts", "project/src/main.ts"],
      ["/tmp/project/README.md", "project/README.md", "project/README.md"],
    ],
  );
});

test("queues multiple uploads and respects global and per-target concurrency", async () => {
  const started: string[] = [];
  const pending = new Map<string, ReturnType<typeof deferred<Awaited<ReturnType<TransferUploadRequest["upload"]>>>>>();
  const manager = createFileTransferManager({ maxConcurrent: 3, maxConcurrentPerTarget: 2 });

  const files = ["/tmp/a.txt", "/tmp/b.txt", "/tmp/c.txt", "/tmp/d.txt"];
  manager.enqueueUploads(files.map((filePath) => ({
    ...request(filePath),
    upload: async () => {
      started.push(filePath);
      const task = deferred<Awaited<ReturnType<TransferUploadRequest["upload"]>>>();
      pending.set(filePath, task);
      return await task.promise;
    },
  })));

  await manager.waitForIdleTick();

  assert.deepEqual(started, ["/tmp/a.txt", "/tmp/b.txt"]);
  assert.equal(manager.tasks.value.filter((task) => task.state === "transferring").length, 2);
  assert.equal(manager.tasks.value.filter((task) => task.state === "queued").length, 2);

  pending.get("/tmp/a.txt")?.resolve({
    fileName: "a.txt",
    remotePath: "/remote/a.txt",
    bytesWritten: 1,
    sha256: "a".repeat(64),
  });
  await manager.waitForIdleTick();

  assert.deepEqual(started, ["/tmp/a.txt", "/tmp/b.txt", "/tmp/c.txt"]);
  assert.equal(manager.tasks.value.find((task) => task.filePath === "/tmp/a.txt")?.state, "completed");
  assert.equal(manager.tasks.value.find((task) => task.filePath === "/tmp/c.txt")?.state, "transferring");
});

test("keeps later uploads moving when one task fails", async () => {
  const manager = createFileTransferManager({ maxConcurrent: 1, maxConcurrentPerTarget: 1 });

  manager.enqueueUploads([
    {
      ...request("/tmp/fails.txt"),
      upload: async () => {
        throw new Error("agent_unreachable");
      },
    },
    request("/tmp/next.txt"),
  ]);

  await manager.waitForIdle();

  assert.equal(manager.tasks.value.find((task) => task.filePath === "/tmp/fails.txt")?.state, "failed");
  assert.equal(manager.tasks.value.find((task) => task.filePath === "/tmp/fails.txt")?.errorMessage, "agent_unreachable");
  assert.equal(manager.tasks.value.find((task) => task.filePath === "/tmp/next.txt")?.state, "completed");
});

test("cancels queued uploads and retries failed uploads from the queue", async () => {
  const manager = createFileTransferManager({ maxConcurrent: 1, maxConcurrentPerTarget: 1 });
  let attempts = 0;

  const [running, queued] = manager.enqueueUploads([
    {
      ...request("/tmp/running.txt"),
      upload: async () => new Promise(() => {}),
    },
    request("/tmp/queued.txt"),
  ]);

  manager.cancelTask(queued.id);
  assert.equal(manager.tasks.value.find((task) => task.id === queued.id)?.state, "canceled");

  const [failed] = manager.enqueueUploads([{
    ...request("/tmp/retry.txt"),
    upload: async () => {
      attempts += 1;
      if (attempts === 1) {
        throw new Error("first_failed");
      }
      return {
        fileName: "retry.txt",
        remotePath: "/remote/retry.txt",
        bytesWritten: 1,
        sha256: "b".repeat(64),
      };
    },
  }]);

  manager.cancelTask(running.id);
  await manager.waitForIdle();
  assert.equal(manager.tasks.value.find((task) => task.id === failed.id)?.state, "failed");

  manager.retryTask(failed.id);
  await manager.waitForIdle();

  assert.equal(attempts, 2);
  assert.equal(manager.tasks.value.find((task) => task.id === failed.id)?.state, "completed");
});
