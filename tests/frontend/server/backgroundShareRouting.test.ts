import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { computed, createSSRApp, effectScope, nextTick, ref } from "vue";
import { createLogger, createServer } from "vite";
import { renderToString } from "vue/server-renderer";
import { useAppConfirmDialog } from "../../../src/app/composables/useAppConfirmDialog.ts";
import type { LocalVncServerActiveConnection, LocalVncServerOverview } from "../../../src/server/localShare.ts";
import type { WindowsServiceConfigureRequest, WindowsServiceOverview } from "../../../src/server/windowsService.ts";
import { windowsServiceErrorKey } from "../../../src/server/windowsService.ts";
import type { BackgroundShareOverview, BackgroundShareStartRequest } from "../../../src/server/backgroundShare.ts";
import { LOCAL_SHARE_SETTINGS_STORAGE_KEY, SERVICE_SUGGESTION_STORAGE_KEY } from "../../../src/server/localShareSettings.ts";

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith("@server/")) {
    return next(new URL(`../../../src/server/${specifier.slice(8)}.ts`, import.meta.url).href, context);
  }
  return next(specifier, context);
} });
const { useLocalShareWorkbench } = await import("../../../src/features/local-share/composables/useLocalShareWorkbench.ts");
const { useBackgroundPermissionRepairFeedback, useLocalShareFeedback, useSharingStartupSettingsFeedback, useWindowsServiceSettingsFeedback } = await import("../../../src/features/local-share/composables/useLocalShareFeedback.ts");

function server(status: LocalVncServerOverview["status"], port = 5903): LocalVncServerOverview {
  return {
    status, bindAddress: "0.0.0.0", port, addresses: [], displays: [],
    selectedDisplayId: null, selectedDisplayLabel: null, resolutionMode: "fit", refreshMode: "balanced",
    framebufferWidth: 0, framebufferHeight: 0, securityType: "vnc_auth", viewOnly: false,
    activeConnections: 0, activeConnectionDevices: [], permissions: [],
    agent: { status: "stopped", bindAddress: "0.0.0.0", port: null, capabilities: [], lastError: null, updatedAt: "0" },
    startedAt: null, updatedAt: "0",
  };
}

async function fixture(serviceStatus = "stopped", backgroundStatus: LocalVncServerOverview["status"] = "stopped", installed = true, supported = true, suggestionHandled = true, preferences = new Map<string, string>()) {
  if (suggestionHandled) preferences.set(SERVICE_SUGGESTION_STORAGE_KEY, "handled");
  const service: WindowsServiceOverview = {
    supported, installed, autoStart: false, status: serviceStatus, workerSessionId: null,
    workerRunning: serviceStatus === "running", bindAddress: "0.0.0.0", port: 5904,
    viewOnly: false, credentialConfigured: true, passwordManagementAuthorized: true, lastErrorCode: null, sasPolicy: "services_allowed",
  };
  const background: BackgroundShareOverview = {
    server: server(backgroundStatus, 5907), configured: true, credentialConfigured: true,
    autoStart: true, processRunning: backgroundStatus === "running", lastErrorCode: null,
    runtime: backgroundStatus === "running" ? { pid: 123, buildId: "build-current" } : null,
    currentBuildId: "build-current", permissionSource: backgroundStatus === "running" ? "background" : "pending",
    restartSupported: !supported, ipcVersion: supported ? 9 : 6,
  };
  background.server.agent = { ...background.server.agent, status: "running", port: 6200,
    capabilities: ["clipboard.text.read", "clipboard.text.write", "file.transfer.upload", "agent.display.enumerate"] };
  const calls: string[] = [];
  const configurations: WindowsServiceConfigureRequest[] = [];
  const installations: Array<WindowsServiceConfigureRequest | null> = [];
  const installationRequests: WindowsServiceConfigureRequest[] = [];
  const configurationRequests: WindowsServiceConfigureRequest[] = [];
  const passwordRequests: Array<{ password: string }> = [];
  const passwordChanges: string[] = [];
  let passwordFailure: string | null = null;
  let pendingConfigure: Promise<void> | null = null;
  let pendingServiceStart: Promise<void> | null = null;
  let serviceStartFailure: string | null = null;
  let nextStartOutcome: import("../../../src/server/windowsService.ts").ServiceStartOutcome | null = null;
  const serviceStartRequests: BackgroundShareStartRequest[] = [];
  const backgroundStarts: BackgroundShareStartRequest[] = [];
  const notifications: string[] = [];
  const logEvents: string[] = [];
  let pendingAutoStart: Promise<void> | null = null;
  let autoStartFailure: string | null = null;
  let statusFailure: string | null = null;
  let configureFailure: string | null = null;
  let pendingStatus: Promise<BackgroundShareOverview> | null = null;
  let pendingRestart: Promise<void> | null = null;
  let restartFailure: string | null = null;
  let prepareFailure: string | null = null;
  let runtimeFailure: string | null = null;
  let backgroundStatusFailure: string | null = null;
  let backgroundStartFailure: string | null = null;
  let backgroundStopFailure: string | null = null;
  let pendingBackgroundStop: Promise<void> | null = null;
  let stoppedBackgroundStatus: LocalVncServerOverview["status"] = "stopped";
  let installFailure: string | null = null;
  let pendingInstall: Promise<void> | null = null;
  const installSavedPasswords: boolean[] = [];
  const startSavedPasswords: boolean[] = [];
  const permissionInspections: boolean[] = [];
  const permissionRequests: Record<string, unknown>[] = [];
  let rememberedPermissions: LocalVncServerOverview["permissions"] | null = null;
  const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push(command);
    let result: unknown;
    switch (command) {
      case "get_windows_service_status":
        if (statusFailure) throw new Error(statusFailure);
        result = service; break;
      case "install_windows_service":
        installSavedPasswords.push(args?.useBackgroundPassword === true);
        installations.push(args?.request ? structuredClone(args.request as WindowsServiceConfigureRequest) : null);
        if (args?.request) installationRequests.push(args.request as WindowsServiceConfigureRequest);
        if (pendingInstall) await pendingInstall;
        if (installFailure) throw new Error(installFailure);
        if (args?.request) {
          const request = args.request as WindowsServiceConfigureRequest;
          service.bindAddress = request.bindAddress;
          service.port = request.port;
          service.viewOnly = request.viewOnly;
          service.credentialConfigured ||= request.password.length > 0 || args?.useBackgroundPassword === true;
          service.passwordManagementAuthorized = true;
        }
        service.installed = true; service.status = "stopped"; result = service; break;
      case "uninstall_windows_service": service.installed = false; service.status = "not_installed"; result = service; break;
      case "authorize_windows_service_password_management": service.passwordManagementAuthorized = true; result = service; break;
      case "set_windows_service_password": {
        const request = args as { password: string };
        passwordRequests.push(request);
        passwordChanges.push(request.password);
        if (passwordFailure) throw new Error(passwordFailure);
        service.credentialConfigured = true;
        result = service;
        break;
      }
      case "set_windows_service_auto_start": service.autoStart = args?.enabled === true; result = service; break;
      case "set_background_share_auto_start":
        if (pendingAutoStart) await pendingAutoStart;
        if (autoStartFailure) throw new Error(autoStartFailure);
        background.autoStart = args?.enabled === true;
        result = background; break;
      case "get_windows_service_server_status": result = service.server ?? server(service.status === "running" ? "running" : "stopped", service.port ?? 5904); break;
      case "disconnect_windows_service_connection": {
        const current = service.server;
        assert.ok(current);
        assert.equal(args?.connectionId, current.activeConnectionDevices[0]?.id);
        current.activeConnectionDevices = [];
        current.activeConnections = 0;
        result = current;
        break;
      }
      case "start_windows_service_with_fallback":
        startSavedPasswords.push(args?.useBackgroundPassword === true);
        serviceStartRequests.push(args?.request as BackgroundShareStartRequest);
        if (pendingServiceStart) await pendingServiceStart;
        if (serviceStartFailure) throw new Error(serviceStartFailure);
        if (nextStartOutcome) {
          if (nextStartOutcome.background) Object.assign(background, structuredClone(nextStartOutcome.background));
          result = nextStartOutcome;
          nextStartOutcome = null;
        } else {
          service.status = "running";
          result = { service, background: null, serviceError: null, fallbackError: null, passwordRequired: false };
        }
        break;
      case "stop_windows_service": service.status = "stopped"; result = service; break;
      case "configure_windows_service": {
        const request = args?.request as WindowsServiceConfigureRequest;
        configurationRequests.push(request);
        configurations.push(structuredClone(request));
        if (pendingConfigure) await pendingConfigure;
        if (configureFailure) throw new Error(configureFailure);
        service.bindAddress = request.bindAddress;
        service.port = request.port;
        service.viewOnly = request.viewOnly;
        service.credentialConfigured ||= request.password.length > 0;
        result = service;
        break;
      }
      case "get_background_share_status": {
        if (backgroundStatusFailure) throw new Error(backgroundStatusFailure);
        const actual = pendingStatus ? await pendingStatus : background;
        const inspect = args?.inspectPermissions === true;
        permissionInspections.push(inspect);
        if (!inspect && runtimeFailure) throw new Error(runtimeFailure);
        if (actual.processRunning) {
          if (inspect || !rememberedPermissions) rememberedPermissions = structuredClone(actual.server.permissions);
          result = { ...actual, server: { ...actual.server, permissions: rememberedPermissions }, permissionSource: inspect ? "background" : "background_cached" };
        } else result = actual;
        break;
      }
      case "prepare_background_share":
        if (prepareFailure) throw new Error(prepareFailure);
        background.processRunning = true;
        background.runtime = { pid: 124, buildId: background.currentBuildId };
        background.permissionSource = "background";
        if (!background.server.permissions.length) background.server.permissions = [
          { kind: "screen_recording", status: "granted", requiredFor: "screen_capture" },
          { kind: "accessibility", status: "granted", requiredFor: "input_injection" },
        ];
        rememberedPermissions = structuredClone(background.server.permissions);
        result = background; break;
      case "start_background_share": {
        const request = args?.request as BackgroundShareStartRequest;
        backgroundStarts.push(structuredClone(request));
        if (backgroundStartFailure && !request.temporaryPassword) throw new Error(backgroundStartFailure);
        if (!request.temporaryPassword) background.credentialConfigured ||= request.password.length > 0;
        background.temporaryShare = request.temporaryPassword === true;
        background.server = server("running", request.port);
        result = background;
        break;
      }
      case "stop_background_share":
        if (pendingBackgroundStop) await pendingBackgroundStop;
        if (backgroundStopFailure) throw new Error(backgroundStopFailure);
        if (stoppedBackgroundStatus !== "stopped") {
          result = { ...background, server: { ...background.server, status: stoppedBackgroundStatus } };
          break;
        }
        background.server.status = "stopped";
        background.temporaryShare = false;
        background.processRunning = false;
        background.permissionSource = "pending";
        background.runtime = null;
        background.server.permissions = background.server.permissions.map(item => ({ ...item, status: "unknown" }));
        result = background; break;
      case "restart_background_share":
        assert.equal(args, undefined); // Recovery never sends credentials or starts a listener.
        if (pendingRestart) await pendingRestart;
        if (restartFailure) throw new Error(restartFailure);
        background.server = { ...server("stopped", background.server.port), permissions: background.server.permissions };
        rememberedPermissions = structuredClone(background.server.permissions);
        background.processRunning = true;
        background.runtime = { pid: 456, buildId: background.currentBuildId };
        background.permissionSource = "background";
        result = background;
        break;
      case "request_background_share_permission":
        permissionRequests.push({ ...args });
        result = { opened: true, kind: args?.kind, platform: "macos", target: "prompt", message: "" }; break;
      default: throw new Error(`unexpected command ${command}`);
    }
    return structuredClone(result) as T;
  };
  let state!: ReturnType<typeof useLocalShareWorkbench>;
  let repairSettingsOpened = 0;
  const confirmation = useAppConfirmDialog();
  await renderToString(createSSRApp({ setup() {
    state = useLocalShareWorkbench({ invoke, t: key => key, requestConfirm: confirmation.requestConfirm, openSharingRepairSettings: () => { repairSettingsOpened++; }, logEvent: (_level, _source, message) => { logEvents.push(message); }, notify: key => { notifications.push(key); }, isTauriUnavailableError: () => false, storage: { getItem: key => {
      return preferences.get(key) ?? null;
    }, setItem: (key, value) => { preferences.set(key, value); } } });
    return () => null;
  } }));
  return { state, calls, service, background, configurations, configurationRequests, installations, installationRequests, logEvents,
    ...confirmation, repairSettingsOpened: () => repairSettingsOpened,
    failBackgroundStop: (error: string | null) => { backgroundStopFailure = error; },
    holdBackgroundStop: (promise: Promise<void> | null) => { pendingBackgroundStop = promise; },
    setStoppedBackgroundStatus: (status: LocalVncServerOverview["status"]) => { stoppedBackgroundStatus = status; },
    failInstall: (error: string | null) => { installFailure = error; },
    permissionInspections, permissionRequests, backgroundStarts, notifications, passwordRequests, passwordChanges,
    holdConfigure: (promise: Promise<void> | null) => { pendingConfigure = promise; },
    holdServiceStart: (promise: Promise<void> | null) => { pendingServiceStart = promise; },
    failServiceStart: (error: string | null) => { serviceStartFailure = error; },
    serviceStartRequests, preferences, installSavedPasswords, startSavedPasswords,
    holdInstall: (promise: Promise<void> | null) => { pendingInstall = promise; },
    setStartOutcome: (outcome: import("../../../src/server/windowsService.ts").ServiceStartOutcome) => { nextStartOutcome = outcome; },
    failPassword: (error: string | null) => { passwordFailure = error; },
    holdAutoStart: (promise: Promise<void> | null) => { pendingAutoStart = promise; },
    failAutoStart: (error: string | null) => { autoStartFailure = error; },
    failRuntimeStatus: (error: string | null) => { runtimeFailure = error; },
    failBackgroundStatus: (error: string | null) => { backgroundStatusFailure = error; },
    failBackgroundStart: (error: string | null) => { backgroundStartFailure = error; },
    failPrepare: (error: string | null) => { prepareFailure = error; },
    failConfigure: (error: string | null) => { configureFailure = error; },
    failStatus: (error: string | null) => { statusFailure = error; }, holdStatus: (promise: Promise<BackgroundShareOverview> | null) => { pendingStatus = promise; },
    holdRestart: (promise: Promise<void> | null) => { pendingRestart = promise; },
    failRestart: (error: string | null) => { restartFailure = error; } };
}

