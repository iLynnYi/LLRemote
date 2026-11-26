import {
  createSessionSettings,
  isClipboardSyncMode,
  isFileTransferRenamePolicy,
  normalizeSessionSettings,
  type SessionSettings,
} from "./sessionSettings.ts";
import { validateKeyboardShortcut } from "../input/keyboardShortcut.ts";
import { isSessionQualityProfile } from "../viewport/sessionDisplay.ts";

export const SESSION_SETTINGS_STORAGE_KEY = "llremote.session-settings.v2";

export type SessionSettingsStorage = {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
};

function parseSettings(value: string): SessionSettings {
  let parsed: unknown;
  try { parsed = JSON.parse(value); }
  catch { throw new Error("session_settings_invalid"); }
  if (!isRecord(parsed)
    || !isSessionQualityProfile(parsed.defaultQualityProfile)
    || typeof parsed.confirmBeforeConnect !== "boolean"
    || typeof parsed.fullscreenExitShortcut !== "string"
    || !validateKeyboardShortcut(parsed.fullscreenExitShortcut).valid
    || !isClipboardSyncMode(parsed.clipboardSyncMode)
    || typeof parsed.artifactDirectory !== "string"
    || (parsed.artifactDirectoryMode !== undefined && parsed.artifactDirectoryMode !== "builtin" && parsed.artifactDirectoryMode !== "custom")
    || (parsed.fileTransferRenamePolicy !== undefined && !isFileTransferRenamePolicy(parsed.fileTransferRenamePolicy))
    || !isRecord(parsed.connectionDefaults)
    || (parsed.connectionDefaults.postConnectPresentation !== "main"
      && parsed.connectionDefaults.postConnectPresentation !== "fullscreen"
      && parsed.connectionDefaults.postConnectPresentation !== "external")
    || typeof parsed.connectionDefaults.readonly !== "boolean"
    || typeof parsed.connectionDefaults.saveTemporaryHost !== "boolean") {
    throw new Error("session_settings_invalid");
  }
  return normalizeSessionSettings({
    defaultQualityProfile: parsed.defaultQualityProfile,
    confirmBeforeConnect: parsed.confirmBeforeConnect,
    fullscreenExitShortcut: parsed.fullscreenExitShortcut,
    clipboardSyncMode: parsed.clipboardSyncMode,
    artifactDirectory: parsed.artifactDirectory,
    artifactDirectoryMode: parsed.artifactDirectoryMode,
    fileTransferRenamePolicy: parsed.fileTransferRenamePolicy,
    connectionDefaults: {
      postConnectPresentation: parsed.connectionDefaults.postConnectPresentation,
      readonly: parsed.connectionDefaults.readonly,
      saveTemporaryHost: parsed.connectionDefaults.saveTemporaryHost,
    },
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function loadSessionSettings(
  storage?: SessionSettingsStorage | null,
  key = SESSION_SETTINGS_STORAGE_KEY,
): SessionSettings {
  if (!storage) {
    return createSessionSettings();
  }

  const stored = storage.getItem(key);
  return stored === null ? createSessionSettings() : parseSettings(stored);
}

export function saveSessionSettings(
  storage: SessionSettingsStorage | null | undefined,
  settings: SessionSettings,
  key = SESSION_SETTINGS_STORAGE_KEY,
): boolean {
  if (!storage) {
    return false;
  }

  storage.setItem(key, JSON.stringify(normalizeSessionSettings(settings)));
  return true;
}
