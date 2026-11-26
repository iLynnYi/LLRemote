import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAppConfirmDialog } from "./useAppConfirmDialog.ts";
import { useWorkspaceNavigation } from "./useWorkspaceNavigation.ts";
import { browserSessionSettingsStorage } from "@app/utils/sessionSettingsStorage";
import {
  createTrayStateSnapshot,
  getTrayNotifications,
  type TrayLocalShareAction,
} from "@app/trayState";
import type {
  AppMessageEvent,
  AppMessageTone,
  CoreTransportHandle,
  ConnectVncOptions,
  PasswordPromptState,
  Translate,
  WorkspaceMode,
} from "@app/types";
import { useHostResourceActions } from "@features/hosts/composables/useHostResourceActions";
import { useHostResourceState } from "@features/hosts/composables/useHostResourceState";
import { useHostWorkbench } from "@features/hosts/composables/useHostWorkbench";
import { useHostGroupPanel } from "@features/hosts/composables/useHostGroupPanel";
import { useLocalShareWorkbench } from "@features/local-share/composables/useLocalShareWorkbench";
import { useConnectionConfirmFlow } from "@features/sessions/composables/useConnectionConfirmFlow";
import { useSessionDerivedState } from "@features/sessions/composables/useSessionDerivedState";
import { useSessionDuration } from "@features/sessions/composables/useSessionDuration";
import { useSessionExternalWindow } from "@features/sessions/composables/useSessionExternalWindow";
import { useSessionExternalWindowCoordinator } from "@features/sessions/composables/useSessionExternalWindowCoordinator";
import { useSessionFullscreen } from "@features/sessions/composables/useSessionFullscreen";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useSessionArtifacts, type SessionArtifactsInvoke } from "@features/sessions/composables/useSessionArtifacts";
import { useSessionClipboardReports } from "@features/sessions/composables/useSessionClipboardReports";
import {
  useSessionConnectionLifecycle,
} from "@features/sessions/composables/useSessionConnectionLifecycle";
import { useSessionDisplayControls } from "@features/sessions/composables/useSessionDisplayControls";
import { useSessionLocalServerControls } from "@features/sessions/composables/useSessionLocalServerControls";
import { useSessionRuntimeState } from "@features/sessions/composables/useSessionRuntimeState";
import { useSettingsWorkbench } from "@features/settings/composables/useSettingsWorkbench";
import { appendEventLog, type EventLogEntry, type EventLogLevel } from "@diagnostics/eventLog";
import { createRuntimeFileLogger, summarizePerformanceLog } from "@diagnostics/runtimeFileLogger";
import type { DiagnosticEventContext } from "@diagnostics/eventLog";
import {
  createSessionManager,
  summarizeSessionTabs,
  type RemoteSession,
  type RemoteSessionTab,
} from "@session/core/sessionManager";
import {
  summarizeRemoteViewport,
  type RemoteViewportSummary,
} from "@session/viewport/remoteViewport";
import {
  summarizeRemoteViewportStatusHud,
  type RemoteViewportStatusHudSummary,
} from "@session/viewport/remoteViewportStatusBar";
import type { SessionDiagnosticsInputActivity } from "@session/diagnostics/sessionDiagnosticsPanel";
import { createSessionConnectionRegistry } from "@session/core/sessionConnectionRegistry";
import type { KeyboardShortcutConflict } from "@session/input/keyboardShortcut";
import { createRemoteKeyboardMapper } from "@session/input/keyboardMapping";
import { detectKeyboardShortcutDisplayPlatform } from "@session/input/keyboardShortcut";
import { sessionKeyboardPreferences } from "../../features/sessions/composables/useSessionKeyboardPreferences.ts";
import { shouldCaptureSessionKeyboardEvent } from "@session/input/sessionKeyboardCapture";
import { createSessionWindowInputFocus } from "@session/input/sessionWindowInputFocus";
import {
  summarizeWorkspaceRenderSlots,
  type SessionWorkspaceRenderSlot,
} from "@session/viewport/sessionWorkspace";
import {
  isSessionExternalWindowLocation,
} from "@session/external-window/sessionExternalWindow";
import {
  createRemoteSessionCoreClient,
  type RemoteSessionSnapshot,
} from "@session/protocol/remoteSessionCore";
import { createSessionFileUploadActions } from "@session/transfers/sessionFileUploads";
import { createFileTransferManager } from "@session/transfers/fileTransferManager";
import { expandFileTransferSourcesInPages } from "@session/transfers/fileTransferSources";
import {
  preparedFileTransferTaskStore,
  setFileTransferStoreErrorHandler,
} from "@session/transfers/fileTransferStore";
import { sessionSupportsAction } from "@session/transfers/sessionCapabilities";
import { createVncProtocolAdapter } from "@session/protocol/protocolAdapter";
import { createClipboardCoordinator } from "@session/clipboard/clipboardCoordinator";
import { createSystemClipboard } from "@session/clipboard/systemClipboard";
import { createSessionRecordingSource } from "@session/artifacts/sessionRecordingSource";
import {
  formatHostDateTime,
  HOST_GROUP_NAME_MAX_LENGTH,
  type HostRecord,
} from "@hosts/hostManager";
import {
  localVncServerPermissionKindKey,
  localVncServerPermissionStatusKey,
} from "@server/localShare";
import { createTrayActions } from "./vncAppState/tray.ts";
import {
  createExternalWindowStateBridge,
  createFallbackWorkspaceRenderSlot,
  isKeyboardEditableTarget,
} from "./vncAppState/viewport.ts";
import { createPasswordPromptController } from "./vncAppState/passwordPrompt.ts";
import { createHostConnectionActions } from "./vncAppState/hostConnection.ts";
import { createLocalVncServerDiagnosticsRefresh } from "./vncAppState/localDiagnostics.ts";
import { createVncAppLifecycle } from "./vncAppState/lifecycle.ts";
import {
  checkboxValue,
  isTauriUnavailableError,
  setElementTarget,
} from "./vncAppState/utils.ts";

