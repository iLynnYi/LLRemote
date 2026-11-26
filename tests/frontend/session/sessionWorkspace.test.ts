import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  selectWorkspaceSessions,
  summarizeWorkspaceRenderSlots,
  type SessionWorkspaceTab,
} from "../../../src/session/viewport/sessionWorkspace.ts";
import { zhCN } from "../../../src/i18n/messages/zh-CN.ts";

const tabs: SessionWorkspaceTab[] = [
  { id: "session-1", label: "10.0.0.1:5900", target: "10.0.0.1:5900", state: "connected", active: false, liveCanvas: true, closable: true },
  { id: "session-2", label: "10.0.0.2:5900", target: "10.0.0.2:5900", state: "failed", active: false, liveCanvas: false, closable: true },
  { id: "session-3", label: "10.0.0.3:5900", target: "10.0.0.3:5900", state: "probing", active: false, liveCanvas: false, closable: true },
  { id: "session-4", label: "10.0.0.4:5900", target: "10.0.0.4:5900", state: "connecting", active: false, liveCanvas: false, closable: true },
  { id: "session-5", label: "10.0.0.5:5900", target: "10.0.0.5:5900", state: "connected", active: false, liveCanvas: true, closable: true },
];

function appStructureSource(): string {
  return [
    "../../../src/App.vue",
    "../../../src/app/components/AppShell.vue",
    "../../../src/app/components/ModuleRail.vue",
    "../../../src/app/composables/useVncAppState.ts",
    "../../../src/app/composables/vncAppState/hostConnection.ts",
    "../../../src/app/composables/vncAppState/lifecycle.ts",
    "../../../src/app/composables/vncAppState/localDiagnostics.ts",
    "../../../src/app/composables/vncAppState/passwordPrompt.ts",
    "../../../src/app/composables/vncAppState/tray.ts",
    "../../../src/app/composables/vncAppState/utils.ts",
    "../../../src/app/composables/vncAppState/viewport.ts",
    "../../../src/app/styles.css",
    "../../../src/app/styles/base.css",
    "../../../src/app/styles/session-layout.css",
    "../../../src/app/styles/session-workbench.css",
    "../../../src/app/styles/session-tabs.css",
    "../../../src/app/styles/session-diagnostics.css",
    "../../../src/app/styles/session-viewport.css",
    "../../../src/features/sessions/components/SessionDiagnosticsPanel.vue",
    "../../../src/features/sessions/components/SessionExternalWindowPage.vue",
    "../../../src/features/sessions/components/sessionExternalWindowPage/feedback.ts",
    "../../../src/features/sessions/components/sessionExternalWindowPage/inputForwarding.ts",
    "../../../src/features/sessions/components/SessionTabs.vue",
    "../../../src/features/sessions/components/SessionToolbar.vue",
    "../../../src/features/sessions/components/SessionTopbar.vue",
    "../../../src/features/sessions/components/SessionViewportGrid.vue",
    "../../../src/features/sessions/components/sessionViewportGrid/inputForwarding.ts",
    "../../../src/features/sessions/components/SessionViewportChrome.vue",
    "../../../src/features/sessions/components/SessionToolbox.vue",
    "../../../src/features/sessions/components/sessionToolbox/menuOptions.ts",
    "../../../src/features/sessions/composables/useSessionAdvancedDiagnostics.ts",
    "../../../src/features/sessions/components/sessionToolbox/fileTransfer.ts",
    "../../../src/features/sessions/components/SessionWorkbench.vue",
    "../../../src/features/sessions/composables/useSessionDerivedState.ts",
    "../../../src/features/sessions/composables/useSessionExternalInputBridge.ts",
    "../../../src/features/sessions/composables/useSessionExternalWindow.ts",
    "../../../src/features/sessions/composables/useSessionExternalSurface.ts",
    "../../../src/features/sessions/composables/useSessionExternalWindowCoordinator.ts",
    "../../../src/features/sessions/composables/sessionExternalWindowRuntime.ts",
    "../../../src/features/sessions/composables/useSessionFullscreen.ts",
    "../../../src/features/sessions/composables/useSessionToolbox.ts",
    "../../../src/features/sessions/composables/useSessionClipboardReports.ts",
    "../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts",
    "../../../src/features/sessions/composables/sessionConnectionLifecycle/coreSync.ts",
    "../../../src/features/sessions/composables/sessionConnectionLifecycle/connectionIssues.ts",
    "../../../src/features/sessions/composables/sessionConnectionLifecycle/sessionDisplayEvents.ts",
    "../../../src/features/sessions/composables/useSessionDisplayControls.ts",
    "../../../src/features/sessions/composables/useSessionLocalServerControls.ts",
    "../../../src/session/external-window/sessionExternalWindow.ts",
    "../../../src/session/transfers/sessionToolbox.ts",
    "../../../src/features/settings/components/SettingsWorkbench.vue",
    "../../../src/features/settings/composables/useSettingsWorkbench.ts",
    "../../../src/features/transfers/components/GlobalTransferStatus.vue",
    "../../../src/features/transfers/components/GlobalTransferDrawer.vue",
  ]
    .map((path) => readFileSync(new URL(path, import.meta.url), "utf8"))
    .join("\n");
}

