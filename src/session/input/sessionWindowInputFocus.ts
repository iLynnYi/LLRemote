type FocusedWindow = { kind: "main" } | { kind: "external"; sessionId: string } | null;

/** Keeps the input target independent of the main window's selected tab. */
export function createSessionWindowInputFocus(options: {
  getMainSessionId: () => string | null;
  onChange: (sessionId: string | null) => void;
}) {
  let focusedWindow: FocusedWindow = { kind: "main" };

  function sessionId(): string | null {
    return focusedWindow?.kind === "external"
      ? focusedWindow.sessionId
      : focusedWindow?.kind === "main" ? options.getMainSessionId() : null;
  }

  function change(next: FocusedWindow) {
    if (focusedWindow === null && next === null) return;
    if (focusedWindow?.kind === "main" && next?.kind === "main") return;
    if (focusedWindow?.kind === "external" && next?.kind === "external"
      && focusedWindow.sessionId === next.sessionId) return;
    focusedWindow = next;
    options.onChange(sessionId());
  }

  return {
    sessionId,
    isMainFocused: () => focusedWindow?.kind === "main",
    mainFocusChanged(focused: boolean) {
      if (focused) change({ kind: "main" });
      else if (focusedWindow?.kind === "main") change(null);
    },
    externalFocusChanged(externalSessionId: string, focused: boolean) {
      if (focused) change({ kind: "external", sessionId: externalSessionId });
      else if (focusedWindow?.kind === "external" && focusedWindow.sessionId === externalSessionId) change(null);
    },
    mainSessionChanged() {
      if (focusedWindow?.kind === "main") options.onChange(sessionId());
    },
  };
}
