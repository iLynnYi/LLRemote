import assert from "node:assert/strict";
import test from "node:test";

import {
  createScreenshotArtifact,
  createSessionArtifactRecorder,
  formatArtifactFileName,
  summarizeArtifactDiagnostics,
  type SessionArtifact,
} from "../../../src/session/artifacts/sessionArtifacts.ts";

test("formatArtifactFileName creates stable sanitized names", () => {
  assert.equal(
    formatArtifactFileName({
      createdAt: "2026-07-10T08:09:10.123Z",
      target: "192.168.1.20:5900",
      sessionId: "session-abcdef123456",
      kind: "screenshot",
      extension: "png",
    }),
    "20260710-080910_192.168.1.20_5900_session-_screenshot.png",
  );
});

test("createScreenshotArtifact records safe session context and dimensions", () => {
  const artifact = createScreenshotArtifact({
    id: () => "artifact-1",
    sessionId: "session-1",
    target: "lab-mac:5900",
    hostNameSnapshot: "Lab Mac",
    createdAt: "2026-07-10T08:09:10.123Z",
    directory: "/tmp/evidence",
    width: 1280,
    height: 720,
    diagnosticSummary: "connected; password=secret; token:abcd",
  });

  assert.deepEqual(artifact, {
    id: "artifact-1",
    sessionId: "session-1",
    target: "lab-mac:5900",
    hostNameSnapshot: "Lab Mac",
    createdAt: "2026-07-10T08:09:10.123Z",
    kind: "screenshot",
    filePath: "/tmp/evidence/20260710-080910_lab-mac_5900_session-_screenshot.png",
    fileName: "20260710-080910_lab-mac_5900_session-_screenshot.png",
    mimeType: "image/png",
    width: 1280,
    height: 720,
    diagnosticSummary: "connected; password=[redacted]; token:[redacted]",
  } satisfies SessionArtifact);
});

test("summarizeArtifactDiagnostics redacts sensitive fields", () => {
  assert.equal(
    summarizeArtifactDiagnostics("auth failed password:secret token=abcd pwd=hunter2"),
    "auth failed password:[redacted] token=[redacted] pwd=[redacted]",
  );
});

test("session artifact recorder prevents duplicate recordings and produces recording artifacts", () => {
  const recorder = createSessionArtifactRecorder({
    id: () => "artifact-1",
    now: () => "2026-07-10T08:09:10.123Z",
  });

  const started = recorder.startRecording({
    sessionId: "session-1",
    target: "lab-mac:5900",
    directory: "/tmp/evidence",
    mimeType: "video/webm",
  });
  const duplicate = recorder.startRecording({
    sessionId: "session-1",
    target: "lab-mac:5900",
    directory: "/tmp/evidence",
    mimeType: "video/webm",
  });
  const stopped = recorder.stopRecording({
    sessionId: "session-1",
    stoppedAt: "2026-07-10T08:10:12.500Z",
    width: 1280,
    height: 720,
    diagnosticSummary: "connected",
  });

  assert.equal(started.status, "recording");
  assert.equal(duplicate.status, "already-recording");
  assert.equal(stopped.status, "stopped");
  assert.equal(stopped.artifact?.kind, "recording");
  assert.equal(stopped.artifact?.durationSeconds, 62);
  assert.equal(stopped.artifact?.fileName, "20260710-080910_lab-mac_5900_session-_recording.webm");
  assert.equal(recorder.getRecording("session-1"), undefined);
});
