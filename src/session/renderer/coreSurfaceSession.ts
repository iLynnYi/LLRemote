import {
  createCoreSurfaceCanvasRenderer,
  decodeCoreSurfaceFrame,
} from "./coreSurfaceCanvas.ts";

export type CoreSurfaceSession = {
  disconnect: () => void;
  canvas: HTMLCanvasElement;
};

export type CoreSurfaceCommit = {
  frameId: bigint;
  width: number;
  height: number;
};

export function encodeCoreSurfaceAck(frameId: bigint): ArrayBuffer {
  const bytes = new Uint8Array(12);
  bytes.set([86, 78, 67, 65]); // VNCA
  new DataView(bytes.buffer).setBigUint64(4, frameId);
  return bytes.buffer;
}

/** Owns only a local Surface WebSocket and Canvas. RFB stays in the Rust Core. */
export function createCoreSurfaceSession(options: {
  target: HTMLElement;
  surfaceUrl: string;
  onFrameCommitted?: (commit: CoreSurfaceCommit) => void;
  onError?: (code: string) => void;
  presentationIntervalMs?: number;
}): CoreSurfaceSession {
  const canvas = document.createElement("canvas");
  canvas.className = "core-surface-canvas";
  options.target.replaceChildren(canvas);
  const renderer = createCoreSurfaceCanvasRenderer(canvas);
  const socket = new WebSocket(options.surfaceUrl);
  socket.binaryType = "arraybuffer";
  let closed = false;
  let animationId: number | null = null;
  let pending: ArrayBuffer | null = null;

  function presentPending() {
    animationId = null;
    if (closed) return;
    const packet = pending;
    pending = null;
    if (!packet) return;
    try {
      const frame = decodeCoreSurfaceFrame(packet);
      renderer.present(frame);
      socket.send(encodeCoreSurfaceAck(frame.frameId));
      options.onFrameCommitted?.({
        frameId: frame.frameId,
        width: frame.width,
        height: frame.height,
      });
    } catch (error) {
      fail(error instanceof Error ? error.message : "surface_packet_decode_failed");
    }
  }

  function onMessage(event: MessageEvent<unknown>) {
    if (closed || !(event.data instanceof ArrayBuffer)) return;
    pending = event.data;
    if (animationId === null) {
      animationId = options.presentationIntervalMs === undefined
        ? window.requestAnimationFrame(presentPending)
        : window.setTimeout(presentPending, options.presentationIntervalMs);
    }
  }
  function onSocketError() { fail("surface_websocket_failed"); }
  function onSocketClose() { fail("surface_websocket_closed"); }
  // Restored backing storage has no previously ACKed pixels. Reconnect must
  // establish a full baseline, even when the contextlost event was missed.
  function onCanvasContextLost() { fail("surface_canvas_context_lost"); }
  function fail(code: string) {
    if (closed) return;
    disconnect();
    options.onError?.(code);
  }
  function disconnect() {
    if (closed) return;
    closed = true;
    pending = null;
    if (animationId !== null) {
      if (options.presentationIntervalMs === undefined) window.cancelAnimationFrame(animationId);
      else window.clearTimeout(animationId);
      animationId = null;
    }
    socket.removeEventListener("message", onMessage);
    socket.removeEventListener("error", onSocketError);
    socket.removeEventListener("close", onSocketClose);
    canvas.removeEventListener("contextlost", onCanvasContextLost);
    canvas.removeEventListener("contextrestored", onCanvasContextLost);
    renderer.reset();
    socket.close();
  }
  socket.addEventListener("message", onMessage);
  socket.addEventListener("error", onSocketError);
  socket.addEventListener("close", onSocketClose);
  canvas.addEventListener("contextlost", onCanvasContextLost);
  canvas.addEventListener("contextrestored", onCanvasContextLost);
  return {
    canvas,
    disconnect,
  };
}
