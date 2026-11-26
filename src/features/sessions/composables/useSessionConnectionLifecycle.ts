import { nextTick, type Ref } from "vue";
import { readNativeRfbFailure } from "../../../session/protocol/rfbFailure.ts";
import type { ConnectVncOptions, CoreTransportHandle, Translate, WorkspaceMode } from "@app/types";
import type { DiagnosticEventContext, EventLogLevel } from "@diagnostics/eventLog";
import {
  classifyConnectionFailure,
  createConnectionIssue,
  createDisconnectIssue,
  type ConnectionIssue,
} from "../../../session/diagnostics/sessionDiagnostics.ts";
import type { SessionConnectionRegistry } from "@session/core/sessionConnectionRegistry";
import { shouldCleanupConnectionForSessionState } from "../../../session/core/sessionConnectionCleanup.ts";
import { createInitialConnectionModel } from "../../../session/connection-model/connectionModel.ts";
import type { LocalVncServerSessionPreparation } from "./useSessionLocalServerControls.ts";
import type {
  RemoteSession,
  RemoteSessionTransitionMetadata,
  RemoteSessionState,
  SessionManager,
} from "@session/core/sessionManager";
import type {
  ProtocolAdapter,
  ProtocolConnection,
} from "@session/protocol/protocolAdapter";
import type { SessionQualityProfile, SessionDisplayInfo } from "@session/viewport/sessionDisplay";
import type { ClipboardSyncMode } from "@session/settings/sessionSettings";
import type {
  RemoteSessionSnapshot,
  RemoteSessionSurfaceSnapshot,
  RemoteSessionStartRequest,
  RemoteSessionUpdateRequest,
  SessionSurfaceAttachRequest,
} from "@session/protocol/remoteSessionCore";
import { createSessionSurfaceId } from "../../../session/viewport/sessionSurface.ts";
import type { ViewportRuntimeEvidence } from "@session/diagnostics/runtimeEvidence";
import type { SessionClipboardActivity } from "@session/clipboard/sessionClipboardActivity";
import { logSessionTransportResolution } from "./sessionConnectionLifecycle/coreSync.ts";
import {
  createConnectionIssueHandler,
  createRawConnectionErrorHandler,
  shouldIgnoreStatusAfterConnectionIssue,
  type LocalizedConnectionIssue,
} from "./sessionConnectionLifecycle/connectionIssues.ts";
import { createSessionDisplayEvents } from "./sessionConnectionLifecycle/sessionDisplayEvents.ts";
import { createSessionDisconnector, createSessionUserDisconnector } from "./sessionConnectionLifecycle/disconnect.ts";
import { createSessionConnectAttempts } from "./sessionConnectionLifecycle/connectAttempts.ts";
type SessionConnectionLifecycleOptions = {
  activeRemoteSession: Ref<RemoteSession | null>;
  activeWorkspace: Ref<WorkspaceMode>;
  transportHandle: Ref<CoreTransportHandle | null>;
  clearSessionConnection: (sessionId: string) => Promise<void>;
  registerClipboardSession?: (sessionId: string, connection: ProtocolConnection, readonly: boolean) => Promise<void>;
  receiveRemoteClipboardText?: (sessionId: string, text: string) => Promise<boolean>;
  activateClipboardSession?: (sessionId: string | null) => Promise<void>;
  isMainWindowFocused?: () => boolean;
  onMainSessionChanged?: () => void;
  clipboardSyncMode: Ref<ClipboardSyncMode>;
  enterSessionFullscreen: (sessionId: string) => Promise<void>;
  errorMessage: Ref<string>;
  exitSessionFullscreen: () => Promise<void>;
  findSession: (sessionId: string) => RemoteSession | undefined;
  fullscreenSessionId: Ref<string | null>;
  host: Ref<string>;
  logEvent: (level: EventLogLevel, source: string, message: string, context?: DiagnosticEventContext) => void;
  notify: (content: string, tone: "error") => void;
  onSessionConnected?: (sessionId: string) => void;
  onServerRuntimeChanged?: (sessionId: string) => void;
  onSessionClosed?: (sessionId: string) => void;
  onSessionStateChanged?: (sessionId: string) => void;
  openExternalWindowSession?: (sessionId: string) => Promise<void>;
  password: Ref<string>;
  startRemoteSessionCore?: (request: RemoteSessionStartRequest) => Promise<RemoteSessionSnapshot>;
  updateRemoteSessionCore?: (request: RemoteSessionUpdateRequest) => Promise<RemoteSessionSnapshot>;
  stopRemoteSessionCore?: (sessionId: string) => Promise<void>;
  attachRemoteSessionCoreSurface?: (request: SessionSurfaceAttachRequest) => Promise<RemoteSessionSurfaceSnapshot>;
  persistHostConnectionAttempt: (hostId: string | undefined) => Promise<void>;
  port: Ref<number>;
  prepareLocalVncServerForSession: (sessionId: string, targetHost: string, targetPort: number) => Promise<LocalVncServerSessionPreparation | undefined>;
  refreshLocalVncServerStatus: () => Promise<void>;
  refreshSessionTabs: () => void;
  refreshVisibleSessionIds: () => Set<string>;
  requestPasswordForSession: (target: string, signal?: AbortSignal) => Promise<string | undefined>;
  screenTargets: Map<string, HTMLElement>;
  sessionConnectionRegistry: SessionConnectionRegistry;
  sessionManager: SessionManager;
  defaultQualityProfile: Ref<SessionQualityProfile>;
  setSessionReadonlyState: (sessionId: string, readonly: boolean) => void;
  setSessionViewOnlySharedState: (sessionId: string, viewOnlyShared: boolean) => void;
  onSessionInputPolicyChanged?: (sessionId: string) => void;
  setSessionClipboardPolicy: (sessionId: string, mode: ClipboardSyncMode, readonly: boolean) => void;
  updateSessionClipboardActivity: (sessionId: string, activity: SessionClipboardActivity) => void;
  setSessionRemoteDisplayInventory: Parameters<typeof createSessionDisplayEvents>[0]["setSessionRemoteDisplayInventory"];
  startDurationTimer: () => void;
  status: Ref<string>;
  t: Translate;
  updateSessionDisplayInfo: (sessionId: string, info: SessionDisplayInfo) => void;
  updateSessionRuntimeEvidence: (sessionId: string, evidence: ViewportRuntimeEvidence) => void;
  vncAdapter: ProtocolAdapter;
};
export function useSessionConnectionLifecycle(options: SessionConnectionLifecycleOptions) {
  const {
    activeRemoteSession,
    activeWorkspace,
    transportHandle,
    clearSessionConnection,
    registerClipboardSession,
    receiveRemoteClipboardText,
    activateClipboardSession,
    isMainWindowFocused,
    onMainSessionChanged,
    clipboardSyncMode,
    enterSessionFullscreen,
    errorMessage,
    exitSessionFullscreen,
    findSession,
    fullscreenSessionId,
    host,
    logEvent,
    notify,
    onSessionConnected,
    onSessionClosed,
    openExternalWindowSession,
    password,
    startRemoteSessionCore,
    updateRemoteSessionCore,
    stopRemoteSessionCore,
    attachRemoteSessionCoreSurface,
    persistHostConnectionAttempt,
    port,
    prepareLocalVncServerForSession,
    refreshLocalVncServerStatus,
    refreshSessionTabs,
    refreshVisibleSessionIds,
    requestPasswordForSession,
    screenTargets,
    sessionConnectionRegistry,
    sessionManager,
    defaultQualityProfile,
    setSessionReadonlyState,
    setSessionViewOnlySharedState,
    setSessionClipboardPolicy,
    updateSessionClipboardActivity,
    setSessionRemoteDisplayInventory,
    startDurationTimer,
    status,
    t,
    updateSessionDisplayInfo,
    updateSessionRuntimeEvidence,
    vncAdapter,
  } = options;
  const hostRecordIdBySessionId = new Map<string, string | undefined>();
  const closingSessions = new Set<string>();
  const sessionDisplayEvents = createSessionDisplayEvents({
    logEvent,
    refreshSessionTabs,
    sessionManager,
    setSessionRemoteDisplayInventory,
    t,
  });
  const applyConnectionIssue = createConnectionIssueHandler({
    errorMessage,
    findSession,
    localizeConnectionIssue: localizedConnectionIssue,
    logEvent,
    notify,
    sessionManager,
    syncSessionAfterIssue,
    t,
  });
  const applyRawConnectionError = createRawConnectionErrorHandler(applyConnectionIssue);
  function localizedConnectionFailureMessage(classification: ReturnType<typeof classifyConnectionFailure>) {
    return t(classification.messageKey);
  }
  function localizedConnectionIssue(issue: ConnectionIssue): LocalizedConnectionIssue {
    const message = t(issue.messageKey);
    return {
      ...issue,
      message,
    };
  }
  function transitionActiveSession(nextState: RemoteSessionState) {
    if (!activeRemoteSession.value) {
      status.value = nextState;
      return;
    }
    transitionSession(activeRemoteSession.value.id, nextState);
  }
  function transitionSession(sessionId: string, nextState: RemoteSessionState, metadata: RemoteSessionTransitionMetadata = {}) {
    const session = sessionManager.transition(sessionId, nextState, {
      ...metadata,
      issue: metadata.issue ? localizedConnectionIssue(metadata.issue) : undefined,
    });
    if (!session) {
      return;
    }
    if (activeRemoteSession.value?.id === sessionId) {
      activeRemoteSession.value = session;
      status.value = session.state;
      transportHandle.value = session.transport ?? null;
    }
    refreshSessionTabs();
    options.onSessionStateChanged?.(sessionId);
  }
  function syncSessionAfterIssue(sessionId: string, session: RemoteSession | undefined) {
    if (!session) {
      return;
    }
    if (activeRemoteSession.value?.id === sessionId) {
      activeRemoteSession.value = session;
      status.value = session.state;
      transportHandle.value = session.transport ?? null;
    }
    refreshSessionTabs();
    options.onSessionStateChanged?.(sessionId);
  }
  function connectionAllowedSessionIds() {
    return sessionManager.sessions
      .filter((session) => !shouldCleanupConnectionForSessionState(session.state))
      .map((session) => session.id);
  }
  async function cleanupZombieSessionConnections(reason: string) {
    const zombieConnections = sessionConnectionRegistry.detachZombieConnections(connectionAllowedSessionIds());
    for (const zombie of zombieConnections) {
      await disconnectSessionConnection(zombie.sessionId);
      await vncAdapter.disconnect(zombie.connection);
      logEvent(
        "warning",
        t("log.sourceSession"),
        `Cleaned zombie connection for ${findSession(zombie.sessionId)?.target ?? zombie.sessionId}; reason=${reason}.`,
      );
    }
  }
  async function refreshLiveDisplays() {
    await cleanupZombieSessionConnections("refresh_live_displays");
    await nextTick();
    const visibleSessionIds = refreshVisibleSessionIds();
    for (const sessionId of sessionConnectionRegistry.listLiveSessionIds()) {
      const connection = sessionConnectionRegistry.get(sessionId)?.connection;
      if (!connection) {
        continue;
      }
      if (visibleSessionIds.has(sessionId)) {
        connection.refreshDisplay();
        const displayInfo = connection.getDisplayInfo();
        if (displayInfo) {
          updateSessionDisplayInfo(sessionId, displayInfo);
        }
      }
      const runtimeEvidence = connection.getRuntimeEvidence();
      if (runtimeEvidence) {
        updateSessionRuntimeEvidence(sessionId, runtimeEvidence);
      }
    }
  }
  const disconnectOwnedSession = createSessionDisconnector({
    getConnection: (sessionId) => sessionConnectionRegistry.get(sessionId)?.connection,
    clearConnection: clearSessionConnection,
    disconnectConnection: (connection, sessionId) => vncAdapter.disconnect(connection).catch((error) => {
      const failure = classifyConnectionFailure(readNativeRfbFailure(error) ?? String(error));
      errorMessage.value = localizedConnectionFailureMessage(failure);
      logEvent("error", t("log.sourceSession"), `Session disconnect failed; failure_code=${failure.code}`, { origin: "conversation", conversationId: sessionId, target: findSession(sessionId)?.target, eventKey: "session_disconnect_failed", userMessage: "断开连接未完成，请重试。" });
      throw error;
    }),
    onDisconnected: async (sessionId, connection) => {
      await updateRemoteSessionCore?.({
        sessionId,
        state: "disconnected",
        connectionModel: connection.connectionModel,
        displaySource: connection.displaySource,
        inputSource: connection.inputSource,
      }).catch(() => undefined);
    },
  });
  const connectAttempts = createSessionConnectAttempts({
    canConnect: (sessionId) => {
      const session = findSession(sessionId);
      return Boolean(session && session.state !== "closed") && !closingSessions.has(sessionId) && !userDisconnector.isPending(sessionId);
    },
    releaseSession: disconnectOwnedSession,
    stopCore: (sessionId) => stopRemoteSessionCore?.(sessionId) ?? Promise.resolve(),
    disconnectConnection: (connection) => vncAdapter.disconnect(connection),
  });
  const userDisconnector = createSessionUserDisconnector({
    getConnection: (sessionId) => sessionConnectionRegistry.get(sessionId)?.connection,
    shouldNotify: (sessionId) => findSession(sessionId)?.state === "connected",
    cancel: (sessionId, removeCore) => connectAttempts.cancel(sessionId, removeCore),
    onFailure: (sessionId) => {
      const message = t("settings.remoteSessionEndUnconfirmed");
      notify(message, "error");
      logEvent("warning", t("log.sourceSession"), "Remote session end notification unconfirmed before user disconnect.", {
        origin: "conversation", conversationId: sessionId, target: findSession(sessionId)?.target,
        eventKey: "remote_session_end_unconfirmed", userMessage: message,
      });
    },
  });
  async function disconnectSessionConnection(sessionId: string) {
    return connectAttempts.cancel(sessionId);
  }
  async function prepareSessionForReconnect(sessionId: string) {
    if (sessionConnectionRegistry.get(sessionId)) {
      logEvent("info", t("log.sourceSession"), `Clearing stale connection before reconnecting ${findSession(sessionId)?.target ?? sessionId}.`);
    }
    // The registry may already be detached while its cleanup is still pending.
    await disconnectSessionConnection(sessionId);
  }
  async function disconnectAllSessionConnections() {
    const sessionIds = [...new Set([...sessionConnectionRegistry.listLiveSessionIds(), ...connectAttempts.sessionIds()])];
    await Promise.all(sessionIds.map((sessionId) => disconnectSessionConnection(sessionId)));
  }
  async function connectVnc(passwordOverride?: string, hostRecordId?: string, connectOptions: ConnectVncOptions = {}, promptForPassword = false) {
    if (connectAttempts.isDisposed()) return;
    connectOptions = { ...connectOptions };
    if (connectOptions.endpoint && connectOptions.retrySessionId) {
      const retryTarget = findSession(connectOptions.retrySessionId);
      if (retryTarget && (retryTarget.host !== connectOptions.endpoint.host || retryTarget.port !== connectOptions.endpoint.port)) return;
    }
    const retrySession = connectOptions.retrySessionId
      ? sessionManager.activate(connectOptions.retrySessionId)
      : undefined;
    if (connectOptions.retrySessionId !== undefined && !retrySession) return;
    errorMessage.value = "";
    if (retrySession) {
      host.value = retrySession.host;
      port.value = retrySession.port;
    }
    // Tab activation and form edits can change shared refs while setup awaits.
    const requestHost = retrySession?.host ?? connectOptions.endpoint?.host ?? host.value;
    const requestPort = retrySession?.port ?? connectOptions.endpoint?.port ?? port.value;
    // An explicit endpoint cannot borrow another target's form credential.
    let connectionPassword = promptForPassword ? undefined : (passwordOverride ?? (connectOptions.endpoint ? "" : password.value)) || undefined;
    passwordOverride = undefined;
    const target = `${requestHost}:${requestPort}`;
    const reusableSession = retrySession ? undefined : sessionManager.findReusableByTarget({
      protocol: "vnc",
      host: requestHost,
      port: requestPort,
    });
    const reusableLocalPreparation = reusableSession
      ? await prepareLocalVncServerForSession(reusableSession.id, requestHost, requestPort)
      : undefined;
    const reusableConnectionModel = createInitialConnectionModel(reusableLocalPreparation?.localEndpointMatch);
    if (reusableSession) {
      logSessionTransportResolution({
        target,
        localPreparation: reusableLocalPreparation,
        connectionModel: reusableConnectionModel,
        logEvent,
        t,
      });
    }
    const reusableConnection = reusableSession
      ? sessionConnectionRegistry.get(reusableSession.id)?.connection
      : undefined;
    const shouldReconnectReusableSession = false;
    const recoverableSession = retrySession || reusableSession ? undefined : sessionManager.findRecoverableByTarget({
      protocol: "vnc",
      host: requestHost,
      port: requestPort,
    });
    const recoverableSessionPreviousState = recoverableSession?.state;
      if (reusableSession && !shouldReconnectReusableSession) {
        connectionPassword = undefined;
        activeWorkspace.value = "sessions";
        await activateSessionTab(reusableSession.id);
      logEvent("info", t("log.sourceSession"), `Reused existing session tab ${target}.`);
      return;
    }
    if (shouldReconnectReusableSession && reusableSession) {
      logEvent("info", t("log.sourceSession"), `Reconnecting ${target} with base_transport=vnc; previous_identity=${reusableConnection?.connectionModel.serverIdentity.kind ?? "unknown"}.`);
    }
    const session = retrySession ?? (shouldReconnectReusableSession ? reusableSession : undefined) ?? recoverableSession ?? sessionManager.create({
      protocol: "vnc",
      host: requestHost,
      port: requestPort,
      qualityProfile: defaultQualityProfile.value,
      savedHostName: connectOptions.savedHostName,
    });
    if (recoverableSession) {
      sessionManager.activate(recoverableSession.id);
      logEvent(
        "info",
        t("log.sourceSession"),
        recoverableSessionPreviousState === "idle"
          ? `Starting existing idle session tab ${target}.`
          : `Recovering session tab ${target}; previous_state=${recoverableSessionPreviousState}.`,
      );
    }
    activeRemoteSession.value = session;
    const sessionId = session.id;
    hostRecordId ??= hostRecordIdBySessionId.get(sessionId);
    hostRecordIdBySessionId.set(sessionId, hostRecordId);
    activeWorkspace.value = "sessions";
    await connectAttempts.run(sessionId, async (attempt) => {
      const syncAttemptCore = (request: RemoteSessionUpdateRequest) => {
        if (!attempt.isLive()) return;
        void attempt.track<unknown>(updateRemoteSessionCore?.(request) ?? Promise.resolve()).catch((error) => {
          if (attempt.isLive()) logEvent("warning", t("log.sourceSession"), `Remote Session Core update failed; error=${readNativeRfbFailure(error)?.code ?? String(error)}`, { origin: "conversation", conversationId: sessionId, target, eventKey: "session_core_update_failed", userMessage: "会话状态同步未完成，连接可能受影响。" });
        });
      };
      try {
        await attempt.wait(() => disconnectOwnedSession(sessionId));
        if (promptForPassword) {
          connectionPassword = await attempt.wait(() => requestPasswordForSession(target, attempt.signal));
          if (connectionPassword === undefined) return false;
        }
        await attempt.wait(() => persistHostConnectionAttempt(hostRecordId));
        refreshSessionTabs();
        if (connectOptions.postConnectPresentation === "fullscreen") {
          void enterSessionFullscreen(sessionId);
        }
        const localPreparation = await attempt.wait(() => prepareLocalVncServerForSession(sessionId, requestHost, requestPort));
        const connectionModel = createInitialConnectionModel(localPreparation?.localEndpointMatch);
        logSessionTransportResolution({
          target,
          localPreparation,
          connectionModel,
          logEvent,
          t,
        });
        await attempt.startCore(async () => startRemoteSessionCore?.({
          sessionId,
          target,
          connectionModel,
          displaySource: "vnc",
          inputSource: "vnc",
        }).catch((error) => {
          if (attempt.isLive()) logEvent("warning", t("log.sourceSession"), `Remote Session Core start failed for ${target}; error=${String(error)}`, { origin: "conversation", conversationId: sessionId, target, eventKey: "session_core_start_failed", userMessage: "会话服务未能启动，请重试连接。" });
          throw error;
        }));
        const coreSurface = await attempt.wait(async () => attachRemoteSessionCoreSurface?.({
          sessionId,
          surfaceId: createSessionSurfaceId({
            sessionId,
            owner: "embedded",
            windowLabel: "main",
          }),
          owner: "embedded",
          windowLabel: "main",
          active: true,
        }).catch((error) => {
          if (attempt.isLive()) logEvent("warning", t("log.sourceSession"), `Remote Session Core surface attach failed for ${target}; error=${String(error)}`, { origin: "conversation", conversationId: sessionId, target, eventKey: "session_surface_attach_failed", userMessage: "画面窗口未能连接，请重试。" });
          throw error;
        }));
        transitionSession(sessionId, "connecting");
        logEvent(
          "info",
          t("log.sourceSession"),
          `Starting native RFB Core transport to ${target}`,
        );
        let readonly = Boolean(connectOptions.readonly) || Boolean(localPreparation?.viewOnlyShared);
        setSessionReadonlyState(sessionId, readonly);
        setSessionClipboardPolicy(sessionId, clipboardSyncMode.value, readonly);
        // Native Core reports the initial connecting state synchronously, before
        // `connect()` can resolve its ProtocolConnection object.
        let protocolConnection: ProtocolConnection | undefined;
        await attempt.wait(nextTick);
        const screenTarget = screenTargets.get(sessionId);
        if (!screenTarget) {
          throw new Error("missing Core Surface target");
        }
        screenTarget.replaceChildren();
        const connecting = attempt.connect(() => vncAdapter.connect({
          target: screenTarget,
          sessionId,
          surfaceUrl: coreSurface?.surfaceUrl,
          host: requestHost,
          port: requestPort,
          connectionModel,
          password: connectionPassword,
          signal: attempt.signal,
          readonly,
          // RFB is receive-only here; Client-originated writes are coordinated globally.
          clipboardSyncMode: "bidirectional-active",
          onCredentialsRequired: () => attempt.wait(() => requestPasswordForSession(target, attempt.signal)),
          onServerRuntimeChanged: () => {
            if (attempt.isLive()) options.onServerRuntimeChanged?.(sessionId);
          },
          onStatus: (value) => {
            if (!attempt.isLive()) return;
            const nextState = value as RemoteSessionState;
            if (shouldIgnoreStatusAfterConnectionIssue(findSession(sessionId), nextState)) {
              logEvent("info", t("log.sourceSession"), `Ignored follow-up status ${value} after connection issue for ${target}.`);
              if (shouldCleanupConnectionForSessionState(nextState)) {
                void disconnectSessionConnection(sessionId).catch(() => undefined);
              }
              return;
            }
            transitionSession(sessionId, nextState);
            syncAttemptCore({
              sessionId,
              state: nextState,
              displaySource: protocolConnection?.displaySource ?? "vnc",
              inputSource: protocolConnection?.inputSource ?? "vnc",
            });
            if (value === "connected") {
              if (activeRemoteSession.value?.id === sessionId) errorMessage.value = "";
              startDurationTimer();
              onSessionConnected?.(sessionId);
              void refreshLiveDisplays();
              if (localPreparation) {
                void refreshLocalVncServerStatus();
              }
              attempt.defer(() => {
                void refreshLiveDisplays();
                if (localPreparation) {
                  void refreshLocalVncServerStatus();
                }
              }, 250);
            }
            if (shouldCleanupConnectionForSessionState(value as RemoteSessionState)) {
              void disconnectSessionConnection(sessionId).catch(() => undefined);
            }
          },
          onError: (message) => {
            if (!attempt.isLive()) return;
            applyRawConnectionError(sessionId, message);
            void disconnectSessionConnection(sessionId).catch(() => undefined);
          },
          onDisplayInfo: (info) => {
            if (!attempt.isLive()) return;
            updateSessionDisplayInfo(sessionId, info);
          },
          onRuntimeEvidence: (evidence) => {
            if (!attempt.isLive()) return;
            updateSessionRuntimeEvidence(sessionId, evidence);
          },
          onClipboardActivity: (activity) => {
            if (!attempt.isLive()) return;
            updateSessionClipboardActivity(sessionId, activity);
          },
          onRemoteClipboardText: async (text, channel) => {
            if (!attempt.isLive()) return false;
            const accepted = await attempt.track(receiveRemoteClipboardText?.(sessionId, text) ?? Promise.resolve(false));
            if (!attempt.isLive()) return false;
            updateSessionClipboardActivity(sessionId, {
              direction: "server-to-client",
              atMs: Date.now(),
              accepted,
              reason: accepted ? "active-session" : "inactive-or-readonly-session",
              mode: "bidirectional-active",
              bytes: new TextEncoder().encode(text).length,
              channel,
            });
            return accepted;
          },
          onConnectionModelChanged: (updatedConnectionModel) => {
            if (!attempt.isLive()) return;
            syncAttemptCore({
              sessionId,
              connectionModel: updatedConnectionModel,
              displaySource: protocolConnection?.displaySource ?? "vnc",
              inputSource: protocolConnection?.inputSource ?? "vnc",
            });
            refreshSessionTabs();
            if (protocolConnection) {
              void attempt.track(sessionDisplayEvents.refreshSessionRemoteDisplayInventory(sessionId, protocolConnection, attempt.isLive));
            }
          },
          onViewOnlySharedChanged: (viewOnly) => {
            if (!attempt.isLive()) return;
            if (!viewOnly) {
              return;
            }
            readonly = true;
            setSessionReadonlyState(sessionId, true);
            setSessionViewOnlySharedState(sessionId, true);
            setSessionClipboardPolicy(sessionId, clipboardSyncMode.value, true);
            protocolConnection?.setReadonlyMode(true);
            refreshSessionTabs();
            options.onSessionInputPolicyChanged?.(sessionId);
          },
          onConnectionIssue: (issue) => {
            if (!attempt.isLive()) return;
            applyConnectionIssue(sessionId, issue);
            void disconnectSessionConnection(sessionId).catch(() => undefined);
          },
          onEvent: (source, message, level = "info") => {
            if (!attempt.isLive()) return;
            const userMessage = level === "warning" && source === "discovery"
              ? "增强能力发现失败，基础 VNC 连接不受影响。"
              : level === "warning" && source === "clipboard"
                ? "增强剪贴板暂不可用，可能影响文本同步。"
                : undefined;
            logEvent(level, source, message, { origin: "conversation", conversationId: sessionId, target, eventKey: source, userMessage });
          },
        }));
        // The Adapter takes the transient value synchronously before its awaits.
        connectionPassword = undefined;
        protocolConnection = await connecting;
        protocolConnection.setQualityProfile(session.qualityProfile);
        const displayInfo = protocolConnection.getDisplayInfo();
        if (displayInfo) {
          updateSessionDisplayInfo(sessionId, displayInfo);
        }
        const runtimeEvidence = protocolConnection.getRuntimeEvidence();
        if (runtimeEvidence) {
          updateSessionRuntimeEvidence(sessionId, runtimeEvidence);
        }
        sessionConnectionRegistry.attach(sessionId, protocolConnection);
        await attempt.wait(async () => registerClipboardSession?.(sessionId, protocolConnection!, readonly));
        if (activeRemoteSession.value?.id === sessionId) {
          sessionConnectionRegistry.activateCanvas(sessionId);
          await attempt.wait(async () => activateClipboardSession?.(sessionId));
        }
        void attempt.track(sessionDisplayEvents.refreshSessionRemoteDisplayInventory(sessionId, protocolConnection, attempt.isLive));
        onSessionConnected?.(sessionId);
        if (localPreparation) {
          void refreshLocalVncServerStatus();
        }
        const transport = protocolConnection.transport;
        const connectedSession = sessionManager.attachTransport(sessionId, transport);
        if (activeRemoteSession.value?.id === sessionId && connectedSession) {
          activeRemoteSession.value = connectedSession;
          transportHandle.value = connectedSession.transport ?? null;
        }
        refreshSessionTabs();
        logEvent("success", t("log.sourceSession"), `Native RFB Core transport ready for ${target}; transport=${transport.id}`, { origin: "conversation", conversationId: sessionId, target, eventKey: "vnc_connected", userMessage: "已建立 VNC 连接" });
        // Display refresh can cancel a zombie and wait for this operation.
        void refreshLiveDisplays().catch(() => undefined);
        if (connectOptions.postConnectPresentation === "external") {
          await attempt.wait(async () => openExternalWindowSession?.(sessionId));
        }
        return true;
      } catch (error) {
        connectionPassword = undefined;
        if (!attempt.isLive()) return false;
        const nativeFailure = readNativeRfbFailure(error);
        const rawError = nativeFailure?.code ?? String(error);
        const reason = nativeFailure ?? rawError;
        const classification = classifyConnectionFailure(reason);
        const issue = createConnectionIssue({ phase: "tcpConnecting", reason });
        const localizedMessage = localizedConnectionFailureMessage(classification);
        errorMessage.value = localizedMessage;
        if (issue.code !== "authentication_cancelled" && issue.code !== "connection_cancelled") {
          notify(localizedMessage, "error");
        }
        const failedSession = sessionManager.fail(
          sessionId,
          { ...issue, message: localizedMessage },
        );
        if (failedSession) {
          if (activeRemoteSession.value?.id === sessionId) {
            activeRemoteSession.value = failedSession;
            status.value = failedSession.state;
            transportHandle.value = failedSession.transport ?? null;
          }
          syncAttemptCore({ sessionId, state: failedSession.state });
          refreshSessionTabs();
        } else {
          status.value = "failed";
          syncAttemptCore({ sessionId, state: "failed" });
        }
        logEvent("error", t("log.sourceSession"), `failure_code=${issue.code}; ${issue.title}; reason=${rawError}`, { origin: "conversation", conversationId: sessionId, target, eventKey: "connection_issue", userMessage: localizedMessage });
        return false;
      } finally {
        connectionPassword = undefined;
      }
    });
  }
  async function disconnectSessionFromViewport(sessionId: string) {
    await activateSessionTab(sessionId);
    const hadConnection = Boolean(sessionConnectionRegistry.get(sessionId));
    if (hadConnection) {
      logEvent("info", t("log.sourceSession"), `Disconnect requested for ${findSession(sessionId)?.target ?? sessionId}.`);
    }
    if (!await userDisconnector.disconnect(sessionId)) return;
    if (activeRemoteSession.value?.id === sessionId) transportHandle.value = null;
    const issue = createDisconnectIssue("user_requested");
    syncSessionAfterIssue(sessionId, sessionManager.transition(sessionId, "disconnected", {
      issue: localizedConnectionIssue(issue),
      disconnectReason: "user_requested",
    }));
  }
  async function retrySessionFromViewport(sessionId: string) {
    await activateSessionTab(sessionId);
    const session = findSession(sessionId);
    if (!session) {
      return;
    }
    logEvent("info", t("log.sourceSession"), `Retrying session ${session.target}.`);
    await connectVnc(undefined, undefined, { retrySessionId: sessionId }, true);
  }
  async function activateSessionTab(sessionId: string) {
    const session = sessionManager.activate(sessionId);
    if (!session) {
      return;
    }
    activeRemoteSession.value = session;
    status.value = session.state;
    host.value = session.host;
    port.value = session.port;
    transportHandle.value = session.transport ?? null;
    onMainSessionChanged?.();
    // Refresh clipboard ownership before display work; the focused child window
    // may still own it while the main window's selected tab changes.
    await activateClipboardSession?.(session.id);
    const canvasConnection = sessionConnectionRegistry.activateCanvas(sessionId);
    if (canvasConnection) {
      await refreshLiveDisplays();
      if (isMainWindowFocused?.() ?? true) canvasConnection.connection.focus();
    } else {
      sessionConnectionRegistry.clearActiveCanvas();
    }
    refreshSessionTabs();
  }
  async function closeSessionTab(sessionId: string) {
    closingSessions.add(sessionId);
    const releasing = userDisconnector.disconnect(sessionId, true);
    if (fullscreenSessionId.value === sessionId) {
      await exitSessionFullscreen();
    }
    await releasing;
    const closed = sessionManager.close(sessionId);
    if (!closed) {
      closingSessions.delete(sessionId);
      return;
    }
    onSessionClosed?.(sessionId);
    activeRemoteSession.value = sessionManager.activeSession;
    status.value = activeRemoteSession.value?.state ?? "idle";
    transportHandle.value = activeRemoteSession.value?.transport ?? null;
    await activateClipboardSession?.(activeRemoteSession.value?.id ?? null);
    if (activeRemoteSession.value) {
      sessionConnectionRegistry.activateCanvas(activeRemoteSession.value.id);
    }
    refreshSessionTabs();
    closingSessions.delete(sessionId);
  }
  return {
    activateSessionTab,
    cleanupZombieSessionConnections,
    closeSessionTab,
    connectionAllowedSessionIds,
    connectVnc,
    disconnectAllSessionConnections,
    disconnectSessionConnection,
    disconnectSessionFromViewport,
    prepareSessionForReconnect,
    refreshLiveDisplays,
    retrySessionFromViewport,
    transitionActiveSession,
    transitionSession,
    dispose: connectAttempts.dispose,
  };
}
