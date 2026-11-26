import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  sessionExternalWindowDockEvent,
  isSessionExternalWindowDockRequest,
  type SessionExternalWindowLocation,
  type SessionExternalWindowDockRequest,
} from "../../../session/external-window/sessionExternalWindow.ts";

type WindowCloseRequest = { preventDefault: () => void };
export type VncAppLifecycleRuntime = {
  listenEvent: (name: string, handler: (payload: unknown) => void) => Promise<() => void>;
  listenCloseRequested: (handler: (event: WindowCloseRequest) => Promise<void>) => Promise<() => void>;
  listenFocusChanged?: (handler: (focused: boolean) => void) => Promise<() => void>;
  emitDockRequest: (name: string, payload: SessionExternalWindowDockRequest) => Promise<void>;
  observeFullscreen: (handler: () => void) => () => void;
  setExternalWindowBody: (enabled: boolean) => void;
};

const nativeRuntime: VncAppLifecycleRuntime = {
  listenEvent: (name, handler) => listen<unknown>(name, (event) => handler(event.payload)),
  listenCloseRequested: (handler) => getCurrentWindow().onCloseRequested(handler),
  listenFocusChanged: (handler) => getCurrentWindow().onFocusChanged(({ payload }) => handler(payload)),
  emitDockRequest: (name, payload) => emit(name, payload),
  observeFullscreen: (handler) => {
    document.addEventListener("fullscreenchange", handler);
    return () => document.removeEventListener("fullscreenchange", handler);
  },
  setExternalWindowBody: (enabled) => document.body.classList.toggle("session-external-window-body", enabled),
};

