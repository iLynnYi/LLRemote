import type { ClipboardAccess } from "./clipboardTypes.ts";

export type ClipboardSessionAdapter = {
  sendText: (text: string) => void | Promise<void>;
  dispose?: () => void;
};

type SessionEntry = { adapter: ClipboardSessionAdapter; readonly: boolean };

export type ClipboardCoordinator = {
  register: (sessionId: string, adapter: ClipboardSessionAdapter, readonly: boolean) => Promise<void>;
  unregister: (sessionId: string) => void;
  setActiveSession: (sessionId: string | null) => Promise<void>;
  setReadonly: (sessionId: string, readonly: boolean) => void;
  receiveRemoteText: (sessionId: string, text: string) => Promise<boolean>;
  syncClientClipboard: () => Promise<void>;
  dispose: () => void;
};

export type ClipboardCoordinatorOptions = {
  onWarning?: (message: string) => void;
};

function isUnavailableClipboardRead(error: unknown): boolean {
  const message = String(error).toLowerCase();
  return message.includes("clipboard contents were not available")
    || message.includes("clipboard is empty")
    || message.includes("requested format");
}

/** Coordinates the single system clipboard across all connected sessions. */
export function createClipboardCoordinator(
  clipboard: ClipboardAccess | undefined,
  options: ClipboardCoordinatorOptions = {},
): ClipboardCoordinator {
  const sessions = new Map<string, SessionEntry>();
  let activeSessionId: string | null = null;
  let lastClientText = "";
  let pendingRemoteText: string | null = null;
  let remoteWriteInFlight = false;
  let activeSessionRevision = 0;
  let interval: ReturnType<typeof setInterval> | null = null;

  async function readClientClipboardText(): Promise<string | undefined> {
    if (!clipboard?.readText) return undefined;

    try {
      return await clipboard.readText();
    } catch (error) {
      if (!isUnavailableClipboardRead(error)) {
        options.onWarning?.(`System clipboard read failed: ${String(error)}`);
      }
      return undefined;
    }
  }

  async function send(entry: SessionEntry, text: string) {
    if (!entry.readonly && text) await entry.adapter.sendText(text);
  }
  async function syncClientClipboard() {
    if (remoteWriteInFlight) return;
    const text = await readClientClipboardText();
    if (text === undefined) return;
    if (!text || text === lastClientText || text === pendingRemoteText) {
      if (text === pendingRemoteText) {
        lastClientText = text;
        pendingRemoteText = null;
      }
      return;
    }
    lastClientText = text;
    await Promise.all([...sessions.values()].map((entry) => send(entry, text)));
  }
  function ensurePolling() {
    if (!interval && clipboard?.readText) interval = setInterval(() => { void syncClientClipboard(); }, 1000);
  }
  return {
    async register(sessionId, adapter, readonly) {
      sessions.set(sessionId, { adapter, readonly });
      ensurePolling();
      if (!readonly) {
        const text = await readClientClipboardText();
        if (text) { lastClientText = text; await send({ adapter, readonly }, text); }
      }
    },
    unregister(sessionId) { sessions.get(sessionId)?.adapter.dispose?.(); sessions.delete(sessionId); },
    async setActiveSession(sessionId) {
      activeSessionId = sessionId;
      const revision = ++activeSessionRevision;
      const entry = sessionId ? sessions.get(sessionId) : undefined;
      if (entry && !entry.readonly) {
        const text = await readClientClipboardText();
        if (text === undefined) return;
        if (revision !== activeSessionRevision || !text || remoteWriteInFlight || text === pendingRemoteText) return;
        // A copy can occur just before the next poll. It is still a Client-originated
        // change and must be broadcast, rather than being consumed by the Active switch.
        if (text !== lastClientText) {
          lastClientText = text;
          await Promise.all([...sessions.values()].map((candidate) => send(candidate, text)));
          return;
        }
        await send(entry, text);
      }
    },
    setReadonly(sessionId, readonly) { const entry = sessions.get(sessionId); if (entry) entry.readonly = readonly; },
    async receiveRemoteText(sessionId, text) {
      if (!text || sessionId !== activeSessionId || sessions.get(sessionId)?.readonly || !clipboard) return false;
      remoteWriteInFlight = true;
      try {
        await clipboard.writeText(text);
        lastClientText = text;
        pendingRemoteText = text;
      } finally {
        remoteWriteInFlight = false;
      }
      return true;
    },
    syncClientClipboard,
    dispose() { if (interval) clearInterval(interval); interval = null; for (const id of sessions.keys()) this.unregister(id); },
  };
}
