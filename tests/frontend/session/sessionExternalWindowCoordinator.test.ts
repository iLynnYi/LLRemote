import assert from "node:assert/strict";
import test from "node:test";
import {
  useSessionExternalWindowCoordinator,
  type SessionExternalWindowCreation,
  type SessionExternalWindowHandle,
} from "../../../src/features/sessions/composables/useSessionExternalWindowCoordinator.ts";
import {
  sessionExternalWindowDockEvent,
  sessionExternalWindowFocusEvent,
  type SessionExternalWindowRequest,
} from "../../../src/session/external-window/sessionExternalWindow.ts";
import type { SessionExternalInputForwarderOptions } from "../../../src/features/sessions/composables/useSessionExternalInputBridge.ts";
import type { SessionSurfaceAttachRequest } from "../../../src/session/protocol/remoteSessionCore.ts";
import type { ProtocolConnection } from "../../../src/session/protocol/protocolAdapter.ts";
import { createInitialConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";
import { sessionExternalWindowStateRequestEvent } from "../../../src/session/external-window/sessionExternalWindowState.ts";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function flush() {
  for (let index = 0; index < 16; index += 1) await Promise.resolve();
}

function fixture() {
  const ids = { value: new Set<string>() };
  const handles = new Map<string, SessionExternalWindowHandle>();
  const dockListeners = new Map<string, () => void>();
  const inputListeners = new Map<string, () => void>();
  const stateListeners = new Map<string, () => void>();
  const attached: SessionSurfaceAttachRequest[] = [];
  const detached: string[] = [];
  const requests: SessionExternalWindowRequest[] = [];
  const callbacks: Array<{ name: string; callback: (payload: unknown) => void; cleaned: number }> = [];
  const inputs: SessionExternalInputForwarderOptions[] = [];
  const errors: string[] = [];
  const destroyed: number[] = [];
  const fullscreen: boolean[] = [];
  const focused: string[] = [];
  let published = 0;
  let refreshed = 0;
  let inputCleaned = 0;
  let recordedInput = 0;
  // The coordinator only returns this reference; forwarding is tested by the
  // input bridge's own behavior tests.
  const connection = { forwardExternalInput: () => undefined } as ProtocolConnection;
  const runtime = {
    createWindow(request: SessionExternalWindowRequest): SessionExternalWindowCreation {
      const index = requests.length;
      requests.push(request);
      return {
        handle: {
          destroy: async () => { destroyed.push(index); },
          setFullscreen: async (value) => { fullscreen.push(value); },
        },
        ready: Promise.resolve(),
      };
    },
    listenEvent: async (name: string, callback: (payload: unknown) => void) => {
      const registration = { name, callback, cleaned: 0 };
      callbacks.push(registration);
      return () => { registration.cleaned += 1; };
    },
    startInputForwarder: async (options: SessionExternalInputForwarderOptions) => {
      inputs.push(options);
      return () => { inputCleaned += 1; };
    },
    focusWindow: async (sessionId: string) => { focused.push(sessionId); },
  };
  const options: Parameters<typeof useSessionExternalWindowCoordinator>[0] = {
    externalWindowSessionIds: ids,
    externalWindowHandles: handles,
    externalWindowDockUnlisteners: dockListeners,
    externalWindowInputUnlisteners: inputListeners,
    externalWindowStateUnlisteners: stateListeners,
    externalWindowLocation: null,
    fullscreenSessionId: { value: null },
    attachRemoteSessionCoreSurface: async (request) => {
      attached.push(request);
      return { ...request, active: true, surfaceUrl: "ws://127.0.0.1/surface" };
    },
    detachRemoteSessionCoreSurface: async (_sessionId, surfaceId) => {
      detached.push(surfaceId);
      return {
        sessionId: "session-1", target: "localhost:5900", state: "connected",
        connectionModel: createInitialConnectionModel(), displaySource: "vnc", inputSource: "vnc", surfaces: [],
      };
    },
    getExternalWindowLocation: (sessionId) => ({
      sessionId, target: "localhost:5900", host: "localhost", port: 5900,
      readonly: false,
    }),
    getProtocolConnection: () => connection,
    getWorkspaceRenderSlot: () => ({
      id: "session-1", label: "session-1", target: "localhost:5900", state: "connected",
      active: true, liveCanvas: true, closable: true, renderState: "visible", visible: true,
      externalWindowCandidate: !ids.value.has("session-1"),
      externalWindowOwned: ids.value.has("session-1"),
    }),
    recordMainViewportInput: () => { recordedInput += 1; },
    refreshLiveDisplays: async () => { refreshed += 1; },
    refreshSessionTabs: () => undefined,
    setNativeWindowFullscreen: async () => undefined,
    stopFullscreenExitShortcutListener: () => undefined,
    onExternalWindowError: (message) => { errors.push(message); },
    publishExternalWindowState: () => { published += 1; },
    runtime,
  };
  return {
    options, ids, handles, dockListeners, inputListeners, stateListeners,
    attached, detached, requests, callbacks, inputs, errors, destroyed, fullscreen, focused,
    published: () => published, refreshed: () => refreshed, inputCleaned: () => inputCleaned,
    recordedInput: () => recordedInput,
  };
}

test("external recovery refresh and frame acknowledgement match the active recovery only", async () => {
  const f = fixture();
  let refreshes = 0;
  const presented: number[] = [];
  const connection = f.options.getProtocolConnection("session-1")!;
  connection.getServerRuntimeState = () => ({ desktop: "recovering", recoveryId: 2, secureAttentionAvailable: false });
  connection.refreshDisplay = () => { refreshes++; };
  connection.notifySurfacePresented = (id) => { presented.push(id); };
  const coordinator = useSessionExternalWindowCoordinator(f.options);
  await coordinator.openExternalWindowSession("session-1");
  const listener = f.callbacks.find((item) => item.name === sessionExternalWindowStateRequestEvent("session-1"))!;
  listener.callback({ sessionId: "other", recoveryId: 2, framePresented: true });
  listener.callback({ sessionId: "session-1", recoveryId: 1, framePresented: true });
  listener.callback({ sessionId: "session-1", recoveryId: 2, framePresented: false });
  listener.callback({ sessionId: "session-1", recoveryId: 2, framePresented: true });
  assert.equal(refreshes, 1);
  assert.deepEqual(presented, [2]);
  await coordinator.dispose();
  listener.callback({ sessionId: "session-1", recoveryId: 2, framePresented: true });
  assert.deepEqual(presented, [2]);
});

test("concurrent external opens own one attach and native creation", async () => {
  const f = fixture();
  const attaching = deferred<Awaited<ReturnType<NonNullable<typeof f.options.attachRemoteSessionCoreSurface>>>>();
  f.options.attachRemoteSessionCoreSurface = async (request) => {
    f.attached.push(request);
    return attaching.promise;
  };
  const coordinator = useSessionExternalWindowCoordinator(f.options);
  const first = coordinator.openExternalWindowSession("session-1");
  const second = coordinator.openExternalWindowSession("session-1", true);
  await flush();
  assert.equal(f.attached.length, 1);
  attaching.resolve({ ...f.attached[0], active: true, surfaceUrl: "ws://127.0.0.1/surface" });
  await Promise.all([first, second]);
  assert.equal(f.requests.length, 1);
  assert.equal(f.requests[0].fullscreen, true);
  assert.equal(f.handles.size, 1);
  await coordinator.openExternalWindowSession("session-1", true);
  assert.equal(f.requests.length, 1);
  assert.ok(f.focused.length > 0);
});

test("only the current child window can claim focus and its first input activates before forwarding", async () => {
  const f = fixture();
  const ownership: Array<[string, boolean]> = [];
  f.options.onExternalWindowFocusChanged = (sessionId, focused) => ownership.push([sessionId, focused]);
  const coordinator = useSessionExternalWindowCoordinator(f.options);
  await coordinator.openExternalWindowSession("session-1");
  const focusListener = f.callbacks.find((entry) => entry.name === sessionExternalWindowFocusEvent("session-1"));
  assert.ok(focusListener);
  const request = f.requests[0]!;
  const before = ownership.length;
  focusListener.callback({ sessionId: "session-1", windowInstanceId: "old-window", focused: false });
  assert.equal(ownership.length, before);
  focusListener.callback({ sessionId: "session-1", windowInstanceId: request.windowInstanceId, focused: false });
  assert.deepEqual(ownership.at(-1), ["session-1", false]);
  f.inputs[0]!.activateInput?.("session-1");
  assert.deepEqual(ownership.at(-1), ["session-1", true]);
  await coordinator.dockExternalWindowSession("session-1");
  assert.deepEqual(ownership.at(-1), ["session-1", false]);
  const afterDock = ownership.length;
  focusListener.callback({ sessionId: "session-1", windowInstanceId: request.windowInstanceId, focused: true });
  assert.equal(ownership.length, afterDock);
});

test("dock during attach prevents publication and gates replacement until detach finishes", async () => {
  const f = fixture();
  const attaching = deferred<Awaited<ReturnType<NonNullable<typeof f.options.attachRemoteSessionCoreSurface>>>>();
  const detaching = deferred<void>();
  const originalAttach = f.options.attachRemoteSessionCoreSurface!;
  f.options.attachRemoteSessionCoreSurface = async (request) => {
    if (f.attached.length === 0) {
      f.attached.push(request);
      return attaching.promise;
    }
    return originalAttach(request);
  };
  const originalDetach = f.options.detachRemoteSessionCoreSurface!;
  f.options.detachRemoteSessionCoreSurface = async (sessionId, surfaceId) => {
    const result = await originalDetach(sessionId, surfaceId);
    if (f.detached.length === 1) await detaching.promise;
    return result;
  };
  const coordinator = useSessionExternalWindowCoordinator(f.options);
  const first = coordinator.openExternalWindowSession("session-1");
  await flush();
  const dock = coordinator.dockExternalWindowSession("session-1");
  const replacement = coordinator.openExternalWindowSession("session-1");
  await flush();
  assert.equal(f.attached.length, 1);
  attaching.resolve({ ...f.attached[0], active: true, surfaceUrl: "ws://127.0.0.1/surface" });
  await flush();
  assert.equal(f.requests.length, 0);
  assert.equal(f.detached.length, 1);
  assert.equal(f.ids.value.size, 0);
  detaching.resolve();
  await Promise.all([first, dock, replacement]);
  assert.equal(f.attached.length, 2);
  assert.equal(f.requests.length, 1);
  assert.equal(f.ids.value.size, 1);

  const native = fixture();
  const ready = deferred<void>();
  const destroying = deferred<void>();
  const originalCreate = native.options.runtime!.createWindow!;
  native.options.runtime!.createWindow = (request) => {
    const creation = originalCreate(request);
    if (native.requests.length !== 1) return creation;
    return {
      ready: ready.promise,
      handle: { ...creation.handle, destroy: async () => {
        await destroying.promise;
        await creation.handle.destroy();
      } },
    };
  };
  const nativeCoordinator = useSessionExternalWindowCoordinator(native.options);
  const pending = nativeCoordinator.openExternalWindowSession("session-1");
  await flush();
  assert.equal(native.requests.length, 1);
  assert.equal(native.ids.value.size, 0);
  const removing = nativeCoordinator.removeExternalWindowSession("session-1");
  const retry = nativeCoordinator.openExternalWindowSession("session-1");
  ready.resolve();
  await flush();
  assert.equal(native.requests.length, 1);
  assert.equal(native.attached.length, 1);
  assert.equal(native.ids.value.size, 0);
  assert.equal(native.detached.length, 1);
  destroying.resolve();
  await Promise.all([pending, removing, retry]);
  assert.equal(native.requests.length, 2);
  assert.equal(native.attached.length, 2);
  assert.deepEqual(native.destroyed, [0]);
  assert.equal(native.ids.value.size, 1);
});

test("late listeners and old dock requests cannot mutate a replacement owner", async () => {
  const f = fixture();
  const listening = deferred<() => void>();
  let cleaned = 0;
  const originalListen = f.options.runtime!.listenEvent!;
  f.options.runtime!.listenEvent = async (name, callback) => {
    if (f.callbacks.length === 0) {
      f.callbacks.push({ name, callback, cleaned: 0 });
      return listening.promise;
    }
    return originalListen(name, callback);
  };
  const coordinator = useSessionExternalWindowCoordinator(f.options);
  const first = coordinator.openExternalWindowSession("session-1");
  await flush();
  const oldInstance = f.requests[0].windowInstanceId;
  assert.equal(typeof oldInstance, "string");
  const dock = coordinator.dockExternalWindowSession("session-1");
  const repeatedDock = coordinator.dockExternalWindowSession("session-1");
  const replacement = coordinator.openExternalWindowSession("session-1");
  listening.resolve(() => { cleaned += 1; });
  await Promise.all([first, dock, repeatedDock, replacement]);
  assert.equal(cleaned, 1);
  assert.deepEqual(f.destroyed, [0]);
  assert.equal(f.detached.length, 1);
  const afterReplacement = f.published();
  f.callbacks[0].callback({ sessionId: "session-1", windowInstanceId: oldInstance });
  const currentDock = f.callbacks.filter((entry) => entry.name === sessionExternalWindowDockEvent("session-1")).at(-1)!;
  currentDock.callback({ sessionId: "session-1", windowInstanceId: oldInstance });
  await coordinator.dockExternalWindowSession("session-1", oldInstance);
  await flush();
  assert.equal(f.ids.value.size, 1);
  assert.equal(f.published(), afterReplacement);
  assert.deepEqual(f.destroyed, [0]);
  const oldInput = f.inputs[0];
  assert.ok(oldInput.getConnection("session-1"));
  const focusing = deferred<void>();
  f.options.runtime!.focusWindow = async () => focusing.promise;
  const pendingFocus = coordinator.focusExternalWindowSession("session-1");
  await flush();
  const closing = coordinator.removeExternalWindowSession("session-1");
  const next = coordinator.openExternalWindowSession("session-1");
  assert.equal(oldInput.getConnection("session-1"), undefined);
  oldInput.recordInput?.("session-1", "keyboard");
  oldInput.onRemoteDesktopShortcutUnavailable?.("session-1", "next");
  await flush();
  assert.equal(f.requests.length, 2);
  assert.deepEqual(f.destroyed, [0]);
  assert.equal(f.recordedInput(), 0);
  focusing.resolve();
  await Promise.all([pendingFocus, closing, next]);
  assert.equal(f.inputCleaned(), 1);
  assert.equal(f.requests.length, 3);
  assert.deepEqual(f.destroyed, [0, 1]);
  const shutdown = coordinator.dispose();
  assert.equal(f.ids.value.size, 0);
  await shutdown;
  await coordinator.openExternalWindowSession("session-1");
  assert.equal(f.requests.length, 3);
  assert.deepEqual(f.destroyed, [0, 1, 2]);
});

test("native creation failures reclaim only their surface and cleanup failures keep reuse closed", async () => {
  for (const failure of ["synchronous", "ready"] as const) {
    const f = fixture();
    const originalCreate = f.options.runtime!.createWindow!;
    f.options.runtime!.createWindow = (request) => {
      if (failure === "synchronous") throw new Error("sensitive native detail");
      const creation = originalCreate(request);
      return { ...creation, ready: Promise.reject(new Error("sensitive native detail")) };
    };
    const coordinator = useSessionExternalWindowCoordinator(f.options);
    await coordinator.openExternalWindowSession("session-1");
    assert.equal(f.ids.value.size, 0);
    assert.equal(f.handles.size, 0);
    assert.equal(f.detached.length, 1);
    assert.equal(f.errors.length, 1);
    assert.doesNotMatch(f.errors[0], /sensitive/);
    f.options.runtime!.createWindow = originalCreate;
    await coordinator.openExternalWindowSession("session-1");
    assert.equal(f.ids.value.size, 1);
    await coordinator.removeExternalWindowSession("session-1");
  }
  const f = fixture();
  const originalCreate = f.options.runtime!.createWindow!;
  let destroyFails = true;
  f.options.runtime!.createWindow = (request) => {
    const creation = originalCreate(request);
    return { ...creation, handle: { ...creation.handle, destroy: async () => {
      if (destroyFails) throw new Error("sensitive native detail");
      await creation.handle.destroy();
    } } };
  };
  const coordinator = useSessionExternalWindowCoordinator(f.options);
  await coordinator.openExternalWindowSession("session-1");
  await assert.rejects(coordinator.removeExternalWindowSession("session-1"), /external_window_cleanup_failed/);
  await coordinator.openExternalWindowSession("session-1");
  assert.equal(f.requests.length, 1);
  assert.equal(f.errors.length, 1);
  destroyFails = false;
  await coordinator.removeExternalWindowSession("session-1");
  await coordinator.openExternalWindowSession("session-1");
  assert.equal(f.requests.length, 2);
});

test("display refresh can reclaim a zombie without waiting on its own opening", { timeout: 1000 }, async () => {
  const f = fixture();
  let cleanup: Promise<void> | undefined;
  const owner = useSessionExternalWindowCoordinator({
    ...f.options,
    refreshLiveDisplays: async () => {
      cleanup = owner.removeExternalWindowSession("session-1");
      await cleanup;
    },
  });
  await owner.openExternalWindowSession("session-1");
  await cleanup;
  assert.equal(f.ids.value.size, 0);
  assert.equal(f.detached.length, 1);
  assert.deepEqual(f.destroyed, [0]);
});
