import type { Translate } from "@app/types";
import type { DiagnosticEventContext, EventLogLevel } from "@diagnostics/eventLog";
import type { SessionManager } from "@session/core/sessionManager";
import type { AgentDisplayInventoryResult, ProtocolConnection } from "@session/protocol/protocolAdapter";
import { sessionSupportsAction } from "../../../../session/transfers/sessionCapabilities.ts";

export type SessionDisplayEventOptions = {
  logEvent: (level: EventLogLevel, source: string, message: string, context?: DiagnosticEventContext) => void;
  refreshSessionTabs: () => void;
  sessionManager: SessionManager;
  setSessionRemoteDisplayInventory: (sessionId: string, inventory: AgentDisplayInventoryResult) => void;
  t: Translate;
};

/** Refreshes optional Agent display metadata without participating in the RFB display path. */
export function createSessionDisplayEvents(options: SessionDisplayEventOptions) {
  async function refreshSessionRemoteDisplayInventory(sessionId: string, protocolConnection: ProtocolConnection, isLive = () => true) {
    if (!isLive()) return;
    if (!sessionSupportsAction(protocolConnection.connectionModel.capabilities, "enumerateAgentDisplays")) {
      return;
    }

    try {
      const inventory = await protocolConnection.enumerateRemoteDisplays();
      if (!isLive()) return;
      options.setSessionRemoteDisplayInventory(sessionId, inventory);
      options.refreshSessionTabs();
    } catch (error) {
      if (!isLive()) return;
      options.logEvent("warning", options.t("log.sourceSession"), `增强显示信息刷新失败；基础 VNC 连接不受影响。详情：${String(error)}`, { origin: "conversation", conversationId: sessionId, target: options.sessionManager.sessions.find((session) => session.id === sessionId)?.target, eventKey: "agent_display_inventory_failed", userMessage: "增强显示信息刷新失败，基础 VNC 连接不受影响。" });
    }
  }

  return { refreshSessionRemoteDisplayInventory };
}
