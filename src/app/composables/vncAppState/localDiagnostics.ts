import type { Ref } from "vue";
import type { WorkspaceMode } from "@app/types";
import type { RemoteSession } from "@session/core/sessionManager";
import { matchesLocalVncServerTarget } from "../../../session/core/localVncServerSession.ts";
import type { LocalVncServerOverview } from "@server/localShare";

export function createLocalVncServerDiagnosticsRefresh(options: {
  activeRemoteSession: Ref<RemoteSession | null>;
  activeWorkspace: Ref<WorkspaceMode>;
  localVncServerOverview: Ref<LocalVncServerOverview | null>;
  refreshRuntimeStatus: () => Promise<void>;
  diagnosticsVisible: Ref<boolean>;
}) {
  let timer: ReturnType<typeof setInterval> | null = null;
  let pending = false;

  function shouldRefresh() {
    if (!options.diagnosticsVisible.value || options.activeWorkspace.value !== "sessions") {
      return false;
    }

    const activeSession = options.activeRemoteSession.value;
    const overview = options.localVncServerOverview.value;
    return Boolean(
      activeSession
      && activeSession.state === "connected"
      && overview
      && matchesLocalVncServerTarget(overview, activeSession.host, activeSession.port),
    );
  }

  function stop() {
    if (timer === null) {
      return;
    }

    clearInterval(timer);
    timer = null;
  }

  function sync() {
    if (!shouldRefresh()) {
      stop();
      return;
    }

    if (timer !== null) {
      return;
    }

    timer = setInterval(() => {
      if (!shouldRefresh()) {
        stop();
        return;
      }

      void refresh();
    }, 1_000);
    void refresh();
  }

  async function refresh() {
    if (pending || !shouldRefresh()) return;
    pending = true;
    try {
      await options.refreshRuntimeStatus();
    } catch {
      // The runtime service owns its structured error state; sampling must not reject globally.
    } finally {
      pending = false;
    }
  }

  return {
    stop,
    sync,
  };
}
