import { computed, getCurrentScope, nextTick, onScopeDispose, ref, watch, type Ref } from "vue";
import type { EventLogLevel } from "@diagnostics/eventLog";
import {
  createScreenshotArtifact,
  createSessionArtifactRecorder,
  summarizeArtifactDiagnostics,
  type SessionArtifact,
} from "../../../session/artifacts/sessionArtifacts.ts";
import { createSessionRecordingTask, type RecordingFileResult, type RecordingSpaceStatus } from "../../../session/artifacts/sessionRecordingTask.ts";
import type { SessionRecordingSource } from "../../../session/artifacts/sessionRecordingSource.ts";
import type { RemoteSession, RemoteSessionTab } from "@session/core/sessionManager";

type WriteArtifactResult = {
  filePath: string;
  fileName: string;
  bytesWritten: number;
};

type SessionArtifactWriteRequest = {
  directory: string;
  fileName: string;
  dataBase64: string;
};

export type SessionArtifactsInvoke = (
  command:
    | "inspect_session_artifact_directory"
    | "write_session_artifact_file"
    | "start_session_recording_file"
    | "append_session_recording_chunk"
    | "finish_session_recording_file"
    | "release_session_recording_file"
    | "inspect_session_recording_space"
    | "list_session_recording_jobs"
    | "recover_session_recordings"
    | "retry_session_recording_save"
    | "relocate_session_recording_file"
    | "reveal_authorized_path"
    | "open_authorized_path"
    | "grant_file_access_directory",
  args?: Record<string, unknown>,
) => Promise<unknown>;

type ArtifactDirectoryStatus = {
  directory: string;
  status: "empty" | "missing" | "not_directory" | "not_writable" | "ready";
  exists: boolean;
  isDirectory: boolean;
  writable: boolean;
  message: string;
};

type FileAccessActionResult = {
  status: "opened" | "authorized" | "needs_authorization" | "invalid_path";
  path: string;
  directory: string;
  message: string;
};

type SessionArtifactsOptions = {
  manageBackground?: boolean;
  chooseRecordingDirectory?: () => Promise<string | null>;
  activeRemoteSession: Ref<RemoteSession | null>;
  sessionTabs: Ref<RemoteSessionTab[]>;
  artifactDirectory: Ref<string>;
  getDiagnosticSummary: () => string;
  createRecordingSource: (sessionId: string, signal: AbortSignal, onError: (code: string) => void) => Promise<SessionRecordingSource>;
  onRecordingChanged?: (sessionId: string) => void;
  getSession: (sessionId: string) => RemoteSession | undefined;
  getViewportTarget: (sessionId: string) => HTMLElement | undefined;
  invoke: SessionArtifactsInvoke;
  isTauriUnavailableError: (error: unknown) => boolean;
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  notify: (content: string, tone?: "info" | "success" | "warning" | "error") => void;
  isArtifactResultVisible?: (sessionId: string) => boolean;
  requestDirectoryAuthorization: (directory: string) => Promise<boolean>;
  t: (key: string, named?: Record<string, string | number>) => string;
};

type ArtifactFeedback = SessionArtifactsOptions["notify"];

type RecordingRuntime = ReturnType<typeof createSessionRecordingTask>;
export type RecordingJob = {
  recordingId: string; fileName: string; recoveryPath: string; status: string;
  progress: number; durationMs: number; files: WriteArtifactResult[]; error?: string;
};
type BackgroundRecording = { task: RecordingRuntime; artifact?: SessionArtifact; reason?: string; notify: ArtifactFeedback; busy: boolean };

function canvasForSession(target: HTMLElement | undefined): HTMLCanvasElement | null {
  return target?.querySelector<HTMLCanvasElement>("canvas") ?? null;
}

function dataUrlToBase64(dataUrl: string): string {
  return dataUrl.split(",", 2)[1] ?? "";
}

async function blobToBase64(blob: Blob): Promise<string> {
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error ?? new Error("failed to read artifact blob"));
    reader.readAsDataURL(blob);
  });
  return dataUrlToBase64(dataUrl);
}

function recordingMimeType(): string | undefined {
  const candidates = ["video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm"];
  const mediaRecorder = globalThis.MediaRecorder;
  if (!mediaRecorder) {
    return undefined;
  }

  return candidates.find((candidate) => mediaRecorder.isTypeSupported(candidate));
}

