import { computed, onBeforeUnmount, ref, watch } from "vue";
import {
  canCopyLocalVncServerConnectionProfile,
  createLocalVncServerConnectionExport,
  getLocalVncServerCopyProfileTitleKey,
  getLocalVncServerStartErrorCopy,
  getLocalVncServerToggleLabelKey,
  isLocalVncServerPasswordInputInvalid,
  isLocalVncServerRunning,
  localVncServerPermissionKindKey,
  localVncServerPermissionStatusKey,
  localVncServerPasswordValidationKey,
  summarizeLocalAgentStatus,
  summarizeLocalVncServerStatus,
  validateLocalVncServerStartInput,
  type LocalVncServerOverview,
  type LocalVncServerViewRequest,
  type LocalVncServerActiveConnection,
  type LocalVncServerPermission,
  type LocalVncServerPermissionKind,
  type LocalVncServerStartValidationError,
  type LocalVncServerStartValidationField,
} from "@server/localShare";
import {
  loadLocalShareSettings,
  saveLocalShareSettings,
  serviceSuggestionHandled, SERVICE_SUGGESTION_STORAGE_KEY,
} from "@server/localShareSettings";
import type { EventLogLevel } from "@diagnostics/eventLog";
import type { AppMessageTone, ConfirmChoice, ConfirmDialogOptions, LocalVncServerDiagnosticView, Translate } from "@app/types";
import type { SessionSettingsStorage } from "@session/settings/sessionSettingsStore";
import {
  createBackgroundShareApi, backgroundShareErrorKey,
  type BackgroundShareOverview, type SharingMode,
} from "@server/backgroundShare";
import { createWindowsServiceApi, windowsServiceErrorKey, type WindowsServiceAction, type ServiceStartOutcome } from "@server/windowsService";
import { useWindowsService } from "./useWindowsService.ts";
import { useLocalShareStartFlow } from "./useLocalShareStartFlow.ts";

type UseLocalShareWorkbenchOptions = {
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  t: Translate;
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  isTauriUnavailableError: (error: unknown) => boolean;
  notify: (message: string, tone?: AppMessageTone) => void;
  requestConfirm: (options: ConfirmDialogOptions) => Promise<ConfirmChoice>;
  openSharingRepairSettings: () => void;
  storage?: SessionSettingsStorage | null;
};

