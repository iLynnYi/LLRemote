import assert from "node:assert/strict";
import test from "node:test";
import { ref, nextTick } from "vue";
import { useSessionArtifacts } from "../../../src/features/sessions/composables/useSessionArtifacts.ts";
import type { RemoteSession, RemoteSessionTab } from "../../../src/session/core/sessionManager.ts";

test("screenshots notify the requesting window only when the saved result is hidden, while errors always report", async () => {
  const messages: Array<{ content: string; tone?: string }> = [];
  const logs: string[] = [];
  let failWrite = true, resultVisible = false;
  const session = { id: "s1", target: "fixture", state: "connected" } as RemoteSession;
  const canvas = { width: 640, height: 480, toDataURL: () => "data:image/png;base64,aW1hZ2U=" };
  const owner = useSessionArtifacts({
    activeRemoteSession: ref(session), sessionTabs: ref([]), artifactDirectory: ref("/selected"),
    getSession: () => session, getDiagnosticSummary: () => "connected",
    createRecordingSource: async () => { throw new Error("not recording"); },
    getViewportTarget: () => ({ querySelector: () => canvas }) as unknown as HTMLElement,
    invoke: async command => {
      if (command === "inspect_session_artifact_directory") return { status: "ready" };
      if (failWrite) throw new Error("disk-full");
      return { filePath: "/selected/image.png", fileName: "image.png" };
    },
    isTauriUnavailableError: () => false, logEvent: (_level, _source, message) => { logs.push(message); },
    isArtifactResultVisible: () => resultVisible,
    notify() { assert.fail("feedback must reach the requesting window"); },
    requestDirectoryAuthorization: async () => true, t: key => key,
  });
  const notify = (content: string, tone?: string) => { messages.push({ content, tone }); };
  await owner.captureSessionScreenshot("s1", notify);
  assert.deepEqual(messages, [{ content: "sessions.artifacts.writeFailed", tone: "error" }]);
  assert.equal(owner.artifacts.value.length, 0);
  failWrite = false;
  messages.length = 0;
  await owner.captureSessionScreenshot("s1", notify);
  assert.deepEqual(messages, [{ content: "sessions.artifacts.screenshotSaved", tone: "success" }]);
  assert.equal(owner.artifacts.value.length, 1);
  resultVisible = true;
  messages.length = 0;
  await owner.captureSessionScreenshot("s1", notify);
  assert.deepEqual(messages, []);
  assert.equal(owner.artifacts.value.length, 2);
  assert.equal(logs.filter(message => message === "sessions.artifacts.screenshotSaved").length, 2);
  failWrite = true;
  await owner.captureSessionScreenshot("s1", notify);
  assert.deepEqual(messages, [{ content: "sessions.artifacts.writeFailed", tone: "error" }]);
});

