import type { SessionQualityProfile } from "../viewport/sessionDisplay.ts";
import { redactSensitiveText, type ConnectionIssue, type DisconnectReasonCode } from "../diagnostics/sessionDiagnostics.ts";

export type RemoteProtocol = "vnc";

export type RemoteSessionState =
  | "idle"
  | "probing"
  | "connecting"
  | "connected"
  | "authentication failed"
  | "security failure"
  | "disconnected"
  | "disconnected unexpectedly"
  | "failed"
  | "closed";

export type RemoteSessionConfig = {
  protocol: RemoteProtocol;
  host: string;
  port: number;
  qualityProfile: SessionQualityProfile;
  /** Name captured at creation when connecting from a saved host. */
  savedHostName?: string;
};

export type RemoteSessionStateEntry = {
  state: RemoteSessionState;
  time: string;
  issue?: ConnectionIssue;
  disconnectReason?: DisconnectReasonCode;
};

export type RemoteSessionTransport = {
  id: string;
  surfaceUrl: string;
};

export type RemoteSession = RemoteSessionConfig & {
  id: string;
  target: string;
  state: RemoteSessionState;
  createdAt: string;
  connectedAt?: string;
  closedAt?: string;
  transport?: RemoteSessionTransport;
  lastError?: string;
  lastIssue?: ConnectionIssue;
  disconnectReason?: DisconnectReasonCode;
  stateHistory: RemoteSessionStateEntry[];
};

export type RemoteSessionTab = {
  id: string;
  label: string;
  /** Used only by the session rail; detail cards keep their existing label. */
  railLabel?: string;
  target: string;
  state: RemoteSessionState;
  active: boolean;
  liveCanvas: boolean;
  closable: boolean;
};

export type SessionManagerOptions = {
  now?: () => Date;
  id?: () => string;
};

export type SessionManager = {
  readonly activeSession: RemoteSession | null;
  readonly sessions: RemoteSession[];
  create: (config: RemoteSessionConfig) => RemoteSession;
  findReusableByTarget: (config: Pick<RemoteSessionConfig, "protocol" | "host" | "port">) => RemoteSession | undefined;
  findRecoverableByTarget: (config: Pick<RemoteSessionConfig, "protocol" | "host" | "port">) => RemoteSession | undefined;
  activate: (sessionId: string) => RemoteSession | undefined;
  updateQualityProfile: (sessionId: string, profile: SessionQualityProfile) => RemoteSession | undefined;
  transition: (sessionId: string, nextState: RemoteSessionState, metadata?: RemoteSessionTransitionMetadata) => RemoteSession | undefined;
  attachTransport: (sessionId: string, transport: RemoteSessionTransport) => RemoteSession | undefined;
  fail: (sessionId: string, error: string | ConnectionIssue) => RemoteSession | undefined;
  close: (sessionId: string) => RemoteSession | undefined;
  closeActive: () => RemoteSession | undefined;
};

export type RemoteSessionTransitionMetadata = {
  issue?: ConnectionIssue;
  disconnectReason?: DisconnectReasonCode;
};

function defaultId(): string {
  return crypto.randomUUID();
}

function timestamp(options: SessionManagerOptions): string {
  return (options.now?.() ?? new Date()).toISOString();
}

function redactConnectionIssue(issue: ConnectionIssue): ConnectionIssue {
  return {
    ...issue,
    technicalMessage: issue.technicalMessage ? redactSensitiveText(issue.technicalMessage) : undefined,
  };
}

export function createRemoteSession(
  config: RemoteSessionConfig,
  options: SessionManagerOptions = {},
): RemoteSession {
  const createdAt = timestamp(options);

  return {
    ...config,
    id: options.id?.() ?? defaultId(),
    target: `${config.host}:${config.port}`,
    state: "idle",
    createdAt,
    stateHistory: [
      {
        state: "idle",
        time: createdAt,
      },
    ],
  };
}

export function summarizeSessionTabs(
  sessions: RemoteSession[],
  activeSessionId?: string,
  liveCanvasSessionIds?: string | string[],
): RemoteSessionTab[] {
  const liveCanvasIds = new Set(Array.isArray(liveCanvasSessionIds) ? liveCanvasSessionIds : liveCanvasSessionIds ? [liveCanvasSessionIds] : []);

  return sessions.map((session) => ({
    id: session.id,
    label: session.target,
    railLabel: session.savedHostName?.trim() || session.target,
    target: session.target,
    state: session.state,
    active: session.id === activeSessionId,
    liveCanvas: liveCanvasIds.has(session.id),
    closable: true,
  }));
}

