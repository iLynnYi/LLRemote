import type { SessionSettingsStorage } from "@session/settings/sessionSettingsStore";

export function browserSessionSettingsStorage(): SessionSettingsStorage | undefined {
  return globalThis.localStorage;
}