export function useVncAppState() {
  const sessionMessageDialogRequest = ref<{ sessionId: string } | null>(null);
  function openSessionMessageDialog(sessionId: string) {
    sessionMessageDialogRequest.value = { sessionId };
  }
  const { t } = useI18n();
  const translate = t as Translate;
  const initialExternalWindowLocation = isSessionExternalWindowLocation(window.location.href);
  let initialExternalSessionIdUsed = false, appMessageEventId = 0;
  const appMessageEvent = ref<AppMessageEvent | null>(null);

  function showAppMessage(content: string, tone: AppMessageTone = "info") {
    appMessageEvent.value = { id: ++appMessageEventId, tone, content };
  }
  setFileTransferStoreErrorHandler(() => showAppMessage(t("sessions.fileTransfers.storageUnavailable"), "error"));

  const runtimeFileLogger = createRuntimeFileLogger(invoke as never);
  const vncAdapter = createVncProtocolAdapter({ invoke });
  const clipboardCoordinator = createClipboardCoordinator(createSystemClipboard(), {
    onWarning: (message) => logSystemEvent("warning", "剪贴板", message),
  });
  const remoteSessionCore = createRemoteSessionCoreClient({ invoke });
  const browserStorage = browserSessionSettingsStorage();
  const sessionConnectionRegistry = createSessionConnectionRegistry();
  const sessionFileTransferManager = createFileTransferManager({
    store: initialExternalWindowLocation ? undefined : preparedFileTransferTaskStore(browserStorage),
    restoreUpload: (task) => async ({ taskId, signal, onProgress }) => {
      const connection = sessionConnectionRegistry.get(task.sessionId)?.connection;
      if (!connection || !sessionSupportsAction(connection.connectionModel.capabilities, "sendFile")) {
        throw new Error("agent_unreachable");
      }
      return await connection.uploadRemoteFile(task.filePath, {
        transferId: taskId,
        relativePath: task.relativePath,
        renamePolicy: sessionSettings.value.fileTransferRenamePolicy,
        signal,
        onProgress: (progress) => onProgress({
          transferredBytes: progress.transferredBytes,
          totalBytes: progress.totalBytes,
        }),
      });
    },
    restoreControl: (task) => async (taskId, action) => {
      const connection = sessionConnectionRegistry.get(task.sessionId)?.connection;
      if (!connection || !sessionSupportsAction(connection.connectionModel.capabilities, "sendFile")) {
        throw new Error("agent_unreachable");
      }
      await connection.controlRemoteFileTransfer(taskId, action);
    },
  });
  const { enqueue: enqueueSessionFileUploads } = createSessionFileUploadActions({
    findSession,
    getConnection: (sessionId) => sessionConnectionRegistry.get(sessionId)?.connection,
    getRenamePolicy: () => sessionSettings.value.fileTransferRenamePolicy,
    expandSources: (paths) => expandFileTransferSourcesInPages(invoke, paths),
    enqueueUploads: sessionFileTransferManager.enqueueUploads,
    enqueueUploadsDurably: sessionFileTransferManager.enqueueUploadsDurably,
    t: translate,
    onUploaded: (result) => logEvent("success", translate("log.sourceSession"),
      `Agent file upload completed; file=${result.fileName}; bytes=${result.bytesWritten}; remote_path=${result.remotePath}; sha256=${result.sha256}`),
  });
  const globalTransferDrawerOpen = ref(false);
  const globalTransferDrawerSessionId = ref<string | null>(null);
  const remoteSessionCoreSnapshots = ref<RemoteSessionSnapshot[]>([]);

  async function refreshRemoteSessionCoreSnapshots() {
    remoteSessionCoreSnapshots.value = await remoteSessionCore.listSnapshots();
  }

  const activeWorkspace = ref<WorkspaceMode>("hosts"), diagnosticsCollapsed = ref(false);
  const sessionDeveloperDiagnosticsVisible = ref(false);
  const {
    activeHostScopeLabel,
    canCreateGroup,
    canSaveGroupRename,
    editingGroupName,
    filteredHosts,
    groupRenameDraft,
    groupValidationMessage,
    hostGroupOptions,
    hostGroups,
    hostRecords,
    hostSearchQuery,
    hostStorageMessage,
    hostSummary,
    newGroupDraft,
    newGroupValidation,
    renameGroupValidation,
    resetGroupEditingState,
    selectHostGroupState,
    selectAllHostsState,
    selectedGroup,
    selectedGroupIsDefault,
    selectedHost,
    selectedHostId,
    startGroupEditState,
    upsertHostRecord,
  } = useHostResourceState(translate, activeWorkspace);
  const {
    discardHostDraftChanges,
    editingHostId,
    hostCreateVisible,
    hostCredentialMethod,
    hostCredentialPlaceholder,
    hostDraft,
    hostDraftDirty,
    hostDraftSnapshot,
    hostEditorMode,
    openHostCreateDraft,
    openHostEditDraft,
    resetHostDraft,
    serializeHostDraft,
  } = useHostWorkbench();
  const host = ref("127.0.0.1");
  const port = ref(5900);
  const password = ref("");
  const passwordPrompt = ref<PasswordPromptState>({
    visible: false,
    target: "",
    password: "",
  });
  const {
    cancelPasswordPrompt,
    requestSessionPassword,
    submitPasswordPrompt,
  } = createPasswordPromptController({
    passwordPrompt,
    t: translate,
    notify: showAppMessage,
  });
  const status = ref("idle");
  const errorMessage = ref("");
  const sessionManager = createSessionManager({
    id: () => {
      if (initialExternalWindowLocation && !initialExternalSessionIdUsed) {
        initialExternalSessionIdUsed = true;
        return initialExternalWindowLocation.sessionId;
      }

      return crypto.randomUUID();
    },
  });
  const activeRemoteSession = ref<RemoteSession | null>(null);
  const sessionListTick = ref(0);
  const transportHandle = ref<CoreTransportHandle | null>(null);
  const eventLog = ref<EventLogEntry[]>([]);
  const {
    clearSessionRuntimeState,
    getSessionDisplaySummary,
    getSessionLocalServerView,
    getSessionRemoteDisplayInventory,
    getSessionSelectedRemoteDisplayId,
    getSessionViewportScaleMode,
    recordSessionInputActivity,
    sessionClipboardStateBySessionId,
    sessionDisplayInfoBySessionId,
    sessionLatestInputBySessionId,
    sessionLocalServerViewBySessionId,
    sessionRemoteDisplayInventoryBySessionId,
    sessionSelectedRemoteDisplayIdBySessionId,
    sessionReadonlyBySessionId,
    sessionRuntimeEvidenceBySessionId,
    sessionViewOnlySharedBySessionId,
    setSessionClipboardPolicy,
    setSessionLocalServerView,
    setSessionRemoteDisplayInventory,
    setSessionSelectedRemoteDisplayId,
    setSessionReadonlyState,
    setSessionViewOnlySharedState,
    setSessionViewportScaleMode,
    updateSessionDisplayInfo,
    updateSessionDisplayInfoFromLocalServer,
    updateSessionClipboardActivity,
    updateSessionRuntimeEvidence,
  } = useSessionRuntimeState();
  const {
    confirmDialog,
    requestConfirm,
    resolveConfirm,
  } = useAppConfirmDialog();
  const {
    closeHostEditor,
    confirmHostDraftBeforeSwitch,
    createHostGroup,
    deleteHostRecord,
    deleteSelectedGroup,
    formatHostStorageError,
    hostSavePending,
    loadGroups,
    loadHostResources,
    loadHosts,
    mapPersistedHost,
    persistHostConnectionAttempt,
    renameSelectedGroup,
    resolveSelectedHostPassword,
    saveHost,
    saveTemporaryConnectionHost,
  } = useHostResourceActions({
    discardHostDraftChanges,
    editingHostId,
    filteredHosts,
    groupRenameDraft,
    groupValidationMessage,
    hostCreateVisible,
    hostDraft,
    hostDraftDirty,
    hostDraftSnapshot,
    hostEditorMode,
    hostGroups,
    hostRecords,
    hostStorageMessage,
    invoke,
    isTauriUnavailableError,
    logEvent: logHostEvent,
    newGroupDraft,
    notify: showAppMessage,
    requestConfirm,
    resetGroupEditingState,
    resetHostDraft,
    selectedGroup,
    selectedHostId,
    serializeHostDraft,
    t: translate,
    upsertHostRecord,
  });
  const {
    durationTimer,
    nowTick,
    sessionDuration,
    sessionSeconds,
    stabilityCheckpointRecorded,
    stabilityStatus,
    startDurationTimer,
    stopDurationTimer,
  } = useSessionDuration({
    activeRemoteSession,
    logStabilityCheckpoint: (message) => logEvent("success", t("log.sourceSession"), message),
  });
  const {
    externalWindowDockUnlisteners,
    externalWindowHandles,
    externalWindowInputUnlisteners,
    externalWindowStateUnlisteners,
    externalWindowLocation,
    externalWindowSessionIds,
  } = useSessionExternalWindow();
  const sessionInputFocus = createSessionWindowInputFocus({
    getMainSessionId: () => activeRemoteSession.value?.id ?? null,
    onChange: () => syncSessionInputOwnership(),
  });
  const screenTargets = new Map<string, HTMLElement>();
  const viewportTargets = new Map<string, HTMLElement>();
  const { publishExternalWindowState } = createExternalWindowStateBridge({
    findSession,
    getRemoteViewportStatusHud,
    getRemoteViewportSummary,
    getSessionLocalServerView,
    getSessionRemoteDisplayInventory,
    getSessionSelectedRemoteDisplayId,
    isSessionRecording: (sessionId) => isSessionRecording(sessionId),
    getSessionRecordingPhase: (sessionId) => getSessionRecordingPhase(sessionId),
    getSessionRecordingElapsedSeconds: (sessionId) => getSessionRecordingElapsedSeconds(sessionId),
    getSessionRecordingSpace: (sessionId) => getSessionRecordingSpace(sessionId),
    sessionConnectionRegistry,
    sessionReadonlyBySessionId,
    getSessionTabs: () => sessionTabs.value,
  });
  let activateSessionTabRef: (sessionId: string) => Promise<void> = async () => undefined;
  let refreshLiveDisplaysRef: () => Promise<void> = async () => undefined;
  let connectVncRef: (passwordOverride?: string, hostRecordId?: string, options?: ConnectVncOptions) => Promise<void> = async () => undefined;
  let localShareIntentionalStopUntil = 0;

  function activateSessionTabProxy(sessionId: string) {
    return activateSessionTabRef(sessionId);
  }

  function refreshLiveDisplaysProxy() {
    return refreshLiveDisplaysRef();
  }

  function connectVncProxy(passwordOverride?: string, hostRecordId?: string, options?: ConnectVncOptions) {
    return connectVncRef(passwordOverride, hostRecordId, options);
  }

  let openExternalWindowSessionForFullscreen: ((sessionId: string, fullscreen: boolean) => Promise<void>) | null = null;

  const {
    canRevealSessionArtifactDirectory,
    captureFullscreenExitShortcut,
    chooseFileAccessDirectory,
    chooseBuiltinFileAccessDirectory,
    chooseSessionArtifactDirectory,
    artifactDirectoryOptions,
    fileAccessDirectories,
    fileAccessDirectoryChoosing,
    fileAccessMessage,
    fileAccessMessageTone,
    fullscreenExitShortcutError,
    persistSessionSettings,
    revealFileAccessDirectory,
    revealSessionArtifactDirectory,
    revokeFileAccessDirectory,
    sessionArtifactDirectoryDisplayPath,
    sessionArtifactDirectoryMessage,
    sessionArtifactDirectoryStatusTone,
    sessionSettings,
    sessionSettingsStorage,
    setClipboardSyncMode,
    setConfirmBeforeConnect,
    setConnectionDefault,
    setPostConnectPresentation,
    setFullscreenExitShortcut,
    setSessionArtifactDirectory,
    setFileTransferRenamePolicy,
  } = useSettingsWorkbench({
    t: translate,
    getGlobalKeyboardShortcutRecords,
    notify: showAppMessage,
  });
  const globalKeyboardShortcutRecords = computed<KeyboardShortcutConflict[]>(() => [
    {
      id: "fullscreenExit",
      labelKey: "settings.shortcutNames.fullscreenExit",
      shortcut: sessionSettings.value.fullscreenExitShortcut,
    },
  ]);

  function getGlobalKeyboardShortcutRecords() {
    return globalKeyboardShortcutRecords.value;
  }
  const {
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
  } = useSessionFullscreen({
    activateSessionTab: activateSessionTabProxy,
    openExternalWindowSession: async (sessionId, fullscreen) => {
      await openExternalWindowSessionForFullscreen?.(sessionId, fullscreen);
    },
    sessionSettings,
  });
  const {
    cleanupExternalWindowDockListener,
    dockExternalWindowSession,
    focusExternalWindowSession,
    openExternalWindowSession,
    removeExternalWindowSession,
    dispose: disposeExternalWindows,
    requestDockExternalWindowSession,
    toggleExternalWindowSession,
  } = useSessionExternalWindowCoordinator({
    externalWindowDockUnlisteners,
    externalWindowHandles,
    externalWindowInputUnlisteners,
    externalWindowStateUnlisteners,
    externalWindowLocation,
    externalWindowSessionIds,
    fullscreenSessionId,
    attachRemoteSessionCoreSurface: (request) => remoteSessionCore.attachSurface(request),
    detachRemoteSessionCoreSurface: (sessionId, surfaceId) => remoteSessionCore.detachSurface(sessionId, surfaceId),
    getExternalWindowLocation,
    getProtocolConnection: (sessionId) => sessionConnectionRegistry.get(sessionId)?.connection,
    getWorkspaceRenderSlot,
    recordMainViewportInput,
    refreshLiveDisplays: refreshLiveDisplaysProxy,
    refreshSessionTabs,
    setNativeWindowFullscreen,
    onExternalWindowError: (message) => showAppMessage(message, "error"),
    onExternalWindowFocusChanged: (sessionId, focused) => sessionInputFocus.externalFocusChanged(sessionId, focused),
    publishExternalWindowState,
    stopFullscreenExitShortcutListener,
  });
  openExternalWindowSessionForFullscreen = openExternalWindowSession;
  const {
    closeConnectionConfirm,
    connectionConfirmDraft,
    openHostConnectionConfirm,
    openTemporaryConnectionConfirm,
    startConnectionFromDraft,
    submitConnectionConfirm,
    updateConnectionCredentialStatus,
  } = useConnectionConfirmFlow({
    activeWorkspace,
    connectSelectedHostRecord,
    connectVnc: connectVncProxy,
    host,
    hostRecords,
    password,
    port,
    saveTemporaryConnectionHost,
    sessionSettings,
  });
  const {
    copyLocalVncServerAddress,
    sharingMode, sharingRuntimeError, backgroundShareOverview, sharingServiceOverview,
    resumeLocalShareStart,
    backgroundRestartSupported, backgroundRuntimeOutdated, backgroundPermissionRepairNeeded, restartBackgroundShare, updateAndRepairBackgroundShare,
    localShareStartPhase, localShareStartPending, localShareMissingPermissions, cancelLocalShareStart, localShareLifecycleAction,
    localShareRequiredPermissions, localShareSignatureChangedPermissions, localShareGuidePurpose, requestLocalShareGuidePermission,
    openLocalShareGuideSettings,
    localSharePermissionChecking, localSharePermissionCheckError, inspectLocalSharePermissions, localShareStartChecking,
    windowsService, manageSharingService, localShareServiceSetup, cancelWindowsServiceSetup, requestWindowsServiceInstallation, localShareSetupSavedPassword, localShareSetupPasswordRequired, localShareFallbackActive,
    readLocalShareServerStatus, updateLocalShareServerView,
    sharingAutoStart, sharingAutoStartKnown, sharingAutoStartBusy, setSharingAutoStart, refreshSharingSettings,
    copyLocalVncServerConnectionProfile,
    copyLocalVncServerPort,
    formatLocalVncServerStartError,
    localAgentStatusSummary,
    localVncServerBindAddress,
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
    localVncServerBindAddressInvalid,
    localVncServerPassword,
    localVncServerPasswordInvalid,
    localVncServerPasswordReady,
    localVncServerPort,
    localVncServerPortInvalid,
    localVncServerStartDisabledTitle,
    localVncServerStartInputLocked,
    localVncServerStatusSummary,
    localVncServerToggleLabelKey,
    localVncServerToggleTitle,
    localVncServerViewOnly,
    disconnectLocalVncServerConnection,
    formatLocalVncServerConnectionDuration,
    formatLocalVncServerConnectionTime,
    refreshLocalVncServerStatus,
    refreshLocalShareRuntimeStatus,
    requestLocalVncServerPermission,
    startLocalVncServer,
    stopLocalVncServer: stopLocalVncServerAction,
    toggleLocalVncServer,
  } = useLocalShareWorkbench({
    invoke,
    t: translate,
    logEvent: logSystemEvent,
    isTauriUnavailableError,
    notify: showAppMessage,
    requestConfirm,
    openSharingRepairSettings: openSharingPermissionSettings,
    storage: browserSessionSettingsStorage(),
  });

  const requestedSettingsCategory = ref<"sharingService" | "about" | null>(null);
  const requestedSharingRepair = ref(0);
  function openSharingPermissionSettings() {
    requestedSharingRepair.value++;
    openSharingServiceSettings();
  }
  function openSharingServiceSettings() {
    requestedSettingsCategory.value = "sharingService";
    void selectWorkspace("settings");
  }
  async function openWindowsServiceSetup() {
    await selectLocalShareWorkspace();
    if (activeWorkspace.value === "localShare") await requestWindowsServiceInstallation();
  }

  const {
    cancelGroupEdit,
    openHostCreateForm,
    openHostEditForm,
    selectHostGroup,
    selectHostRecord,
    selectAllHosts,
    selectLocalShareWorkspace,
    selectWorkspace,
    startGroupEdit,
  } = useWorkspaceNavigation({
    activeWorkspace,
    confirmHostDraftBeforeSwitch,
    hostStorageMessage,
    localVncServerOverview,
    openHostCreateDraft,
    openHostEditDraft,
    refreshLocalVncServerStatus,
    resetGroupEditingState,
    selectHostGroupState,
    selectAllHostsState,
    selectedGroup,
    selectedHostId,
    startGroupEditState,
  });
  const {
    hostGroupsCollapsed, hostGroupSearchQuery, visibleHostGroups,
    toggleHostGroups, canManageHostGroup, manageHostGroup,
  } = useHostGroupPanel({
    groups: hostGroupOptions, editingGroupName,
    startGroupEdit, deleteSelectedGroup, cancelGroupEdit,
  }, browserSessionSettingsStorage());
  const liveSessionIds = computed(() => {
    sessionListTick.value;
    return sessionConnectionRegistry.listLiveSessionIds();
  });
  const activeSessionCount = computed(() => liveSessionIds.value.length);
  const failedSessionCount = computed(() => {
    sessionListTick.value;
    return sessionManager.sessions.filter((session) =>
      ["authentication failed", "security failure", "disconnected unexpectedly", "failed"].includes(session.state),
    ).length;
  });
  const trayLocalShareAction = computed<TrayLocalShareAction>(() => {
    const phase = localShareStartPhase.value;
    if (phase === "awaiting_permission" || phase === "awaiting_password") return phase;
    if (localShareLifecycleAction.value !== "idle") return localShareLifecycleAction.value;
    if (phase !== "idle") return "starting";
    if (sharingMode.value === "windowsService") {
      if (sharingServiceOverview.value?.status === "start_pending") return "starting";
      if (sharingServiceOverview.value?.status === "stop_pending") return "stopping";
    }
    return localVncServerLoading.value || windowsService.busy.value ? "busy" : "idle";
  });
  const trayStateSnapshot = computed(() =>
    createTrayStateSnapshot({
      localVncServerOverview: localVncServerOverview.value,
      activeSessionCount: activeSessionCount.value,
      failedSessionCount: failedSessionCount.value,
      canStartLocalShare: localVncServerCanStart.value,
      localShareAction: trayLocalShareAction.value,
    }),
  );
  const {
    refreshLocalShareTray,
    sendTrayNotification,
    startLocalShareFromTray,
    syncTrayState,
  } = createTrayActions({
    invoke: invoke as (command: "update_tray_state", args: Record<string, unknown>) => Promise<unknown>,
    disabled: () => Boolean(externalWindowLocation),
    isTauriUnavailableError,
    localVncServerMessage,
    logEvent,
    refreshLocalVncServerStatus,
    selectLocalShareWorkspace,
    startLocalVncServer,
    t: translate,
    trayStateSnapshot,
  });
  const localVncServerDiagnosticsRefresh = createLocalVncServerDiagnosticsRefresh({
    activeRemoteSession,
    activeWorkspace,
    localVncServerOverview,
    refreshRuntimeStatus: refreshLocalShareRuntimeStatus,
    diagnosticsVisible: sessionDeveloperDiagnosticsVisible,
  });
  const sessionTabs = computed(() => {
    sessionListTick.value;
    return summarizeSessionTabs(sessionManager.sessions, activeRemoteSession.value?.id, liveSessionIds.value);
  });
  const workspaceRenderSlots = computed(() =>
    summarizeWorkspaceRenderSlots(sessionTabs.value, {
      activeSessionId: activeRemoteSession.value?.id,
      fullscreenSessionId: fullscreenSessionId.value,
      externalSessionIds: [...externalWindowSessionIds.value],
    }),
  );
  const activeRemoteSessionCoreSnapshot = computed(() => {
    const activeSessionId = activeRemoteSession.value?.id;
    if (!activeSessionId) {
      return null;
    }

    return remoteSessionCoreSnapshots.value.find((snapshot) => snapshot.sessionId === activeSessionId) ?? null;
  });
  const workspaceSessionTabs = computed(() => workspaceRenderSlots.value.filter((slot) => slot.visible));
  const {
    activeViewportSummary,
    activeWorkspaceRenderSlot,
    diagnosticReport,
    technicalDiagnosticReport,
    sessionDiagnosticsSummary,
  } = useSessionDerivedState({
    activeRemoteSession,
    eventLog,
    getRemoteViewportSummary,
    nowTick,
    t: translate,
    sessionClipboardStateBySessionId,
    sessionLatestInputBySessionId,
    localVncServerOverview,
    sessionLocalServerViewBySessionId,
    sessionRuntimeEvidenceBySessionId,
    remoteSessionCoreSnapshot: activeRemoteSessionCoreSnapshot,
    sessionTabs,
    workspaceRenderSlots,
  });
  const {
    activeSessionArtifacts,
    recordingJobs,
    retryRecordingJob,
    artifactMessage,
    artifactMessageTone,
    copySessionArtifactPath,
    captureSessionScreenshot,
    isSessionRecording,
    getSessionRecordingPhase,
    getSessionRecordingSpace,
    getSessionRecordingElapsedSeconds,
    openSessionArtifactFile,
    revealSessionArtifactFile,
    toggleSessionRecording,
    stopSessionRecording,
  } = useSessionArtifacts({
    manageBackground: !externalWindowLocation,
    chooseRecordingDirectory: async () => {
      const selected = await openDialog({ directory: true, multiple: false });
      return typeof selected === "string" ? selected : null;
    },
    activeRemoteSession,
    sessionTabs,
    artifactDirectory: computed(() => sessionSettings.value.artifactDirectory),
    getDiagnosticSummary: () => technicalDiagnosticReport.value,
    createRecordingSource: (sessionId, signal, onError) => createSessionRecordingSource({ sessionId, core: remoteSessionCore, signal, onError }),
    onRecordingChanged: publishExternalWindowState,
    isArtifactResultVisible: sessionId => {
      if (activeWorkspace.value !== "sessions" || diagnosticsCollapsed.value || fullscreenSessionId.value
        || activeRemoteSession.value?.id !== sessionId || externalWindowLocation || externalWindowSessionIds.value.has(sessionId)
        || document.visibilityState !== "visible" || !document.hasFocus()) return false;
      const row = document.querySelector<HTMLElement>(".session-artifact-list > .session-artifact-row");
      if (!row) return false;
      const bounds = row.getBoundingClientRect();
      if (bounds.width <= 0 || bounds.height <= 0) return false;
      const visibleElement = document.elementFromPoint(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2);
      return visibleElement !== null && row.contains(visibleElement);
    },
    getSession: findSession,
    getViewportTarget: (sessionId) => viewportTargets.get(sessionId),
    invoke: invoke as SessionArtifactsInvoke,
    isTauriUnavailableError,
    logEvent,
    notify: showAppMessage,
    requestDirectoryAuthorization,
    t: translate,
  });
  const {
    copyDiagnosticReport,
  } = useSessionClipboardReports({
    diagnosticReport,
    errorMessage,
    logEvent,
    notify: showAppMessage,
    t: translate,
  });
  const {
    detectLocalVncServerForSession,
    syncLocalEndpointMatchesForSessions,
    updateSessionLocalServerView,
  } = useSessionLocalServerControls({
    errorMessage,
    readServerStatus: readLocalShareServerStatus,
    updateServerView: updateLocalShareServerView,
    localVncServerOverview,
    logEvent,
    sessionConnectionRegistry,
    sessionLocalServerViewBySessionId,
    setSessionLocalServerView,
    t: translate,
    updateSessionDisplayInfoFromLocalServer,
    getSessionTarget: (sessionId) => {
      const session = findSession(sessionId);
      return session ? { host: session.host, port: session.port } : undefined;
    },
    syncSessionViewOnlyShared: (sessionId, viewOnly) => {
      setSessionViewOnlySharedState(sessionId, viewOnly);
      if (viewOnly && !sessionReadonlyBySessionId.value[sessionId]) {
        setSessionReadonlyState(sessionId, true);
        setSessionClipboardPolicy(sessionId, sessionSettings.value.clipboardSyncMode, true);
        sessionConnectionRegistry.get(sessionId)?.connection.setReadonlyMode(true);
        syncSessionInputOwnership();
      }
      publishExternalWindowState(sessionId);
    },
    onAgentChannelChanged: refreshSessionTabs,
  });
  const {
    setSessionReadonlyMode,
  } = useSessionDisplayControls({
    logEvent,
    refreshSessionTabs,
    sessionConnectionRegistry,
    sessionManager,
    getClipboardSyncMode: () => sessionSettings.value.clipboardSyncMode,
    setSessionClipboardPolicy,
    setSessionReadonlyState,
    isSessionViewOnlyShared: (sessionId) => Boolean(sessionViewOnlySharedBySessionId.value[sessionId]),
    onReadonlyChanged: publishExternalWindowState,
    syncSessionInputOwnership,
    t: translate,
  });
  let hostConnectionActions: ReturnType<typeof createHostConnectionActions> | null = null;
  const {
    activateSessionTab,
    cleanupZombieSessionConnections,
    closeSessionTab,
    connectionAllowedSessionIds,
    connectVnc,
    disconnectAllSessionConnections,
    dispose: disposeSessionConnections,
    disconnectSessionConnection,
    disconnectSessionFromViewport,
    prepareSessionForReconnect,
    refreshLiveDisplays,
    retrySessionFromViewport,
    transitionActiveSession,
    transitionSession,
  } = useSessionConnectionLifecycle({
    activeRemoteSession,
    activeWorkspace,
    transportHandle,
    clearSessionConnection,
    registerClipboardSession: async (sessionId, connection, readonly) => {
      await clipboardCoordinator.register(sessionId, {
        sendText: async (text) => { await connection.sendRemoteClipboardText(text); },
      }, readonly);
    },
    receiveRemoteClipboardText: (sessionId, text) => clipboardCoordinator.receiveRemoteText(sessionId, text),
    activateClipboardSession: () => clipboardCoordinator.setActiveSession(sessionInputFocus.sessionId()),
    isMainWindowFocused: () => sessionInputFocus.isMainFocused(),
    onMainSessionChanged: () => sessionInputFocus.mainSessionChanged(),
    clipboardSyncMode: computed(() => sessionSettings.value.clipboardSyncMode),
    enterSessionFullscreen,
    errorMessage,
    exitSessionFullscreen,
    findSession,
    fullscreenSessionId,
    host,
    logEvent,
    notify: showAppMessage,
    onSessionClosed: (sessionId) => sessionFileTransferManager.removeSessionTasks(sessionId),
    onSessionStateChanged: publishExternalWindowState,
    onServerRuntimeChanged: (sessionId) => {
      refreshSessionTabs();
      publishExternalWindowState(sessionId);
    },
    onSessionInputPolicyChanged: publishExternalWindowState,
    openExternalWindowSession,
    password,
    startRemoteSessionCore: (request) => remoteSessionCore.startSession(request),
    updateRemoteSessionCore: (request) => remoteSessionCore.updateSession(request),
    stopRemoteSessionCore: (sessionId) => remoteSessionCore.stopSession(sessionId),
    attachRemoteSessionCoreSurface: (request) => remoteSessionCore.attachSurface(request),
    persistHostConnectionAttempt,
    port,
    prepareLocalVncServerForSession: detectLocalVncServerForSession,
    refreshLocalVncServerStatus: refreshLocalShareRuntimeStatus,
    refreshSessionTabs,
    refreshVisibleSessionIds: visibleWorkspaceSessionIds,
    requestPasswordForSession: (target, signal) => requestSessionPassword(target, "", signal),
    screenTargets,
    sessionConnectionRegistry,
    sessionManager,
    defaultQualityProfile: computed(() => sessionSettings.value.defaultQualityProfile),
    setSessionClipboardPolicy,
    setSessionReadonlyState,
    setSessionViewOnlySharedState,
    setSessionRemoteDisplayInventory,
    startDurationTimer,
    status,
    t: translate,
    updateSessionClipboardActivity,
    updateSessionDisplayInfo,
    updateSessionRuntimeEvidence,
    vncAdapter,
  });
  activateSessionTabRef = activateSessionTab;
  refreshLiveDisplaysRef = refreshLiveDisplays;
  connectVncRef = connectVnc;
  hostConnectionActions = createHostConnectionActions({
    activeWorkspace,
    connectVnc,
    host,
    openHostConnectionConfirm,
    openTemporaryConnectionConfirm,
    password,
    port,
    requestSessionPassword,
    resolveSelectedHostPassword,
    sessionSettings,
    showAppMessage,
    t: translate,
  });
  function logEvent(level: EventLogLevel, source: string, message: string, context: DiagnosticEventContext = {}) {
    const mergedContext = {
      ...inferDiagnosticEventContext(),
      ...context,
    };
    const performanceSample = source === "rfb-performance" || source === "surface-performance";
    const performanceSummary = performanceSample ? summarizePerformanceLog(source, message) : undefined;
    eventLog.value = appendEventLog(eventLog.value, { level, source, message, ...mergedContext }, { limit: 300 });
    runtimeFileLogger.emit(
      level,
      `frontend.${mergedContext.origin ?? "conversation"}`,
      context.eventKey ?? (performanceSample ? "performance_sample" : "activity"),
      message,
      {
        source,
        target: mergedContext.target,
        sessionId: mergedContext.conversationId ?? undefined,
        summary: mergedContext.userMessage
          ?? performanceSummary
          ?? (source === "connection-model" ? "连接能力状态已更新" : undefined),
      },
    );
  }

  function logHostEvent(level: EventLogLevel, source: string, message: string, context: DiagnosticEventContext = {}) {
    logEvent(level, source, message, { origin: "host", ...context });
  }

  function logSystemEvent(level: EventLogLevel, source: string, message: string, context: DiagnosticEventContext = {}) {
    logEvent(level, source, message, { origin: "system", ...context });
  }

  function inferDiagnosticEventContext(): DiagnosticEventContext {
    const activeSession = activeRemoteSession.value;
    return {
      origin: "conversation",
      target: activeSession?.target ?? `${host.value}:${port.value}`,
      conversationId: activeSession?.id,
      conversationName: activeSession?.target,
    };
  }

  function refreshSessionTabs() {
    sessionListTick.value += 1;
    syncSessionInputOwnership();
  }

  function syncSessionInputOwnership() {
    const activeSessionId = sessionInputFocus.sessionId();
    for (const sessionId of sessionConnectionRegistry.listLiveSessionIds()) {
      sessionConnectionRegistry.get(sessionId)?.connection.setInputEnabled(sessionId === activeSessionId && !sessionReadonlyBySessionId.value[sessionId]);
    }
    void clipboardCoordinator.setActiveSession(activeSessionId);
  }

  function claimMainWindowInput() {
    sessionInputFocus.mainFocusChanged(true);
  }

  function setMainWindowFocused(focused: boolean) {
    sessionInputFocus.mainFocusChanged(focused);
  }

  watch(() => sessionSettings.value.clipboardSyncMode, (mode) => {
    for (const sessionId of sessionConnectionRegistry.listLiveSessionIds()) {
      const connection = sessionConnectionRegistry.get(sessionId)?.connection;
      if (!connection) {
        continue;
      }

      connection.setClipboardSyncMode(mode);
      setSessionClipboardPolicy(sessionId, mode, sessionReadonlyBySessionId.value[sessionId] ?? false);
      logEvent("info", translate("log.sourceSession"), `Clipboard sync mode applied to live session ${sessionId}; mode=${mode}.`);
    }
  });

  function getWorkspaceRenderSlot(sessionId: string): SessionWorkspaceRenderSlot {
    const slot = workspaceRenderSlots.value.find((item) => item.id === sessionId);
    if (slot) {
      return slot;
    }

    return {
      ...createFallbackWorkspaceRenderSlot(sessionId),
    };
  }

  function visibleWorkspaceSessionIds(): Set<string> {
    return new Set(workspaceRenderSlots.value.filter((slot) => slot.visible).map((slot) => slot.id));
  }

  function recordMainViewportInput(
    sessionId: string,
    type: SessionDiagnosticsInputActivity["type"],
  ) {
    const slot = getWorkspaceRenderSlot(sessionId);
    if (!slot.active || !slot.visible || sessionReadonlyBySessionId.value[sessionId]) {
      return;
    }

    recordSessionInputActivity(sessionId, "main", type);
  }

  async function requestDirectoryAuthorization(directory: string): Promise<boolean> {
    const choice = await requestConfirm({
      title: translate("sessions.artifacts.authorizationTitle"),
      message: translate("sessions.artifacts.authorizationMessage", { directory }),
      primaryLabel: translate("sessions.artifacts.authorizationAllow"),
      secondaryLabel: translate("sessions.artifacts.authorizationDeny"),
    });
    return choice === "primary";
  }

  function applyHostToConnection(hostRecord: HostRecord) {
    hostConnectionActions?.applyHostToConnection(hostRecord);
  }

  const fillSelectedHostConnectionForm = async (hostRecord: HostRecord) => hostConnectionActions?.fillSelectedHostConnectionForm(hostRecord);
  const connectSelectedHost = async () => hostConnectionActions?.connectSelectedHost(selectedHost.value);

  async function connectSelectedHostRecord(hostRecord: HostRecord, options: ConnectVncOptions = {}, passwordOverride?: string) {
    await hostConnectionActions?.connectSelectedHostRecord(hostRecord, options, passwordOverride);
  }

  const setScreenTarget = (sessionId: string, element: Element | null) => setElementTarget(screenTargets, sessionId, element);
  const setViewportTarget = (sessionId: string, element: Element | null) => setElementTarget(viewportTargets, sessionId, element);

  function findSession(sessionId: string): RemoteSession | undefined {
    return sessionManager.sessions.find((session) => session.id === sessionId);
  }

  function getExternalWindowLocation(sessionId: string) {
    const session = findSession(sessionId);
    if (!session) {
      return null;
    }

    return {
      sessionId,
      target: session.target,
      host: session.host,
      port: session.port,
      readonly: Boolean(sessionReadonlyBySessionId.value[sessionId]),
    };
  }

  function getRemoteViewportSummary(tab: RemoteSessionTab): RemoteViewportSummary {
    const session = findSession(tab.id);
    const connection = sessionConnectionRegistry.get(tab.id)?.connection;
    return summarizeRemoteViewport(tab, session
      ? {
        ...session,
        connectionModel: connection?.connectionModel,
        displaySource: connection?.displaySource,
        inputSource: connection?.inputSource,
        readonly: sessionReadonlyBySessionId.value[tab.id] ?? false,
        viewOnlyShared: sessionViewOnlySharedBySessionId.value[tab.id] ?? false,
      }
      : undefined);
  }

  function getRemoteViewportStatusHud(tab: RemoteSessionTab): RemoteViewportStatusHudSummary {
    const hud = summarizeRemoteViewportStatusHud({
      viewport: getRemoteViewportSummary(tab),
      session: findSession(tab.id),
      events: eventLog.value,
      nowMs: nowTick.value,
      display: getSessionDisplaySummary(tab.id),
    });
    const runtime = sessionConnectionRegistry.get(tab.id)?.connection.getServerRuntimeState?.();
    return { ...hud, runtimeDesktop: runtime?.desktop, runtimeRecoveryId: runtime?.recoveryId };
  }

  const keyboardMappers = new Map<string, ReturnType<typeof createRemoteKeyboardMapper>>();

  function releaseSessionKeyboardInput(sessionId: string) {
    sessionConnectionRegistry.get(sessionId)?.connection.releaseRemoteInput?.();
    keyboardMappers.get(sessionId)?.clear();
    keyboardMappers.delete(sessionId);
  }

  function handleSessionViewportKeyboardEvent(sessionId: string, event: KeyboardEvent, down: boolean, options: { remotePriority?: boolean } = {}) {
    const session = findSession(sessionId);
    const slot = getWorkspaceRenderSlot(sessionId);

    if (!session) {
      return;
    }

    const shouldCapture = shouldCaptureSessionKeyboardEvent({
      readonly: Boolean(sessionReadonlyBySessionId.value[sessionId]),
      active: slot.active,
      visible: slot.visible,
      editableTarget: isKeyboardEditableTarget(event.target),
      reservedShortcut: !options.remotePriority && fullscreenExitShortcutMatches(event),
    });

    if (!shouldCapture) {
      return;
    }

    let mapper = keyboardMappers.get(sessionId);
    if (!mapper) { mapper = createRemoteKeyboardMapper(); keyboardMappers.set(sessionId, mapper); }
    const remoteKeyEvent = mapper.map(event, down, {
      rightControlAsCommand: detectKeyboardShortcutDisplayPlatform() === "windows"
        && sessionKeyboardPreferences(session.target).rightControlAsCommand,
    });
    if (!remoteKeyEvent) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    claimMainWindowInput();
    if (down) {
      recordMainViewportInput(sessionId, "keyboard");
    }
    sessionConnectionRegistry.get(sessionId)?.connection.sendRemoteKeyEvent({
      ...remoteKeyEvent,
      down,
    });
  }

  const openGlobalTransferDrawer = (sessionId: string | null = null) => {
    globalTransferDrawerSessionId.value = sessionId;
    globalTransferDrawerOpen.value = true;
    void getCurrentWindow().show()
      .then(() => getCurrentWindow().unminimize())
      .then(() => getCurrentWindow().setFocus())
      .catch(() => undefined);
  };
  const closeGlobalTransferDrawer = () => { globalTransferDrawerOpen.value = false; };

  async function clearSessionConnection(sessionId: string): Promise<void> {
    releaseSessionKeyboardInput(sessionId);
    await stopSessionRecording(sessionId);
    const closingWindow = removeExternalWindowSession(sessionId);
    clipboardCoordinator.unregister(sessionId);
    sessionConnectionRegistry.detach(sessionId);
    clearSessionRuntimeState(sessionId);

    screenTargets.get(sessionId)?.replaceChildren();

    if (!sessionConnectionRegistry.listLiveSessionIds().length) {
      stopDurationTimer();
    }

    refreshSessionTabs();
    await closingWindow;
  }

  async function stopLocalVncServer() {
    localShareIntentionalStopUntil = Date.now() + 5_000;
    await stopLocalVncServerAction();
  }

  watch(
    () => workspaceSessionTabs.value.map((tab) => tab.id).join(","),
    () => {
      void refreshLiveDisplays();
    },
  );

  watch(localVncServerOverview, (overview) => {
    if (!overview) {
      return;
    }

    syncLocalEndpointMatchesForSessions(overview);
  });

  watch([sessionLocalServerViewBySessionId, sessionRemoteDisplayInventoryBySessionId, sessionSelectedRemoteDisplayIdBySessionId], () => {
    for (const sessionId of externalWindowSessionIds.value) publishExternalWindowState(sessionId);
  });

  watch(
    () => [
      activeWorkspace.value,
      sessionDeveloperDiagnosticsVisible.value,
      activeRemoteSession.value?.id ?? "",
      activeRemoteSession.value?.state ?? "",
      activeRemoteSession.value?.host ?? "",
      activeRemoteSession.value?.port ?? 0,
      localVncServerOverview.value?.status ?? "",
      localVncServerOverview.value?.port ?? 0,
    ],
    localVncServerDiagnosticsRefresh.sync,
  );

  watch(activeWorkspace, (workspace) => {
    if (workspace !== "sessions") {
      return;
    }

    void refreshLiveDisplays();
    window.setTimeout(() => {
      void refreshLiveDisplays();
    }, 250);
  });
  const vncAppLifecycle = createVncAppLifecycle({
    activeWorkspace,
    resumeLocalShareStart,
    copyLocalVncServerConnectionProfile,
    disconnectAllSessionConnections: disposeSessionConnections,
    disposeExternalWindows,
    disconnectLocalVncServerConnection: (connection) => disconnectLocalVncServerConnection(connection as Parameters<typeof disconnectLocalVncServerConnection>[0]),
    dockExternalWindowSession,
    externalWindowDockUnlisteners,
    externalWindowInputUnlisteners,
    externalWindowStateUnlisteners,
    externalWindowLocation,
    host,
    loadHostResources,
    localVncServerOverview,
    openAboutSettings: async () => {
      await selectWorkspace("settings");
      if (activeWorkspace.value === "settings") requestedSettingsCategory.value = "about";
    },
    port,
    refreshLocalShareTray,
    refreshRemoteSessionCoreSnapshots,
    selectLocalShareWorkspace,
    selectWorkspace: (workspace) => selectWorkspace(workspace),
    startLocalShareFromTray,
    stopDurationTimer,
    stopFullscreenExitShortcutListener,
    stopLocalVncServer,
    syncFullscreenState,
    syncTrayState,
  });

  onBeforeUnmount(() => {
    localVncServerDiagnosticsRefresh.stop();
    vncAppLifecycle.cleanup();
  });

  onMounted(vncAppLifecycle.mount);

  watch(trayStateSnapshot, (next, previous) => {
    const notifications = getTrayNotifications(previous ?? null, next, {
      intentionalStop: Date.now() <= localShareIntentionalStopUntil,
    });
    for (const notification of notifications) {
      void sendTrayNotification(notification.title, notification.body);
    }
    void syncTrayState();
  }, { deep: true });

  return {
    activateSessionTab,
    openSessionMessageDialog,
    sessionMessageDialogRequest,
    activeHostScopeLabel,
    activeRemoteSession,
    activeSessionArtifacts,
    recordingJobs,
    retryRecordingJob,
    activeViewportSummary,
    activeWorkspace,
    activeWorkspaceRenderSlot,
    applyHostToConnection,
    transportHandle,
    cancelGroupEdit,
    cancelPasswordPrompt,
    canCreateGroup,
    canSaveGroupRename,
    captureFullscreenExitShortcut,
    claimMainWindowInput,
    setMainWindowFocused,
    captureSessionScreenshot,
    checkboxValue,
    chooseFileAccessDirectory,
    chooseBuiltinFileAccessDirectory,
    chooseSessionArtifactDirectory,
    fileAccessDirectoryChoosing,
    cleanupExternalWindowDockListener,
    cleanupZombieSessionConnections,
    clearSessionConnection,
    closeConnectionConfirm,
    closeGlobalTransferDrawer,
    closeHostEditor,
    closeSessionTab,
    confirmDialog,
    confirmHostDraftBeforeSwitch,
    connectionAllowedSessionIds,
    connectionConfirmDraft,
    connectSelectedHost,
    connectSelectedHostRecord,
    connectVnc,
    copyDiagnosticReport,
    copyLocalVncServerAddress,
    copyLocalVncServerConnectionProfile,
    copyLocalVncServerPort,
    disconnectLocalVncServerConnection,
    formatLocalVncServerConnectionDuration,
    formatLocalVncServerConnectionTime,
    createHostGroup,
    deleteHostRecord,
    deleteSelectedGroup,
    detectLocalVncServerForSession,
    diagnosticReport,
    diagnosticsCollapsed,
    sessionDeveloperDiagnosticsVisible,
    discardHostDraftChanges,
    disconnectAllSessionConnections,
    disconnectSessionConnection,
    disconnectSessionFromViewport,
    dockExternalWindowSession,
    durationTimer,
    editingGroupName,
    editingHostId,
    errorMessage,
    eventLog,
    fileAccessDirectories,
    enqueueSessionFileUploads,
    sessionFileTransferManager,
    fileAccessMessage,
    fileAccessMessageTone,
    exitSessionFullscreen,
    externalWindowDockUnlisteners,
    externalWindowHandles,
    externalWindowLocation,
    externalWindowSessionIds,
    fillSelectedHostConnectionForm,
    filteredHosts,
    findSession,
    focusExternalWindowSession,
    formatHostDateTime,
    formatHostStorageError,
    formatLocalVncServerStartError,
    fullscreenExitShortcutError,
    fullscreenExitShortcutListening,
    fullscreenExitShortcutMatches,
    fullscreenSessionId,
    globalTransferDrawerOpen,
    globalTransferDrawerSessionId,
    getRemoteViewportStatusHud,
    getRemoteViewportSummary,
    getSessionDisplaySummary,
    getSessionLocalServerView,
    getSessionRemoteDisplayInventory,
    getSessionSelectedRemoteDisplayId,
    getWorkspaceRenderSlot,
    globalKeyboardShortcutRecords,
    groupRenameDraft,
    groupValidationMessage,
    handleSessionViewportKeyboardEvent,
    releaseSessionKeyboardInput,
    handleGlobalSessionKeydown,
    hostSavePending,
    host,
    HOST_GROUP_NAME_MAX_LENGTH,
    hostCreateVisible,
    hostCredentialMethod,
    hostCredentialPlaceholder,
    hostDraft,
    hostDraftDirty,
    hostDraftSnapshot,
    hostEditorMode,
    hostGroupOptions,
    hostGroups,
    hostSearchQuery,
    hostStorageMessage,
    hostSummary,
    isEditableKeyTarget,
    isSessionRecording,
    getSessionRecordingPhase,
    getSessionRecordingSpace,
    getSessionRecordingElapsedSeconds,
    isTauriUnavailableError,
    liveSessionIds,
    loadGroups,
    loadHostResources,
    loadHosts,
    appMessageEvent,
    canRevealSessionArtifactDirectory,
    artifactDirectoryOptions,
    artifactMessage,
    artifactMessageTone,
    localAgentStatusSummary,
    sharingMode, sharingRuntimeError, backgroundShareOverview, sharingServiceOverview,
    backgroundRestartSupported, backgroundRuntimeOutdated, backgroundPermissionRepairNeeded, restartBackgroundShare, updateAndRepairBackgroundShare,
    localShareStartPhase, localShareStartPending, localShareMissingPermissions, cancelLocalShareStart,
    localShareRequiredPermissions, localShareSignatureChangedPermissions, localShareGuidePurpose, requestLocalShareGuidePermission,
    openLocalShareGuideSettings,
    resumeLocalShareStart,
    localSharePermissionChecking, localSharePermissionCheckError, inspectLocalSharePermissions, localShareStartChecking,
    windowsService, manageSharingService, localShareServiceSetup, cancelWindowsServiceSetup, requestWindowsServiceInstallation, localShareSetupSavedPassword, localShareSetupPasswordRequired, localShareFallbackActive, openWindowsServiceSetup,
    readLocalShareServerStatus, updateLocalShareServerView,
    requestedSettingsCategory, openSharingServiceSettings, requestedSharingRepair, openSharingPermissionSettings,
    sharingAutoStart, sharingAutoStartKnown, sharingAutoStartBusy, setSharingAutoStart, refreshSharingSettings,
    localVncServerBindAddress,
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
    localVncServerBindAddressInvalid,
    localVncServerPassword,
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
    logEvent,
    mapPersistedHost,
    newGroupDraft,
    newGroupValidation,
    nowTick,
    openExternalWindowSession,
    openGlobalTransferDrawer,
    openHostConnectionConfirm,
    openHostCreateForm,
    openHostEditForm,
    openTemporaryConnectionConfirm,
    password,
    passwordPrompt,
    persistHostConnectionAttempt,
    persistSessionSettings,
    port,
    prepareSessionForReconnect,
    remoteSessionCoreSnapshots,
    recordMainViewportInput,
    recordSessionInputActivity,
    refreshLiveDisplays,
    refreshRemoteSessionCoreSnapshots,
    refreshLocalVncServerStatus,
    refreshSessionTabs,
    revealFileAccessDirectory,
    revealSessionArtifactDirectory,
    removeExternalWindowSession,
    renameGroupValidation,
    renameSelectedGroup,
    requestConfirm,
    requestDockExternalWindowSession,
    requestLocalVncServerPermission,
    resetHostDraft,
    resolveConfirm,
    resolveSelectedHostPassword,
    retrySessionFromViewport,
    revokeFileAccessDirectory,
    saveHost,
    saveTemporaryConnectionHost,
    screenTargets,
    selectedGroup,
    selectedGroupIsDefault,
    selectedHost,
    selectedHostId,
    copySessionArtifactPath,
    selectHostGroup,
    selectHostRecord,
    selectAllHosts,
    selectLocalShareWorkspace,
    selectWorkspace,
    serializeHostDraft,
    sessionConnectionRegistry,
    sessionArtifactDirectoryDisplayPath,
    sessionArtifactDirectoryMessage,
    sessionArtifactDirectoryStatusTone,
    sessionDiagnosticsSummary,
    sessionDisplayInfoBySessionId,
    sessionDuration,
    sessionLatestInputBySessionId,
    sessionListTick,
    sessionLocalServerViewBySessionId,
    sessionManager,
    openSessionArtifactFile,
    sessionReadonlyBySessionId,
    sessionRuntimeEvidenceBySessionId,
    sessionSeconds,
    sessionSettings,
    sessionSettingsStorage,
    sessionTabs,
    revealSessionArtifactFile,
    setClipboardSyncMode,
    setConfirmBeforeConnect,
    setConnectionDefault,
    setPostConnectPresentation,
    setFullscreenExitShortcut,
    setNativeWindowFullscreen,
    setSessionArtifactDirectory,
    setFileTransferRenamePolicy,
    setScreenTarget,
    setSessionReadonlyMode,
    setSessionViewportScaleMode,
    setViewportTarget,
    showAppMessage,
    hostGroupsCollapsed, hostGroupSearchQuery, visibleHostGroups,
    toggleHostGroups, canManageHostGroup, manageHostGroup,
    stabilityCheckpointRecorded,
    stabilityStatus,
    startConnectionFromDraft,
    startDurationTimer,
    startFullscreenExitShortcutListener,
    startGroupEdit,
    submitPasswordPrompt,
    startLocalVncServer,
    status,
    stopDurationTimer,
    stopFullscreenExitShortcutListener,
    stopLocalVncServer,
    submitConnectionConfirm,
    syncFullscreenState,
    syncSessionInputOwnership,
    t,
    toggleExternalWindowSession,
    toggleLocalVncServer,
    toggleSessionRecording,
    toggleSessionFullscreen,
    transitionActiveSession,
    transitionSession,
    updateConnectionCredentialStatus,
    updateSessionDisplayInfo,
    updateSessionDisplayInfoFromLocalServer,
    updateSessionLocalServerView,
    setSessionRemoteDisplayInventory,
    setSessionSelectedRemoteDisplayId,
    getSessionViewportScaleMode,
    updateSessionRuntimeEvidence,
    viewportTargets,
    visibleWorkspaceSessionIds,
    vncAdapter,
    workspaceRenderSlots,
    workspaceSessionTabs,
  };
}

export type VncAppState = ReturnType<typeof useVncAppState>;
