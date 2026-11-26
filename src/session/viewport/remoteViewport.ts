import type { RemoteSessionState, RemoteSessionTab, RemoteSessionTransport } from "../core/sessionManager.ts";
import type { ConnectionIssue } from "../diagnostics/sessionDiagnostics.ts";
import type { SessionConnectionModel } from "../connection-model/connectionModel.ts";

export type RemoteViewportTone = "neutral" | "working" | "connected" | "failed" | "closed";
export type RemoteViewportInputMode = "control" | "readonly" | "locked";
export type RemoteViewportInputPolicy = "control" | "clientReadonly" | "viewOnlyShared" | "locked";

export type RemoteViewportSession = {
  id: string;
  target: string;
  state: RemoteSessionState;
  connectionModel?: SessionConnectionModel;
  displaySource?: "vnc";
  inputSource?: "vnc";
  transport?: RemoteSessionTransport;
  connectedAt?: string;
  lastError?: string;
  lastIssue?: ConnectionIssue;
  fallbackError?: string;
  readonly?: boolean;
  viewOnlyShared?: boolean;
};

export type RemoteViewportSummary = {
  id: string;
  title: string;
  target: string;
  state: RemoteSessionState;
  statusLabel: string;
  connectionModel: SessionConnectionModel;
  displaySource: "vnc";
  inputSource: "vnc";
  fallbackError?: string;
  tone: RemoteViewportTone;
  inputMode: RemoteViewportInputMode;
  inputPolicy: RemoteViewportInputPolicy;
  detail: string;
  transportLabel: string;
  canRetry: boolean;
  canDisconnect: boolean;
  canClose: boolean;
  canFullscreen: boolean;
  canToggleInputMode: boolean;
};

const FAILED_STATES = new Set<RemoteSessionState>([
  "authentication failed",
  "security failure",
  "disconnected unexpectedly",
  "failed",
]);

const WORKING_STATES = new Set<RemoteSessionState>([
  "probing",
  "connecting",
]);

export function summarizeRemoteViewport(
  tab: RemoteSessionTab,
  session?: RemoteViewportSession,
): RemoteViewportSummary {
  const state = session?.state ?? tab.state;
  const statusLabel = viewportStatusLabel(state, session?.lastIssue);
  const isFailed = FAILED_STATES.has(state);
  const isWorking = WORKING_STATES.has(state);
  const isConnected = state === "connected";
  const readonly = Boolean(session?.readonly);
  const viewOnlyShared = readonly && Boolean(session?.viewOnlyShared);
  const canControlInput = isConnected && tab.active && !readonly;
  const inputMode = readonly && isConnected ? "readonly" : canControlInput ? "control" : "locked";
  const inputPolicy = viewOnlyShared && isConnected
    ? "viewOnlyShared"
    : readonly && isConnected
      ? "clientReadonly"
      : canControlInput
        ? "control"
        : "locked";
  const canDisconnect = isConnected || isWorking;

  return {
    id: tab.id,
    title: tab.label,
    target: tab.target,
    state,
    statusLabel,
    connectionModel: session?.connectionModel ?? {
      baseTransport: { kind: "vnc" },
      serverIdentity: { kind: "unknown", verification: "none" },
      localEndpointMatch: { state: "not-matched" },
      enhancedChannel: { status: "unavailable", authentication: "not-attempted" },
      capabilities: { items: [] },
    },
    displaySource: session?.displaySource ?? "vnc",
    inputSource: session?.inputSource ?? "vnc",
    fallbackError: session?.fallbackError,
    tone: viewportTone(state),
    inputMode,
    inputPolicy,
    detail: viewportDetail(state, session),
    transportLabel: session?.transport?.id ?? "",
    canRetry: isFailed || state === "disconnected",
    canDisconnect,
    canClose: tab.closable,
    canFullscreen: isConnected,
    canToggleInputMode: isConnected && !viewOnlyShared,
  };
}

function viewportStatusLabel(state: RemoteSessionState, issue?: ConnectionIssue): string {
  if (!issue) {
    return state;
  }

  return issue.code.replace(/_/g, " ");
}

function viewportTone(state: RemoteSessionState): RemoteViewportTone {
  if (state === "connected") {
    return "connected";
  }

  if (FAILED_STATES.has(state)) {
    return "failed";
  }

  if (WORKING_STATES.has(state)) {
    return "working";
  }

  if (state === "closed") {
    return "closed";
  }

  return "neutral";
}

function viewportDetail(state: RemoteSessionState, session?: RemoteViewportSession): string {
  if (session?.lastError) {
    return session.lastError;
  }

  if (state === "connected") {
    return session?.transport ? `transport=${session.transport.id}` : "connected";
  }

  if (state === "idle") {
    return "waiting";
  }

  return state;
}
