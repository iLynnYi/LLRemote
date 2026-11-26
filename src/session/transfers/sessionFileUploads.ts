import type { ProtocolConnection } from "../protocol/protocolAdapter.ts";
import type { FileTransferRenamePolicy } from "../settings/sessionSettings.ts";
import type { FileTransferManager, TransferUploadRequest } from "./fileTransferManager.ts";
import type { FileTransferSource } from "./fileTransferSources.ts";
import { sessionSupportsAction } from "./sessionCapabilities.ts";
import { isFileTransferConflictSkipped } from "./fileTransferErrors.ts";

type UploadConnection = Pick<ProtocolConnection, "connectionModel" | "uploadRemoteFile" | "controlRemoteFileTransfer">;
type UploadFeedback = {
  onFileCompleted?: (remotePath: string, total: number) => void;
  onBatchFinished?: (completed: number, failed: number, skipped: number) => void;
};

/** One upload path for picker, drag/drop and external-window requests. UI owns feedback. */
export function createSessionFileUploadActions(options: {
  findSession: (sessionId: string) => { target: string } | undefined;
  getConnection: (sessionId: string) => UploadConnection | undefined;
  getRenamePolicy: () => FileTransferRenamePolicy;
  expandSources: (paths: string[]) => Promise<unknown>;
  enqueueUploads: FileTransferManager["enqueueUploads"];
  enqueueUploadsDurably?: FileTransferManager["enqueueUploadsDurably"];
  t: (key: string) => string;
  onUploaded: (result: Awaited<ReturnType<ProtocolConnection["uploadRemoteFile"]>>) => void;
}) {
  function connectionFor(sessionId: string): UploadConnection {
    const connection = options.getConnection(sessionId);
    if (!connection) throw new Error(options.t("sessions.toolbox.disabled.disconnected"));
    if (!sessionSupportsAction(connection.connectionModel.capabilities, "sendFile")) {
      throw new Error(options.t("sessions.toolbox.disabled.enhancedCapability"));
    }
    return connection;
  }

  async function enqueue(sessionId: string, sourcePaths: string[], feedback: UploadFeedback = {}): Promise<number> {
    const session = options.findSession(sessionId);
    if (!session) throw new Error(options.t("sessions.toolbox.disabled.disconnected"));
    connectionFor(sessionId);
    const expanded = await options.expandSources(sourcePaths);
    if (!Array.isArray(expanded) || !expanded.length || !expanded.every(isFileTransferSource)) {
      throw new Error(options.t("sessions.toolbox.disabled.noFileSelected"));
    }
    connectionFor(sessionId);
    const total = expanded.length;
    let completed = 0, failed = 0, skipped = 0, notified = false;
    function settle(remotePath?: string, skip = false) {
      if (remotePath !== undefined) {
        completed += 1;
        feedback.onFileCompleted?.(remotePath, total);
      } else if (skip) skipped += 1;
      else failed += 1;
      if (!notified && completed + failed + skipped >= total) {
        notified = true;
        feedback.onBatchFinished?.(completed, failed, skipped);
      }
    }
    const requests: TransferUploadRequest[] = [];
    for (let offset = 0; offset < expanded.length; offset += 200) {
      for (const source of expanded.slice(offset, offset + 200)) requests.push({
        sessionId, target: session.target, filePath: source.filePath,
        relativePath: source.relativePath ?? undefined,
        renamePolicy: options.getRenamePolicy(),
        control: async (taskId, action) => connectionFor(sessionId).controlRemoteFileTransfer(taskId, action),
        upload: async ({ taskId, signal, onProgress }) => {
          let result;
          try {
            result = await connectionFor(sessionId).uploadRemoteFile(source.filePath, {
              transferId: taskId,
              relativePath: source.relativePath ?? undefined,
              renamePolicy: options.getRenamePolicy(), signal,
              onProgress: progress => onProgress({
                transferredBytes: progress.transferredBytes, totalBytes: progress.totalBytes,
                ...(progress.phase ? { phase: progress.phase } : {}),
              }),
            });
          } catch (error) {
            if (signal.reason !== "paused") settle(undefined, isFileTransferConflictSkipped(error));
            throw error;
          }
          options.onUploaded(result);
          settle(result.remotePath);
          return result;
        },
      });
      if (offset + 200 < expanded.length) await new Promise(resolve => setTimeout(resolve, 0));
    }
    try {
      await (options.enqueueUploadsDurably ?? options.enqueueUploads)(requests);
    } catch {
      throw new Error(options.t("sessions.fileTransfers.queueStorageFailed"));
    }
    return total;
  }
  return { enqueue };
}

export type SessionFileUploadActions = ReturnType<typeof createSessionFileUploadActions>;

function isFileTransferSource(value: unknown): value is FileTransferSource {
  return typeof value === "object" && value !== null && "filePath" in value && typeof value.filePath === "string"
    && (!("relativePath" in value) || value.relativePath == null || typeof value.relativePath === "string");
}
