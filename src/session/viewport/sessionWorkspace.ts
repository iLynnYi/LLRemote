import type { RemoteSessionTab } from "../core/sessionManager.ts";

export type SessionWorkspaceTab = RemoteSessionTab;

export type SessionWorkspaceRenderState = "visible" | "parked" | "external";

export type SessionWorkspaceRenderSlot = SessionWorkspaceTab & {
  renderState: SessionWorkspaceRenderState;
  visible: boolean;
  externalWindowCandidate: boolean;
  externalWindowOwned: boolean;
};

export type SelectWorkspaceSessionsOptions = {
  activeSessionId?: string;
  fullscreenSessionId?: string | null;
};

export type SummarizeWorkspaceRenderSlotsOptions = SelectWorkspaceSessionsOptions & {
  externalSessionIds?: string[];
};

export function selectWorkspaceSessions(
  tabs: SessionWorkspaceTab[],
  options: SelectWorkspaceSessionsOptions,
): SessionWorkspaceTab[] {
  const fullscreen = tabs.find((tab) => tab.id === options.fullscreenSessionId);
  if (fullscreen) {
    return [fullscreen];
  }

  const active = tabs.find((tab) => tab.id === options.activeSessionId);
  return active ? [active] : tabs.slice(0, 1);
}

export function summarizeWorkspaceRenderSlots(
  tabs: SessionWorkspaceTab[],
  options: SummarizeWorkspaceRenderSlotsOptions,
): SessionWorkspaceRenderSlot[] {
  const externalIds = new Set(options.externalSessionIds ?? []);
  const mainTabs = tabs.filter((tab) => !externalIds.has(tab.id));
  const selected = tabs.find((tab) => tab.id === options.fullscreenSessionId)
    ?? tabs.find((tab) => tab.id === options.activeSessionId);
  // Keep the selected external session's place instead of showing another session's canvas or error.
  const visibleTabs = selected && externalIds.has(selected.id) ? [] : selectWorkspaceSessions(mainTabs, options);
  const visibleIds = new Set(visibleTabs.map((tab) => tab.id));

  return tabs.map((tab) => {
    const externalWindowOwned = externalIds.has(tab.id);
    const visible = visibleIds.has(tab.id);

    return {
      ...tab,
      renderState: externalWindowOwned ? "external" : visible ? "visible" : "parked",
      visible,
      externalWindowCandidate: tab.liveCanvas && visible && !externalWindowOwned,
      externalWindowOwned,
    };
  });
}