test("Windows background config errors finish permission checking and allow a successful retry", async () => {
  const f = await fixture("not_installed", "stopped", false, true);
  f.failBackgroundStatus("background_share_config_invalid");
  const checking = f.state.inspectLocalSharePermissions();
  assert.equal(f.state.localSharePermissionChecking.value, true);
  await checking;
  assert.equal(f.state.localSharePermissionChecking.value, false);
  assert.equal(f.state.localVncServerLoading.value, false);
  assert.equal(f.state.sharingMode.value, null);
  assert.equal(f.state.localSharePermissionCheckError.value, "localShare.background.errors.config_invalid");
  assert.equal(f.state.localVncServerCanToggle.value, false);

  f.failBackgroundStatus(null);
  await f.state.inspectLocalSharePermissions();
  assert.equal(f.state.localSharePermissionChecking.value, false);
  assert.equal(f.state.localVncServerLoading.value, false);
  assert.equal(f.state.localSharePermissionCheckError.value, "");
  assert.equal(f.state.sharingMode.value, "background");
  assert.equal(f.state.localVncServerPort.value, 5907);
  assert.equal(f.state.localVncServerStartInputLocked.value, false);
  assert.ok(!f.calls.some(command => command.startsWith("start_")));
});

test("Mac credential denial unlocks the share form password and the start button resumes temporary sharing", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  await f.state.refreshLocalVncServerStatus();
  f.state.localVncServerPort.value = 5917;
  f.failBackgroundStart("background_share_credential_unavailable");
  await f.state.startLocalVncServer();
  assert.equal(f.state.localShareStartPhase.value, "awaiting_password");
  assert.equal(f.state.sharingAutoStartBusy.value, true);
  assert.equal(f.state.localVncServerLoading.value, false);
  assert.equal(f.state.localVncServerCanToggle.value, false);
  assert.equal(f.state.localVncServerToggleLabelKey.value, "localShare.temporaryPassword.start");
  assert.ok(!f.notifications.includes("localShare.background.errors.credential_unavailable"));
  f.state.localVncServerPort.value = 5999;
  const password = crypto.randomUUID().slice(0, 8);
  f.state.localVncServerPassword.value = password;
  assert.equal(f.state.localVncServerCanToggle.value, true);
  await f.state.toggleLocalVncServer();
  assert.equal(f.backgroundStarts.length, 2);
  assert.equal(f.backgroundStarts[1]?.temporaryPassword, true);
  assert.equal(f.backgroundStarts[1]?.password, password);
  assert.equal(f.backgroundStarts[1]?.port, 5917);
  assert.equal(f.state.localShareStartPhase.value, "idle");
  assert.equal(f.state.localVncServerPassword.value, "");
  assert.equal(f.background.autoStart, true);
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundShareOverview.value?.temporaryShare, true);
  await f.state.stopLocalVncServer();
  assert.equal(f.state.backgroundShareOverview.value?.temporaryShare, false);
});

test("Mac first password save denial automatically reuses the submitted password temporarily", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.background.configured = false;
  f.background.credentialConfigured = false;
  f.background.autoStart = false;
  await f.state.refreshLocalVncServerStatus();
  f.failBackgroundStart("background_share_initial_credential_rollback_failed");
  const password = crypto.randomUUID().slice(0, 8);
  f.state.localVncServerPassword.value = password;
  await f.state.startLocalVncServer();
  assert.equal(f.state.localShareStartPhase.value, "idle");
  assert.ok(!f.notifications.includes("localShare.background.errors.initial_credential_rollback_failed"));
  assert.equal(f.backgroundStarts.length, 2);
  assert.equal(f.backgroundStarts[1]?.temporaryPassword, true);
  assert.equal(f.backgroundStarts[1]?.password, password);
  assert.equal(f.background.credentialConfigured, false);
});

test("compact connection times distinguish today from the previous calendar day", async context => {
  const now = new Date(2026, 9, 16, 0, 0, 1);
  context.mock.timers.enable({ apis: ["Date"], now });
  const { state } = await fixture();
  const connection: LocalVncServerActiveConnection = {
    id: "rfb-1", kind: "rfb", ipAddress: "192.0.2.1", status: "online",
    connectedAt: String(Math.floor(now.getTime() / 1000)),
  };

  assert.equal(state.formatLocalVncServerConnectionTime(connection, true), "localShare.connectionDevices.todayAt");
  assert.equal(state.formatLocalVncServerConnectionTime(connection), now.toLocaleString());

  const previousDay = new Date(now.getTime() - 2000);
  connection.connectedAt = String(Math.floor(previousDay.getTime() / 1000));
  assert.equal(state.formatLocalVncServerConnectionTime(connection, true), previousDay.toLocaleString());

  connection.connectedAt = "invalid";
  assert.equal(state.formatLocalVncServerConnectionTime(connection, true), "-");
});

test("Windows background mode stays inline while failures deduplicate and rearm after recovery", async () => {
  const f = await fixture("not_installed", "running", false);
  await f.state.refreshSharingSettings();
  const active = ref(true);
  const messages: string[] = [];
  const scope = effectScope();
  const feedback = scope.run(() => useLocalShareFeedback({
    state: f.state, active, t: key => key,
    notify: content => { messages.push(content); },
  }))!;
  try {
    assert.deepEqual(messages, []);
    assert.equal(feedback.backgroundModeHint.value, "localShare.windowsBackgroundHint");
    assert.deepEqual(feedback.hints.value, []);
    await f.state.refreshSharingSettings();
    await nextTick();
    active.value = false;
    await nextTick();
    active.value = true;
    await nextTick();
    assert.equal(messages.length, 0);

    f.background.lastErrorCode = "background_share_runtime_failed";
    await f.state.refreshSharingSettings();
    await nextTick();
    const errorMessage = messages.at(-1);
    assert.equal(messages.length, 1);
    await f.state.refreshSharingSettings();
    await nextTick();
    assert.equal(messages.length, 1);
    f.background.lastErrorCode = null;
    await f.state.refreshSharingSettings();
    await nextTick();
    f.background.lastErrorCode = "background_share_runtime_failed";
    await f.state.refreshSharingSettings();
    await nextTick();
    assert.equal(messages.length, 2);
    assert.equal(messages.at(-1), errorMessage);
  } finally { scope.stop(); }
  f.state.sharingRuntimeError.value = "after-dispose";
  await nextTick();
  assert.equal(messages.length, 2);
});

test("background mode guidance follows Windows routing and disappears when status cannot be confirmed", async () => {
  const f = await fixture("stopped", "running");
  await f.state.refreshSharingSettings();
  const scope = effectScope();
  const messages: string[] = [];
  const feedback = scope.run(() => useLocalShareFeedback({
    state: f.state, active: ref(true), t: key => key, notify: content => { messages.push(content); },
  }))!;
  try {
    assert.equal(feedback.backgroundModeHint.value, "localShare.existingBackgroundHint");
    assert.deepEqual(messages, []);
    f.background.server.status = "stopped";
    await f.state.refreshSharingSettings();
    await nextTick();
    assert.equal(feedback.backgroundModeHint.value, "");
    f.service.installed = false;
    await f.state.refreshSharingSettings();
    await nextTick();
    assert.equal(feedback.backgroundModeHint.value, "localShare.windowsBackgroundHint");
    f.state.sharingRuntimeError.value = "unknown-runtime";
    assert.equal(feedback.backgroundModeHint.value, "");
    f.state.sharingRuntimeError.value = "";
    f.service.supported = false;
    await f.state.refreshSharingSettings();
    await nextTick();
    assert.equal(feedback.backgroundModeHint.value, "", "Mac does not show Windows mode guidance");
  } finally { scope.stop(); }
});

test("desktop recovery and worker startup stay out of main-page guidance while specific failures remain visible", async () => {
  const f = await fixture("running");
  f.service.workerRunning = false;
  f.service.desktopRecovering = true;
  await f.state.refreshSharingSettings();
  const messages: Array<{ content: string; tone: string }> = [];
  const scope = effectScope();
  const feedback = scope.run(() => useLocalShareFeedback({
    state: f.state, active: ref(true), t: key => key,
    notify: (content, tone) => { messages.push({ content, tone }); },
  }))!;
  try {
    assert.deepEqual(messages, []);
    assert.deepEqual(feedback.hints.value, []);
    f.service.desktopRecovering = false;
    await f.state.refreshSharingSettings();
    assert.deepEqual(feedback.hints.value, []);
    f.service.lastErrorCode = "windows_service_desktop_relay_unavailable";
    await f.state.refreshSharingSettings();
    await nextTick();
    assert.deepEqual(messages.at(-1), { content: "localShare.windowsService.errors.desktop_relay_unavailable", tone: "error" });
    assert.equal(messages.length, 1);
  } finally { scope.stop(); }
});

test("service settings shortcut requires a confirmed supported and uninstalled service", async () => {
  const f = await fixture("not_installed", "running", false);
  const scope = effectScope();
  const feedback = scope.run(() => useLocalShareFeedback({
    state: f.state, active: ref(false), t: key => key, notify: () => {},
  }))!;
  try {
    assert.equal(feedback.serviceNotInstalled.value, false); // No status yet.
    await f.state.refreshSharingSettings();
    assert.equal(feedback.serviceNotInstalled.value, true);
    f.service.installed = true;
    await f.state.refreshSharingSettings();
    assert.equal(feedback.serviceNotInstalled.value, false);
    f.service.installed = false;
    f.service.supported = false;
    await f.state.refreshSharingSettings();
    assert.equal(feedback.serviceNotInstalled.value, false);
    f.service.supported = true;
    await f.state.refreshSharingSettings();
    assert.equal(feedback.serviceNotInstalled.value, true);
    f.failStatus("windows_service_status_failed");
    await f.state.refreshSharingSettings();
    assert.equal(feedback.serviceNotInstalled.value, false); // Old overview is no longer proof.
  } finally { scope.stop(); }
});

test("server failures use deduplicated messages while normal states stay quiet", async () => {
  const f = await fixture("not_installed", "running", false, false);
  await f.state.refreshLocalVncServerStatus();
  const scope = effectScope();
  const messages: string[] = [];
  scope.run(() => useLocalShareFeedback({
    state: f.state, active: ref(true), t: key => key, notify: content => { messages.push(content); },
  }));
  try {
    assert.deepEqual(messages, []);
    f.background.server.status = "stopped";
    await f.state.refreshLocalVncServerStatus();
    assert.deepEqual(messages, []);
    f.background.server.status = "port_conflict";
    await f.state.refreshLocalVncServerStatus();
    assert.deepEqual(messages, ["localShare.serverStatusDetail.port_conflict"]);
    await f.state.refreshLocalVncServerStatus();
    assert.equal(messages.length, 1);
    f.background.server.status = "failed";
    await f.state.refreshLocalVncServerStatus();
    assert.equal(messages.at(-1), "localShare.serverStatusDetail.failed");
    f.failStatus("windows_service_status_failed");
    await f.state.refreshLocalVncServerStatus();
    assert.equal(messages.at(-1), f.state.sharingRuntimeError.value);
  } finally { scope.stop(); }
});

