import { remoteMessageTextError } from "../messages/remoteMessage.ts";
import type { RemoteKeyDispatch } from "../input/keyboardMapping.ts";
import {
  sessionSupportsAction,
  type SessionCapabilityAction,
} from "./sessionCapabilities.ts";
import type { RemoteSessionState } from "../core/sessionManager.ts";
import { sessionLocalShortcutLabel } from "../input/sessionLocalShortcuts.ts";
import type { CapabilityMatrix } from "../connection-model/connectionModel.ts";
import type { SessionDisplayChoice } from "../viewport/sessionDisplayChoices.ts";

export type SessionToolboxGroupId =
  | "viewWindow"
  | "inputControl"
  | "fileTransfer"
  | "more";

export type SessionToolboxActionStatus = "enabled" | "disabled";

export type SessionToolboxExecutionKind =
  | "ui"
  | "shortcut"
  | "wakeDisplay"
  | "remoteMessage"
  | "fileTransfer"
  | "diagnostics";

export type SessionToolboxContext = {
  sessionId?: string | null;
  state?: RemoteSessionState | null;
  readonly?: boolean;
  viewOnlyShared?: boolean;
  agentChannel?: boolean;
  capabilities?: CapabilityMatrix | null;
  active?: boolean;
  fullscreen?: boolean;
  externalWindow?: boolean;
  localServer?: boolean;
  recording?: boolean;
  recordingPhase?: import("../artifacts/sessionRecordingTask.ts").RecordingPhase;
  recordingElapsedSeconds?: number;
  recordingSpace?: import("../artifacts/sessionRecordingTask.ts").RecordingSpaceStatus;
  fileTransferAvailable?: boolean;
  displayChoices?: SessionDisplayChoice[];
};

export type SessionToolboxGroup = {
  id: SessionToolboxGroupId;
  labelKey: string;
};

export type SessionToolboxActionId =
  | "toggle-fullscreen"
  | "exit-fullscreen"
  | "toggle-external-window"
  | "toggle-hud"
  | "open-file-transfer-center"
  | "wake-display"
  | "toggle-readonly"
  | "shortcut-windows-secure-attention"
  | "shortcut-windows-window-switch"
  | "shortcut-windows-run"
  | "shortcut-windows-task-manager"
  | "shortcut-macos-force-quit"
  | "shortcut-macos-window-switch"
  | "shortcut-macos-next-desktop"
  | "shortcut-macos-previous-desktop"
  | "shortcut-macos-mission-control"
  | "send-message"
  | "send-file"
  | "retry-session"
  | "capture-screenshot"
  | "toggle-recording"
  | "copy-diagnostic-summary"
  | "copy-target"
  | "open-diagnostics";

export type SessionToolboxAction = {
  id: SessionToolboxActionId;
  group: SessionToolboxGroupId;
  labelKey: string;
  descriptionKey: string;
  kind: SessionToolboxExecutionKind;
  status: SessionToolboxActionStatus;
  disabledReasonKey?: string;
  shortcutLabel?: string;
  dispatches?: RemoteKeyDispatch[];
  successKey?: string;
};

export type SessionToolboxActionRequest =
  | {
    sessionId: string;
    actionId: SessionToolboxActionId;
    kind: "remoteMessage";
    text: string;
  }
  | {
    sessionId: string;
    actionId: SessionToolboxActionId;
    kind: "shortcut";
    dispatches: RemoteKeyDispatch[];
  }
  | {
    sessionId: string;
    actionId: SessionToolboxActionId;
    kind: "ui" | "wakeDisplay" | "diagnostics";
  };

export type CreateSessionToolboxActionRequestInput = {
  context: SessionToolboxContext;
  actionId: SessionToolboxActionId;
  text?: string;
};

export type SessionToolboxButtonSnapInput = {
  x: number;
  y: number;
  workspaceWidth: number;
  workspaceHeight: number;
  buttonWidth: number;
  buttonHeight: number;
  inset: number;
  snapDistance: number;
};

export type SessionToolboxVisibleButtonInput = {
  position: SessionToolboxButtonPosition;
  workspaceWidth: number;
  workspaceHeight: number;
  buttonWidth: number;
  buttonHeight: number;
  inset: number;
  snapDistance: number;
};

export type SessionToolboxButtonCorner = "top-left" | "top-right" | "bottom-left" | "bottom-right" | null;

export type SessionToolboxButtonPosition = {
  x: number;
  y: number;
  corner: SessionToolboxButtonCorner;
};

