import type { Event } from "@tauri-apps/api/event";
import { keyboardEventToRemoteKeyEvent, type RemoteKeyEvent } from "../../../session/input/keyboardMapping.ts";
import {
  mapExternalPointerToFramebuffer,
  sessionExternalInputEvent,
  toExternalMouseButtonMask,
  toExternalMouseButtonsMask,
  type ExternalFrameRect,
  type SessionExternalInputKeyboardPayload,
  type SessionExternalInputPayload,
  type SessionExternalInputPointerPayload,
  type SessionExternalInputWheelPayload,
  type SessionExternalPointerAction,
} from "../../../session/external-window/sessionExternalInput.ts";
import type { ProtocolConnection } from "../../../session/protocol/protocolAdapter.ts";
import { sessionSupportsAction } from "../../../session/transfers/sessionCapabilities.ts";

type EmitExternalInput = (
  eventName: string,
  payload: SessionExternalInputPayload,
) => Promise<void> | void;

type ListenExternalInput = (
  eventName: string,
  handler: (event: Pick<Event<SessionExternalInputPayload>, "payload">) => void,
) => Promise<() => void>;

export type ExternalKeyboardInputOptions = {
  sessionId: string;
  event: KeyboardEvent;
  down: boolean;
  rightControlAsCommand?: boolean;
  mapKey?: (event: KeyboardEvent, down: boolean) => RemoteKeyEvent | null;
};

export type ExternalPointerInputOptions = {
  sessionId: string;
  action: SessionExternalPointerAction;
  clientX: number;
  clientY: number;
  button: number;
  buttons: number;
  frameRect: ExternalFrameRect;
  framebufferWidth: number;
  framebufferHeight: number;
};

export type ExternalWheelInputOptions = {
  sessionId: string;
  clientX: number;
  clientY: number;
  buttons: number;
  deltaX: number;
  deltaY: number;
  frameRect: ExternalFrameRect;
  framebufferWidth: number;
  framebufferHeight: number;
};

export type SessionExternalInputForwarderOptions = {
  sessionId: string;
  windowInstanceId?: string;
  listen: ListenExternalInput;
  getConnection: (sessionId: string) => ProtocolConnection | undefined;
  recordInput?: (sessionId: string, type: "pointer" | "wheel" | "keyboard") => void;
  activateInput?: (sessionId: string) => void;
  onRemoteDesktopShortcutUnavailable?: (sessionId: string, actionId: string) => void;
};

export function createExternalKeyboardInputPayload(
  options: ExternalKeyboardInputOptions,
): SessionExternalInputKeyboardPayload | null {
  const remoteKey = options.mapKey ? options.mapKey(options.event, options.down) : keyboardEventToRemoteKeyEvent(options.event, options);

  if (!remoteKey) {
    return null;
  }

  return {
    sessionId: options.sessionId,
    type: "keyboard",
    key: options.event.key,
    code: remoteKey.code,
    down: options.down,
    keysym: remoteKey.keysym,
  };
}

export function createExternalPointerInputPayload(
  options: ExternalPointerInputOptions,
): SessionExternalInputPointerPayload {
  const point = mapExternalPointerToFramebuffer(options);
  const currentButtonsMask = toExternalMouseButtonsMask(options.buttons);
  const buttonMask = options.action === "down" && currentButtonsMask === 0
    ? toExternalMouseButtonMask(options.button)
    : currentButtonsMask;

  return {
    sessionId: options.sessionId,
    type: "pointer",
    action: options.action,
    x: point.x,
    y: point.y,
    buttonMask,
  };
}

export function createExternalWheelInputPayload(
  options: ExternalWheelInputOptions,
): SessionExternalInputWheelPayload {
  const point = mapExternalPointerToFramebuffer(options);

  return {
    sessionId: options.sessionId,
    type: "wheel",
    x: point.x,
    y: point.y,
    deltaX: options.deltaX,
    deltaY: options.deltaY,
    buttonMask: toExternalMouseButtonsMask(options.buttons),
  };
}

export async function emitExternalInput(
  emit: EmitExternalInput,
  payload: SessionExternalInputPayload,
) {
  await emit(sessionExternalInputEvent(payload.sessionId), payload);
}

export async function startSessionExternalInputForwarder(
  options: SessionExternalInputForwarderOptions,
): Promise<() => void> {
  let disposed = false;
  let inputOwner: ProtocolConnection | null = null;
  const heldKeys = new Map<number, SessionExternalInputKeyboardPayload>();
  let heldPointer: { x: number; y: number } | null = null;

  // Capture the owner before clearing state; an old window must never release
  // its pressed input through a replacement connection with the same ID.
  function releaseHeldInput() {
    const owner = inputOwner;
    const keys = [...heldKeys.values()];
    const pointer = heldPointer;
    inputOwner = null;
    heldKeys.clear();
    heldPointer = null;
    if (!owner) return;
    for (const key of keys) owner.forwardExternalInput({ ...key, down: false });
    if (pointer) owner.forwardExternalInput({
      sessionId: options.sessionId, type: "pointer", action: "up", ...pointer, buttonMask: 0,
    });
  }

  const unlisten = await options.listen(sessionExternalInputEvent(options.sessionId), (event) => {
    const payload = event.payload;
    if (disposed || payload.sessionId !== options.sessionId
      || (options.windowInstanceId !== undefined && payload.windowInstanceId !== options.windowInstanceId)) {
      return;
    }

    if (payload.type === "releaseInput") {
      releaseHeldInput();
      return;
    }

    const connection = options.getConnection(options.sessionId);
    if (!connection) {
      return;
    }
    if (inputOwner && inputOwner !== connection) releaseHeldInput();
    // Hover and background scrolling must not take ownership from the focused window.
    if ((payload.type === "keyboard" && payload.down)
      || (payload.type === "pointer" && payload.action === "down")
      || payload.type === "remoteDesktopShortcut") {
      options.activateInput?.(options.sessionId);
    }

    if (payload.type === "remoteDesktopShortcut") {
      if (sessionSupportsAction(connection.connectionModel.capabilities, "controlRemoteDesktop") && connection.controlRemoteDesktop) {
        void connection.controlRemoteDesktop(payload.actionId).catch(() => undefined);
      } else {
        options.onRemoteDesktopShortcutUnavailable?.(options.sessionId, payload.actionId);
      }
      return;
    }

    connection.forwardExternalInput(payload);
    inputOwner = connection;
    if (payload.type === "keyboard") {
      if (payload.down) heldKeys.set(payload.keysym, { ...payload });
      else heldKeys.delete(payload.keysym);
    } else {
      // Wheel payloads carry the physical buttons, before the Adapter adds its
      // temporary scroll bits and immediately releases those bits again.
      heldPointer = payload.buttonMask ? { x: payload.x, y: payload.y } : null;
    }
    options.recordInput?.(options.sessionId, payload.type);
  });
  return () => {
    if (disposed) return;
    disposed = true;
    try {
      releaseHeldInput();
    } finally {
      unlisten();
    }
  };
}
