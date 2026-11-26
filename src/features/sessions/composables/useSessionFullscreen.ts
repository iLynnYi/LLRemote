import { ref, type Ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { shortcutFromKeyboardEvent } from "@session/input/keyboardShortcut";
import type { SessionSettings } from "@session/settings/sessionSettings";

export type SessionFullscreenOptions = {
  activateSessionTab: (sessionId: string) => Promise<void>;
  openExternalWindowSession: (sessionId: string, fullscreen: boolean) => Promise<void>;
  sessionSettings: Ref<SessionSettings>;
};

export function useSessionFullscreen({
  activateSessionTab,
  openExternalWindowSession,
  sessionSettings,
}: SessionFullscreenOptions) {
  const fullscreenSessionId = ref<string | null>(null);
  const fullscreenExitShortcutListening = ref(false);

  function isEditableKeyTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) {
      return false;
    }

    return (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement ||
      target.isContentEditable
    );
  }

  function fullscreenExitShortcutMatches(event: KeyboardEvent): boolean {
    return shortcutFromKeyboardEvent(event) === sessionSettings.value.fullscreenExitShortcut;
  }

  async function setNativeWindowFullscreen(fullscreen: boolean) {
    try {
      await getCurrentWindow().setFullscreen(fullscreen);
    } catch {
      // Browser preview cannot access the Tauri window API.
    }
  }

  function startFullscreenExitShortcutListener() {
    if (fullscreenExitShortcutListening.value) {
      return;
    }

    document.addEventListener("keydown", handleGlobalSessionKeydown, true);
    fullscreenExitShortcutListening.value = true;
  }

  function stopFullscreenExitShortcutListener() {
    if (!fullscreenExitShortcutListening.value) {
      return;
    }

    document.removeEventListener("keydown", handleGlobalSessionKeydown, true);
    fullscreenExitShortcutListening.value = false;
  }

  async function exitSessionFullscreen() {
    if (!fullscreenSessionId.value) {
      return;
    }

    fullscreenSessionId.value = null;
    stopFullscreenExitShortcutListener();
    await setNativeWindowFullscreen(false);
    if (document.fullscreenElement) {
      await document.exitFullscreen().catch(() => undefined);
    }
  }

  async function enterSessionFullscreen(sessionId: string) {
    await openExternalWindowSession(sessionId, true);
  }

  async function toggleSessionFullscreen(sessionId: string) {
    await activateSessionTab(sessionId);

    if (fullscreenSessionId.value === sessionId) {
      await exitSessionFullscreen();
      return;
    }

    await enterSessionFullscreen(sessionId);
  }

  function syncFullscreenState() {
    if (!document.fullscreenElement) {
      fullscreenSessionId.value = null;
      stopFullscreenExitShortcutListener();
      void setNativeWindowFullscreen(false);
    }
  }

  function handleGlobalSessionKeydown(event: KeyboardEvent) {
    if (!fullscreenSessionId.value || isEditableKeyTarget(event.target) || !fullscreenExitShortcutMatches(event)) {
      return;
    }

    event.preventDefault();
    void exitSessionFullscreen();
  }

  return {
    enterSessionFullscreen,
    exitSessionFullscreen,
    fullscreenExitShortcutListening,
    fullscreenExitShortcutMatches,
    fullscreenSessionId,
    handleGlobalSessionKeydown,
    isEditableKeyTarget,
    setNativeWindowFullscreen,
    startFullscreenExitShortcutListener,
    stopFullscreenExitShortcutListener,
    syncFullscreenState,
    toggleSessionFullscreen,
  };
}
