import { eventsForSession, type EventLogEntry } from "../../diagnostics/eventLog.ts";
import type { LocalVncServerOverview } from "@server/localShare.ts";
import type { RemoteViewportInputMode, RemoteViewportSummary, RemoteViewportTone } from "../viewport/remoteViewport.ts";
import {
  emptyViewportRuntimeEvidenceSummary,
  summarizeViewportRuntimeEvidence,
  type ViewportRuntimeEvidence,
  type ViewportRuntimeEvidenceSummary,
} from "./runtimeEvidence.ts";
import type { RemoteSessionSnapshot } from "../protocol/remoteSessionCore.ts";
import type { SessionWorkspaceRenderState } from "../viewport/sessionWorkspace.ts";
import type {
  SessionClipboardActivity,
  SessionClipboardActivityChannel,
  SessionClipboardState,
} from "../clipboard/sessionClipboardActivity.ts";
import { summarizeAppServerDiagnostics } from "./sessionDiagnosticsPanel/appServer.ts";
import { formatTimestamp, formatTimeOfDay } from "./sessionDiagnosticsPanel/time.ts";
import {
  formatClipboardCopyLine,
  summarizeClipboard,
} from "./sessionDiagnosticsPanel/clipboard.ts";
import {
  summarizeTransportLayers,
  summarizeTransportLayerTexts,
} from "./sessionDiagnosticsPanel/transport.ts";

export type DiagnosticsEventScope = "selected" | "global";
export type SessionDiagnosticsRenderState = SessionWorkspaceRenderState | "none";
export type SessionDiagnosticsInputSource = "main" | "external" | "none";
export type SessionDiagnosticsInputType = "pointer" | "wheel" | "keyboard" | "none";

export type SessionDiagnosticsInputActivity = {
  source: Exclude<SessionDiagnosticsInputSource, "none">;
  type: Exclude<SessionDiagnosticsInputType, "none">;
  atMs: number;
};

export type SessionDiagnosticsInputActivitySummary = {
  source: SessionDiagnosticsInputSource;
  type: SessionDiagnosticsInputType;
  at: string;
  atText: string;
};

export type SessionDiagnosticsClipboardActivitySummary = {
  direction: SessionClipboardActivity["direction"];
  at: string;
  atText: string;
  accepted: boolean;
  reason: string;
  statusText: string;
  mode: string;
  bytes: string;
  source?: SessionClipboardActivity["source"];
  sourceText?: string;
  channel: SessionClipboardActivityChannel;
  channelText: string;
};

export type SessionDiagnosticsClipboardSummary = {
  mode: string;
  enabled: boolean;
  readonly: boolean;
  clientToServer: SessionDiagnosticsClipboardActivitySummary | null;
  serverToClient: SessionDiagnosticsClipboardActivitySummary | null;
};

export type SessionDiagnosticsRemoteSessionCoreSummary = {
  sessionId: string;
  state: string;
  transportKind: string;
  baseTransport: string;
  enhancedChannel: string;
  displaySource: string;
  inputSource: string;
  activeSurfaceId: string;
  activeSurfaceOwner: string;
  activeSurfaceWindow: string;
  surfaceCount: string;
};

export type SessionDiagnosticsIssueLayer = "none" | "base" | "enhanced";

export type SessionDiagnosticsTransportLayerSummary = {
  base: string;
  enhanced: string;
  displaySource: "vnc_framebuffer" | "vnc_framebuffer_fallback" | "-";
  inputPath: "vnc_input" | "-";
  issueLayer: SessionDiagnosticsIssueLayer | "-";
};

export type SessionDiagnosticsTransportLayerTextSummary = {
  base: string;
  enhanced: string;
  displaySource: string;
  inputPath: string;
  issueLayer: string;
};

export type SessionAppServerTargetType = "local_app_server" | "remote_app_server";
export type SessionAppServerConfidence = "high" | "medium";
export type SessionAppServerEvidence = "local_server_view" | "desktop_name" | "agent_sidecar";
export type SessionAppServerCapabilityBoundary = "local_server_reported" | "rfb_inferred" | "agent_transport";
export type SessionAppServerDiagnosticsSummary = {
  targetType: SessionAppServerTargetType;
  label: string;
  targetText: string;
  confidence: SessionAppServerConfidence;
  confidenceText: string;
  evidence: SessionAppServerEvidence;
  evidenceText: string;
  endpoint: string;
  agent: string;
  agentText: string;
  display: string;
  framebuffer: string;
  modes: string;
  permissions: string;
  frameDiagnostics?: string;
  inputStatus: "input_ready" | "received_but_unauthorized" | "view_only" | "waiting_for_input";
  inputStatusText: string;
  capabilityBoundary: SessionAppServerCapabilityBoundary;
  capabilityBoundaryText: string;
};