test("startup failures keep their action message without a second feedback message", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  await f.state.refreshLocalVncServerStatus();
  const scope = effectScope();
  const messages: string[] = [];
  scope.run(() => useLocalShareFeedback({
    state: f.state, active: ref(true), t: key => key, notify: content => { messages.push(content); },
  }));
  try {
    f.failPrepare("background_share_bundle_unavailable");
    await f.state.startLocalVncServer();
    assert.equal(f.state.localVncServerMessage.value, "localShare.background.errors.bundle_unavailable");
    assert.equal(f.notifications.at(-1), f.state.localVncServerMessage.value);
    assert.deepEqual(messages, []);
    f.failPrepare(null);
    await f.state.refreshLocalVncServerStatus();
    assert.equal(f.state.localVncServerMessage.value, "");
    assert.deepEqual(messages, []);
  } finally { scope.stop(); }
});

test("service settings keep contextual guidance inline and toast only actionable failures", async () => {
  const f = await fixture();
  await f.state.refreshSharingSettings();
  const active = ref(false);
  const backgroundRunning = ref(true);
  const messages: string[] = [];
  const scope = effectScope();
  const feedback = scope.run(() => useWindowsServiceSettingsFeedback({
    active, backgroundRunning,
    overview: computed(() => f.state.windowsService.overview.value!), t: key => key,
    notify: content => { messages.push(content); },
  }))!;
  try {
    active.value = true;
    await nextTick();
    assert.deepEqual(messages, []);
    assert.equal(feedback.backgroundHint.value, "localShare.windowsService.stopBackgroundFirst");
    f.service.sasPolicy = "services_denied";
    await f.state.refreshSharingSettings();
    assert.equal(feedback.sasDeniedHint.value, "localShare.windowsService.sasDeniedHint");
    assert.deepEqual(messages, []);
    f.service.sasErrorCode = "windows_service_sas_policy_denied";
    await f.state.refreshSharingSettings();
    await nextTick();
    assert.equal(messages.length, 1);
    await f.state.refreshSharingSettings();
    active.value = false;
    await nextTick();
    active.value = true;
    await nextTick();
    assert.equal(messages.length, 1, "unchanged failures do not repeat on page revisits");
    backgroundRunning.value = false;
    assert.equal(feedback.backgroundHint.value, "");
  } finally { scope.stop(); }
});

test("uninstalled service does not show a background-sharing installation hint", async () => {
  const f = await fixture("not_installed", "running", false);
  await f.state.refreshSharingSettings();
  const scope = effectScope();
  const messages: string[] = [];
  const feedback = scope.run(() => useWindowsServiceSettingsFeedback({
    active: ref(true), backgroundRunning: ref(true),
    overview: computed(() => f.state.windowsService.overview.value!), t: key => key,
    notify: content => { messages.push(content); },
  }))!;
  try {
    assert.deepEqual(messages, []);
    assert.equal(feedback.backgroundHint.value, "");
  } finally { scope.stop(); }
});

test("entering Mac local share prepares an absent host once and shows its actual grants without prompting or starting", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.background.server.permissions = [
    { kind: "screen_recording", status: "granted", requiredFor: "screen_capture" },
    { kind: "accessibility", status: "missing", requiredFor: "input_injection" },
  ];
  const check = f.state.inspectLocalSharePermissions();
  assert.equal(f.state.localSharePermissionChecking.value, true);
  await Promise.all([check, f.state.inspectLocalSharePermissions()]);
  assert.equal(f.calls.filter(call => call === "prepare_background_share").length, 1);
  assert.deepEqual(f.state.localVncServerOverview.value?.permissions.map(item => item.status), ["granted", "missing"]);
  assert.equal(f.state.backgroundShareOverview.value?.permissionSource, "background");
  assert.equal(f.state.localSharePermissionChecking.value, false);
  assert.ok(!f.calls.includes("request_background_share_permission"));
  assert.ok(!f.calls.includes("start_background_share"));
  f.calls.length = 0;
  await f.state.inspectLocalSharePermissions();
  assert.ok(f.calls.includes("get_background_share_status"));
  assert.ok(!f.calls.includes("prepare_background_share")); // Reopening rechecks a live host without recovery.
});

test("stopping and polling an absent Mac host preserves the last actual grants only in the display", async () => {
  const f = await fixture("not_installed", "running", false, false);
  f.background.server.permissions = [
    { kind: "screen_recording", status: "granted", requiredFor: "screen_capture" },
    { kind: "accessibility", status: "missing", requiredFor: "input_injection" },
  ];
  await f.state.readLocalShareServerStatus();
  await f.state.stopLocalVncServer();
  await f.state.readLocalShareServerStatus();
  assert.equal(f.state.localVncServerOverview.value?.status, "stopped");
  assert.deepEqual(f.state.localVncServerOverview.value?.permissions.map(item => item.status), ["granted", "missing"]);
  assert.deepEqual(f.state.backgroundShareOverview.value?.server.permissions.map(item => item.status), ["unknown", "unknown"]);
  assert.ok(!f.calls.includes("prepare_background_share"));
  f.background.processRunning = true;
  f.background.permissionSource = "background";
  f.background.server.permissions[0]!.status = "missing"; // A real subsequent revocation replaces the snapshot.
  await f.state.inspectLocalSharePermissions();
  assert.equal(f.state.localVncServerOverview.value?.permissions[0]?.status, "missing");
});

test("remembered granted cards never allow startup to bypass a missing actual grant", async () => {
  const f = await fixture("not_installed", "running", false, false);
  f.background.server.permissions = [
    { kind: "screen_recording", status: "granted", requiredFor: "screen_capture" },
    { kind: "accessibility", status: "granted", requiredFor: "input_injection" },
  ];
  await f.state.readLocalShareServerStatus();
  await f.state.stopLocalVncServer();
  assert.equal(f.state.localVncServerOverview.value?.permissions[0]?.status, "granted");
  f.background.server.permissions[0]!.status = "missing";
  await f.state.startLocalVncServer();
  assert.equal(f.state.localShareStartPhase.value, "awaiting_permission");
  assert.equal(f.state.localVncServerOverview.value?.permissions[0]?.status, "missing");
  assert.ok(!f.calls.includes("start_background_share"));
  f.state.cancelLocalShareStart();
});

test("runtime refresh reads cached grants while explicit page or toolbar inspection reads actual permissions", async () => {
  const f = await fixture("not_installed", "running", false, false);
  f.background.server.permissions = [
    { kind: "screen_recording", status: "granted", requiredFor: "screen_capture" },
    { kind: "accessibility", status: "granted", requiredFor: "input_injection" },
  ];
  await f.state.inspectLocalSharePermissions();
  f.permissionInspections.length = 0;
  f.background.server.permissions[1]!.status = "missing";
  await f.state.refreshLocalShareRuntimeStatus();
  assert.deepEqual(f.permissionInspections, [false]);
  assert.equal(f.state.localVncServerOverview.value?.permissions[1]?.status, "granted");
  assert.equal(f.state.backgroundShareOverview.value?.permissionSource, "background_cached");
  await f.state.refreshLocalVncServerStatus();
  assert.deepEqual(f.permissionInspections, [false, true]);
  assert.equal(f.state.localVncServerOverview.value?.permissions[1]?.status, "missing");
});

test("a runtime protocol error never triggers a permission inspection or restarts sharing", async () => {
  const f = await fixture("not_installed", "running", false, false);
  await f.state.inspectLocalSharePermissions();
  f.failRuntimeStatus("background_share_protocol_incompatible");
  f.permissionInspections.length = 0;
  await f.state.refreshLocalShareRuntimeStatus();
  assert.deepEqual(f.permissionInspections, [false]);
  assert.notEqual(f.state.sharingRuntimeError.value, "");
  assert.equal(f.state.localVncServerOverview.value?.status, "unknown");
  assert.ok(!f.calls.includes("prepare_background_share"));
});

test("the 1.5 second timer updates runtime during permission waiting without checking or resuming startup", async () => {
  const intervals = new Map<number, () => void>();
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  Object.defineProperty(globalThis, "window", { configurable: true, value: {
    setInterval: (callback: () => void, delay: number) => { intervals.set(delay, callback); return delay; },
    clearInterval: () => {}, addEventListener: () => {}, removeEventListener: () => {},
  } });
  Object.defineProperty(globalThis, "document", { configurable: true, value: { visibilityState: "visible", addEventListener: () => {}, removeEventListener: () => {} } });
  try {
    const f = await fixture("not_installed", "stopped", false, false);
    f.background.server.permissions = [
      { kind: "screen_recording", status: "missing", requiredFor: "screen_capture" },
      { kind: "accessibility", status: "missing", requiredFor: "input_injection" },
    ];
    await f.state.startLocalVncServer();
    try {
      assert.equal(f.state.localShareStartPhase.value, "awaiting_permission");
      f.background.server.permissions.forEach(item => { item.status = "granted"; });
      f.permissionInspections.length = 0;
      intervals.get(1500)!();
      await new Promise(resolve => setImmediate(resolve));
      assert.deepEqual(f.permissionInspections, [false]);
      assert.equal(f.state.localShareStartPhase.value, "awaiting_permission");
      assert.ok(!f.calls.includes("start_background_share"));
      await f.state.resumeLocalShareStart();
      assert.equal(f.state.localShareStartPhase.value, "idle");
      assert.equal(f.calls.filter(call => call === "start_background_share").length, 1);
    } finally { f.state.cancelLocalShareStart(); }
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else Reflect.deleteProperty(globalThis, "window");
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else Reflect.deleteProperty(globalThis, "document");
  }
});

test("an entry permission check failure stays distinct from a missing grant and can be retried", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.failPrepare("background_share_bundle_unavailable");
  await f.state.inspectLocalSharePermissions();
  assert.ok(f.state.localSharePermissionCheckError.value);
  assert.equal(f.state.localSharePermissionChecking.value, false);
  assert.equal(f.state.localVncServerLoading.value, false);
  assert.notEqual(f.state.localVncServerOverview.value?.permissions[0]?.status, "missing");
  f.failPrepare(null);
  await f.state.inspectLocalSharePermissions();
  assert.equal(f.state.localSharePermissionCheckError.value, "");
  assert.equal(f.state.sharingMode.value, "background");
});

test("a first Mac identity-check failure keeps local-share status, connections and permission sections visible", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  const notices: string[] = [];
  const feedbackScope = effectScope();
  feedbackScope.run(() => useLocalShareFeedback({
    state: f.state, active: ref(true), t: key => key,
    notify: (content, tone) => { notices.push(`${tone}:${content}`); },
  }));
  f.failBackgroundStatus("background_share_access_denied");
  await f.state.inspectLocalSharePermissions();
  await nextTick();
  assert.ok(notices.includes("error:localShare.background.errors.access_denied"), "entry failure is reported by Toast");
  assert.equal(f.state.sharingMode.value, null);
  assert.equal(f.state.localVncServerOverview.value, null);
  const logger = createLogger("error");
  const reportViteError = logger.error.bind(logger);
  logger.error = (message, options) => {
    // SSR compilation needs no HMR socket; sandboxed tests cannot bind one.
    if (message.startsWith("WebSocket server error:") && message.includes("listen EPERM")) return;
    reportViteError(message, options);
  };
  const vite = await createServer({
    customLogger: logger,
    optimizeDeps: { noDiscovery: true, entries: [] },
    server: { middlewareMode: true, hmr: false },
  });
  try {
    const { default: LocalShareWorkbench } = await vite.ssrLoadModule("/src/features/local-share/components/LocalShareWorkbench.vue");
    const { provideVncAppContext } = await vite.ssrLoadModule("/src/app/composables/useVncAppContext.ts");
    const { h } = await import("vue");
    const { NMessageProvider } = await import("naive-ui");
    const render = () => renderToString(createSSRApp({ setup() {
      provideVncAppContext({ ...f.state, activeWorkspace: ref("hosts"), t: (key: string) => key });
      return () => h(NMessageProvider, null, { default: () => h(LocalShareWorkbench) });
    } }));
    const html = await render();
    assert.ok(html.includes("localShare.connectionDevices.title"), "connection section remains visible");
    assert.ok(html.includes("localShare.permissions.title"), "permission section remains visible");
    assert.ok(!html.includes("local-share-check-error"), "there is no inline error card");
    assert.ok(html.includes("localShare.refreshServerStatus"), "header refresh remains available");
    assert.ok(html.includes("localShare.connectionDevices.unconfirmed"), "unknown connections are not shown as zero");
    assert.ok(html.includes("localShare.permissions.checkUnavailable"), "permission failure is not shown as a missing grant");
    assert.equal(f.state.localVncServerCanToggle.value, false);
    assert.equal(f.state.localVncServerCanCopyProfile.value, false);

    f.failBackgroundStatus(null);
    await f.state.refreshLocalVncServerStatus();
    assert.equal(f.state.sharingMode.value, "background");
    assert.equal(f.state.sharingRuntimeError.value, "");
    assert.ok(!(await render()).includes("localShare.permissions.checkUnavailable"), "successful refresh clears unavailable permissions");

    f.failBackgroundStatus("background_share_access_denied:peer_path_mismatch");
    await f.state.refreshLocalVncServerStatus();
    await nextTick();
    assert.equal(notices.filter(message => message === "error:localShare.background.errors.access_denied").length, 2, "a later failure can show a new Toast after recovery");
    assert.equal(f.state.sharingRuntimeError.value, "localShare.background.errors.access_denied");
    assert.ok(f.logEvents.some(message => message.includes("background_share_access_denied:peer_path_mismatch")), "diagnostics record the failing identity-check stage");
    const staleHtml = await render();
    assert.ok(staleHtml.includes("localShare.lastConfirmedStatus"), "last known settings are labelled as stale");
    assert.ok(staleHtml.includes("localShare.permissions.checkUnavailable"), "stale grants are hidden");
    assert.ok(staleHtml.includes("localShare.connectionDevices.unconfirmed"), "stale devices are hidden");
    assert.equal(f.state.localVncServerCanToggle.value, false);
    assert.equal(f.state.localVncServerCanCopyProfile.value, false);
  } finally { feedbackScope.stop(); await vite.close(); }
});

