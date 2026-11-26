export type NativeKeyboardCaptureStatus = "inactive" | "requested" | "ready" | "unavailable";

export type NativeKeyboardMessage = {
  token: string;
  key: (KeyboardEventInit & { down: boolean }) | null;
  reason: "focus" | "overflow" | "disabled" | "unavailable" | null;
};

type Target = { id: string; onKey: (key: KeyboardEventInit, down: boolean) => void; release: () => void };
type Runtime = {
  setCapture: (enabled: boolean, token: string) => Promise<boolean>;
  token: () => string;
  onUnavailable: (reason: string) => void;
  onStopped?: () => void;
  onAvailable?: () => void;
  onStateChanged?: (status: NativeKeyboardCaptureStatus) => void;
};

/** Serializes native ownership changes and rejects events from an earlier view. */
export function createNativeKeyboardCaptureController(runtime: Runtime) {
  let current: { target: Target; token: string } | null = null;
  let pending = Promise.resolve();
  let disposed = false;
  let paused = false;
  let status: NativeKeyboardCaptureStatus = "inactive";
  function changeStatus(next: NativeKeyboardCaptureStatus) {
    if (status === next) return;
    status = next;
    runtime.onStateChanged?.(next);
  }

  function enqueue(enabled: boolean, token: string) {
    pending = pending.then(async () => {
      if (enabled && (disposed || current?.token !== token)) return;
      try {
        const available = await runtime.setCapture(enabled, token);
        if (enabled && current?.token === token) {
          if (available) { changeStatus("ready"); runtime.onAvailable?.(); }
          else unavailable("keyboard_capture_unsupported");
        }
      } catch (error) {
        if (enabled && current?.token === token) unavailable(String(error));
      }
    });
  }

  function clear() {
    const previous = current;
    current = null;
    changeStatus("inactive");
    if (!previous) return;
    previous.target.release();
    enqueue(false, previous.token);
  }

  function unavailable(reason: string) { paused = true; clear(); changeStatus("unavailable"); runtime.onUnavailable(reason); }

  return {
    setTarget(target: Target | null) {
      if (disposed || (current && current.target.id === target?.id)) return;
      if (!target) { clear(); return; }
      if (paused) return;
      clear();
      current = { target, token: runtime.token() };
      changeStatus("requested");
      enqueue(true, current.token);
    },
    consume(message: NativeKeyboardMessage) {
      if (disposed || message.token !== current?.token) return;
      if (message.key) current.target.onKey(message.key, message.key.down);
      else {
        if (message.reason === "unavailable" || message.reason === "overflow") {
          unavailable(message.reason === "overflow" ? "keyboard_capture_overflow" : "keyboard_capture_failed"); return;
        }
        paused = true;
        runtime.onStopped?.();
        clear();
      }
    },
    resume() { paused = false; },
    dispose() { disposed = true; clear(); },
    settled: () => pending,
  };
}