test("external-only recording needs no main viewport and still finalizes after Session removal", async () => {
  const oldRecorder = globalThis.MediaRecorder, oldReader = globalThis.FileReader;
  let encoders = 0, sourceCloses = 0, changes = 0, attempts = 0, viewportReads = 0;
  let exists = true, failSave = true, resultVisible = true; const offsets: number[] = [];
  const messages: Array<{ content: string; tone?: string }> = [];
  class Encoder extends EventTarget {
    static isTypeSupported() { return true; }
    state = "inactive";
    constructor() { super(); encoders++; }
    start() { this.state = "recording"; }
    stop() {
      this.state = "inactive";
      queueMicrotask(() => {
        this.dispatchEvent(Object.assign(new Event("dataavailable"), { data: new Blob(["encoded"]) }));
        this.dispatchEvent(new Event("stop"));
      });
    }
  }
  class Reader {
    result = ""; onload?: () => void;
    readAsDataURL(blob: Blob) {
      void blob.arrayBuffer().then(bytes => { this.result = `data:video/webm;base64,${Buffer.from(bytes).toString("base64")}`; this.onload?.(); });
    }
  }
  Object.assign(globalThis, { MediaRecorder: Encoder, FileReader: Reader });
  try {
    const session = { id: "s1", target: "fixture", state: "connected" } as RemoteSession;
    const sessionTabs = ref<RemoteSessionTab[]>([{ id: "s1", state: "connected" } as RemoteSessionTab]);
    const artifactDirectory = ref("/selected");
    const startedDirectories: string[] = [];
    const owner = useSessionArtifacts({
      activeRemoteSession: ref(session), sessionTabs, artifactDirectory, getDiagnosticSummary: () => "connected",
      createRecordingSource: async () => ({ stream: {} as MediaStream, width: 640, height: 480, close: async () => { sourceCloses++; } }),
      onRecordingChanged() { changes++; }, getSession: () => exists ? session : undefined,
      getViewportTarget: () => { viewportReads++; return undefined; },
      invoke: async (command, args) => {
        if (command === "inspect_session_artifact_directory") return { status: "ready" };
        if (command === "inspect_session_recording_space") return { availableBytes: 100000, closingRequiredBytes: 100, mergeRequiredBytes: 200, mergeWarning: false, shouldStop: false };
        if (command === "start_session_recording_file") {
          const request = args?.request as { directory: string };
          startedDirectories.push(request.directory);
          return { recordingId: "r1", recoveryPath: "/selected/recovery.part" };
        }
        if (command === "append_session_recording_chunk") {
          const request = args?.request as { offset: number; dataBase64: string };
          offsets.push(request.offset); assert.equal(Buffer.from(request.dataBase64, "base64").toString(), "encoded"); return;
        }
        if (command === "finish_session_recording_file") {
          attempts++; if (failSave) throw new Error("disk-full");
          return { filePath: "/selected/video.webm", fileName: "video.webm", bytesWritten: 7 };
        }
      },
      isTauriUnavailableError: () => false, logEvent() {},
      notify: (content, tone) => { messages.push({ content, tone }); },
      isArtifactResultVisible: () => resultVisible,
      requestDirectoryAuthorization: async () => true, t: key => key,
    });
    await Promise.all([owner.startSessionRecording("s1"), owner.startSessionRecording("s1")]);
    assert.equal(encoders, 1); assert.equal(viewportReads, 0); assert.equal(owner.isSessionRecording("s1"), true);
    artifactDirectory.value = "/new-builtin";
    exists = false;
    sessionTabs.value = [];
    await nextTick(); await new Promise(resolve => setImmediate(resolve));
    assert.equal(owner.getSessionRecordingPhase("s1"), undefined);
    assert.equal(owner.recordingJobs.value[0]?.status, "failed");
    assert.equal(owner.artifacts.value.length, 0);
    failSave = false; messages.length = 0; await owner.retryRecordingJob("r1");
    assert.equal(owner.isSessionRecording("s1"), false); assert.equal(owner.artifacts.value.length, 1);
    assert.equal(owner.artifacts.value[0]?.width, 640); assert.deepEqual(offsets, [0]);
    assert.deepEqual(startedDirectories, ["/selected"]);
    assert.equal(owner.artifacts.value[0]?.filePath, "/selected/video.webm", "changing the save location does not move an active recording");
    assert.equal(attempts, 2); assert.ok(changes >= 4); assert.ok(sourceCloses >= 1);
    assert.deepEqual(messages, [], "the saved recording is already visible in the list");
    exists = true;
    sessionTabs.value = [{ id: "s1", state: "connected" } as RemoteSessionTab];
    await owner.startSessionRecording("s1");
    await owner.stopSessionRecording("s1", "recording_disconnected");
    await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(messages.map(item => item.tone), ["warning"], "interruptions remain visible even beside the saved result");
    resultVisible = false;
    messages.length = 0;
    await owner.startSessionRecording("s1");
    await owner.stopSessionRecording("s1");
    await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(messages, [{ content: "sessions.artifacts.recordingSaved", tone: "success" }]);
  } finally { Object.assign(globalThis, { MediaRecorder: oldRecorder, FileReader: oldReader }); }
});

