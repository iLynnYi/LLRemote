import { computed, ref, shallowRef } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { createRemoteSessionCoreClient, type RemoteSessionCoreClient, type RemoteSessionSnapshot } from "../../../session/protocol/remoteSessionCore.ts";
import {
  createCoreSurfaceSession,
  type CoreSurfaceCommit,
  type CoreSurfaceSession,
} from "../../../session/renderer/coreSurfaceSession.ts";

export type SessionExternalSurfacePhase = "loading" | "ready" | "suspended" | "failed" | "disposed";

type SurfaceAttempt = { failed: boolean; surface: CoreSurfaceSession | null };
type SurfaceVisibilityRuntime = {
  document: EventTarget & Pick<Document, "visibilityState">;
  window: EventTarget;
};
const surfaceErrorCodes = new Set([
  "surface_canvas_context_unavailable", "surface_canvas_context_lost",
  "surface_websocket_failed", "surface_websocket_closed", "surface_packet_decode_failed",
  "surface_packet_truncated", "surface_packet_invalid_header", "surface_packet_rect_out_of_bounds",
  "surface_packet_trailing_bytes",
]);

/** Owns the external window's local Surface without owning its RFB connection. */
export function useSessionExternalSurface(options: {
  sessionId: string;
  getMountTarget: () => HTMLElement | null;
  onError: (code: string) => void;
  onDeactivate?: () => void;
  onFramePresented?: () => void;
  getSnapshot?: RemoteSessionCoreClient["getSnapshot"];
  createSurfaceSession?: typeof createCoreSurfaceSession;
  runtime?: SurfaceVisibilityRuntime;
}) {
  const phase = ref<SessionExternalSurfacePhase>("loading");
  const canvas = shallowRef<HTMLCanvasElement | null>(null);
  const commit = shallowRef<CoreSurfaceCommit | null>(null);
  const active = computed(() => phase.value === "ready");
  const getSnapshot = options.getSnapshot ?? createRemoteSessionCoreClient({ invoke }).getSnapshot;
  const createSurfaceSession = options.createSurfaceSession ?? createCoreSurfaceSession;
  let currentAttempt: SurfaceAttempt | null = null;
  let disposed = false;
  let visibilityRuntime: SurfaceVisibilityRuntime | null = null;
  let stopVisibility: (() => void) | null = null;
  let needsForegroundRefresh = false;

  function isCurrent(attempt: SurfaceAttempt) {
    return !disposed && currentAttempt === attempt;
  }

  function isHidden() {
    return visibilityRuntime?.document.visibilityState === "hidden";
  }

  function clearRefs() {
    const hadSurface = canvas.value !== null || commit.value !== null;
    canvas.value = null;
    commit.value = null;
    if (hadSurface) options.onDeactivate?.();
  }

  function disconnectSurface(attempt: SurfaceAttempt | null) {
    const surface = attempt?.surface;
    if (attempt) attempt.surface = null;
    surface?.disconnect();
  }

  function fail(attempt: SurfaceAttempt, code: string) {
    if (!isCurrent(attempt) || attempt.failed) return;
    if (isHidden()) {
      suspend();
      return;
    }
    attempt.failed = true;
    clearRefs();
    phase.value = "failed";
    disconnectSurface(attempt);
    options.onError(code);
  }

  async function load() {
    if (disposed) return;
    if (isHidden()) {
      suspend();
      return;
    }
    needsForegroundRefresh = false;
    const previous = currentAttempt;
    const attempt: SurfaceAttempt = { failed: false, surface: null };
    currentAttempt = attempt;
    clearRefs();
    phase.value = "loading";
    disconnectSurface(previous);
    let snapshot: RemoteSessionSnapshot | null;
    try {
      snapshot = await getSnapshot(options.sessionId);
    } catch {
      fail(attempt, "surface_snapshot_failed");
      return;
    }
    if (!isCurrent(attempt) || attempt.failed) return;
    if (isHidden()) {
      suspend();
      return;
    }
    const descriptor = snapshot?.surfaces.find((item) => item.owner === "external" && item.surfaceUrl);
    if (!descriptor) {
      fail(attempt, "surface_external_unavailable");
      return;
    }
    const target = options.getMountTarget();
    if (!target) {
      fail(attempt, "surface_mount_unavailable");
      return;
    }
    try {
      const surface = createSurfaceSession({
        target,
        surfaceUrl: descriptor.surfaceUrl,
        onFrameCommitted: (frame) => {
          if (!isCurrent(attempt) || attempt.failed) return;
          commit.value = frame;
          phase.value = "ready";
          options.onFramePresented?.();
        },
        onError: (code) => fail(attempt, surfaceErrorCodes.has(code) ? code : "surface_packet_decode_failed"),
      });
      if (!isCurrent(attempt) || attempt.failed) {
        surface.disconnect();
        return;
      }
      attempt.surface = surface;
      canvas.value = surface.canvas;
    } catch (error) {
      fail(attempt, error instanceof Error && surfaceErrorCodes.has(error.message)
        ? error.message : "surface_startup_failed");
    }
  }

  function suspend() {
    if (disposed) return;
    const previous = currentAttempt;
    currentAttempt = null;
    clearRefs();
    phase.value = "suspended";
    disconnectSurface(previous);
  }

  /** Hidden WebViews cannot meet the Canvas ACK deadline. Reopen locally on
   * foreground entry; Rust resets delivery to a full baseline for the new socket. */
  async function start() {
    if (disposed || visibilityRuntime) return;
    const runtime = options.runtime ?? { document, window };
    visibilityRuntime = runtime;
    const onFocus = () => {
      if (disposed || runtime.document.visibilityState === "hidden") return;
      if (needsForegroundRefresh || phase.value === "suspended" || phase.value === "failed") {
        void load();
      }
    };
    const onBlur = () => { needsForegroundRefresh = true; };
    const onVisibility = () => {
      if (runtime.document.visibilityState === "hidden") suspend();
      else onFocus();
    };
    runtime.document.addEventListener("visibilitychange", onVisibility);
    runtime.window.addEventListener("focus", onFocus);
    runtime.window.addEventListener("blur", onBlur);
    stopVisibility = () => {
      runtime.document.removeEventListener("visibilitychange", onVisibility);
      runtime.window.removeEventListener("focus", onFocus);
      runtime.window.removeEventListener("blur", onBlur);
    };
    await load();
  }

  function dispose() {
    if (disposed) return;
    disposed = true;
    stopVisibility?.();
    stopVisibility = null;
    const previous = currentAttempt;
    currentAttempt = null;
    clearRefs();
    phase.value = "disposed";
    disconnectSurface(previous);
  }

  return { phase, canvas, commit, active, start, load, dispose,
    canForwardInput: (inputEnabled: boolean) => phase.value === "ready" && inputEnabled };
}
