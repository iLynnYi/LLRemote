export type RuntimeEvidenceSize = { width: number; height: number };
export type RuntimeEvidenceDisplaySource = "core_framebuffer";
export type RuntimeEvidenceInputPath = "native_rfb_input";

export type CoreSurfaceRuntimeEvidence = {
  presentedFrames: number;
  firstFrameAtMs: number;
  latestFrameAtMs: number;
  estimatedFps?: number;
  latestFrameIntervalMs?: number;
  maxFrameIntervalMs?: number;
  averageFrameIntervalMs?: number;
  framebuffer: RuntimeEvidenceSize;
};

export type ViewportRuntimeEvidence = {
  canvasCount: number;
  canvasSizes: RuntimeEvidenceSize[];
  framebuffer: RuntimeEvidenceSize;
  transportId: string;
  latestCoreEvent: string;
  latestCoreEventAtMs: number;
  capturedAtMs: number;
  displaySource?: RuntimeEvidenceDisplaySource;
  inputPath?: RuntimeEvidenceInputPath;
  surface?: CoreSurfaceRuntimeEvidence;
};

export type ViewportRuntimeEvidenceInput = Partial<Omit<ViewportRuntimeEvidence, "canvasCount" | "canvasSizes" | "framebuffer" | "capturedAtMs">> & {
  canvasSizes?: RuntimeEvidenceSize[];
  framebuffer?: RuntimeEvidenceSize | null;
  capturedAtMs?: number;
};

export type ViewportRuntimeHealth = "evidence_available" | "missing_transport" | "no_canvas" | "no_framebuffer" | "no_core_event";

export type ViewportRuntimeEvidenceSummary = {
  health: ViewportRuntimeHealth | "-";
  healthText: string;
  canvasCount: string;
  canvasSize: string;
  framebuffer: string;
  transportId: string;
  latestCoreEvent: string;
  latestCoreEventAt: string;
  capturedAt: string;
  displaySource: RuntimeEvidenceDisplaySource | "-";
  inputPath: RuntimeEvidenceInputPath | "-";
  surface: string;
};

export function createViewportRuntimeEvidence(input: ViewportRuntimeEvidenceInput): ViewportRuntimeEvidence {
  const canvasSizes = (input.canvasSizes ?? []).map(normalizeSize);
  return {
    canvasCount: canvasSizes.length,
    canvasSizes,
    framebuffer: normalizeSize(input.framebuffer),
    transportId: input.transportId?.trim() || "",
    latestCoreEvent: input.latestCoreEvent?.trim() || "",
    latestCoreEventAtMs: normalizeTimestamp(input.latestCoreEventAtMs),
    capturedAtMs: normalizeTimestamp(input.capturedAtMs) || Date.now(),
    displaySource: input.displaySource,
    inputPath: input.inputPath,
    surface: normalizeSurface(input.surface),
  };
}

export function summarizeViewportRuntimeEvidence(evidence: ViewportRuntimeEvidence | null | undefined): ViewportRuntimeEvidenceSummary {
  if (!evidence) return emptyViewportRuntimeEvidenceSummary();
  return {
    health: runtimeHealth(evidence),
    healthText: {
      evidence_available: "已记录画面提交", missing_transport: "缺少传输证据", no_canvas: "缺少 Canvas 证据",
      no_framebuffer: "缺少画面尺寸", no_core_event: "缺少提交事件",
    }[runtimeHealth(evidence)],
    canvasCount: evidence.canvasCount > 0 ? String(evidence.canvasCount) : "-",
    canvasSize: evidence.canvasSizes.length ? evidence.canvasSizes.map(formatSize).join(", ") : "-",
    framebuffer: formatSize(evidence.framebuffer),
    transportId: evidence.transportId || "-",
    latestCoreEvent: evidence.latestCoreEvent || "-",
    latestCoreEventAt: formatTimestamp(evidence.latestCoreEventAtMs),
    capturedAt: formatTimestamp(evidence.capturedAtMs),
    displaySource: evidence.displaySource ?? "-",
    inputPath: evidence.inputPath ?? "-",
    surface: formatSurface(evidence.surface),
  };
}

export function emptyViewportRuntimeEvidenceSummary(): ViewportRuntimeEvidenceSummary {
  return {
    health: "-", healthText: "尚无画面提交证据", canvasCount: "-", canvasSize: "-", framebuffer: "-", transportId: "-",
    latestCoreEvent: "-", latestCoreEventAt: "-", capturedAt: "-", displaySource: "-",
    inputPath: "-", surface: "-",
  };
}

function normalizeSize(size: RuntimeEvidenceSize | null | undefined): RuntimeEvidenceSize {
  return { width: normalizeCount(size?.width), height: normalizeCount(size?.height) };
}
function normalizeTimestamp(value: number | undefined): number { return Number.isFinite(value) && value && value > 0 ? Math.round(value) : 0; }
function normalizeCount(value: number | undefined): number { return Number.isFinite(value) && value && value > 0 ? Math.round(value) : 0; }
function normalizeOptional(value: number | undefined): number | undefined { return Number.isFinite(value) && value !== undefined && value >= 0 ? Math.round(value * 100) / 100 : undefined; }

function normalizeSurface(value: CoreSurfaceRuntimeEvidence | undefined): CoreSurfaceRuntimeEvidence | undefined {
  if (!value) return undefined;
  return {
    presentedFrames: normalizeCount(value.presentedFrames),
    firstFrameAtMs: normalizeTimestamp(value.firstFrameAtMs),
    latestFrameAtMs: normalizeTimestamp(value.latestFrameAtMs),
    estimatedFps: normalizeOptional(value.estimatedFps),
    latestFrameIntervalMs: normalizeOptional(value.latestFrameIntervalMs),
    maxFrameIntervalMs: normalizeOptional(value.maxFrameIntervalMs),
    averageFrameIntervalMs: normalizeOptional(value.averageFrameIntervalMs),
    framebuffer: normalizeSize(value.framebuffer),
  };
}

function runtimeHealth(evidence: ViewportRuntimeEvidence): ViewportRuntimeHealth {
  if (!evidence.transportId) return "missing_transport";
  if (evidence.canvasCount <= 0) return "no_canvas";
  if (!isKnownSize(evidence.framebuffer)) return "no_framebuffer";
  if (!evidence.latestCoreEvent) return "no_core_event";
  return "evidence_available";
}

function isKnownSize(size: RuntimeEvidenceSize): boolean { return size.width > 0 && size.height > 0; }
function formatSize(size: RuntimeEvidenceSize): string { return isKnownSize(size) ? `${size.width}x${size.height}` : "-"; }
function formatTimestamp(value: number): string { return value > 0 ? new Date(value).toISOString() : "-"; }
function formatNumber(value: number | undefined): string { return value === undefined ? "-" : String(value); }
function formatDuration(value: number | undefined): string { return value === undefined ? "-" : `${value}ms`; }

function formatSurface(value: CoreSurfaceRuntimeEvidence | undefined): string {
  if (!value) return "-";
  return [
    `frames=${value.presentedFrames}`,
    `average_submit_fps=${formatNumber(value.estimatedFps)}`,
    `interval=${formatDuration(value.latestFrameIntervalMs)}`,
    `maxInterval=${formatDuration(value.maxFrameIntervalMs)}`,
    `avgInterval=${formatDuration(value.averageFrameIntervalMs)}`,
    `first=${formatTimestamp(value.firstFrameAtMs)}`,
    `latest=${formatTimestamp(value.latestFrameAtMs)}`,
    `framebuffer=${formatSize(value.framebuffer)}`,
  ].join(" ");
}
