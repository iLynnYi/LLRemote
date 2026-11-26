import type { RemoteViewportSummary } from "../../viewport/remoteViewport.ts";
import type {
  SessionDiagnosticsIssueLayer,
  SessionDiagnosticsTransportLayerSummary,
  SessionDiagnosticsTransportLayerTextSummary,
} from "../sessionDiagnosticsPanel.ts";

export function summarizeTransportLayerTexts(
  layers: SessionDiagnosticsTransportLayerSummary,
): SessionDiagnosticsTransportLayerTextSummary {
  return {
    base: baseTransportText(layers.base),
    enhanced: enhancedChannelText(layers.enhanced),
    displaySource: displaySourceText(layers.displaySource),
    inputPath: inputPathText(layers.inputPath),
    issueLayer: issueLayerText(layers.issueLayer),
  };
}

export function summarizeTransportLayers(
  viewport: RemoteViewportSummary | null | undefined,
): SessionDiagnosticsTransportLayerSummary {
  if (!viewport) {
    return {
      base: "-",
      enhanced: "-",
      displaySource: "-",
      inputPath: "-",
      issueLayer: "-",
    };
  }

  const baseKind = viewport.connectionModel.baseTransport.kind;
  const baseState = baseTransportState(viewport.state);
  const enhancedChannel = viewport.connectionModel.enhancedChannel;
  const enhanced = summarizeEnhancedChannel(enhancedChannel);
  const displaySource = displaySourceForViewport(viewport);
  const inputPath = inputPathForViewport();
  const issueLayer = issueLayerForViewport(viewport, baseState);

  return {
    base: `${baseKind}:${baseState}`,
    enhanced,
    displaySource,
    inputPath,
    issueLayer,
  };
}

export function summarizeEnhancedChannel(channel: RemoteViewportSummary["connectionModel"]["enhancedChannel"]): string {
  const parts: string[] = [channel.status];
  if (channel.endpoint?.port !== undefined) {
    parts[0] = `${channel.status}:${channel.endpoint.port}`;
  }
  if (channel.lastErrorCode) {
    parts.push(`error=${channel.lastErrorCode}`);
  }
  return parts.join(" ");
}

function baseTransportText(value: string): string {
  if (value === "-") {
    return "-";
  }

  const [kind, state] = value.split(":");
  const kindText = kind === "vnc" ? "标准 VNC" : kind;
  const stateText = {
    connected: "已连接",
    connecting: "连接中",
    failed: "连接失败",
    disconnected: "已断开",
  }[state] ?? state;
  return `${kindText} ${stateText}`.trim();
}

function enhancedChannelText(value: string): string {
  if (value === "-") {
    return "-";
  }

  const status = value.split(/[:\s]/, 1)[0];
  switch (status) {
    case "available":
      return "可用";
    case "degraded":
      return "部分不可用";
    case "probing":
      return "检测中";
    case "unavailable":
      return "不可用";
    default:
      return value;
  }
}

function displaySourceText(value: SessionDiagnosticsTransportLayerSummary["displaySource"]): string {
  switch (value) {
    case "vnc_framebuffer":
      return "标准 VNC 画面";
    case "vnc_framebuffer_fallback":
      return "标准 VNC 画面（Agent 已回退）";
    case "-":
      return "-";
  }
}

function inputPathText(value: SessionDiagnosticsTransportLayerSummary["inputPath"]): string {
  switch (value) {
    case "vnc_input":
      return "标准 VNC 鼠标键盘";
    case "-":
      return "-";
  }
}

function issueLayerText(value: SessionDiagnosticsTransportLayerSummary["issueLayer"]): string {
  switch (value) {
    case "none":
      return "暂无异常";
    case "base":
      return "基础连接异常";
    case "enhanced":
      return "增强能力异常";
    case "-":
      return "-";
  }
}

function baseTransportState(state: RemoteViewportSummary["state"]): "connected" | "connecting" | "failed" | "disconnected" {
  if (state === "connected") {
    return "connected";
  }

  if (state === "connecting" || state === "probing") {
    return "connecting";
  }

  if (state === "failed" || state === "authentication failed" || state === "security failure" || state === "disconnected unexpectedly") {
    return "failed";
  }

  return "disconnected";
}

function displaySourceForViewport(
  viewport: RemoteViewportSummary,
): SessionDiagnosticsTransportLayerSummary["displaySource"] {
  if (viewport.fallbackError && viewport.connectionModel.serverIdentity.kind === "managed") {
    return "vnc_framebuffer_fallback";
  }
  return "vnc_framebuffer";
}

function inputPathForViewport(): SessionDiagnosticsTransportLayerSummary["inputPath"] {
  return "vnc_input";
}

function issueLayerForViewport(
  viewport: RemoteViewportSummary,
  baseState: ReturnType<typeof baseTransportState>,
): SessionDiagnosticsIssueLayer {
  if (baseState === "failed") {
    return "base";
  }

  const enhancedChannel = viewport.connectionModel.enhancedChannel;
  if (viewport.fallbackError || enhancedChannel.status === "degraded" || Boolean(enhancedChannel.lastErrorCode)) {
    return "enhanced";
  }

  return "none";
}
