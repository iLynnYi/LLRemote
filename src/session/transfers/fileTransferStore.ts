import type {
  FileTransferTaskSnapshot,
  FileTransferTaskStore,
  TransferRenamePolicy,
} from "./fileTransferManager.ts";

const TASK_STORE_KEY = "llremote.file-transfer-tasks.v1";

type FileTransferStorage = {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
};

type NativeTaskCommand = (command: "load_file_transfer_tasks" | "apply_file_transfer_task_changes", args?: Record<string, unknown>) => Promise<unknown>;
let preparedNativeStore: FileTransferTaskStore | undefined;
let preparationError: unknown;
let reportNativeError: ((error: unknown) => void) | undefined;

export function setFileTransferStoreErrorHandler(handler: (error: unknown) => void) {
  reportNativeError = handler;
  if (preparationError) handler(preparationError);
}

export function preparedFileTransferTaskStore(storage: FileTransferStorage | null | undefined): FileTransferTaskStore | undefined {
  if (preparedNativeStore) return preparedNativeStore;
  if (preparationError) return { load: () => [], save: () => { /* Preserve damaged storage for recovery. */ } };
  return createBrowserFileTransferTaskStore(storage);
}

/** Called before Vue mounts so the manager starts with a complete, durable snapshot. */
export async function prepareNativeFileTransferTaskStore(invoke: NativeTaskCommand): Promise<void> {
  preparedNativeStore = undefined;
  preparationError = undefined;
  try {
    const loaded = await invoke("load_file_transfer_tasks");
    if (!Array.isArray(loaded) || !loaded.every(isTransferTaskSnapshot)) throw new Error("file_transfer_tasks_invalid");
    preparedNativeStore = createNativeFileTransferTaskStore(invoke, loaded);
  } catch (error) {
    preparationError = error;
    reportNativeError?.(error);
  }
}

function createNativeFileTransferTaskStore(invoke: NativeTaskCommand, initial: FileTransferTaskSnapshot[]): FileTransferTaskStore {
  const pending = new Map<string, FileTransferTaskSnapshot | null>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let inFlight: Promise<void> | undefined;

  async function flush(): Promise<void> {
    if (inFlight) {
      await inFlight;
      if (pending.size) await flush();
      return;
    }
    if (!pending.size) return;
    const changes = new Map(pending);
    pending.clear();
    const upserts = [...changes.values()].filter((task): task is FileTransferTaskSnapshot => task !== null);
    const removes = [...changes].filter(([, task]) => task === null).map(([id]) => id);
    inFlight = (async () => {
      try {
        await invoke("apply_file_transfer_task_changes", { upserts, removes });
      } catch (error) {
        for (const [id, task] of changes) if (!pending.has(id)) pending.set(id, task);
        throw error;
      }
    })();
    try { await inFlight; }
    finally { inFlight = undefined; }
  }

  return {
    load: () => initial,
    save: () => { /* Native writes use row deltas. */ },
    insert: async tasks => {
      if (timer) { clearTimeout(timer); timer = undefined; }
      await flush();
      await invoke("apply_file_transfer_task_changes", { upserts: tasks, removes: [] });
    },
    applyChanges: (upserts, removes) => {
      for (const task of upserts) pending.set(task.id, task);
      for (const id of removes) pending.set(id, null);
      if (timer) return;
      timer = setTimeout(() => {
        timer = undefined;
        void flush().catch(error => reportNativeError?.(error));
      }, 150);
    },
  };
}

/** Missing caches start empty; malformed data and storage failures abort restoration. */
export function createBrowserFileTransferTaskStore(storage: FileTransferStorage | null | undefined): FileTransferTaskStore | undefined {
  if (!storage) return undefined;
  return {
    load: () => {
      const value = readJson(storage, TASK_STORE_KEY, "file_transfer_tasks_invalid");
      if (value === undefined) return [];
      if (!Array.isArray(value) || !value.every(isTransferTaskSnapshot)) {
        throw new Error("file_transfer_tasks_invalid");
      }
      return value;
    },
    save: (tasks) => {
      storage.setItem(TASK_STORE_KEY, JSON.stringify(tasks));
    },
  };
}

function readJson(storage: FileTransferStorage | null | undefined, key: string, invalidCode: string): unknown {
  if (!storage) return undefined;
  const raw = storage.getItem(key);
  if (raw === null) return undefined;
  try { return JSON.parse(raw); }
  catch { throw new Error(invalidCode); }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNonnegativeNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function isTransferTaskSnapshot(value: unknown): value is FileTransferTaskSnapshot {
  if (!isRecord(value)) return false;
  const requiredStrings = ["id", "sessionId", "target", "filePath", "fileName", "createdAt", "updatedAt"];
  const optionalStrings = ["hostId", "relativePath", "targetDirectory", "remotePath", "sha256", "errorMessage"];
  return requiredStrings.every(key => typeof value[key] === "string")
    && optionalStrings.every(key => value[key] === undefined || typeof value[key] === "string")
    && value.direction === "upload"
    && typeof value.state === "string" && ["queued", "transferring", "paused", "completed", "skipped", "failed", "canceled"].includes(value.state)
    && isNonnegativeNumber(value.transferredBytes) && isNonnegativeNumber(value.totalBytes)
    && isNonnegativeNumber(value.speedBytesPerSecond)
    && isNonnegativeNumber(value.progress) && value.progress <= 100
    && (value.phase === undefined || value.phase === "preparing" || value.phase === "transferring" || value.phase === "verifying")
    && (value.etaSeconds === null || isNonnegativeNumber(value.etaSeconds))
    && (value.renamePolicy === undefined || isRenamePolicy(value.renamePolicy));
}

function isRenamePolicy(value: unknown): value is TransferRenamePolicy {
  return value === "autoRename" || value === "overwrite" || value === "skip" || value === "ask";
}