test("opening local share during a pending status refresh still probes the Mac host", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  let release!: (overview: BackgroundShareOverview) => void;
  f.holdStatus(new Promise(resolve => { release = resolve; }));
  const read = f.state.readLocalShareServerStatus();
  await new Promise(resolve => setImmediate(resolve));
  const check = f.state.inspectLocalSharePermissions();
  release(f.background);
  await Promise.all([read, check]);
  assert.ok(f.calls.includes("prepare_background_share"));
  assert.equal(f.state.localSharePermissionChecking.value, false);
});

test("leaving local share during startup status detection cancels a late startup intent", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  let release!: (overview: BackgroundShareOverview) => void;
  f.holdStatus(new Promise(resolve => { release = resolve; }));
  const start = f.state.startLocalVncServer();
  assert.equal(f.state.localShareStartChecking.value, true);
  f.state.cancelLocalShareStart();
  release(f.background);
  await start;
  assert.ok(!f.calls.includes("prepare_background_share"));
  assert.ok(!f.calls.includes("start_background_share"));
  assert.equal(f.state.localShareStartPhase.value, "idle");
});

test("opening an installed service page does not prepare a Mac permissions host", async () => {
  const f = await fixture();
  await f.state.inspectLocalSharePermissions();
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.ok(!f.calls.includes("prepare_background_share"));
});

test("Windows service disconnects a listed device without password management authorization", async () => {
  const f = await fixture("running");
  f.service.passwordManagementAuthorized = false;
  const connection: LocalVncServerActiveConnection = {
    id: "ip:192.168.85.1", kind: "rfb", ipAddress: "192.168.85.1",
    connectedAt: "1", status: "online",
  };
  f.service.server = {
    ...server("running", 5904), activeConnections: 1, activeConnectionDevices: [connection],
  };
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.equal(f.state.localVncServerOverview.value?.activeConnections, 1);

  f.calls.length = 0;
  await f.state.disconnectLocalVncServerConnection(connection);
  assert.ok(f.calls.includes("disconnect_windows_service_connection"));
  assert.ok(!f.calls.includes("stop_windows_service"));
  assert.ok(!f.calls.includes("authorize_windows_service_password_management"));
  assert.equal(f.state.localVncServerOverview.value?.activeConnections, 0);
  assert.equal(f.service.status, "running");
});

test("an outdated Windows service reports the protocol mismatch instead of invalid configuration", () => {
  assert.equal(windowsServiceErrorKey("windows_service_protocol_incompatible"),
    "localShare.windowsService.errors.protocol_incompatible");
});

test("Mac keeps an active old background running and offers maintenance", async () => {
  const f = await fixture("not_installed", "running", false, false);
  f.background.runtime = { pid: 123, buildId: "build-old" };
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundRuntimeOutdated.value, true);
  assert.equal(f.state.localVncServerCanToggle.value, true); // Existing sharing can still be stopped.
  assert.ok(!f.calls.includes("restart_background_share"));
  f.background.runtime = null;
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundRuntimeOutdated.value, true);
  f.background.server.status = "stopped";
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.localVncServerCanToggle.value, true);
  assert.equal(f.state.localVncServerCanStart.value, true);
});

test("Mac startup replaces an idle old background once without starting sharing", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.background.processRunning = true;
  f.background.runtime = { pid: 123, buildId: "build-old" };
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundRuntimeOutdated.value, false);
  assert.equal(f.state.backgroundShareOverview.value?.runtime?.buildId, "build-current");
  assert.equal(f.state.backgroundShareOverview.value?.server.status, "stopped");
  assert.equal(f.calls.filter(call => call === "prepare_background_share").length, 1);
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.calls.filter(call => call === "prepare_background_share").length, 1);
  assert.ok(!f.calls.includes("start_background_share"));
});

test("Mac recovery serializes actions, selects the new process and leaves sharing stopped", async () => {
  const f = await fixture("not_installed", "running", false, false);
  await f.state.refreshLocalVncServerStatus();
  let release!: () => void;
  f.holdRestart(new Promise(resolve => { release = resolve; }));
  f.calls.length = 0;
  const restart = f.state.restartBackgroundShare();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.localVncServerLoading.value, true);
  await f.state.restartBackgroundShare();
  await f.state.startLocalVncServer();
  assert.equal(f.calls.filter(call => call === "restart_background_share").length, 1);
  release();
  await restart;
  assert.equal(f.state.localVncServerOverview.value?.status, "stopped");
  assert.equal(f.state.backgroundShareOverview.value?.runtime?.pid, 456);
  assert.equal(f.state.localVncServerPassword.value, "");
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
});

test("Windows service does not expose or dispatch Mac permission recovery", async () => {
  const f = await fixture();
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundRestartSupported.value, false);
  await f.state.restartBackgroundShare();
  await f.state.updateAndRepairBackgroundShare();
  assert.ok(!f.calls.includes("restart_background_share"));
});

test("Mac authorization uses one command and manual rechecks never reset or restart the host", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  const permission = { kind: "screen_recording", status: "missing", requiredFor: "screen_capture" } as const;
  f.background.server.permissions = [{ ...permission }];
  await f.state.refreshLocalVncServerStatus();
  await f.state.requestLocalVncServerPermission(permission);
  try {
    assert.deepEqual(f.permissionRequests, []);
    await f.state.openLocalShareGuideSettings();
    await f.state.requestLocalShareGuidePermission();
    assert.deepEqual(f.permissionRequests, [
      { kind: "screen_recording", openSettings: true },
      { kind: "screen_recording", openSettings: false },
    ]);
    const beforeRecheck = f.calls.length;
    await f.state.resumeLocalShareStart(false);
    assert.ok(!f.calls.slice(beforeRecheck).includes("prepare_background_share"));
    assert.ok(!f.calls.slice(beforeRecheck).includes("restart_background_share"));
    assert.equal(f.permissionRequests.length, 2);
  } finally { f.state.cancelLocalShareStart(); }
});

test("Mac repair shortcut requires a missing grant and changed permission identity, not a different build", async () => {
  const f = await fixture("not_installed", "running", false, false);
  f.background.runtime = { pid: 123, buildId: "old" };
  f.background.server.permissions = [{ kind: "screen_recording", status: "missing", requiredFor: "screen_capture" }];
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundPermissionRepairNeeded.value, false);
  f.background.signatureChangedPermissions = ["screen_recording"];
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundPermissionRepairNeeded.value, true);
  f.background.server.permissions[0]!.status = "granted";
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.backgroundPermissionRepairNeeded.value, false);
});

test("Mac permission identity warnings toast once until a confirmed recovery", async () => {
  const f = await fixture("not_installed", "running", false, false);
  const permission = { kind: "screen_recording", status: "missing", requiredFor: "screen_capture" } as const;
  f.background.server.permissions = [{ ...permission }];
  f.background.signatureChangedPermissions = [permission.kind];
  const active = ref(false);
  const messages: string[] = [];
  const scope = effectScope();
  scope.run(() => useBackgroundPermissionRepairFeedback({
    active, state: f.state, t: key => key,
    notify: (content, tone) => { messages.push(`${tone}:${content}`); },
  }));
  try {
    await f.state.refreshLocalVncServerStatus();
    assert.deepEqual(messages, []);
    active.value = true;
    await nextTick();
    const warning = "warning:localShare.permissions.signatureChanged";
    assert.deepEqual(messages, [warning]);

    await f.state.refreshLocalShareRuntimeStatus();
    active.value = false;
    await nextTick();
    active.value = true;
    await nextTick();
    f.failStatus("windows_service_status_failed");
    await f.state.refreshLocalVncServerStatus();
    f.failStatus(null);
    f.background.server.permissions[0]!.status = "unknown";
    await f.state.refreshLocalVncServerStatus();
    f.background.server.permissions[0]!.status = "missing";
    await f.state.refreshLocalVncServerStatus();
    assert.deepEqual(messages, [warning]);

    f.background.server.permissions[0]!.status = "granted";
    await f.state.refreshLocalVncServerStatus();
    f.background.server.permissions[0]!.status = "missing";
    await f.state.refreshLocalVncServerStatus();
    assert.deepEqual(messages, [warning, warning]);
  } finally { scope.stop(); }
});

test("settings maintenance replaces the host before guidance and keeps configuration and normal grants", async () => {
  const f = await fixture("not_installed", "running", false, false);
  f.background.server.permissions = [
    { kind: "screen_recording", status: "granted", requiredFor: "screen_capture" },
    { kind: "accessibility", status: "missing", requiredFor: "input_injection" },
  ];
  await f.state.refreshLocalVncServerStatus();
  f.calls.length = 0;
  await f.state.updateAndRepairBackgroundShare();
  assert.equal(f.calls[0], "restart_background_share");
  assert.equal(f.state.localShareGuidePurpose.value, "repair");
  assert.deepEqual(f.state.localShareMissingPermissions.value, ["accessibility"]);
  assert.equal(f.state.localVncServerOverview.value?.status, "stopped");
  assert.equal(f.background.credentialConfigured, true);
  assert.equal(f.background.autoStart, true);
  assert.ok(!f.calls.includes("request_background_share_permission"));
  await f.state.startLocalVncServer();
  assert.ok(!f.calls.includes("start_background_share"));
  f.state.cancelLocalShareStart();
});

test("cancelled maintenance keeps other lifecycle actions locked until its restart settles", async () => {
  const f = await fixture("not_installed", "running", false, false);
  await f.state.refreshLocalVncServerStatus();
  let release!: () => void;
  f.holdRestart(new Promise(resolve => { release = resolve; }));
  f.calls.length = 0;
  const operation = f.state.updateAndRepairBackgroundShare();
  await new Promise(resolve => setImmediate(resolve));
  f.state.cancelLocalShareStart();
  assert.equal(f.state.localVncServerLoading.value, true);
  await f.state.restartBackgroundShare();
  await f.state.startLocalVncServer();
  assert.equal(f.calls.filter(call => call === "restart_background_share").length, 1);
  assert.ok(!f.calls.includes("start_background_share"));
  release();
  await operation;
  assert.equal(f.state.localVncServerLoading.value, false);
  assert.equal(f.state.localShareStartPhase.value, "idle");
});

test("failed settings maintenance never enters permission guidance or starts a listener", async () => {
  const f = await fixture("not_installed", "running", false, false);
  await f.state.refreshLocalVncServerStatus();
  f.failRestart("background_share_restart_timeout");
  f.calls.length = 0;
  await f.state.updateAndRepairBackgroundShare();
  assert.equal(f.state.localShareStartPhase.value, "idle");
  assert.ok(!f.calls.includes("request_background_share_permission"));
  assert.ok(!f.calls.includes("start_background_share"));
});

