import { useVncAppContext } from "@app/composables/useVncAppContext";
import { createSystemClipboard } from "@session/clipboard/systemClipboard";
import { sessionSupportsCapability } from "@session/connection-model/connectionModel";
import { resolveTransferTaskPathAction, type TransferTaskPathAction } from "@session/transfers/fileTransferTaskActions";

/** Executes task path actions through the connection owner in the main window. */
export function useTransferTaskActions() {
  const { findSession, sessionTabs, sessionListTick, getRemoteViewportSummary, sessionFileTransferManager, sessionConnectionRegistry, showAppMessage, t } = useVncAppContext();
  const clipboard = createSystemClipboard();

  function canOpenRemoteDirectory(sessionId: string): boolean {
    sessionListTick.value;
    const tab = sessionTabs.value.find((item) => item.id === sessionId);
    const connection = sessionConnectionRegistry.get(sessionId)?.connection;
    if (!tab) return false;
    return findSession(sessionId)?.state === "connected"
      && getRemoteViewportSummary(tab).inputMode !== "readonly"
      && sessionSupportsCapability(connection?.connectionModel.capabilities, "file.reveal");
  }

  async function runTaskPathAction(
    sessionId: string, taskId: string, action: TransferTaskPathAction,
    notify: (message: string, tone: "success" | "error") => void = showAppMessage,
  ) {
    const request = resolveTransferTaskPathAction(sessionFileTransferManager.tasks.value, sessionId, taskId, action);
    if (!request) return;
    try {
      if (action === "copy-path") {
        if (!clipboard) throw new Error(t("sessions.toolbox.disabled.unavailable"));
        await clipboard.writeText(request.path);
        notify(t("sessions.toolbox.feedback.fileUploadPathCopied"), "success");
        return;
      }
      const session = findSession(sessionId);
      const connection = sessionConnectionRegistry.get(sessionId)?.connection;
      if (session?.state !== "connected" || !connection) throw new Error(t("sessions.toolbox.disabled.disconnected"));
      const tab = sessionTabs.value.find((item) => item.id === sessionId);
      if (!tab || getRemoteViewportSummary(tab).inputMode === "readonly") throw new Error(t("sessions.toolbox.disabled.readonly"));
      if (!sessionSupportsCapability(connection.connectionModel.capabilities, "file.reveal")) {
        throw new Error(t("sessions.toolbox.disabled.enhancedCapability"));
      }
      await connection.revealRemoteFile(request.path);
    } catch (error) {
      notify(t("sessions.toolbox.feedback.failed", { error: String(error) }), "error");
    }
  }

  return { runTaskPathAction, canOpenRemoteDirectory };
}
