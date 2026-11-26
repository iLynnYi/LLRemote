import assert from "node:assert/strict";
import test from "node:test";
import { createSessionRecordingTask, type RecordingFileSink, type RecordingPhase } from "../../../src/session/artifacts/sessionRecordingTask.ts";
import type { SessionRecordingSource } from "../../../src/session/artifacts/sessionRecordingSource.ts";

class Encoder extends EventTarget {
  state = "inactive";
  starts = 0;
  start() { this.starts++; this.state = "recording"; }
  data(value: string) {
    const event = Object.assign(new Event("dataavailable"), { data: new Blob([value]) });
    this.dispatchEvent(event);
  }
  stop() {
    if (this.state === "inactive") return;
    this.state = "inactive";
    queueMicrotask(() => { this.data("tail"); this.dispatchEvent(new Event("stop")); });
  }
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}
function fixture() {
  let now = 100, closes = 0, finishes = 0, releases = 0;
  const encoder = new Encoder(); const phases: RecordingPhase[] = []; const reasons: Array<string | undefined> = [];
  const chunks: string[] = []; const offsets: number[] = [];
  const source: SessionRecordingSource = { stream: {} as MediaStream, width: 800, height: 600, close: async () => { closes++; } };
  const sink: RecordingFileSink = {
    start: async () => ({ recordingId: "file-1", recoveryPath: "/selected/recording.part" }),
    append: async (_id, offset, blob) => { offsets.push(offset); chunks.push(await blob.text()); },
    finish: async () => { finishes++; return { filePath: "/selected/video.webm", fileName: "video.webm", bytesWritten: 8 }; },
    release: async () => { releases++; },
  };
  const options = {
    createSource: async (_signal: AbortSignal, _onError: (code: string) => void) => source,
    createRecorder: () => encoder as unknown as MediaRecorder,
    sink, now: () => now, onPhase: (phase: RecordingPhase) => phases.push(phase),
    onStopRequested: (reason?: string) => { reasons.push(reason); },
  };
  return { encoder, options, source, sink, chunks, offsets, phases, reasons, advance: () => { now = 2100; },
    counts: () => ({ closes, finishes, releases }) };
}

test("recording writes chunks before stop, coalesces stop, and preserves idle elapsed time", async () => {
  const f = fixture(); const task = createSessionRecordingTask(f.options); await task.start();
  f.encoder.data("body"); await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(f.chunks, ["body"]);
  f.advance(); const stopping = task.stop(); assert.equal(task.stop(), stopping);
  const saved = await stopping;
  assert.equal(saved?.durationMs, 2000); assert.equal(saved?.width, 800);
  assert.deepEqual(f.chunks, ["body", "tail"]); assert.deepEqual(f.offsets, [0, 4]);
  assert.deepEqual(f.counts(), { closes: 1, finishes: 1, releases: 0 });
});

test("failed finalization retains its handle and retries without starting another encoder", async () => {
  const f = fixture(); const finish = f.sink.finish; let attempts = 0;
  f.sink.finish = async (...args) => { if (++attempts === 1) throw new Error("disk-full"); return finish(...args); };
  const task = createSessionRecordingTask(f.options); await task.start(); f.advance();
  await assert.rejects(task.stop(), /disk-full/);
  assert.equal(task.phase, "failed"); assert.equal(task.recoveryPath, "/selected/recording.part");
  const saved = await task.stop(); assert.equal(saved?.durationMs, 2000); assert.equal(f.encoder.starts, 1);
  assert.deepEqual(f.chunks, ["tail"]);
});

test("automatic encoder stop requests file finalization and records stopped duration", async () => {
  const f = fixture(); const task = createSessionRecordingTask(f.options); await task.start(); f.advance();
  f.encoder.stop(); await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(f.reasons, [undefined]); assert.equal((await task.stop())?.durationMs, 2000);
});

test("stop during pending source startup cancels startup and never creates an encoder or file", async () => {
  const f = fixture(); const pending = deferred<SessionRecordingSource>(); let signal: AbortSignal | undefined;
  f.options.createSource = async (value) => { signal = value; return pending.promise; };
  const task = createSessionRecordingTask(f.options); const stopping = task.stop();
  assert.equal(signal?.aborted, true); pending.resolve(f.source);
  assert.equal(await stopping, null); assert.equal(f.encoder.starts, 0);
  assert.equal(f.counts().closes, 1); assert.equal(f.counts().releases, 0);
});

test("encoder start failure releases the disk handle and captured source", async () => {
  const f = fixture(); f.encoder.start = () => { throw new Error("encoder-start"); };
  const task = createSessionRecordingTask(f.options); await assert.rejects(task.start(), /encoder-start/);
  assert.deepEqual(f.counts(), { closes: 1, finishes: 0, releases: 1 });
});