export type SessionDiagnosticsPanelSummary = {
  selectedTarget: string;
  selectedTitle: string;
  state: string;
  tone: RemoteViewportTone;
  inputMode: RemoteViewportInputMode | "viewOnlyShared";
  detail: string;
  transportLabel: string;
  fallbackError: string;
  renderState: SessionDiagnosticsRenderState;
  externalWindowOwned: boolean;
  latestInput: SessionDiagnosticsInputActivitySummary;
  clipboard: SessionDiagnosticsClipboardSummary;
  remoteSessionCore?: SessionDiagnosticsRemoteSessionCoreSummary;
  runtimeEvidence: ViewportRuntimeEvidenceSummary;
  transportLayers: SessionDiagnosticsTransportLayerSummary;
  transportLayerTexts: SessionDiagnosticsTransportLayerTextSummary;
  appServer?: SessionAppServerDiagnosticsSummary;
  copyText: string;
  eventScope: DiagnosticsEventScope;
  events: EventLogEntry[];
};

export type SessionDiagnosticsPanelInput = {
  viewport: RemoteViewportSummary | null | undefined;
  events: EventLogEntry[];
  runtimeEvidence?: ViewportRuntimeEvidence | null;
  renderState?: SessionWorkspaceRenderState;
  externalWindowOwned?: boolean;
  latestInput?: SessionDiagnosticsInputActivity | null;
  clipboard?: SessionClipboardState | null;
  remoteSessionCore?: RemoteSessionSnapshot | null;
  localServer?: LocalVncServerOverview | null;
  eventLimit?: number;
};

export function summarizeSessionDiagnosticsPanel(
  input: SessionDiagnosticsPanelInput,
): SessionDiagnosticsPanelSummary {
  const eventLimit = input.eventLimit ?? 12;
  const selectedEvents = input.viewport
    ? eventsForSession(input.events, input.viewport.id)
    : [];
  const scopedEvents = input.viewport ? selectedEvents : input.events;

  const latestInput = summarizeInputActivity(input.viewport ? input.latestInput : null);
  const runtimeEvidence = input.viewport
    ? summarizeViewportRuntimeEvidence(input.runtimeEvidence)
    : emptyViewportRuntimeEvidenceSummary();
  const appServer = input.viewport
    ? summarizeAppServerDiagnostics(input.localServer, runtimeEvidence, latestInput, input.viewport)
    : undefined;
  const transportLayers = summarizeTransportLayers(input.viewport);

  const summary: SessionDiagnosticsPanelSummary = {
    selectedTarget: input.viewport?.target ?? "",
    selectedTitle: input.viewport?.title ?? "",
    state: input.viewport?.statusLabel ?? "idle",
    tone: input.viewport?.tone ?? "neutral",
    inputMode: input.viewport?.inputPolicy === "viewOnlyShared"
      ? "viewOnlyShared"
      : input.viewport?.inputMode ?? "locked",
    detail: input.viewport?.detail ?? "",
    transportLabel: input.viewport?.transportLabel ?? "",
    fallbackError: input.viewport?.fallbackError ?? "",
    renderState: input.viewport ? input.renderState ?? "none" : "none",
    externalWindowOwned: input.viewport ? Boolean(input.externalWindowOwned) : false,
    latestInput,
    clipboard: summarizeClipboard(input.viewport ? input.clipboard : null),
    remoteSessionCore: input.viewport ? summarizeRemoteSessionCore(input.remoteSessionCore) : undefined,
    runtimeEvidence,
    transportLayers,
    transportLayerTexts: summarizeTransportLayerTexts(transportLayers),
    appServer,
    copyText: "",
    eventScope: input.viewport ? "selected" : "global",
    events: scopedEvents.slice(0, eventLimit),
  };
  summary.copyText = formatSessionDiagnosticsPanelCopyText(summary);
  return summary;
}

function summarizeRemoteSessionCore(
  snapshot: RemoteSessionSnapshot | null | undefined,
): SessionDiagnosticsRemoteSessionCoreSummary | undefined {
  if (!snapshot) {
    return undefined;
  }

  const activeSurface = snapshot.surfaces.find((surface) => surface.surfaceId === snapshot.activeSurfaceId)
    ?? snapshot.surfaces.find((surface) => surface.active);

  return {
    sessionId: snapshot.sessionId,
    state: snapshot.state,
    transportKind: snapshot.connectionModel.serverIdentity.kind,
    baseTransport: snapshot.connectionModel.baseTransport.kind,
    enhancedChannel: formatCoreEnhancedChannel(snapshot.connectionModel.enhancedChannel, snapshot.connectionModel.capabilities.items),
    displaySource: snapshot.displaySource,
    inputSource: snapshot.inputSource,
    activeSurfaceId: snapshot.activeSurfaceId ?? "-",
    activeSurfaceOwner: activeSurface?.owner ?? "-",
    activeSurfaceWindow: activeSurface?.windowLabel ?? "-",
    surfaceCount: String(snapshot.surfaces.length),
  };
}