const TOOLBOX_GROUPS: SessionToolboxGroup[] = [
  { id: "viewWindow", labelKey: "sessions.toolbox.groups.viewWindow" },
  { id: "inputControl", labelKey: "sessions.toolbox.groups.inputControl" },
  { id: "fileTransfer", labelKey: "sessions.toolbox.groups.fileTransfer" },
  { id: "more", labelKey: "sessions.toolbox.groups.more" },
];

const CONNECTED_STATES = new Set<RemoteSessionState>(["connected"]);
const RECOVERABLE_STATES = new Set<RemoteSessionState>([
  "authentication failed",
  "idle",
  "security failure",
  "disconnected",
  "disconnected unexpectedly",
  "failed",
]);

const SPECIAL_KEYSYMS: Record<string, number> = {
  Escape: 0xff1b,
  Delete: 0xffff,
  Enter: 0xff0d,
  Tab: 0xff09,
  ArrowUp: 0xff52,
  ArrowDown: 0xff54,
  ArrowLeft: 0xff51,
  ArrowRight: 0xff53,
  Control: 0xffe3,
  Shift: 0xffe1,
  Alt: 0xffe9,
  Command: 0xffeb,
  Win: 0xffeb,
};

const SPECIAL_KEY_CODES: Partial<Record<string, string>> = {
  Control: "ControlLeft",
  Alt: "AltLeft",
  Shift: "ShiftLeft",
  Command: "MetaLeft",
  Win: "MetaLeft",
};

type ShortcutDefinition = {
  label: string;
  dispatches: RemoteKeyDispatch[];
};

type ActionBlueprint = {
  id: SessionToolboxActionId;
  group: SessionToolboxGroupId;
  kind: SessionToolboxExecutionKind;
  requiresConnected?: boolean;
  requiresControl?: boolean;
  requiresRecoverable?: boolean;
  requiresCapabilityAction?: SessionCapabilityAction;
  localShortcutActionId?: Parameters<typeof sessionLocalShortcutLabel>[0];
  shortcut?: () => ShortcutDefinition | null;
};

const ACTION_BLUEPRINTS: ActionBlueprint[] = [
  { id: "toggle-fullscreen", group: "viewWindow", kind: "ui", requiresConnected: true, localShortcutActionId: "toggle-fullscreen" },
  { id: "exit-fullscreen", group: "viewWindow", kind: "ui" },
  { id: "toggle-external-window", group: "viewWindow", kind: "ui", requiresConnected: true, localShortcutActionId: "toggle-external-window" },
  { id: "toggle-hud", group: "more", kind: "ui", requiresConnected: true },
  { id: "wake-display", group: "more", kind: "wakeDisplay", requiresConnected: true, requiresControl: true },
  { id: "toggle-readonly", group: "inputControl", kind: "ui", requiresConnected: true },
  { id: "shortcut-windows-secure-attention", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: windowsSecureAttentionShortcut },
  { id: "shortcut-windows-window-switch", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: windowsWindowSwitchShortcut },
  { id: "shortcut-windows-run", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: windowsRunShortcut },
  { id: "shortcut-windows-task-manager", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: windowsTaskManagerShortcut },
  { id: "shortcut-macos-force-quit", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: macosForceQuitShortcut },
  { id: "shortcut-macos-window-switch", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: macosWindowSwitchShortcut },
  { id: "shortcut-macos-next-desktop", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: macosNextDesktopShortcut },
  { id: "shortcut-macos-previous-desktop", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: macosPreviousDesktopShortcut },
  { id: "shortcut-macos-mission-control", group: "inputControl", kind: "shortcut", requiresConnected: true, requiresControl: true, shortcut: macosMissionControlShortcut },
  { id: "send-message", group: "inputControl", kind: "remoteMessage", requiresConnected: true, requiresControl: true, requiresCapabilityAction: "sendMessage", localShortcutActionId: "open-message-dialog" },
  { id: "send-file", group: "fileTransfer", kind: "fileTransfer", requiresConnected: true, requiresControl: true, requiresCapabilityAction: "sendFile", localShortcutActionId: "upload-file" },
  { id: "open-file-transfer-center", group: "fileTransfer", kind: "ui", localShortcutActionId: "open-file-transfer-center" },
  { id: "retry-session", group: "more", kind: "diagnostics", requiresRecoverable: true },
  { id: "capture-screenshot", group: "more", kind: "diagnostics", requiresConnected: true },
  { id: "toggle-recording", group: "more", kind: "diagnostics", requiresConnected: true },
  { id: "copy-diagnostic-summary", group: "more", kind: "diagnostics" },
  { id: "copy-target", group: "more", kind: "ui" },
  { id: "open-diagnostics", group: "more", kind: "ui" },
];

