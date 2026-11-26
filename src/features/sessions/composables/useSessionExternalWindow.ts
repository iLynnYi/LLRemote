import { ref } from "vue";
import type { SessionExternalWindowHandle } from "./useSessionExternalWindowCoordinator.ts";
import { isSessionExternalWindowLocation } from "@session/external-window/sessionExternalWindow";

export function useSessionExternalWindow(locationHref = window.location.href) {
  const externalWindowLocation = isSessionExternalWindowLocation(locationHref);
  const externalWindowSessionIds = ref<Set<string>>(new Set());
  const externalWindowHandles = new Map<string, SessionExternalWindowHandle>();
  const externalWindowDockUnlisteners = new Map<string, (() => void)>();
  const externalWindowInputUnlisteners = new Map<string, (() => void)>();
  const externalWindowStateUnlisteners = new Map<string, (() => void)>();

  return {
    externalWindowDockUnlisteners,
    externalWindowHandles,
    externalWindowInputUnlisteners,
    externalWindowStateUnlisteners,
    externalWindowLocation,
    externalWindowSessionIds,
  };
}
