import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { ClipboardAccess } from "./clipboardTypes.ts";

export type SystemClipboardOptions = {
  browserClipboard?: ClipboardAccess;
  isTauriRuntime?: () => boolean;
  tauriClipboard?: ClipboardAccess;
};

const tauriClipboard: ClipboardAccess = {
  readText,
  writeText,
};

function defaultBrowserClipboard(): ClipboardAccess | undefined {
  if (typeof navigator === "undefined") {
    return undefined;
  }

  return navigator.clipboard;
}

function defaultIsTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function createSystemClipboard(options: SystemClipboardOptions = {}): ClipboardAccess | undefined {
  if ((options.isTauriRuntime ?? defaultIsTauriRuntime)()) {
    return options.tauriClipboard ?? tauriClipboard;
  }

  return options.browserClipboard ?? defaultBrowserClipboard();
}
