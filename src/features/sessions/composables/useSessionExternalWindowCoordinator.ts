import type { Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { ProtocolConnection } from "@session/protocol/protocolAdapter";
import type { SessionWorkspaceRenderSlot } from "@session/viewport/sessionWorkspace";
import {
  createSessionExternalWindowRequest,
  isSessionExternalWindowDockRequest,
  isSessionExternalWindowFocus,
  sessionExternalWindowDockEvent,
  sessionExternalWindowFocusEvent,
  sessionExternalWindowLabel,
  type SessionExternalWindowDockRequest,
  type SessionExternalWindowLocation,
  type SessionExternalWindowRequest,
} from "../../../session/external-window/sessionExternalWindow.ts";
import {
  createSessionSurfaceId,
} from "../../../session/viewport/sessionSurface.ts";
import type {
  RemoteSessionSnapshot,
  RemoteSessionSurfaceSnapshot,
  SessionSurfaceAttachRequest,
} from "@session/protocol/remoteSessionCore";
import { startSessionExternalInputForwarder } from "./useSessionExternalInputBridge.ts";
import { sessionExternalWindowStateRequestEvent } from "../../../session/external-window/sessionExternalWindowState.ts";
import { createSessionExternalWindow } from "./sessionExternalWindowRuntime.ts";

export type SessionExternalWindowHandle = {
  destroy: () => Promise<void>;
  setFullscreen: (fullscreen: boolean) => Promise<void>;
};

export type SessionExternalWindowCreation = {
  handle: SessionExternalWindowHandle;
  ready: Promise<void>;
};

type ExternalWindowCoordinatorOptions = {
  externalWindowDockUnlisteners: Map<string, () => void>;
  externalWindowHandles: Map<string, SessionExternalWindowHandle>;
  externalWindowInputUnlisteners: Map<string, () => void>;
  externalWindowStateUnlisteners: Map<string, () => void>;
  externalWindowLocation: SessionExternalWindowLocation | null;
  externalWindowSessionIds: Ref<Set<string>>;
  fullscreenSessionId: Ref<string | null>;
  attachRemoteSessionCoreSurface?: (request: SessionSurfaceAttachRequest) => Promise<RemoteSessionSurfaceSnapshot>;
  detachRemoteSessionCoreSurface?: (sessionId: string, surfaceId: string) => Promise<RemoteSessionSnapshot>;
  getExternalWindowLocation: (sessionId: string) => SessionExternalWindowLocation | null;
  getProtocolConnection: (sessionId: string) => ProtocolConnection | undefined;
  getWorkspaceRenderSlot: (sessionId: string) => SessionWorkspaceRenderSlot;
  recordMainViewportInput: (sessionId: string, type: "pointer" | "wheel" | "keyboard") => void;
  refreshLiveDisplays: () => Promise<void>;
  refreshSessionTabs: () => void;
  setNativeWindowFullscreen: (fullscreen: boolean) => Promise<void>;
  onExternalWindowError?: (message: string) => void;
  onExternalWindowFocusChanged?: (sessionId: string, focused: boolean) => void;
  publishExternalWindowState?: (sessionId: string) => void;
  stopFullscreenExitShortcutListener: () => void;
  runtime?: {
    createWindow?: (request: SessionExternalWindowRequest) => SessionExternalWindowCreation;
    listenEvent?: (name: string, handler: (payload: unknown) => void) => Promise<() => void>;
    startInputForwarder?: typeof startSessionExternalInputForwarder;
    focusWindow?: (sessionId: string) => Promise<void>;
  };
};

export function useSessionExternalWindowCoordinator(options: ExternalWindowCoordinatorOptions) {
  const {
    externalWindowDockUnlisteners,
    externalWindowHandles,
    externalWindowInputUnlisteners,
    externalWindowStateUnlisteners,
    externalWindowLocation,
    externalWindowSessionIds,
    fullscreenSessionId,
    attachRemoteSessionCoreSurface,
    detachRemoteSessionCoreSurface,
    getExternalWindowLocation,
    getProtocolConnection,
    getWorkspaceRenderSlot,
    recordMainViewportInput,
    refreshLiveDisplays,
    refreshSessionTabs,
    setNativeWindowFullscreen,
    onExternalWindowError,
    publishExternalWindowState,
    stopFullscreenExitShortcutListener,
  } = options;
  type Attempt = {
    sessionId: string;
    instanceId: string;
    request: SessionExternalWindowRequest;
    surfaceId: string;
    openingPromise: Promise<void>;
    creation: SessionExternalWindowCreation | null;
    attached: boolean;
    created: boolean;
    canceled: boolean;
    cleanupFailed: boolean;
    errorReported: boolean;
    fullscreenPending: boolean;
    focusTask: Promise<void> | null;
    closePromise: Promise<boolean> | null;
    listeners: Set<() => void>;
  };
  const attempts = new Map<string, Attempt>();
  const focusUnlisteners = new Map<string, () => void>();
  let disposed = false;
  let disposePromise: Promise<void> | null = null;
  const listenEvent = options.runtime?.listenEvent ?? ((name, handler) =>
    listen<unknown>(name, (event) => handler(event.payload)));

  function isLive(attempt: Attempt): boolean {
    return !disposed && !attempt.canceled && attempts.get(attempt.sessionId) === attempt;
  }

  function reportError(attempt: Attempt, message: string) {
    if (attempt.errorReported) return;
    attempt.errorReported = true;
    onExternalWindowError?.(message);
  }

  // Each registration owns its Map entry, even if it finishes after revocation.
  async function ownListener(attempt: Attempt, registry: Map<string, () => void>, register: () => Promise<() => void>) {
    const unlisten = await register();
    let released = false;
    const release = () => {
      if (released) return;
      released = true;
      attempt.listeners.delete(release);
      if (registry.get(attempt.sessionId) === release) registry.delete(attempt.sessionId);
      unlisten();
    };
    if (!isLive(attempt)) {
      release();
      return;
    }
    attempt.listeners.add(release);
    registry.set(attempt.sessionId, release);
  }

  function cleanupAttemptListeners(attempt: Attempt) {
    for (const release of [...attempt.listeners]) {
      try { release(); } catch { reportError(attempt, "独立窗口监听清理失败。"); }
    }
  }

  function cleanupExternalWindowDockListener(sessionId: string) {
    const attempt = attempts.get(sessionId);
    if (attempt) cleanupAttemptListeners(attempt);
  }

  function revoke(attempt: Attempt) {
    attempt.canceled = true;
    options.onExternalWindowFocusChanged?.(attempt.sessionId, false);
    cleanupAttemptListeners(attempt);
    if (externalWindowHandles.get(attempt.sessionId) === attempt.creation?.handle) {
      externalWindowHandles.delete(attempt.sessionId);
    }
    if (attempts.get(attempt.sessionId) === attempt && externalWindowSessionIds.value.has(attempt.sessionId)) {
      const next = new Set(externalWindowSessionIds.value);
      next.delete(attempt.sessionId);
      externalWindowSessionIds.value = next;
      if (!disposed) refreshSessionTabs();
    }
  }

  // Keep the owner until native attach/create and destruction have settled. IDs
  // and labels are reusable, so a callback gate alone cannot make reuse safe.
  function closeAttempt(attempt: Attempt): Promise<boolean> {
    revoke(attempt);
    if (attempt.closePromise) return attempt.closePromise;
    const closing = Promise.resolve().then(async () => {
      await attempt.openingPromise;
      await attempt.focusTask;
      cleanupAttemptListeners(attempt);
      let failed = false;
      if (attempt.attached) {
        try {
          if (!detachRemoteSessionCoreSurface) throw new Error("external_surface_detach_unavailable");
          await detachRemoteSessionCoreSurface(attempt.sessionId, attempt.surfaceId);
          attempt.attached = false;
        } catch {
          failed = true;
        }
      }
      if (attempt.created && attempt.creation) {
        try {
          await attempt.creation.handle.destroy();
          attempt.created = false;
        } catch {
          failed = true;
        }
      }
      attempt.cleanupFailed = failed;
      if (failed) {
        reportError(attempt, "独立窗口资源尚未释放，请再次停靠或关闭。");
        return false;
      }
      if (attempts.get(attempt.sessionId) === attempt) attempts.delete(attempt.sessionId);
      return true;
    });
    attempt.closePromise = closing;
    void closing.then((closed) => {
      if (!closed && attempt.closePromise === closing) attempt.closePromise = null;
    });
    return closing;
  }

  async function removeExternalWindowSession(sessionId: string): Promise<void> {
    const attempt = attempts.get(sessionId);
    if (attempt && !await closeAttempt(attempt)) throw new Error("external_window_cleanup_failed");
  }

  async function dockExternalWindowSession(sessionId: string, windowInstanceId?: string): Promise<void> {
    const attempt = attempts.get(sessionId);
    if (!attempt || (windowInstanceId !== undefined && attempt.instanceId !== windowInstanceId)) return;
    await closeAttempt(attempt);
    if (!disposed) {
      refreshSessionTabs();
      await refreshLiveDisplays();
    }
  }

  function focusAttempt(attempt: Attempt, fullscreen = false): Promise<void> {
    if (fullscreen) attempt.fullscreenPending = true;
    if (attempt.focusTask) return attempt.focusTask;
    const task = Promise.resolve().then(async () => {
      do {
        if (!isLive(attempt) || !attempt.created || !attempt.creation) return;
        if (attempt.fullscreenPending) {
          attempt.fullscreenPending = false;
          await attempt.creation.handle.setFullscreen(true).catch(() => undefined);
        }
        if (!isLive(attempt)) return;
        if (options.runtime?.focusWindow) {
          await options.runtime.focusWindow(attempt.sessionId);
        } else {
          await invoke("focus_session_external_window", { label: sessionExternalWindowLabel(attempt.sessionId) });
        }
        if (isLive(attempt)) options.onExternalWindowFocusChanged?.(attempt.sessionId, true);
      } while (isLive(attempt) && attempt.fullscreenPending);
    }).catch(() => undefined);
    attempt.focusTask = task;
    void task.then(() => {
      if (attempt.focusTask === task) attempt.focusTask = null;
    });
    return task;
  }

  async function openAttempt(attempt: Attempt) {
    try {
      if (!isLive(attempt)) return;
      await attachRemoteSessionCoreSurface!({
        sessionId: attempt.sessionId, surfaceId: attempt.surfaceId,
        owner: "external", windowLabel: attempt.request.label, active: true,
      });
      attempt.attached = true;
      if (!isLive(attempt)) return;
      attempt.request = { ...attempt.request, fullscreen: attempt.fullscreenPending };
      attempt.creation = (options.runtime?.createWindow ?? createSessionExternalWindow)(attempt.request);
      await attempt.creation.ready;
      attempt.created = true;
      if (!isLive(attempt)) return;
      if (attempt.request.fullscreen) attempt.fullscreenPending = false;
      externalWindowHandles.set(attempt.sessionId, attempt.creation.handle);
      externalWindowSessionIds.value = new Set([...externalWindowSessionIds.value, attempt.sessionId]);
      if (fullscreenSessionId.value === attempt.sessionId) {
        fullscreenSessionId.value = null;
        stopFullscreenExitShortcutListener();
        await setNativeWindowFullscreen(false);
        if (!isLive(attempt)) return;
      }
      await ownListener(attempt, externalWindowDockUnlisteners, () => listenEvent(
        sessionExternalWindowDockEvent(attempt.sessionId), (payload) => {
          if (!isLive(attempt) || !isSessionExternalWindowDockRequest(payload)
            || payload.sessionId !== attempt.sessionId || payload.windowInstanceId !== attempt.instanceId) return;
          void dockExternalWindowSession(attempt.sessionId, attempt.instanceId).catch(() => undefined);
        },
      ));
      if (!isLive(attempt)) return;
      await ownListener(attempt, focusUnlisteners, () => listenEvent(
        sessionExternalWindowFocusEvent(attempt.sessionId), (payload) => {
          if (!isLive(attempt) || !isSessionExternalWindowFocus(payload)
            || payload.sessionId !== attempt.sessionId || payload.windowInstanceId !== attempt.instanceId) return;
          options.onExternalWindowFocusChanged?.(attempt.sessionId, payload.focused);
        },
      ));
      if (!isLive(attempt)) return;
      await ownListener(attempt, externalWindowInputUnlisteners, () =>
        (options.runtime?.startInputForwarder ?? startSessionExternalInputForwarder)({
          sessionId: attempt.sessionId,
          windowInstanceId: attempt.instanceId,
          listen,
          getConnection: (sessionId) => isLive(attempt) ? getProtocolConnection(sessionId) : undefined,
          activateInput: (sessionId) => { if (isLive(attempt)) options.onExternalWindowFocusChanged?.(sessionId, true); },
          recordInput: (sessionId, type) => { if (isLive(attempt)) recordMainViewportInput(sessionId, type); },
          onRemoteDesktopShortcutUnavailable: (sessionId) => {
            if (isLive(attempt)) recordMainViewportInput(sessionId, "keyboard");
          },
        }));
      if (!isLive(attempt)) return;
      await ownListener(attempt, externalWindowStateUnlisteners, () => listenEvent(
        sessionExternalWindowStateRequestEvent(attempt.sessionId), (payload) => {
          if (!isLive(attempt)) return;
          if (typeof payload === "object" && payload !== null && "sessionId" in payload
            && payload.sessionId === attempt.sessionId && "recoveryId" in payload && "framePresented" in payload) {
            const connection = getProtocolConnection(attempt.sessionId);
            const state = connection?.getServerRuntimeState?.();
            if (state?.desktop === "recovering" && state.recoveryId === payload.recoveryId) {
              if (payload.framePresented === true) connection?.notifySurfacePresented?.(state.recoveryId);
              else if (payload.framePresented === false) connection?.refreshDisplay();
            }
          }
          publishExternalWindowState?.(attempt.sessionId);
        },
      ));
      if (!isLive(attempt)) return;
      publishExternalWindowState?.(attempt.sessionId);
      refreshSessionTabs();
      // Display refresh may disconnect a zombie session and wait for this owner.
      void refreshLiveDisplays().catch(() => undefined);
    } catch {
      if (!attempt.canceled) reportError(attempt, "独立窗口创建失败，请重试。");
      revoke(attempt);
    }
  }

  async function openExternalWindowSession(sessionId: string, fullscreen = false): Promise<void> {
    if (disposed) return;
    let attempt = attempts.get(sessionId);
    if (attempt?.canceled) {
      if (attempt.cleanupFailed || !await closeAttempt(attempt)) return;
      if (disposed) return;
      attempt = attempts.get(sessionId);
    }
    if (!attempt) {
      if (!getWorkspaceRenderSlot(sessionId).externalWindowCandidate) return;
      const location = getExternalWindowLocation(sessionId);
      if (!location) return;
      if (!attachRemoteSessionCoreSurface) {
        onExternalWindowError?.("Core Surface 服务不可用，无法创建独立窗口。");
        return;
      }
      const instanceId = crypto.randomUUID();
      const request = createSessionExternalWindowRequest({ ...location, windowInstanceId: instanceId }, { fullscreen });
      attempt = {
        sessionId, instanceId, request,
        surfaceId: createSessionSurfaceId({ sessionId, owner: "external", windowLabel: request.label }),
        openingPromise: Promise.resolve(), creation: null, attached: false, created: false,
        canceled: false, cleanupFailed: false, errorReported: false, fullscreenPending: fullscreen,
        focusTask: null, closePromise: null, listeners: new Set(),
      };
      attempts.set(sessionId, attempt);
      const owner = attempt;
      owner.openingPromise = Promise.resolve().then(() => openAttempt(owner));
    } else if (fullscreen) {
      attempt.fullscreenPending = true;
    }
    await attempt.openingPromise;
    if (attempt.canceled) {
      await closeAttempt(attempt);
      return;
    }
    if (isLive(attempt)) await focusAttempt(attempt);
  }

  async function focusExternalWindowSession(sessionId: string): Promise<void> {
    const attempt = attempts.get(sessionId);
    if (!attempt) return;
    await attempt.openingPromise;
    if (isLive(attempt)) await focusAttempt(attempt);
  }

  function dispose(): Promise<void> {
    if (disposePromise) return disposePromise;
    disposed = true;
    disposePromise = Promise.all([...attempts.values()].map((attempt) => closeAttempt(attempt))).then(() => undefined);
    return disposePromise;
  }

  async function toggleExternalWindowSession(sessionId: string) {
    if (externalWindowLocation?.sessionId === sessionId) {
      await requestDockExternalWindowSession(sessionId);
      return;
    }

    const slot = getWorkspaceRenderSlot(sessionId);
    if (slot.externalWindowOwned) {
      await dockExternalWindowSession(sessionId);
      return;
    }

    await openExternalWindowSession(sessionId);
  }

  async function requestDockExternalWindowSession(sessionId: string) {
    const windowInstanceId = externalWindowLocation?.windowInstanceId;
    if (!windowInstanceId || externalWindowLocation?.sessionId !== sessionId) return;
    const request: SessionExternalWindowDockRequest = { sessionId, windowInstanceId };
    await emit("session-window-dock-requested", request).catch(() => undefined);
    await emit(sessionExternalWindowDockEvent(sessionId), request).catch(() => undefined);
  }

  return {
    cleanupExternalWindowDockListener,
    dispose,
    dockExternalWindowSession,
    focusExternalWindowSession,
    openExternalWindowSession,
    removeExternalWindowSession,
    requestDockExternalWindowSession,
    toggleExternalWindowSession,
  };
}