test("failed Mac recovery rechecks actual state and releases the action gate", async () => {
  const f = await fixture("not_installed", "running", false, false);
  f.background.runtime = { pid: 123, buildId: "build-old" };
  await f.state.refreshLocalVncServerStatus();
  f.failRestart("background_share_restart_timeout");
  f.calls.length = 0;
  await f.state.restartBackgroundShare();
  assert.equal(f.state.localVncServerLoading.value, false);
  assert.equal(f.state.backgroundShareOverview.value?.runtime?.pid, 123);
  assert.equal(f.state.backgroundRuntimeOutdated.value, true);
  assert.ok(f.calls.includes("get_background_share_status"));
  assert.ok(!f.calls.includes("start_background_share"));
});

test("Mac start owns one permission wait and the normal toggle cancels it", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.background.server.permissions = [
    { kind: "screen_recording", status: "missing", requiredFor: "screen_capture" },
    { kind: "accessibility", status: "missing", requiredFor: "input_injection" },
  ];
  await f.state.startLocalVncServer();
  assert.equal(f.state.localShareStartPhase.value, "awaiting_permission");
  assert.equal(f.state.localVncServerStartInputLocked.value, true);
  assert.equal(f.state.localVncServerCanToggle.value, true);
  await f.state.startLocalVncServer();
  assert.equal(f.calls.filter(call => call === "prepare_background_share").length, 1);
  await f.state.toggleLocalVncServer();
  assert.equal(f.state.localShareStartPhase.value, "idle");
  assert.equal(f.state.localVncServerStartInputLocked.value, false);
  assert.ok(!f.calls.includes("start_background_share"));
});

test("installed Windows service is the default and start/stop uses its real instance", async () => {
  const f = await fixture();
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.equal(f.state.localVncServerDisplayPort.value, 5904);
  await f.state.startLocalVncServer();
  assert.equal(f.state.localVncServerCanStart.value, false);
  assert.equal(f.state.localVncServerOverview.value?.status, "running");
  await f.state.stopLocalVncServer();
  assert.equal(f.state.localVncServerCanStart.value, true);
  assert.equal(f.state.localVncServerOverview.value?.status, "stopped");
  assert.ok(f.calls.includes("start_windows_service_with_fallback"));
  assert.ok(f.calls.includes("stop_windows_service"));
  assert.ok(!f.calls.includes("start_local_vnc_server"));
});

test("an already running user background instance remains selected over an idle service", async () => {
  const f = await fixture("stopped", "running");
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.sharingMode.value, "background");
  assert.equal(f.state.localVncServerPort.value, 5907);
  await f.state.stopLocalVncServer();
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.ok(!f.calls.includes("start_background_share"));
});

test("stopping sharing also closes a legacy background listener beside the running service", async () => {
  const f = await fixture("running", "running");
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.sharingMode.value, "windowsService");
  await f.state.stopLocalVncServer();
  assert.equal(f.state.localVncServerOverview.value?.status, "stopped");
  assert.equal(f.background.server.status, "stopped");
  assert.ok(f.calls.includes("stop_windows_service"));
  assert.ok(f.calls.includes("stop_background_share"));
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
});

test("stopping sharing reminds about enabled automatic startup only after a successful stop", async () => {
  for (const serviceMode of [false, true]) {
    for (const autoStart of [false, true]) {
      const f = await fixture(serviceMode ? "running" : "not_installed", serviceMode ? "stopped" : "running", serviceMode, serviceMode);
      f.service.autoStart = autoStart;
      f.background.autoStart = autoStart;
      await f.state.refreshLocalVncServerStatus();
      await f.state.stopLocalVncServer();
      const hint = `localShare.${serviceMode ? "windowsService" : "background"}.autoStartHint`;
      assert.deepEqual(f.notifications, autoStart ? [hint] : []);
      assert.equal(f.service.autoStart, autoStart);
      assert.equal(f.background.autoStart, autoStart);
    }
  }
  const f = await fixture("not_installed", "running", false, false);
  await f.state.refreshLocalVncServerStatus();
  f.failBackgroundStop("background_share_runtime_failed");
  await f.state.stopLocalVncServer();
  assert.ok(!f.notifications.includes("localShare.background.autoStartHint"));
});

test("leftover startup entries toast once in visible sharing settings and rearm after recovery", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.background.autoStart = false;
  f.background.startupRegistration = "enabled";
  await f.state.refreshSharingSettings();
  const active = ref(false);
  const messages: string[] = [];
  const scope = effectScope();
  scope.run(() => useSharingStartupSettingsFeedback({
    active, sharingMode: f.state.sharingMode, backgroundShareOverview: f.state.backgroundShareOverview,
    t: key => key, notify: content => { messages.push(content); },
  }));
  try {
    assert.deepEqual(messages, []);
    active.value = true;
    await nextTick();
    assert.deepEqual(messages, ["localShare.background.registration.leftover"]);
    await f.state.refreshSharingSettings();
    active.value = false;
    await nextTick();
    active.value = true;
    await nextTick();
    assert.equal(messages.length, 1);
    f.background.startupRegistration = "missing";
    await f.state.refreshSharingSettings();
    f.background.startupRegistration = "enabled";
    await f.state.refreshSharingSettings();
    assert.equal(messages.length, 2);
    f.background.autoStart = true;
    await f.state.refreshSharingSettings();
    assert.equal(messages.length, 2);
  } finally { scope.stop(); }
});

test("unknown runtime and registration checks do not rearm the same leftover startup warning", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.background.autoStart = false;
  f.background.startupRegistration = "enabled";
  await f.state.refreshSharingSettings();
  const messages: string[] = [];
  const scope = effectScope();
  scope.run(() => useSharingStartupSettingsFeedback({
    active: ref(true), sharingMode: f.state.sharingMode, backgroundShareOverview: f.state.backgroundShareOverview,
    t: key => key, notify: content => { messages.push(content); },
  }));
  try {
    assert.equal(messages.length, 1);
    for (let attempt = 0; attempt < 3; attempt++) {
      f.failBackgroundStatus("background_share_access_denied:peer_path_mismatch");
      await f.state.refreshSharingSettings();
      assert.equal(f.state.sharingMode.value, null);
      f.failBackgroundStatus(null);
      await f.state.refreshSharingSettings();
      assert.equal(messages.length, 1, "a failed identity check is not a confirmed startup recovery");
      f.background.startupRegistration = "unknown";
      await f.state.refreshSharingSettings();
      f.background.startupRegistration = "enabled";
      await f.state.refreshSharingSettings();
      assert.equal(messages.length, 1, "an unknown registration does not rearm the warning");
    }
    f.background.startupRegistration = "missing";
    await f.state.refreshSharingSettings();
    f.background.startupRegistration = "enabled";
    await f.state.refreshSharingSettings();
    assert.equal(messages.length, 2, "confirmed removal still rearms a later leftover startup entry");
  } finally { scope.stop(); }
});

test("background automatic startup saves independently without starting or overwriting the form", async () => {
  const f = await fixture("not_installed", "stopped", false);
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.localVncServerPort.value, 5907);
  assert.equal(f.state.localVncServerCanStart.value, true);
  f.state.localVncServerPort.value = 5911;
  f.state.localVncServerBindAddress.value = "127.0.0.1";
  f.state.localVncServerViewOnly.value = true;
  f.state.localVncServerPassword.value = crypto.randomUUID().slice(0, 8);
  const password = f.state.localVncServerPassword.value;
  assert.equal(f.state.sharingAutoStart.value, true);
  assert.equal(f.state.sharingAutoStartKnown.value, true);
  await f.state.setSharingAutoStart(false);
  assert.equal(f.state.sharingAutoStart.value, false);
  assert.equal(f.background.autoStart, false);
  assert.equal(f.state.localVncServerPort.value, 5911);
  assert.equal(f.state.localVncServerBindAddress.value, "127.0.0.1");
  assert.equal(f.state.localVncServerViewOnly.value, true);
  assert.equal(f.state.localVncServerPassword.value, password);
  await f.state.setSharingAutoStart(true);
  assert.equal(f.state.sharingAutoStart.value, true);
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
  assert.ok(!f.calls.includes("prepare_background_share"));
  await f.state.startLocalVncServer();
  assert.equal(f.background.autoStart, true);
  await f.state.requestLocalVncServerPermission({ kind: "screen_recording", status: "missing", requiredFor: "screen_capture" });
  assert.ok(f.calls.includes("request_background_share_permission"));
  assert.ok(!f.calls.includes("request_local_vnc_server_permission"));
});

test("background startup switch reflects system registration instead of saved intent alone", async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.background.startupRegistration = "disabled";
  await f.state.refreshSharingSettings();
  assert.equal(f.background.autoStart, true);
  assert.equal(f.state.sharingAutoStart.value, false);
  await f.state.setSharingAutoStart(true);
  assert.equal(f.state.sharingAutoStart.value, false, "system denial remains visible after saving the policy");
  f.background.startupRegistration = "missing";
  await f.state.refreshSharingSettings();
  assert.equal(f.state.sharingAutoStart.value, false);
  f.background.startupRegistration = "enabled";
  await f.state.refreshSharingSettings();
  assert.equal(f.state.sharingAutoStart.value, true);
});

test("both background platforms save passwords by default and retain saved credentials when stopping", async () => {
  for (const supported of [false, true]) {
    const f = await fixture("not_installed", "stopped", false, supported);
    f.background.credentialConfigured = false;
    const password = crypto.randomUUID().slice(0, 8);
    await f.state.refreshLocalVncServerStatus();
    f.state.localVncServerPassword.value = password;
    await f.state.startLocalVncServer();
    assert.equal(f.backgroundStarts[0]?.password, password);
    assert.equal(f.state.localVncServerPassword.value, "");
    await f.state.stopLocalVncServer();
    assert.equal(f.state.backgroundShareOverview.value?.credentialConfigured, true);
    assert.equal(f.state.sharingAutoStart.value, true);
    await f.state.startLocalVncServer();
    assert.equal(f.backgroundStarts[1]?.password, "");
  }
});

test("automatic startup requires a persisted credential, never a password draft", async () => {
  for (const supported of [false, true]) {
    const f = await fixture(supported ? "stopped" : "not_installed", "stopped", supported, supported);
    f.service.credentialConfigured = false;
    f.background.credentialConfigured = false;
    f.background.autoStart = false;
    await f.state.refreshLocalVncServerStatus();
    f.state.localVncServerPassword.value = crypto.randomUUID().slice(0, 8);
    f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
    await f.state.setSharingAutoStart(true);
    assert.equal(f.state.sharingAutoStart.value, false);
    assert.ok(!f.calls.includes("set_background_share_auto_start"));
    assert.ok(!f.calls.includes("set_windows_service_auto_start"));
  }
});

test("automatic startup serializes actions and leaves saved state unchanged after failure", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  await f.state.refreshLocalVncServerStatus();
  let release!: () => void;
  f.holdAutoStart(new Promise(resolve => { release = resolve; }));
  const change = f.state.setSharingAutoStart(false);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.sharingAutoStartBusy.value, true);
  await f.state.setSharingAutoStart(true);
  await f.state.startLocalVncServer();
  await f.state.stopLocalVncServer();
  await f.state.refreshSharingSettings();
  assert.equal(f.calls.filter(call => call === "set_background_share_auto_start").length, 1);
  release();
  await change;
  assert.equal(f.state.sharingAutoStartBusy.value, false);
  f.failAutoStart("background_share_startup_unavailable");
  await f.state.setSharingAutoStart(true);
  assert.equal(f.state.sharingAutoStart.value, false);
  assert.equal(f.state.sharingAutoStartBusy.value, false);
  assert.equal(f.notifications.at(-1), "localShare.background.errors.startup_unavailable");
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
});

test("automatic startup waits for an active status refresh before saving", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  await f.state.refreshSharingSettings();
  let release!: (overview: BackgroundShareOverview) => void;
  f.holdStatus(new Promise(resolve => { release = resolve; }));
  const refresh = f.state.refreshSharingSettings();
  const change = f.state.setSharingAutoStart(false);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.sharingAutoStartBusy.value, true);
  assert.ok(!f.calls.includes("set_background_share_auto_start"));
  release(f.background);
  await Promise.all([refresh, change]);
  assert.equal(f.state.sharingAutoStart.value, false);
  assert.equal(f.calls.filter(call => call === "set_background_share_auto_start").length, 1);
});

