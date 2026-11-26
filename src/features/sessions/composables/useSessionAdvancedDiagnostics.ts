import { computed, onBeforeUnmount, onMounted, ref, watch, watchEffect, type Ref } from "vue";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import type { Translate, WorkspaceMode } from "../../../app/types.ts";
import type { EventLogLevel } from "../../../diagnostics/eventLog.ts";
import type { RemoteSession } from "../../../session/core/sessionManager.ts";
import type { SessionConnectionRegistry } from "../../../session/core/sessionConnectionRegistry.ts";
import { sessionSupportsAction } from "../../../session/transfers/sessionCapabilities.ts";
import { summarizeAgentPermissionDiagnostics, type AgentPermissionDiagnosticsReport } from "../../../session/diagnostics/agentPermissionDiagnostics.ts";

type AdvancedDiagnosticsContext = {
  activeRemoteSession: Readonly<Ref<RemoteSession | null>>;
  activeWorkspace: Readonly<Ref<WorkspaceMode>>;
  fullscreenSessionId: Readonly<Ref<string | null>>;
  sessionDeveloperDiagnosticsVisible: Ref<boolean>;
  sessionListTick: Readonly<Ref<number>>;
  sessionConnectionRegistry: Pick<SessionConnectionRegistry, "get">;
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  t: Translate;
};

/** Keeps optional Agent permission checks in the active session's advanced diagnostics. */
export function useSessionAdvancedDiagnostics(context: AdvancedDiagnosticsContext = useVncAppContext()) {
  const { activeRemoteSession, activeWorkspace, fullscreenSessionId, sessionDeveloperDiagnosticsVisible, sessionListTick, sessionConnectionRegistry, logEvent, t } = context;
  const developerDiagnosticsExpanded = ref(false);
  const documentVisible = ref(document.visibilityState !== "hidden");
  const updateDocumentVisibility = () => { documentVisible.value = document.visibilityState !== "hidden"; };
  watchEffect(() => {
    sessionDeveloperDiagnosticsVisible.value = developerDiagnosticsExpanded.value && documentVisible.value
      && activeWorkspace.value === "sessions" && !fullscreenSessionId.value;
  });
  onMounted(() => { document.addEventListener("visibilitychange", updateDocumentVisibility); updateDocumentVisibility(); });
  const checkingPermissions = ref(false);
  const permissionReport = ref<AgentPermissionDiagnosticsReport | null>(null);
  const permissionError = ref("");
  let attempt = 0;
  let alive = true;
  const currentConnection = computed(() => {
    sessionListTick.value;
    const session = activeRemoteSession.value;
    return session?.state === "connected" ? sessionConnectionRegistry.get(session.id)?.connection : null;
  });
  const canCheckPermissions = computed(() => {
    sessionListTick.value;
    return sessionSupportsAction(currentConnection.value?.connectionModel.capabilities, "checkAgentPermissions");
  });

  watch([() => activeRemoteSession.value?.id, currentConnection, canCheckPermissions], () => {
    attempt += 1;
    permissionReport.value = null;
    permissionError.value = "";
    checkingPermissions.value = false;
  }, { flush: "sync" });
  onBeforeUnmount(() => {
    alive = false;
    attempt += 1;
    sessionDeveloperDiagnosticsVisible.value = false;
    document.removeEventListener("visibilitychange", updateDocumentVisibility);
  });

  async function checkPermissions() {
    const session = activeRemoteSession.value;
    if (!session || !canCheckPermissions.value || checkingPermissions.value) return;
    const connection = currentConnection.value;
    if (!connection) return;
    const currentAttempt = ++attempt;
    const isCurrent = () => alive && attempt === currentAttempt && currentConnection.value === connection && activeRemoteSession.value?.id === session.id;
    checkingPermissions.value = true;
    permissionReport.value = null;
    permissionError.value = "";
    try {
      const result = await connection.diagnoseRemotePermissions();
      if (!isCurrent()) return;
      const rows = summarizeAgentPermissionDiagnostics(result.items);
      permissionReport.value = { target: session.target, checkedAt: new Date().toLocaleTimeString("zh-CN"), rows };
      const missing = rows.filter((item) => item.status === "missing").length;
      const summary = t("sessions.toolbox.agentPermissions.summary", {
        target: session.target,
        granted: rows.filter((item) => item.status === "granted").length,
        missing,
        unsupported: rows.filter((item) => item.status === "unsupported").length,
        unknown: rows.filter((item) => item.status === "unknown").length,
      });
      const details = rows.map((item) => `${item.kind}:${item.status}`).join(", ");
      logEvent(rows.some((item) => item.status === "missing" || item.status === "unknown") ? "warning" : "success", t("log.sourceSession"), `${summary}; ${details}`);
    } catch {
      if (isCurrent()) permissionError.value = t("sessions.toolbox.agentPermissions.failed");
    } finally {
      if (isCurrent()) checkingPermissions.value = false;
    }
  }

  return { canCheckPermissions, checkingPermissions, checkPermissions, permissionReport, permissionError, developerDiagnosticsExpanded };
}
