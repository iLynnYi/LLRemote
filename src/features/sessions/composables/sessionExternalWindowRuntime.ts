import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import type { SessionExternalWindowRequest } from "../../../session/external-window/sessionExternalWindow.ts";
import type { SessionExternalWindowCreation } from "./useSessionExternalWindowCoordinator.ts";
import { nativeSessionWindowOptions, setSessionWindowExpanded } from "../../../session/external-window/sessionWindowPresentation.ts";

/** Destroy only queues a native event; retain ownership until the label is gone. */
export async function destroySessionExternalWindow(
  handle: Pick<WebviewWindow, "label" | "destroy">,
  findWindow: (label: string) => Promise<unknown | null> = (label) => WebviewWindow.getByLabel(label),
  listenDestroyed: (handler: () => void) => Promise<() => void> = (handler) =>
    listen("tauri://destroyed", handler, { target: { kind: "Window", label: handle.label } }),
  destroyTimeoutMs = 2_000,
): Promise<void> {
  const failure = () => new Error("external_window_destroy_failed");
  async function isAbsent(): Promise<boolean> {
    try { return await findWindow(handle.label) === null; } catch { throw failure(); }
  }
  let signalDestroyed!: () => void;
  const destroyed = new Promise<void>((resolve) => { signalDestroyed = resolve; });
  let timeout: ReturnType<typeof setTimeout> | undefined;
  let unlisten: () => void;
  try {
    // Subscribe before dispatch so an early Destroyed cannot be lost. This
    // listener belongs to the main WebView and targets the child window.
    unlisten = await listenDestroyed(signalDestroyed);
  } catch {
    if (await isAbsent()) return;
    throw failure();
  }
  try {
    let dispatchFailed = false;
    try {
      await handle.destroy();
    } catch {
      dispatchFailed = true;
    }
    if (await isAbsent()) return;
    if (dispatchFailed) throw failure();
    // A missed native event must not hold the owner forever. The registry
    // check remains authoritative before the label can be reused.
    await Promise.race([
      destroyed,
      new Promise<void>((resolve) => { timeout = setTimeout(resolve, destroyTimeoutMs); }),
    ]);
    if (!await isAbsent()) throw failure();
  } finally {
    if (timeout) clearTimeout(timeout);
    try { await unlisten(); } catch { throw failure(); }
  }
}

/** Native creation finishes through local events, after the constructor returns. */
export function createSessionExternalWindow(request: SessionExternalWindowRequest): SessionExternalWindowCreation {
  const handle = new WebviewWindow(request.label, {
    title: request.title,
    url: request.url,
    width: request.width,
    height: request.height,
    minWidth: request.minWidth,
    minHeight: request.minHeight,
    resizable: request.resizable,
    preventOverflow: true,
    center: true,
    ...nativeSessionWindowOptions(request.fullscreen),
  });
  let settled = false;
  let stopCreated: (() => void) | null = null;
  let stopError: (() => void) | null = null;
  const ready = new Promise<void>((resolve, reject) => {
    function finish(failed: boolean) {
      if (settled) return;
      settled = true;
      stopCreated?.();
      stopError?.();
      if (failed) reject(new Error("external_window_create_failed"));
      else resolve();
    }
    // Only native completion events settle readiness, not listener setup errors.
    void handle.once("tauri://created", () => finish(false)).then((unlisten) => {
      if (settled) unlisten();
      else stopCreated = unlisten;
    }).catch(() => undefined);
    void handle.once("tauri://error", () => finish(true)).then((unlisten) => {
      if (settled) unlisten();
      else stopError = unlisten;
    }).catch(() => undefined);
  });
  return { handle: {
    setFullscreen: (fullscreen) => setSessionWindowExpanded(handle, fullscreen),
    destroy: () => destroySessionExternalWindow(handle),
  }, ready };
}