function sourceFile(path: string): string {
  if (path === "../../../src/features/sessions/components/SessionExternalWindowPage.vue") {
    return [
      path,
      "../../../src/features/sessions/components/sessionExternalWindowPage/feedback.ts",
      "../../../src/features/sessions/components/sessionExternalWindowPage/inputForwarding.ts",
      "../../../src/features/sessions/composables/useSessionExternalSurface.ts",
    ].map((item) => readFileSync(new URL(item, import.meta.url), "utf8")).join("\n");
  }
  if (path === "../../../src/features/sessions/components/SessionViewportGrid.vue") {
    return [
      path,
      "../../../src/features/sessions/components/sessionViewportGrid/inputForwarding.ts",
    ].map((item) => readFileSync(new URL(item, import.meta.url), "utf8")).join("\n");
  }
  if (path === "../../../src/app/composables/useVncAppState.ts") {
    return [
      path,
      "../../../src/app/composables/vncAppState/hostConnection.ts",
      "../../../src/app/composables/vncAppState/lifecycle.ts",
      "../../../src/app/composables/vncAppState/localDiagnostics.ts",
      "../../../src/app/composables/vncAppState/passwordPrompt.ts",
      "../../../src/app/composables/vncAppState/tray.ts",
      "../../../src/app/composables/vncAppState/utils.ts",
      "../../../src/app/composables/vncAppState/viewport.ts",
    ].map((item) => readFileSync(new URL(item, import.meta.url), "utf8")).join("\n");
  }
  if (path === "../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts") {
    return [
      path,
      "../../../src/features/sessions/composables/sessionConnectionLifecycle/coreSync.ts",
      "../../../src/features/sessions/composables/sessionConnectionLifecycle/connectionIssues.ts",
      "../../../src/features/sessions/composables/sessionConnectionLifecycle/sessionDisplayEvents.ts",
    ].map((item) => readFileSync(new URL(item, import.meta.url), "utf8")).join("\n");
  }
  if (path === "../../../src/features/sessions/components/SessionToolbox.vue") {
    return [
      path,
      "../../../src/features/sessions/components/sessionToolbox/menuOptions.ts",
      "../../../src/features/sessions/composables/useSessionAdvancedDiagnostics.ts",
      "../../../src/features/sessions/components/sessionToolbox/fileTransfer.ts",
    ].map((item) => readFileSync(new URL(item, import.meta.url), "utf8")).join("\n");
  }
  if (path === "../../../src/session/protocol/protocolAdapter.ts") {
    return [
      path,
      "../../../src/session/protocol/protocolAdapter/nativeAgentRuntime.ts",
      "../../../src/session/protocol/protocolAdapter/types.ts",
    ].map((item) => readFileSync(new URL(item, import.meta.url), "utf8")).join("\n");
  }
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("selectWorkspaceSessions shows only the active session", () => {
  assert.deepEqual(
    selectWorkspaceSessions(tabs, {
      activeSessionId: "session-3",
    }).map((tab) => tab.id),
    ["session-3"],
  );
});

test("selectWorkspaceSessions shows only the fullscreen session when one is selected", () => {
  assert.deepEqual(
    selectWorkspaceSessions(tabs, {
      activeSessionId: "session-1",
      fullscreenSessionId: "session-4",
    }).map((tab) => tab.id),
    ["session-4"],
  );
});

test("selectWorkspaceSessions falls back to the first sessions when no active session exists", () => {
  assert.deepEqual(
    selectWorkspaceSessions(tabs, {
      activeSessionId: "missing",
    }).map((tab) => tab.id),
    ["session-1"],
  );
});

test("summarizeWorkspaceRenderSlots marks visible parked and external-candidate sessions", () => {
  const slots = summarizeWorkspaceRenderSlots(tabs, {
    activeSessionId: "session-5",
  });

  assert.deepEqual(slots.map((slot) => [slot.id, slot.renderState, slot.visible, slot.externalWindowCandidate]), [
    ["session-1", "parked", false, false],
    ["session-2", "parked", false, false],
    ["session-3", "parked", false, false],
    ["session-4", "parked", false, false],
    ["session-5", "visible", true, true],
  ]);
});

test("summarizeWorkspaceRenderSlots keeps fullscreen as the only visible viewport", () => {
  const slots = summarizeWorkspaceRenderSlots(tabs, {
    activeSessionId: "session-1",
    fullscreenSessionId: "session-4",
  });

  assert.deepEqual(slots.map((slot) => [slot.id, slot.renderState, slot.visible]), [
    ["session-1", "parked", false],
    ["session-2", "parked", false],
    ["session-3", "parked", false],
    ["session-4", "visible", true],
    ["session-5", "parked", false],
  ]);
});

test("summarizeWorkspaceRenderSlots parks other sessions while the active session is in an external window", () => {
  const slots = summarizeWorkspaceRenderSlots(tabs, {
    activeSessionId: "session-5",
    externalSessionIds: ["session-5"],
  });

  assert.deepEqual(slots.map((slot) => [slot.id, slot.renderState, slot.visible, slot.externalWindowOwned]), [
    ["session-1", "parked", false, false],
    ["session-2", "parked", false, false],
    ["session-3", "parked", false, false],
    ["session-4", "parked", false, false],
    ["session-5", "external", false, true],
  ]);
});

test("summarizeWorkspaceRenderSlots does not show a failed session behind the active external session", () => {
  const mixedTabs = [tabs[1]!, tabs[4]!];
  const options = { activeSessionId: "session-5", externalSessionIds: ["session-5"] };
  const detached = summarizeWorkspaceRenderSlots(mixedTabs, options);
  assert.deepEqual(detached.map((slot) => [slot.id, slot.renderState, slot.visible]), [
    ["session-2", "parked", false],
    ["session-5", "external", false],
  ]);

  const selectedFailed = summarizeWorkspaceRenderSlots(mixedTabs, { ...options, activeSessionId: "session-2" });
  assert.deepEqual(selectedFailed.filter((slot) => slot.visible).map((slot) => slot.id), ["session-2"]);

  const docked = summarizeWorkspaceRenderSlots(mixedTabs, { ...options, externalSessionIds: [] });
  assert.deepEqual(docked.filter((slot) => slot.visible).map((slot) => slot.id), ["session-5"]);
});

test("summarizeWorkspaceRenderSlots preserves fullscreen selection and fallback with external sessions", () => {
  const options = { activeSessionId: "session-1", externalSessionIds: ["session-1"] };
  const fullscreen = summarizeWorkspaceRenderSlots(tabs, { ...options, fullscreenSessionId: "session-2" });
  assert.deepEqual(fullscreen.filter((slot) => slot.visible).map((slot) => slot.id), ["session-2"]);

  const fallback = summarizeWorkspaceRenderSlots(tabs, { ...options, activeSessionId: "missing" });
  assert.deepEqual(fallback.filter((slot) => slot.visible).map((slot) => slot.id), ["session-2"]);
});

test("summarizeWorkspaceRenderSlots treats hosted external sessions as externally owned even after the main canvas is released", () => {
  const slots = summarizeWorkspaceRenderSlots(tabs, {
    activeSessionId: "session-1",
    externalSessionIds: ["session-2"],
  });

  assert.deepEqual(slots.map((slot) => [slot.id, slot.renderState, slot.externalWindowOwned]), [
    ["session-1", "visible", false],
    ["session-2", "external", true],
    ["session-3", "parked", false],
    ["session-4", "parked", false],
    ["session-5", "parked", false],
  ]);
});

test("App keeps the session workspace mounted when switching modules so Core Surface canvases survive", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /<section v-show="activeWorkspace === 'sessions'" class="session-workbench">/);
  assert.doesNotMatch(appVue, /<section v-else class="session-workbench">/);
});

