import { ref } from "vue";

export function createSessionViewportHudState(initialExpandedSessionId?: string) {
  const hiddenHudSessionIds = ref<Set<string>>(new Set());
  const expandedHudSessionIds = ref<Set<string>>(
    new Set(initialExpandedSessionId ? [initialExpandedSessionId] : []),
  );

  function isViewportHudHidden(sessionId: string): boolean {
    return hiddenHudSessionIds.value.has(sessionId);
  }

  function isViewportHudExpanded(sessionId: string): boolean {
    return expandedHudSessionIds.value.has(sessionId);
  }

  function toggleViewportHud(sessionId: string) {
    hiddenHudSessionIds.value = toggleSetValue(hiddenHudSessionIds.value, sessionId);
  }

  function toggleViewportHudExpanded(sessionId: string) {
    expandedHudSessionIds.value = toggleSetValue(expandedHudSessionIds.value, sessionId);
  }

  return {
    isViewportHudExpanded,
    isViewportHudHidden,
    toggleViewportHud,
    toggleViewportHudExpanded,
  };
}

function toggleSetValue(values: Set<string>, value: string): Set<string> {
  const next = new Set(values);
  if (next.has(value)) {
    next.delete(value);
  } else {
    next.add(value);
  }
  return next;
}