test("a new recording and Session close do not wait for the previous background save", async () => {
  const oldRecorder = globalThis.MediaRecorder, oldReader = globalThis.FileReader;
  let finishFirst!: (value: unknown) => void;
  const firstFinish = new Promise(resolve => { finishFirst = resolve; });
  let fileCount = 0, closes = 0;
  class Encoder extends EventTarget {
    static isTypeSupported() { return true; }
    state = "inactive";
    start() { this.state = "recording"; }
    stop() { this.state = "inactive"; queueMicrotask(() => { this.dispatchEvent(Object.assign(new Event("dataavailable"), { data: new Blob(["frame"]) })); this.dispatchEvent(new Event("stop")); }); }
  }
  class Reader {
    result = ""; onload?: () => void;
    readAsDataURL(blob: Blob) { void blob.arrayBuffer().then(bytes => { this.result = `data:video/webm;base64,${Buffer.from(bytes).toString("base64")}`; this.onload?.(); }); }
  }
  Object.assign(globalThis, { MediaRecorder: Encoder, FileReader: Reader });
  try {
    const session = { id: "s1", target: "fixture", state: "connected" } as RemoteSession;
    const owner = useSessionArtifacts({
      activeRemoteSession: ref(session), sessionTabs: ref([]), artifactDirectory: ref("/selected"), getSession: () => session,
      createRecordingSource: async () => ({ stream: {} as MediaStream, width: 640, height: 480, close: async () => { closes++; } }),
      getViewportTarget: () => undefined, getDiagnosticSummary: () => "", isTauriUnavailableError: () => false,
      requestDirectoryAuthorization: async () => true, t: key => key, logEvent() {}, notify() {},
      invoke: async (command, args) => {
        if (command === "inspect_session_artifact_directory") return { status: "ready" };
        if (command === "inspect_session_recording_space") return { availableBytes: 10000, closingRequiredBytes: 10, mergeRequiredBytes: 100, mergeWarning: false, shouldStop: false };
        if (command === "start_session_recording_file") return { recordingId: `r${++fileCount}`, recoveryPath: `/selected/r${fileCount}` };
        if (command === "list_session_recording_jobs") return [];
        if (command === "finish_session_recording_file") {
          const id = (args?.request as { recordingId: string }).recordingId;
          if (id === "r1") return firstFinish;
          return { filePath: `/selected/${id}.webm`, fileName: `${id}.webm`, bytesWritten: 5 };
        }
      },
    });
    await owner.startSessionRecording("s1");
    await owner.stopSessionRecording("s1");
    assert.equal(closes, 1); assert.equal(owner.isSessionRecording("s1"), false);
    assert.equal(owner.recordingJobs.value[0]?.status, "saving");
    await owner.startSessionRecording("s1");
    assert.equal(fileCount, 2); assert.equal(owner.isSessionRecording("s1"), true);
    finishFirst({ filePath: "/selected/r1.webm", fileName: "r1.webm", bytesWritten: 5 });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(owner.isSessionRecording("s1"), true, "completion of the old save must not remove a new capture");
    assert.equal(owner.artifacts.value.length, 1);
    await owner.stopSessionRecording("s1"); await new Promise(resolve => setImmediate(resolve));
    assert.equal(owner.artifacts.value.length, 2);
  } finally { Object.assign(globalThis, { MediaRecorder: oldRecorder, FileReader: oldReader }); }
});

test("recovered recording can save elsewhere without a connected Session", async () => {
  const calls: string[] = [];
  const owner = useSessionArtifacts({
    activeRemoteSession: ref(null), sessionTabs: ref([]), artifactDirectory: ref("/old"), getSession: () => undefined,
    createRecordingSource: async () => { throw new Error("must not acquire a source"); }, getViewportTarget: () => undefined,
    getDiagnosticSummary: () => "", isTauriUnavailableError: () => false, logEvent() {}, notify() {}, t: key => key,
    chooseRecordingDirectory: async () => "/new", requestDirectoryAuthorization: async () => true,
    invoke: async (command, args) => {
      calls.push(command);
      if (command === "recover_session_recordings") return [{ recordingId: "recovered", fileName: "video.webm", recoveryPath: "/old/task", status: "recoverable", durationMs: 1000, progress: 0, files: [] }];
      if (command === "retry_session_recording_save") {
        assert.deepEqual(args, { request: { recordingId: "recovered", directory: "/new" } });
        return { status: "saved", filePath: "/new/video.webm", fileName: "video.webm", bytesWritten: 5, files: [{ filePath: "/new/video.webm", fileName: "video.webm", bytesWritten: 5 }] };
      }
    },
  });
  await owner.refreshRecordingJobs(true); await owner.retryRecordingJob("recovered", true);
  assert.equal(owner.recordingJobs.value[0]?.status, "saved");
  assert.ok(calls.includes("grant_file_access_directory")); assert.ok(calls.includes("retry_session_recording_save"));
  assert.equal(owner.isSessionRecording("s1"), false);
});