test("unknown SCM startup state blocks changes while missing credentials still allow disabling", async () => {
  const f = await fixture();
  f.service.autoStart = null;
  await f.state.refreshSharingSettings();
  assert.equal(f.state.sharingAutoStartKnown.value, false);
  await f.state.setSharingAutoStart(false);
  assert.ok(!f.calls.includes("set_windows_service_auto_start"));
  f.service.autoStart = true;
  f.service.credentialConfigured = false;
  await f.state.refreshSharingSettings();
  await f.state.setSharingAutoStart(false);
  assert.equal(f.state.sharingAutoStart.value, false);
  assert.ok(f.calls.includes("set_windows_service_auto_start"));
});

test("automatic startup cannot change during a permission startup intent", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  f.background.server.permissions = [
    { kind: "screen_recording", status: "missing", requiredFor: "screen_capture" },
    { kind: "accessibility", status: "missing", requiredFor: "input_injection" },
  ];
  await f.state.startLocalVncServer();
  try {
    assert.equal(f.state.localShareStartPending.value, true);
    await f.state.setSharingAutoStart(false);
    assert.ok(!f.calls.includes("set_background_share_auto_start"));
  } finally { f.state.cancelLocalShareStart(); }
});

test("settings refresh reads runtime only and does not prepare an absent Mac host", async () => {
  const f = await fixture("not_installed", "stopped", false, false);
  assert.equal(f.state.sharingAutoStartKnown.value, false);
  await f.state.refreshSharingSettings();
  assert.equal(f.state.sharingAutoStartKnown.value, true);
  assert.deepEqual(f.permissionInspections, [false]);
  assert.ok(!f.calls.includes("prepare_background_share"));
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
});

test("Windows service automatic startup uses SCM and preserves the current shared instance", async () => {
  const f = await fixture("running");
  await f.state.refreshSharingSettings();
  await f.state.setSharingAutoStart(true);
  assert.equal(f.state.sharingAutoStart.value, true);
  assert.equal(f.service.status, "running");
  await f.state.setSharingAutoStart(false);
  assert.equal(f.state.sharingAutoStart.value, false);
  assert.equal(f.calls.filter(call => call === "set_windows_service_auto_start").length, 2);
  assert.ok(!f.calls.includes("set_background_share_auto_start"));
  assert.ok(!f.calls.some(call => call.startsWith("start_") || call.startsWith("stop_")));
});

test("simultaneous refreshes share one background status request", async () => {
  const f = await fixture("stopped", "running");
  await f.state.refreshLocalVncServerStatus();
  let release!: (value: BackgroundShareOverview) => void;
  f.holdStatus(new Promise(resolve => { release = resolve; }));
  f.calls.length = 0;
  const first = f.state.refreshLocalVncServerStatus();
  const second = f.state.refreshLocalVncServerStatus();
  release(f.background);
  await Promise.all([first, second]);
  assert.deepEqual(f.calls, ["get_windows_service_status", "get_background_share_status"]);
});

test("starting before the default runtime is resolved cannot create a temporary server", async () => {
  const f = await fixture();
  assert.equal(f.state.localVncServerCanToggle.value, false);
  await f.state.startLocalVncServer();
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.ok(f.calls.includes("start_windows_service_with_fallback"));
  assert.ok(!f.calls.includes("start_local_vnc_server"));
});

test("missing Windows service preserves background enhancement capabilities and starts its Agent runtime", async () => {
  const f = await fixture("not_installed", "stopped", false);
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.sharingMode.value, "background");
  assert.deepEqual(f.state.localVncServerOverview.value?.agent.capabilities, f.background.server.agent.capabilities);
  await f.state.startLocalVncServer();
  assert.ok(f.calls.includes("start_background_share"));
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  assert.ok(!f.calls.includes("start_local_vnc_server"));
});

test("Mac uses background sharing without consulting a persisted temporary selection", async () => {
  const f = await fixture("unsupported", "stopped", false, false);
  await f.state.startLocalVncServer();
  assert.equal(f.state.sharingMode.value, "background");
  assert.ok(f.calls.includes("start_background_share"));
});

test("service detection errors and failed services cannot start a background replacement", async () => {
  const f = await fixture("unknown");
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.sharingMode.value, "windowsService");
  f.failStatus("windows_service_access_denied");
  await f.state.startLocalVncServer();
  assert.equal(f.state.localVncServerCanToggle.value, false);
  assert.ok(!f.calls.includes("start_background_share"));
});

test("settings installation and uninstall refresh daily sharing routing without starting either instance", async () => {
  const f = await fixture("stopped", "stopped", false);
  await f.state.refreshLocalVncServerStatus();
  await f.state.manageSharingService("install");
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.equal(f.confirmDialog.value.visible, false);
  await f.state.manageSharingService("enableAutoStart");
  assert.equal(f.state.windowsService.overview.value?.autoStart, true);
  await f.state.manageSharingService("uninstall");
  assert.equal(f.state.sharingMode.value, "background");
  assert.equal(f.state.localVncServerPort.value, 5907);
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
});

test("cancelling service installation leaves the active background sharing untouched", async () => {
  const f = await fixture("not_installed", "running", false);
  await f.state.refreshLocalVncServerStatus();
  const installing = f.state.manageSharingService("install");
  await nextTick();
  assert.equal(f.confirmDialog.value.visible, true);
  assert.equal(f.confirmDialog.value.title, "localShare.windowsService.stopAndInstallTitle");
  assert.ok(!f.calls.includes("stop_background_share"));
  assert.ok(!f.calls.includes("install_windows_service"));
  f.resolveConfirm("secondary");
  await installing;
  assert.ok(!f.calls.includes("stop_background_share"));
  assert.ok(!f.calls.includes("install_windows_service"));
  assert.equal(f.background.server.status, "running");
  assert.equal(f.state.sharingMode.value, "background");
  assert.equal(f.state.localVncServerLoading.value, false);
});

test("initial service setup uses one command then daily sharing reuses its configuration", async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.service.bindAddress = null;
  f.service.port = null;
  f.service.credentialConfigured = false;
  f.service.passwordManagementAuthorized = false;
  await f.state.refreshLocalVncServerStatus();
  f.state.windowsService.bindAddress.value = "127.0.0.1";
  f.state.windowsService.port.value = 5913;
  f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
  const secret = f.state.windowsService.password.value;
  await f.state.manageSharingService("install");
  assert.deepEqual(f.installations, [{ bindAddress: "127.0.0.1", port: 5913, viewOnly: false, password: secret }]);
  assert.equal(f.installationRequests[0]?.password, "");
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.service.status, "stopped");
  assert.equal(f.service.passwordManagementAuthorized, true);
  assert.ok(!f.calls.includes("configure_windows_service"));
  assert.ok(!f.calls.includes("authorize_windows_service_password_management"));
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  await f.state.startLocalVncServer();
  await f.state.stopLocalVncServer();
  await f.state.startLocalVncServer();
  assert.equal(f.calls.filter(call => call === "start_windows_service_with_fallback").length, 2);
  assert.ok(!f.calls.includes("configure_windows_service"));
  assert.ok(!f.calls.includes("set_windows_service_password"));
});

test("entering service installation only selects the existing form and cancel preserves background sharing", async () => {
  const f = await fixture("not_installed", "running", false);
  await f.state.refreshLocalVncServerStatus();
  f.state.localVncServerPassword.value = "fixture";
  f.state.beginWindowsServiceSetup();
  assert.equal(f.state.localShareServiceSetup.value, true);
  assert.equal(f.state.localVncServerPassword.value, "");
  assert.equal(f.state.windowsService.password.value, "fixture");
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.some(call => call.startsWith("install_") || call.startsWith("stop_") || call.startsWith("start_")));
  f.state.windowsService.password.value = "fixture";
  f.state.cancelWindowsServiceSetup();
  assert.equal(f.state.localShareServiceSetup.value, false);
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.state.sharingMode.value, "background");
  assert.equal(f.background.server.status, "running");
});

test("sharing method stays in one form and retains all live connection sections", async () => {
  const f = await fixture("not_installed", "stopped", false);
  await f.state.refreshLocalVncServerStatus();
  const logger = createLogger("error");
  const report = logger.error.bind(logger);
  logger.error = (message, options) => {
    if (message.startsWith("WebSocket server error:") && message.includes("listen EPERM")) return;
    report(message, options);
  };
  const vite = await createServer({ customLogger: logger, optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false } });
  try {
    const { default: Main } = await vite.ssrLoadModule("/src/features/local-share/components/LocalShareWorkbench.vue");
    const { provideVncAppContext } = await vite.ssrLoadModule("/src/app/composables/useVncAppContext.ts");
    const { h } = await import("vue");
    const { NMessageProvider } = await import("naive-ui");
    const render = () => renderToString(createSSRApp({ setup() {
      provideVncAppContext({ ...f.state, activeWorkspace: ref("localShare"), inspectLocalSharePermissions: async () => {}, t: (key: string) => key });
      return () => h(NMessageProvider, null, { default: () => h(Main) });
    } }));
    const normal = await render();
    f.state.beginWindowsServiceSetup();
    const preparing = await render();
    for (const html of [normal, preparing]) {
      for (const section of ["local-share-addresses", "local-share-connections", "local-share-permissions", "local-share-connection-details"]) assert.ok(html.includes(section), section);
      assert.match(html, /localShare.connectionPort: 5907/, "live background port remains independent of the service draft");
    }
    const normalHeader = normal.match(/<header[\s\S]*?<\/header>/)?.[0] ?? "";
    const setupHeader = preparing.match(/<header[\s\S]*?<\/header>/)?.[0] ?? "";
    assert.ok(!normalHeader.includes("localShare.windowsService.installSettingsHint"));
    assert.ok(!setupHeader.includes("localShare.windowsService.exitInstallation"));
    assert.ok(setupHeader.includes("localShare.windowsService.installAndStart"));
    assert.ok(!setupHeader.includes("localShare.windowsService.preparingInstallation"));
    for (const html of [normal, preparing]) {
      assert.ok(!html.includes("local-share-method"));
      assert.ok(!html.includes("localShare.windowsService.sharingMethod"));
    }
    assert.ok(!preparing.slice(setupHeader.length + preparing.indexOf(setupHeader)).includes("localShare.windowsService.exitInstallation"));
  } finally { await vite.close(); }
});

test("the main sharing action installs and starts once using the service form", async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.service.bindAddress = null; f.service.port = null; f.service.credentialConfigured = false;
  await f.state.refreshLocalVncServerStatus();
  f.state.beginWindowsServiceSetup();
  f.state.windowsService.port.value = 5914;
  f.state.windowsService.password.value = "fixture";
  assert.equal(f.state.localVncServerToggleLabelKey.value, "localShare.windowsService.installAndStart");
  await f.state.toggleLocalVncServer();
  assert.equal(f.installations.length, 1);
  assert.equal(f.installations[0]?.port, 5914);
  assert.equal(f.calls.filter(call => call === "start_windows_service_with_fallback").length, 1);
  assert.ok(f.calls.indexOf("install_windows_service") < f.calls.indexOf("start_windows_service_with_fallback"));
  assert.ok(!f.calls.includes("configure_windows_service"));
  assert.ok(!f.calls.includes("start_background_share"));
  assert.equal(f.state.localShareServiceSetup.value, false);
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.equal(f.state.windowsService.password.value, "");
});

test("the installation form survives the read-only status refresh triggered by navigation", async () => {
  const f = await fixture("not_installed", "stopped", false);
  await f.state.refreshLocalVncServerStatus();
  let release!: (value: BackgroundShareOverview) => void;
  f.holdStatus(new Promise(resolve => { release = resolve; }));
  const refreshing = f.state.inspectLocalSharePermissions();
  f.state.beginWindowsServiceSetup();
  assert.equal(f.state.localShareServiceSetup.value, true);
  release(f.background);
  await refreshing;
  assert.equal(f.state.localShareServiceSetup.value, true);
  assert.ok(!f.calls.includes("install_windows_service"));
});

test("leaving the main form during installation cannot start a late listener", async () => {
  const f = await fixture("not_installed", "running", false);
  await f.state.refreshLocalVncServerStatus();
  f.state.beginWindowsServiceSetup();
  f.state.windowsService.password.value = "fixture";
  const pending = f.state.startLocalVncServer();
  await nextTick();
  assert.equal(f.confirmDialog.value.visible, true);
  f.state.cancelWindowsServiceSetup();
  f.resolveConfirm("primary");
  await pending;
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.includes("install_windows_service"));
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  assert.ok(!f.calls.includes("stop_background_share"));
});

test("initializing a registered service confirms stopping active background sharing only on submit", async () => {
  const f = await fixture("stopped", "running", true);
  f.service.bindAddress = null; f.service.port = null; f.service.credentialConfigured = false;
  await f.state.refreshLocalVncServerStatus();
  f.state.beginWindowsServiceSetup();
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.includes("stop_background_share"));
  f.state.windowsService.password.value = "fixture";
  const pending = f.state.startLocalVncServer();
  await nextTick();
  assert.equal(f.confirmDialog.value.visible, true);
  f.resolveConfirm("primary");
  await pending;
  assert.equal(f.service.status, "running");
  assert.equal(f.background.server.status, "stopped");
  assert.ok(f.calls.indexOf("install_windows_service") < f.calls.indexOf("stop_background_share"));
});

