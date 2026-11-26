import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  createTrayStartFailureNotification,
  refreshLocalShareTrayState,
  type TrayStateSnapshot,
} from "../../trayState.ts";
import type { Translate } from "@app/types";
import type { EventLogLevel } from "@diagnostics/eventLog";

export type TrayActionOptions = {
  invoke: (command: "update_tray_state", args: Record<string, unknown>) => Promise<unknown>;
  disabled?: () => boolean;
  isTauriUnavailableError: (error: unknown) => boolean;
  localVncServerMessage: { value: string };
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  refreshLocalVncServerStatus: () => Promise<void>;
  selectLocalShareWorkspace: () => Promise<void>;
  startLocalVncServer: () => Promise<void>;
  t: Translate;
  trayStateSnapshot: { value: TrayStateSnapshot };
};

export function createTrayActions(options: TrayActionOptions) {
  let lastSyncedSnapshot: string | undefined;
  let pendingSync = Promise.resolve();
  let startingFromTray = false;

  function syncTrayState(): Promise<void> {
    // Polls can arrive while IPC is pending. Read the latest state in order so
    // duplicate polls neither replace an open native menu nor race newer state.
    pendingSync = pendingSync.then(syncLatestTrayState);
    return pendingSync;
  }

  async function syncLatestTrayState() {
    if (options.disabled?.()) {
      return;
    }

    try {
      const snapshot = options.trayStateSnapshot.value;
      const serialized = JSON.stringify(snapshot);
      if (serialized === lastSyncedSnapshot) {
        return;
      }

      await options.invoke("update_tray_state", { snapshot });
      // Only successful updates count, so startup/IPC failures remain retryable.
      lastSyncedSnapshot = serialized;
    } catch (error) {
      if (!options.isTauriUnavailableError(error)) {
        options.logEvent("error", options.t("log.sourceSystem"), `托盘状态更新失败：${String(error)}`);
      }
    }
  }

  async function revealMainWindowFromFrontend() {
    try {
      const currentWindow = getCurrentWindow();
      await currentWindow.show();
      await currentWindow.unminimize();
      await currentWindow.setFocus();
    } catch {
      // Browser preview and some external-window paths cannot control the native main window.
    }
  }

  async function sendTrayNotification(title: string, body: string) {
    if (typeof Notification === "undefined") {
      return;
    }

    try {
      let permission = Notification.permission;
      if (permission === "default") {
        permission = await Notification.requestPermission();
      }
      if (permission === "granted") {
        new Notification(title, { body });
      }
    } catch (error) {
      options.logEvent("warning", options.t("log.sourceSystem"), `系统通知发送失败：${String(error)}`);
    }
  }

  async function startLocalShareFromTray() {
    if (options.disabled?.() || startingFromTray) return;
    const before = options.trayStateSnapshot.value;
    if (isAwaitingInput(before)) {
      await openLocalShare();
      return;
    }
    if (isBusy(before) || before.localShareRunning) return;
    if (!before.canStartLocalShare) {
      await openLocalShare();
      return;
    }

    startingFromTray = true;
    try {
      await options.startLocalVncServer();
      const after = options.trayStateSnapshot.value;
      if (isAwaitingInput(after)) {
        await openLocalShare();
      } else if (!isBusy(after) && after.localShareStatus !== "running") {
        const notification = createTrayStartFailureNotification(options.localVncServerMessage.value);
        void sendTrayNotification(notification.title, notification.body);
        await openLocalShare();
      }
    } finally {
      startingFromTray = false;
    }
  }

  function isAwaitingInput(snapshot: TrayStateSnapshot) {
    return snapshot.localShareAction === "awaiting_permission" || snapshot.localShareAction === "awaiting_password";
  }

  function isBusy(snapshot: TrayStateSnapshot) {
    return snapshot.localShareAction !== "idle"
      || snapshot.localShareStatus === "starting" || snapshot.localShareStatus === "stopping";
  }

  async function openLocalShare() {
    await revealMainWindowFromFrontend();
    await options.selectLocalShareWorkspace();
  }

  async function refreshLocalShareTray() {
    await refreshLocalShareTrayState({
      refreshLocalVncServerStatus: options.refreshLocalVncServerStatus,
      syncTrayState,
    });
  }

  return {
    refreshLocalShareTray,
    sendTrayNotification,
    startLocalShareFromTray,
    syncTrayState,
  };
}
