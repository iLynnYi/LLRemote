import type { ProtocolConnection } from "../protocol/protocolAdapter.ts";

export type SessionConnectionEntry = {
  sessionId: string;
  connection: ProtocolConnection;
};

export type SessionConnectionRegistry = {
  readonly activeCanvasSessionId: string | null;
  readonly activeCanvasConnection: SessionConnectionEntry | null;
  attach: (sessionId: string, connection: ProtocolConnection) => SessionConnectionEntry;
  get: (sessionId: string) => SessionConnectionEntry | undefined;
  detach: (sessionId: string) => SessionConnectionEntry | undefined;
  activateCanvas: (sessionId: string) => SessionConnectionEntry | undefined;
  clearActiveCanvas: () => void;
  shouldDisconnectBeforeActivating: (sessionId: string) => boolean;
  listLiveSessionIds: () => string[];
  findZombieSessionIds: (allowedSessionIds: string[]) => string[];
  detachZombieConnections: (allowedSessionIds: string[]) => SessionConnectionEntry[];
};

export function createSessionConnectionRegistry(): SessionConnectionRegistry {
  const connections = new Map<string, ProtocolConnection>();
  let activeCanvasSessionId: string | null = null;

  function entryFor(sessionId: string): SessionConnectionEntry | undefined {
    const connection = connections.get(sessionId);

    if (!connection) {
      return undefined;
    }

    return { sessionId, connection };
  }

  return {
    get activeCanvasSessionId() {
      return activeCanvasSessionId;
    },

    get activeCanvasConnection() {
      return activeCanvasSessionId ? entryFor(activeCanvasSessionId) ?? null : null;
    },

    attach(sessionId, connection) {
      connections.set(sessionId, connection);
      return { sessionId, connection };
    },

    get(sessionId) {
      return entryFor(sessionId);
    },

    detach(sessionId) {
      const entry = entryFor(sessionId);

      if (!entry) {
        return undefined;
      }

      connections.delete(sessionId);

      if (activeCanvasSessionId === sessionId) {
        activeCanvasSessionId = null;
      }

      return entry;
    },

    activateCanvas(sessionId) {
      const entry = entryFor(sessionId);

      if (!entry) {
        return undefined;
      }

      activeCanvasSessionId = sessionId;
      return entry;
    },

    clearActiveCanvas() {
      activeCanvasSessionId = null;
    },

    shouldDisconnectBeforeActivating(_sessionId) {
      return false;
    },

    listLiveSessionIds() {
      return [...connections.keys()];
    },

    findZombieSessionIds(allowedSessionIds) {
      const allowed = new Set(allowedSessionIds);
      return [...connections.keys()].filter((sessionId) => !allowed.has(sessionId));
    },

    detachZombieConnections(allowedSessionIds) {
      return this.findZombieSessionIds(allowedSessionIds)
        .map((sessionId) => this.detach(sessionId))
        .filter((entry): entry is SessionConnectionEntry => Boolean(entry));
    },
  };
}
