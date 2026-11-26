import type { RemoteDesktopShortcutActionId } from "../input/sessionRemoteDesktopShortcuts.ts";

export type SessionExternalPointerAction = "move" | "down" | "up";

export type SessionExternalInputPointerPayload = {
  sessionId: string;
  type: "pointer";
  action: SessionExternalPointerAction;
  x: number;
  y: number;
  buttonMask: number;
};

export type SessionExternalInputWheelPayload = {
  sessionId: string;
  type: "wheel";
  x: number;
  y: number;
  deltaX: number;
  deltaY: number;
  buttonMask: number;
};

export type SessionExternalInputKeyboardPayload = {
  sessionId: string;
  type: "keyboard";
  key: string;
  code: string;
  down: boolean;
  keysym: number;
};

export type SessionExternalRemoteDesktopShortcutPayload = {
  sessionId: string;
  type: "remoteDesktopShortcut";
  actionId: RemoteDesktopShortcutActionId;
};

/** Releases only input held by this window's forwarder in the main window. */
export type SessionExternalReleaseInputPayload = {
  sessionId: string;
  type: "releaseInput";
};

export type SessionExternalWindowFeedbackTone = "info" | "success" | "warning" | "error";

export type SessionExternalWindowFeedbackPayload = {
  sessionId: string;
  tone: SessionExternalWindowFeedbackTone;
  message: string;
};

export type SessionExternalInputPayload =
  (SessionExternalInputPointerPayload
    | SessionExternalInputWheelPayload
    | SessionExternalInputKeyboardPayload
    | SessionExternalRemoteDesktopShortcutPayload
    | SessionExternalReleaseInputPayload) & { windowInstanceId?: string };

export type ExternalFrameRect = {
  left: number;
  top: number;
  width: number;
  height: number;
};

export type ExternalPointerMappingInput = {
  clientX: number;
  clientY: number;
  frameRect: ExternalFrameRect;
  framebufferWidth: number;
  framebufferHeight: number;
};

export function sessionExternalInputEvent(sessionId: string): string {
  return `session-window-input:${sessionId}`;
}

export function sessionExternalWindowFeedbackEvent(sessionId: string): string {
  return `session-window-feedback:${sessionId}`;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function mapExternalPointerToFramebuffer(input: ExternalPointerMappingInput): { x: number; y: number } {
  const framebufferWidth = Math.max(1, Math.round(input.framebufferWidth));
  const framebufferHeight = Math.max(1, Math.round(input.framebufferHeight));
  const frameWidth = Math.max(1, input.frameRect.width);
  const frameHeight = Math.max(1, input.frameRect.height);
  const relativeX = (input.clientX - input.frameRect.left) / frameWidth;
  const relativeY = (input.clientY - input.frameRect.top) / frameHeight;

  return {
    x: clamp(Math.round(relativeX * framebufferWidth), 0, framebufferWidth - 1),
    y: clamp(Math.round(relativeY * framebufferHeight), 0, framebufferHeight - 1),
  };
}

export function toExternalMouseButtonMask(button: number): number {
  if (button === 0) {
    return 1;
  }
  if (button === 1) {
    return 2;
  }
  if (button === 2) {
    return 4;
  }

  return 0;
}

export function toExternalMouseButtonsMask(buttons: number): number {
  let mask = 0;
  if (buttons & 1) {
    mask |= 1;
  }
  if (buttons & 2) {
    mask |= 4;
  }
  if (buttons & 4) {
    mask |= 2;
  }

  return mask;
}