test("failed chunk remains queued for retry with the original offset", async () => {
  const f = fixture(); const append = f.sink.append; let fail = true;
  f.sink.append = async (...args) => { if (fail) throw new Error("write-failed"); await append(...args); };
  const task = createSessionRecordingTask(f.options); await task.start(); f.encoder.data("body");
  await new Promise(resolve => setImmediate(resolve));
  await assert.rejects(task.stop(), /write-failed/);
  fail = false; await task.stop(); assert.deepEqual(f.chunks, ["body", "tail"]);
  assert.deepEqual(f.offsets, [0, 4]); assert.deepEqual(f.reasons, ["recording_write_failed"]);
});

test("synchronous encoder stop failure closes the source and keeps saving retryable", async () => {
  const f = fixture(); const stop = f.encoder.stop.bind(f.encoder); let calls = 0;
  f.encoder.stop = () => { if (++calls === 1) throw new Error("encoder-stop"); stop(); };
  const task = createSessionRecordingTask(f.options); await task.start();
  await assert.rejects(task.stop(), /encoder-stop/);
  assert.equal(task.phase, "failed"); assert.equal(f.counts().closes, 1);
  assert.ok(await task.stop()); assert.deepEqual(f.chunks, ["tail"]);
});

test("space warnings fire once per transition, recover after cleanup, and stop only at the closing threshold", async () => {
  const f = fixture();
  let status = { availableBytes: 10000, closingRequiredBytes: 100, mergeRequiredBytes: 1000, mergeWarning: false, shouldStop: false };
  const warnings: boolean[] = [];
  const task = createSessionRecordingTask({ ...f.options,
    sink: { ...f.sink, space: async () => ({ ...status }) },
    onSpaceChanged: (_status, entered) => { warnings.push(entered); },
  });
  try {
    await task.start(); await task.checkSpace();
    status = { ...status, availableBytes: 800, mergeWarning: true };
    await task.checkSpace(); await task.checkSpace();
    assert.equal(warnings.filter(Boolean).length, 1);
    assert.deepEqual(f.reasons, [], "merge warning must not stop capture");
    status = { ...status, availableBytes: 2000, mergeWarning: false };
    await task.checkSpace(); assert.equal(task.spaceStatus?.mergeWarning, false);
    status = { ...status, availableBytes: 50, mergeWarning: true, shouldStop: true };
    await task.checkSpace(); await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(f.reasons, ["recording_space_insufficient"]);
    assert.equal(warnings.filter(Boolean).length, 2);
  } finally { await task.stop(); }
});

test("capture is released before background finalization finishes, with no duplicate release on retry", async () => {
  const f = fixture(); const pending = deferred<{ filePath: string; fileName: string; bytesWritten: number }>();
  f.sink.finish = () => pending.promise;
  let releases = 0;
  const task = createSessionRecordingTask({ ...f.options, onCaptureStopped: () => { releases++; } });
  await task.start();
  let finished = false;
  const stopping = task.stop().then(result => { finished = true; return result; });
  await task.captureStopped;
  assert.equal(releases, 1); assert.equal(finished, false); assert.equal(f.encoder.state, "inactive");
  pending.resolve({ filePath: "/selected/video.webm", fileName: "video.webm", bytesWritten: 4 });
  assert.ok(await stopping); await task.stop(); assert.equal(releases, 1);
});

test("unavailable space inspection requests a safe stop instead of silently losing monitoring", async () => {
  const f = fixture(); f.sink.space = async () => { throw new Error("disk-detached"); };
  const task = createSessionRecordingTask(f.options);
  try { await task.start(); await new Promise(resolve => setImmediate(resolve)); assert.deepEqual(f.reasons, ["recording_space_check_failed"]); }
  finally { await task.stop(); }
});

test("a failed queued write can move to a new disk before retrying the original offsets", async () => {
  const f = fixture(); const append = f.sink.append; let moved = false;
  f.sink.append = async (...args) => { if (!moved) throw new Error("old-disk-full"); await append(...args); };
  f.sink.relocate = async (id, directory) => { assert.equal(id, "file-1"); assert.equal(directory, "/new"); moved = true; return { recoveryPath: "/new/recovery" }; };
  const task = createSessionRecordingTask(f.options);
  await task.start(); f.encoder.data("body");
  await new Promise(resolve => setImmediate(resolve));
  await assert.rejects(task.stop(), /old-disk-full/);
  await task.relocate("/new"); assert.equal(task.recoveryPath, "/new/recovery"); await task.stop();
  assert.deepEqual(f.offsets, [0, 4]); assert.deepEqual(f.chunks, ["body", "tail"]);
});
