import { computed, ref, shallowRef, type ComputedRef, type Ref } from "vue";
import type { AgentFileUploadResult } from "../protocol/protocolAdapter.ts";
import { isFileTransferConflictSkipped } from "./fileTransferErrors.ts";

const SPEED_SAMPLE_INTERVAL_MS = 1_000;
const SPEED_SMOOTHING_WEIGHT = 0.35;

export type TransferDirection = "upload";
export type TransferRenamePolicy = "autoRename" | "overwrite" | "skip" | "ask";

export type TransferState =
  | "queued"
  | "transferring"
  | "paused"
  | "completed"
  | "skipped"
  | "failed"
  | "canceled";

export type TransferProgressUpdate = {
  transferredBytes: number;
  totalBytes: number;
  phase?: "preparing" | "transferring" | "verifying";
};

export type TransferUploadContext = {
  taskId: string;
  signal: AbortSignal;
  onProgress: (progress: TransferProgressUpdate) => void;
};

export type TransferControlAction = "pause" | "cancel";

export type TransferTask = {
  id: string;
  sessionId: string;
  hostId?: string;
  target: string;
  direction: TransferDirection;
  filePath: string;
  fileName: string;
  relativePath?: string;
  targetDirectory?: string;
  renamePolicy?: TransferRenamePolicy;
  state: TransferState;
  phase?: TransferProgressUpdate["phase"];
  transferredBytes: number;
  totalBytes: number;
  progress: number;
  speedBytesPerSecond: number;
  etaSeconds: number | null;
  remotePath?: string;
  sha256?: string;
  errorMessage?: string;
  createdAt: string;
  updatedAt: string;
};

export type TransferUploadRequest = {
  sessionId: string;
  hostId?: string;
  target: string;
  filePath: string;
  relativePath?: string;
  targetDirectory?: string;
  renamePolicy?: TransferRenamePolicy;
  upload: (context: TransferUploadContext) => Promise<AgentFileUploadResult>;
  control?: (taskId: string, action: TransferControlAction) => Promise<void>;
};

export type FileTransferTaskSnapshot = TransferTask;

export type FileTransferTaskStore = {
  load: () => FileTransferTaskSnapshot[];
  save: (tasks: FileTransferTaskSnapshot[]) => void;
  /** Native stores can commit new tasks before they become visible or runnable. */
  insert?: (tasks: FileTransferTaskSnapshot[]) => Promise<void>;
  /** Native stores persist only changed rows, without serializing the entire queue. */
  applyChanges?: (upserts: FileTransferTaskSnapshot[], removes: string[]) => void;
};

export type TransferManagerOptions = {
  maxConcurrent?: number;
  maxConcurrentPerTarget?: number;
  id?: () => string;
  now?: () => string;
  store?: FileTransferTaskStore;
  restoredTasks?: FileTransferTaskSnapshot[];
  restoreUpload?: (task: TransferTask) => TransferUploadRequest["upload"];
  restoreControl?: (task: TransferTask) => TransferUploadRequest["control"];
};

export type TransferSummary = {
  total: number;
  queued: number;
  transferring: number;
  paused: number;
  completed: number;
  skipped: number;
  failed: number;
  canceled: number;
  remaining: number;
  totalBytes: number;
  transferredBytes: number;
  totalProgress: number;
  totalSpeedBytesPerSecond: number;
};

type InternalTransferTask = TransferTask & {
  upload: (context: TransferUploadContext) => Promise<AgentFileUploadResult>;
  control?: (taskId: string, action: TransferControlAction) => Promise<void>;
  runToken: number;
  abortController?: AbortController;
  lastProgressAtMs?: number;
  lastProgressBytes?: number;
};

