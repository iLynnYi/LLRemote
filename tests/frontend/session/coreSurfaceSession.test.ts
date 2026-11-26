import assert from "node:assert/strict";
import test from "node:test";
import { createCoreSurfaceSession, encodeCoreSurfaceAck } from "../../../src/session/renderer/coreSurfaceSession.ts";

test("core surface acknowledgment carries the committed frame id", () => {
  const ack = new Uint8Array(encodeCoreSurfaceAck(0x0102_0304_0506_0708n));
  assert.deepEqual([...ack.slice(0, 4)], [86, 78, 67, 65]);
  assert.equal(new DataView(ack.buffer).getBigUint64(4), 0x0102_0304_0506_0708n);
});

test("core surface session keeps the renderer boundary free of RFB state", async () => {
  const source = await import("node:fs/promises").then((fs) => fs.readFile(
    new URL("../../../src/session/renderer/coreSurfaceSession.ts", import.meta.url), "utf8",
  ));
  assert.match(source, /new WebSocket\(options\.surfaceUrl\)/);
  assert.match(source, /decodeCoreSurfaceFrame/);
  assert.match(source, /socket\.send\(encodeCoreSurfaceAck\(frame\.frameId\)\)/);
  assert.doesNotMatch(source, /decode_framebuffer_update|TcpStream/);
});

test("Surface disposal cancels queued Canvas work and ignores late socket errors", () => {
  const callbacks = new Map<number, () => void>();
  const canvas = Object.assign(new EventTarget(), { getContext: () => ({ putImageData() {} }) });
  const sockets: FakeSocket[] = [];
  class FakeSocket extends EventTarget {
    binaryType = "";
    constructor(_url: string) { super(); sockets.push(this); }
    close() { this.dispatchEvent(new Event("close")); }
    send() { assert.fail("disposed Surface must not send an ACK"); }
  }
  const globals = {
    document: { createElement: () => canvas },
    window: {
      requestAnimationFrame(callback: () => void) { callbacks.set(1, callback); return 1; },
      cancelAnimationFrame(id: number) { callbacks.delete(id); },
    },
    WebSocket: FakeSocket,
  };
  const descriptors = new Map<string, PropertyDescriptor | undefined>();
  for (const [key, value] of Object.entries(globals)) {
    descriptors.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, { configurable: true, value });
  }
  try {
    const errors: string[] = [];
    const target = Object.assign(document.createElement("div"), { replaceChildren() {} });
    const session = createCoreSurfaceSession({ target, surfaceUrl: "ws://localhost/fixture", onError: (code) => errors.push(code) });
    sockets[0].dispatchEvent(new MessageEvent("message", { data: new ArrayBuffer(28) }));
    assert.equal(callbacks.size, 1);
    session.disconnect();
    assert.equal(callbacks.size, 0, "queued rAF must be cancelled on disconnect");
    sockets[0].dispatchEvent(new Event("error"));
    assert.deepEqual(errors, [], "an old Surface error must not affect a later session");
    session.disconnect();
    const next = createCoreSurfaceSession({ target, surfaceUrl: "ws://localhost/fixture", onError: (code) => errors.push(code) });
    sockets[1].dispatchEvent(new MessageEvent("message", { data: new ArrayBuffer(28) }));
    sockets[1].dispatchEvent(new Event("error"));
    assert.equal(callbacks.size, 0, "terminal socket failure must cancel pending Canvas work");
    sockets[1].dispatchEvent(new Event("close"));
    assert.deepEqual(errors, ["surface_websocket_failed"], "terminal error is reported once");
    next.disconnect();
  } finally {
    for (const [key, descriptor] of descriptors) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
});

function surfacePacket(frameId: bigint, full: boolean): ArrayBuffer {
  const width = full ? 2 : 1;
  const packet = new Uint8Array(44 + width * 4);
  packet.set([86, 78, 67, 70, 1, 0, 0, 28]);
  const view = new DataView(packet.buffer);
  view.setBigUint64(8, frameId);
  view.setUint32(16, 2);
  view.setUint32(20, 1);
  view.setUint16(24, 1);
  view.setUint32(28, full ? 0 : 1);
  view.setUint32(36, width);
  view.setUint32(40, 1);
  packet.fill(255, 44);
  return packet.buffer;
}

