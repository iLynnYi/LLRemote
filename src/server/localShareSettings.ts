import type { SessionSettingsStorage } from "@session/settings/sessionSettingsStore.ts";
import { isValidIpv4Address } from "./localShare.ts";

export const LOCAL_SHARE_SETTINGS_STORAGE_KEY = "llremote.local-share-settings.v1";
export const SERVICE_SUGGESTION_STORAGE_KEY = "llremote.local-share.service-suggestion.v1";

export function serviceSuggestionHandled(storage?: SessionSettingsStorage | null): boolean {
  try { return storage?.getItem(SERVICE_SUGGESTION_STORAGE_KEY) === "handled"; }
  catch { return false; }
}

export type LocalShareSettings = {
  port: number;
  bindAddress: string;
  viewOnly: boolean;
};

export type LocalShareSettingsInput = Partial<{
  port: unknown;
  bindAddress: unknown;
  viewOnly: unknown;
}>;

export function createLocalShareSettings(input: LocalShareSettingsInput = {}): LocalShareSettings {
  return normalizeLocalShareSettings(input);
}

export function normalizeLocalShareSettings(input: LocalShareSettingsInput = {}): LocalShareSettings {
  const port = typeof input.port === "number" && Number.isInteger(input.port) && input.port >= 1 && input.port <= 65535
    ? input.port
    : 5903;
  const bindAddress = typeof input.bindAddress === "string" && isValidIpv4Address(input.bindAddress)
    ? input.bindAddress
    : "0.0.0.0";

  return {
    port,
    bindAddress,
    viewOnly: Boolean(input.viewOnly),
  };
}

function parseSettings(parsed: unknown): LocalShareSettings {
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)
    || !("port" in parsed) || typeof parsed.port !== "number"
    || !Number.isInteger(parsed.port) || parsed.port < 1 || parsed.port > 65535
    || !("bindAddress" in parsed) || typeof parsed.bindAddress !== "string" || !isValidIpv4Address(parsed.bindAddress)
    || !("viewOnly" in parsed) || typeof parsed.viewOnly !== "boolean") {
    throw new Error("local_share_settings_invalid");
  }
  return { port: parsed.port, bindAddress: parsed.bindAddress, viewOnly: parsed.viewOnly };
}

export function loadLocalShareSettings(
  storage?: SessionSettingsStorage | null,
  key = LOCAL_SHARE_SETTINGS_STORAGE_KEY,
): LocalShareSettings {
  if (!storage) {
    return createLocalShareSettings();
  }

  const stored = storage.getItem(key);
  if (stored === null) return createLocalShareSettings();
  let parsed: unknown;
  try { parsed = JSON.parse(stored); }
  catch { throw new Error("local_share_settings_invalid"); }
  return parseSettings(parsed);
}

export function saveLocalShareSettings(
  storage: SessionSettingsStorage | null | undefined,
  settings: LocalShareSettings,
  key = LOCAL_SHARE_SETTINGS_STORAGE_KEY,
): boolean {
  if (!storage) {
    return false;
  }

  storage.setItem(key, JSON.stringify(normalizeLocalShareSettings(settings)));
  return true;
}
