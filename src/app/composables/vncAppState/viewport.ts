import type { SessionWorkspaceRenderSlot } from "@session/viewport/sessionWorkspace";
import { emit } from "@tauri-apps/api/event";
import type { Ref } from "vue";
import type { RemoteSession, RemoteSessionTab } from "@session/core/sessionManager";
import type { SessionConnectionRegistry } from "@session/core/sessionConnectionRegistry";
import { sessionExternalWindowStateEvent } from "@session/external-window/sessionExternalWindowState";
import type { RemoteViewportSummary } from "@session/viewport/remoteViewport";
import type { RemoteViewportStatusHudSummary } from "@session/viewport/remoteViewportStatusBar";
import { sessionSupportsAction } from "@session/transfers/sessionCapabilities";
import { shouldCleanupConnectionForSessionState } from "@session/core/sessionConnectionCleanup";
import { createSessionDisplayChoices, type LocalServerDisplayInventory } from "../../../session/viewport/sessionDisplayChoices.ts";
import type { AgentDisplayInventoryResult } from "../../../session/protocol/protocolAdapter.ts";

export function createFallbackWorkspaceRenderSlot(sessionId: string): SessionWorkspaceRenderSlot {
  return {
    id: sessionId,
    label: sessionId,
    target: sessionId,
    state: "closed",
    active: false,
    liveCanvas: false,
    closable: false,
    renderState: "parked",
    visible: false,
    externalWindowCandidate: false,
    externalWindowOwned: false,
  };
}

export function isKeyboardEditableTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  );
}

export type ExternalWindowStateBridgeOptions = {
  findSession: (sessionId: string) => RemoteSession | undefined;
  getRemoteViewportStatusHud: (tab: RemoteSessionTab) => RemoteViewportStatusHudSummary;
  getRemoteViewportSummary: (tab: RemoteSessionTab) => RemoteViewportSummary;
  getSessionLocalServerView: (sessionId: string) => LocalServerDisplayInventory | null;
  getSessionRemoteDisplayInventory?: (sessionId: string) => AgentDisplayInventoryResult | null;
  getSessionSelectedRemoteDisplayId?: (sessionId: string) => string | null;
  isSessionRecording: (sessionId: string) => boolean;
  getSessionRecordingPhase?: (sessionId: string) => import("../../../session/artifacts/sessionRecordingTask.ts").RecordingPhase | undefined;
  getSessionRecordingSpace?: (sessionId: string) => import("../../../session/artifacts/sessionRecordingTask.ts").RecordingSpaceStatus | undefined;
  getSessionRecordingElapsedSeconds?: (sessionId: string) => number;
  sessionConnectionRegistry: SessionConnectionRegistry;
  sessionReadonlyBySessionId: Ref<Record<string, boolean>>;
  getSessionTabs: () => RemoteSessionTab[];
};

export function createExternalWindowStateBridge(options: ExternalWindowStateBridgeOptions) {
  function createExternalWindowControlSnapshot(sessionId: string) {
    const session = options.findSession(sessionId);
    const tab = options.getSessionTabs().find((item) => item.id === sessionId);
    const connection = options.sessionConnectionRegistry.get(sessionId)?.connection;
    const fileTransferAvailable = sessionSupportsAction(connection?.connectionModel.capabilities, "sendFile");
    const snapshotTab = tab ?? {
      id: sessionId,
      label: session?.target ?? sessionId,
      target: session?.target ?? sessionId,
      state: session?.state ?? "connecting",
      active: true,
      liveCanvas: true,
      closable: false,
    };
    const statusHud = options.getRemoteViewportStatusHud(snapshotTab);
    const viewportSummary = options.getRemoteViewportSummary(snapshotTab);
    const viewportTone = viewportSummary.tone;
    return {
      sessionId,
      fileTransferAvailable,
      statusHud,
      viewportTone,
      toolboxContext: {
        sessionId,
        state: session?.state ?? tab?.state ?? null,
        readonly: Boolean(options.sessionReadonlyBySessionId.value[sessionId]),
        viewOnlyShared: viewportSummary.inputPolicy === "viewOnlyShared",
        capabilities: connection?.connectionModel.capabilities ?? null,
        active: true,
        fullscreen: false,
        externalWindow: true,
        localServer: Boolean(options.getSessionLocalServerView(sessionId)),
        displayChoices: createSessionDisplayChoices(
          options.getSessionLocalServerView(sessionId),
          options.getSessionRemoteDisplayInventory?.(sessionId),
          options.getSessionSelectedRemoteDisplayId?.(sessionId),
        ),
        recording: Boolean(options.isSessionRecording(sessionId)),
        recordingPhase: options.getSessionRecordingPhase?.(sessionId),
        recordingElapsedSeconds: options.getSessionRecordingElapsedSeconds?.(sessionId),
        recordingSpace: options.getSessionRecordingSpace?.(sessionId),
        fileTransferAvailable,
      },
    };
  }

  function publishExternalWindowState(sessionId: string) {
    const state = options.findSession(sessionId)?.state;
    const payload = {
      sessionId,
      updatedAt: Date.now(),
      inputEnabled: !Boolean(options.sessionReadonlyBySessionId.value[sessionId])
        && !(state && shouldCleanupConnectionForSessionState(state)),
      controlSnapshot: createExternalWindowControlSnapshot(sessionId),
    };
    void emit(sessionExternalWindowStateEvent(sessionId), payload).catch(() => undefined);
  }

  return { publishExternalWindowState };
}