function formatCoreEnhancedChannel(
  channel: RemoteSessionSnapshot["connectionModel"]["enhancedChannel"],
  capabilityItems: RemoteSessionSnapshot["connectionModel"]["capabilities"]["items"],
): string {
  const port = channel.endpoint?.port === undefined ? "" : `:${channel.endpoint.port}`;
  const capabilities = capabilityItems.filter((item) => item.source === "agent" && item.state === "supported").map((item) => item.id);
  const capabilitySummary = capabilities.length
    ? ` capabilities=${capabilities.join(",")}`
    : " capabilities=-";
  const error = channel.lastErrorCode ? ` error=${channel.lastErrorCode}` : "";
  return `${channel.status}${port}${capabilitySummary}${error}`;
}

function summarizeInputActivity(
  activity: SessionDiagnosticsInputActivity | null | undefined,
): SessionDiagnosticsInputActivitySummary {
  if (!activity) {
    return {
      source: "none",
      type: "none",
      at: "-",
      atText: "-",
    };
  }

  return {
    source: activity.source,
    type: activity.type,
    at: formatTimestamp(activity.atMs),
    atText: formatTimeOfDay(activity.atMs),
  };
}

export function formatSessionDiagnosticsPanelCopyText(
  summary: Omit<SessionDiagnosticsPanelSummary, "copyText">,
): string {
  const lines = [
    `Selected Target: ${summary.selectedTarget || "-"}`,
    `State: ${summary.state}`,
    `Input Mode: ${summary.inputMode}`,
    `Base Transport: ${summary.transportLayers.base}`,
    `Enhanced Channel: ${summary.transportLayers.enhanced}`,
    `Display Source: ${summary.transportLayers.displaySource}`,
    `Input Path: ${summary.transportLayers.inputPath}`,
    `Issue Layer: ${summary.transportLayers.issueLayer}`,
    `Render State: ${summary.renderState}`,
    `Latest Input: ${summary.latestInput.source}/${summary.latestInput.type} at ${summary.latestInput.at}`,
    `Clipboard Mode: ${summary.clipboard.mode}`,
    `Clipboard Enabled: ${summary.clipboard.enabled}`,
    `Clipboard Client -> Server: ${formatClipboardCopyLine(summary.clipboard.clientToServer)}`,
    `Clipboard Server -> Client: ${formatClipboardCopyLine(summary.clipboard.serverToClient)}`,
    `Runtime Evidence: ${summary.runtimeEvidence.health}`,
    `Evidence Captured At: ${summary.runtimeEvidence.capturedAt}`,
    `Framebuffer: ${summary.runtimeEvidence.framebuffer}`,
    `Core Transport: ${summary.runtimeEvidence.transportId}`,
    `Core Surface: ${summary.runtimeEvidence.surface}`,
  ];

  if (summary.remoteSessionCore) {
    lines.push(
      `Core Session: ${summary.remoteSessionCore.sessionId}`,
      `Core State: ${summary.remoteSessionCore.state}`,
      `Core Transport: ${summary.remoteSessionCore.transportKind}`,
      `Core Base Transport: ${summary.remoteSessionCore.baseTransport}`,
      `Core Enhanced Channel: ${summary.remoteSessionCore.enhancedChannel}`,
      `Core Display Source: ${summary.remoteSessionCore.displaySource}`,
      `Core Input Source: ${summary.remoteSessionCore.inputSource}`,
      `Core Active Surface: ${summary.remoteSessionCore.activeSurfaceOwner}/${summary.remoteSessionCore.activeSurfaceWindow}`,
      `Core Surface Count: ${summary.remoteSessionCore.surfaceCount}`,
    );
  }

  if (summary.appServer) {
    lines.push(
      `App Server: ${summary.appServer.label}`,
      `App Server Confidence: ${summary.appServer.confidence}`,
      `App Server Evidence: ${summary.appServer.evidence}`,
      `App Server Endpoint: ${summary.appServer.endpoint}`,
      `App Server Agent: ${summary.appServer.agent}`,
      `App Server Display: ${summary.appServer.display}`,
      `App Server Framebuffer: ${summary.appServer.framebuffer}`,
      `App Server Modes: ${summary.appServer.modes}`,
      `App Server Permissions: ${summary.appServer.permissions}`,
      `App Server Capability Boundary: ${summary.appServer.capabilityBoundary}`,
      `Input Status: ${summary.appServer.inputStatus}`,
    );
  }

  return lines.join("\n");
}
