import type { Translate } from "../../app/types.ts";
import { eventsForSession, type EventLogEntry } from "../../diagnostics/eventLog.ts";
import type { RemoteSession } from "../core/sessionManager.ts";
import type { RemoteViewportSummary } from "../viewport/remoteViewport.ts";
import { formatDuration, redactSensitiveText } from "./sessionDiagnostics.ts";
import type { SessionDiagnosticsPanelSummary } from "./sessionDiagnosticsPanel.ts";

export type UserDiagnosticEvent = {
  id: string;
  time: string;
  tone: "normal" | "warning" | "error";
  message: string;
  count: number;
};

export type SessionDiagnosticView = {
  sessionId: string;
  target: string;
  stateText: string;
  tone: RemoteViewportSummary["tone"];
  duration: string;
  issue: string;
  base: string;
  display: string;
  displayEvidence: string;
  input: string;
  inputStatus: string;
  enhanced: string;
  userEvents: UserDiagnosticEvent[];
  recentEvents: EventLogEntry[];
  summaryReport: string;
  technicalReport: string;
};

export function buildSessionDiagnosticView(input: {
  session: RemoteSession;
  viewport: RemoteViewportSummary;
  summary: SessionDiagnosticsPanelSummary;
  events: EventLogEntry[];
  nowMs: number;
  t: Translate;
}): SessionDiagnosticView {
  const { session, viewport, summary, t } = input;
  const sessionEvents = eventsForSession(input.events, session.id)
    .map((event) => ({ ...event, message: redactSensitiveText(event.message) }));
  const recentEvents = sessionEvents;
  const userEvents: UserDiagnosticEvent[] = [];
  const eventsByMessage = new Map<string, UserDiagnosticEvent>();
  for (const event of sessionEvents) {
    const presented = presentUserEvent(event);
    if (!presented) continue;
    const existing = eventsByMessage.get(presented.message);
    if (existing) {
      existing.count += 1;
      continue;
    }
    eventsByMessage.set(presented.message, presented);
    if (userEvents.length < 6) userEvents.push(presented);
  }
  const connectedAtMs = session.connectedAt ? Date.parse(session.connectedAt) : NaN;
  const endedAtMs = session.closedAt ? Date.parse(session.closedAt) : input.nowMs;
  const elapsedSeconds = Number.isFinite(connectedAtMs) && Number.isFinite(endedAtMs)
    ? Math.max(0, Math.floor((endedAtMs - connectedAtMs) / 1000))
    : 0;
  const duration = formatDuration(elapsedSeconds);
  const stateText = t(`connection.${session.state}`);
  const failed = viewport.tone === "failed";
  const issue = failed
    ? redactSensitiveText(session.lastIssue?.message?.trim() || "连接未完成，请检查地址、网络或认证信息后重试。")
    : summary.transportLayers.issueLayer === "enhanced"
      ? "增强能力出现问题，基础 VNC 连接仍可使用。"
      : "";
  const enhancedStatus = viewport.connectionModel.enhancedChannel.status;
  const enhanced = enhancedStatus === "unavailable" && !viewport.connectionModel.enhancedChannel.lastErrorCode
    ? "未启用（可选）"
    : enhancedStatus === "available"
      ? "可用（可选）"
      : enhancedStatus === "degraded"
        ? "部分不可用（基础连接不受影响）"
        : summary.transportLayerTexts.enhanced;
  const displayEvidence = session.state !== "connected"
    ? "等待连接"
    : summary.runtimeEvidence.health === "evidence_available"
      ? "收到过远端画面"
      : summary.runtimeEvidence.health === "-"
        ? "暂无画面状态信息"
        : "尚未确认画面到达";
  const inputStatus = session.state !== "connected"
    ? "等待连接"
    : summary.inputMode === "control"
      ? "允许操作"
      : summary.inputMode === "readonly" || summary.inputMode === "viewOnlyShared"
        ? "只读"
        : "尚未确认当前窗口的操作状态";

  const summaryReport = [
    "会话诊断摘要",
    `目标：${session.target}`,
    `连接状态：${stateText}`,
    `持续时间：${duration}`,
    `当前判断：${issue || "未记录到明确故障"}`,
    `基础连接：${summary.transportLayerTexts.base}`,
    `画面：${displayEvidence}；${summary.transportLayerTexts.displaySource}`,
    `操作：${inputStatus}；${summary.transportLayerTexts.inputPath}`,
    `附加功能：${enhanced}`,
    "近期会话记录：",
    ...(!userEvents.length
      ? ["- 无需关注的状态变化"]
      : userEvents.map((event) => `- ${event.time} ${event.message}${event.count > 1 ? `（重复 ${event.count} 次）` : ""}`)),
  ].join("\n");
  const technicalReport = [
    summaryReport,
    `技术字段：\n${redactSensitiveText(summary.copyText)}`,
    `当前会话内存事件（${recentEvents.length} 条）：\n${!recentEvents.length
      ? "- 无"
      : recentEvents.map((event) => `[${event.time}] ${event.level.toUpperCase()} ${event.source}: ${event.message}`).join("\n")}`,
  ].join("\n\n");

  return {
    sessionId: session.id,
    target: session.target,
    stateText,
    tone: viewport.tone,
    duration,
    issue,
    base: summary.transportLayerTexts.base,
    display: summary.transportLayerTexts.displaySource,
    displayEvidence,
    input: summary.transportLayerTexts.inputPath,
    inputStatus,
    enhanced,
    userEvents,
    recentEvents,
    summaryReport,
    technicalReport,
  };
}

function presentUserEvent(event: EventLogEntry): UserDiagnosticEvent | null {
  if (event.eventKey === "local_server_detection_failed") return null;
  let message = event.userMessage;
  if (!message && event.level === "error") {
    message = "会话发生错误，可查看技术信息了解详情。";
  }
  if (!message && event.level === "warning") {
    message = "会话遇到问题，可查看技术信息了解详情。";
  }
  if (!message) return null;
  return {
    id: event.id,
    time: event.time,
    tone: event.level === "error" ? "error" : event.level === "warning" ? "warning" : "normal",
    message: redactSensitiveText(message),
    count: 1,
  };
}
