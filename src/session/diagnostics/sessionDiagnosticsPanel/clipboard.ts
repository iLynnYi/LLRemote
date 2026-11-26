import type {
  SessionClipboardActivity,
  SessionClipboardActivityChannel,
  SessionClipboardState,
} from "../../clipboard/sessionClipboardActivity.ts";
import type {
  SessionDiagnosticsClipboardActivitySummary,
  SessionDiagnosticsClipboardSummary,
} from "../sessionDiagnosticsPanel.ts";
import { formatTimestamp, formatTimeOfDay } from "./time.ts";

export function summarizeClipboard(
  clipboard: SessionClipboardState | null | undefined,
): SessionDiagnosticsClipboardSummary {
  return {
    mode: clipboard?.mode ?? "off",
    enabled: Boolean(clipboard?.enabled),
    readonly: Boolean(clipboard?.readonly),
    clientToServer: summarizeClipboardActivity(clipboard?.clientToServer),
    serverToClient: summarizeClipboardActivity(clipboard?.serverToClient),
  };
}

export function formatClipboardCopyLine(activity: SessionDiagnosticsClipboardActivitySummary | null): string {
  if (!activity) {
    return "-";
  }

  return [
    activity.at,
    activity.statusText,
    `reason=${activity.reason}`,
    `mode=${activity.mode}`,
    `channel=${activity.channel}`,
    `bytes=${activity.bytes}`,
  ].join(" ");
}

function summarizeClipboardActivity(
  activity: SessionClipboardActivity | null | undefined,
): SessionDiagnosticsClipboardActivitySummary | null {
  if (!activity) {
    return null;
  }

  return {
    direction: activity.direction,
    at: formatTimestamp(activity.atMs),
    atText: formatTimeOfDay(activity.atMs),
    accepted: activity.accepted,
    reason: activity.reason,
    statusText: clipboardActivityStatusText(activity),
    mode: activity.mode,
    bytes: String(activity.bytes),
    source: activity.source,
    sourceText: activity.source ? clipboardSourceText(activity.source) : undefined,
    channel: activity.channel,
    channelText: clipboardChannelText(activity.channel),
  };
}

function clipboardActivityStatusText(activity: SessionClipboardActivity): string {
  if (activity.direction === "client-to-server") {
    if (!activity.accepted) return "未发送";
    return activity.channel === "agent" && activity.reason === "remote-written"
      ? "已写入远端" : "已发送";
  }

  return activity.accepted ? "已写入" : "未写入";
}

function clipboardSourceText(source: NonNullable<SessionClipboardActivity["source"]>): string {
  switch (source) {
    case "clipboard":
      return "剪贴板轮询";
    case "paste":
      return "粘贴事件";
    case "pasteShortcut":
      return "粘贴快捷键";
  }
}

function clipboardChannelText(channel: SessionClipboardActivityChannel): string {
  switch (channel) {
    case "agent":
      return "Agent";
    case "rfb":
      return "RFB";
    case "unknown":
      return "待确认";
  }
}
