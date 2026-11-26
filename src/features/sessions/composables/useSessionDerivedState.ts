import { computed, type ComputedRef, type Ref } from "vue";
import type { Translate } from "@app/types";
import type { EventLogEntry } from "../../../diagnostics/eventLog.ts";
import type { RemoteViewportSummary } from "@session/viewport/remoteViewport";
import type { RemoteSession, RemoteSessionTab } from "@session/core/sessionManager";
import { buildSessionDiagnosticView } from "../../../session/diagnostics/sessionDiagnosticView.ts";
import {
  summarizeSessionDiagnosticsPanel,
  type SessionDiagnosticsInputActivity,
} from "@session/diagnostics/sessionDiagnosticsPanel";
import { matchesLocalVncServerTarget } from "@session/core/localVncServerSession";
import type { RemoteSessionSnapshot } from "@session/protocol/remoteSessionCore";
import type { SessionWorkspaceRenderSlot } from "@session/viewport/sessionWorkspace";
import type { ViewportRuntimeEvidence } from "@session/diagnostics/runtimeEvidence";
import type { SessionClipboardState } from "@session/clipboard/sessionClipboardActivity";
import type { LocalVncServerOverview } from "@server/localShare";

type SessionDerivedStateOptions = {
  activeRemoteSession: Ref<RemoteSession | null>;
  eventLog: Ref<EventLogEntry[]>;
  getRemoteViewportSummary: (tab: RemoteSessionTab) => RemoteViewportSummary;
  nowTick: Ref<number>;
  t: Translate;
  sessionClipboardStateBySessionId: Ref<Record<string, SessionClipboardState>>;
  sessionLatestInputBySessionId: Ref<Record<string, SessionDiagnosticsInputActivity>>;
  localVncServerOverview: Ref<LocalVncServerOverview | null>;
  sessionLocalServerViewBySessionId: Ref<Record<string, LocalVncServerOverview>>;
  sessionRuntimeEvidenceBySessionId: Ref<Record<string, ViewportRuntimeEvidence>>;
  remoteSessionCoreSnapshot: ComputedRef<RemoteSessionSnapshot | null>;
  sessionTabs: ComputedRef<RemoteSessionTab[]>;
  workspaceRenderSlots: ComputedRef<SessionWorkspaceRenderSlot[]>;
};

export function useSessionDerivedState(options: SessionDerivedStateOptions) {
  const {
    activeRemoteSession,
    eventLog,
    getRemoteViewportSummary,
    nowTick,
    t,
    sessionClipboardStateBySessionId,
    sessionLatestInputBySessionId,
    localVncServerOverview,
    sessionLocalServerViewBySessionId,
    sessionRuntimeEvidenceBySessionId,
    remoteSessionCoreSnapshot,
    sessionTabs,
    workspaceRenderSlots,
  } = options;
  const activeViewportSummary = computed<RemoteViewportSummary | null>(() => {
    const activeTab = sessionTabs.value.find((tab) => tab.id === activeRemoteSession.value?.id);

    return activeTab ? getRemoteViewportSummary(activeTab) : null;
  });
  const activeWorkspaceRenderSlot = computed<SessionWorkspaceRenderSlot | null>(() => {
    if (!activeRemoteSession.value) {
      return null;
    }

    return workspaceRenderSlots.value.find((item) => item.id === activeRemoteSession.value?.id) ?? null;
  });
  const activeLocalServerDiagnostics = computed<LocalVncServerOverview | null>(() => {
    const activeSession = activeRemoteSession.value;
    if (!activeSession) {
      return null;
    }

    const sessionLocalServerView = sessionLocalServerViewBySessionId.value[activeSession.id];
    if (sessionLocalServerView) {
      return sessionLocalServerView;
    }

    const overview = localVncServerOverview.value;
    return overview && matchesLocalVncServerTarget(overview, activeSession.host, activeSession.port)
      ? overview
      : null;
  });
  const sessionDiagnosticsSummary = computed(() =>
    summarizeSessionDiagnosticsPanel({
      viewport: activeViewportSummary.value,
      events: eventLog.value,
      runtimeEvidence: activeRemoteSession.value
        ? sessionRuntimeEvidenceBySessionId.value[activeRemoteSession.value.id]
        : null,
      renderState: activeWorkspaceRenderSlot.value?.renderState,
      externalWindowOwned: activeWorkspaceRenderSlot.value?.externalWindowOwned,
      latestInput: activeRemoteSession.value
        ? sessionLatestInputBySessionId.value[activeRemoteSession.value.id] ?? null
        : null,
      clipboard: activeRemoteSession.value
        ? sessionClipboardStateBySessionId.value[activeRemoteSession.value.id] ?? null
        : null,
      remoteSessionCore: activeRemoteSession.value
        ? remoteSessionCoreSnapshot.value
        : null,
      localServer: activeLocalServerDiagnostics.value,
    }),
  );
  const diagnosticView = computed(() => {
    const session = activeRemoteSession.value;
    const viewport = activeViewportSummary.value;
    return session && viewport
      ? buildSessionDiagnosticView({
        session,
        viewport,
        summary: sessionDiagnosticsSummary.value,
        events: eventLog.value,
        nowMs: nowTick.value,
        t,
      })
      : null;
  });
  const diagnosticReport = computed(() => diagnosticView.value?.summaryReport ?? "");
  const technicalDiagnosticReport = computed(() => diagnosticView.value?.technicalReport ?? "");

  return {
    activeViewportSummary,
    activeWorkspaceRenderSlot,
    diagnosticReport,
    technicalDiagnosticReport,
    sessionDiagnosticsSummary,
  };
}
