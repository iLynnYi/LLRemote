import type { Translate } from "@app/types";
import type { DiagnosticEventContext, EventLogLevel } from "@diagnostics/eventLog";
import type {
  ConnectionIssue,
  DisconnectReasonCode,
} from "@session/diagnostics/sessionDiagnostics";
import {
  createConnectionIssue,
  shouldIgnoreFollowupRemoteDisconnectIssue,
  shouldReplaceConnectionIssue,
} from "../../../../session/diagnostics/sessionDiagnostics.ts";
import type { RemoteSession, RemoteSessionState, SessionManager } from "@session/core/sessionManager";

export type LocalizedConnectionIssue = ConnectionIssue & {
  message: string;
};

export type ConnectionIssueHandlerOptions = {
  errorMessage: { value: string };
  findSession: (sessionId: string) => RemoteSession | undefined;
  localizeConnectionIssue: (issue: ConnectionIssue) => LocalizedConnectionIssue;
  logEvent: (level: EventLogLevel, source: string, message: string, context?: DiagnosticEventContext) => void;
  notify: (content: string, tone: "error") => void;
  sessionManager: SessionManager;
  syncSessionAfterIssue: (sessionId: string, session: RemoteSession | undefined) => void;
  t: Translate;
};

export function createConnectionIssueHandler(options: ConnectionIssueHandlerOptions) {
  return (sessionId: string, issue: ConnectionIssue) => {
    const currentSession = options.findSession(sessionId);
    if (currentSession && shouldIgnoreFollowupRemoteDisconnectIssue({
      state: currentSession.state,
      lastIssue: currentSession.lastIssue,
      incomingIssue: issue,
    })) {
      options.logEvent("info", options.t("log.sourceSession"), `Ignored follow-up remote disconnect for ${currentSession.target}; existing_failure_code=${currentSession.lastIssue?.code}.`, { origin: "conversation", conversationId: sessionId, target: currentSession.target, eventKey: "connection_issue_ignored" });
      return;
    }
    if (currentSession?.lastIssue && !shouldReplaceConnectionIssue(currentSession.lastIssue, issue)) {
      options.logEvent("info", options.t("log.sourceSession"), `Ignored lower-priority connection issue for ${currentSession.target}; existing_failure_code=${currentSession.lastIssue.code}; incoming_failure_code=${issue.code}.`, { origin: "conversation", conversationId: sessionId, target: currentSession.target, eventKey: "connection_issue_ignored" });
      return;
    }

    const localizedIssue = options.localizeConnectionIssue(issue);
    const nextState = stateForConnectionIssue(issue);
    const session = options.sessionManager.transition(sessionId, nextState, {
      issue: localizedIssue,
      disconnectReason: disconnectReasonForIssue(issue),
    });
    options.syncSessionAfterIssue(sessionId, session);
    options.errorMessage.value = localizedIssue.message;
    if (issue.code !== "authentication_cancelled" && issue.code !== "connection_cancelled") {
      options.notify(localizedIssue.message, "error");
    }
    if (issue.logLevel !== "none") {
      options.logEvent(issue.logLevel, options.t("log.sourceSession"), `连接问题：${localizedIssue.message}（阶段：${issue.phase}，代码：${issue.code}）${issue.technicalMessage ? `；详情：${issue.technicalMessage}` : ""}`, { origin: "conversation", conversationId: sessionId, target: currentSession?.target, eventKey: "connection_issue", userMessage: localizedIssue.message });
    }
  };
}

export function createRawConnectionErrorHandler(applyConnectionIssue: (sessionId: string, issue: ConnectionIssue) => void) {
  return (sessionId: string, message: string) => {
    applyConnectionIssue(sessionId, createConnectionIssue({
      phase: "failed",
      reason: message,
    }));
  };
}

export function shouldIgnoreStatusAfterConnectionIssue(session: RemoteSession | undefined, nextState: RemoteSessionState): boolean {
  if (nextState !== "disconnected" && nextState !== "disconnected unexpectedly") {
    return false;
  }
  if (!session?.lastIssue) {
    return false;
  }
  return ["authentication failed", "security failure", "failed", "disconnected unexpectedly"].includes(session.state);
}

export function stateForConnectionIssue(issue: ConnectionIssue): RemoteSessionState {
  if (issue.code === "authentication_failed" || issue.code === "credentials_required" || issue.code === "authentication_cancelled") {
    return "authentication failed";
  }

  if (issue.code === "security_failure" || issue.code === "unsupported_security") {
    return "security failure";
  }

  if (issue.phase === "disconnected") {
    return "disconnected unexpectedly";
  }

  return "failed";
}

export function disconnectReasonForIssue(issue: ConnectionIssue): DisconnectReasonCode | undefined {
  if (issue.code === "remote_closed") return "remote_closed";
  if (issue.code === "target_unreachable") return "network_lost";
  return undefined;
}
