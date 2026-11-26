import { computed, shallowRef } from "vue";
import { detectKeyboardShortcutDisplayPlatform } from "../../../session/input/keyboardShortcut.ts";
import {
  SESSION_KEYBOARD_PREFERENCES_KEY,
  normalizeSessionKeyboardPreferences,
  parseSessionKeyboardPreferences,
  type SessionKeyboardPreferences,
} from "../../../session/input/sessionKeyboardPreferences.ts";

function readPreferences() {
  try { return parseSessionKeyboardPreferences(globalThis.localStorage?.getItem(SESSION_KEYBOARD_PREFERENCES_KEY) ?? null); }
  catch { return {}; }
}

// Shared by the viewport and its toolbox. The storage event synchronizes other windows.
const preferencesByTarget = shallowRef(readPreferences());
globalThis.window?.addEventListener("storage", (event) => {
  if (event.key === SESSION_KEYBOARD_PREFERENCES_KEY || event.key === null) preferencesByTarget.value = readPreferences();
});

export function sessionKeyboardPreferences(target: string): SessionKeyboardPreferences {
  return normalizeSessionKeyboardPreferences(preferencesByTarget.value[target]);
}

export function useSessionKeyboardPreferences(getTarget: () => string) {
  const windowsClient = detectKeyboardShortcutDisplayPlatform() === "windows";
  const preferences = computed(() => sessionKeyboardPreferences(getTarget()));
  function setPreference(key: keyof SessionKeyboardPreferences, value: boolean): boolean {
    const target = getTarget();
    if (!target) return false;
    const next = { ...preferencesByTarget.value, [target]: { ...preferences.value, [key]: value } };
    try {
      globalThis.localStorage.setItem(SESSION_KEYBOARD_PREFERENCES_KEY, JSON.stringify(next));
      preferencesByTarget.value = next;
      return true;
    } catch { return false; }
  }
  return { preferences, windowsClient, setPreference };
}