// Visible state changes and artifact operations provide their own feedback.
const SUCCESS_FEEDBACK_ACTIONS = new Set<SessionToolboxActionId>([
  "wake-display", "copy-target",
]);

/** Keeps disconnected viewports focused on recovery instead of unavailable remote actions. */
export function resolveSessionToolboxPresentation(context: SessionToolboxContext) {
  const connected = context.state === "connected";
  return {
    connected,
    recoverable: Boolean(context.sessionId && context.state && RECOVERABLE_STATES.has(context.state)),
    fileTransfer: connected && (Boolean(context.fileTransferAvailable) || sessionSupportsAction(context.capabilities, "sendFile")),
  };
}

export function listSessionToolboxGroups(): SessionToolboxGroup[] {
  return [...TOOLBOX_GROUPS];
}

export function resolveSessionToolboxActions(context: SessionToolboxContext): SessionToolboxAction[] {
  return ACTION_BLUEPRINTS.map((blueprint) => {
    const shortcut = blueprint.shortcut?.() ?? null;
    const disabledReasonKey = disabledReasonForAction(blueprint, context);
    return {
      id: blueprint.id,
      group: blueprint.group,
      labelKey: labelKeyForAction(blueprint, context, disabledReasonKey),
      descriptionKey: `sessions.toolbox.actions.${blueprint.id}.description`,
      kind: blueprint.kind,
      status: disabledReasonKey ? "disabled" : "enabled",
      disabledReasonKey,
      shortcutLabel: shortcut?.label ?? localShortcutLabelForAction(blueprint),
      dispatches: shortcut?.dispatches,
      successKey: SUCCESS_FEEDBACK_ACTIONS.has(blueprint.id)
        ? `sessions.toolbox.feedback.${blueprint.id}` : undefined,
    };
  });
}

function localShortcutLabelForAction(blueprint: ActionBlueprint): string | undefined {
  return blueprint.localShortcutActionId ? sessionLocalShortcutLabel(blueprint.localShortcutActionId) : undefined;
}

function labelKeyForAction(
  blueprint: ActionBlueprint,
  _context: SessionToolboxContext,
  _disabledReasonKey: string | undefined,
): string {
  return `sessions.toolbox.actions.${blueprint.id}.label`;
}

export function createSessionToolboxActionRequest(
  input: CreateSessionToolboxActionRequestInput,
): SessionToolboxActionRequest | null {
  const sessionId = input.context.sessionId;
  if (!sessionId) {
    return null;
  }

  const action = resolveSessionToolboxActions(input.context).find((item) => item.id === input.actionId);
  if (!action || action.status === "disabled") {
    return null;
  }

  if (action.kind === "remoteMessage") {
    const text = input.text ?? "";
    return remoteMessageTextError(text) === null ? { sessionId, actionId: input.actionId, kind: "remoteMessage", text } : null;
  }

  if (action.kind === "shortcut") {
    return action.dispatches?.length ? {
      sessionId,
      actionId: input.actionId,
      kind: "shortcut",
      dispatches: action.dispatches,
    } : null;
  }

  if (action.kind === "ui" || action.kind === "wakeDisplay" || action.kind === "diagnostics") {
    return {
      sessionId,
      actionId: input.actionId,
      kind: action.kind,
    };
  }

  return null;
}

export function snapSessionToolboxButton(input: SessionToolboxButtonSnapInput): SessionToolboxButtonPosition {
  const minX = input.inset;
  const minY = input.inset;
  const maxX = Math.max(minX, input.workspaceWidth - input.buttonWidth - input.inset);
  const maxY = Math.max(minY, input.workspaceHeight - input.buttonHeight - input.inset);
  let x = clamp(input.x, minX, maxX);
  let y = clamp(input.y, minY, maxY);
  let corner: SessionToolboxButtonCorner = null;

  const corners: Array<{ corner: Exclude<SessionToolboxButtonCorner, null>; x: number; y: number }> = [
    { corner: "top-left", x: minX, y: minY },
    { corner: "top-right", x: maxX, y: minY },
    { corner: "bottom-left", x: minX, y: maxY },
    { corner: "bottom-right", x: maxX, y: maxY },
  ];

  for (const candidate of corners) {
    const distance = Math.hypot(x - candidate.x, y - candidate.y);
    if (distance <= input.snapDistance) {
      x = candidate.x;
      y = candidate.y;
      corner = candidate.corner;
      break;
    }
  }

  return { x, y, corner };
}