export function createSessionManager(options: SessionManagerOptions = {}): SessionManager {
  const sessions: RemoteSession[] = [];
  let activeSession: RemoteSession | null = null;

  function findSession(sessionId: string): RemoteSession | undefined {
    return sessions.find((item) => item.id === sessionId);
  }

  function latestOpenSession(): RemoteSession | null {
    return [...sessions].reverse().find((item) => item.state !== "closed") ?? null;
  }

  function isReusableSession(session: RemoteSession): boolean {
    return ["probing", "connecting", "connected"].includes(session.state);
  }

  function isRecoverableSession(session: RemoteSession): boolean {
    return ["idle", "disconnected", "disconnected unexpectedly", "failed", "authentication failed", "security failure"].includes(session.state);
  }

  function transition(sessionId: string, nextState: RemoteSessionState, metadata: RemoteSessionTransitionMetadata = {}): RemoteSession | undefined {
    const session = findSession(sessionId);

    if (!session) {
      return undefined;
    }

    const time = timestamp(options);
    session.state = nextState;

    if (nextState === "probing" || nextState === "connecting" || nextState === "connected") {
      session.lastIssue = undefined;
      session.lastError = undefined;
      session.disconnectReason = undefined;
    }

    if (metadata.issue) {
      const redactedIssue = redactConnectionIssue(metadata.issue);
      session.lastIssue = redactedIssue;
      session.lastError = redactedIssue.message ?? redactedIssue.messageKey;
    }

    if (metadata.disconnectReason) {
      session.disconnectReason = metadata.disconnectReason;
    }

    if (nextState === "connected" && !session.connectedAt) {
      session.connectedAt = time;
    }

    if ((nextState === "closed" || nextState === "disconnected") && !session.closedAt) {
      session.closedAt = time;
    }

    if (nextState === "closed" || nextState === "disconnected" || nextState === "disconnected unexpectedly" || nextState === "failed") {
      session.transport = undefined;
    }

    session.stateHistory.push({
      state: nextState,
      time,
      issue: metadata.issue ? redactConnectionIssue(metadata.issue) : undefined,
      disconnectReason: metadata.disconnectReason,
    });

    return session;
  }

  return {
    get activeSession() {
      return activeSession;
    },

    get sessions() {
      return sessions;
    },

    create(config: RemoteSessionConfig) {
      const session = createRemoteSession(config, options);
      sessions.push(session);
      activeSession = session;
      return session;
    },

    findReusableByTarget(config) {
      const target = `${config.host}:${config.port}`;
      return [...sessions].reverse().find((session) =>
        session.protocol === config.protocol && session.target === target && isReusableSession(session)
      );
    },

    findRecoverableByTarget(config) {
      const target = `${config.host}:${config.port}`;
      return [...sessions].reverse().find((session) =>
        session.protocol === config.protocol && session.target === target && isRecoverableSession(session)
      );
    },

    activate(sessionId: string) {
      const session = findSession(sessionId);

      if (!session || session.state === "closed") {
        return undefined;
      }

      activeSession = session;
      return session;
    },

    updateQualityProfile(sessionId: string, profile: SessionQualityProfile) {
      const session = findSession(sessionId);

      if (!session) {
        return undefined;
      }

      session.qualityProfile = profile;
      return session;
    },

    transition,

    attachTransport(sessionId: string, transport: RemoteSessionTransport) {
      const session = findSession(sessionId);

      if (!session) {
        return undefined;
      }

      session.transport = transport;
      return session;
    },

    fail(sessionId: string, error: string | ConnectionIssue) {
      const session = transition(sessionId, "failed");

      if (!session) {
        return undefined;
      }

      if (typeof error === "string") {
        session.lastError = redactSensitiveText(error);
      } else {
        const issue = redactConnectionIssue(error);
        session.lastIssue = issue;
        session.lastError = issue.message ?? issue.messageKey;
        const latestHistoryEntry = session.stateHistory[session.stateHistory.length - 1];
        if (latestHistoryEntry) {
          latestHistoryEntry.issue = issue;
        }
      }
      return session;
    },

    close(sessionId: string) {
      const session = findSession(sessionId);

      if (!session) {
        return undefined;
      }

      const closedAt = timestamp(options);
      session.state = "closed";
      session.closedAt = session.closedAt ?? closedAt;
      session.transport = undefined;
      session.stateHistory.push({
        state: "closed",
        time: closedAt,
      });

      const index = sessions.findIndex((item) => item.id === sessionId);
      if (index >= 0) {
        sessions.splice(index, 1);
      }

      if (activeSession?.id === sessionId) {
        activeSession = latestOpenSession();
      }

      return session;
    },

    closeActive() {
      if (!activeSession) {
        return undefined;
      }

      return this.close(activeSession.id);
    },
  };
}
