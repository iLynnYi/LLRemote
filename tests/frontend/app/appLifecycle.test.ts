import assert from "node:assert/strict";
import test from "node:test";
import {
  createVncAppLifecycle,
  type VncAppLifecycleRuntime,
} from "../../../src/app/composables/vncAppState/lifecycle.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}
const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

test("native app focus resumes permission startup and owns a late unlistener", async () => {
  const f = fixture();
  let focus: (focused: boolean) => void = () => {};
  let cleaned = 0;
  const ready = deferred<() => void>();
  f.runtime.listenFocusChanged = handler => { focus = handler; return ready.promise; };
  f.options.resumeLocalShareStart = async () => { f.calls.push("resume-share"); };
  f.options.onMainWindowFocusChanged = focused => { f.calls.push(`main-focus:${focused}`); };
  const lifecycle = createVncAppLifecycle(f.options, f.runtime);
  lifecycle.mount();
  focus(false);
  focus(true);
  assert.equal(f.calls.filter(call => call === "resume-share").length, 1);
  assert.deepEqual(f.calls.filter(call => call.startsWith("main-focus:")), ["main-focus:false", "main-focus:true"]);
  lifecycle.cleanup();
  ready.resolve(() => { cleaned++; });
  f.resolveListeners();
  await flush();
  focus(true);
  assert.equal(f.calls.filter(call => call === "resume-share").length, 1);
  assert.equal(cleaned, 1);
});

function fixture(external = false) {
  const calls: string[] = [];
  const emitted: string[] = [];
  const registrations: Array<{
    name: string; handler: (payload: unknown) => void;
    ready: ReturnType<typeof deferred<() => void>>; cleaned: number;
  }> = [];
  const closeReady = deferred<() => void>();
  let closeHandler: Parameters<VncAppLifecycleRuntime["listenCloseRequested"]>[0] | undefined;
  let closeCleaned = 0;
  let fullscreenHandler: () => void = () => undefined;
  let fullscreenCleaned = 0;
  const bodyStates: boolean[] = [];
  const runtime: VncAppLifecycleRuntime = {
    listenEvent: (name, handler) => {
      const registration = { name, handler, ready: deferred<() => void>(), cleaned: 0 };
      registrations.push(registration);
      return registration.ready.promise;
    },
    listenCloseRequested: (handler) => { closeHandler = handler; return closeReady.promise; },
    emitDockRequest: async (name) => { emitted.push(name); },
    observeFullscreen: (handler) => {
      fullscreenHandler = handler;
      return () => { fullscreenCleaned += 1; };
    },
    setExternalWindowBody: (enabled) => { bodyStates.push(enabled); },
  };
  const action = (name: string) => async () => { calls.push(name); };
  const maps = Array.from({ length: 3 }, () => {
    const listeners = new Map<string, () => void>();
    listeners.set("session-1", () => { calls.push("session-unlisten"); });
    return listeners;
  });
  const options: Parameters<typeof createVncAppLifecycle>[0] = {
    activeWorkspace: { value: "hosts" },
    copyLocalVncServerConnectionProfile: action("copy"),
    disconnectAllSessionConnections: action("disconnect-all"),
    disposeExternalWindows: action("dispose-windows"),
    disconnectLocalVncServerConnection: action("disconnect-device"),
    dockExternalWindowSession: action("dock"),
    externalWindowDockUnlisteners: maps[0]!,
    externalWindowInputUnlisteners: maps[1]!,
    externalWindowStateUnlisteners: maps[2]!,
    externalWindowLocation: external ? {
      sessionId: "session-1", target: "localhost:5900", host: "localhost", port: 5900,
      readonly: false, windowInstanceId: "instance-1",
    } : null,
    loadHostResources: action("load-hosts"),
    localVncServerOverview: { value: { activeConnectionDevices: [{ id: "device-1" }] } },
    openAboutSettings: action("open-about"),
    port: { value: 0 },
    refreshLocalShareTray: action("refresh-share"),
    refreshRemoteSessionCoreSnapshots: action("refresh-core"),
    selectLocalShareWorkspace: action("select-share"),
    selectWorkspace: action("select-settings"),
    stopDurationTimer: () => { calls.push("stop-timer"); },
    stopFullscreenExitShortcutListener: () => { calls.push("stop-shortcut"); },
    stopLocalVncServer: action("stop-share"),
    syncFullscreenState: () => { calls.push("fullscreen"); },
    syncTrayState: action("sync-tray"),
    host: { value: "" },
    startLocalShareFromTray: action("start-share"),
  };
  return {
    calls, emitted, registrations, runtime, options, maps, bodyStates,
    close: () => closeHandler!, closeCleaned: () => closeCleaned,
    resolveClose: () => closeReady.resolve(() => { closeCleaned += 1; }),
    fullscreen: () => fullscreenHandler(), fullscreenCleaned: () => fullscreenCleaned,
    resolveListeners: () => registrations.forEach((entry) => entry.ready.resolve(() => { entry.cleaned += 1; })),
  };
}

