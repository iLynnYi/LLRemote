import type { SessionToolboxContext } from "../transfers/sessionToolbox.ts";
import type { RemoteViewportTone } from "../viewport/remoteViewport.ts";
import type { RemoteViewportStatusHudSummary } from "../viewport/remoteViewportStatusBar.ts";
import type { TransferTask } from "../transfers/fileTransferManager.ts";
import type { SessionDiagnosticView } from "../diagnostics/sessionDiagnosticView.ts";

export type SessionExternalTransferTaskSnapshot = Pick<TransferTask, "id" | "sessionId" | "target" | "fileName" | "state" | "transferredBytes" | "totalBytes" | "progress" | "speedBytesPerSecond" | "etaSeconds" | "errorMessage" | "remotePath">;
export type SessionExternalTransferDrawerSnapshot = { tasks: SessionExternalTransferTaskSnapshot[]; clearingHistory: boolean };
export type SessionExternalTransferDrawerControl = { sessionId: string; taskId?: string; taskIds?: string[]; action: "pause" | "resume" | "retry" | "cancel" | "pause-all" | "resume-all" | "clear-history" | "copy-path" | "open-directory" };

export type SessionExternalWindowLocation = {
  sessionId: string;
  windowInstanceId?: string;
  target: string;
  host: string;
  port: number;
  readonly: boolean;
};

export type SessionExternalWindowDockRequest = {
  sessionId: string;
  windowInstanceId: string;
};

export type SessionExternalWindowFocus = SessionExternalWindowDockRequest & { focused: boolean };

export function isSessionExternalWindowFocus(value: unknown): value is SessionExternalWindowFocus {
  return isSessionExternalWindowDockRequest(value)
    && "focused" in value && typeof value.focused === "boolean";
}

/** Accepts dock messages only when they identify a specific window attempt. */
export function isSessionExternalWindowDockRequest(value: unknown): value is SessionExternalWindowDockRequest {
  return typeof value === "object" && value !== null
    && "sessionId" in value && typeof value.sessionId === "string" && value.sessionId.trim().length > 0
    && "windowInstanceId" in value && typeof value.windowInstanceId === "string" && value.windowInstanceId.trim().length > 0;
}

export type SessionExternalWindowRequest = SessionExternalWindowLocation & {
  label: string;
  title: string;
  url: string;
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
  resizable: boolean;
  fullscreen: boolean;
};

export type SessionExternalWindowControlSnapshot = {
  sessionId: string;
  fileTransferAvailable: boolean;
  statusHud: RemoteViewportStatusHudSummary;
  viewportTone: RemoteViewportTone;
  toolboxContext: SessionToolboxContext;
};

export type SessionExternalWindowToolboxCommandPayload = {
  sessionId: string;
  key: string;
  text: string;
  requestId?: string;
  feedbackTarget?: "main" | "external";
  selectedFileTransferSourcePaths?: string[];
};

export type SessionExternalWindowToolboxCommandResultPayload = {
  sessionId: string;
  tone: "info" | "success" | "warning" | "error";
  message: string;
  requestId?: string;
  succeeded?: boolean;
  diagnosticView?: SessionDiagnosticView;
};

export function sessionExternalWindowLabel(sessionId: string): string {
  return `session-${sessionId}`;
}

export function sessionExternalWindowDockEvent(sessionId: string): string {
  return `session-window-dock:${sessionId}`;
}

export function sessionExternalWindowFocusEvent(sessionId: string): string {
  return `session-window-focus:${sessionId}`;
}

export function sessionExternalWindowToolboxCommandEvent(sessionId: string): string {
  return `session-window-toolbox-command:${sessionId}`;
}

export function sessionExternalWindowToolboxCommandResultEvent(sessionId: string): string {
  return `session-window-toolbox-command-result:${sessionId}`;
}

export function sessionExternalTransferDrawerRequestEvent(sessionId: string): string { return `session-window-transfer-drawer-request:${sessionId}`; }
export function sessionExternalTransferDrawerSnapshotEvent(sessionId: string): string { return `session-window-transfer-drawer-snapshot:${sessionId}`; }
export function sessionExternalTransferDrawerControlEvent(sessionId: string): string { return `session-window-transfer-drawer-control:${sessionId}`; }

export function createSessionExternalWindowRequest(
  location: SessionExternalWindowLocation,
  options: { fullscreen?: boolean } = {},
): SessionExternalWindowRequest {
  const params = new URLSearchParams({
    workspace: "session-window",
    sessionId: location.sessionId,
    target: location.target,
    host: location.host,
    port: String(location.port),
    readonly: String(location.readonly),
    fullscreen: String(options.fullscreen ?? false),
  });
  if (location.windowInstanceId !== undefined) {
    params.set("windowInstanceId", location.windowInstanceId);
  }

  return {
    ...location,
    label: sessionExternalWindowLabel(location.sessionId),
    title: `VNC Session ${location.target}`,
    url: `/?${params.toString()}`,
    width: 960,
    height: 640,
    minWidth: 320,
    minHeight: 240,
    resizable: true,
    fullscreen: options.fullscreen ?? false,
  };
}

export function isSessionExternalWindowLocation(href: string): SessionExternalWindowLocation | null {
  const url = new URL(href);

  if (url.searchParams.get("workspace") !== "session-window") {
    return null;
  }

  const sessionId = url.searchParams.get("sessionId");
  const target = url.searchParams.get("target");
  const host = url.searchParams.get("host");
  const port = Number(url.searchParams.get("port"));
  const readonly = url.searchParams.get("readonly") === "true";
  const windowInstanceId = url.searchParams.get("windowInstanceId");

  if (!sessionId || !target || !host || !Number.isInteger(port) || port < 1 || port > 65535) {
    return null;
  }

  return {
    sessionId, target, host, port, readonly,
    ...(windowInstanceId !== null ? { windowInstanceId } : {}),
  };
}
