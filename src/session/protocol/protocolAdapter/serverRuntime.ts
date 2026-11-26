export type ServerDesktopState = "waiting" | "available" | "unavailable" | "captureUnavailable" | "recovering";
export type SecureAttentionOutcome = "submitted" | "unsupported" | "denied" | "failed" | "unconfirmed";
export type ServerRuntimeState = { desktop: ServerDesktopState; secureAttentionAvailable: boolean; recoveryId: number };
export type ServerRuntimeNotice =
  | { kind: "desktop"; desktop: Exclude<ServerDesktopState, "recovering">; secureAttentionAvailable: boolean }
  | { kind: "secureAttentionResponse"; requestId: number; result: Exclude<SecureAttentionOutcome, "unconfirmed"> };

/** Per-connection runtime state. Availability clears only after a fresh Surface
 * commit; action replies match a request ID and never claim an observed SAS. */
export function createServerRuntime(options: {
  sendRequest: (requestId: number) => Promise<void>;
  refresh: () => void;
  onChanged?: () => void;
  timeoutMs?: number;
}) {
  let state: ServerRuntimeState | null = null;
  let closed = false;
  let nextId = 0;
  let recoveryId = 0;
  let pending: { id: number; promise: Promise<SecureAttentionOutcome>; finish: (value: SecureAttentionOutcome) => void } | null = null;
  return {
    get state() { return state; },
    consume(notice: ServerRuntimeNotice) {
      if (closed) return;
      if (notice.kind === "secureAttentionResponse") {
        if (pending?.id === notice.requestId) pending.finish(notice.result);
        return;
      }
      const recovering = notice.desktop === "available" && state !== null && state.desktop !== "available";
      const refresh = recovering && state?.desktop !== "recovering";
      if (refresh) recoveryId++;
      const next = { secureAttentionAvailable: notice.secureAttentionAvailable, desktop: recovering ? "recovering" as const : notice.desktop, recoveryId };
      const changed = state?.desktop !== next.desktop || state?.secureAttentionAvailable !== next.secureAttentionAvailable;
      state = next;
      if (changed) options.onChanged?.();
      if (refresh) options.refresh();
    },
    framePresented(id = state?.recoveryId): boolean {
      if (closed || state?.desktop !== "recovering" || id !== state.recoveryId) return false;
      state = { ...state, desktop: "available" };
      options.onChanged?.();
      return true;
    },
    request(): Promise<SecureAttentionOutcome> {
      if (closed) return Promise.resolve("failed");
      if (!state) return Promise.resolve("unconfirmed");
      if (!state.secureAttentionAvailable) return Promise.resolve("unsupported");
      if (pending) return pending.promise;
      const id = nextId = nextId % 0xffff_ffff + 1;
      let resolve!: (result: SecureAttentionOutcome) => void;
      const promise = new Promise<SecureAttentionOutcome>((done) => { resolve = done; });
      const timer = setTimeout(() => pending?.id === id && pending.finish("unconfirmed"), options.timeoutMs ?? 5000);
      pending = { id, promise, finish(value) { clearTimeout(timer); pending = null; resolve(value); } };
      void options.sendRequest(id).catch(() => { if (pending?.id === id) pending.finish("failed"); });
      return promise;
    },
    dispose() { closed = true; state = null; pending?.finish("failed"); },
  };
}