export type FileTransferManager = {
  tasks: Ref<TransferTask[]>;
  clearingHistory: Ref<boolean>;
  history: ComputedRef<TransferTask[]>;
  summary: ComputedRef<TransferSummary>;
  enqueueUploads: (requests: TransferUploadRequest[]) => TransferTask[];
  enqueueUploadsDurably: (requests: TransferUploadRequest[]) => Promise<TransferTask[]>;
  cancelTask: (taskId: string) => void;
  cancelAll: (sessionId?: string) => void;
  pauseTask: (taskId: string) => void;
  pauseAll: (sessionId?: string, taskIds?: readonly string[]) => void;
  resumeTask: (taskId: string) => void;
  resumeAll: (sessionId?: string, taskIds?: readonly string[]) => void;
  retryTask: (taskId: string) => void;
  clearHistory: (sessionId?: string, taskIds?: readonly string[]) => Promise<void>;
  removeSessionTasks: (sessionId: string) => void;
  removeMissingSessionTasks: (sessionIds: ReadonlySet<string>) => void;
  tasksForSession: (sessionId: string) => TransferTask[];
  waitForIdleTick: () => Promise<void>;
  waitForIdle: () => Promise<void>;
};

export function createFileTransferManager(options: TransferManagerOptions = {}): FileTransferManager {
  const maxConcurrent = Math.max(1, options.maxConcurrent ?? 3);
  const maxConcurrentPerTarget = Math.max(1, options.maxConcurrentPerTarget ?? 2);
  const id = options.id ?? (() => crypto.randomUUID());
  const now = options.now ?? (() => new Date().toISOString());
  const store = options.store;
  // Task changes replace objects/arrays; avoid deep proxies when scanning large histories.
  const internalTasks = shallowRef<InternalTransferTask[]>(restoreTasks());
  const clearingHistory = ref(false);
  let historyCleanup: Promise<void> | undefined;
  let deferPersistence = false;
  let persistencePending = false;
  let running = 0;
  let schedulePending = false;
  let idleResolvers: Array<() => void> = [];
  const activeRuns = new Set<string>();

  const publicTasks = computed<TransferTask[]>(() => internalTasks.value.map(toPublicTask));
  const history = computed<TransferTask[]>(() => publicTasks.value.filter((task) => isTerminalState(task.state)));
  const summary = computed<TransferSummary>(() => summarizeTransfers(publicTasks.value));
  persistTasks();

  function enqueueUploads(requests: TransferUploadRequest[]): TransferTask[] {
    const created = requests.map((request) => createTask(request, id(), now()));
    replaceTasks([...internalTasks.value, ...created]);
    schedule();
    return created.map(toPublicTask);
  }

  async function enqueueUploadsDurably(requests: TransferUploadRequest[]): Promise<TransferTask[]> {
    if (!store?.insert) return enqueueUploads(requests);
    const created = requests.map((request) => createTask(request, id(), now()));
    const publicCreated = created.map(toPublicTask);
    await store.insert(publicCreated);
    internalTasks.value = [...internalTasks.value, ...created];
    schedule();
    return publicCreated;
  }

  function cancelTask(taskId: string) {
    updateTask(taskId, (task) => {
      if (isTerminalState(task.state)) {
        return task;
      }
      task.abortController?.abort("canceled");
      void task.control?.(task.id, "cancel");

      return {
        ...task,
        state: "canceled",
        phase: undefined,
        runToken: task.runToken + 1,
        updatedAt: now(),
      };
    });
    releaseActiveRun(taskId);
    schedule();
  }

  function cancelAll(sessionId?: string) {
    replaceTasks(internalTasks.value.map((task) => {
      if (sessionId && task.sessionId !== sessionId) {
        return task;
      }
      if (isTerminalState(task.state)) {
        return task;
      }
      task.abortController?.abort("canceled");
      void task.control?.(task.id, "cancel");
      return {
        ...task,
        state: "canceled",
        phase: undefined,
        runToken: task.runToken + 1,
        updatedAt: now(),
      };
    }));
    internalTasks.value
      .filter((task) => (!sessionId || task.sessionId === sessionId) && task.state === "canceled")
      .forEach((task) => releaseActiveRun(task.id));
    schedule();
  }

  function pauseTask(taskId: string) {
    updateTask(taskId, (task) => pauseTransferTask(task));
    schedule();
  }

  function pauseAll(sessionId?: string, taskIds?: readonly string[]) {
    const selected = taskIds === undefined ? undefined : new Set(taskIds);
    replaceTasks(internalTasks.value.map((task) => {
      if ((sessionId && task.sessionId !== sessionId) || (selected && !selected.has(task.id))) {
        return task;
      }
      return pauseTransferTask(task);
    }));
    schedule();
  }

  function resumeTask(taskId: string) {
    updateTask(taskId, (task) => resumeTransferTask(task));
    schedule();
  }

  function resumeAll(sessionId?: string, taskIds?: readonly string[]) {
    const selected = taskIds === undefined ? undefined : new Set(taskIds);
    replaceTasks(internalTasks.value.map((task) => {
      if ((sessionId && task.sessionId !== sessionId) || (selected && !selected.has(task.id))) {
        return task;
      }
      return resumeTransferTask(task);
    }));
    schedule();
  }

  function retryTask(taskId: string) {
    updateTask(taskId, (task) => {
      if (task.state !== "failed" && task.state !== "canceled") {
        return task;
      }

      return {
        ...task,
        state: "queued",
        phase: undefined,
        transferredBytes: task.state === "canceled" ? 0 : task.transferredBytes,
        totalBytes: task.state === "canceled" ? 0 : task.totalBytes,
        progress: task.state === "canceled" ? 0 : task.progress,
        remotePath: undefined,
        sha256: undefined,
        errorMessage: undefined,
        speedBytesPerSecond: task.state === "canceled" ? 0 : task.speedBytesPerSecond,
        etaSeconds: task.state === "canceled" ? null : task.etaSeconds,
        runToken: task.runToken + 1,
        updatedAt: now(),
      };
    });
    schedule();
  }

  function tasksForSession(sessionId: string): TransferTask[] {
    return publicTasks.value.filter((task) => task.sessionId === sessionId);
  }

  function clearHistory(sessionId?: string, taskIds?: readonly string[]): Promise<void> {
    if (historyCleanup) return historyCleanup;
    if (taskIds?.length === 0 && !persistencePending) return Promise.resolve();
    const snapshot = internalTasks.value;
    if (!snapshot.length && !persistencePending) return Promise.resolve();
    clearingHistory.value = true;
    deferPersistence = true;
    historyCleanup = cleanHistory(snapshot, sessionId, taskIds).finally(() => {
      clearingHistory.value = false;
      historyCleanup = undefined;
    });
    return historyCleanup;
  }

  async function cleanHistory(snapshot: InternalTransferTask[], sessionId?: string, taskIds?: readonly string[]) {
    // A timer yields to rendering/input; a resolved Promise would keep work in the same frame.
    await yieldToBackground();
    const selected = taskIds === undefined ? undefined : new Set(taskIds);
    const remoteCleanup: InternalTransferTask[] = [];
    try {
      // Work backwards so removing old rows does not shift every remaining rendered row.
      for (let end = snapshot.length; end > 0; end -= 25) {
        const start = Math.max(0, end - 25);
        const candidates = new Map(snapshot.slice(start, end).filter(task => (
          (isTerminalState(task.state) || task.state === "paused")
          && (sessionId === undefined || task.sessionId === sessionId)
          && (selected === undefined || selected.has(task.id))
        )).map(task => [task.id, task]));
        if (candidates.size) {
          const remaining = internalTasks.value.filter(task => {
            // Preserve tasks resumed/retried since the click, as well as newly queued work.
            if (candidates.get(task.id) !== task) return true;
            if (task.state === "paused") {
              task.abortController?.abort("canceled");
              if (task.control) remoteCleanup.push(task);
            }
            releaseActiveRun(task.id);
            return false;
          });
          internalTasks.value = remaining;
          schedule();
        }
        if (start > 0) await yieldToBackground();
      }
    } finally {
      deferPersistence = false;
      // Persist the latest tasks once, including changes made while cleanup yielded.
      try {
        if (store?.applyChanges) {
          persistNativeChanges(snapshot, internalTasks.value);
          persistencePending = false;
        }
        else persistTasks();
      }
      finally {
        schedule();
        await cleanRemotePartials(remoteCleanup);
      }
    }
  }

  async function cleanRemotePartials(tasks: InternalTransferTask[]) {
    let index = 0;
    async function worker() {
      while (index < tasks.length) {
        const task = tasks[index++];
        try { await task.control?.(task.id, "cancel"); }
        catch { /* A disconnected receiver must not prevent the remaining history cleanup. */ }
        await yieldToBackground();
      }
    }
    await Promise.all(Array.from({ length: Math.min(3, tasks.length) }, worker));
  }

  function removeSessionTasks(sessionId: string) {
    const removedTasks = internalTasks.value.filter((task) => task.sessionId === sessionId);
    for (const task of removedTasks) {
      if (!isTerminalState(task.state)) {
        task.abortController?.abort("canceled");
        void task.control?.(task.id, "cancel");
      }
      releaseActiveRun(task.id);
    }
    replaceTasks(internalTasks.value.filter((task) => task.sessionId !== sessionId));
    schedule();
  }

  function removeMissingSessionTasks(sessionIds: ReadonlySet<string>) {
    const orphanTasks = internalTasks.value.filter((task) => !sessionIds.has(task.sessionId));
    for (const task of orphanTasks) {
      if (!isTerminalState(task.state)) {
        task.abortController?.abort("canceled");
        void task.control?.(task.id, "cancel");
      }
      releaseActiveRun(task.id);
    }
    replaceTasks(internalTasks.value.filter((task) => sessionIds.has(task.sessionId)));
    schedule();
  }

  async function waitForIdleTick() {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  }

  async function waitForIdle() {
    await waitForIdleTick();
    if (isIdle()) {
      return;
    }
    await new Promise<void>((resolve) => idleResolvers.push(resolve));
    await waitForIdleTick();
  }

  function createTask(request: TransferUploadRequest, taskId: string, createdAt: string): InternalTransferTask {
    return {
      id: taskId,
      sessionId: request.sessionId,
      hostId: request.hostId,
      target: request.target,
      direction: "upload",
      filePath: request.filePath,
      fileName: request.relativePath ?? basename(request.filePath),
      relativePath: request.relativePath,
      targetDirectory: request.targetDirectory,
      renamePolicy: request.renamePolicy,
      state: "queued",
      phase: undefined,
      transferredBytes: 0,
      totalBytes: 0,
      progress: 0,
      speedBytesPerSecond: 0,
      etaSeconds: null,
      upload: request.upload,
      control: request.control,
      runToken: 0,
      createdAt,
      updatedAt: createdAt,
    };
  }

  function schedule() {
    if (schedulePending) {
      return;
    }

    schedulePending = true;
    void Promise.resolve().then(() => {
      schedulePending = false;
      startNextTasks();
      notifyIdleIfNeeded();
    });
  }

  function startNextTasks() {
    while (running < maxConcurrent) {
      const next = nextQueuedTask();
      if (!next) {
        return;
      }
      startTask(next);
    }
  }

  function nextQueuedTask(): InternalTransferTask | null {
    const activeByTarget = activeTransferCountByTarget();
    return internalTasks.value.find((task) => (
      task.state === "queued"
      && !hasActiveRun(task.id)
      && (activeByTarget.get(task.target) ?? 0) < maxConcurrentPerTarget
    )) ?? null;
  }

  function startTask(task: InternalTransferTask) {
    const runToken = task.runToken;
    const runKey = activeRunKey(task.id, runToken);
    const abortController = new AbortController();
    activeRuns.add(runKey);
    running += 1;
    updateTask(task.id, (current) => ({
      ...current,
      state: "transferring",
      phase: "preparing",
      errorMessage: undefined,
      abortController,
      speedBytesPerSecond: 0,
      etaSeconds: null,
      lastProgressAtMs: epochMs(now()),
      lastProgressBytes: current.transferredBytes,
      updatedAt: now(),
    }));

    void task.upload({
      taskId: task.id,
      signal: abortController.signal,
      onProgress: (progress) => updateProgress(task.id, runToken, progress),
    })
      .then((result) => completeTask(task.id, runToken, result))
      .catch((error) => failTask(task.id, runToken, error))
      .finally(() => {
        releaseActiveRun(task.id, runToken);
        schedule();
      });
  }

  function completeTask(taskId: string, runToken: number, result: AgentFileUploadResult) {
    updateTask(taskId, (task) => {
      if (task.runToken !== runToken || task.state === "canceled") {
        return task;
      }
      return {
        ...task,
        fileName: task.relativePath ?? result.fileName,
        state: "completed",
        phase: undefined,
        transferredBytes: result.bytesWritten,
        totalBytes: result.bytesWritten,
        progress: 100,
        speedBytesPerSecond: task.speedBytesPerSecond,
        etaSeconds: 0,
        remotePath: result.remotePath,
        sha256: result.sha256,
        errorMessage: undefined,
        updatedAt: now(),
      };
    });
  }

  function failTask(taskId: string, runToken: number, error: unknown) {
    updateTask(taskId, (task) => {
      if (task.runToken !== runToken || task.state === "canceled") {
        return task;
      }
      if (task.state === "paused") {
        return {
          ...task,
          errorMessage: undefined,
          abortController: undefined,
          updatedAt: now(),
        };
      }
      return {
        ...task,
        state: isFileTransferConflictSkipped(error) ? "skipped" : "failed",
        phase: undefined,
        speedBytesPerSecond: 0,
        etaSeconds: null,
        errorMessage: error instanceof Error ? error.message : String(error),
        updatedAt: now(),
      };
    });
  }

  function updateProgress(taskId: string, runToken: number, progress: TransferProgressUpdate) {
    updateTask(taskId, (task) => {
      if (task.runToken !== runToken || task.state !== "transferring") {
        return task;
      }
      const currentTime = epochMs(now());
      const previousTime = task.lastProgressAtMs ?? currentTime;
      const previousBytes = task.lastProgressBytes ?? task.transferredBytes;
      const elapsedMs = currentTime - previousTime;
      const phase = progress.phase ?? (progress.transferredBytes >= progress.totalBytes ? "verifying" : "transferring");
      const transferredBytes = phase === "preparing"
        ? Math.max(task.transferredBytes, progress.transferredBytes, 0)
        : Math.max(0, progress.transferredBytes);
      const totalBytes = Math.max(progress.totalBytes, transferredBytes);
      const phaseChanged = task.phase !== phase;
      const resetSample = phaseChanged || elapsedMs < 0 || transferredBytes < previousBytes;
      const sampleSpeed = phase === "transferring" && elapsedMs >= SPEED_SAMPLE_INTERVAL_MS && !resetSample;
      const bytesDelta = Math.max(0, transferredBytes - previousBytes);
      const measuredSpeed = sampleSpeed ? Math.round(bytesDelta / (elapsedMs / 1000)) : 0;
      let speedBytesPerSecond = task.speedBytesPerSecond;
      if (resetSample || phase !== "transferring") speedBytesPerSecond = 0;
      else if (sampleSpeed) speedBytesPerSecond = speedBytesPerSecond > 0
        ? Math.round(speedBytesPerSecond * (1 - SPEED_SMOOTHING_WEIGHT) + measuredSpeed * SPEED_SMOOTHING_WEIGHT)
        : measuredSpeed;
      const remainingBytes = Math.max(0, totalBytes - transferredBytes);
      let etaSeconds = task.etaSeconds;
      if (phase !== "transferring" || resetSample) etaSeconds = null;
      else if (remainingBytes === 0) etaSeconds = 0;
      else if (sampleSpeed) etaSeconds = speedBytesPerSecond > 0
        ? Math.ceil(remainingBytes / speedBytesPerSecond) : null;
      return {
        ...task,
        transferredBytes,
        totalBytes,
        phase,
        progress: percent(transferredBytes, totalBytes),
        speedBytesPerSecond,
        etaSeconds,
        lastProgressAtMs: sampleSpeed || resetSample ? currentTime : task.lastProgressAtMs,
        lastProgressBytes: sampleSpeed || resetSample ? transferredBytes : task.lastProgressBytes,
        updatedAt: now(),
      };
    });
  }

  function pauseTransferTask(task: InternalTransferTask): InternalTransferTask {
    if (task.state !== "queued" && task.state !== "transferring") {
      return task;
    }
    task.abortController?.abort("paused");
    if (task.state === "transferring" || hasActiveRun(task.id)) {
      void task.control?.(task.id, "pause");
    }
    return {
      ...task,
      state: "paused",
      phase: undefined,
      runToken: task.runToken + 1,
      abortController: undefined,
      updatedAt: now(),
    };
  }

  function resumeTransferTask(task: InternalTransferTask): InternalTransferTask {
    if (task.state !== "paused") {
      return task;
    }
    return {
      ...task,
      state: "queued",
      phase: undefined,
      errorMessage: undefined,
      runToken: task.runToken + 1,
      updatedAt: now(),
    };
  }

  function updateTask(taskId: string, update: (task: InternalTransferTask) => InternalTransferTask) {
    let changed = false;
    const tasks = internalTasks.value.map(task => {
      const next = task.id === taskId ? update(task) : task;
      if (next !== task) changed = true;
      return next;
    });
    // Late callbacks from removed tasks must not rewrite the whole cache.
    if (changed) replaceTasks(tasks);
  }

  function replaceTasks(tasks: InternalTransferTask[]) {
    const previous = internalTasks.value;
    if (deferPersistence) {
      internalTasks.value = tasks;
      persistencePending = true;
      return;
    }
    if (store?.applyChanges) {
      internalTasks.value = tasks;
      persistNativeChanges(previous, tasks);
      return;
    }
    try {
      store?.save(tasks.map(toPublicTask));
    } catch (error) {
      persistencePending = true;
      throw error;
    }
    internalTasks.value = tasks;
    persistencePending = false;
  }

  function persistNativeChanges(before: InternalTransferTask[], after: InternalTransferTask[]) {
    if (!store?.applyChanges) return;
    const beforeById = new Map(before.map(task => [task.id, task]));
    const upserts = after.filter(task => beforeById.get(task.id) !== task).map(toPublicTask);
    const afterIds = new Set(after.map(task => task.id));
    const removes = before.filter(task => !afterIds.has(task.id)).map(task => task.id);
    if (upserts.length || removes.length) store.applyChanges(upserts, removes);
  }

  function persistTasks() {
    persistencePending = true;
    if (deferPersistence) return;
    if (!store?.applyChanges) store?.save(internalTasks.value.map(toPublicTask));
    persistencePending = false;
  }

  function restoreTasks(): InternalTransferTask[] {
    const snapshots = options.restoredTasks ?? store?.load() ?? [];
    return snapshots.map((task) => restoreTask(task, now()));
  }

  function restoreTask(task: FileTransferTaskSnapshot, restoredAt: string): InternalTransferTask {
    const publicTask: TransferTask = {
      ...task,
      state: restoredState(task),
      phase: undefined,
      speedBytesPerSecond: 0,
      etaSeconds: null,
      updatedAt: task.updatedAt || restoredAt,
    };
    return {
      ...publicTask,
      upload: options.restoreUpload?.(publicTask) ?? restoredUploadUnavailable,
      control: options.restoreControl?.(publicTask),
      runToken: 0,
      lastProgressBytes: publicTask.transferredBytes,
      lastProgressAtMs: epochMs(publicTask.updatedAt),
    };
  }

  async function restoredUploadUnavailable(): Promise<AgentFileUploadResult> {
    throw new Error("file_transfer_restore_unavailable");
  }

  function releaseActiveRun(taskId: string, runToken?: number) {
    const keys = runToken === undefined
      ? Array.from(activeRuns).filter((key) => key.startsWith(`${taskId}:`))
      : [activeRunKey(taskId, runToken)];
    for (const key of keys) {
      if (activeRuns.delete(key)) {
        running = Math.max(0, running - 1);
      }
    }
  }

  function hasActiveRun(taskId: string): boolean {
    return Array.from(activeRuns).some((key) => key.startsWith(`${taskId}:`));
  }

  function activeTransferCountByTarget(): Map<string, number> {
    const counts = new Map<string, number>();
    for (const task of internalTasks.value) {
      if (task.state !== "transferring") {
        continue;
      }
      counts.set(task.target, (counts.get(task.target) ?? 0) + 1);
    }
    return counts;
  }

  function isIdle(): boolean {
    return running === 0 && !internalTasks.value.some((task) => task.state === "queued" || task.state === "transferring");
  }

  function notifyIdleIfNeeded() {
    if (!isIdle() || idleResolvers.length === 0) {
      return;
    }
    const resolvers = idleResolvers;
    idleResolvers = [];
    resolvers.forEach((resolve) => resolve());
  }

  return {
    tasks: publicTasks,
    clearingHistory,
    history,
    summary,
    enqueueUploads,
    enqueueUploadsDurably,
    cancelTask,
    cancelAll,
    pauseTask,
    pauseAll,
    resumeTask,
    resumeAll,
    retryTask,
    clearHistory,
    removeSessionTasks,
    removeMissingSessionTasks,
    tasksForSession,
    waitForIdleTick,
    waitForIdle,
  };
}

