export type KeyboardShortcutValidation =
  | { valid: true; shortcut: string }
  | { valid: false; shortcut: string; reason: "empty" | "missing-modifier" };

export type KeyboardShortcutConflict = {
  id: string;
  labelKey: string;
  shortcut: string;
};

export type KeyboardShortcutDisplayPlatform = "macos" | "windows";

const MODIFIER_KEYS = ["Control", "Alt", "Shift", "Meta"] as const;
const STANDALONE_FUNCTION_KEYS = new Set([
  "Escape",
  "Tab",
  "Enter",
  "Backspace",
  "Delete",
  "Insert",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  ...Array.from({ length: 24 }, (_value, index) => `F${index + 1}`),
]);
const MODIFIER_ALIASES: Record<string, string> = {
  ctrl: "Control",
  control: "Control",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
  meta: "Meta",
  cmd: "Meta",
  command: "Meta",
  win: "Meta",
  windows: "Meta",
};

function normalizeKeyName(value: string): string {
  const trimmed = value.trim();
  const lower = trimmed.toLowerCase();

  if (MODIFIER_ALIASES[lower]) {
    return MODIFIER_ALIASES[lower];
  }

  if (lower === "esc") {
    return "Escape";
  }

  if (trimmed.length === 1) {
    return trimmed.toUpperCase();
  }

  return trimmed;
}

function keyNameFromKeyboardEvent(event: KeyboardEvent): string {
  if (/^Key[A-Z]$/.test(event.code)) {
    return event.code.slice("Key".length);
  }
  if (/^Digit[0-9]$/.test(event.code)) {
    return event.code.slice("Digit".length);
  }
  if (/^Numpad[0-9]$/.test(event.code)) {
    return event.code.slice("Numpad".length);
  }
  return event.key;
}

function uniqueParts(parts: string[]): string[] {
  return parts.filter((part, index) => parts.indexOf(part) === index);
}

export function normalizeKeyboardShortcut(value: unknown, fallback = ""): string {
  if (typeof value !== "string") {
    return fallback;
  }

  const parts = uniqueParts(
    value
      .split("+")
      .map(normalizeKeyName)
      .filter(Boolean),
  );

  const modifiers = MODIFIER_KEYS.filter((modifier) => parts.includes(modifier));
  const key = parts.find((part) => !MODIFIER_KEYS.includes(part as (typeof MODIFIER_KEYS)[number]));
  return [...modifiers, ...(key ? [key] : [])].join("+") || fallback;
}

export function shortcutFromKeyboardEvent(event: KeyboardEvent): string {
  const pressedModifiers = [
    event.ctrlKey ? "Control" : "",
    event.altKey ? "Alt" : "",
    event.shiftKey ? "Shift" : "",
    event.metaKey ? "Meta" : "",
  ].filter(Boolean);
  const key = normalizeKeyName(keyNameFromKeyboardEvent(event));
  const keyIsModifier = MODIFIER_KEYS.includes(key as (typeof MODIFIER_KEYS)[number]);

  return normalizeKeyboardShortcut([...pressedModifiers, ...(keyIsModifier ? [] : [key])].join("+"));
}

export function formatKeyboardShortcutSymbolsForPlatform(
  shortcut: string,
  platform: KeyboardShortcutDisplayPlatform = detectKeyboardShortcutDisplayPlatform(),
): string {
  if (platform !== "macos") {
    return formatKeyboardShortcutForPlatform(shortcut, platform);
  }

  const modifierLabels: Record<string, string> = {
    Control: "⌃",
    Alt: "⌥",
    Meta: "⌘",
    Shift: "⇧",
  };
  const keyLabels: Record<string, string> = {
    Enter: "↩︎",
    Escape: "⎋",
    Backspace: "⌫",
    Delete: "⌦",
    Tab: "⇥",
    ArrowLeft: "←",
    ArrowRight: "→",
    ArrowUp: "↑",
    ArrowDown: "↓",
  };

  return normalizeKeyboardShortcut(shortcut, shortcut)
    .split("+")
    .filter(Boolean)
    .map((part) => modifierLabels[part] ?? keyLabels[part] ?? part)
    .join("");
}

export function validateKeyboardShortcut(shortcut: string): KeyboardShortcutValidation {
  const normalized = normalizeKeyboardShortcut(shortcut);

  if (!normalized) {
    return { valid: false, shortcut: normalized, reason: "empty" };
  }

  const hasModifier = normalized
    .split("+")
    .some((part) => MODIFIER_KEYS.includes(part as (typeof MODIFIER_KEYS)[number]));

  const canUseStandalone = STANDALONE_FUNCTION_KEYS.has(normalized);

  if (!hasModifier && !canUseStandalone) {
    return { valid: false, shortcut: normalized, reason: "missing-modifier" };
  }

  return { valid: true, shortcut: normalized };
}

export function findKeyboardShortcutConflict(
  shortcut: string,
  records: KeyboardShortcutConflict[],
  currentId: string,
): KeyboardShortcutConflict | null {
  const normalized = normalizeKeyboardShortcut(shortcut);
  return records.find((record) =>
    record.id !== currentId && normalizeKeyboardShortcut(record.shortcut) === normalized
  ) ?? null;
}

export function detectKeyboardShortcutDisplayPlatform(
  navigatorLike: Pick<Navigator, "platform" | "userAgent"> | undefined = globalThis.navigator,
): KeyboardShortcutDisplayPlatform {
  const platform = `${navigatorLike?.platform ?? ""} ${navigatorLike?.userAgent ?? ""}`;
  return /win/i.test(platform) ? "windows" : "macos";
}

export function formatKeyboardShortcutForPlatform(
  shortcut: string,
  platform: KeyboardShortcutDisplayPlatform = detectKeyboardShortcutDisplayPlatform(),
): string {
  const modifierLabels: Record<KeyboardShortcutDisplayPlatform, Record<string, string>> = {
    macos: {
      Control: "Control",
      Alt: "Option",
      Meta: "Command",
      Shift: "Shift",
    },
    windows: {
      Control: "Ctrl",
      Alt: "Alt",
      Meta: "Win",
      Shift: "Shift",
    },
  };
  const keyLabels: Record<string, string> = {
    ArrowLeft: "←",
    ArrowRight: "→",
    ArrowUp: "↑",
    ArrowDown: "↓",
  };

  return normalizeKeyboardShortcut(shortcut, shortcut)
    .split("+")
    .filter(Boolean)
    .map((part) => modifierLabels[platform][part] ?? keyLabels[part] ?? part)
    .join("+");
}
