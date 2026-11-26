import assert from "node:assert/strict";
import test from "node:test";

import { useSessionExternalSurface } from "../../../src/features/sessions/composables/useSessionExternalSurface.ts";
import { createInitialConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";
import type { RemoteSessionSnapshot } from "../../../src/session/protocol/remoteSessionCore.ts";
import type { createCoreSurfaceSession } from "../../../src/session/renderer/coreSurfaceSession.ts";

type SurfaceOptions = Parameters<typeof createCoreSurfaceSession>[0];
const target = {} as HTMLElement;
const snapshot: RemoteSessionSnapshot = {
  sessionId: "session-1", target: "test-target", state: "connected",
  connectionModel: createInitialConnectionModel(), displaySource: "vnc", inputSource: "vnc",
  surfaces: [{ surfaceId: "external-1", owner: "external", windowLabel: "session-1", active: true,
    surfaceUrl: "ws://127.0.0.1:61001/surface/external-1" }],
};
const frame = { frameId: 1n, width: 1920, height: 1080 };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}

test("external Surface failure clears state and input while old callbacks cannot replace a later owner", async () => {
  const surfaces: Array<{ callbacks: SurfaceOptions; canvas: HTMLCanvasElement; disconnects: number }> = [];
  const errors: string[] = [];
  const owner = useSessionExternalSurface({
    sessionId: "session-1", getMountTarget: () => target, onError: (code) => { errors.push(code); },
    getSnapshot: async () => snapshot,
    createSurfaceSession: (callbacks) => {
      const surface = { callbacks, canvas: { width: 1920, height: 1080 } as HTMLCanvasElement, disconnects: 0 };
      surfaces.push(surface);
      return { canvas: surface.canvas, disconnect() { surface.disconnects += 1; } };
    },
  });
  await owner.load();
  const first = surfaces[0]!;
  first.callbacks.onFrameCommitted?.(frame);
  assert.equal(owner.active.value, true);
  assert.equal(owner.canForwardInput(true), true);
  assert.equal(owner.canForwardInput(false), false);
  first.callbacks.onError?.("surface_canvas_context_lost");
  first.callbacks.onFrameCommitted?.({ ...frame, frameId: 2n });
  first.callbacks.onError?.("surface_websocket_closed");
  assert.deepEqual({ phase: owner.phase.value, active: owner.active.value, canvas: owner.canvas.value,
    commit: owner.commit.value, input: owner.canForwardInput(true), errors, disconnects: first.disconnects }, {
    phase: "failed", active: false, canvas: null, commit: null, input: false,
    errors: ["surface_canvas_context_lost"], disconnects: 1,
  });

  await owner.load();
  const second = surfaces[1]!;
  const nextFrame = { ...frame, frameId: 3n };
  second.callbacks.onFrameCommitted?.(nextFrame);
  first.callbacks.onError?.("surface_websocket_failed");
  first.callbacks.onFrameCommitted?.(frame);
  assert.equal(owner.phase.value, "ready");
  assert.equal(owner.canvas.value, second.canvas);
  assert.equal(owner.commit.value, nextFrame);
  assert.deepEqual(errors, ["surface_canvas_context_lost"]);
  owner.dispose();
  owner.dispose();
  second.callbacks.onFrameCommitted?.(frame);
  second.callbacks.onError?.("surface_websocket_closed");
  assert.deepEqual({ phase: owner.phase.value, canvas: owner.canvas.value, commit: owner.commit.value,
    disconnects: second.disconnects, input: owner.canForwardInput(true) }, {
    phase: "disposed", canvas: null, commit: null, disconnects: 1, input: false,
  });
});

