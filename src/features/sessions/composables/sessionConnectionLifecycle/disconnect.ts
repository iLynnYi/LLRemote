import type { ProtocolConnection } from "../../../../session/protocol/protocolAdapter.ts";

type SessionDisconnectorOptions = {
  getConnection: (sessionId: string) => ProtocolConnection | undefined;
  clearConnection: (sessionId: string) => Promise<void>;
  disconnectConnection: (connection: ProtocolConnection, sessionId: string) => Promise<void>;
  onDisconnected: (sessionId: string, connection: ProtocolConnection) => void | Promise<void>;
};

/** Keeps repeated disconnects waiting for the same UI and transport cleanup. */
export function createSessionDisconnector(options: SessionDisconnectorOptions) {
  const pending = new Map<string, Promise<void>>();

  return function disconnectSession(sessionId: string, capturedConnection?: ProtocolConnection): Promise<void> {
    const existing = pending.get(sessionId);
    if (existing) return existing;

    const connection = capturedConnection ?? options.getConnection(sessionId);
    const operation = Promise.resolve().then(async () => {
      const clearing = Promise.resolve().then(() => options.clearConnection(sessionId));
      const disconnecting = connection
        ? Promise.resolve().then(() => options.disconnectConnection(connection, sessionId))
        : Promise.resolve();
      const results = await Promise.allSettled([clearing, disconnecting]);
      for (const result of results) {
        if (result.status === "rejected") throw result.reason;
      }
      if (connection) await options.onDisconnected(sessionId, connection);
    });
    pending.set(sessionId, operation);
    const release = () => {
      if (pending.get(sessionId) === operation) pending.delete(sessionId);
    };
    void operation.then(release, release);
    return operation;
  };
}

/** User actions report session end before cancellation revokes the Agent channel. */
export function createSessionUserDisconnector(options: {
  getConnection: (sessionId: string) => ProtocolConnection | undefined;
  shouldNotify: (sessionId: string) => boolean;
  onFailure: (sessionId: string) => void;
  cancel: (sessionId: string, removeCore: boolean) => Promise<boolean>;
}) {
  const pending = new Map<string, Promise<boolean>>();
  return {
    isPending: (sessionId: string) => pending.has(sessionId),
    disconnect(sessionId: string, removeCore = false): Promise<boolean> {
      const existing = pending.get(sessionId);
      if (existing) {
        // Closing a tab can escalate a concurrent plain disconnect.
        return removeCore ? existing.then(() => options.cancel(sessionId, true)) : existing;
      }
      const connection = options.getConnection(sessionId);
      const shouldNotify = connection && options.shouldNotify(sessionId);
      const operation = Promise.resolve().then(async () => {
        if (shouldNotify && connection.notifyRemoteSessionEnd) {
          try {
            await connection.notifyRemoteSessionEnd();
          } catch {
            options.onFailure(sessionId);
          }
        }
        return options.cancel(sessionId, removeCore);
      });
      pending.set(sessionId, operation);
      const release = () => { if (pending.get(sessionId) === operation) pending.delete(sessionId); };
      void operation.then(release, release);
      return operation;
    },
  };
}
