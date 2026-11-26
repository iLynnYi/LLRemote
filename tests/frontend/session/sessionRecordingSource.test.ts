import assert from "node:assert/strict";
import test from "node:test";
import { createSessionRecordingSource } from "../../../src/session/artifacts/sessionRecordingSource.ts";
import type { createCoreSurfaceSession } from "../../../src/session/renderer/coreSurfaceSession.ts";

test("recording owns a passive Core surface and a fixed-size video-only canvas across resize", async (context) => {
  context.mock.timers.enable({ apis: ["setInterval"] });
  const previous = globalThis.document;
  const input = { width: 3456, height: 2024 } as HTMLCanvasElement;
  const paints: unknown[][] = []; let rate = 0, stopped = 0, disconnected = 0, detached = 0;
  const stream = { getTracks: () => [{ kind: "video", stop: () => { stopped++; } }] } as unknown as MediaStream;
  const output = Object.assign(new EventTarget(), { width: 0, height: 0, getContext: () => ({ fillRect() {}, drawImage: (...args: unknown[]) => paints.push(args) }), captureStream: (fps: number) => { rate = fps; return stream; } });
  Object.defineProperty(globalThis, "document", { configurable: true, value: { createElement: (tag: string) => tag === "canvas" ? output : {} } });
  let callbacks: Parameters<typeof createCoreSurfaceSession>[0] | undefined;
  const errors: string[] = [];
  try {
    const starting = createSessionRecordingSource({
      sessionId: "s1", signal: new AbortController().signal, onError: (code) => errors.push(code),
      core: {
        attachSurface: async (request) => {
          assert.equal(request.owner, "recording"); assert.equal(request.active, false);
          return { ...request, active: false, surfaceUrl: "ws://local/surface" };
        },
        detachSurface: async () => { detached++; return {} as never; },
      },
      createSurface: (options) => { callbacks = options; return { canvas: input, disconnect() { disconnected++; } }; },
    });
    await Promise.resolve(); assert.equal(callbacks?.presentationIntervalMs, 50);
    callbacks?.onFrameCommitted?.({ frameId: 1n, width: 3456, height: 2024 });
    const source = await starting;
    assert.equal(rate, 20); assert.equal(source.stream, stream);
    assert.equal(source.width, 1920); assert.equal(source.height, 1124);
    input.width = 1600; input.height = 900;
    callbacks?.onFrameCommitted?.({ frameId: 2n, width: 1600, height: 900 });
    context.mock.timers.tick(50);
    assert.deepEqual([output.width, output.height], [1920, 1124]); assert.equal(paints.length, 2);
    assert.deepEqual(paints[1]?.slice(1), [0, 22, 1920, 1080]);
    output.dispatchEvent(new Event("contextlost"));
    assert.deepEqual(errors, ["recording_canvas_failed"]);
    const closing = source.close(); assert.equal(source.close(), closing); await closing;
    context.mock.timers.tick(50); assert.equal(paints.length, 2);
    assert.deepEqual({ stopped, disconnected, detached }, { stopped: 1, disconnected: 1, detached: 1 });
  } finally { Object.defineProperty(globalThis, "document", { configurable: true, value: previous }); }
});

test("cancel while attaching immediately releases the eventual Core consumer", async () => {
  const abort = new AbortController(); let resolve!: (value: never) => void; let detached = 0;
  const starting = createSessionRecordingSource({ sessionId: "cancel", signal: abort.signal, onError() {},
    core: {
      attachSurface: () => new Promise(complete => { resolve = complete; }),
      detachSurface: async () => { detached++; return {} as never; },
    },
  });
  abort.abort(); resolve({ surfaceUrl: "ws://local/surface" } as never);
  await assert.rejects(starting, /recording_cancelled/); assert.equal(detached, 1);
});
