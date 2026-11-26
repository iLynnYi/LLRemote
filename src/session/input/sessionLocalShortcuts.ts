import {
  formatKeyboardShortcutForPlatform,
  normalizeKeyboardShortcut,
  shortcutFromKeyboardEvent,
  type KeyboardShortcutDisplayPlatform,
} from "./keyboardShortcut.ts";
import type { RemoteSessionState } from "../core/sessionManager.ts";

export type SessionLocalShortcutActionId =
  | "toggle-fullscreen"
  | "toggle-external-window"
  | "upload-file"
  | "open-file-transfer-center"
  | "open-message-dialog"
  | "retry-session";

export type SessionLocalShortcutScope = "session-viewport" | "connection-failed";

export type SessionLocalShortcutDefinition = {
  actionId: SessionLocalShortcutActionId;
  shortcut: string;
  label: string;
  nameKey: string;
  scope: SessionLocalShortcutScope;
  conditionKey: string;
  descriptionKey: string;
};

export type SessionLocalShortcutMatchInput = {
  event: KeyboardEvent;
  state?: RemoteSessionState | null;
  active: boolean;
  visible: boolean;
  editableTarget: boolean;
  remotePriority?: boolean;
};

const RECOVERABLE_STATES = new Set<RemoteSessionState>([
  "authentication failed",
  "idle",
  "security failure",
  "disconnected",
  "disconnected unexpectedly",
  "failed",
]);
const CONNECTED_STATES = new Set<RemoteSessionState>(["connected"]);

export const SESSION_LOCAL_SHORTCUTS: SessionLocalShortcutDefinition[] = [
  {
    actionId: "open-message-dialog",
    shortcut: "Control+Alt+M",
    label: "Control+Option+M",
    nameKey: "settings.sessionShortcuts.sendMessage",
    scope: "session-viewport",
    conditionKey: "settings.sessionShortcutConditions.messageSession",
    descriptionKey: "settings.sessionShortcutDescriptions.sendMessage",
  },
  {
    actionId: "toggle-external-window",
    shortcut: "Control+Alt+W",
    label: "Control+Option+W",
    nameKey: "settings.sessionShortcuts.toggleExternalWindow",
    scope: "session-viewport",
    conditionKey: "settings.sessionShortcutConditions.focusedSession",
    descriptionKey: "settings.sessionShortcutDescriptions.toggleExternalWindow",
  },
  {
    actionId: "toggle-fullscreen",
    shortcut: "Control+Alt+Enter",
    label: "Control+Option+Enter",
    nameKey: "settings.sessionShortcuts.toggleFullscreen",
    scope: "session-viewport",
    conditionKey: "settings.sessionShortcutConditions.focusedSession",
    descriptionKey: "settings.sessionShortcutDescriptions.toggleFullscreen",
  },
  {
    actionId: "upload-file",
    shortcut: "Control+Alt+U",
    label: "Control+Option+U",
    nameKey: "settings.sessionShortcuts.uploadFile",
    scope: "session-viewport",
    conditionKey: "settings.sessionShortcutConditions.fileTransferSession",
    descriptionKey: "settings.sessionShortcutDescriptions.uploadFile",
  },
  {
    actionId: "open-file-transfer-center",
    shortcut: "Control+Alt+T",
    label: "Control+Option+T",
    nameKey: "settings.sessionShortcuts.openFileTransferCenter",
    scope: "session-viewport",
    conditionKey: "settings.sessionShortcutConditions.focusedSession",
    descriptionKey: "settings.sessionShortcutDescriptions.openFileTransferCenter",
  },
  {
    actionId: "retry-session",
    shortcut: "Enter",
    label: "Enter",
    nameKey: "settings.sessionShortcuts.retrySession",
    scope: "connection-failed",
    conditionKey: "settings.sessionShortcutConditions.failedSession",
    descriptionKey: "settings.sessionShortcutDescriptions.retrySession",
  },
];

export function sessionLocalShortcutLabel(
  actionId: SessionLocalShortcutActionId,
  platform?: KeyboardShortcutDisplayPlatform,
): string | undefined {
  const shortcut = SESSION_LOCAL_SHORTCUTS.find((shortcut) => shortcut.actionId === actionId)?.shortcut;
  return shortcut ? formatKeyboardShortcutForPlatform(shortcut, platform) : undefined;
}

export function matchSessionLocalShortcut(input: SessionLocalShortcutMatchInput): SessionLocalShortcutActionId | null {
  if (input.remotePriority || !input.active || !input.visible || input.editableTarget) {
    return null;
  }

  const eventShortcut = shortcutFromKeyboardEvent(input.event);
  const match = SESSION_LOCAL_SHORTCUTS.find((shortcut) => {
    if (normalizeKeyboardShortcut(shortcut.shortcut) !== eventShortcut) {
      return false;
    }
    if (shortcut.actionId !== "retry-session") {
      return input.state ? CONNECTED_STATES.has(input.state) : false;
    }
    return input.state ? RECOVERABLE_STATES.has(input.state) : false;
  });

  return match?.actionId ?? null;
}
