import assert from "node:assert/strict";
import test from "node:test";
import { createViewportRuntimeEvidence, summarizeViewportRuntimeEvidence } from "../../../src/session/diagnostics/runtimeEvidence.ts";

test("runtime evidence summarizes native Core transport and Surface presentation", () => {
  const evidence = createViewportRuntimeEvidence({
    transportId: "transport-1", canvasSizes: [{ width: 1024, height: 768 }], framebuffer: { width: 1920, height: 1080 },
    latestCoreEvent: "framebuffer committed", latestCoreEventAtMs: 1782813600000, capturedAtMs: 1782813601000,
    displaySource: "core_framebuffer", inputPath: "native_rfb_input",
    surface: { presentedFrames: 12, firstFrameAtMs: 1782813599000, latestFrameAtMs: 1782813600000, estimatedFps: 29.97,
      latestFrameIntervalMs: 33, maxFrameIntervalMs: 120, averageFrameIntervalMs: 45, framebuffer: { width: 1920, height: 1080 } },
  });
  const summary = summarizeViewportRuntimeEvidence(evidence);
  assert.equal(summary.health, "evidence_available");
  assert.equal(summary.transportId, "transport-1");
  assert.equal(summary.displaySource, "core_framebuffer");
  assert.equal(summary.inputPath, "native_rfb_input");
  assert.match(summary.surface, /frames=12/);
  assert.match(summary.surface, /average_submit_fps=29.97/);
  assert.match(summary.surface, /framebuffer=1920x1080/);
});

test("runtime evidence reports each missing native Core runtime piece", () => {
  assert.equal(summarizeViewportRuntimeEvidence(createViewportRuntimeEvidence({ transportId: "", canvasSizes: [{ width: 800, height: 600 }], framebuffer: { width: 800, height: 600 }, latestCoreEvent: "connected" })).health, "missing_transport");
  assert.equal(summarizeViewportRuntimeEvidence(createViewportRuntimeEvidence({ transportId: "transport-1", canvasSizes: [], framebuffer: { width: 800, height: 600 }, latestCoreEvent: "connected" })).health, "no_canvas");
  assert.equal(summarizeViewportRuntimeEvidence(createViewportRuntimeEvidence({ transportId: "transport-1", canvasSizes: [{ width: 800, height: 600 }], framebuffer: { width: 0, height: 0 }, latestCoreEvent: "connected" })).health, "no_framebuffer");
  assert.equal(summarizeViewportRuntimeEvidence(createViewportRuntimeEvidence({ transportId: "transport-1", canvasSizes: [{ width: 800, height: 600 }], framebuffer: { width: 800, height: 600 }, latestCoreEvent: "" })).health, "no_core_event");
});

test("runtime evidence uses stable placeholders without evidence", () => {
  assert.equal(summarizeViewportRuntimeEvidence(null).transportId, "-");
  assert.equal(summarizeViewportRuntimeEvidence(null).surface, "-");
});

test("old frame evidence describes a past submission without claiming live health", () => {
  const summary = summarizeViewportRuntimeEvidence(createViewportRuntimeEvidence({
    transportId: "native:audit", canvasSizes: [{ width: 800, height: 600 }],
    framebuffer: { width: 800, height: 600 }, latestCoreEvent: "surface_frame:1",
    latestCoreEventAtMs: 1, capturedAtMs: 1,
  }));
  assert.equal(summary.health, "evidence_available");
  assert.equal(summary.healthText, "已记录画面提交");
  assert.equal(summary.capturedAt, "1970-01-01T00:00:00.001Z");
});
