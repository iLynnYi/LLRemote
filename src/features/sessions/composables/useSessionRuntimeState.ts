import { ref } from "vue";
import type { SessionDiagnosticsInputActivity } from "@session/diagnostics/sessionDiagnosticsPanel";
import type { ViewportRuntimeEvidence } from "@session/diagnostics/runtimeEvidence";
import type { ClipboardSyncMode } from "@session/settings/sessionSettings";
import type {
  SessionClipboardActivity,
  SessionClipboardState,
} from "@session/clipboard/sessionClipboardActivity";
import {
  createSessionDisplayInfoFromAgentDisplays,
  mergeSessionDisplayInfo,
  summarizeSessionDisplay,
  type SessionDisplayInfo,
} from "@session/viewport/sessionDisplay";
import { createDisplayInfoFromLocalVncServer } from "@session/core/localVncServerSession";
import type { SessionViewportScaleMode } from "@session/viewport/sessionViewportScale";
import type { AgentDisplayInventoryResult } from "@session/protocol/protocolAdapter";
import type { LocalVncServerOverview } from "@server/localShare";

export function useSessionRuntimeState() {
  const sessionDisplayInfoBySessionId = ref<Record<string, SessionDisplayInfo>>({});
  const sessionRuntimeEvidenceBySessionId = ref<Record<string, ViewportRuntimeEvidence>>({});
  const sessionLatestInputBySessionId = ref<Record<string, SessionDiagnosticsInputActivity>>({});
  const sessionClipboardStateBySessionId = ref<Record<string, SessionClipboardState>>({});
  const sessionLocalServerViewBySessionId = ref<Record<string, LocalVncServerOverview>>({});
  const sessionRemoteDisplayInventoryBySessionId = ref<Record<string, AgentDisplayInventoryResult>>({});
  const sessionSelectedRemoteDisplayIdBySessionId = ref<Record<string, string>>({});
  const sessionViewportScaleModeBySessionId = ref<Record<string, SessionViewportScaleMode>>({});
  const sessionReadonlyBySessionId = ref<Record<string, boolean>>({});
  const sessionViewOnlySharedBySessionId = ref<Record<string, boolean>>({});

  function recordSessionInputActivity(
    sessionId: string,
    source: SessionDiagnosticsInputActivity["source"],
    type: SessionDiagnosticsInputActivity["type"],
  ) {
    sessionLatestInputBySessionId.value = {
      ...sessionLatestInputBySessionId.value,
      [sessionId]: {
        source,
        type,
        atMs: Date.now(),
      },
    };
  }

  function updateSessionDisplayInfo(sessionId: string, info: SessionDisplayInfo) {
    const current = sessionDisplayInfoBySessionId.value[sessionId];
    const preserveCurrentMultiDisplayHint = (sessionLocalServerViewBySessionId.value[sessionId]?.displays.length ?? 0) > 1
      || (sessionRemoteDisplayInventoryBySessionId.value[sessionId]?.displays.length ?? 0) > 1;
    sessionDisplayInfoBySessionId.value = {
      ...sessionDisplayInfoBySessionId.value,
      [sessionId]: mergeSessionDisplayInfo(current, info, preserveCurrentMultiDisplayHint),
    };
  }

  function updateSessionRuntimeEvidence(sessionId: string, evidence: ViewportRuntimeEvidence) {
    sessionRuntimeEvidenceBySessionId.value = {
      ...sessionRuntimeEvidenceBySessionId.value,
      [sessionId]: evidence,
    };
  }

  function setSessionClipboardPolicy(sessionId: string, mode: ClipboardSyncMode, readonly: boolean) {
    const current = sessionClipboardStateBySessionId.value[sessionId];
    sessionClipboardStateBySessionId.value = {
      ...sessionClipboardStateBySessionId.value,
      [sessionId]: {
        mode,
        enabled: mode !== "off",
        readonly,
        clientToServer: current?.clientToServer ?? null,
        serverToClient: current?.serverToClient ?? null,
      },
    };
  }

  function updateSessionClipboardActivity(sessionId: string, activity: SessionClipboardActivity) {
    const current = sessionClipboardStateBySessionId.value[sessionId];
    sessionClipboardStateBySessionId.value = {
      ...sessionClipboardStateBySessionId.value,
      [sessionId]: {
        mode: activity.mode,
        enabled: activity.mode !== "off",
        readonly: current?.readonly ?? false,
        clientToServer: activity.direction === "client-to-server"
          ? activity
          : current?.clientToServer ?? null,
        serverToClient: activity.direction === "server-to-client"
          ? activity
          : current?.serverToClient ?? null,
      },
    };
  }

  function updateSessionDisplayInfoFromLocalServer(sessionId: string, overview: LocalVncServerOverview) {
    updateSessionDisplayInfo(sessionId, createDisplayInfoFromLocalVncServer(overview));
  }

  function setSessionLocalServerView(sessionId: string, overview: LocalVncServerOverview) {
    sessionLocalServerViewBySessionId.value = {
      ...sessionLocalServerViewBySessionId.value,
      [sessionId]: overview,
    };
  }

  function setSessionRemoteDisplayInventory(sessionId: string, inventory: AgentDisplayInventoryResult) {
    sessionRemoteDisplayInventoryBySessionId.value = {
      ...sessionRemoteDisplayInventoryBySessionId.value,
      [sessionId]: inventory,
    };
    if (!sessionSelectedRemoteDisplayIdBySessionId.value[sessionId]) {
      const selectedDisplay = inventory.displays.find((display) => display.isPrimary) ?? inventory.displays[0];
      if (selectedDisplay) {
        setSessionSelectedRemoteDisplayId(sessionId, selectedDisplay.id);
      }
    }
    updateSessionDisplayInfo(sessionId, createSessionDisplayInfoFromAgentDisplays(inventory.displays));
  }

  function setSessionSelectedRemoteDisplayId(sessionId: string, displayId: string) {
    sessionSelectedRemoteDisplayIdBySessionId.value = {
      ...sessionSelectedRemoteDisplayIdBySessionId.value,
      [sessionId]: displayId,
    };
  }

  function getSessionDisplaySummary(sessionId: string) {
    return summarizeSessionDisplay(sessionDisplayInfoBySessionId.value[sessionId] ?? null);
  }

  function getSessionLocalServerView(sessionId: string) {
    return sessionLocalServerViewBySessionId.value[sessionId] ?? null;
  }

  function getSessionRemoteDisplayInventory(sessionId: string) {
    return sessionRemoteDisplayInventoryBySessionId.value[sessionId] ?? null;
  }

  function getSessionSelectedRemoteDisplayId(sessionId: string) {
    return sessionSelectedRemoteDisplayIdBySessionId.value[sessionId] ?? null;
  }

  function getSessionViewportScaleMode(sessionId: string) {
    return sessionViewportScaleModeBySessionId.value[sessionId] ?? "fit";
  }

  function setSessionViewportScaleMode(sessionId: string, scaleMode: SessionViewportScaleMode) {
    sessionViewportScaleModeBySessionId.value = {
      ...sessionViewportScaleModeBySessionId.value,
      [sessionId]: scaleMode,
    };
  }

  function setSessionReadonlyState(sessionId: string, readonly: boolean) {
    sessionReadonlyBySessionId.value = {
      ...sessionReadonlyBySessionId.value,
      [sessionId]: readonly,
    };
  }

  function setSessionViewOnlySharedState(sessionId: string, viewOnlyShared: boolean) {
    sessionViewOnlySharedBySessionId.value = {
      ...sessionViewOnlySharedBySessionId.value,
      [sessionId]: viewOnlyShared,
    };
  }

  function clearSessionRuntimeState(sessionId: string) {
    const { [sessionId]: _removed, ...remainingDisplayInfo } = sessionDisplayInfoBySessionId.value;
    sessionDisplayInfoBySessionId.value = remainingDisplayInfo;
    const { [sessionId]: _removedEvidence, ...remainingRuntimeEvidence } = sessionRuntimeEvidenceBySessionId.value;
    sessionRuntimeEvidenceBySessionId.value = remainingRuntimeEvidence;
    const { [sessionId]: _removedInputActivity, ...remainingInputActivity } = sessionLatestInputBySessionId.value;
    sessionLatestInputBySessionId.value = remainingInputActivity;
    const { [sessionId]: _removedClipboardState, ...remainingClipboardState } = sessionClipboardStateBySessionId.value;
    sessionClipboardStateBySessionId.value = remainingClipboardState;
    const { [sessionId]: _removedLocalServer, ...remainingLocalServerViews } = sessionLocalServerViewBySessionId.value;
    sessionLocalServerViewBySessionId.value = remainingLocalServerViews;
    const { [sessionId]: _removedRemoteDisplayInventory, ...remainingRemoteDisplayInventories } = sessionRemoteDisplayInventoryBySessionId.value;
    sessionRemoteDisplayInventoryBySessionId.value = remainingRemoteDisplayInventories;
    const { [sessionId]: _removedSelectedRemoteDisplayId, ...remainingSelectedRemoteDisplayIds } = sessionSelectedRemoteDisplayIdBySessionId.value;
    sessionSelectedRemoteDisplayIdBySessionId.value = remainingSelectedRemoteDisplayIds;
    const { [sessionId]: _removedViewportScaleMode, ...remainingViewportScaleModes } = sessionViewportScaleModeBySessionId.value;
    sessionViewportScaleModeBySessionId.value = remainingViewportScaleModes;
    const { [sessionId]: _removedReadonly, ...remainingReadonly } = sessionReadonlyBySessionId.value;
    sessionReadonlyBySessionId.value = remainingReadonly;
    const { [sessionId]: _removedViewOnlyShared, ...remainingViewOnlyShared } = sessionViewOnlySharedBySessionId.value;
    sessionViewOnlySharedBySessionId.value = remainingViewOnlyShared;
  }

  return {
    clearSessionRuntimeState,
    getSessionDisplaySummary,
    getSessionLocalServerView,
    getSessionRemoteDisplayInventory,
    getSessionSelectedRemoteDisplayId,
    getSessionViewportScaleMode,
    recordSessionInputActivity,
    sessionClipboardStateBySessionId,
    sessionDisplayInfoBySessionId,
    sessionLatestInputBySessionId,
    sessionLocalServerViewBySessionId,
    sessionRemoteDisplayInventoryBySessionId,
    sessionSelectedRemoteDisplayIdBySessionId,
    sessionReadonlyBySessionId,
    sessionRuntimeEvidenceBySessionId,
    sessionViewOnlySharedBySessionId,
    sessionViewportScaleModeBySessionId,
    setSessionLocalServerView,
    setSessionClipboardPolicy,
    setSessionRemoteDisplayInventory,
    setSessionSelectedRemoteDisplayId,
    setSessionReadonlyState,
    setSessionViewOnlySharedState,
    setSessionViewportScaleMode,
    updateSessionDisplayInfo,
    updateSessionDisplayInfoFromLocalServer,
    updateSessionClipboardActivity,
    updateSessionRuntimeEvidence,
  };
}
