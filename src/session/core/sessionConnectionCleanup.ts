import type { RemoteSessionState } from "./sessionManager.ts";

const cleanupStates: ReadonlySet<RemoteSessionState> = new Set([
  "authentication failed",
  "security failure",
  "disconnected",
  "disconnected unexpectedly",
  "failed",
  "closed",
]);

export function shouldCleanupConnectionForSessionState(state: RemoteSessionState): boolean {
  return cleanupStates.has(state);
}