test("App exposes low-profile global transfer entry and right drawer", () => {
  const drawer = sourceFile("../../../src/features/transfers/components/GlobalTransferDrawer.vue");
  const shared = sourceFile("../../../src/features/transfers/components/TransferTaskDrawer.vue");
  assert.match(drawer, /<TransferTaskDrawer/);
  assert.match(drawer, /:show="globalTransferDrawerOpen"/);
  assert.match(drawer, /:selected-session-id="globalTransferDrawerSessionId"/);
  assert.match(drawer, /@task-action="controlTask"/);
  assert.match(shared, /<NDrawer[\s\S]*placement="right"/);
});
test("Fullscreen and external session windows keep transfer detail out of the viewport", () => {
  const viewportGrid = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const viewportChrome = sourceFile("../../../src/features/sessions/components/SessionViewportChrome.vue");
  const externalWindowPage = sourceFile("../../../src/features/sessions/components/SessionExternalWindowPage.vue");

  assert.match(viewportGrid, /import SessionViewportChrome/);
  assert.doesNotMatch(viewportChrome, /import GlobalTransferStatus/);
  assert.doesNotMatch(viewportGrid, /function shouldShowViewportTransferEntry/);
  assert.doesNotMatch(viewportGrid, /:show-transfer-status=/);
  assert.doesNotMatch(viewportChrome, /<GlobalTransferStatus/);
  assert.doesNotMatch(viewportGrid, /SessionFileTransferPanel/);
  assert.doesNotMatch(externalWindowPage, /SessionFileTransferPanel/);
  assert.doesNotMatch(externalWindowPage, /import GlobalTransferDrawer/);
  assert.doesNotMatch(externalWindowPage, /<GlobalTransferDrawer/);
});

test("App renders every session through explicit workspace render slots", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /summarizeWorkspaceRenderSlots/);
  assert.match(appVue, /const workspaceRenderSlots = computed/);
  assert.match(appVue, /getWorkspaceRenderSlot\(tab\.id\)\.renderState/);
  assert.match(appVue, /'external-window-candidate': getWorkspaceRenderSlot\(tab\.id\)\.externalWindowCandidate/);
});

test("App exposes an independent-window ownership toggle without duplicating connections", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /const externalWindowSessionIds = ref<Set<string>>/);
  assert.match(appVue, /externalSessionIds: \[\.\.\.externalWindowSessionIds\.value\]/);
  assert.match(appVue, /function toggleExternalWindowSession\(sessionId: string\)/);
  assert.match(appVue, /getWorkspaceRenderSlot\(tab\.id\)\.externalWindowOwned/);
  assert.doesNotMatch(appVue, /function openExternalWindowSession\(sessionId: string\)[\s\S]*connect_remote_session_rfb/);
});

