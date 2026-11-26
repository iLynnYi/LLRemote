import type { Ref } from "vue";
import type { Translate } from "@app/types";
import type { DiagnosticEventContext, EventLogLevel } from "@diagnostics/eventLog";
import {
  matchesLocalVncServerTarget,
} from "../../../session/core/localVncServerSession.ts";
import type { SessionConnectionRegistry } from "../../../session/core/sessionConnectionRegistry.ts";
import type {
  LocalVncServerOverview,
  LocalVncServerViewRequest,
} from "@server/localShare";

export type LocalVncServerSessionPreparation = {
  localEndpointMatch: {
    state: "matched";
    viewOnly: boolean;
    selectedDisplayId?: string;
    framebufferWidth?: number;
    framebufferHeight?: number;
  };
  selectedDisplayId?: string;
  framebufferWidth?: number;
  framebufferHeight?: number;
  viewOnlyShared?: boolean;
};

type SessionLocalServerControlsOptions = {
  errorMessage: Ref<string>;
  readServerStatus: () => Promise<LocalVncServerOverview>;
  updateServerView: (request: LocalVncServerViewRequest) => Promise<LocalVncServerOverview>;
  localVncServerOverview: Ref<LocalVncServerOverview | null>;
  logEvent: (level: EventLogLevel, source: string, message: string, context?: DiagnosticEventContext) => void;
  sessionConnectionRegistry: SessionConnectionRegistry;
  sessionLocalServerViewBySessionId: Ref<Record<string, LocalVncServerOverview>>;
  setSessionLocalServerView: (sessionId: string, overview: LocalVncServerOverview) => void;
  t: Translate;
  updateSessionDisplayInfoFromLocalServer: (sessionId: string, overview: LocalVncServerOverview) => void;
  getSessionTarget?: (sessionId: string) => { host: string; port: number } | undefined;
  onAgentChannelChanged?: () => void;
  syncSessionViewOnlyShared?: (sessionId: string, viewOnly: boolean) => void;
};

export function useSessionLocalServerControls(options: SessionLocalServerControlsOptions) {
  const {
    errorMessage,
    readServerStatus, updateServerView,
    localVncServerOverview,
    logEvent,
    sessionConnectionRegistry,
    sessionLocalServerViewBySessionId,
    setSessionLocalServerView,
    t,
    updateSessionDisplayInfoFromLocalServer,
    getSessionTarget,
    onAgentChannelChanged,
    syncSessionViewOnlyShared,
  } = options;

  function syncLocalAgentPortForSession(
    sessionId: string,
    overview: LocalVncServerOverview,
    targetHost: string,
    targetPort: number,
  ): LocalVncServerSessionPreparation | undefined {
    const localMatch = matchesLocalVncServerTarget(overview, targetHost, targetPort);
    if (!localMatch) {
      return undefined;
    }

    setSessionLocalServerView(sessionId, overview);
    updateSessionDisplayInfoFromLocalServer(sessionId, overview);
    syncSessionViewOnlyShared?.(sessionId, overview.viewOnly);
    onAgentChannelChanged?.();
    return {
      localEndpointMatch: {
        state: "matched",
        viewOnly: Boolean(overview.viewOnly),
        selectedDisplayId: overview.selectedDisplayId,
        framebufferWidth: overview.framebufferWidth,
        framebufferHeight: overview.framebufferHeight,
      },
      selectedDisplayId: overview.selectedDisplayId,
      framebufferWidth: overview.framebufferWidth,
      framebufferHeight: overview.framebufferHeight,
      viewOnlyShared: overview.viewOnly,
    };
  }

  function syncLocalEndpointMatchesForSessions(overview: LocalVncServerOverview) {
    if (!getSessionTarget) {
      return;
    }

    for (const sessionId of sessionConnectionRegistry.listLiveSessionIds()) {
      const target = getSessionTarget(sessionId);
      if (!target) {
        continue;
      }

      syncLocalAgentPortForSession(sessionId, overview, target.host, target.port);
    }
  }

  async function detectLocalVncServerForSession(
    sessionId: string,
    targetHost: string,
    targetPort: number,
  ): Promise<LocalVncServerSessionPreparation | undefined> {
    try {
      const overview = await readServerStatus();
      localVncServerOverview.value = overview;

      const preparation = syncLocalAgentPortForSession(sessionId, overview, targetHost, targetPort);
      if (!preparation) {
        logEvent(
          "info",
          t("log.sourceSession"),
          `本机共享检测：目标 ${targetHost}:${targetPort} 未匹配；本机服务 ${overview.status}（端口 ${overview.port}），附加服务 ${overview.agent.status}。`,
          { origin: "conversation", conversationId: sessionId, target: `${targetHost}:${targetPort}`, eventKey: "local_server_unmatched" },
        );
        return undefined;
      }

      logEvent(
        "info",
        t("log.sourceSession"),
        t("sessions.localServerDetected", { count: overview.displays.length }),
        { origin: "conversation", conversationId: sessionId, target: `${targetHost}:${targetPort}`, eventKey: "local_server_matched" },
      );
      return preparation;
    } catch (error) {
      logEvent("warning", t("log.sourceSession"), t("sessions.localServerDetectFailed", { error: String(error) }), { origin: "conversation", conversationId: sessionId, target: `${targetHost}:${targetPort}`, eventKey: "local_server_detection_failed" });
      return undefined;
    }
  }

  async function updateSessionLocalServerView(sessionId: string, request: LocalVncServerViewRequest) {
    const current = sessionLocalServerViewBySessionId.value[sessionId];
    if (!current) {
      return;
    }

    try {
      const overview = await updateServerView(request);
      localVncServerOverview.value = overview;
      setSessionLocalServerView(sessionId, overview);
      updateSessionDisplayInfoFromLocalServer(sessionId, overview);
      const connection = sessionConnectionRegistry.get(sessionId)?.connection;
      connection?.refreshDisplay();
      globalThis.setTimeout(() => {
        sessionConnectionRegistry.get(sessionId)?.connection.refreshDisplay();
      }, 120);
    } catch (error) {
      errorMessage.value = String(error);
      logEvent("error", t("log.sourceSession"), String(error), { origin: "conversation", conversationId: sessionId, eventKey: "local_server_view_failed", userMessage: "切换本机共享画面失败，请重试。" });
    }
  }

  return {
    detectLocalVncServerForSession,
    syncLocalEndpointMatchesForSessions,
    updateSessionLocalServerView,
  };
}