test("late snapshots cannot create a Surface after disposal or replace a newer load", async () => {
  const delayed = deferred<RemoteSessionSnapshot | null>();
  const errors: string[] = [];
  let creations = 0;
  const owner = useSessionExternalSurface({
    sessionId: "session-1", getMountTarget: () => target, onError: (code) => { errors.push(code); },
    getSnapshot: () => delayed.promise,
    createSurfaceSession: () => { creations += 1; return { canvas: {} as HTMLCanvasElement, disconnect() {} }; },
  });
  const loading = owner.load();
  owner.dispose();
  delayed.resolve(snapshot);
  await loading;
  await owner.load();
  assert.deepEqual({ phase: owner.phase.value, canvas: owner.canvas.value, creations, errors }, {
    phase: "disposed", canvas: null, creations: 0, errors: [],
  });

  const stale = deferred<RemoteSessionSnapshot | null>();
  let loads = 0;
  let replacementCreations = 0;
  const replacement = useSessionExternalSurface({
    sessionId: "session-1", getMountTarget: () => target, onError: (code) => { errors.push(code); },
    getSnapshot: () => ++loads === 1 ? stale.promise : Promise.resolve(snapshot),
    createSurfaceSession: ({ onFrameCommitted }) => {
      replacementCreations += 1;
      onFrameCommitted?.(frame);
      return { canvas: {} as HTMLCanvasElement, disconnect() {} };
    },
  });
  const oldLoad = replacement.load();
  await replacement.load();
  const replacementCanvas = replacement.canvas.value;
  stale.resolve(snapshot);
  await oldLoad;
  assert.equal(replacementCreations, 1);
  assert.equal(replacement.canvas.value, replacementCanvas);
  assert.equal(replacement.phase.value, "ready");
  replacement.dispose();
});

test("synchronous Surface callbacks and construction errors stay failed without late commits or raw error text", async () => {
  for (const mode of ["callback", "unknown-callback", "known-throw", "unknown-throw"] as const) {
    let callbacks: SurfaceOptions | undefined;
    let disconnects = 0;
    const errors: string[] = [];
    const expectedCode = mode === "callback" ? "surface_canvas_context_lost"
      : mode === "unknown-callback" ? "surface_packet_decode_failed"
      : mode === "known-throw" ? "surface_canvas_context_unavailable" : "surface_startup_failed";
    const owner = useSessionExternalSurface({
      sessionId: "session-1", getMountTarget: () => target, onError: (code) => { errors.push(code); },
      getSnapshot: async () => snapshot,
      createSurfaceSession: (options) => {
        callbacks = options;
        if (mode === "known-throw") throw new Error("surface_canvas_context_unavailable");
        if (mode === "unknown-throw") throw new Error(snapshot.surfaces[0]!.surfaceUrl);
        options.onError?.(mode === "unknown-callback" ? snapshot.surfaces[0]!.surfaceUrl : "surface_canvas_context_lost");
        options.onFrameCommitted?.(frame);
        return { canvas: {} as HTMLCanvasElement, disconnect() { disconnects += 1; } };
      },
    });
    await owner.load();
    callbacks?.onFrameCommitted?.(frame);
    callbacks?.onError?.("surface_websocket_failed");
    assert.deepEqual({ phase: owner.phase.value, canvas: owner.canvas.value, commit: owner.commit.value,
      errors, input: owner.canForwardInput(true), disconnects }, {
      phase: "failed", canvas: null, commit: null, errors: [expectedCode], input: false,
      disconnects: mode === "callback" || mode === "unknown-callback" ? 1 : 0,
    });
    owner.dispose();
  }
});

class VisibilityDocument extends EventTarget {
  visibilityState: DocumentVisibilityState = "visible";

  setVisibility(state: DocumentVisibilityState) {
    this.visibilityState = state;
    this.dispatchEvent(new Event("visibilitychange"));
  }
}

function visibilityFixture(getSnapshot: () => Promise<RemoteSessionSnapshot | null> = async () => snapshot) {
  const visibilityDocument = new VisibilityDocument();
  const focusTarget = new EventTarget();
  const surfaces: Array<{ callbacks: SurfaceOptions; disconnects: number }> = [];
  const errors: string[] = [];
  let releases = 0;
  const owner = useSessionExternalSurface({
    sessionId: "session-1", getMountTarget: () => target, getSnapshot,
    runtime: { document: visibilityDocument, window: focusTarget },
    onDeactivate: () => { releases += 1; },
    onError: (code) => errors.push(code),
    createSurfaceSession: (callbacks) => {
      const surface = { callbacks, disconnects: 0 };
      surfaces.push(surface);
      return { canvas: {} as HTMLCanvasElement, disconnect() { surface.disconnects += 1; } };
    },
  });
  return { owner, visibilityDocument, focusTarget, surfaces, errors, releases: () => releases };
}