test("App hosts independent windows with a Core Surface shell instead of the full app shell", () => {
  const appVue = appStructureSource();
  const externalWindowPage = sourceFile("../../../src/features/sessions/components/SessionExternalWindowPage.vue");
  const lifecycle = sourceFile("../../../src/app/composables/vncAppState/lifecycle.ts");

  assert.match(appVue, /import \{ WebviewWindow \} from "@tauri-apps\/api\/webviewWindow"/);
  assert.match(appVue, /createSessionExternalWindowRequest/);
  assert.match(appVue, /new WebviewWindow\(request\.label/);
  assert.match(appVue, /sessionExternalWindowDockEvent\(sessionId\)/);
  assert.match(lifecycle, /listenEvent\("session-window-dock-requested"/);
  assert.match(lifecycle, /isSessionExternalWindowDockRequest\(payload\)/);
  assert.match(lifecycle, /dockExternalWindowSession\(payload\.sessionId, payload\.windowInstanceId\)/);
  assert.match(appVue, /isSessionExternalWindowLocation\(locationHref\)/);
  assert.match(appVue, /<SessionExternalWindowPage v-if="app\.externalWindowLocation" \/>/);
  assert.match(appVue, /<AppShell v-else \/>/);
  assert.match(externalWindowPage, /'session-external-window-page'/);
  assert.match(externalWindowPage, /sessionExternalWindowStateEvent/);
  assert.match(externalWindowPage, /createCoreSurfaceSession/);
  assert.match(externalWindowPage, /ref="coreSurfaceMount"/);
  assert.match(externalWindowPage, /window\.addEventListener\("keydown", handleWindowKeydown, true\)/);
  assert.match(externalWindowPage, /window\.removeEventListener\("keyup", handleWindowKeyup, true\)/);
  assert.doesNotMatch(externalWindowPage, /<SessionViewportGrid/);
  assert.doesNotMatch(externalWindowPage, /@keydown\.capture/);
  assert.doesNotMatch(externalWindowPage, /session-external-window-toolbar/);
  assert.doesNotMatch(externalWindowPage, /<SessionInputModeToggle/);
  assert.doesNotMatch(externalWindowPage, /requestDockExternalWindowSession\(currentTab\.value\.id\)/);
  assert.doesNotMatch(externalWindowPage, /disconnectSessionFromViewport\(currentTab\.value\.id\)/);
  assert.doesNotMatch(externalWindowPage, /<SessionWorkbench/);
  assert.doesNotMatch(externalWindowPage, /SessionTopbar|SessionToolbar|SessionTabs|SessionEventPanel|SessionDiagnosticsPanel/);
  assert.doesNotMatch(externalWindowPage, /<ModuleRail/);
  assert.doesNotMatch(externalWindowPage, /<ContextSidebar/);
  assert.doesNotMatch(externalWindowPage, /HostWorkbench|HistoryWorkbench|SettingsWorkbench|LocalShareWorkbench/);
  assert.doesNotMatch(appVue, /class="session-external-shell"/);
});

test("external session window request carries enough connection data for the hosted workspace", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /function getExternalWindowLocation\(sessionId: string\)[\s\S]*host: session\.host/);
  assert.match(appVue, /port: session\.port/);
  assert.match(appVue, /readonly: Boolean\(sessionReadonlyBySessionId\.value\[sessionId\]\)/);
  assert.match(appVue, /const request = createSessionExternalWindowRequest\(/);
  assert.match(appVue, /url\.searchParams\.get\("windowInstanceId"\)/);
  assert.match(appVue, /const host = url\.searchParams\.get\("host"\)/);
  assert.match(appVue, /const port = Number\(url\.searchParams\.get\("port"\)\)/);
  assert.match(appVue, /const readonly = url\.searchParams\.get\("readonly"\) === "true"/);
});




test("App provides explicit fullscreen exit controls and a configurable shortcut", () => {
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");
  const settings = sourceFile("../../../src/features/settings/composables/useSettingsWorkbench.ts");
  assert.match(toolbox, /case "exit-fullscreen":[\s\S]*await exitSessionFullscreen\(\);/);
  assert.match(settings, /validateKeyboardShortcut/);
  assert.match(settings, /findKeyboardShortcutConflict/);
});
test("external session window claims ownership without starting a second connection", () => {
  const appState = sourceFile("../../../src/app/composables/useVncAppState.ts");
  const externalWindowBootBlock = appState.match(/function mountExternalWindow\(\)[\s\S]*?\n  \}/)?.[0] ?? "";

  assert.match(externalWindowBootBlock, /options\.activeWorkspace\.value = "sessions"/);
  assert.match(externalWindowBootBlock, /options\.host\.value = location\.host/);
  assert.match(externalWindowBootBlock, /options\.port\.value = location\.port/);
  assert.doesNotMatch(externalWindowBootBlock, /connectVnc\(/);
  assert.match(appState, /createExternalWindowStateBridge/);
  assert.match(appState, /publishExternalWindowState/);
});

test("App closes the external window shell when docking from the child window", () => {
  const appVue = appStructureSource();
  const coordinator = sourceFile("../../../src/features/sessions/composables/useSessionExternalWindowCoordinator.ts");
  const lifecycle = sourceFile("../../../src/app/composables/vncAppState/lifecycle.ts");
  const capability = readFileSync(new URL("../../../src-tauri/capabilities/default.json", import.meta.url), "utf8");

  assert.match(lifecycle, /getCurrentWindow\(\)\.onCloseRequested\(handler\)/);
  assert.match(lifecycle, /function mountExternalWindow\(\)[\s\S]*runtime\.listenCloseRequested/);
  assert.match(lifecycle, /event\.preventDefault\(\)/);
  assert.match(lifecycle, /emitDockRequest\("session-window-dock-requested", request\)/);
  assert.match(coordinator, /emit\(sessionExternalWindowDockEvent\(sessionId\), request\)/);
  assert.match(coordinator, /\.handle\.destroy\(\)/);
  assert.doesNotMatch(coordinator, /getCurrentWindow\(\)\.(?:destroy|close)\(/);
  assert.doesNotMatch(appVue, /getCurrentWindow\(\)\.close\(\)\.catch\(\(\) => undefined\)/);
  assert.match(capability, /"core:window:allow-close"/);
  assert.match(capability, /"core:window:allow-destroy"/);
});

test("App destroys externally owned child windows when disconnecting or closing a session", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /async function removeExternalWindowSession\(sessionId: string\): Promise<void>/);
  assert.match(appVue, /const closingWindow = removeExternalWindowSession\(sessionId\);/);
  assert.match(appVue, /await closingWindow;/);
  assert.doesNotMatch(appVue, /removeExternalWindowSession\(sessionId\)\?\.destroy/);
  assert.match(appVue, /dispose: disposeExternalWindows/);
  assert.match(appVue, /clearConnection: clearSessionConnection/);
  assert.match(appVue, /releaseSession: disconnectOwnedSession/);
  assert.match(appVue, /async function disconnectSessionConnection\(sessionId: string\) \{\s*return connectAttempts\.cancel\(sessionId\)/);
  assert.match(appVue, /async function closeSessionTab\(sessionId: string\)[\s\S]*const releasing = userDisconnector\.disconnect\(sessionId, true\);[\s\S]*await releasing;/);
  assert.match(appVue, /disconnectAllSessionConnections: disposeSessionConnections/);
  assert.match(appVue, /onSessionClosed: \(sessionId\) => sessionFileTransferManager\.removeSessionTasks\(sessionId\)/);
});

test("App releases per-session external window listeners when docking back", () => {
  const appVue = appStructureSource();
  const appState = sourceFile("../../../src/app/composables/useVncAppState.ts");

  assert.match(appVue, /const externalWindowDockUnlisteners = new Map<string, \(\(\) => void\)>/);
  assert.match(appVue, /function cleanupExternalWindowDockListener\(sessionId: string\)/);
  assert.match(appState, /useSessionExternalWindowCoordinator\(\{[\s\S]*externalWindowDockUnlisteners,[\s\S]*externalWindowInputUnlisteners,[\s\S]*externalWindowStateUnlisteners,/);
  assert.match(appState, /dispose: disposeExternalWindows/);
  assert.match(appVue, /options\.disposeExternalWindows\(\)/);
  assert.match(appState, /disposeExternalWindows,/);
});

test("App proxies external child-window input through a focused session input bridge", () => {
  const appVue = appStructureSource();
  const mainTs = sourceFile("../../../src/main.ts");

  assert.match(appVue, /startSessionExternalInputForwarder/);
  assert.match(appVue, /createExternalKeyboardInputPayload/);
  assert.match(appVue, /createExternalPointerInputPayload/);
  assert.match(appVue, /createExternalWheelInputPayload/);
  assert.match(appVue, /forwardExternalInput\(payload\)/);
  assert.doesNotMatch(mainTs, /sessionExternalInputEvent|startSessionExternalInputForwarder|forwardExternalInput/);
  assert.doesNotMatch(appVue, /function openExternalWindowSession\(sessionId: string\)[\s\S]*connect_remote_session_rfb/);
});

test("App refreshes visible sessions differently from parked sessions", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /function visibleWorkspaceSessionIds\(\)/);
  assert.match(appVue, /const visibleSessionIds = refreshVisibleSessionIds\(\);/);
  assert.match(appVue, /if \(visibleSessionIds\.has\(sessionId\)\)/);
  assert.match(appVue, /updateSessionRuntimeEvidence\(sessionId, runtimeEvidence\);/);
});

test("App clears stale per-session connections before reconnecting a recoverable viewport", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /await connectAttempts\.run\(sessionId,[\s\S]*await attempt\.wait\(\(\) => disconnectOwnedSession\(sessionId\)\)/);
  assert.match(appVue, /async function prepareSessionForReconnect\(sessionId: string\)/);
  assert.match(appVue, /await disconnectSessionConnection\(sessionId\);/);
});

test("App cleans terminal native RFB statuses through the connection cleanup path", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /shouldCleanupConnectionForSessionState\(value as RemoteSessionState\)/);
  assert.match(appVue, /void disconnectSessionConnection\(sessionId\)\.catch\(\(\) => undefined\);/);
  assert.doesNotMatch(appVue, /value\.startsWith\("disconnected"\)[\s\S]*clearSessionConnection\(sessionId\)/);
});

test("App detects zombie live connections before refreshing displays", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /async function cleanupZombieSessionConnections\(reason: string\)/);
  assert.match(appVue, /sessionConnectionRegistry\.detachZombieConnections\(connectionAllowedSessionIds\(\)\)/);
  assert.match(appVue, /await cleanupZombieSessionConnections\("refresh_live_displays"\);/);
});

test("connection flow does not downgrade a fast native RFB connect event back to connecting", () => {
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");
  const connectingTransitions = lifecycle.match(/transitionSession\(sessionId, "connecting"\)/g) ?? [];

  assert.equal(connectingTransitions.length, 1);
  assert.doesNotMatch(lifecycle, /await refreshLiveDisplays\(\);[\s\S]*transitionSession\(sessionId, "connecting"\)/);
});