test("initial setup validates before elevation and clears secrets on cancellation", async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.service.bindAddress = null;
  f.service.port = null;
  f.service.credentialConfigured = false;
  await f.state.refreshLocalVncServerStatus();
  await f.state.manageSharingService("install");
  assert.ok(!f.calls.includes("install_windows_service"));
  f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
  f.failInstall("windows_service_uac_cancelled");
  await f.state.manageSharingService("install");
  assert.equal(f.installations.length, 1);
  assert.equal(f.installationRequests[0]?.password, "");
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.service.installed, false);
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
});

test("confirmed service installation waits for background sharing to stop and installs without starting", async () => {
  const f = await fixture("not_installed", "running", false);
  await f.state.refreshLocalVncServerStatus();
  const installing = f.state.manageSharingService("install");
  await nextTick();
  assert.equal(f.confirmDialog.value.visible, true);
  assert.equal(f.confirmDialog.value.primaryLabel, "localShare.windowsService.stopAndInstall");
  const confirmation = f.confirmDialog.value;
  await f.state.manageSharingService("install");
  assert.equal(f.confirmDialog.value, confirmation);

  let finishStop!: () => void;
  f.holdBackgroundStop(new Promise<void>(resolve => { finishStop = resolve; }));
  f.resolveConfirm("primary");
  await nextTick();
  assert.equal(f.state.localVncServerLoading.value, true);
  assert.ok(f.calls.includes("stop_background_share"));
  assert.ok(!f.calls.includes("install_windows_service"));
  await f.state.manageSharingService("install");
  finishStop();
  await installing;

  assert.equal(f.calls.filter(call => call === "stop_background_share").length, 1);
  assert.equal(f.calls.filter(call => call === "install_windows_service").length, 1);
  assert.ok(f.calls.indexOf("stop_background_share") < f.calls.indexOf("install_windows_service"));
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
  assert.equal(f.background.server.status, "stopped");
  assert.equal(f.background.credentialConfigured, true);
  assert.equal(f.background.autoStart, true);
  assert.equal(f.service.status, "stopped");
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.equal(f.state.localVncServerLoading.value, false);
});

test("a failed or incomplete background stop never proceeds to service installation", async () => {
  for (const failure of ["rejected", "running", "stopping"] as const) {
    const f = await fixture("not_installed", "running", false);
    await f.state.refreshLocalVncServerStatus();
    if (failure === "rejected") f.failBackgroundStop("background_share_ipc_busy");
    else f.setStoppedBackgroundStatus(failure);
    const installing = f.state.manageSharingService("install");
    await nextTick();
    assert.equal(f.confirmDialog.value.visible, true);
    f.resolveConfirm("primary");
    await installing;
    assert.ok(f.calls.includes("stop_background_share"));
    assert.ok(!f.calls.includes("install_windows_service"));
    assert.equal(f.state.localVncServerLoading.value, false);
    assert.equal(f.notifications.at(-1), failure === "rejected"
      ? "localShare.background.errors.ipc_busy" : "localShare.windowsService.stopBeforeInstallFailed");
  }
});

test("an installation cancellation after stopping never restarts background sharing", async () => {
  const f = await fixture("not_installed", "running", false);
  await f.state.refreshLocalVncServerStatus();
  f.failInstall("windows_service_uac_cancelled");
  const installing = f.state.manageSharingService("install");
  await nextTick();
  assert.equal(f.confirmDialog.value.visible, true);
  f.resolveConfirm("primary");
  await installing;
  assert.ok(f.calls.includes("stop_background_share"));
  assert.ok(f.calls.includes("install_windows_service"));
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
  assert.equal(f.background.server.status, "stopped");
  assert.equal(f.service.installed, false);
  assert.equal(f.notifications.at(-1), "localShare.windowsService.errors.uac_cancelled");
});

test("updating an installed service still requires the active background instance to be stopped first", async () => {
  const f = await fixture("stopped", "running", true);
  await f.state.refreshLocalVncServerStatus();
  await f.state.manageSharingService("install");
  assert.equal(f.confirmDialog.value.visible, false);
  assert.ok(!f.calls.includes("stop_background_share"));
  assert.ok(!f.calls.includes("install_windows_service"));
  assert.equal(f.notifications.at(-1), "localShare.windowsService.stopBackgroundFirst");
});

test("Windows start saves changed settings with an empty password before starting and preserves credentials", async () => {
  const f = await fixture();
  await f.state.refreshLocalVncServerStatus();
  f.state.windowsService.port.value = 5911;
  f.state.windowsService.bindAddress.value = "127.0.0.1";
  f.state.windowsService.viewOnly.value = true;
  assert.equal(f.state.windowsService.canStart.value, true);
  await f.state.startLocalVncServer();
  assert.deepEqual(f.configurations, [{ bindAddress: "127.0.0.1", port: 5911, viewOnly: true, password: "" }]);
  assert.equal(f.state.windowsService.overview.value?.credentialConfigured, true);
  assert.equal(f.service.status, "running");
  assert.ok(f.calls.indexOf("configure_windows_service") < f.calls.indexOf("start_windows_service_with_fallback"));
  assert.ok(!f.calls.includes("set_windows_service_password"));
});

test("the first Windows start initializes and authorizes once before starting", async () => {
  const f = await fixture();
  f.service.credentialConfigured = false;
  f.service.bindAddress = null;
  f.service.port = null;
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.windowsService.canStart.value, false);
  await f.state.startLocalVncServer();
  assert.equal(f.configurations.length, 0);
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  const password = crypto.randomUUID().slice(0, 8);
  f.state.windowsService.password.value = password;
  f.state.windowsService.port.value = 5912;
  assert.equal(f.state.windowsService.canStart.value, true);
  assert.equal(f.state.localVncServerCanToggle.value, true);
  await f.state.startLocalVncServer();
  assert.equal(f.installations[0]?.password, password);
  assert.equal(f.service.port, 5912);
  assert.equal(f.state.windowsService.overview.value?.credentialConfigured, true);
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.installationRequests[0]?.password, "");
  assert.equal(f.configurations.length, 0);
  assert.ok(f.calls.indexOf("install_windows_service") < f.calls.indexOf("start_windows_service_with_fallback"));
  assert.equal(f.service.status, "running");
  assert.ok(f.calls.includes("start_windows_service_with_fallback"));
});

test("cancelled configuration authorization clears transient input and never starts Windows sharing", async () => {
  const f = await fixture();
  await f.state.refreshLocalVncServerStatus();
  f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
  f.state.windowsService.port.value = 5913;
  f.failConfigure("windows_service_uac_cancelled");
  await f.state.startLocalVncServer();
  assert.equal(f.state.windowsService.messageTone.value, "info");
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.configurationRequests[0]?.password, "");
  assert.equal(f.state.windowsService.overview.value?.credentialConfigured, true);
  assert.equal(f.service.port, 5904);
  assert.ok(!f.calls.some(call => call.startsWith("start_")));
});

test("Windows start cannot save an overlong UTF-8 password or change a running service", async () => {
  const f = await fixture();
  await f.state.refreshLocalVncServerStatus();
  f.state.windowsService.password.value = "界".repeat(3);
  assert.equal(f.state.windowsService.canStart.value, false);
  await f.state.startLocalVncServer();
  assert.equal(f.state.windowsService.message.value, "localShare.windowsService.errors.invalid_config");
  f.state.windowsService.password.value = "";
  f.service.status = "running";
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.windowsService.canStart.value, false);
  await f.state.windowsService.act("start");
  assert.equal(f.configurations.length, 0);
  assert.equal(f.passwordChanges.length, 0);
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
});

test("Windows start with unchanged settings reuses saved credentials without saving anything", async () => {
  const f = await fixture();
  await f.state.refreshSharingSettings();
  await f.state.startLocalVncServer();
  assert.equal(f.service.status, "running");
  assert.equal(f.configurations.length, 0);
  assert.equal(f.passwordChanges.length, 0);
});

test("authorized Windows password-only changes use the narrow password API before starting", async () => {
  const f = await fixture();
  await f.state.refreshSharingSettings();
  const password = crypto.randomUUID().slice(0, 8);
  f.state.windowsService.password.value = password;
  await f.state.startLocalVncServer();
  assert.deepEqual(f.passwordChanges, [password]);
  assert.equal(f.configurations.length, 0);
  assert.equal(f.service.status, "running");
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.passwordRequests[0]?.password, "");
  assert.ok(f.calls.indexOf("set_windows_service_password") < f.calls.indexOf("start_windows_service_with_fallback"));
});

test("an authorized account repairs missing Windows credentials without rewriting saved listener configuration", async () => {
  const f = await fixture();
  f.service.credentialConfigured = false;
  await f.state.refreshSharingSettings();
  assert.equal(f.state.windowsService.canStart.value, false);
  f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
  assert.equal(f.state.windowsService.canStart.value, true);
  await f.state.startLocalVncServer();
  assert.equal(f.configurations.length, 0);
  assert.equal(f.passwordChanges.length, 1);
  assert.equal(f.state.windowsService.overview.value?.credentialConfigured, true);
  assert.equal(f.service.status, "running");
  assert.equal(f.state.windowsService.password.value, "");
});

test("Windows password-only changes require explicit authorization and never fall back to admin configuration", async () => {
  const f = await fixture();
  f.service.passwordManagementAuthorized = false;
  await f.state.refreshSharingSettings();
  f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
  await f.state.startLocalVncServer();
  assert.equal(f.service.status, "stopped");
  assert.equal(f.configurations.length, 0);
  assert.equal(f.passwordChanges.length, 0);
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.state.windowsService.message.value, "localShare.windowsService.errors.password_management_authorization_required");
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  assert.ok(!f.calls.includes("authorize_windows_service_password_management"));
});

test("a rejected Windows password save never starts sharing and clears all transient material", async () => {
  const f = await fixture();
  await f.state.refreshSharingSettings();
  f.failPassword("windows_service_password_management_authorization_required");
  f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
  await f.state.startLocalVncServer();
  assert.equal(f.service.status, "stopped");
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.passwordRequests[0]?.password, "");
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
});

test("Windows clears transient passwords before awaiting a failing service start", async () => {
  const f = await fixture();
  await f.state.refreshSharingSettings();
  f.state.windowsService.port.value = 5917;
  f.state.windowsService.password.value = crypto.randomUUID().slice(0, 8);
  let release!: () => void;
  f.holdServiceStart(new Promise(resolve => { release = resolve; }));
  f.failServiceStart("windows_service_start_failed");
  const start = f.state.startLocalVncServer();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.configurationRequests[0]?.password, "");
  release();
  await start;
  assert.equal(f.service.status, "stopped");
  assert.equal(f.state.windowsService.message.value, "localShare.windowsService.errors.start_failed");
});

test("granting Windows password management refreshes authorization without overwriting draft settings", async () => {
  const f = await fixture();
  f.service.passwordManagementAuthorized = false;
  await f.state.refreshSharingSettings();
  f.state.windowsService.port.value = 5914;
  f.state.windowsService.bindAddress.value = "127.0.0.1";
  await f.state.manageSharingService("authorizePasswordManagement");
  assert.equal(f.state.windowsService.overview.value?.passwordManagementAuthorized, true);
  assert.equal(f.state.windowsService.port.value, 5914);
  assert.equal(f.state.windowsService.bindAddress.value, "127.0.0.1");
  assert.equal(f.configurations.length, 0);
  assert.equal(f.passwordChanges.length, 0);
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
});

test("leaving the page during a Windows configuration save cannot start a late listener", async () => {
  const f = await fixture();
  await f.state.refreshSharingSettings();
  f.state.windowsService.port.value = 5915;
  let release!: () => void;
  f.holdConfigure(new Promise(resolve => { release = resolve; }));
  const start = f.state.startLocalVncServer();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.windowsService.busy.value, true);
  f.state.cancelLocalShareStart();
  release();
  await start;
  assert.equal(f.state.localVncServerLoading.value, false);
  assert.equal(f.state.windowsService.busy.value, false);
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
});

test("Windows maintenance start uses the same save-before-start flow and cancellation gate", async () => {
  const f = await fixture();
  await f.state.refreshSharingSettings();
  f.state.windowsService.port.value = 5916;
  let release!: () => void;
  f.holdConfigure(new Promise(resolve => { release = resolve; }));
  const start = f.state.manageSharingService("start");
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.configurations[0]?.port, 5916);
  await f.state.manageSharingService("start");
  assert.equal(f.configurations.length, 1);
  f.state.cancelLocalShareStart();
  release();
  await start;
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
});