test("hidden external windows suspend Surface delivery and coalesce foreground visibility and focus into one reload", async () => {
  const f = visibilityFixture();
  await f.owner.start();
  f.surfaces[0]!.callbacks.onFrameCommitted?.(frame);
  f.focusTarget.dispatchEvent(new Event("blur"));
  assert.equal(f.owner.active.value, true, "a visible unfocused window must keep displaying frames");
  f.visibilityDocument.setVisibility("hidden");
  assert.equal(f.owner.phase.value, "suspended");
  assert.equal(f.owner.canvas.value, null);
  assert.equal(f.owner.commit.value, null);
  assert.equal(f.owner.canForwardInput(true), false);
  assert.equal(f.surfaces[0]!.disconnects, 1);
  assert.equal(f.releases(), 1, "suspension releases held input");
  f.surfaces[0]!.callbacks.onError?.("surface_websocket_closed");
  f.surfaces[0]!.callbacks.onFrameCommitted?.(frame);
  assert.deepEqual(f.errors, [], "expected background closure must not report a terminal failure");
  f.visibilityDocument.setVisibility("visible");
  f.focusTarget.dispatchEvent(new Event("focus"));
  await Promise.resolve();
  assert.equal(f.surfaces.length, 2, "foreground events must share one new Surface");
  assert.equal(f.owner.phase.value, "loading");
  assert.equal(f.owner.canForwardInput(true), false, "input waits for the replacement baseline");
  f.surfaces[1]!.callbacks.onFrameCommitted?.(frame);
  assert.equal(f.owner.phase.value, "ready");
  assert.equal(f.owner.canForwardInput(true), true);
  f.owner.dispose();
});

test("focus rebuilds a Surface after missed visibility notifications and ignores the previous socket's delayed failure", async () => {
  const f = visibilityFixture();
  await f.owner.start();
  await f.owner.start();
  f.surfaces[0]!.callbacks.onFrameCommitted?.(frame);
  f.focusTarget.dispatchEvent(new Event("blur"));
  // Some WebViews become throttled while the document still reports visible.
  f.focusTarget.dispatchEvent(new Event("focus"));
  await Promise.resolve();
  assert.equal(f.surfaces.length, 2);
  assert.equal(f.surfaces[0]!.disconnects, 1);
  f.surfaces[0]!.callbacks.onError?.("surface_websocket_closed");
  f.surfaces[1]!.callbacks.onFrameCommitted?.(frame);
  assert.equal(f.owner.phase.value, "ready");
  assert.deepEqual(f.errors, []);
  f.surfaces[1]!.callbacks.onError?.("surface_canvas_context_lost");
  assert.equal(f.owner.phase.value, "failed");
  f.focusTarget.dispatchEvent(new Event("focus"));
  await Promise.resolve();
  assert.equal(f.surfaces.length, 3, "foreground focus can recover an already failed local Surface");
  f.owner.dispose();
  f.focusTarget.dispatchEvent(new Event("blur"));
  f.focusTarget.dispatchEvent(new Event("focus"));
  f.visibilityDocument.setVisibility("hidden");
  f.visibilityDocument.setVisibility("visible");
  await f.owner.start();
  assert.equal(f.surfaces.length, 3, "disposed listeners cannot reopen a Surface");
});

test("a socket failure delivered before the hidden notification suspends without a false disconnection error", async () => {
  const f = visibilityFixture();
  await f.owner.start();
  f.surfaces[0]!.callbacks.onFrameCommitted?.(frame);
  f.visibilityDocument.visibilityState = "hidden";
  f.surfaces[0]!.callbacks.onError?.("surface_websocket_closed");
  assert.equal(f.owner.phase.value, "suspended");
  assert.equal(f.owner.canForwardInput(true), false);
  assert.deepEqual(f.errors, []);
  // Focus can arrive before visibilitychange after the document becomes visible.
  f.visibilityDocument.visibilityState = "visible";
  f.focusTarget.dispatchEvent(new Event("focus"));
  f.visibilityDocument.setVisibility("visible");
  await Promise.resolve();
  assert.equal(f.surfaces.length, 2);
  f.surfaces[1]!.callbacks.onFrameCommitted?.(frame);
  assert.equal(f.owner.phase.value, "ready");
  f.owner.dispose();
});