test("App mounts the Core Surface into a dedicated child node so Vue-owned overlays survive reconnect cleanup", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /class="screen-mount"/);
  assert.match(appVue, /setScreenTarget\(tab\.id, element as Element \| null\)/);
  assert.doesNotMatch(appVue, /:ref="\{}\(element\) => setScreenTarget\(tab\.id, element as Element \| null\)"\s*:class="\['screen'/);
});

test("failed viewport exposes retry, address copy and diagnostics without connected toolbox groups", () => {
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");
  assert.match(toolbox, /v-if="presentation.recoverable" class="session-recovery-actions"/);
  assert.match(toolbox, /action:retry-session/);
  assert.match(toolbox, /action:copy-target/);
  assert.match(toolbox, /action:open-diagnostics/);
  assert.match(toolbox, /v-if="presentation.connected"[\s\S]*class="session-toolbox-trigger"/);
});


test("App makes the selected main viewport visually dominant and routes input to the focused window", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /'input-active': tab\.active/);
  assert.match(appVue, /\.remote-viewport\.active[\s\S]*box-shadow:[\s\S]*rgba\(92, 200, 167/);
  assert.match(appVue, /\.screen\.input-active[\s\S]*pointer-events: auto/);
  assert.match(appVue, /syncSessionInputOwnership\(\)/);
  assert.match(appVue, /const activeSessionId = sessionInputFocus\.sessionId\(\)/);
  assert.match(appVue, /connection\.setInputEnabled\(sessionId === activeSessionId && !sessionReadonlyBySessionId\.value\[sessionId\]\)/);
});

test("App applies clipboard sync mode changes to live sessions without reconnecting", () => {
  const appVue = appStructureSource();

  assert.match(appVue, /watch\(\(\) => sessionSettings\.value\.clipboardSyncMode/);
  assert.match(appVue, /sessionConnectionRegistry\.listLiveSessionIds\(\)/);
  assert.match(appVue, /connection\.setClipboardSyncMode\(mode\)/);
  assert.doesNotMatch(appVue.match(/watch\(\(\) => sessionSettings\.value\.clipboardSyncMode[\s\S]*?\n  \},\);/)?.[0] ?? "", /connectVnc|vncAdapter\.connect/);
});

test("App keeps read-only switching in toolbox and displays input mode in the HUD", () => {
  const appVue = appStructureSource();
  const runtimeState = sourceFile("../../../src/features/sessions/composables/useSessionRuntimeState.ts");
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");

  assert.match(appVue, /function setSessionReadonlyMode\(sessionId: string, readonly: boolean\)/);
  assert.match(appVue, /setSessionReadonlyState\(sessionId, readonly\)/);
  assert.match(appVue, /sessionConnectionRegistry\.get\(sessionId\)\?\.connection\.setReadonlyMode\(readonly\)/);
  assert.match(runtimeState, /function setSessionReadonlyState\(sessionId: string, readonly: boolean\)/);
  assert.match(runtimeState, /sessionReadonlyBySessionId\.value = \{/);
  assert.match(runtimeState, /sessionViewOnlySharedBySessionId/);
  assert.match(runtimeState, /function setSessionViewOnlySharedState\(sessionId: string, viewOnlyShared: boolean\)/);
  assert.match(appVue, /readonly: sessionReadonlyBySessionId\.value\[tab\.id\] \?\? false/);
  assert.match(appVue, /viewOnlyShared: sessionViewOnlySharedBySessionId\.value\[tab\.id\] \?\? false/);
  assert.match(appVue, /statusHud\.inputMode/);
  assert.doesNotMatch(appVue, /<SessionInputModeToggle/);
  assert.match(toolbox, /actionOption\("toggle-readonly"\)/);
  const modeToggleFunction = appVue.match(/function setSessionReadonlyMode\(sessionId: string, readonly: boolean\)[\s\S]*?\n}\n\nasync function disconnectSessionConnection/)?.[0] ?? "";
  assert.doesNotMatch(modeToggleFunction, /connectVnc|vncAdapter\.connect/);
});

test("keyboard input has no remote OS compatibility configuration", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");
  const hostWorkbench = sourceFile("../../../src/features/hosts/components/HostWorkbench.vue");

  assert.doesNotMatch(sessionViewport, /keyboardCompatibilityOptions/);
  assert.doesNotMatch(sessionViewport, /setSessionRemoteOsProfile/);
  assert.doesNotMatch(toolbox, /remote-os|RemoteOsProfile|键盘兼容模式/);
  assert.doesNotMatch(hostWorkbench, /remoteOsOptions|remoteOsProfile/);
});

