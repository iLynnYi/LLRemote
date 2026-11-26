export type SessionKeyboardPreferences = {
  rightControlAsCommand: boolean;
};

export const SESSION_KEYBOARD_PREFERENCES_KEY = "llremote.session-keyboard.v1";

export function normalizeSessionKeyboardPreferences(value: unknown): SessionKeyboardPreferences {
  const input = value && typeof value === "object" ? value as Record<string, unknown> : {};
  return {
    rightControlAsCommand: input.rightControlAsCommand === true,
  };
}

export function parseSessionKeyboardPreferences(value: string | null): Record<string, SessionKeyboardPreferences> {
  if (!value) return {};
  try {
    const parsed: unknown = JSON.parse(value);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    return Object.fromEntries(Object.entries(parsed).map(([target, preferences]) => [target, normalizeSessionKeyboardPreferences(preferences)]));
  } catch { return {}; }
}