test("initially hidden windows wait and a hidden or disposed pending snapshot cannot reopen the old Surface", async () => {
  const delayed = deferred<RemoteSessionSnapshot | null>();
  let snapshots = 0;
  const f = visibilityFixture(() => ++snapshots === 1 ? delayed.promise : Promise.resolve(snapshot));
  f.visibilityDocument.setVisibility("hidden");
  await f.owner.start();
  assert.equal(f.owner.phase.value, "suspended");
  assert.equal(snapshots, 0);
  f.visibilityDocument.setVisibility("visible");
  f.visibilityDocument.setVisibility("hidden");
  delayed.resolve(snapshot);
  await Promise.resolve();
  assert.equal(f.surfaces.length, 0);
  f.visibilityDocument.setVisibility("visible");
  f.owner.dispose();
  await Promise.resolve();
  assert.equal(f.owner.phase.value, "disposed");
  assert.equal(f.surfaces.length, 0);
  assert.deepEqual(f.errors, []);
});

test("background rAF suspension cannot strand the external Surface after returning to the foreground", async () => {
  const visibilityDocument = new VisibilityDocument();
  const focusTarget = Object.assign(new EventTarget(), {
    requestAnimationFrame(callback: FrameRequestCallback) { callbacks.set(++nextId, callback); return nextId; },
    cancelAnimationFrame(id: number) { callbacks.delete(id); },
  });
  const callbacks = new Map<number, FrameRequestCallback>();
  let nextId = 0;
  const sockets: FakeSocket[] = [];
  const acks: bigint[] = [];
  const errors: string[] = [];
  let draws = 0;
  class FakeSocket extends EventTarget {
    constructor(_url: string) { super(); sockets.push(this); }
    close() { this.dispatchEvent(new Event("close")); }
    send(bytes: ArrayBuffer) { acks.push(new DataView(bytes).getBigUint64(4)); }
  }
  class FakeImageData {
    data: Uint8ClampedArray;
    width: number;
    height: number;
    constructor(data: Uint8ClampedArray, width: number, height: number) {
      this.data = data; this.width = width; this.height = height;
    }
  }
  const globals = {
    document: { createElement: () => Object.assign(new EventTarget(), {
      width: 1, height: 1, getContext: () => ({ putImageData() { draws += 1; } }),
    }) },
    window: focusTarget, WebSocket: FakeSocket, ImageData: FakeImageData,
  };
  const descriptors = new Map<string, PropertyDescriptor | undefined>();
  for (const [key, value] of Object.entries(globals)) {
    descriptors.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, { configurable: true, value });
  }
  const owner = useSessionExternalSurface({
    sessionId: "session-1", getMountTarget: () => ({ replaceChildren() {} }) as HTMLElement,
    runtime: { document: visibilityDocument, window: focusTarget },
    getSnapshot: async () => snapshot, onError: (code) => errors.push(code),
  });
  function sendFrame(socket: FakeSocket, id: bigint) {
    const bytes = new Uint8Array(48);
    bytes.set([86, 78, 67, 70, 1, 0, 0, 28]);
    const view = new DataView(bytes.buffer);
    view.setBigUint64(8, id);
    view.setUint32(16, 1); view.setUint32(20, 1); view.setUint16(24, 1);
    view.setUint32(36, 1); view.setUint32(40, 1);
    socket.dispatchEvent(new MessageEvent("message", { data: bytes.buffer }));
  }
  function flushFrames() {
    const pending = [...callbacks.values()];
    callbacks.clear();
    pending.forEach((callback) => callback(0));
  }
  try {
    await owner.start();
    sendFrame(sockets[0]!, 1n);
    flushFrames();
    assert.equal(owner.phase.value, "ready");
    sendFrame(sockets[0]!, 2n);
    const delayedFrame = [...callbacks.values()][0]!;
    visibilityDocument.setVisibility("hidden");
    assert.equal(callbacks.size, 0, "hidden windows must cancel the unacknowledged queued draw");
    sockets[0]!.dispatchEvent(new Event("close")); // The old server ACK deadline expires.
    visibilityDocument.setVisibility("visible");
    focusTarget.dispatchEvent(new Event("focus"));
    await Promise.resolve();
    assert.equal(sockets.length, 2);
    delayedFrame(0);
    assert.deepEqual(acks, [1n], "old queued work must never ACK the replacement baseline");
    // Rust resets delivery for a replacement socket, including an unchanged frame id.
    sendFrame(sockets[1]!, 1n);
    flushFrames();
    assert.equal(owner.phase.value, "ready");
    assert.equal(owner.canForwardInput(true), true);
    assert.equal(draws, 2);
    assert.deepEqual(acks, [1n, 1n]);
    assert.deepEqual(errors, []);
  } finally {
    owner.dispose();
    for (const [key, descriptor] of descriptors) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
});