export function useSessionArtifacts(options: SessionArtifactsOptions) {
  const artifacts = ref<SessionArtifact[]>([]);
  const artifactMessage = ref("");
  const artifactMessageTone = ref<"default" | "error">("default");
  const recordingRuntimes = new Map<string, RecordingRuntime>();
  const recordingTick = ref(0);
  const recorder = createSessionArtifactRecorder();
  const backgroundRuntimes = new Map<string, BackgroundRecording>();
  const nativeJobs = ref<RecordingJob[]>([]);
  const localJobs = ref<Record<string, RecordingJob>>({});
  const recordingJobs = computed(() => {
    const jobs = new Map(nativeJobs.value.map(job => [job.recordingId, job]));
    for (const job of Object.values(localJobs.value)) jobs.set(job.recordingId, job);
    return [...jobs.values()].filter(job => job.status !== "recording"
      && !(job.status === "saved" && artifacts.value.some(artifact => artifact.recordingId === job.recordingId)));
  });
  let refreshing = false;
  let disposed = false;
  async function refreshRecordingJobs(recover = false) {
    if (refreshing || disposed) return;
    refreshing = true;
    try {
      const result = await options.invoke(recover ? "recover_session_recordings" : "list_session_recording_jobs",
        recover ? { directory: options.artifactDirectory.value } : undefined);
      if (!Array.isArray(result) || disposed) return;
      nativeJobs.value = result;
      if (recover && result.some(job => job.status === "recoverable")) options.notify(options.t("sessions.artifacts.recordingRecoveryFound"), "warning");
      for (const job of result as RecordingJob[]) {
        const local = localJobs.value[job.recordingId];
        if (!local) continue;
        const background = backgroundRuntimes.get(job.recordingId);
        if (job.status !== "recording" && (background?.busy || ["saving", "merging", "verifying", "failed", "segmented"].includes(local.status))) {
          localJobs.value = { ...localJobs.value, [job.recordingId]: job };
        } else if (job.files.length) {
          localJobs.value = { ...localJobs.value, [job.recordingId]: { ...local, files: job.files } };
        }
      }
    } catch { /* A missing backend in browser previews must not break the workspace. */ }
    finally { refreshing = false; }
  }
  if (options.manageBackground) {
    void refreshRecordingJobs(true);
    const timer = setInterval(() => { void refreshRecordingJobs(); }, 1000);
    if (getCurrentScope()) onScopeDispose(() => { disposed = true; clearInterval(timer); });
    watch(options.artifactDirectory, () => { void refreshRecordingJobs(true); });
  }

  function detachRecording(sessionId: string, task: RecordingRuntime, reason: string | undefined, notify: ArtifactFeedback) {
    const id = task.recordingId;
    if (!id || backgroundRuntimes.has(id)) return;
    const stopped = recorder.stopRecording({ sessionId, diagnosticSummary: summarizeArtifactDiagnostics(options.getDiagnosticSummary()) });
    if (recordingRuntimes.get(sessionId) === task) recordingRuntimes.delete(sessionId);
    backgroundRuntimes.set(id, { task, artifact: stopped.artifact, reason, notify, busy: true });
    localJobs.value = { ...localJobs.value, [id]: { recordingId: id, fileName: stopped.artifact?.fileName ?? "",
      recoveryPath: task.recoveryPath ?? "", status: "saving", progress: 0, durationMs: task.elapsedMs, files: [] } };
    recordingChanged(sessionId);
  }

  async function completeRecording(id: string, result: RecordingFileResult | null, width?: number, height?: number, durationMs?: number) {
    const background = backgroundRuntimes.get(id);
    if (!background || !result) return;
    background.busy = false;
    const files = result.files ?? [{ filePath: result.filePath, fileName: result.fileName, bytesWritten: result.bytesWritten }];
    const status = result.status ?? "saved";
    localJobs.value = { ...localJobs.value, [id]: { ...localJobs.value[id]!, status, files, progress: 100 } };
    if (status === "segmented") {
      background.notify(options.t("sessions.artifacts.recordingSegmented"), "warning");
      return;
    }
    const previous = artifacts.value.find(artifact => artifact.recordingId === id);
    const base = background.artifact ?? previous;
    if (base) {
      artifacts.value = [{ ...base, recordingId: id, filePath: result.filePath, fileName: result.fileName,
        width: width ?? base.width, height: height ?? base.height,
        durationSeconds: durationMs === undefined ? base.durationSeconds : Math.floor(durationMs / 1000) },
      ...artifacts.value.filter(artifact => artifact.recordingId !== id)];
    }
    const message = options.t("sessions.artifacts.recordingSaved", { fileName: result.fileName });
    artifactMessage.value = message; artifactMessageTone.value = "default";
    options.logEvent("success", options.t("log.sourceSession"), message);
    if (background.reason) background.notify(`${options.t("sessions.artifacts.recordingInterrupted")} ${message}`, "warning");
    else await notifySavedArtifact(base?.sessionId ?? "", message, background.notify);
    backgroundRuntimes.delete(id);
  }
  function failRecording(id: string, error: unknown) {
    const background = backgroundRuntimes.get(id);
    if (background) background.busy = false;
    const job = localJobs.value[id];
    if (job) localJobs.value = { ...localJobs.value, [id]: { ...job, status: "failed", error: String(error) } };
    artifactMessage.value = options.t("sessions.artifacts.recordingSaveFailed", { path: job?.recoveryPath ?? "" });
    artifactMessageTone.value = "error";
    (background?.notify ?? options.notify)(artifactMessage.value, "error");
    options.logEvent("error", options.t("log.sourceSession"), artifactMessage.value);
  }

  async function retryRecordingJob(id: string, chooseDirectory = false) {
    const job = recordingJobs.value.find(job => job.recordingId === id);
    if (!job || ["saving", "merging", "verifying"].includes(job.status)) return;
    let directory: string | null = null;
    try {
      if (chooseDirectory) {
        directory = await options.chooseRecordingDirectory?.() ?? null;
        if (!directory) return;
        if (!await options.requestDirectoryAuthorization(directory)) return;
        await options.invoke("grant_file_access_directory", { request: { directory } });
      }
    } catch {
      options.notify(options.t("sessions.artifacts.recordingChooseDirectoryFailed"), "error");
      return;
    }
    const background = backgroundRuntimes.get(id);
    if (background?.busy) return;
    if (background) background.busy = true;
    localJobs.value = { ...localJobs.value, [id]: { ...job, status: "saving", progress: 0 } };
    try {
      let result: RecordingFileResult | null;
      if (background?.task.phase === "failed") {
        if (directory) {
          await background.task.relocate(directory);
          localJobs.value = { ...localJobs.value, [id]: { ...localJobs.value[id]!, recoveryPath: background.task.recoveryPath ?? job.recoveryPath } };
        }
        const captured = await background.task.stop();
        result = captured;
        await completeRecording(id, result, captured?.width, captured?.height, captured?.durationMs);
      } else {
        result = await options.invoke("retry_session_recording_save", { request: { recordingId: id, directory } }) as RecordingFileResult;
        if (background) await completeRecording(id, result);
        else {
          localJobs.value = { ...localJobs.value, [id]: { ...job, status: result?.status ?? "saved", files: result?.files ?? [], progress: 100 } };
          options.notify(options.t(result?.status === "segmented" ? "sessions.artifacts.recordingSegmented" : "sessions.artifacts.recordingSaved", { fileName: result?.fileName ?? job.fileName }), result?.status === "segmented" ? "warning" : "success");
        }
      }
    } catch (error) { failRecording(id, error); }
    finally { if (background) background.busy = false; void refreshRecordingJobs(); }
  }

  async function notifySavedArtifact(sessionId: string, message: string, notify: ArtifactFeedback) {
    // Let the newly saved row render before deciding whether a Toast would repeat it.
    await nextTick();
    if (!options.isArtifactResultVisible?.(sessionId)) notify(message, "success");
  }

  const activeSessionArtifacts = computed(() => {
    const sessionId = options.activeRemoteSession.value?.id;
    if (!sessionId) {
      return [];
    }
    return artifacts.value.filter((artifact) => artifact.sessionId === sessionId);
  });

  const recordingSessionIds = computed(() => {
    recordingTick.value;
    return new Set(recordingRuntimes.keys());
  });

  watch([options.sessionTabs, recordingSessionIds], () => {
    for (const [sessionId, task] of recordingRuntimes) {
      if ((task.phase === "starting" || task.phase === "recording")
        && options.getSession(sessionId)?.state !== "connected") {
        void stopSessionRecording(sessionId, "recording_disconnected");
      }
    }
  }, { deep: true });

  function requireDirectory(notify: ArtifactFeedback): string | null {
    const directory = options.artifactDirectory.value.trim();
    if (!directory) {
      artifactMessage.value = options.t("sessions.artifacts.directoryRequired");
      artifactMessageTone.value = "error";
      notify(artifactMessage.value, "warning");
      return null;
    }
    return directory;
  }

  async function ensureArtifactDirectoryReady(directory: string, notify: ArtifactFeedback): Promise<boolean> {
    try {
      const status = await options.invoke("inspect_session_artifact_directory", { directory }) as ArtifactDirectoryStatus;
      if (status.status === "ready") {
        return true;
      }

      artifactMessage.value = options.t(`sessions.artifacts.directoryStatus.${status.status}`, {
        directory: status.directory,
        message: status.message,
      });
      artifactMessageTone.value = "error";
      notify(artifactMessage.value, "error");
      options.logEvent("error", options.t("log.sourceSession"), artifactMessage.value);
      return false;
    } catch (error) {
      artifactMessage.value = options.t("sessions.artifacts.directoryInspectFailed", { error: String(error) });
      artifactMessageTone.value = "error";
      notify(artifactMessage.value, "error");
      options.logEvent("error", options.t("log.sourceSession"), artifactMessage.value);
      return false;
    }
  }

  async function writeArtifactFile(request: SessionArtifactWriteRequest): Promise<WriteArtifactResult> {
    return await options.invoke("write_session_artifact_file", { request }) as WriteArtifactResult;
  }

  async function captureSessionScreenshot(sessionId: string, notify = options.notify) {
    const session = options.getSession(sessionId);
    const directory = requireDirectory(notify);
    if (!session || !directory) {
      return;
    }
    if (!await ensureArtifactDirectoryReady(directory, notify)) {
      return;
    }

    const canvas = canvasForSession(options.getViewportTarget(sessionId));
    if (!canvas || canvas.width <= 0 || canvas.height <= 0) {
      notify(options.t("sessions.artifacts.captureUnavailable"), "warning");
      return;
    }

    try {
      const createdAt = new Date().toISOString();
      const artifact = createScreenshotArtifact({
        sessionId: session.id,
        target: session.target,
        createdAt,
        directory,
        width: canvas.width,
        height: canvas.height,
        diagnosticSummary: options.getDiagnosticSummary(),
      });
      const written = await writeArtifactFile({
        directory,
        fileName: artifact.fileName,
        dataBase64: dataUrlToBase64(canvas.toDataURL("image/png")),
      });
      artifacts.value = [{
        ...artifact,
        filePath: written.filePath,
        fileName: written.fileName,
      }, ...artifacts.value];
      const savedMessage = options.t("sessions.artifacts.screenshotSaved", { fileName: written.fileName });
      artifactMessage.value = savedMessage;
      artifactMessageTone.value = "default";
      options.logEvent("success", options.t("log.sourceSession"), savedMessage);
      await notifySavedArtifact(sessionId, savedMessage, notify);
    } catch (error) {
      artifactMessage.value = options.t("sessions.artifacts.writeFailed", { error: String(error) });
      artifactMessageTone.value = "error";
      notify(artifactMessage.value, "error");
      options.logEvent("error", options.t("log.sourceSession"), artifactMessage.value);
    }
  }

  function isSessionRecording(sessionId: string): boolean {
    recordingTick.value;
    return recordingRuntimes.has(sessionId);
  }

  function getSessionRecordingPhase(sessionId: string) {
    recordingTick.value;
    return recordingRuntimes.get(sessionId)?.phase;
  }

  function getSessionRecordingSpace(sessionId: string): RecordingSpaceStatus | undefined {
    recordingTick.value;
    return recordingRuntimes.get(sessionId)?.spaceStatus;
  }

  function getSessionRecordingElapsedSeconds(sessionId: string) {
    recordingTick.value;
    return Math.floor((recordingRuntimes.get(sessionId)?.elapsedMs ?? 0) / 1000);
  }

  function recordingChanged(sessionId: string) {
    recordingTick.value += 1;
    options.onRecordingChanged?.(sessionId);
  }

  async function startSessionRecording(sessionId: string, notify = options.notify) {
    const session = options.getSession(sessionId);
    const directory = requireDirectory(notify);
    if (!session || !directory) {
      return;
    }
    if (!await ensureArtifactDirectoryReady(directory, notify)) {
      return;
    }

    if (recordingRuntimes.has(sessionId)) {
      notify(options.t("sessions.artifacts.recordingAlreadyActive"), "warning");
      return;
    }

    const mediaRecorder = globalThis.MediaRecorder;
    const mimeType = recordingMimeType();
    if (!mediaRecorder || !mimeType || session.state !== "connected") {
      notify(options.t("sessions.artifacts.recordingUnavailable"), "warning");
      return;
    }

    let task: RecordingRuntime | undefined;
    let stopReason: string | undefined;
    try {
      const started = recorder.startRecording({
        sessionId: session.id,
        target: session.target,
        directory,
        mimeType,
      });
      if (started.status === "already-recording") {
        notify(options.t("sessions.artifacts.recordingAlreadyActive"), "warning");
        return;
      }

      task = createSessionRecordingTask({
        createSource: (signal, onError) => options.createRecordingSource(sessionId, signal, onError),
        createRecorder: (stream) => new mediaRecorder(stream, { mimeType }),
        sink: {
          start: async () => await options.invoke("start_session_recording_file", {
            request: { directory, fileName: started.recording.fileName },
          }) as { recordingId: string; recoveryPath: string },
          append: async (recordingId, offset, chunk) => {
            await options.invoke("append_session_recording_chunk", {
              request: { recordingId, offset, dataBase64: await blobToBase64(chunk) },
            });
          },
          finish: async (recordingId, durationMs) => await options.invoke("finish_session_recording_file", {
            request: { recordingId, durationMs },
          }) as WriteArtifactResult,
          relocate: async (recordingId, directory) => await options.invoke("relocate_session_recording_file", { request: { recordingId, directory } }) as { recoveryPath: string },
          space: async (recordingId, pendingBytes) => await options.invoke("inspect_session_recording_space", {
            request: { recordingId, pendingBytes },
          }) as RecordingSpaceStatus,
          release: async (recordingId) => { await options.invoke("release_session_recording_file", { recordingId }); },
        },
        onPhase: () => recordingChanged(sessionId),
        onCaptureStopped: () => { if (task) detachRecording(sessionId, task, stopReason, notify); },
        onSpaceChanged: (_status, enteredWarning) => {
          recordingChanged(sessionId);
          if (enteredWarning) notify(options.t("sessions.artifacts.recordingMergeSpaceWarning"), "warning");
        },
        onTick: () => recordingChanged(sessionId),
        onStopRequested: (reason) => {
          stopReason = reason;
          if (reason === "recording_space_insufficient") notify(options.t("sessions.artifacts.recordingSpaceStopped"), "warning");
          else if (reason === "recording_space_check_failed") notify(options.t("sessions.artifacts.recordingSpaceCheckStopped"), "warning");
          void stopSessionRecording(sessionId, reason, notify);
        },
      });
      recordingRuntimes.set(sessionId, task);
      recordingChanged(sessionId);
      await task.start();
      if (recordingRuntimes.get(sessionId) !== task || task.phase !== "recording") return;
      artifactMessage.value = options.t("sessions.artifacts.recordingStarted", { target: session.target });
      artifactMessageTone.value = "default";
      options.logEvent("info", options.t("log.sourceSession"), artifactMessage.value);
    } catch (error) {
      if (task && recordingRuntimes.get(sessionId) !== task) return;
      recorder.stopRecording({ sessionId });
      recordingRuntimes.delete(sessionId);
      recordingChanged(sessionId);
      if (error instanceof Error && error.message === "recording_cancelled") return;
      artifactMessage.value = options.t("sessions.artifacts.recordingStartFailed");
      artifactMessageTone.value = "error";
      notify(artifactMessage.value, "error");
    }
  }

  async function stopSessionRecording(sessionId: string, reason?: string, notify = options.notify) {
    const runtime = recordingRuntimes.get(sessionId);
    if (!runtime) return;
    if (runtime.phase === "stopping") return;
    // Capture release detaches this runtime before its queued writes and native
    // background finalization complete, allowing another recording immediately.
    const stopping = runtime.stop();
    const processing = (async () => {
      try {
        const written = await stopping;
        const id = runtime.recordingId;
        if (id) {
          const background = backgroundRuntimes.get(id);
          if (background) { background.reason = reason; background.notify = notify; }
          await completeRecording(id, written, written?.width, written?.height, written?.durationMs);
        }
        if (recordingRuntimes.get(sessionId) === runtime) {
          recordingRuntimes.delete(sessionId); recorder.stopRecording({ sessionId }); recordingChanged(sessionId);
        }
      } catch (error) {
        detachRecording(sessionId, runtime, reason, notify);
        if (runtime.recordingId) failRecording(runtime.recordingId, error);
      } finally { void refreshRecordingJobs(); }
    })();
    await Promise.race([runtime.captureStopped, processing]);
  }

  async function toggleSessionRecording(sessionId: string, notify = options.notify) {
    if (isSessionRecording(sessionId)) {
      await stopSessionRecording(sessionId, undefined, notify);
      return;
    }

    await startSessionRecording(sessionId, notify);
  }

  async function copySessionArtifactPath(artifact: SessionArtifact) {
    try {
      await navigator.clipboard.writeText(artifact.filePath);
      options.notify(options.t("sessions.artifacts.pathCopied"), "success");
    } catch (error) {
      options.notify(options.t("sessions.artifacts.pathCopyFailed", { error: String(error) }), "error");
    }
  }

  async function runAuthorizedArtifactAction(
    command: "reveal_authorized_path" | "open_authorized_path",
    artifact: SessionArtifact,
    failureKey: "sessions.artifacts.revealFailed" | "sessions.artifacts.openFailed",
  ) {
    try {
      const result = await options.invoke(command, { request: { path: artifact.filePath } }) as FileAccessActionResult;
      if (result.status === "opened") {
        return;
      }

      if (result.status !== "needs_authorization") {
        throw new Error(result.message);
      }

      const allowed = await options.requestDirectoryAuthorization(result.directory);
      if (!allowed) {
        artifactMessage.value = options.t("sessions.artifacts.accessDenied", { directory: result.directory });
        artifactMessageTone.value = "error";
        options.notify(artifactMessage.value, "error");
        options.logEvent("error", options.t("log.sourceSession"), artifactMessage.value);
        return;
      }

      await options.invoke("grant_file_access_directory", { request: { directory: result.directory } });
      const retry = await options.invoke(command, { request: { path: artifact.filePath } }) as FileAccessActionResult;
      if (retry.status !== "opened") {
        throw new Error(retry.message);
      }
    } catch (error) {
      artifactMessage.value = options.t(failureKey, { error: String(error) });
      artifactMessageTone.value = "error";
      options.notify(artifactMessage.value, "error");
    }
  }

  async function revealSessionArtifactFile(artifact: SessionArtifact) {
    await runAuthorizedArtifactAction("reveal_authorized_path", artifact, "sessions.artifacts.revealFailed");
  }

  async function openSessionArtifactFile(artifact: SessionArtifact) {
    await runAuthorizedArtifactAction("open_authorized_path", artifact, "sessions.artifacts.openFailed");
  }

  return {
    activeSessionArtifacts,
    recordingJobs,
    refreshRecordingJobs,
    retryRecordingJob,
    artifactMessage,
    artifactMessageTone,
    artifacts,
    copySessionArtifactPath,
    captureSessionScreenshot,
    isSessionRecording,
    getSessionRecordingPhase,
    getSessionRecordingSpace,
    getSessionRecordingElapsedSeconds,
    openSessionArtifactFile,
    recordingSessionIds,
    revealSessionArtifactFile,
    startSessionRecording,
    stopSessionRecording,
    toggleSessionRecording,
  };
}
