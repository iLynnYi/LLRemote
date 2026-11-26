import type { LocalVncServerOverview, LocalVncServerCaptureStatus } from "@server/localShare.ts";
import type { RemoteViewportSummary } from "../../viewport/remoteViewport.ts";
import type { ViewportRuntimeEvidenceSummary } from "../runtimeEvidence.ts";
import type {
  SessionAppServerDiagnosticsSummary,
  SessionDiagnosticsInputActivitySummary,
} from "../sessionDiagnosticsPanel.ts";
import { summarizeEnhancedChannel } from "./transport.ts";

export function summarizeAppServerDiagnostics(
  overview: LocalVncServerOverview | null | undefined,
  runtimeEvidence: ViewportRuntimeEvidenceSummary,
  latestInput: SessionDiagnosticsInputActivitySummary,
  viewport: RemoteViewportSummary,
): SessionAppServerDiagnosticsSummary | undefined {
  if (overview?.status === "running") {
    const selectedDisplay = overview.displays.find((display) => display.id === overview.selectedDisplayId)
      ?? overview.displays[0];

    return {
      targetType: "local_app_server",
      label: "App Server · 本机",
      targetText: "本应用 Server（本机）",
      confidence: "high",
      confidenceText: "已确认",
      evidence: "local_server_view",
      evidenceText: "通过本机共享状态确认",
      endpoint: `${overview.bindAddress || "-"}:${overview.port || "-"}`,
      agent: overview.agent.status === "running" && overview.agent.port
        ? `running:${overview.agent.port}`
        : overview.agent.status,
      agentText: localAgentStatusText(overview.agent.status),
      display: selectedDisplay
        ? `${selectedDisplay.label} · ${formatSize(selectedDisplay.width, selectedDisplay.height)} · scale ${selectedDisplay.scaleFactor}`
        : "-",
      framebuffer: formatSize(overview.framebufferWidth, overview.framebufferHeight),
      modes: `${overview.resolutionMode} / ${overview.refreshMode}`,
      permissions: overview.permissions.map((item) => `${item.kind}=${item.status}`).join(", ") || "-",
      frameDiagnostics: formatFrameDiagnostics(overview.frameDiagnostics),
      inputStatus: appServerInputStatus(overview, latestInput),
      inputStatusText: inputStatusText(appServerInputStatus(overview, latestInput)),
      capabilityBoundary: "local_server_reported",
      capabilityBoundaryText: "来自本机共享状态",
    };
  }

  if (viewport.connectionModel.serverIdentity.verification === "authenticated-agent") {
    return {
      targetType: "remote_app_server",
      label: "App Server · Agent",
      targetText: "配套 Server（已认证 Agent 会话）",
      confidence: "high",
      confidenceText: "会话已确认",
      evidence: "agent_sidecar",
      evidenceText: "已认证 Agent 会话与本次 RFB 声明的 Server ID 一致",
      endpoint: "-",
      agent: summarizeEnhancedChannel(viewport.connectionModel.enhancedChannel),
      agentText: "增强通道已连接",
      display: "not_reported",
      framebuffer: runtimeEvidence.framebuffer,
      modes: "not_reported",
      permissions: "not_reported",
      frameDiagnostics: "not_reported",
      inputStatus: latestInput.type === "none" ? "waiting_for_input" : "input_ready",
      inputStatusText: inputStatusText(latestInput.type === "none" ? "waiting_for_input" : "input_ready"),
      capabilityBoundary: "agent_transport",
      capabilityBoundaryText: "远端能力由增强通道上报",
    };
  }

  return undefined;
}

function appServerInputStatus(
  overview: LocalVncServerOverview,
  latestInput: SessionDiagnosticsInputActivitySummary,
): SessionAppServerDiagnosticsSummary["inputStatus"] {
  if (overview.viewOnly) {
    return "view_only";
  }

  const accessibility = overview.permissions.find((item) => item.kind === "accessibility");
  if (latestInput.type !== "none" && accessibility?.status === "missing") {
    return "received_but_unauthorized";
  }

  return latestInput.type === "none" ? "waiting_for_input" : "input_ready";
}

function formatFrameDiagnostics(
  value: LocalVncServerOverview["frameDiagnostics"],
): string | undefined {
  if (!value) {
    return "not_reported";
  }

  return [
    `采集=${captureStatusText(value.captureStatus)}`,
    `capture=${formatMetricNumber(value.captureFrames)}`,
    `captureFailures=${formatMetricNumber(value.captureFailures)}`,
    `requests=${formatMetricNumber(value.framebufferUpdateRequests)}`,
    `updates=${formatMetricNumber(value.framebufferUpdates)}`,
    `captureMs=${formatMetricNumber(value.lastCaptureDurationMs)}`,
    `ageMs=${formatMetricNumber(value.lastFrameAgeMs)}`,
    `diffMs=${formatMetricNumber(value.lastDiffDurationMs)}`,
    `writeMs=${formatMetricNumber(value.lastWriteDurationMs)}`,
    `rects=${formatMetricNumber(value.lastDirtyRects)}`,
    `encoding=${value.lastFramebufferEncoding || "-"}`,
    `rawBytes=${formatMetricNumber(value.lastRawBytesWritten)}`,
    `encodedBytes=${formatMetricNumber(value.lastEncodedBytesWritten)}`,
    `bytes=${formatMetricNumber(value.lastBytesWritten)}`,
  ].join(" ");
}

function captureStatusText(status: LocalVncServerCaptureStatus): string {
  switch (status) {
    case "waiting": return "等待画面";
    case "ready": return "正常";
    case "permission_denied": return "未获屏幕录制权限";
    case "unavailable": return "暂不可用";
    case "invalid_frame": return "画面数据无效";
    case "unsupported": return "平台不支持";
    default: return "未上报";
  }
}

function formatMetricNumber(value: number | null | undefined): string {
  return Number.isFinite(value) ? String(value) : "-";
}

function inputStatusText(status: SessionAppServerDiagnosticsSummary["inputStatus"]): string {
  switch (status) {
    case "input_ready":
      return "输入正常";
    case "received_but_unauthorized":
      return "已收到输入，系统未授权控制";
    case "view_only":
      return "只读模式";
    case "waiting_for_input":
      return "尚未收到输入";
  }
}

function localAgentStatusText(status: string): string {
  switch (status) {
    case "running":
      return "增强服务运行中";
    case "failed":
      return "增强服务异常";
    case "stopped":
      return "增强服务未启动";
    default:
      return "增强服务状态未知";
  }
}

function formatSize(width: number, height: number): string {
  const normalizedWidth = Number.isFinite(width) && width > 0 ? Math.round(width) : 0;
  const normalizedHeight = Number.isFinite(height) && height > 0 ? Math.round(height) : 0;
  return normalizedWidth > 0 && normalizedHeight > 0
    ? `${normalizedWidth}x${normalizedHeight}`
    : "-";
}