export function useLocalShareWorkbench(options: UseLocalShareWorkbenchOptions) {
  const { invoke, t, logEvent, isTauriUnavailableError, notify, storage } = options;
  const initialSettings = loadLocalShareSettings(storage);
  const backgroundApi = createBackgroundShareApi(invoke);
  const serviceApi = createWindowsServiceApi(invoke);
  const sharingMode = ref<SharingMode | null>(null);
  const localShareServiceSetup = ref(false);
  const localShareSetupPasswordRequired = ref(false);
  const installationSuggestionHandled = ref(serviceSuggestionHandled(storage));
  const localShareFallbackActive = computed(() => sharingServiceOverview.value?.installed === true
    && backgroundShareOverview.value?.serviceFallback === true
    && sharingMode.value === "background" && localVncServerOverview.value?.status === "running");
  const sharingRuntimeError = ref("");
  let lastRuntimeErrorDiagnostic = "";
  const backgroundShareOverview = ref<BackgroundShareOverview | null>(null);
  let lastBackgroundPermissions: LocalVncServerPermission[] = [];
  let backgroundFormInitialized = false;
  let backgroundUpdateChecked = false;
  const localVncServerOverview = ref<LocalVncServerOverview | null>(null);
  const windowsService = useWindowsService({
    t, invoke,
    normalServer: computed(() => backgroundShareOverview.value?.server ?? null),
  });
  const sharingServiceOverview = windowsService.overview;
  const localVncServerLoading = ref(false);
  const localShareLifecycleAction = ref<"idle" | "starting" | "stopping">("idle");
  const localVncServerPort = ref<number | null>(initialSettings.port);
  const localVncServerBindAddress = ref(initialSettings.bindAddress);
  const localVncServerPassword = ref("");
  const localVncServerViewOnly = ref(initialSettings.viewOnly);
  const localVncServerMessage = ref("");
  const localVncServerMessageTone = ref<"success" | "warning" | "error">("warning");
  const localVncServerPasswordPrompted = ref(false);
  const localVncServerValidationErrors = ref<LocalVncServerStartValidationError[]>([]);
  const localVncServerConnectionDurationTick = ref(Math.floor(Date.now() / 1000));
  let localVncServerConnectionDurationTimer: number | undefined;
  let backgroundStatusTimer: number | undefined;
  let refreshingStatus: Promise<void> | null = null;
  let inspectingPermissions: Promise<void> | null = null;
  const localSharePermissionChecking = ref(false);
  const localSharePermissionCheckError = ref("");
  const localShareStartChecking = ref(false);
  let startCheckRevision = 0;
  const backgroundRestartSupported = computed(() => sharingMode.value === "background"
    && backgroundShareOverview.value?.restartSupported === true);
  const backgroundRuntimeOutdated = computed(() => backgroundRestartSupported.value
    && backgroundShareOverview.value?.processRunning === true
    && backgroundShareOverview.value.runtime?.buildId !== backgroundShareOverview.value.currentBuildId);
  const backgroundPermissionRepairNeeded = computed(() => backgroundRestartSupported.value
    && !sharingRuntimeError.value && !localSharePermissionCheckError.value
    && backgroundShareOverview.value?.processRunning === true
    && backgroundShareOverview.value.server.permissions.some(permission => permission.status === "missing"
      && backgroundShareOverview.value?.signatureChangedPermissions?.includes(permission.kind)));

  const localShareStartFlow = useLocalShareStartFlow({
    api: { ...backgroundApi, restart: async () => { await refreshingStatus; return backgroundApi.restart(); } },
    readStatus: async () => {
      await refreshingStatus;
      await readLocalVncServerStatus(true, true);
      if (sharingMode.value !== "background" || !backgroundShareOverview.value || sharingRuntimeError.value) {
        throw new Error("background_share_runtime_failed");
      }
      return backgroundShareOverview.value;
    },
    acceptOverview: acceptBackgroundOverview,
    setBusy: busy => { localVncServerLoading.value = busy; },
    onStarted: acceptStartedOverview,
    onError: error => {
      if (error instanceof Error && error.message === "background_share_temporary_password_timeout") {
        localVncServerPassword.value = "";
      }
      showStartError(error);
      if (localShareStartFlow.purpose.value === "repair") void refreshDetachedStatus(true);
    },
  });
  const localShareStartPhase = localShareStartFlow.phase;
  const awaitingTemporaryPassword = computed(() => localShareStartPhase.value === "awaiting_password");
  const temporaryPasswordError = computed(() => localVncServerPasswordValidationKey(localVncServerPassword.value));
  const localShareMissingPermissions = localShareStartFlow.missingPermissions;
  const localShareRequiredPermissions = localShareStartFlow.requiredPermissions;
  const localShareSignatureChangedPermissions = localShareStartFlow.signatureChangedPermissions;
  const localShareGuidePurpose = localShareStartFlow.purpose;
  const requestLocalShareGuidePermission = localShareStartFlow.requestPermission;
  const openLocalShareGuideSettings = () => localShareStartFlow.requestPermission(true);
  const localShareStartPending = computed(() => localShareStartPhase.value !== "idle");
  const sharingAutoStartKnown = computed(() => sharingServiceOverview.value?.installed === true
    ? typeof sharingServiceOverview.value?.autoStart === "boolean"
    : sharingMode.value === "background" && typeof backgroundShareOverview.value?.autoStart === "boolean");
  const sharingAutoStart = computed(() => sharingServiceOverview.value?.installed === true
    ? sharingServiceOverview.value?.autoStart === true
    : sharingMode.value === "background" && backgroundShareOverview.value?.autoStart === true
      && backgroundShareOverview.value?.startupRegistration !== "missing"
      && backgroundShareOverview.value?.startupRegistration !== "disabled");
  const sharingAutoStartBusy = computed(() => localVncServerLoading.value || windowsService.busy.value
    || localShareStartPending.value || localShareStartChecking.value);
  const resumeLocalShareStart = (fromSettings = true) => localShareStartFlow.resume(fromSettings);
  const resumeAfterSettings = () => { void resumeLocalShareStart(); };
  const resumeWhenVisible = () => { if (document.visibilityState === "visible") resumeAfterSettings(); };

  if (typeof window !== "undefined") {
    localVncServerConnectionDurationTimer = window.setInterval(() => {
      localVncServerConnectionDurationTick.value = Math.floor(Date.now() / 1000);
    }, 1000);
    window.addEventListener("focus", resumeAfterSettings);
    document.addEventListener("visibilitychange", resumeWhenVisible);
    // Detached runtimes cannot emit Tauri events into this App. Poll their
    // actual state, including device changes, without locking the form.
    backgroundStatusTimer = window.setInterval(() => {
      if (!localVncServerLoading.value && !windowsService.busy.value) {
        void refreshDetachedStatus(true);
      }
    }, 1500);
  }

  onBeforeUnmount(() => {
    startCheckRevision++;
    localShareStartFlow.dispose();
    if (typeof window !== "undefined") {
      window.removeEventListener("focus", resumeAfterSettings);
      document.removeEventListener("visibilitychange", resumeWhenVisible);
    }
    if (localVncServerConnectionDurationTimer !== undefined) {
      window.clearInterval(localVncServerConnectionDurationTimer);
    }
    if (backgroundStatusTimer !== undefined) window.clearInterval(backgroundStatusTimer);
  });

  function showLocalShareMessage(message: string, tone: AppMessageTone = "success") {
    notify(message, tone);
  }

  watch(
    [localVncServerPort, localVncServerBindAddress, localVncServerViewOnly],
    ([port, bindAddress, viewOnly]) => {
      saveLocalShareSettings(storage, {
        port: port ?? 5903,
        bindAddress,
        viewOnly,
      });
    },
  );

  const localVncServerStatusSummary = computed(() =>
    summarizeLocalVncServerStatus(localVncServerOverview.value?.status ?? "stopped"),
  );
  const localAgentStatusSummary = computed(() =>
    summarizeLocalAgentStatus(localVncServerOverview.value?.agent.status ?? "stopped"),
  );
  const savedBackgroundCredential = computed(() => sharingMode.value === "background" && Boolean(backgroundShareOverview.value?.credentialConfigured));
  const localVncServerPasswordReady = computed(() => localVncServerPassword.value.length > 0 || savedBackgroundCredential.value);
  const localVncServerRunning = computed(() => sharingMode.value === "windowsService"
    ? ["running", "start_pending", "stop_pending"].includes(sharingServiceOverview.value?.status ?? "")
    : isLocalVncServerRunning(localVncServerOverview.value));
  const localVncServerStartInputLocked = computed(() =>
    localVncServerLoading.value || localVncServerRunning.value || localShareStartPending.value,
  );
  const serviceRunning = computed(() => ["running", "start_pending", "stop_pending"].includes(sharingServiceOverview.value?.status ?? ""));
  const localShareSetupSavedPassword = computed(() => !localShareSetupPasswordRequired.value
    && Boolean(sharingServiceOverview.value?.credentialConfigured && sharingServiceOverview.value.installed
      || !sharingServiceOverview.value?.installed && backgroundShareOverview.value?.credentialConfigured));
  const serviceSetupValid = computed(() => windowsService.formValid.value
    && (windowsService.password.value ? windowsService.passwordValid.value : localShareSetupSavedPassword.value));
  const localVncServerCanStart = computed(() => sharingMode.value === "windowsService"
    ? !localVncServerLoading.value && windowsService.canStart.value
    : !localVncServerLoading.value
    && !localShareStartPending.value
    && !localVncServerRunning.value
    && !(sharingServiceOverview.value?.supported && !sharingServiceOverview.value.installed && !installationSuggestionHandled.value)
    && validateLocalVncServerStartInput({
      port: localVncServerPort.value,
      bindAddress: localVncServerBindAddress.value,
      password: localVncServerPassword.value,
      allowSavedCredential: savedBackgroundCredential.value,
    }).valid,
  );
  const localVncServerCanToggle = computed(() => !localVncServerLoading.value
    && sharingMode.value !== null
    && !windowsService.busy.value
    && (!awaitingTemporaryPassword.value || !temporaryPasswordError.value)
    && (localShareServiceSetup.value ? serviceSetupValid.value && !serviceRunning.value
      : sharingMode.value !== "windowsService" || serviceRunning.value || localVncServerCanStart.value));
  const localVncServerCanCopyProfile = computed(() =>
    sharingMode.value !== null && !sharingRuntimeError.value
      && canCopyLocalVncServerConnectionProfile(localVncServerOverview.value),
  );
  const localVncServerPasswordInvalid = computed(() =>
    (awaitingTemporaryPassword.value && Boolean(localVncServerPassword.value && temporaryPasswordError.value))
    || hasLocalVncServerValidationError("password")
    || isLocalVncServerPasswordInputInvalid(
      localVncServerPassword.value,
      localVncServerPasswordPrompted.value,
      localVncServerOverview.value,
    ),
  );
  const localVncServerPortInvalid = computed(() => hasLocalVncServerValidationError("port"));
  const localVncServerBindAddressInvalid = computed(() => hasLocalVncServerValidationError("bindAddress"));
  const localVncServerToggleLabelKey = computed(() => {
    if (localSharePermissionChecking.value) return "localShare.refreshing";
    if (localShareStartPhase.value === "awaiting_permission") return "localShare.permissions.cancelStart";
    if (awaitingTemporaryPassword.value) return "localShare.temporaryPassword.start";
    if (localShareStartChecking.value || ["preparing", "starting"].includes(localShareStartPhase.value)) return "localShare.startingSharing";
    if (localVncServerOverview.value?.status === "starting" || (sharingMode.value === "windowsService" && sharingServiceOverview.value?.status === "start_pending")) return "localShare.startingSharing";
    if (localVncServerOverview.value?.status === "stopping" || (sharingMode.value === "windowsService" && sharingServiceOverview.value?.status === "stop_pending")) return "localShare.stoppingSharing";
    if (localVncServerLoading.value || windowsService.busy.value) return "localShare.serverWorking";
    if (localShareServiceSetup.value) return sharingServiceOverview.value?.installed
      ? "localShare.windowsService.initializeAndStart" : "localShare.windowsService.installAndStart";
    if (sharingMode.value === "windowsService" && windowsService.needsSetup.value && !serviceRunning.value) return "localShare.windowsService.initializeAndStart";
    return sharingMode.value === "windowsService"
      ? (localVncServerRunning.value ? "localShare.stopServer" : "localShare.startServer")
      : getLocalVncServerToggleLabelKey(localVncServerOverview.value);
  });
  const localVncServerToggleTitle = computed(() => t(localVncServerToggleLabelKey.value));
  const localVncServerCopyProfileTitle = computed(() =>
    t(getLocalVncServerCopyProfileTitleKey(localVncServerOverview.value)),
  );
  const localVncServerStartDisabledTitle = computed(() => {
    if (localVncServerRunning.value) {
      return "";
    }

    if (!localVncServerPasswordReady.value) {
      return t("localShare.accessPasswordRequired");
    }

    return "";
  });
  const localVncServerDisplayBindAddress = computed(() =>
    (localVncServerOverview.value?.status === "running" || sharingMode.value === "windowsService") && localVncServerOverview.value
      ? localVncServerOverview.value.bindAddress
      : localVncServerBindAddress.value,
  );
  const localVncServerDisplayPort = computed(() =>
    (localVncServerOverview.value?.status === "running" || sharingMode.value === "windowsService") && localVncServerOverview.value
      ? localVncServerOverview.value.port
      : localVncServerPort.value,
  );
  const localVncServerCopyHost = computed(() =>
    localVncServerOverview.value?.addresses.find((address) => address.kind === "lan_candidate")?.address
      ?? localVncServerOverview.value?.addresses[0]?.address
      ?? "127.0.0.1",
  );
  const localVncServerConnectionProfileText = computed(() =>
    localVncServerOverview.value
      ? createLocalVncServerConnectionExport(
          localVncServerOverview.value,
          localVncServerCopyHost.value,
          "Local VNC Server",
        )
      : "",
  );
  const localVncServerDiagnostics = computed<LocalVncServerDiagnosticView[]>(() => {
    const overview = localVncServerOverview.value;

    if (!overview) {
      return [
        {
          code: "status_unknown",
          severity: "info" as const,
          messageKey: "localShare.serverDiagnostic.statusUnknown.message",
          actionKey: "localShare.serverDiagnostic.statusUnknown.action",
        },
      ];
    }

    const diagnostics: LocalVncServerDiagnosticView[] = [];

    if (overview.status === "running") {
      diagnostics.push({
        code: "running",
        severity: "info" as const,
        messageKey: "localShare.serverDiagnostic.running.message",
        actionKey: "localShare.serverDiagnostic.running.action",
        values: {
          port: overview.port,
        },
      });
    } else if (overview.status === "stopped") {
      diagnostics.push({
        code: "stopped",
        severity: "warning" as const,
        messageKey: "localShare.serverDiagnostic.stopped.message",
        actionKey: "localShare.serverDiagnostic.stopped.action",
      });
    } else if (overview.status === "port_conflict") {
      diagnostics.push({
        code: "port_conflict",
        severity: "error" as const,
        messageKey: "localShare.serverDiagnostic.portConflict.message",
        actionKey: "localShare.serverDiagnostic.portConflict.action",
      });
    } else if (overview.status === "failed") {
      diagnostics.push({
        code: "failed",
        severity: "error" as const,
        messageKey: "localShare.serverDiagnostic.failed.message",
        actionKey: "localShare.serverDiagnostic.failed.action",
      });
    }

    for (const permission of overview.permissions) {
      if (permission.status === "missing") {
        diagnostics.push({
          code: `permission_${permission.kind}`,
          severity: "warning" as const,
          messageKey: "localShare.serverDiagnostic.permissionMissing.message",
          actionKey: "localShare.serverDiagnostic.permissionMissing.action",
          values: {
            permission: t(localVncServerPermissionKindKey(permission.kind)),
          },
        });
      }
    }

    return diagnostics.length ? diagnostics : [
      {
        code: "ready",
        severity: "info" as const,
        messageKey: "localShare.serverDiagnostic.ready.message",
        actionKey: "localShare.serverDiagnostic.ready.action",
      },
    ];
  });

  function refreshLocalVncServerStatus(): Promise<void> {
    if (localVncServerLoading.value || windowsService.busy.value) return Promise.resolve();
    return inspectLocalSharePermissions();
  }

  /** Diagnostics and connection events refresh live state without probing permissions. */
  function refreshLocalShareRuntimeStatus(): Promise<void> {
    if (localVncServerLoading.value || windowsService.busy.value) return Promise.resolve();
    return refreshDetachedStatus(true);
  }

  /** Settings read persisted startup state without preparing a permissions host. */
  function refreshSharingSettings(): Promise<void> {
    if (sharingAutoStartBusy.value) return Promise.resolve();
    return refreshDetachedStatus(true);
  }

  /** Opening the page probes the actual Mac host, preparing it only if absent.
   * This never sends a listener start or a system permission request. */
  function inspectLocalSharePermissions(): Promise<void> {
    if (inspectingPermissions) return inspectingPermissions;
    if (localShareStartPending.value || (localVncServerLoading.value && !refreshingStatus) || windowsService.busy.value) return Promise.resolve();
    localSharePermissionChecking.value = true;
    localSharePermissionCheckError.value = "";
    localVncServerMessage.value = "";
    localVncServerLoading.value = true;
    inspectingPermissions = (async () => {
      await refreshingStatus;
      localVncServerLoading.value = true;
      await readLocalVncServerStatus(true, true);
      localSharePermissionCheckError.value = sharingRuntimeError.value;
    })().finally(() => {
      localSharePermissionChecking.value = false;
      localVncServerLoading.value = false;
      inspectingPermissions = null;
    });
    return inspectingPermissions;
  }

  /** User-confirmed recovery replaces only the Mac background, without starting sharing. */
  async function restartBackgroundShare() {
    if (localShareStartPending.value) return;
    if (!backgroundRestartSupported.value || localVncServerLoading.value || windowsService.busy.value) return;
    localVncServerLoading.value = true;
    localVncServerPassword.value = "";
    try {
      await refreshingStatus;
      const overview = await backgroundApi.restart();
      acceptBackgroundOverview(overview);
      await refreshDetachedStatus(true);
    } catch (error) {
      await refreshDetachedStatus(true);
      showLocalShareMessage(formatLocalVncServerStartError(error), "error");
    } finally {
      localVncServerLoading.value = false;
    }
  }

  /** Settings maintenance owns no password or listener start intent. */
  function updateAndRepairBackgroundShare(): Promise<void> {
    if (!backgroundRestartSupported.value || localVncServerLoading.value || windowsService.busy.value || localShareStartPending.value) return Promise.resolve();
    localVncServerPassword.value = "";
    return localShareStartFlow.beginRepair();
  }

  function refreshDetachedStatus(quiet: boolean): Promise<void> {
    if (refreshingStatus) return refreshingStatus;
    refreshingStatus = readLocalVncServerStatus(quiet).finally(() => { refreshingStatus = null; });
    return refreshingStatus;
  }

  async function readLocalVncServerStatus(quiet: boolean, inspectPermissions = false, preparePermissionsHost = true) {
    if (!quiet) {
      localVncServerLoading.value = true;
      localVncServerMessage.value = "";
    }

    try {
      // Resolve from installed services on every refresh. Old mode preferences
      // cannot revive the removed in-app runtime or bypass a stopped service.
      const service = await serviceApi.status();
      windowsService.acceptOverview(service);
      const background = await backgroundApi.status(inspectPermissions).catch(error => {
        if (service.supported && service.installed) return null;
        throw error;
      });
      backgroundShareOverview.value = background;
      // Finish managing an existing background instance before adopting a newly
      // installed idle service. This never starts a replacement automatically.
      if (service.supported && service.installed && !(background?.server.status === "running" && service.status === "stopped")) {
        if (sharingMode.value !== "windowsService") localVncServerPassword.value = "";
        sharingMode.value = "windowsService";
        localVncServerOverview.value = await serviceApi.serverStatus();
      } else {
        let overview = background;
        if (!overview) throw new Error("background_share_runtime_failed");
        // One startup check, independent of opening Settings. Recover in the
        // host verifies that sharing is still idle before replacing it.
        if (overview.restartSupported && !backgroundUpdateChecked) {
          backgroundUpdateChecked = true;
          if (overview.processRunning && overview.server.status === "stopped"
            && overview.runtime && overview.runtime.buildId !== overview.currentBuildId) {
            overview = await backgroundApi.prepare();
          }
        }
        if (inspectPermissions && preparePermissionsHost && overview.restartSupported && !overview.processRunning) {
          overview = await backgroundApi.prepare();
        }
        if (sharingMode.value !== "background") {
          localVncServerPassword.value = "";
          backgroundFormInitialized = false;
        }
        sharingMode.value = "background";
        acceptBackgroundOverview(overview);
      }
      sharingRuntimeError.value = "";
      lastRuntimeErrorDiagnostic = "";
    } catch (error) {
      // Failed service detection is not evidence that it is uninstalled.
      sharingMode.value = null;
      if (localVncServerOverview.value) localVncServerOverview.value = { ...localVncServerOverview.value, status: "unknown" };
      const message = isTauriUnavailableError(error)
        ? t("localShare.backendUnavailable")
        : formatLocalVncServerStartError(error);
      const code = error instanceof Error ? error.message : typeof error === "string" ? error : "";
      const diagnostic = /^background_share_[a-z_]+(?::(?:[a-z_]+(?::\d+)?| stage=[a-z_]+;(?:win32|exit_code)=\d+))?$/.test(code) ? code : "";
      if (sharingRuntimeError.value !== message || lastRuntimeErrorDiagnostic !== diagnostic) {
        logEvent("error", t("log.sourceLocalShare"), diagnostic ? `${message} (${diagnostic})` : message);
      }
      lastRuntimeErrorDiagnostic = diagnostic;
      sharingRuntimeError.value = message;
      if (!quiet) {
        showLocalShareMessage(message, "error");
      }
    } finally {
      if (!quiet) localVncServerLoading.value = false;
    }
  }

  function acceptBackgroundOverview(overview: BackgroundShareOverview, updateForm = false, preserveForm = false) {
    if (overview.permissionSource === "background") localSharePermissionCheckError.value = "";
    backgroundShareOverview.value = overview;
    if (overview.permissionSource === "background") {
      lastBackgroundPermissions = overview.server.permissions.map(item => ({ ...item }));
      localVncServerOverview.value = overview.server;
    } else {
      // Keep last observed grants for display only. Startup reads the unmodified
      // background overview and checks the actual process again before listening.
      const permissions = overview.server.permissions.map(permission => {
        if (permission.status !== "unknown") return permission;
        const previous = lastBackgroundPermissions.find(item => item.kind === permission.kind && item.status !== "unknown");
        return previous ? { ...previous } : permission;
      });
      if (overview.permissionSource === "background_cached") lastBackgroundPermissions = permissions.map(item => ({ ...item }));
      localVncServerOverview.value = { ...overview.server, permissions };
    }
    if (preserveForm) return;
    const restoreSettings = updateForm || !backgroundFormInitialized;
    if (restoreSettings) backgroundFormInitialized = true;
    if ((restoreSettings && overview.configured) || overview.server.status === "running") {
      localVncServerPort.value = overview.server.port;
      localVncServerBindAddress.value = overview.server.bindAddress;
      localVncServerViewOnly.value = overview.server.viewOnly;
    }
  }

  /** Settings and daily actions share one gate; first installation may stop background sharing after confirmation. */
  async function manageSharingService(action: WindowsServiceAction, shouldContinue?: () => boolean, startAfterInstall = false) {
    if (localVncServerLoading.value || windowsService.busy.value || localShareStartPending.value) return;
    const previousLifecycleAction = localShareLifecycleAction.value;
    if (action === "start" || action === "stop") localShareLifecycleAction.value = action === "start" ? "starting" : "stopping";
    const revision = startCheckRevision;
    const checkIntent = action === "start" || startAfterInstall ? () => revision === startCheckRevision && (shouldContinue?.() ?? true) : shouldContinue;
    localVncServerLoading.value = true;
    try {
      await refreshingStatus;
      if (checkIntent && !checkIntent()) return;
      if (action === "install" && startAfterInstall) {
        await installAndStartSharingService(checkIntent ?? (() => true));
        return;
      }
      if (action === "install" && backgroundShareOverview.value?.server.status === "running") {
        const service = windowsService.overview.value;
        if (!service?.supported || (service.installed && (!startAfterInstall || !windowsService.needsSetup.value))) {
          showLocalShareMessage(t("localShare.windowsService.stopBackgroundFirst"), "warning");
          return;
        }
        const choice = await options.requestConfirm({
          title: t("localShare.windowsService.stopAndInstallTitle"),
          message: t("localShare.windowsService.stopAndInstallMessage"),
          primaryLabel: t("localShare.windowsService.stopAndInstall"),
          secondaryLabel: t("localShare.windowsService.cancelInstall"),
          danger: true,
        });
        if (choice !== "primary") return;
        if (checkIntent && !checkIntent()) return;
        const background = await backgroundApi.stop();
        acceptBackgroundOverview(background);
        if (background.server.status !== "stopped") {
          showLocalShareMessage(t("localShare.windowsService.stopBeforeInstallFailed"), "error");
          return;
        }
      }
      let startOutcome: ServiceStartOutcome | null = null;
      const succeeded = await windowsService.act(action, checkIntent, startAfterInstall, async request => {
        const outgoing = { ...request, temporaryPassword: true };
        try {
          const outcome = await serviceApi.startWithFallback(outgoing);
          startOutcome = outcome;
          return outcome.service ?? await serviceApi.status();
        } finally { outgoing.password = ""; }
      });
      if (action === "stop" && backgroundShareOverview.value?.server.status === "running") {
        backgroundShareOverview.value = await backgroundApi.stop();
      }
      await refreshDetachedStatus(true);
      if (startAfterInstall && windowsService.configured.value) localShareServiceSetup.value = false;
      // The backend owns the complete attempt and logs the original native
      // failure before handoff. Reflect its actual runtime, never save a mode.
      const outcome = startOutcome as ServiceStartOutcome | null;
      if (outcome?.serviceError) { showServiceStartOutcome(outcome); return; }
      if (action === "stop" && succeeded
        && windowsService.overview.value?.status === "stopped" && windowsService.overview.value.autoStart) {
        showLocalShareMessage(t("localShare.windowsService.autoStartHint"), "info");
      }
      if (windowsService.message.value && (windowsService.messageTone.value !== "success"
        || !["start", "stop", "enableAutoStart", "disableAutoStart", "authorizePasswordManagement"].includes(action))) {
        showLocalShareMessage(windowsService.message.value, windowsService.messageTone.value);
      }
    } catch (error) {
      await refreshDetachedStatus(true);
      showLocalShareMessage(formatLocalVncServerStartError(error), "error");
    } finally {
      if (action === "install") windowsService.password.value = "";
      localVncServerLoading.value = false;
      localShareLifecycleAction.value = previousLifecycleAction;
    }
  }

  function rememberInstallationSuggestion() {
    installationSuggestionHandled.value = true;
    try { storage?.setItem(SERVICE_SUGGESTION_STORAGE_KEY, "handled"); }
    catch { logEvent("warning", t("log.sourceLocalShare"), "Service installation suggestion preference could not be saved."); }
  }

  function showServiceStartOutcome(outcome: ServiceStartOutcome) {
    if (!outcome.serviceError) return;
    windowsService.message.value = "";
    const cause = t(windowsServiceErrorKey(outcome.serviceError).replace(".errors.", ".fallbackReasons."));
    if (outcome.background) {
      sharingMode.value = "background";
      acceptBackgroundOverview(outcome.background);
      showLocalShareMessage(t("localShare.windowsService.fallbackSucceeded", { reason: cause }), "warning");
    } else if (outcome.passwordRequired) {
      showLocalShareMessage(t("localShare.windowsService.fallbackPasswordRequired", { reason: cause }), "warning");
    } else {
      const detail = outcome.fallbackError?.startsWith("background_share_")
        ? t(backgroundShareErrorKey(outcome.fallbackError)) : t(windowsServiceErrorKey(outcome.fallbackError));
      showLocalShareMessage(t("localShare.windowsService.fallbackFailed", { reason: cause, detail }), "error");
    }
  }

  async function installAndStartSharingService(shouldContinue: () => boolean) {
    const validation = validateLocalVncServerStartInput({
      port: windowsService.port.value, bindAddress: windowsService.bindAddress.value,
      password: windowsService.password.value, allowSavedCredential: localShareSetupSavedPassword.value,
    });
    if (!validation.valid) {
      windowsService.password.value = "";
      localShareSetupPasswordRequired.value = validation.errors.some(error => error.field === "password");
      showLocalShareMessage(t(validation.errors[0]?.messageKey ?? "localShare.serverStartFailed"), "warning");
      return;
    }
    const request = { ...validation.value, viewOnly: windowsService.viewOnly.value };
    validation.value.password = "";
    windowsService.password.value = "";
    const reusePassword = !request.password && !sharingServiceOverview.value?.installed;
    let installed = false;
    try {
      if (backgroundShareOverview.value?.server.status === "running") {
        const choice = await options.requestConfirm({
          title: t("localShare.windowsService.stopAndInstallTitle"),
          message: t("localShare.windowsService.switchAfterInstallMessage"),
          primaryLabel: t("localShare.windowsService.installAndStart"),
          secondaryLabel: t("localShare.windowsService.cancelInstall"), danger: true,
        });
        if (choice !== "primary" || !shouldContinue()) return;
      }
      if (!shouldContinue()) return;
      rememberInstallationSuggestion();
      // Do not stop the user's listener before the UAC decision and setup finish.
      const service = await serviceApi.install(request, reusePassword);
      installed = true;
      windowsService.acceptOverview(service, true);
      if (!shouldContinue()) return;
      if (backgroundShareOverview.value?.server.status === "running") {
        const background = await backgroundApi.stop();
        acceptBackgroundOverview(background);
        if (background.server.status !== "stopped") throw new Error("windows_service_stop_failed");
      }
      if (!shouldContinue()) return;
      const outgoing = { ...request, temporaryPassword: true };
      try {
        const outcome = await serviceApi.startWithFallback(outgoing, reusePassword);
        if (outcome.service) windowsService.acceptOverview(outcome.service);
        await refreshDetachedStatus(true);
        showServiceStartOutcome(outcome);
      } finally { outgoing.password = ""; }
    } catch (error) {
      const key = windowsServiceErrorKey(error);
      // Only cancelling the installation UAC continues ordinary sharing. A
      // partially installed service or unknown status must keep service priority.
      if (!installed && key === "localShare.windowsService.errors.uac_cancelled") {
        rememberInstallationSuggestion();
        const service = await serviceApi.status();
        windowsService.acceptOverview(service);
        if (service.supported && !service.installed && shouldContinue()) {
          localShareServiceSetup.value = false;
          if (backgroundShareOverview.value?.server.status !== "running") {
            const background = await backgroundApi.start(request);
            sharingMode.value = "background";
            acceptStartedOverview(background);
          }
          showLocalShareMessage(t("localShare.windowsService.installCancelledOrdinary"), "info");
          return;
        }
      }
      if (String(error).includes("credential_missing") || String(error).includes("credential_unavailable")) {
        localShareSetupPasswordRequired.value = true;
      }
      logEvent("error", t("log.sourceLocalShare"), `Service setup failed: ${key}`);
      const detail = String(error).includes("background_share_") ? backgroundShareErrorKey(error) : key;
      if (!installed && ["uac_disabled", "admin_required", "elevation_denied", "elevation_failed"]
        .some(code => key === `localShare.windowsService.errors.${code}`)) {
        request.password = "";
        if (!shouldContinue()) return;
        const choice = await options.requestConfirm({
          title: t("localShare.windowsService.installNeedsAdminTitle"),
          message: key === "localShare.windowsService.errors.uac_disabled"
            ? t("localShare.windowsService.installUacDisabledGuide")
            : `${t(key)}\n\n${t("localShare.windowsService.installNeedsAdminGuide")}`,
          primaryLabel: t("localShare.windowsService.goToRepair"),
          secondaryLabel: t("localShare.windowsService.returnToSharing"),
        });
        if (choice === "primary" && shouldContinue()) options.openSharingRepairSettings();
        return;
      }
      showLocalShareMessage(t(detail), "error");
    } finally {
      request.password = "";
      await refreshDetachedStatus(true);
      if (sharingServiceOverview.value?.installed && windowsService.configured.value) localShareServiceSetup.value = false;
    }
  }

  /** Startup preference has its own save action and never owns a listener start. */
  async function setSharingAutoStart(enabled: boolean): Promise<void> {
    if (sharingAutoStartBusy.value || !sharingAutoStartKnown.value) return;
    localVncServerLoading.value = true;
    try {
      await refreshingStatus;
      if (!sharingAutoStartKnown.value) return;
      const credentialConfigured = sharingServiceOverview.value?.installed === true
        ? sharingServiceOverview.value?.credentialConfigured : backgroundShareOverview.value?.credentialConfigured;
      if (enabled && !credentialConfigured) {
        showLocalShareMessage(t(sharingServiceOverview.value?.installed === true
          ? "localShare.windowsService.errors.credential_missing" : "localShare.background.errors.auto_start_credential_required"), "warning");
        return;
      }
      if (sharingServiceOverview.value?.installed === true) {
        localVncServerLoading.value = false;
        await manageSharingService(enabled ? "enableAutoStart" : "disableAutoStart");
        return;
      }
      const overview = await backgroundApi.setAutoStart(enabled);
      acceptBackgroundOverview(overview, false, true);
    } catch (error) {
      showLocalShareMessage(formatLocalVncServerStartError(error), "error");
    } finally { localVncServerLoading.value = false; }
  }

  /** Self-connections read the same owned instance as the sharing page. */
  async function readLocalShareServerStatus(): Promise<LocalVncServerOverview> {
    await refreshDetachedStatus(true);
    if (!sharingMode.value || !localVncServerOverview.value) throw new Error("background_share_runtime_failed");
    return localVncServerOverview.value;
  }

  /** Service display changes use its authenticated Agent; local IPC owns only background views. */
  async function updateLocalShareServerView(request: LocalVncServerViewRequest): Promise<LocalVncServerOverview> {
    if (sharingMode.value !== "background") throw new Error("agent_channel_unavailable");
    const overview = await backgroundApi.view(request);
    acceptBackgroundOverview(overview);
    return overview.server;
  }

  function formatLocalVncServerStartError(error: unknown): string {
    if (String(error).includes("background_share_")) return t(backgroundShareErrorKey(error));
    if (String(error).includes("windows_service_")) return t(windowsServiceErrorKey(error));
    const copy = getLocalVncServerStartErrorCopy(error);

    if (copy) {
      return t(copy.messageKey);
    }

    return t("localShare.serverStartFailed", { error: String(error) });
  }

  function hasLocalVncServerValidationError(field: LocalVncServerStartValidationField): boolean {
    return localVncServerValidationErrors.value.some((error) => error.field === field);
  }

  async function startLocalVncServer() {
    if (localVncServerLoading.value || windowsService.busy.value
      || (localShareStartPending.value && !awaitingTemporaryPassword.value)) return;
    localShareLifecycleAction.value = "starting";
    try {
      if (awaitingTemporaryPassword.value) {
        if (sharingMode.value === "background" && !temporaryPasswordError.value) {
          const password = localVncServerPassword.value;
          localVncServerPassword.value = "";
          await localShareStartFlow.submitTemporaryPassword(password);
        }
      } else {
        await performLocalVncServerStart();
      }
    } finally {
      localShareLifecycleAction.value = "idle";
    }
  }

  async function performLocalVncServerStart() {
    if (localShareServiceSetup.value) {
      const revision = ++startCheckRevision;
      await manageSharingService("install", () => localShareServiceSetup.value && revision === startCheckRevision, true);
      return;
    }
    localVncServerLoading.value = true;
    const revision = ++startCheckRevision;
    localShareStartChecking.value = true;
    await refreshingStatus;
    await readLocalVncServerStatus(true, true, false);
    localShareStartChecking.value = false;
    localVncServerLoading.value = false;
    if (revision !== startCheckRevision) return;
    if (!sharingMode.value || localVncServerRunning.value) return;
    localVncServerMessage.value = "";
    if (sharingServiceOverview.value?.installed || sharingMode.value === "windowsService") {
      await manageSharingService("start", () => revision === startCheckRevision);
      return;
    }

    const validation = validateLocalVncServerStartInput({
      port: localVncServerPort.value,
      bindAddress: localVncServerBindAddress.value,
      password: localVncServerPassword.value,
      allowSavedCredential: savedBackgroundCredential.value,
    });

    if (!validation.valid) {
      localVncServerValidationErrors.value = validation.errors;
      localVncServerPasswordPrompted.value = validation.errors.some((error) => error.field === "password");
      localVncServerMessageTone.value = "warning";
      localVncServerMessage.value = t(validation.errors[0]?.messageKey ?? "localShare.serverStartFailed");
      showLocalShareMessage(localVncServerMessage.value, "warning");
      return;
    }

    if (sharingServiceOverview.value?.supported && !sharingServiceOverview.value.installed && !installationSuggestionHandled.value) {
      localVncServerLoading.value = true;
      try {
        const choice = await options.requestConfirm({
          title: t("localShare.windowsService.suggestionTitle"),
          message: t("localShare.windowsService.suggestionMessage"),
          primaryLabel: t("localShare.windowsService.installAndStart"),
          secondaryLabel: t("localShare.windowsService.startOrdinary"), dismissChoice: "cancel",
        });
        if (revision !== startCheckRevision || choice === "cancel") return;
        rememberInstallationSuggestion();
        if (choice === "primary") {
          prepareWindowsServiceSetup();
          localVncServerLoading.value = false;
          await manageSharingService("install", () => revision === startCheckRevision, true);
          return;
        }
      } finally { validation.value.password = ""; localVncServerLoading.value = false; }
      // Re-read only the secret from the still-locked form after choosing ordinary.
      validation.value.password = localVncServerPassword.value;
    }

    localVncServerValidationErrors.value = [];
    localVncServerPasswordPrompted.value = false;
    localVncServerLoading.value = true;

    const request = {
      port: validation.value.port,
      bindAddress: validation.value.bindAddress,
      password: validation.value.password,
      viewOnly: localVncServerViewOnly.value,
    };
    localVncServerPassword.value = "";
    validation.value.password = "";
    if (backgroundRestartSupported.value) {
      await localShareStartFlow.begin(request);
      return;
    }
    try {
      acceptStartedOverview(await backgroundApi.start(request));
    } catch (error) { showStartError(error); }
    finally { request.password = ""; localVncServerLoading.value = false; }
  }

  function acceptStartedOverview(background: BackgroundShareOverview) {
    acceptBackgroundOverview(background, true);
    localVncServerOverview.value = background.server;
    localVncServerPort.value = background.server.port;
    localVncServerMessageTone.value = "success";
    localVncServerMessage.value = t("localShare.serverStartedToast");
    logEvent("success", t("log.sourceLocalShare"), localVncServerMessage.value);
  }

  function showStartError(error: unknown) {
    localVncServerMessageTone.value = "error";
    localVncServerMessage.value = formatLocalVncServerStartError(error);
    showLocalShareMessage(localVncServerMessage.value, "error");
    logEvent("error", t("log.sourceLocalShare"), localVncServerMessage.value);
  }

  function cancelLocalShareStart() {
    startCheckRevision++;
    localShareStartChecking.value = false;
    if (awaitingTemporaryPassword.value) localVncServerPassword.value = "";
    localShareStartFlow.cancel();
  }

  function beginWindowsServiceSetup() {
    if (localShareLifecycleAction.value !== "idle" || windowsService.busy.value || localShareStartPending.value
      || sharingServiceOverview.value?.supported !== true) return;
    if ((sharingServiceOverview.value.installed && !windowsService.needsSetup.value) || localShareServiceSetup.value) return;
    prepareWindowsServiceSetup();
  }

  function prepareWindowsServiceSetup() {
    if (sharingServiceOverview.value?.installed) { localShareServiceSetup.value = true; return; }
    windowsService.port.value = localVncServerPort.value;
    windowsService.bindAddress.value = localVncServerBindAddress.value;
    windowsService.viewOnly.value = localVncServerViewOnly.value;
    windowsService.password.value = localVncServerPassword.value;
    localVncServerPassword.value = "";
    localShareServiceSetup.value = true;
  }

  let settingsInstallPending = false;
  async function requestWindowsServiceInstallation() {
    if (settingsInstallPending || localShareLifecycleAction.value !== "idle" || windowsService.busy.value) return;
    settingsInstallPending = true;
    const revision = startCheckRevision;
    try {
      await inspectingPermissions;
      await refreshingStatus;
      await readLocalVncServerStatus(true, false, false);
      if (revision !== startCheckRevision || sharingRuntimeError.value) return;
      beginWindowsServiceSetup();
      if (!localShareServiceSetup.value) return;
      if (serviceSetupValid.value) await startLocalVncServer();
      else localShareSetupPasswordRequired.value = !windowsService.password.value && !localShareSetupSavedPassword.value;
    } finally { settingsInstallPending = false; }
  }

  function cancelWindowsServiceSetup() {
    if (localShareServiceSetup.value && !sharingServiceOverview.value?.installed) {
      localVncServerPort.value = windowsService.port.value;
      localVncServerBindAddress.value = windowsService.bindAddress.value;
      localVncServerViewOnly.value = windowsService.viewOnly.value;
    }
    localShareServiceSetup.value = false;
    windowsService.password.value = "";
    cancelLocalShareStart();
  }

  async function stopLocalVncServer() {
    if (localShareStartPending.value || localVncServerLoading.value || windowsService.busy.value || !sharingMode.value) return;
    if (sharingMode.value === "windowsService") { await manageSharingService("stop"); return; }
    localShareLifecycleAction.value = "stopping";
    localVncServerLoading.value = true;
    localVncServerMessage.value = "";

    try {
      await refreshingStatus;
      const background = await backgroundApi.stop();
      acceptBackgroundOverview(background);
      localVncServerMessageTone.value = "success";
      localVncServerMessage.value = t("localShare.serverStopped");
      logEvent("info", t("log.sourceLocalShare"), t("localShare.serverStopped"));
      if (background.server.status === "stopped" && background.autoStart) {
        showLocalShareMessage(t("localShare.background.autoStartHint"), "info");
      }
    } catch (error) {
      localVncServerMessageTone.value = "error";
      localVncServerMessage.value = formatLocalVncServerStartError(error);
      showLocalShareMessage(localVncServerMessage.value, "error");
      logEvent("error", t("log.sourceLocalShare"), localVncServerMessage.value);
    } finally {
      localVncServerLoading.value = false;
      localShareLifecycleAction.value = "idle";
    }
  }

  async function toggleLocalVncServer() {
    if (localShareServiceSetup.value || awaitingTemporaryPassword.value) { await startLocalVncServer(); return; }
    if (localShareStartPhase.value === "awaiting_permission") { cancelLocalShareStart(); return; }
    if (localVncServerRunning.value) {
      await stopLocalVncServer();
      return;
    }

    await startLocalVncServer();
  }

  async function requestLocalVncServerPermission(permission: LocalVncServerPermission) {
    if (sharingMode.value !== "background" || localVncServerLoading.value || localShareStartPending.value) return;
    if (backgroundRestartSupported.value) {
      await localShareStartFlow.beginPermission(permission.kind);
      return;
    }
    localVncServerLoading.value = true;

    try {
      const kind = permission.kind as LocalVncServerPermissionKind;
      const result = await backgroundApi.permission(kind);
      const message = result.opened
        ? t("localShare.permissions.opened")
        : t("localShare.permissions.openFallback", { message: result.message });
      if (!result.opened) showLocalShareMessage(message, "warning");
      logEvent("info", t("log.sourceLocalShare"), message);
      await refreshDetachedStatus(true);
    } catch (error) {
      const message = t("localShare.permissions.openFailed", { error: formatLocalVncServerStartError(error) });
      showLocalShareMessage(message, "error");
      logEvent("error", t("log.sourceLocalShare"), message);
    } finally {
      localVncServerLoading.value = false;
    }
  }

  function formatLocalVncServerConnectionDuration(connection: LocalVncServerActiveConnection): string {
    localVncServerConnectionDurationTick.value;
    const connectedAt = Number.parseInt(connection.connectedAt, 10);
    if (!Number.isFinite(connectedAt) || connectedAt <= 0) {
      return "-";
    }

    const elapsedSeconds = Math.max(0, localVncServerConnectionDurationTick.value - connectedAt);
    const hours = Math.floor(elapsedSeconds / 3600);
    const minutes = Math.floor((elapsedSeconds % 3600) / 60);
    const seconds = elapsedSeconds % 60;

    if (hours > 0) {
      return t("localShare.connectionDevices.durationHours", { hours, minutes });
    }

    if (minutes > 0) {
      return t("localShare.connectionDevices.durationMinutes", { minutes, seconds });
    }

    return t("localShare.connectionDevices.durationSeconds", { seconds });
  }

  function formatLocalVncServerConnectionTime(connection: LocalVncServerActiveConnection, compact = false): string {
    const connectedAt = Number.parseInt(connection.connectedAt, 10);
    if (!Number.isFinite(connectedAt) || connectedAt <= 0) {
      return "-";
    }

    const date = new Date(connectedAt * 1000);
    // Use the live duration clock so the compact label also updates after midnight.
    const now = new Date(localVncServerConnectionDurationTick.value * 1000);
    if (compact && date.toDateString() === now.toDateString()) {
      return t("localShare.connectionDevices.todayAt", { time: date.toLocaleTimeString() });
    }

    return date.toLocaleString();
  }

  async function disconnectLocalVncServerConnection(connection: LocalVncServerActiveConnection) {
    if (sharingMode.value === null || sharingRuntimeError.value) return;
    localVncServerLoading.value = true;

    try {
      const overview = sharingMode.value === "windowsService"
        ? await serviceApi.disconnect(connection.id)
        : (await backgroundApi.disconnect(connection.id)).server;
      localVncServerOverview.value = overview;
      const message = t("localShare.connectionDevices.disconnected");
      logEvent("info", t("log.sourceLocalShare"), message);
      await refreshDetachedStatus(true);
    } catch (error) {
      if (String(error).includes("screen_lock_failed")) {
        await refreshDetachedStatus(true);
        showLocalShareMessage(t("localShare.disconnectLockFailed"), "error");
        logEvent("warning", t("log.sourceLocalShare"), "Device disconnected; local screen lock request failed.");
        return;
      }
      const detail = sharingMode.value === "windowsService" && String(error).includes("windows_service_")
        ? t(windowsServiceErrorKey(error)) : String(error);
      const message = t("localShare.connectionDevices.disconnectFailed", { error: detail });
      showLocalShareMessage(message, "error");
      logEvent("error", t("log.sourceLocalShare"), message);
    } finally {
      localVncServerLoading.value = false;
    }
  }

  async function copyLocalVncServerConnectionProfile() {
    if (!localVncServerCanCopyProfile.value) {
      showLocalShareMessage(t("localShare.copyServerProfileDisabled"), "warning");
      return;
    }

    try {
      await navigator.clipboard.writeText(localVncServerConnectionProfileText.value);
      showLocalShareMessage(t("localShare.serverCopied"), "success");
      logEvent("success", t("log.sourceLocalShare"), t("localShare.serverCopied"));
    } catch (error) {
      const message = t("localShare.copyFailed", { error: String(error) });
      showLocalShareMessage(message, "error");
      logEvent("error", t("log.sourceLocalShare"), message);
    }
  }

  async function copyLocalVncServerAddress(address: string) {
    if (sharingMode.value === null || sharingRuntimeError.value) return;
    try {
      await navigator.clipboard.writeText(address);
      showLocalShareMessage(t("localShare.serverAddressCopied"), "success");
      logEvent("success", t("log.sourceLocalShare"), t("localShare.serverAddressCopied"));
    } catch (error) {
      const message = t("localShare.copyFailed", { error: String(error) });
      showLocalShareMessage(message, "error");
      logEvent("error", t("log.sourceLocalShare"), message);
    }
  }

  async function copyLocalVncServerPort() {
    if (sharingMode.value === null || sharingRuntimeError.value) return;
    try {
      await navigator.clipboard.writeText(String(localVncServerDisplayPort.value));
      showLocalShareMessage(t("localShare.serverPortCopied"), "success");
      logEvent("success", t("log.sourceLocalShare"), t("localShare.serverPortCopied"));
    } catch (error) {
      const message = t("localShare.copyFailed", { error: String(error) });
      showLocalShareMessage(message, "error");
      logEvent("error", t("log.sourceLocalShare"), message);
    }
  }

  return {
    localShareServiceSetup, beginWindowsServiceSetup, cancelWindowsServiceSetup, requestWindowsServiceInstallation, localShareSetupSavedPassword, localShareSetupPasswordRequired, localShareFallbackActive,
    sharingMode, sharingRuntimeError, backgroundShareOverview, sharingServiceOverview,
    backgroundRestartSupported, backgroundRuntimeOutdated, backgroundPermissionRepairNeeded, restartBackgroundShare, updateAndRepairBackgroundShare,
    localShareStartPhase, localShareStartPending, localShareMissingPermissions, cancelLocalShareStart, localShareLifecycleAction,
    localShareRequiredPermissions, localShareSignatureChangedPermissions, localShareGuidePurpose, requestLocalShareGuidePermission,
    openLocalShareGuideSettings,
    localSharePermissionChecking, localSharePermissionCheckError, inspectLocalSharePermissions, localShareStartChecking,
    resumeLocalShareStart,
    windowsService, manageSharingService,
    readLocalShareServerStatus, updateLocalShareServerView,
    sharingAutoStart, sharingAutoStartKnown, sharingAutoStartBusy, setSharingAutoStart, refreshSharingSettings,
    copyLocalVncServerAddress,
    copyLocalVncServerConnectionProfile,
    copyLocalVncServerPort,
    disconnectLocalVncServerConnection,
    formatLocalVncServerConnectionDuration,
    formatLocalVncServerConnectionTime,
    formatLocalVncServerStartError,
    localVncServerBindAddress,
    localAgentStatusSummary,
    localVncServerCanCopyProfile,
    localVncServerCanStart,
    localVncServerCanToggle,
    localVncServerConnectionProfileText,
    localVncServerCopyProfileTitle,
    localVncServerCopyHost,
    localVncServerDiagnostics,
    localVncServerDisplayBindAddress,
    localVncServerDisplayPort,
    localVncServerLoading,
    localVncServerMessage,
    localVncServerMessageTone,
    localVncServerOverview,
    localVncServerPassword,
    localVncServerBindAddressInvalid,
    localVncServerPasswordInvalid,
    localVncServerPasswordReady,
    localVncServerPermissionKindKey,
    localVncServerPermissionStatusKey,
    localVncServerPort,
    localVncServerPortInvalid,
    localVncServerStartDisabledTitle,
    localVncServerStartInputLocked,
    localVncServerStatusSummary,
    localVncServerToggleLabelKey,
    localVncServerToggleTitle,
    localVncServerViewOnly,
    refreshLocalVncServerStatus,
    refreshLocalShareRuntimeStatus,
    requestLocalVncServerPermission,
    startLocalVncServer,
    stopLocalVncServer,
    toggleLocalVncServer,
  };
}
