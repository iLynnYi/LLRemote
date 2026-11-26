import { onBeforeUnmount, onMounted, ref, watchEffect } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { createNativeKeyboardCaptureController, type NativeKeyboardMessage, type NativeKeyboardCaptureStatus } from "../../../session/input/sessionNativeKeyboardCapture.ts";
import { sessionKeyboardPreferences } from "./useSessionKeyboardPreferences.ts";

type CaptureRuntime = {
  document: Document;
  window: Window;
  native: boolean;
  setCapture: (enabled: boolean, token: string) => Promise<boolean>;
  listenKeyboard: (callback: (message: NativeKeyboardMessage) => void) => Promise<() => void>;
  listenFocus: (callback: (focused: boolean) => void) => Promise<() => void>;
  isWindowFocused: () => Promise<boolean>;
  token: () => string;
};

type Context = { sessionId: string; target: string; element: HTMLElement };

export function useSessionKeyboardCapture(options: {
  runtime?: CaptureRuntime;
  getContext: () => Context | null;
  onKey: (sessionId: string, event: KeyboardEvent, down: boolean) => void;
  release: (sessionId: string) => void;
  onUnavailable: (reason: string) => void;
  onWindowFocusChanged?: (focused: boolean) => void;
}) {
  const runtime = options.runtime ?? {
    document, window, native: isTauri() && /Mac|Win/i.test(navigator.platform),
    setCapture: (enabled: boolean, token: string) => invoke<boolean>("set_session_keyboard_capture", { enabled, token }),
    listenKeyboard: (callback: (message: NativeKeyboardMessage) => void) => listen<NativeKeyboardMessage>("session-native-keyboard", event => callback(event.payload)),
    listenFocus: (callback: (focused: boolean) => void) => getCurrentWindow().onFocusChanged(({ payload }) => callback(payload)),
    isWindowFocused: () => invoke<boolean>("is_session_keyboard_capture_focused"),
    token: () => crypto.randomUUID(),
  };
  const { document: captureDocument, window: captureWindow } = runtime;
  const revision = ref(0);
  const focused = ref(captureDocument.hasFocus());
  let native = runtime.native;
  const status = ref<NativeKeyboardCaptureStatus>("inactive");
  let focusVersion = 0;
  let windowFocusVersion = 0;
  let changingDomFocus = false;
  let lastUnavailable = "";
  let mounted = false;
  let disposed = false;
  let reportedWindowFocus: boolean | null = null;
  let stopWatching: (() => void) | null = null;
  let previous: { sessionId: string; identity: string } | null = null;
  const unlisteners: Array<() => void> = [];
  const controller = createNativeKeyboardCaptureController({
    setCapture: runtime.setCapture,
    onStateChanged: next => { status.value = next; },
    token: runtime.token,
    onAvailable: () => { lastUnavailable = ""; },
    onUnavailable: reason => {
      if (reason !== lastUnavailable) { lastUnavailable = reason; options.onUnavailable(reason); }
    },
  });

  const localTargets = "button,input,textarea,select,[contenteditable='true'],[role='button'],[data-native-editing],.n-button,.n-input,.n-select,.session-toolbox-trigger,.session-toolbox-button,.session-toolbox-menu,.n-dropdown-menu,.n-modal,.remote-viewport-hud,.remote-viewport-mode-toggle";

  function contextForInput() {
    if (!mounted || !focused.value || changingDomFocus || captureDocument.visibilityState === "hidden") return null;
    const context = options.getContext();
    const target = captureDocument.activeElement;
    if (!context || (target instanceof HTMLElement && target.closest(localTargets))) return null;
    // A newly focused session window can have body focus. Local controls and
    // other workspaces remain local; the canvas does not need a second click.
    if (target && target !== captureDocument.body && target !== captureDocument.documentElement
      && !context.element.contains(target)) return null;
    return context;
  }

  function update() {
    revision.value;
    const context = contextForInput();
    const preferences = sessionKeyboardPreferences(context?.target ?? "");
    const identity = context ? `${context.sessionId}:${context.target}:${preferences.rightControlAsCommand}` : "";
    if (previous?.identity !== identity) {
      if (previous) options.release(previous.sessionId);
      controller.resume();
    }
    previous = context ? { sessionId: context.sessionId, identity } : null;
    if (context && (captureDocument.activeElement === captureDocument.body || captureDocument.activeElement === captureDocument.documentElement)) {
      context.element.focus({ preventScroll: true });
    }
    controller.setTarget(native && context ? {
      id: identity,
      release: () => options.release(context.sessionId),
      onKey: (key, down) => {
        if (contextForInput()?.sessionId !== context.sessionId) return;
        options.onKey(context.sessionId, new KeyboardEvent(down ? "keydown" : "keyup", key), down);
      },
    } : null);
  }

  const refresh = () => { revision.value++; };
  function reportWindowFocus(value: boolean) {
    if (reportedWindowFocus === value) return;
    reportedWindowFocus = value;
    options.onWindowFocusChanged?.(value);
  }
  async function verifyWindowFocus() {
    const version = ++focusVersion;
    const windowVersion = ++windowFocusVersion;
    try {
      const value = native ? await runtime.isWindowFocused() : captureDocument.hasFocus();
      if (disposed || windowVersion !== windowFocusVersion) return;
      // Clipboard ownership must use the same verified native window as capture,
      // even when a local control currently owns DOM keyboard focus.
      reportWindowFocus(value);
      if (version !== focusVersion) return;
      focused.value = value;
      if (value) controller.resume();
      refresh();
    } catch {
      if (disposed || windowVersion !== windowFocusVersion) return;
      reportWindowFocus(false);
      if (version !== focusVersion) return;
      focused.value = false;
      refresh();
      options.onUnavailable("keyboard_capture_focus_failed");
    }
  }
  const focus = () => { changingDomFocus = false; void verifyWindowFocus(); };
  const blur = () => {
    focusVersion++; focused.value = false; refresh();
    // WebView child-focus changes can emit false while the containing native
    // window remains foreground. Release first, then verify ownership again.
    if (native) void verifyWindowFocus();
    else { windowFocusVersion++; reportWindowFocus(false); }
  };
  const focusIn = () => {
    focusVersion++; changingDomFocus = false;
    if (contextForInput()) controller.resume();
    refresh();
    if (!focused.value) void verifyWindowFocus();
  };
  const focusOut = () => { focusVersion++; changingDomFocus = true; refresh(); };
  const visibility = () => {
    windowFocusVersion++;
    refresh();
    if (captureDocument.visibilityState !== "hidden") void verifyWindowFocus();
    else reportWindowFocus(false);
  };
  const resume = (event: PointerEvent) => {
    const context = options.getContext();
    const target = event.target;
    if (disposed || !context || !(target instanceof HTMLElement)
      || !context.element.contains(target) || target.closest(localTargets)) return;
    context.element.focus({ preventScroll: true });
    controller.resume(); refresh();
    // A canvas click must also recover when no native focus event was emitted.
    void verifyWindowFocus();
  };

  async function own(register: () => Promise<() => void>) {
    const unlisten = await register();
    if (disposed) unlisten(); else unlisteners.push(unlisten);
  }

  onMounted(async () => {
    mounted = true;
    captureWindow.addEventListener("focus", focus);
    captureWindow.addEventListener("blur", blur);
    captureDocument.addEventListener("focusin", focusIn);
    captureDocument.addEventListener("focusout", focusOut);
    captureDocument.addEventListener("visibilitychange", visibility);
    captureDocument.addEventListener("pointerdown", resume, true);
    if (native) {
      try {
        await own(() => runtime.listenKeyboard(message => controller.consume(message)));
        await own(() => runtime.listenFocus(payload => { if (payload) focus(); else blur(); }));
        await verifyWindowFocus();
      } catch {
        native = false;
        if (!disposed) options.onUnavailable("keyboard_capture_failed");
      }
    } else {
      await verifyWindowFocus();
    }
    if (!disposed) stopWatching = watchEffect(update, { flush: "sync" });
  });

  function routeKeyboardEvent(sessionId: string, event: KeyboardEvent, _down: boolean) {
    if (event.target instanceof HTMLElement && event.target.closest(localTargets)) return "local";
    return contextForInput()?.sessionId === sessionId ? "remote" : "local";
  }

  onBeforeUnmount(() => {
    disposed = true;
    mounted = false;
    reportWindowFocus(false);
    stopWatching?.();
    controller.dispose();
    if (previous) options.release(previous.sessionId);
    unlisteners.splice(0).forEach(unlisten => unlisten());
    captureWindow.removeEventListener("focus", focus);
    captureWindow.removeEventListener("blur", blur);
    captureDocument.removeEventListener("focusin", focusIn);
    captureDocument.removeEventListener("focusout", focusOut);
    captureDocument.removeEventListener("visibilitychange", visibility);
    captureDocument.removeEventListener("pointerdown", resume, true);
  });
  return { routeKeyboardEvent, status };
}