function withSurfaceFixture(run: (fixture: {
  canvas: EventTarget;
  sendFrame: (id: bigint, full: boolean) => void;
  flush: () => void;
  loseBeforeEvent: () => void;
  queued: () => (() => void) | undefined;
  acks: bigint[];
  commits: bigint[];
  errors: string[];
  draws: number[];
  pendingCount: () => number;
}) => void) {
  let lost = false;
  let nextAnimationId = 0;
  const callbacks = new Map<number, () => void>();
  const acks: bigint[] = [];
  const commits: bigint[] = [];
  const errors: string[] = [];
  const draws: number[] = [];
  const canvas = Object.assign(new EventTarget(), {
    width: 2,
    height: 1,
    getContext: () => ({
      isContextLost: () => lost,
      putImageData() { draws.push(1); },
    }),
  });
  let socket: FakeSocket;
  class FakeSocket extends EventTarget {
    binaryType = "";
    constructor(_url: string) { super(); socket = this; }
    close() { this.dispatchEvent(new Event("close")); }
    send(bytes: ArrayBuffer) { acks.push(new DataView(bytes).getBigUint64(4)); }
  }
  class FakeImageData {
    data: Uint8ClampedArray;
    width: number;
    height: number;
    constructor(data: Uint8ClampedArray, width: number, height: number) {
      this.data = data;
      this.width = width;
      this.height = height;
    }
  }
  const globals = {
    document: { createElement: () => canvas },
    window: {
      requestAnimationFrame(callback: () => void) { callbacks.set(++nextAnimationId, callback); return nextAnimationId; },
      cancelAnimationFrame(id: number) { callbacks.delete(id); },
    },
    WebSocket: FakeSocket,
    ImageData: FakeImageData,
  };
  const descriptors = new Map<string, PropertyDescriptor | undefined>();
  for (const [key, value] of Object.entries(globals)) {
    descriptors.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, { configurable: true, value });
  }
  const session = createCoreSurfaceSession({
    target: { replaceChildren() {} } as HTMLElement,
    surfaceUrl: "ws://localhost/fixture",
    onFrameCommitted: (commit) => commits.push(commit.frameId),
    onError: (code) => errors.push(code),
  });
  try {
    run({
      canvas, acks, commits, errors, draws,
      sendFrame(id, full) { socket.dispatchEvent(new MessageEvent("message", { data: surfacePacket(id, full) })); },
      flush() {
        const ready = [...callbacks.values()];
        callbacks.clear();
        for (const callback of ready) callback();
      },
      queued: () => [...callbacks.values()][0],
      loseBeforeEvent() { lost = true; },
      pendingCount: () => callbacks.size,
    });
  } finally {
    session.disconnect();
    for (const [key, descriptor] of descriptors) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
}

for (const eventType of ["contextlost", "contextrestored"]) {
  test(`Canvas ${eventType} invalidates its baseline before a queued patch can ACK`, () => {
    withSurfaceFixture((fixture) => {
      fixture.sendFrame(1n, true);
      fixture.flush();
      fixture.sendFrame(2n, false);
      const lateCallback = fixture.queued();
      fixture.canvas.dispatchEvent(new Event(eventType));
      const cancelled = fixture.pendingCount() === 0;
      lateCallback?.();
      fixture.canvas.dispatchEvent(new Event("contextrestored"));
      fixture.sendFrame(3n, false);
      fixture.flush();
      assert.equal(cancelled, true, "baseline loss must cancel queued Canvas work");
      assert.deepEqual(fixture.acks, [1n], "a patch without its Canvas baseline must not ACK");
      assert.deepEqual(fixture.commits, [1n]);
      assert.equal(fixture.draws.length, 1);
      assert.deepEqual(fixture.errors, ["surface_canvas_context_lost"]);
    });
  });
}

test("Canvas context loss detected before its DOM event cannot ACK a patch", () => {
  withSurfaceFixture((fixture) => {
    fixture.sendFrame(1n, true);
    fixture.flush();
    fixture.sendFrame(2n, false);
    fixture.loseBeforeEvent();
    fixture.flush();
    fixture.canvas.dispatchEvent(new Event("contextlost"));
    assert.deepEqual(fixture.acks, [1n]);
    assert.deepEqual(fixture.commits, [1n]);
    assert.equal(fixture.draws.length, 1);
    assert.deepEqual(fixture.errors, ["surface_canvas_context_lost"]);
  });
});