/** Owns app listeners across pending registration and final teardown. */
export function createVncAppLifecycle(options: {
  activeWorkspace: { value: string };
  onMainWindowFocusChanged?: (focused: boolean) => void;
  copyLocalVncServerConnectionProfile: () => Promise<void>;
  disconnectAllSessionConnections: () => Promise<void>;
  disposeExternalWindows: () => Promise<void>;
  disconnectLocalVncServerConnection: (connection: unknown) => Promise<void>;
  dockExternalWindowSession: (sessionId: string, windowInstanceId?: string) => Promise<void>;
  externalWindowDockUnlisteners: Map<string, () => void>;
  externalWindowInputUnlisteners: Map<string, () => void>;
  externalWindowStateUnlisteners: Map<string, () => void>;
  externalWindowLocation: SessionExternalWindowLocation | null;
  loadHostResources: () => Promise<void>;
  localVncServerOverview: { value: { activeConnectionDevices: Array<{ id: string }> } | null };
  openAboutSettings: () => Promise<void>;
  port: { value: number };
  refreshLocalShareTray: () => Promise<void>;
  refreshRemoteSessionCoreSnapshots: () => Promise<void>;
  selectLocalShareWorkspace: () => Promise<void>;
  selectWorkspace: (workspace: "settings") => Promise<void>;
  stopDurationTimer: () => void;
  stopFullscreenExitShortcutListener: () => void;
  stopLocalVncServer: () => Promise<void>;
  syncFullscreenState: () => void;
  syncTrayState: () => Promise<void>;
  host: { value: string };
  startLocalShareFromTray: () => Promise<void>;
  resumeLocalShareStart?: () => Promise<void>;
}, runtime: VncAppLifecycleRuntime = nativeRuntime) {
  let mounted = false;
  let disposed = false;
  const ownedListeners = new Set<() => void>();
  const isLive = () => mounted && !disposed;

  function release(unlisten: () => void) {
    try { void Promise.resolve(unlisten()).catch(() => undefined); } catch { /* Continue other cleanup. */ }
  }

  async function ownRegistration(register: () => Promise<() => void>) {
    if (!isLive()) return;
    try {
      const unlisten = await register();
      if (!isLive()) release(unlisten);
      else ownedListeners.add(unlisten);
    } catch { /* A failed subscription does not restart an app action. */ }
  }

  function listenEvent(name: string, handler: (payload: unknown) => void) {
    void ownRegistration(() => runtime.listenEvent(name, (payload) => {
      if (isLive()) handler(payload);
    }));
  }

  function runAction(action: () => Promise<void>) {
    if (isLive()) void action();
  }

  function cleanup() {
    if (disposed) return;
    disposed = true;
    void options.disposeExternalWindows().catch(() => undefined);
    options.stopDurationTimer();
    options.stopFullscreenExitShortcutListener();
    const listeners = [...ownedListeners];
    ownedListeners.clear();
    listeners.forEach(release);
    runtime.setExternalWindowBody(false);
    for (const registry of [options.externalWindowDockUnlisteners, options.externalWindowInputUnlisteners, options.externalWindowStateUnlisteners]) {
      const registered = [...registry.values()];
      registry.clear();
      registered.forEach(release);
    }
    void options.disconnectAllSessionConnections().catch(() => undefined);
  }

  function mount() {
    if (mounted || disposed) return;
    mounted = true;
    ownedListeners.add(runtime.observeFullscreen(() => { if (isLive()) options.syncFullscreenState(); }));
    listenEvent("remote-session-status", () => runAction(options.refreshRemoteSessionCoreSnapshots));
    listenEvent("remote-session-surface", () => runAction(options.refreshRemoteSessionCoreSnapshots));

    if (!options.externalWindowLocation) {
      listenEvent("app-action-open-about", () => runAction(options.openAboutSettings));
      const registerFocus = runtime.listenFocusChanged;
      if (registerFocus && (options.resumeLocalShareStart || options.onMainWindowFocusChanged)) {
        void ownRegistration(() => registerFocus(focused => {
          if (!isLive()) return;
          options.onMainWindowFocusChanged?.(focused);
          if (focused && options.resumeLocalShareStart) runAction(options.resumeLocalShareStart);
        }));
      }
      mountTrayListeners();
      runAction(options.syncTrayState);
    }
    runAction(options.refreshRemoteSessionCoreSnapshots);
    if (options.externalWindowLocation) {
      mountExternalWindow();
      return;
    }
    listenEvent("session-window-dock-requested", (payload) => {
      if (isSessionExternalWindowDockRequest(payload)) {
        runAction(() => options.dockExternalWindowSession(payload.sessionId, payload.windowInstanceId).catch(() => undefined));
      }
    });
    runAction(options.loadHostResources);
  }

  function mountTrayListeners() {
    listenEvent("tray-action-open-local-share", () => runAction(options.selectLocalShareWorkspace));
    listenEvent("tray-action-start-local-share", () => runAction(options.startLocalShareFromTray));
    listenEvent("tray-action-stop-local-share", () => runAction(options.stopLocalVncServer));
    listenEvent("tray-action-copy-local-share", () => runAction(options.copyLocalVncServerConnectionProfile));
    listenEvent("tray-action-disconnect-local-connection", (payload) => {
      const connection = options.localVncServerOverview.value?.activeConnectionDevices.find((item) => item.id === payload);
      if (connection) runAction(() => options.disconnectLocalVncServerConnection(connection));
    });
    listenEvent("tray-action-open-settings", () => runAction(() => options.selectWorkspace("settings")));
  }

  function mountExternalWindow() {
    const location = options.externalWindowLocation;
    if (!location || !isLive()) return;
    runtime.setExternalWindowBody(true);
    options.activeWorkspace.value = "sessions";
    options.host.value = location.host;
    options.port.value = location.port;
    void ownRegistration(() => runtime.listenCloseRequested(async (event) => {
      if (!isLive() || !location.windowInstanceId) return;
      event.preventDefault();
      const request: SessionExternalWindowDockRequest = { sessionId: location.sessionId, windowInstanceId: location.windowInstanceId };
      await runtime.emitDockRequest("session-window-dock-requested", request).catch(() => undefined);
      if (!isLive()) return;
      await runtime.emitDockRequest(sessionExternalWindowDockEvent(location.sessionId), request).catch(() => undefined);
    }));
  }

  return { cleanup, mount };
}