test("service fallback shows its cause, keeps autostart policy, and retries service on the next start", async () => {
  const f = await fixture();
  await f.state.refreshLocalVncServerStatus();
  const originalAutoStart = f.background.autoStart;
  f.state.windowsService.password.value = "fixture";
  const background = structuredClone(f.background);
  background.server = server("running", 5904);
  background.serviceFallback = true;
  f.setStartOutcome({ service: null, background, serviceError: "windows_service_worker_exited:exit_code=1067", fallbackError: null, passwordRequired: false });
  await f.state.startLocalVncServer();
  assert.equal(f.state.localShareFallbackActive.value, true);
  assert.equal(f.state.sharingMode.value, "background");
  assert.ok(f.notifications.includes("localShare.windowsService.fallbackSucceeded"));
  assert.equal(f.background.autoStart, originalAutoStart);
  assert.ok(!f.calls.includes("set_background_share_auto_start"));
  assert.equal(f.serviceStartRequests[0]?.password, "");
  await f.state.stopLocalVncServer();
  await f.state.startLocalVncServer();
  assert.equal(f.service.status, "running");
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.equal(f.calls.filter(call => call === "start_windows_service_with_fallback").length, 2);
  assert.ok(!f.calls.includes("start_background_share"));
});

test("missing service fallback password remains in the main form and both failures stay visible", async () => {
  const f = await fixture();
  await f.state.refreshLocalVncServerStatus();
  f.setStartOutcome({ service: null, background: null, serviceError: "windows_service_worker_exited", fallbackError: null, passwordRequired: true });
  await f.state.startLocalVncServer();
  assert.equal(f.state.sharingMode.value, "windowsService");
  assert.equal(f.state.localShareStartPhase.value, "idle");
  assert.ok(f.notifications.includes("localShare.windowsService.fallbackPasswordRequired"));
  assert.ok(!f.calls.includes("start_background_share"));
  f.setStartOutcome({ service: null, background: null, serviceError: "windows_service_start_failed", fallbackError: "background_share_port_conflict", passwordRequired: false });
  await f.state.startLocalVncServer();
  assert.ok(f.notifications.includes("localShare.windowsService.fallbackFailed"));
});


async function waitForDialog(f: Awaited<ReturnType<typeof fixture>>) {
  for (let tick = 0; tick < 100 && !f.confirmDialog.value.visible; tick++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.confirmDialog.value.visible, true, "confirmation dialog must become visible");
}

test("first start recommends service only after validation and direct sharing is remembered across app restarts", { timeout: 5000 }, async () => {
  const preferences = new Map<string, string>();
  const f = await fixture("not_installed", "stopped", false, true, false, preferences);
  await f.state.refreshLocalVncServerStatus();
  assert.equal(f.state.localVncServerCanStart.value, false, "tray start must open the page for the first decision");
  f.state.localVncServerPort.value = null;
  await f.state.startLocalVncServer();
  assert.equal(f.confirmDialog.value.visible, false);
  f.state.localVncServerPort.value = 5911;
  f.state.localVncServerPassword.value = "fixture";
  const pending = f.state.startLocalVncServer();
  await waitForDialog(f);
  assert.equal(f.confirmDialog.value.title, "localShare.windowsService.suggestionTitle");
  const dialog = f.confirmDialog.value;
  await f.state.startLocalVncServer();
  assert.equal(f.confirmDialog.value, dialog);
  f.resolveConfirm("secondary");
  await pending;
  assert.equal(f.backgroundStarts[0]?.password, "fixture");
  assert.equal(f.backgroundStarts[0]?.port, 5911);
  assert.equal(preferences.get(SERVICE_SUGGESTION_STORAGE_KEY), "handled");
  const reopened = await fixture("not_installed", "stopped", false, true, false, preferences);
  await reopened.state.startLocalVncServer();
  assert.equal(reopened.confirmDialog.value.visible, false);
  assert.equal(reopened.backgroundStarts.length, 1);
});

test("closing the recommendation cancels this start without remembering a sharing choice", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "stopped", false, true, false);
  const pending = f.state.startLocalVncServer();
  await waitForDialog(f);
  assert.equal(f.confirmDialog.value.dismissChoice, "cancel");
  f.resolveConfirm("cancel");
  await pending;
  assert.equal(f.preferences.has(SERVICE_SUGGESTION_STORAGE_KEY), false);
  assert.ok(!f.calls.includes("install_windows_service"));
  assert.equal(f.backgroundStarts.length, 0);
});

test("accepting the first recommendation installs and starts with the submitted draft", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "stopped", false, true, false);
  await f.state.refreshLocalVncServerStatus();
  f.state.localVncServerPort.value = 5916;
  f.state.localVncServerPassword.value = "fixture";
  const pending = f.state.startLocalVncServer();
  await waitForDialog(f);
  f.resolveConfirm("primary");
  await pending;
  assert.equal(f.installations[0]?.password, "fixture");
  assert.equal(f.installations[0]?.port, 5916);
  assert.equal(f.installationRequests[0]?.password, "");
  assert.equal(f.serviceStartRequests[0]?.password, "");
  assert.equal(f.service.status, "running");
  assert.equal(f.backgroundStarts.length, 0);
});

test("cancelled installation UAC starts ordinary sharing once and suppresses later recommendations", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "stopped", false, true, false);
  await f.state.refreshLocalVncServerStatus();
  f.state.localVncServerPassword.value = "fixture";
  f.failInstall("windows_service_uac_cancelled");
  const pending = f.state.startLocalVncServer();
  await waitForDialog(f);
  f.resolveConfirm("primary");
  await pending;
  assert.equal(f.backgroundStarts.length, 1);
  assert.equal(f.backgroundStarts[0]?.password, "fixture");
  assert.equal(f.state.localShareServiceSetup.value, false);
  assert.ok(f.notifications.includes("localShare.windowsService.installCancelledOrdinary"));
  await f.state.stopLocalVncServer();
  await f.state.startLocalVncServer();
  assert.equal(f.backgroundStarts.length, 2);
  assert.equal(f.confirmDialog.value.visible, false);
  assert.equal(f.installations.length, 1);
});

test("settings installation reuses saved credentials on the backend and starts without a second click", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.service.credentialConfigured = false;
  await f.state.requestWindowsServiceInstallation();
  assert.equal(f.installations.length, 1);
  assert.equal(f.installations[0]?.password, "");
  assert.deepEqual(f.installSavedPasswords, [true]);
  assert.deepEqual(f.startSavedPasswords, [true]);
  assert.equal(f.service.status, "running");
});

test("disabled UAC offers repair settings in one dialog without stopping ordinary sharing", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "running", false);
  f.failInstall("windows_service_uac_disabled");
  const pending = f.state.requestWindowsServiceInstallation();
  await waitForDialog(f);
  f.resolveConfirm("primary");
  await waitForDialog(f);
  assert.equal(f.confirmDialog.value.title, "localShare.windowsService.installNeedsAdminTitle");
  assert.equal(f.confirmDialog.value.message, "localShare.windowsService.installUacDisabledGuide");
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.installationRequests[0]?.password, "");
  assert.equal(f.repairSettingsOpened(), 0);
  assert.ok(!f.notifications.includes("localShare.windowsService.errors.uac_disabled"));
  f.resolveConfirm("primary");
  await pending;
  assert.equal(f.repairSettingsOpened(), 1);
  assert.equal(f.state.localShareServiceSetup.value, true);
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.includes("stop_background_share"));
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  assert.ok(!f.calls.includes("repair_windows_service"));
});

test("settings installation without a password waits in the original form", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.service.credentialConfigured = false;
  f.background.credentialConfigured = false;
  await f.state.requestWindowsServiceInstallation();
  assert.equal(f.state.localShareServiceSetup.value, true);
  assert.equal(f.state.localShareSetupPasswordRequired.value, true);
  assert.equal(f.installations.length, 0);
  f.state.windowsService.password.value = "fixture";
  await f.state.toggleLocalVncServer();
  assert.equal(f.service.status, "running");
  assert.deepEqual(f.installSavedPasswords, [false]);
});

test("cancelling installation authorization preserves an already running ordinary instance", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "running", false);
  f.failInstall("windows_service_uac_cancelled");
  const pending = f.state.requestWindowsServiceInstallation();
  await waitForDialog(f);
  assert.equal(f.confirmDialog.value.visible, true);
  f.resolveConfirm("primary");
  await pending;
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.includes("stop_background_share"));
  assert.equal(f.backgroundStarts.length, 0);
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  assert.ok(f.notifications.includes("localShare.windowsService.installCancelledOrdinary"));
});

test("installation completes before ordinary sharing is stopped for service takeover", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "running", false);
  let release!: () => void;
  f.holdInstall(new Promise(resolve => { release = resolve; }));
  const pending = f.state.requestWindowsServiceInstallation();
  await waitForDialog(f);
  f.resolveConfirm("primary");
  await nextTick();
  assert.ok(f.calls.includes("install_windows_service"));
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.includes("stop_background_share"));
  release();
  await pending;
  assert.ok(f.calls.indexOf("install_windows_service") < f.calls.indexOf("stop_background_share"));
  assert.ok(f.calls.indexOf("stop_background_share") < f.calls.indexOf("start_windows_service_with_fallback"));
  assert.equal(f.service.status, "running");
});

test("leaving during installation keeps ordinary sharing and prevents a late service start", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "running", false);
  let release!: () => void;
  f.holdInstall(new Promise(resolve => { release = resolve; }));
  const pending = f.state.requestWindowsServiceInstallation();
  await waitForDialog(f);
  f.resolveConfirm("primary");
  await nextTick();
  f.state.cancelLocalShareStart();
  release();
  await pending;
  assert.equal(f.service.installed, true);
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.includes("stop_background_share"));
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
});

test("other installation errors and registered partial services never silently start ordinary sharing", { timeout: 5000 }, async () => {
  for (const installed of [false, true]) {
    const f = await fixture("stopped", "stopped", installed);
    f.service.credentialConfigured = false;
    await f.state.refreshLocalVncServerStatus();
    if (installed) f.state.windowsService.password.value = "fixture";
    f.failInstall(installed ? "windows_service_uac_cancelled" : "windows_service_install_register_failed");
    await f.state.requestWindowsServiceInstallation();
    assert.equal(f.installations.length, 1);
    assert.equal(f.installations.length, 1);
    assert.equal(f.backgroundStarts.length, 0);
    assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  }
});

test("unavailable saved setup credentials request a password in the same form and can be repaired by typing one", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.failInstall("background_share_credential_missing");
  await f.state.requestWindowsServiceInstallation();
  assert.equal(f.state.localShareSetupPasswordRequired.value, true);
  assert.equal(f.state.localShareSetupSavedPassword.value, false);
  assert.equal(f.state.localShareServiceSetup.value, true);
  assert.equal(f.backgroundStarts.length, 0);
  f.failInstall(null);
  f.state.windowsService.password.value = "fixture";
  await f.state.toggleLocalVncServer();
  assert.equal(f.service.status, "running");
  assert.deepEqual(f.installSavedPasswords, [true, false]);
  assert.ok(!f.logEvents.some(event => event.includes("fixture")));
});

test("a failed ordinary stop after installation blocks service takeover", { timeout: 5000 }, async () => {
  const f = await fixture("not_installed", "running", false);
  f.failBackgroundStop("background_share_ipc_busy");
  const pending = f.state.requestWindowsServiceInstallation();
  await waitForDialog(f);
  f.resolveConfirm("primary");
  await pending;
  assert.equal(f.service.installed, true);
  assert.equal(f.service.status, "stopped");
  assert.equal(f.background.server.status, "running");
  assert.ok(!f.calls.includes("start_windows_service_with_fallback"));
  assert.equal(f.backgroundStarts.length, 0);
});


test("leaving password preparation cancels installation intent and preserves the common non-secret draft", async () => {
  const f = await fixture("not_installed", "stopped", false);
  f.service.credentialConfigured = false;
  f.background.credentialConfigured = false;
  await f.state.requestWindowsServiceInstallation();
  f.state.windowsService.port.value = 5919;
  f.state.windowsService.password.value = "fixture";
  f.state.cancelWindowsServiceSetup();
  assert.equal(f.state.localShareServiceSetup.value, false);
  assert.equal(f.state.localVncServerPort.value, 5919);
  assert.equal(f.state.windowsService.password.value, "");
  assert.equal(f.installations.length, 0);
});