function yieldToBackground(): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, 0));
}

function activeRunKey(taskId: string, runToken: number): string {
  return `${taskId}:${runToken}`;
}

export function summarizeTransfers(tasks: TransferTask[]): TransferSummary {
  const summary: TransferSummary = {
    total: tasks.length,
    queued: 0,
    transferring: 0,
    paused: 0,
    completed: 0,
    skipped: 0,
    failed: 0,
    canceled: 0,
    remaining: 0,
    totalBytes: 0,
    transferredBytes: 0,
    totalProgress: 0,
    totalSpeedBytesPerSecond: 0,
  };

  for (const task of tasks) {
    summary[task.state] += 1;
    if (task.state === "queued" || task.state === "transferring") {
      summary.remaining += 1;
    }
    summary.totalBytes += task.totalBytes;
    summary.transferredBytes += task.transferredBytes;
    summary.totalSpeedBytesPerSecond += task.speedBytesPerSecond;
  }
  summary.totalProgress = percent(summary.transferredBytes, summary.totalBytes);

  return summary;
}

function toPublicTask(task: InternalTransferTask): TransferTask {
  const {
    upload: _upload,
    control: _control,
    runToken: _runToken,
    abortController: _abortController,
    lastProgressAtMs: _lastProgressAtMs,
    lastProgressBytes: _lastProgressBytes,
    ...publicTask
  } = task;
  return publicTask;
}

function basename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function percent(transferredBytes: number, totalBytes: number): number {
  if (totalBytes <= 0) {
    return 0;
  }
  return Math.min(100, Math.max(0, Math.round((transferredBytes / totalBytes) * 100)));
}

function isTerminalState(state: TransferState): boolean {
  return state === "completed" || state === "failed" || state === "canceled" || state === "skipped";
}

function restoredState(task: FileTransferTaskSnapshot): TransferState {
  if (task.state === "skipped" || (task.state === "failed" && isFileTransferConflictSkipped(task.errorMessage))) {
    return "skipped";
  }
  if (task.state === "completed" || task.state === "canceled") {
    return task.state;
  }
  if (task.state === "failed" && task.totalBytes > 0 && task.transferredBytes >= task.totalBytes) {
    return "failed";
  }
  return "paused";
}

function epochMs(timestamp: string): number {
  const parsed = Date.parse(timestamp);
  return Number.isFinite(parsed) ? parsed : Date.now();
}