export function resolveVisibleSessionToolboxButton(input: SessionToolboxVisibleButtonInput): SessionToolboxButtonPosition {
  return snapSessionToolboxButton({
    x: input.position.x,
    y: input.position.y,
    workspaceWidth: input.workspaceWidth,
    workspaceHeight: input.workspaceHeight,
    buttonWidth: input.buttonWidth,
    buttonHeight: input.buttonHeight,
    inset: input.inset,
    snapDistance: input.snapDistance,
  });
}

function disabledReasonForAction(blueprint: ActionBlueprint, context: SessionToolboxContext): string | undefined {
  if (!context.sessionId) {
    return "sessions.toolbox.disabled.noSession";
  }

  const connected = context.state ? CONNECTED_STATES.has(context.state) : false;
  const recoverable = context.state ? RECOVERABLE_STATES.has(context.state) : false;

  const dockingExternalWindow = blueprint.id === "toggle-external-window" && context.externalWindow;
  if (blueprint.requiresConnected && !connected && !dockingExternalWindow) {
    return "sessions.toolbox.disabled.disconnected";
  }

  if (blueprint.requiresRecoverable && !recoverable) {
    return "sessions.toolbox.disabled.notRecoverable";
  }

  if (blueprint.requiresControl && context.readonly) {
    return context.viewOnlyShared
      ? "sessions.toolbox.disabled.viewOnlyShared"
      : "sessions.toolbox.disabled.readonly";
  }

  if (blueprint.id === "toggle-readonly" && context.viewOnlyShared) {
    return "sessions.toolbox.disabled.viewOnlyShared";
  }

  if (
    blueprint.requiresCapabilityAction
    && !sessionSupportsAction(context.capabilities, blueprint.requiresCapabilityAction)
  ) {
    return blueprint.id === "send-message"
      ? "sessions.toolbox.disabled.messageUnsupported"
      : "sessions.toolbox.disabled.enhancedCapability";
  }

  return undefined;
}

function modifierShortcut(modifier: "Control" | "Alt" | "Command" | "Win", key: string, label: string): ShortcutDefinition {
  const modifierDown = remoteKey(modifier, SPECIAL_KEYSYMS[modifier]);
  const printable = SPECIAL_KEYSYMS[key] ? remoteKey(key, SPECIAL_KEYSYMS[key]) : remotePrintableKey(key);
  return {
    label,
    dispatches: [
      { ...modifierDown, down: true },
      { ...printable, down: true },
      { ...printable, down: false },
      { ...modifierDown, down: false },
    ],
  };
}

function multiKeyShortcut(keys: string[], label: string): ShortcutDefinition {
  const dispatches: RemoteKeyDispatch[] = [];
  const remoteKeys = keys.map((key) => remoteKey(key, SPECIAL_KEYSYMS[key]));

  for (const key of remoteKeys) {
    dispatches.push({ ...key, down: true });
  }
  for (const key of [...remoteKeys].reverse()) {
    dispatches.push({ ...key, down: false });
  }

  return { label, dispatches };
}

function windowsSecureAttentionShortcut(): ShortcutDefinition {
  return multiKeyShortcut(["Control", "Alt", "Delete"], "Ctrl+Alt+Del");
}

function windowsWindowSwitchShortcut(): ShortcutDefinition {
  return modifierShortcut("Alt", "Tab", "Alt+Tab");
}

function windowsRunShortcut(): ShortcutDefinition {
  return modifierShortcut("Win", "r", "Win+R");
}

function windowsTaskManagerShortcut(): ShortcutDefinition {
  return multiKeyShortcut(["Control", "Shift", "Escape"], "Ctrl+Shift+Esc");
}

function macosForceQuitShortcut(): ShortcutDefinition {
  return multiKeyShortcut(["Command", "Alt", "Escape"], "Command+Option+Esc");
}

function macosWindowSwitchShortcut(): ShortcutDefinition {
  return modifierShortcut("Command", "Tab", "Command+Tab");
}

function macosNextDesktopShortcut(): ShortcutDefinition {
  return modifierShortcut("Control", "ArrowRight", "Control+ArrowRight");
}

function macosPreviousDesktopShortcut(): ShortcutDefinition {
  return modifierShortcut("Control", "ArrowLeft", "Control+ArrowLeft");
}

function macosMissionControlShortcut(): ShortcutDefinition {
  return modifierShortcut("Control", "ArrowUp", "Control+ArrowUp");
}

function remotePrintableKey(key: string): Omit<RemoteKeyDispatch, "down"> {
  return {
    keysym: key.codePointAt(0) ?? 0,
    code: `Key${key.toUpperCase()}`,
  };
}

function remoteKey(key: string, keysym: number): Omit<RemoteKeyDispatch, "down"> {
  return { keysym, code: SPECIAL_KEY_CODES[key] ?? key };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