test("Session toolbox nests platform shortcut actions under a send-shortcuts submenu", () => {
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");
  assert.match(toolbox, /submenuOption\("shortcuts", t\("sessions\.toolbox\.sendShortcuts"\), \[/);
  assert.match(toolbox, /submenuOption\("shortcuts-windows"/);
  assert.match(toolbox, /submenuOption\("shortcuts-macos"/);
  assert.match(toolbox, /shortcutActionsById\(WINDOWS_SHORTCUT_ACTION_IDS, "windows"\)/);
  assert.match(toolbox, /shortcutActionsById\(MACOS_SHORTCUT_ACTION_IDS, "macos"\)/);
  assert.doesNotMatch(toolbox, /SessionShortcutPanel|shortcutPanelOpen|open-shortcuts/);
});

test("Session toolbox exposes local display scale controls without remote resolution switching", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");
  const runtimeState = sourceFile("../../../src/features/sessions/composables/useSessionRuntimeState.ts");
  const coreSurfaceSession = sourceFile("../../../src/session/renderer/coreSurfaceSession.ts");
  const styles = sourceFile("../../../src/app/styles/session-viewport.css");

  assert.match(sessionViewport, /<SessionViewportChrome/);
  assert.doesNotMatch(sessionViewport, /remote-viewport-controls/);
  assert.match(toolbox, /submenuOption\("display-scale"/);
  assert.doesNotMatch(toolbox, /submenuOption\("quality"/);
  assert.doesNotMatch(toolbox, /submenuOption\("resolution"/);
  assert.doesNotMatch(toolbox, /sessionQualityProfiles\.map/);
  assert.match(toolbox, /displayScaleMenuOptions\(\)/);
  assert.doesNotMatch(toolbox, /setSessionQualityProfile/);
  assert.doesNotMatch(toolbox, /setSessionViewportResolution/);
  assert.match(toolbox, /setSessionViewportScaleMode/);
  assert.match(sessionViewport, /`scale-\$\{getSessionViewportScaleMode\(tab\.id\)\}`/);
  assert.match(runtimeState, /sessionViewportScaleModeBySessionId/);
  assert.match(runtimeState, /function getSessionViewportScaleMode\(sessionId: string\)/);
  assert.match(coreSurfaceSession, /createCoreSurfaceSession/);
  assert.match(coreSurfaceSession, /canvas\.className = "core-surface-canvas"/);
  assert.match(toolbox, /setSessionViewportScaleMode\(props\.sessionId, command\.value\)/);
  assert.match(styles, /\.screen\.scale-scale200[\s\S]*overflow: auto/);
  assert.match(styles, /\.screen\.scale-scale200 \.screen-mount canvas/);
});


test("Session viewport supports local panning for fixed display scale without forwarding remote input", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const styles = sourceFile("../../../src/app/styles/session-viewport.css");

  assert.match(sessionViewport, /function shouldStartViewportPan\(sessionId: string, event: PointerEvent\)/);
  assert.match(sessionViewport, /options\.getSessionViewportScaleMode\(sessionId\) !== "fit"/);
  assert.match(sessionViewport, /event\.button === 1 \|\| \(event\.altKey && event\.button === 0\)/);
  assert.match(sessionViewport, /function startViewportPan\(sessionId: string, event: PointerEvent\)/);
  assert.match(sessionViewport, /function moveViewportPan\(event: PointerEvent\)/);
  assert.match(sessionViewport, /function finishViewportPan\(event\?: PointerEvent\)/);
  assert.match(sessionViewport, /startViewportPan\(tabId, event\);\s*return;/);
  assert.match(sessionViewport, /moveViewportPan\(event\);\s*return;/);
  assert.match(sessionViewport, /@pointercancel\.capture="finishViewportPan\(\$event\)"/);
  assert.match(styles, /\.remote-viewport\.is-panning/);
});

test("Session viewport does not expose Ctrl Alt Del in the send-key menu", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const zh = sourceFile("../../../src/i18n/messages/zh-CN.ts");

  assert.doesNotMatch(sessionViewport, /sendKeyMenuOptions/);
  assert.doesNotMatch(sessionViewport, /ctrl-alt-del/);
  assert.doesNotMatch(sessionViewport, /keySequences\.ctrlAltDelSequenceUnsupported/);
  assert.doesNotMatch(sessionViewport, /sendSessionKeySequence/);
  assert.doesNotMatch(zh, /sendKeys: "发送按键"/);
  assert.doesNotMatch(zh, /ctrlAltDelSequenceUnsupported/);
});

test("Session viewport HUD can be hidden and shown per session", () => {
  const appVue = appStructureSource();
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const viewportChrome = sourceFile("../../../src/features/sessions/components/SessionViewportChrome.vue");
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");
  const zh = sourceFile("../../../src/i18n/messages/zh-CN.ts");

  assert.match(appVue, /createSessionViewportHudState\(externalWindowLocation\?\.sessionId\)/);
  assert.match(sessionViewport, /:is-hud-hidden="isViewportHudHidden"/);
  assert.match(sessionViewport, /:toggle-hud="toggleViewportHud"/);
  assert.match(viewportChrome, /props\.isHudHidden\(tab\.id\)/);
  assert.match(viewportChrome, /props\.toggleHudExpanded\(tab\.id\)/);
  assert.doesNotMatch(appVue, /remote-viewport-hud-toggle/);
  assert.match(toolbox, /case "toggle-hud"/);
  assert.match(toolbox, /props\.toggleHud\(sessionId\)/);
  assert.match(zh, /hideViewportHud: "隐藏状态 HUD"/);
  assert.match(zh, /showViewportHud: "显示状态 HUD"/);
  assert.match(zh, /showLabel: "显示状态 HUD"/);
  assert.match(zh, /hideLabel: "隐藏状态 HUD"/);
  assert.match(zh, /description: "显示或隐藏右下角状态 HUD。"/);
});

test("external session page docks the current session instead of opening a nested independent window", () => {
  const tabs = sourceFile("../../../src/features/sessions/components/SessionTabs.vue");
  const externalPage = sourceFile("../../../src/features/sessions/components/SessionExternalWindowPage.vue");
  const coordinator = sourceFile("../../../src/features/sessions/composables/useSessionExternalWindowCoordinator.ts");

  assert.match(tabs, /isCurrentExternalSession\(tabId\) \|\| slot\.externalWindowOwned\s*\? t\("sessions\.dockWindow"\)\s*: t\("sessions\.detachWindow"\)/);
  assert.match(externalPage, /case "toggle-external-window":\s*await requestDockExternalWindowSession\(sessionId\);\s*return;/);
  assert.match(coordinator, /if \(externalWindowLocation\?\.sessionId === sessionId\) \{\s*await requestDockExternalWindowSession\(sessionId\);\s*return;/);
});

test("external session page sends remote desktop shortcuts through Agent-only payloads", () => {
  const externalWindowPage = sourceFile("../../../src/features/sessions/components/SessionExternalWindowPage.vue");

  assert.match(externalWindowPage, /type: "remoteDesktopShortcut"/);
  assert.doesNotMatch(externalWindowPage, /createRemoteDesktopShortcutSequence/);
});

test("App routes default fullscreen presentation into the external window", () => {
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");
  const fullscreen = sourceFile("../../../src/features/sessions/composables/useSessionFullscreen.ts");
  assert.match(lifecycle, /postConnectPresentation === "fullscreen"/);
  assert.match(fullscreen, /await openExternalWindowSession\(sessionId, true\)/);
});
test("Session connection can open the connected tab in an independent window", () => {
  const appVue = appStructureSource();
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");

  assert.match(appVue, /openExternalWindowSession,\s*\n\s*password,/);
  assert.match(lifecycle, /openExternalWindowSession\?: \(sessionId: string\) => Promise<void>/);
  assert.match(lifecycle, /void refreshLiveDisplays\(\)\.catch\(\(\) => undefined\);\s*if \(connectOptions\.postConnectPresentation === "external"\) \{\s*await attempt\.wait\(async \(\) => openExternalWindowSession\?\.\(sessionId\)\);/);
  assert.doesNotMatch(lifecycle, /openInExternalWindowAfterConnect/);
});


test("Session does not probe explicit Agent port from the VNC connection lifecycle", () => {
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");

  assert.doesNotMatch(lifecycle, /async function probePreferredAgentPort/);
  assert.doesNotMatch(lifecycle, /connection\.reportRemoteCapabilities\(\)/);
  assert.doesNotMatch(lifecycle, /recordFailedConnectionHistory[\s\S]*agent_port/);
});

test("Session refreshes local VNC Server status after a local connection reaches connected", () => {
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");

  assert.match(lifecycle, /refreshLocalVncServerStatus: \(\) => Promise<void>;/);
  assert.match(lifecycle, /if \(value === "connected"\)[\s\S]*if \(localPreparation\)[\s\S]*refreshLocalVncServerStatus\(\)/);
});


test("Session connection lifecycle does not auto-detect target Agent capabilities", () => {
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");

  assert.doesNotMatch(lifecycle, /async function detectTargetEnhancedPreparation\(/);
  assert.doesNotMatch(lifecycle, /report_agent_capabilities/);
  assert.doesNotMatch(lifecycle, /port \+ 1000|probePreferredAgentPort/);
});

test("Session does not infer a keyboard profile from the RFB desktop name", () => {
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");

  assert.doesNotMatch(lifecycle, /inferRemoteOsProfile|remoteOsProfile|remote_os/);
});




test("Session workspace does not render the duplicated diagnostic event drawer", () => {
  const sessionWorkbench = sourceFile("../../../src/features/sessions/components/SessionWorkbench.vue");
  const diagnosticsPanel = sourceFile("../../../src/features/sessions/components/SessionDiagnosticsPanel.vue");

  assert.doesNotMatch(sessionWorkbench, /SessionEventPanel/);
  assert.doesNotMatch(sessionWorkbench, /session-event-drawer/);
  assert.doesNotMatch(diagnosticsPanel, /SessionEventPanel/);
});

test("Session workspace keeps the remote viewport as the last main workflow surface", () => {
  const sessionWorkbench = sourceFile("../../../src/features/sessions/components/SessionWorkbench.vue");
  const viewportIndex = sessionWorkbench.indexOf("<SessionViewportGrid />");
  const inspectorIndex = sessionWorkbench.indexOf("session-inspector");

  assert.notEqual(viewportIndex, -1);
  assert.notEqual(inspectorIndex, -1);
  assert.ok(viewportIndex < inspectorIndex);
  assert.doesNotMatch(sessionWorkbench, /<SessionEventPanel|event-drawer/);
});

test("App exposes runtime evidence in a separate folded developer diagnostics component", () => {
  const panel = sourceFile("../../../src/features/sessions/components/SessionDiagnosticsPanel.vue");
  const developer = sourceFile("../../../src/features/sessions/components/SessionDeveloperDiagnostics.vue");
  assert.match(panel, /<SessionDeveloperDiagnostics[\s\S]*v-if="showDeveloperDebugInfo"/);
  assert.match(developer, /<details class="session-debug-card"/);
  assert.match(developer, /summary\.runtimeEvidence\.latestCoreEvent/);
  assert.match(developer, /summary\.runtimeEvidence\.surface/);
  assert.match(developer, /summary\.remoteSessionCore\.activeSurfaceOwner/);
  assert.doesNotMatch(developer, /clipboard.*\.text|clipboard.*\.preview/);
});

test("App Server sampling follows diagnostics visibility and self-session identity", () => {
  const appState = sourceFile("../../../src/app/composables/useVncAppState.ts");
  const sampler = sourceFile("../../../src/app/composables/vncAppState/localDiagnostics.ts");
  const derived = sourceFile("../../../src/features/sessions/composables/useSessionDerivedState.ts");
  assert.match(appState, /diagnosticsVisible: sessionDeveloperDiagnosticsVisible/);
  assert.match(appState, /localVncServerDiagnosticsRefresh\.stop\(\)/);
  assert.match(sampler, /matchesLocalVncServerTarget/);
  assert.match(derived, /localServer: activeLocalServerDiagnostics\.value/);
});

test("Session diagnostics keep permission checks independent from developer details", () => {
  const panel = sourceFile("../../../src/features/sessions/components/SessionDiagnosticsPanel.vue");
  assert.match(panel, /<details class="session-advanced-diagnostics"/);
  assert.match(panel, /v-if="canCheckPermissions"/);
  assert.doesNotMatch(panel, /showDeveloperDebugInfo && canCheckPermissions/);
  assert.match(panel, /<SessionAgentPermissionReport/);
});

test("Session no longer exposes optional Agent screen stream controls", () => {
  const appState = sourceFile("../../../src/app/composables/useVncAppState.ts");
  const displayControls = sourceFile("../../../src/features/sessions/composables/useSessionDisplayControls.ts");
  const toolbox = sourceFile("../../../src/features/sessions/components/SessionToolbox.vue");
  const protocolAdapter = sourceFile("../../../src/session/protocol/protocolAdapter.ts");

  assert.doesNotMatch(protocolAdapter, /enableAgentScreenStream/);
  assert.doesNotMatch(protocolAdapter, /disableAgentScreenStream/);
  assert.doesNotMatch(protocolAdapter, /AgentScreenStreamActivationResult/);
  assert.doesNotMatch(displayControls, /setSessionAgentScreenStreamMode/);
  assert.doesNotMatch(toolbox, /toggle-agent-screen-stream/);
  assert.doesNotMatch(appState, /setSessionAgentScreenStreamMode/);
  assert.doesNotMatch(displayControls, /transportKind = "local-agent-enhanced"/);
});

test("Session viewport is focusable so main-window keyboard input reaches the enhanced transport", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");

  assert.match(sessionViewport, /tabindex="0"/);
  assert.match(sessionViewport, /focusRemoteViewport/);
  assert.match(sessionViewport, /@pointerdown\.capture="forwardMainPointer\(tab\.id, \$event, 'down'\)"/);
});

test("Session viewport claims main-window input before forwarding the first pointer down", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const forwardMainPointer = sessionViewport.match(/async function forwardMainPointer[\s\S]*?\n}\n\nfunction forwardMainWheel/)?.[0] ?? "";

  assert.match(forwardMainPointer, /void activateSessionTab\(tabId\)/);
  assert.doesNotMatch(forwardMainPointer, /await activateSessionTab\(tabId\)/);
  assert.match(forwardMainPointer, /if \(action === "down"\) claimMainWindowInput\(\)/);
  assert.ok(
    forwardMainPointer.indexOf("claimMainWindowInput()") < forwardMainPointer.indexOf("connection.forwardExternalInput"),
  );
  assert.ok(
    forwardMainPointer.indexOf("connection.forwardExternalInput") < forwardMainPointer.indexOf("void activateSessionTab(tabId)"),
  );
});

test("Session activation syncs main-window input ownership before asynchronous display refresh", () => {
  const lifecycle = sourceFile("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts");
  const activateSessionTab = lifecycle.match(/async function activateSessionTab\(sessionId: string\)[\s\S]*?\n  \}/)?.[0] ?? "";

  assert.match(activateSessionTab, /onMainSessionChanged\?\.\(\)/);
  assert.ok(
    activateSessionTab.indexOf("onMainSessionChanged?.()") <
      activateSessionTab.indexOf("await refreshLiveDisplays()"),
  );
  assert.match(activateSessionTab, /if \(isMainWindowFocused\?\.\(\) \?\? true\) canvasConnection\.connection\.focus\(\)/);
});

test("Session viewport gives toolbox and HUD pointer priority over the remote canvas", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");
  const viewportChrome = sourceFile("../../../src/features/sessions/components/SessionViewportChrome.vue");
  const viewportSurface = `${sessionViewport}\n${viewportChrome}`;
  const styles = sourceFile("../../../src/app/styles/session-viewport.css");
  const toolboxIndex = viewportSurface.indexOf("<SessionToolbox");
  const hudIndex = viewportSurface.indexOf("'remote-viewport-hud'");
  const screenIndex = viewportSurface.indexOf("class=\"screen-mount\"");

  assert.match(sessionViewport, /function isLocalInteractiveTarget\(event: Event\): boolean/);
  assert.match(sessionViewport, /event\.composedPath\(\)/);
  assert.match(sessionViewport, /target instanceof Element/);
  assert.match(sessionViewport, /\.session-toolbox-trigger/);
  assert.match(sessionViewport, /\.remote-viewport-floating-stack/);
  assert.match(sessionViewport, /\.remote-viewport-hud/);
  assert.doesNotMatch(sessionViewport, /target instanceof HTMLElement && Boolean\(target\.closest/);
  assert.match(sessionViewport, /if \(isLocalInteractiveTarget\(event\)\) \{\s*return;\s*\}/);
  assert.ok(screenIndex !== -1 && hudIndex !== -1 && toolboxIndex !== -1);
  assert.ok(screenIndex < hudIndex);
  assert.ok(hudIndex < toolboxIndex);
  assert.match(styles, /\.screen\s*\{[\s\S]*?z-index: 0/);
  assert.match(styles, /\.remote-viewport-floating-stack\s*\{[\s\S]*?z-index: 10/);
  assert.match(styles, /\.remote-viewport-hud\s*\{[\s\S]*?pointer-events: auto/);
  assert.match(styles, /\.session-toolbox-layer\s*\{[\s\S]*?z-index: 30/);
  assert.match(styles, /\.session-toolbox-trigger\s*\{[\s\S]*?z-index: 31/);
});

test("external session window keeps toolbox pointer events local so dragging still works", () => {
  const externalWindowPage = sourceFile("../../../src/features/sessions/components/SessionExternalWindowPage.vue");

  assert.match(externalWindowPage, /function isInteractiveTarget\(event: Event\): boolean/);
  assert.match(externalWindowPage, /event\.composedPath\(\)/);
  assert.match(externalWindowPage, /target instanceof Element/);
  assert.match(externalWindowPage, /\.session-toolbox-trigger/);
  assert.match(externalWindowPage, /\.session-toolbox-button/);
  assert.match(externalWindowPage, /\.remote-viewport-hud/);
  assert.match(externalWindowPage, /if \(!externalWindowLocation \|\| isInteractiveTarget\(event\)\) \{\s*return;\s*\}/);
  assert.doesNotMatch(externalWindowPage, /target instanceof HTMLElement && Boolean\(target\.closest/);
});

test("Session viewport does not forward main-window pointer or wheel input while read-only", () => {
  const sessionViewport = sourceFile("../../../src/features/sessions/components/SessionViewportGrid.vue");

  assert.match(sessionViewport, /function isReadonlyViewport\(sessionId: string\): boolean/);
  assert.match(sessionViewport, /getRemoteViewportSummary\(tab\)\.inputMode === "readonly"/);
  assert.match(sessionViewport, /if \(isReadonlyViewport\(tabId\)\) \{[\s\S]*?return;[\s\S]*?\}[\s\S]*?connection\.forwardExternalInput\(createExternalPointerInputPayload/);
  assert.match(sessionViewport, /if \(isReadonlyViewport\(tabId\)\) \{[\s\S]*?return;[\s\S]*?\}[\s\S]*?connection\.forwardExternalInput\(createExternalWheelInputPayload/);
});

test("Session keyboard diagnostics only records input after capture passes read-only checks", () => {
  const appState = sourceFile("../../../src/app/composables/useVncAppState.ts");
  const keyboardHandler = appState.match(/function handleSessionViewportKeyboardEvent[\s\S]*?\n  }\n\n  const openGlobalTransferDrawer/)?.[0] ?? "";

  assert.match(keyboardHandler, /readonly: Boolean\(sessionReadonlyBySessionId\.value\[sessionId\]\)/);
  assert.match(keyboardHandler, /if \(!shouldCapture\) \{[\s\S]*?return;[\s\S]*?\}/);
  assert.match(keyboardHandler, /event\.preventDefault\(\);[\s\S]*event\.stopPropagation\(\);[\s\S]*recordMainViewportInput\(sessionId, "keyboard"\)/);
});

test("App exposes production session ownership and input activity in diagnostics", () => {
  const appVue = appStructureSource();
  const runtimeState = sourceFile("../../../src/features/sessions/composables/useSessionRuntimeState.ts");

  assert.match(appVue, /sessionLatestInputBySessionId/);
  assert.match(appVue, /sessionRuntimeEvidenceBySessionId/);
  assert.match(appVue, /renderState: activeWorkspaceRenderSlot\.value\?\.renderState/);
  assert.match(appVue, /externalWindowOwned: activeWorkspaceRenderSlot\.value\?\.externalWindowOwned/);
  assert.match(appVue, /latestInput: activeRemoteSession\.value/);
  assert.match(appVue, /runtimeEvidence: activeRemoteSession\.value/);
  assert.match(appVue, /sessionDiagnosticsSummary(?:\.value)?\.renderState/);
  assert.match(appVue, /sessionDiagnosticsSummary(?:\.value)?\.externalWindowOwned/);
  assert.match(appVue, /sessionDiagnosticsSummary(?:\.value)?\.latestInput/);
  assert.match(appVue, /forwardMainPointer\(tab\.id, \$event, 'down'\)/);
  assert.match(appVue, /connection\.forwardExternalInput\(createExternalPointerInputPayload/);
  assert.match(appVue, /connection\.forwardExternalInput\(createExternalWheelInputPayload/);
  assert.match(appVue, /recordMainViewportInput\(tabId, "pointer"\)/);
  assert.match(sourceFile("../../../src/features/sessions/composables/useSessionExternalWindowCoordinator.ts"), /recordInput: \(sessionId, type\)/);
  assert.match(runtimeState, /_removedInputActivity/);
  assert.match(runtimeState, /_removedEvidence/);
  assert.doesNotMatch(runtimeState, /sessionExternalFrameAtBySessionId/);
});

test("App assigns diagnostic ownership explicitly instead of relying on source copy", () => {
  const appState = sourceFile("../../../src/app/composables/useVncAppState.ts");

  assert.match(appState, /logHostEvent/);
  assert.match(appState, /logSystemEvent/);
  assert.match(appState, /logEvent\(level, source, message, \{ origin: "host"/);
  assert.match(appState, /logEvent\(level, source, message, \{ origin: "system"/);
});
