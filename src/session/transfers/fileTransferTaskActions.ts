import type { TransferTask } from "./fileTransferManager.ts";

export type TransferTaskPathAction = "copy-path" | "open-directory";
export type TransferBatchAction = "pause" | "resume";

export function formatTransferSessionLabel(session: { label: string; target: string }): string {
  const label = session.label.trim();
  const target = session.target.trim();
  return !label || label === target ? target : `${label} · ${target}`;
}

/** The caller supplies the current session and status filter results. */
export function resolveTransferBatchAction(
  tasks: readonly Pick<TransferTask, "state">[],
): TransferBatchAction | null {
  if (tasks.some(task => task.state === "transferring" || task.state === "queued")) return "pause";
  return tasks.some(task => task.state === "paused") ? "resume" : null;
}

/** Freeze the visible cleanup scope; queued and active transfers must not be included. */
export function resolveTransferHistorySelection(
  tasks: readonly Pick<TransferTask, "id" | "state">[],
): { taskIds: string[]; pausedCount: number } {
  const removable = tasks.filter(task => ["paused", "completed", "failed", "skipped", "canceled"].includes(task.state));
  return { taskIds: removable.map(task => task.id), pausedCount: removable.filter(task => task.state === "paused").length };
}

/** Resolves only completed uploads owned by the requesting session; never accepts a caller supplied path. */
export function resolveTransferTaskPathAction(
  tasks: readonly TransferTask[],
  sessionId: string,
  taskId: string,
  action: TransferTaskPathAction,
): { sessionId: string; path: string } | null {
  const task = tasks.find((item) => item.id === taskId && item.sessionId === sessionId);
  if (task?.state !== "completed" || !task.remotePath) return null;
  if (action === "copy-path") return { sessionId, path: task.remotePath };
  const separator = Math.max(task.remotePath.lastIndexOf("/"), task.remotePath.lastIndexOf("\\"));
  if (separator < 0) return null;
  const path = separator === 0 ? task.remotePath.slice(0, 1)
    : separator === 2 && task.remotePath[1] === ":" ? task.remotePath.slice(0, 3)
      : task.remotePath.slice(0, separator);
  return { sessionId, path };
}
