import type { SessionRecordingSource } from "./sessionRecordingSource.ts";

export type RecordingPhase = "starting" | "recording" | "stopping" | "failed";
export type RecordingSpaceStatus = {
  availableBytes: number; closingRequiredBytes: number; mergeRequiredBytes: number;
  mergeWarning: boolean; shouldStop: boolean;
};
export type RecordingFileResult = {
  filePath: string; fileName: string; bytesWritten: number;
  recordingId?: string; status?: "saved" | "segmented";
  files?: Array<{ filePath: string; fileName: string; bytesWritten: number }>;
};
export type RecordingFileHandle = { recordingId: string; recoveryPath: string };
export type RecordingFileSink = {
  start: () => Promise<RecordingFileHandle>;
  append: (recordingId: string, offset: number, chunk: Blob) => Promise<void>;
  finish: (recordingId: string, durationMs: number) => Promise<RecordingFileResult>;
  release: (recordingId: string) => Promise<void>;
  relocate?: (recordingId: string, directory: string) => Promise<void | { recoveryPath: string }>;
  space?: (recordingId: string, pendingBytes: number) => Promise<RecordingSpaceStatus>;
};
export type RecordingResult = RecordingFileResult & { durationMs: number; width: number; height: number };

/** Owns one encoder and a bounded write queue, including retryable finalization. */
export function createSessionRecordingTask(options: {
  createSource: (signal: AbortSignal, onError: (code: string) => void) => Promise<SessionRecordingSource>;
  createRecorder: (stream: MediaStream) => MediaRecorder;
  sink: RecordingFileSink;
  onPhase: (phase: RecordingPhase) => void;
  onTick?: () => void;
  onCaptureStopped?: () => void;
  onSpaceChanged?: (status: RecordingSpaceStatus, enteredWarning: boolean) => void;
  onStopRequested: (reason?: string) => void;
  now?: () => number;
}) {
  const now = options.now ?? (() => performance.now());
  const abort = new AbortController();
  let phase: RecordingPhase = "starting";
  let source: SessionRecordingSource | undefined;
  let recorder: MediaRecorder | undefined;
  let handle: RecordingFileHandle | undefined;
  let startedAt = 0, durationMs = 0, offset = 0, bufferedBytes = 0;
  let stopped = false;
  let resolveStopped: (() => void) | undefined;
  const stoppedEvent = new Promise<void>((resolve) => { resolveStopped = resolve; });
  const pending: Blob[] = [];
  let flushing: Promise<void> | null = null;
  let flushError: unknown;
  let stopping: Promise<RecordingResult | null> | null = null;
  let saved: RecordingResult | null = null;
  let stopRequested = false;
  let spaceTimer: ReturnType<typeof setInterval> | undefined;
  let checkingSpace = false;
  let spaceStatus: RecordingSpaceStatus | undefined;
  let captureReleased = false;
  let resolveCaptureStopped: (() => void) | undefined;
  const captureStopped = new Promise<void>(resolve => { resolveCaptureStopped = resolve; });
  const releaseCapture = () => {
    if (captureReleased) return;
    captureReleased = true;
    options.onCaptureStopped?.();
    resolveCaptureStopped?.();
  };
  const checkSpace = async () => {
    if (checkingSpace || !handle || !options.sink.space || phase !== "recording") return;
    checkingSpace = true;
    try {
      const next = await options.sink.space(handle.recordingId, bufferedBytes);
      if (phase !== "recording") return;
      // Clearing requires enough room for another observed write interval. This
      // dynamic hysteresis avoids repeated warning/recovery at a single threshold.
      const hysteresis = Math.max(bufferedBytes * 2, next.closingRequiredBytes - (spaceStatus?.closingRequiredBytes ?? next.closingRequiredBytes));
      if (spaceStatus?.mergeWarning && !next.mergeWarning && next.availableBytes < next.mergeRequiredBytes + hysteresis) next.mergeWarning = true;
      const enteredWarning = next.mergeWarning && !spaceStatus?.mergeWarning;
      spaceStatus = next;
      options.onSpaceChanged?.(next, enteredWarning);
      if (next.shouldStop) requestStop("recording_space_insufficient");
    } catch {
      if (phase === "recording") requestStop("recording_space_check_failed");
    } finally { checkingSpace = false; }
  };
  let progressTimer: ReturnType<typeof setInterval> | undefined;
  const stopProgress = () => {
    if (progressTimer !== undefined) clearInterval(progressTimer);
    progressTimer = undefined;
    if (spaceTimer !== undefined) clearInterval(spaceTimer);
    spaceTimer = undefined;
  };
  const setPhase = (next: RecordingPhase) => { phase = next; options.onPhase(next); };
  const requestStop = (reason?: string) => {
    if (stopRequested || phase === "stopping") return;
    stopRequested = true;
    queueMicrotask(() => options.onStopRequested(reason));
  };

  function flush(): Promise<void> {
    if (flushing) return flushing;
    const operation = (async () => {
      while (pending.length && handle) {
        const chunk = pending[0];
        await options.sink.append(handle.recordingId, offset, chunk);
        offset += chunk.size;
        bufferedBytes -= chunk.size;
        pending.shift();
      }
    })();
    flushing = operation;
    void operation.then(() => {
      if (flushing === operation) flushing = null;
    }, (error: unknown) => {
      if (flushing === operation) flushing = null;
      flushError = error;
      requestStop("recording_write_failed");
    });
    return operation;
  }

  const starting = (async () => {
    try {
      source = await options.createSource(abort.signal, (code) => requestStop(code));
      if (abort.signal.aborted) throw new Error("recording_cancelled");
      handle = await options.sink.start();
      if (abort.signal.aborted) throw new Error("recording_cancelled");
      recorder = options.createRecorder(source.stream);
      recorder.addEventListener("dataavailable", (event) => {
        if (!event.data.size) return;
        for (let start = 0; start < event.data.size; start += 256 * 1024) {
          pending.push(event.data.slice(start, start + 256 * 1024));
        }
        bufferedBytes += event.data.size;
        if (!flushError) void flush().catch(() => undefined);
        // Stop acquiring before a slow disk can keep growing the queue.
        if (bufferedBytes > 32 * 1024 * 1024) requestStop("recording_write_backpressure");
      });
      recorder.addEventListener("error", () => requestStop("recording_encoder_failed"));
      recorder.addEventListener("stop", () => {
        stopProgress();
        stopped = true;
        if (!durationMs) durationMs = Math.max(1, now() - startedAt);
        resolveStopped?.();
        if (phase !== "stopping") requestStop();
      });
      startedAt = now();
      recorder.start(1000);
      setPhase("recording");
      progressTimer = setInterval(() => options.onTick?.(), 1000);
      if (options.sink.space) {
        spaceTimer = setInterval(() => { void checkSpace(); }, 3000);
        void checkSpace();
      }
    } catch (error) {
      stopProgress();
      await source?.close().catch(() => undefined);
      if (handle) await options.sink.release(handle.recordingId).catch(() => undefined);
      throw error;
    }
  })();
  // The owner awaits start(), but disposal may overtake that await.
  void starting.catch(() => undefined);

  async function finish(): Promise<RecordingResult | null> {
    if (saved) return saved;
    if (phase === "starting") abort.abort();
    try { await starting; } catch (error) {
      if (abort.signal.aborted) return null;
      throw error;
    }
    if (!source || !recorder || !handle) return null;
    setPhase("stopping");
    stopProgress();
    if (!durationMs) durationMs = Math.max(1, now() - startedAt);
    try {
      if (!stopped && recorder.state !== "inactive") recorder.stop();
      if (!stopped) {
        await new Promise<void>((resolve, reject) => {
          const timeout = setTimeout(() => reject(new Error("recording_stop_timeout")), 5000);
          void stoppedEvent.then(() => { clearTimeout(timeout); resolve(); });
        });
      }
      await source.close().catch(() => undefined);
      releaseCapture();
      if (flushing) await flushing.catch(() => undefined);
      flushError = undefined;
      await flush();
      const written = await options.sink.finish(handle.recordingId, durationMs);
      saved = { ...written, durationMs, width: source.width, height: source.height };
      return saved;
    } catch (error) {
      await source.close().catch(() => undefined);
      releaseCapture();
      setPhase("failed");
      throw error;
    }
  }

  return {
    start: () => starting,
    stop(): Promise<RecordingResult | null> {
      if (stopping) return stopping;
      const operation = finish();
      stopping = operation;
      void operation.catch(() => { if (stopping === operation) stopping = null; });
      return operation;
    },
    checkSpace,
    async relocate(directory: string) {
      if (!handle || phase !== "failed" || !options.sink.relocate) throw new Error("recording_job_busy");
      const moved = await options.sink.relocate(handle.recordingId, directory);
      if (moved) handle.recoveryPath = moved.recoveryPath;
    },
    captureStopped,
    get recordingId() { return handle?.recordingId; },
    get pendingBytes() { return bufferedBytes; },
    get spaceStatus() { return spaceStatus; },
    get recoveryPath() { return handle?.recoveryPath; },
    get phase() { return phase; },
    get elapsedMs() { return phase === "starting" ? 0 : durationMs || Math.max(0, now() - startedAt); },
  };
}
