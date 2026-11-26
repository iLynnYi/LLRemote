export type SessionArtifactKind = "screenshot" | "recording";

export type SessionArtifact = {
  id: string;
  sessionId: string;
  target: string;
  hostNameSnapshot?: string;
  createdAt: string;
  kind: SessionArtifactKind;
  filePath: string;
  fileName: string;
  mimeType: string;
  width?: number;
  height?: number;
  durationSeconds?: number;
  diagnosticSummary?: string;
  recordingId?: string;
};

export type ArtifactFileNameInput = {
  createdAt: string;
  target: string;
  sessionId: string;
  kind: SessionArtifactKind;
  extension: string;
};

export type CreateScreenshotArtifactInput = {
  id?: () => string;
  sessionId: string;
  target: string;
  hostNameSnapshot?: string;
  createdAt: string;
  directory: string;
  width: number;
  height: number;
  diagnosticSummary?: string;
};

export type SessionArtifactRecording = {
  sessionId: string;
  target: string;
  hostNameSnapshot?: string;
  startedAt: string;
  directory: string;
  mimeType: string;
  fileName: string;
};

export type SessionArtifactRecorderOptions = {
  id?: () => string;
  now?: () => string;
};

export type StartRecordingInput = {
  sessionId: string;
  target: string;
  hostNameSnapshot?: string;
  directory: string;
  mimeType: string;
};

export type StopRecordingInput = {
  sessionId: string;
  stoppedAt?: string;
  width?: number;
  height?: number;
  diagnosticSummary?: string;
};

export type StartRecordingResult =
  | { status: "recording"; recording: SessionArtifactRecording }
  | { status: "already-recording"; recording: SessionArtifactRecording };

export type StopRecordingResult =
  | { status: "stopped"; artifact: SessionArtifact }
  | { status: "not-recording"; artifact?: undefined };

function defaultId(): string {
  return crypto.randomUUID();
}

function extensionForMimeType(mimeType: string): string {
  if (mimeType.includes("webm")) {
    return "webm";
  }
  if (mimeType.includes("jpeg") || mimeType.includes("jpg")) {
    return "jpg";
  }
  return "png";
}

function joinPath(directory: string, fileName: string): string {
  const trimmed = directory.trim();
  return `${trimmed.replace(/[\\/]+$/, "")}/${fileName}`;
}

function timestampForFileName(createdAt: string): string {
  const date = new Date(createdAt);
  if (Number.isNaN(date.getTime())) {
    return "unknown-time";
  }

  return [
    String(date.getUTCFullYear()).padStart(4, "0"),
    String(date.getUTCMonth() + 1).padStart(2, "0"),
    String(date.getUTCDate()).padStart(2, "0"),
    "-",
    String(date.getUTCHours()).padStart(2, "0"),
    String(date.getUTCMinutes()).padStart(2, "0"),
    String(date.getUTCSeconds()).padStart(2, "0"),
  ].join("");
}

function safeFileSegment(value: string): string {
  return value
    .trim()
    .replace(/[^a-zA-Z0-9._-]+/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 80) || "unknown";
}

export function summarizeArtifactDiagnostics(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  if (!trimmed) {
    return undefined;
  }

  return trimmed
    .replace(/\b(password|passwd|pwd|secret|token)\s*=\s*[^;\s]+/gi, "$1=[redacted]")
    .replace(/\b(password|passwd|pwd|secret|token)\s*:\s*[^;\s]+/gi, "$1:[redacted]");
}

export function formatArtifactFileName(input: ArtifactFileNameInput): string {
  const extension = safeFileSegment(input.extension.replace(/^\./, "").toLowerCase());
  return [
    timestampForFileName(input.createdAt),
    safeFileSegment(input.target),
    safeFileSegment(input.sessionId).slice(0, 8),
    input.kind,
  ].join("_") + `.${extension}`;
}

export function createScreenshotArtifact(input: CreateScreenshotArtifactInput): SessionArtifact {
  const mimeType = "image/png";
  const fileName = formatArtifactFileName({
    createdAt: input.createdAt,
    target: input.target,
    sessionId: input.sessionId,
    kind: "screenshot",
    extension: "png",
  });

  return {
    id: input.id?.() ?? defaultId(),
    sessionId: input.sessionId,
    target: input.target,
    hostNameSnapshot: input.hostNameSnapshot,
    createdAt: input.createdAt,
    kind: "screenshot",
    filePath: joinPath(input.directory, fileName),
    fileName,
    mimeType,
    width: input.width,
    height: input.height,
    diagnosticSummary: summarizeArtifactDiagnostics(input.diagnosticSummary),
  };
}

export function createSessionArtifactRecorder(options: SessionArtifactRecorderOptions = {}) {
  const recordings = new Map<string, SessionArtifactRecording>();
  const id = options.id ?? defaultId;
  const now = options.now ?? (() => new Date().toISOString());

  return {
    getRecording(sessionId: string): SessionArtifactRecording | undefined {
      return recordings.get(sessionId);
    },

    listRecordings(): SessionArtifactRecording[] {
      return [...recordings.values()];
    },

    startRecording(input: StartRecordingInput): StartRecordingResult {
      const existing = recordings.get(input.sessionId);
      if (existing) {
        return { status: "already-recording", recording: existing };
      }

      const startedAt = now();
      const fileName = formatArtifactFileName({
        createdAt: startedAt,
        target: input.target,
        sessionId: input.sessionId,
        kind: "recording",
        extension: extensionForMimeType(input.mimeType),
      });
      const recording: SessionArtifactRecording = {
        sessionId: input.sessionId,
        target: input.target,
        hostNameSnapshot: input.hostNameSnapshot,
        startedAt,
        directory: input.directory,
        mimeType: input.mimeType,
        fileName,
      };
      recordings.set(input.sessionId, recording);
      return { status: "recording", recording };
    },

    stopRecording(input: StopRecordingInput): StopRecordingResult {
      const recording = recordings.get(input.sessionId);
      if (!recording) {
        return { status: "not-recording" };
      }

      recordings.delete(input.sessionId);
      const stoppedAt = input.stoppedAt ?? now();
      const durationSeconds = Math.max(0, Math.floor((Date.parse(stoppedAt) - Date.parse(recording.startedAt)) / 1000));
      return {
        status: "stopped",
        artifact: {
          id: id(),
          sessionId: recording.sessionId,
          target: recording.target,
          hostNameSnapshot: recording.hostNameSnapshot,
          createdAt: recording.startedAt,
          kind: "recording",
          filePath: joinPath(recording.directory, recording.fileName),
          fileName: recording.fileName,
          mimeType: recording.mimeType,
          width: input.width,
          height: input.height,
          durationSeconds,
          diagnosticSummary: summarizeArtifactDiagnostics(input.diagnosticSummary),
        },
      };
    },
  };
}
