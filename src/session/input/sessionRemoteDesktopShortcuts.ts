import { shortcutFromKeyboardEvent } from "./keyboardShortcut.ts";

export type RemoteDesktopShortcutActionId =
  | "previous-desktop"
  | "next-desktop"
  | "overview"
  | "show-current-desktop";

export type RemoteDesktopShortcutMatchInput = {
  event: KeyboardEvent;
  externalWindow: boolean;
  remotePriority?: boolean;
};

const SHORTCUT_BY_EVENT: Record<string, RemoteDesktopShortcutActionId> = {
  "Control+Alt+ArrowLeft": "previous-desktop",
  "Control+Alt+ArrowRight": "next-desktop",
  "Control+Alt+ArrowUp": "overview",
  "Control+Alt+ArrowDown": "show-current-desktop",
};

export function matchRemoteDesktopShortcut(
  input: RemoteDesktopShortcutMatchInput,
): RemoteDesktopShortcutActionId | null {
  if (input.remotePriority || !input.externalWindow) {
    return null;
  }

  return SHORTCUT_BY_EVENT[shortcutFromKeyboardEvent(input.event)] ?? null;
}