test("aggregate Core events refresh snapshots and stop having effects after cleanup", async () => {
  const f = fixture();
  const lifecycle = createVncAppLifecycle(f.options, f.runtime);
  lifecycle.mount();
  for (const name of ["remote-session-status", "remote-session-surface"]) {
    f.registrations.find(entry => entry.name === name)!.handler(undefined);
  }
  assert.equal(f.calls.filter(call => call === "refresh-core").length, 3);
  lifecycle.cleanup();
  f.resolveListeners();
  await flush();
  f.registrations.find(entry => entry.name === "remote-session-status")!.handler(undefined);
  assert.equal(f.calls.filter(call => call === "refresh-core").length, 3);
});

test("app cleanup owns late registrations and prevents old events or remount from starting work", async () => {
  const f = fixture();
  const lifecycle = createVncAppLifecycle(f.options, f.runtime);
  lifecycle.mount();
  lifecycle.mount();
  const first = f.registrations[0]!;
  first.ready.resolve(() => { first.cleaned += 1; });
  await flush();
  first.handler(undefined);
  assert.equal(f.calls.filter((call) => call === "refresh-core").length, 2);
  for (const name of ["app-action-open-about", "tray-action-start-local-share", "tray-action-open-settings", "tray-action-disconnect-local-connection", "session-window-dock-requested"]) {
    f.registrations.find((entry) => entry.name === name)!.handler(name === "session-window-dock-requested"
      ? { sessionId: "session-1", windowInstanceId: "instance-1" } : "device-1");
  }
  for (const action of ["open-about", "start-share", "select-settings", "disconnect-device", "dock"]) {
    assert.equal(f.calls.filter((call) => call === action).length, 1);
  }
  lifecycle.cleanup();
  f.resolveListeners();
  await flush();
  assert.ok(f.registrations.every((entry) => entry.cleaned === 1));
  assert.equal(f.registrations.length, 10);
  assert.equal(f.fullscreenCleaned(), 1);
  assert.ok(f.maps.every((listeners) => listeners.size === 0));
  const afterCleanup = [...f.calls];
  for (const entry of f.registrations) {
    entry.handler(entry.name === "session-window-dock-requested"
      ? { sessionId: "session-1", windowInstanceId: "instance-1" } : "device-1");
  }
  f.fullscreen();
  lifecycle.cleanup();
  lifecycle.mount();
  await flush();
  assert.deepEqual(f.calls, afterCleanup);
  assert.equal(f.registrations.length, 10);
  assert.equal(f.fullscreenCleaned(), 1);
  assert.deepEqual(f.bodyStates, [false]);
});

test("external close stops its second dock emit after cleanup and releases a late native registration", async () => {
  const f = fixture(true);
  const firstEmit = deferred<void>();
  f.runtime.emitDockRequest = async (name, request) => {
    assert.equal(request.windowInstanceId, "instance-1");
    f.emitted.push(name);
    if (name === "session-window-dock-requested") await firstEmit.promise;
  };
  const lifecycle = createVncAppLifecycle(f.options, f.runtime);
  lifecycle.mount();
  assert.equal(f.registrations.some(entry => entry.name === "app-action-open-about"), false);
  let prevented = 0;
  const closing = f.close()({ preventDefault: () => { prevented += 1; } });
  assert.equal(prevented, 1);
  assert.deepEqual(f.emitted, ["session-window-dock-requested"]);
  lifecycle.cleanup();
  firstEmit.resolve();
  f.resolveClose();
  f.resolveListeners();
  await Promise.all([closing, flush()]);
  assert.deepEqual(f.emitted, ["session-window-dock-requested"]);
  assert.equal(f.closeCleaned(), 1);
  assert.ok(f.registrations.every((entry) => entry.cleaned === 1));
  await f.close()({ preventDefault: () => { prevented += 1; } });
  assert.equal(prevented, 1);
  assert.deepEqual(f.bodyStates, [true, false]);
});
