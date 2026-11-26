import { createCoreSurfaceSession } from "../renderer/coreSurfaceSession.ts";
import type { RemoteSessionCoreClient } from "../protocol/remoteSessionCore.ts";

export const SESSION_RECORDING_FPS = 20;
const SESSION_RECORDING_MAX_EDGE = 1920;
export type SessionRecordingSource = {
  stream: MediaStream;
  width: number;
  height: number;
  close: () => Promise<void>;
};

/** A Session-owned consumer; display ownership and window DOM never select its pixels. */
export async function createSessionRecordingSource(options: {
  sessionId: string;
  core: Pick<RemoteSessionCoreClient, "attachSurface" | "detachSurface">;
  signal: AbortSignal;
  onError: (code: string) => void;
  createSurface?: typeof createCoreSurfaceSession;
}): Promise<SessionRecordingSource> {
  const surfaceId = `recording-${crypto.randomUUID()}`;
  const descriptor = await options.core.attachSurface({
    sessionId: options.sessionId, surfaceId, owner: "recording", windowLabel: "main", active: false,
  });
  let surface: ReturnType<typeof createCoreSurfaceSession> | undefined;
  let stream: MediaStream | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  let output: HTMLCanvasElement | undefined;
  const canvasFailed = () => options.onError("recording_canvas_failed");
  let closing: Promise<void> | undefined;
  const close = () => closing ??= (async () => {
    if (timer !== undefined) clearInterval(timer);
    output?.removeEventListener("contextlost", canvasFailed);
    output?.removeEventListener("contextrestored", canvasFailed);
    try {
      stream?.getTracks().forEach((track) => track.stop());
    } finally {
      try { surface?.disconnect(); }
      finally { await options.core.detachSurface(options.sessionId, surfaceId); }
    }
  })();
  try {
    if (options.signal.aborted) throw new Error("recording_cancelled");
    const recordingCanvas = document.createElement("canvas");
    output = recordingCanvas;
    output.addEventListener("contextlost", canvasFailed);
    output.addEventListener("contextrestored", canvasFailed);
    const context = output.getContext("2d", { alpha: false });
    if (!context || typeof output.captureStream !== "function") throw new Error("recording_unavailable");
    await new Promise<void>((resolve, reject) => {
      const deadline = setTimeout(() => finish(new Error("recording_frame_timeout")), 5000);
      let settled = false;
      const aborted = () => finish(new Error("recording_cancelled"));
      const finish = (error?: Error) => {
        if (settled) return;
        settled = true;
        clearTimeout(deadline);
        options.signal.removeEventListener("abort", aborted);
        if (error) reject(error); else resolve();
      };
      options.signal.addEventListener("abort", aborted, { once: true });
      try {
        surface = (options.createSurface ?? createCoreSurfaceSession)({
          target: document.createElement("div"), surfaceUrl: descriptor.surfaceUrl,
          presentationIntervalMs: 1000 / SESSION_RECORDING_FPS,
          onFrameCommitted(frame) {
            if (!settled) {
              const scale = Math.min(1, SESSION_RECORDING_MAX_EDGE / Math.max(frame.width, frame.height));
              recordingCanvas.width = Math.max(1, Math.round(frame.width * scale));
              recordingCanvas.height = Math.max(1, Math.round(frame.height * scale));
              finish();
            }
          },
          onError(code) {
            if (!settled) finish(new Error(code));
            else options.onError(code);
          },
        });
      } catch (error) {
        finish(error instanceof Error ? error : new Error("recording_unavailable"));
      }
    });
    if (options.signal.aborted) throw new Error("recording_cancelled");
    const paint = () => {
      if (!surface) return;
      if (typeof context.isContextLost === "function" && context.isContextLost()) throw new Error("recording_canvas_failed");
      const input = surface.canvas;
      const scale = Math.min(recordingCanvas.width / input.width, recordingCanvas.height / input.height);
      const width = input.width * scale, height = input.height * scale;
      context.fillStyle = "#000";
      context.fillRect(0, 0, recordingCanvas.width, recordingCanvas.height);
      context.drawImage(input, (recordingCanvas.width - width) / 2, (recordingCanvas.height - height) / 2, width, height);
    };
    paint();
    stream = output.captureStream(SESSION_RECORDING_FPS);
    // Redraw static pixels too, keeping the recording's elapsed time through idle periods.
    timer = setInterval(() => {
      try { paint(); } catch { options.onError("recording_canvas_failed"); }
    }, 1000 / SESSION_RECORDING_FPS);
    return { stream, width: output.width, height: output.height, close };
  } catch (error) {
    await close().catch(() => undefined);
    throw error;
  }
}
