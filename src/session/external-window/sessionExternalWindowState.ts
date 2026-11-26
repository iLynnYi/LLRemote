import type { SessionExternalWindowControlSnapshot } from "./sessionExternalWindow.ts";
import type { RemoteViewportStatusHudSummary } from "../viewport/remoteViewportStatusBar.ts";

export type SessionExternalWindowStatePayload = {
  sessionId: string;
  updatedAt: number;
  inputEnabled: boolean;
  controlSnapshot: SessionExternalWindowControlSnapshot;
};

export function sessionExternalWindowStateEvent(sessionId: string): string {
  return `session-window-state:${sessionId}`;
}

export function sessionExternalWindowStateRequestEvent(sessionId: string): string {
  return `session-window-state-request:${sessionId}`;
}

/** The external Surface acknowledges only its current recovery after presenting
 * a frame; requesting a refresh also covers pixels that preceded the snapshot. */
export function createExternalDesktopRecovery(request: (id: number, framePresented: boolean) => Promise<void>) {
  let currentId = 0;
  let requestedId = 0;
  let presentedId = 0;
  return {
    consume(hud: RemoteViewportStatusHudSummary) {
      currentId = hud.runtimeDesktop === "recovering" ? hud.runtimeRecoveryId ?? 0 : 0;
      if (!currentId) { requestedId = 0; presentedId = 0; return; }
      if (currentId === requestedId) return;
      const id = requestedId = currentId;
      void request(id, false).catch(() => { if (requestedId === id) requestedId = 0; });
    },
    framePresented() {
      if (!currentId || currentId !== requestedId || currentId === presentedId) return;
      const id = presentedId = currentId;
      void request(id, true).catch(() => { if (presentedId === id) presentedId = 0; });
    },
  };
}
