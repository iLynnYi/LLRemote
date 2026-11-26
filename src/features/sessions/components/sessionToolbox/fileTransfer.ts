import { emit } from "@tauri-apps/api/event";
import type { Translate } from "@app/types";
import {
  sessionExternalWindowToolboxCommandResultEvent,
  type SessionExternalWindowToolboxCommandPayload,
} from "@session/external-window/sessionExternalWindow";
import type { RemoteSession } from "@session/core/sessionManager";
import type { FileTransferManager } from "@session/transfers/fileTransferManager";
import type { SessionFileUploadActions } from "@session/transfers/sessionFileUploads";
import { selectFileTransferSources } from "@session/transfers/fileTransferSources";
import type { MenuCommand } from "./menuOptions.ts";

export const DEFAULT_REMOTE_RECEIVE_DIRECTORY = "~/Downloads/LL Remote";

export type DispatchFileTransferActionOptions = {
  commandKey: (command: MenuCommand) => string;
  consumeExternalFileTransferSourcePaths: () => string[] | null;
  currentFileTransferReceiveDirectory: () => string;
  emitExternalToolboxCommand: (
    key: string,
    extra?: Partial<SessionExternalWindowToolboxCommandPayload>,
  ) => Promise<void>;
  externalToolboxFeedbackTarget: SessionExternalWindowToolboxCommandPayload["feedbackTarget"] | null;
  externalWindowSessionId?: string | null;
  enqueueSessionFileUploads: SessionFileUploadActions["enqueue"];
  invoke: (command: "select_file_transfer_sources") => Promise<unknown>;
  openTransferCenter: (sessionId: string) => void;
  isTransferCenterVisible: () => boolean;
  recordFailure: (error: unknown) => void;
  sessionId: string;
  showMessage: (message: string, tone: "success" | "warning" | "error") => void;
  t: Translate;
};

export async function dispatchFileTransferAction(options: DispatchFileTransferActionOptions): Promise<boolean> {
  const selectedSourcePaths = options.consumeExternalFileTransferSourcePaths()
    ?? await selectFileTransferSources(options.invoke, options.t);
  if (!selectedSourcePaths.length) {
    return false;
  }
  if (options.externalWindowSessionId === options.sessionId) {
    await options.emitExternalToolboxCommand(options.commandKey({ kind: "action", actionId: "send-file" }), {
      selectedFileTransferSourcePaths: selectedSourcePaths,
    });
    return false;
  }
  await options.enqueueSessionFileUploads(options.sessionId, selectedSourcePaths, {
    onFileCompleted: (remotePath, count) => {
      if (count !== 1) return;
      void navigator.clipboard.writeText(remoteDirectoryFromPath(remotePath) || options.currentFileTransferReceiveDirectory()).catch(options.recordFailure);
    },
    onBatchFinished: (completed, failed, skipped) => {
      if (!failed && !skipped && options.isTransferCenterVisible()) return;
      const message = options.t("sessions.toolbox.feedback.fileUploadBatchFinished", { completed, failed, skipped });
      const tone = failed ? "error" : skipped ? "warning" : "success";
      if (options.externalToolboxFeedbackTarget === "external") {
        void emit(sessionExternalWindowToolboxCommandResultEvent(options.sessionId), {
          sessionId: options.sessionId, tone, message,
        });
      } else options.showMessage(message, tone);
    },
  });
  if (options.externalToolboxFeedbackTarget !== "external") {
    options.openTransferCenter(options.sessionId);
  }
  return false;
}

export function remoteDirectoryFromPath(remotePath: string): string {
  const normalized = remotePath.trim();
  const separatorIndex = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"));
  return separatorIndex > 0 ? normalized.slice(0, separatorIndex) : "";
}

export function resolveCurrentFileTransferReceiveDirectory(options: {
  session: RemoteSession | undefined;
  sessionId: string;
  sessionFileTransferManager: FileTransferManager;
}): string {
  const session = options.session;
  if (!session) {
    return DEFAULT_REMOTE_RECEIVE_DIRECTORY;
  }
  const completedDirectory = latestCompletedTransferRemoteDirectory(options.sessionId, options.sessionFileTransferManager);
  return completedDirectory
    || DEFAULT_REMOTE_RECEIVE_DIRECTORY;
}

function latestCompletedTransferRemoteDirectory(
  sessionId: string,
  sessionFileTransferManager: FileTransferManager,
): string {
  const tasks = sessionFileTransferManager.tasksForSession(sessionId);
  for (const task of [...tasks].reverse()) {
    if (task.state !== "completed" || !task.remotePath) {
      continue;
    }
    const directory = remoteDirectoryFromPath(task.remotePath);
    if (directory) {
      return directory;
    }
  }
  return "";
}
