import type { DropdownOption } from "naive-ui";
import type { SessionToolboxActionId } from "@session/transfers/sessionToolbox";
import type { SessionViewportScaleMode } from "@session/viewport/sessionViewportScale";

export type MenuCommand =
  | { kind: "action"; actionId: SessionToolboxActionId }
  | { kind: "display-scale"; value: SessionViewportScaleMode }
  | { kind: "display"; value: string }
  | { kind: "open-text-dialog" }
  | { kind: "clipboard-text" };

export const WINDOWS_SHORTCUT_ACTION_IDS: SessionToolboxActionId[] = [
  "shortcut-windows-window-switch",
  "shortcut-windows-run",
  "shortcut-windows-task-manager",
  "shortcut-windows-secure-attention",
];

export const MACOS_SHORTCUT_ACTION_IDS: SessionToolboxActionId[] = [
  "shortcut-macos-force-quit",
  "shortcut-macos-window-switch",
  "shortcut-macos-next-desktop",
  "shortcut-macos-previous-desktop",
  "shortcut-macos-mission-control",
];

export function commandKey(command: MenuCommand): string {
  switch (command.kind) {
    case "action":
      return `action:${command.actionId}`;
    case "display-scale":
      return `display-scale:${command.value}`;
    case "display":
      return `display:${encodeURIComponent(command.value)}`;
    case "open-text-dialog":
      return "open-text-dialog";
    case "clipboard-text":
      return "clipboard-text";
  }
}

export function parseCommandKey(key: string): MenuCommand | null {
  const [kind, value] = key.split(":") as [string, string | undefined];
  switch (kind) {
    case "action":
      return value ? { kind: "action", actionId: value as SessionToolboxActionId } : null;
    case "display-scale":
      return value ? { kind: "display-scale", value: value as SessionViewportScaleMode } : null;
    case "display":
      return value ? { kind: "display", value: decodeURIComponent(value) } : null;
    case "open-text-dialog":
      return { kind: "open-text-dialog" };
    default:
      return null;
  }
}

export function remoteDisplayIdFromCommandValue(value: string): string | null {
  return value.startsWith("remote:") ? value.slice("remote:".length) : null;
}

export function isDropdownOption(option: DropdownOption | null): option is DropdownOption {
  return Boolean(option);
}

export function isToolboxTriggerEvent(event: Event): boolean {
  return event.composedPath().some((target) =>
    target instanceof Element && Boolean(target.closest(".session-toolbox-trigger")),
  );
}

export function isSessionToolboxEvent(event: Event, sessionId: string): boolean {
  return event.composedPath().some((target) =>
    target instanceof Element && target.getAttribute("data-session-toolbox-owner") === sessionId,
  );
}
