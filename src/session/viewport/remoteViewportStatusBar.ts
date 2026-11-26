import type { EventLogEntry } from "@diagnostics/eventLog.ts";
import type { RemoteViewportSummary } from "./remoteViewport.ts";
import type { ServerDesktopState } from "../protocol/protocolAdapter/serverRuntime.ts";
import type { RemoteSessionTiming } from "../diagnostics/sessionDiagnostics.ts";
import { elapsedRemoteSessionSeconds, formatDuration } from "../diagnostics/sessionDiagnostics.ts";

export type RemoteViewportStatusBarSummary = {
  state: string;
  duration: string;
  transport: string;
  latestEvent: string;
};

export type RemoteViewportStatusHudDisplay = {
  width?: number | null;
  height?: number | null;
  label?: string | null;
};

export type RemoteViewportStatusHudSummary = RemoteViewportStatusBarSummary & {
  runtimeDesktop?: ServerDesktopState;
  runtimeRecoveryId?: number;
  title: string;
  target: string;
  inputMode: string;
  display: string;
  displaySource: string;
};

export type RemoteViewportStatusBarInput = {
  viewport: RemoteViewportSummary;
  session?: RemoteSessionTiming | null;
  events: EventLogEntry[];
  nowMs: number;
};

export type RemoteViewportStatusHudInput = RemoteViewportStatusBarInput & {
  display?: RemoteViewportStatusHudDisplay | null;
};

export function summarizeRemoteViewportStatusBar(
  input: RemoteViewportStatusBarInput,
): RemoteViewportStatusBarSummary {
  return {
    state: input.viewport.statusLabel,
    duration: formatDuration(elapsedRemoteSessionSeconds(input.session, input.nowMs)),
    transport: input.viewport.transportLabel || "-",
    latestEvent: formatLatestEvent(findLatestViewportEvent(input.events, input.viewport)),
  };
}

export function summarizeRemoteViewportStatusHud(
  input: RemoteViewportStatusHudInput,
): RemoteViewportStatusHudSummary {
  const status = summarizeRemoteViewportStatusBar(input);
  return {
    title: formatHudTitle(input.viewport),
    target: input.viewport.target || input.viewport.title || "-",
    state: status.state,
    duration: status.duration,
    inputMode: input.viewport.inputPolicy === "viewOnlyShared"
      ? "viewOnlyShared"
      : input.viewport.inputMode,
    display: formatHudDisplay(input.display),
    displaySource: input.display?.label || "-",
    transport: status.transport,
    latestEvent: status.latestEvent,
  };
}

function formatHudTitle(viewport: RemoteViewportSummary): string {
  const target = viewport.title || viewport.target;
  return target.split(":")[0] || target || "-";
}

function formatHudDisplay(display: RemoteViewportStatusHudDisplay | null | undefined): string {
  const width = display?.width ?? 0;
  const height = display?.height ?? 0;
  return width > 0 && height > 0 ? `${width}x${height}` : "-";
}

function findLatestViewportEvent(
  events: EventLogEntry[],
  viewport: RemoteViewportSummary,
): EventLogEntry | undefined {
  const markers = [
    viewport.id,
    viewport.target,
    viewport.transportLabel,
  ].filter(Boolean);

  return events.find((entry) =>
    markers.some((marker) => entry.message.includes(marker)),
  );
}

function formatLatestEvent(event: EventLogEntry | undefined): string {
  if (!event) {
    return "-";
  }

  return `${event.source}: ${event.message}`;
}
