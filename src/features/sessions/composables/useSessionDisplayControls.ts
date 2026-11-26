import type { Translate } from "@app/types";
import type { EventLogLevel } from "@diagnostics/eventLog";
import type { SessionConnectionRegistry } from "@session/core/sessionConnectionRegistry";
import type { SessionManager } from "@session/core/sessionManager";
import type { SessionQualityProfile } from "@session/viewport/sessionDisplay";
import type { ClipboardSyncMode } from "@session/settings/sessionSettings";

type SessionDisplayControlsOptions = {
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  refreshSessionTabs: () => void;
  sessionConnectionRegistry: SessionConnectionRegistry;
  sessionManager: SessionManager;
  getClipboardSyncMode: () => ClipboardSyncMode;
  setSessionClipboardPolicy: (sessionId: string, mode: ClipboardSyncMode, readonly: boolean) => void;
  setSessionReadonlyState: (sessionId: string, readonly: boolean) => void;
  isSessionViewOnlyShared: (sessionId: string) => boolean;
  onReadonlyChanged: (sessionId: string) => void;
  syncSessionInputOwnership: () => void;
  t: Translate;
};

export function useSessionDisplayControls(options: SessionDisplayControlsOptions) {
  const {
    logEvent,
    refreshSessionTabs,
    sessionConnectionRegistry,
    sessionManager,
    getClipboardSyncMode,
    setSessionClipboardPolicy,
    setSessionReadonlyState,
    onReadonlyChanged,
    syncSessionInputOwnership,
    t,
  } = options;

  function setSessionQualityProfile(sessionId: string, profile: SessionQualityProfile) {
    const updated = sessionManager.updateQualityProfile(sessionId, profile);
    if (!updated) {
      return;
    }

    sessionConnectionRegistry.get(sessionId)?.connection.setQualityProfile(profile);
    refreshSessionTabs();
    logEvent("info", t("log.sourceSession"), t("sessions.qualityChanged", {
      quality: t(`sessions.qualityProfiles.${profile}`),
      target: updated.target,
    }));
  }

  function setSessionReadonlyMode(sessionId: string, readonly: boolean) {
    if (options.isSessionViewOnlyShared(sessionId)) return;
    setSessionReadonlyState(sessionId, readonly);
    setSessionClipboardPolicy(sessionId, getClipboardSyncMode(), readonly);
    sessionConnectionRegistry.get(sessionId)?.connection.setReadonlyMode(readonly);
    syncSessionInputOwnership();
    refreshSessionTabs();
    onReadonlyChanged(sessionId);
  }

  return {
    setSessionQualityProfile,
    setSessionReadonlyMode,
  };
}
