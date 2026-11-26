import {
  defaultConnectionConfirmOptions,
  type ConnectionConfirmOptions,
  type ConnectionConfirmOptionsInput,
} from "../core/connectionConfirm.ts";
import { normalizeKeyboardShortcut, validateKeyboardShortcut } from "../input/keyboardShortcut.ts";
import { isSessionQualityProfile, type SessionQualityProfile } from "../viewport/sessionDisplay.ts";
import type { TransferRenamePolicy } from "../transfers/fileTransferManager.ts";

export type FileTransferRenamePolicy = Exclude<TransferRenamePolicy, "ask">;

export function isFileTransferRenamePolicy(value: unknown): value is FileTransferRenamePolicy {
  return value === "autoRename" || value === "overwrite" || value === "skip";
}

export type ClipboardSyncMode =
  | "off"
  | "local-to-active"
  | "bidirectional-active"
  | "local-to-all"
  | "bidirectional-all";

export type SessionSettings = {
  defaultQualityProfile: SessionQualityProfile;
  confirmBeforeConnect: boolean;
  fullscreenExitShortcut: string;
  clipboardSyncMode: ClipboardSyncMode;
  artifactDirectory: string;
  artifactDirectoryMode: "builtin" | "custom";
  fileTransferRenamePolicy: FileTransferRenamePolicy;
  connectionDefaults: ConnectionConfirmOptions;
};

export type SessionSettingsInput = Partial<{
  defaultQualityProfile: SessionQualityProfile;
  confirmBeforeConnect: boolean;
  fullscreenExitShortcut: string;
  clipboardSyncMode: ClipboardSyncMode;
  artifactDirectory: string;
  artifactDirectoryMode: "builtin" | "custom";
  fileTransferRenamePolicy: FileTransferRenamePolicy;
  connectionDefaults: ConnectionConfirmOptionsInput;
}>;

const DEFAULT_SESSION_SETTINGS: SessionSettings = {
  defaultQualityProfile: "balanced",
  confirmBeforeConnect: true,
  fullscreenExitShortcut: "Control+Shift+F",
  clipboardSyncMode: "local-to-active",
  artifactDirectory: "",
  artifactDirectoryMode: "builtin",
  fileTransferRenamePolicy: "autoRename",
  connectionDefaults: {
    postConnectPresentation: "main",
    readonly: false,
    saveTemporaryHost: false,
  },
};

function booleanOrDefault(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

export function isClipboardSyncMode(value: unknown): value is ClipboardSyncMode {
  return (
    value === "off"
    || value === "local-to-active"
    || value === "bidirectional-active"
    || value === "local-to-all"
    || value === "bidirectional-all"
  );
}

export function normalizeFullscreenExitShortcut(value: unknown): string {
  const normalized = normalizeKeyboardShortcut(value, DEFAULT_SESSION_SETTINGS.fullscreenExitShortcut);
  const validation = validateKeyboardShortcut(normalized);
  return validation.valid ? validation.shortcut : DEFAULT_SESSION_SETTINGS.fullscreenExitShortcut;
}

export function createSessionSettings(input: SessionSettingsInput = {}): SessionSettings {
  return normalizeSessionSettings(input);
}

export function normalizeSessionSettings(input: SessionSettingsInput = {}): SessionSettings {
  const defaults = DEFAULT_SESSION_SETTINGS;
  const connectionDefaults = input.connectionDefaults ?? {};
  const artifactDirectory = typeof input.artifactDirectory === "string" ? input.artifactDirectory.trim() : defaults.artifactDirectory;

  return {
    defaultQualityProfile: isSessionQualityProfile(input.defaultQualityProfile)
      ? input.defaultQualityProfile
      : defaults.defaultQualityProfile,
    confirmBeforeConnect: booleanOrDefault(input.confirmBeforeConnect, defaults.confirmBeforeConnect),
    fullscreenExitShortcut: normalizeFullscreenExitShortcut(input.fullscreenExitShortcut),
    clipboardSyncMode: isClipboardSyncMode(input.clipboardSyncMode) ? input.clipboardSyncMode : defaults.clipboardSyncMode,
    artifactDirectory,
    artifactDirectoryMode: input.artifactDirectoryMode === "builtin" || input.artifactDirectoryMode === "custom"
      ? input.artifactDirectoryMode : artifactDirectory ? "custom" : "builtin",
    fileTransferRenamePolicy: isFileTransferRenamePolicy(input.fileTransferRenamePolicy) ? input.fileTransferRenamePolicy : defaults.fileTransferRenamePolicy,
    connectionDefaults: {
      ...defaultConnectionConfirmOptions(connectionDefaults),
      readonly: booleanOrDefault(connectionDefaults.readonly, defaults.connectionDefaults.readonly),
      saveTemporaryHost: booleanOrDefault(connectionDefaults.saveTemporaryHost, defaults.connectionDefaults.saveTemporaryHost),
    },
  };
}

export function updateSessionSettings(current: SessionSettings, patch: SessionSettingsInput): SessionSettings {
  return normalizeSessionSettings({
    ...current,
    ...patch,
    artifactDirectoryMode: patch.artifactDirectoryMode ?? (typeof patch.artifactDirectory === "string"
      ? patch.artifactDirectory.trim() ? "custom" : "builtin" : current.artifactDirectoryMode),
    connectionDefaults: {
      ...current.connectionDefaults,
      ...patch.connectionDefaults,
    },
  });
}
