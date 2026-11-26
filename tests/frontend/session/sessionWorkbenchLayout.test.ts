import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const sourceRoot = new URL("../../../src/", import.meta.url);

function sourceFile(path: string): string {
  if (path === "features/sessions/components/SessionExternalWindowPage.vue") {
    return [
      path,
      "features/sessions/components/sessionExternalWindowPage/feedback.ts",
      "features/sessions/components/sessionExternalWindowPage/inputForwarding.ts",
    ].map((item) => readFileSync(new URL(item, sourceRoot), "utf8")).join("\n");
  }
  if (path === "features/sessions/components/SessionViewportGrid.vue") {
    return [
      path,
      "features/sessions/components/sessionViewportGrid/inputForwarding.ts",
    ].map((item) => readFileSync(new URL(item, sourceRoot), "utf8")).join("\n");
  }
  if (path === "app/composables/useVncAppState.ts") {
    return [
      path,
      "app/composables/vncAppState/hostConnection.ts",
      "app/composables/vncAppState/lifecycle.ts",
      "app/composables/vncAppState/localDiagnostics.ts",
      "app/composables/vncAppState/passwordPrompt.ts",
      "app/composables/vncAppState/tray.ts",
      "app/composables/vncAppState/utils.ts",
      "app/composables/vncAppState/viewport.ts",
    ].map((item) => readFileSync(new URL(item, sourceRoot), "utf8")).join("\n");
  }
  if (path === "features/sessions/components/SessionToolbox.vue") {
    return [
      path,
      "features/sessions/components/sessionToolbox/menuOptions.ts",
      "features/sessions/composables/useSessionAdvancedDiagnostics.ts",
      "features/sessions/components/sessionToolbox/fileTransfer.ts",
    ].map((item) => readFileSync(new URL(item, sourceRoot), "utf8")).join("\n");
  }
  return readFileSync(new URL(path, sourceRoot), "utf8");
}

function settingsPanel(source: string, panel: "session" | "files" | "shortcuts" | "sharingService" | "about"): string {
  const start = source.indexOf(`data-settings-panel="${panel}"`);
  assert.notEqual(start, -1);
  const next = source.indexOf("data-settings-panel=", start + 1);
  return next === -1 ? source.slice(start) : source.slice(start, next);
}

function cssRule(source: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return source.match(new RegExp(`${escaped} \\{[\\s\\S]*?\\n\\}`))?.[0] ?? "";
}

function constSetDeclaration(source: string, name: string): string {
  return source.match(new RegExp(`const ${name} = new Set[\\s\\S]*?\\n\\]\\);`))?.[0] ?? "";
}

test("Session workbench exposes the progressive disclosure layout regions", () => {
  const workbench = sourceFile("features/sessions/components/SessionWorkbench.vue");
  const toolbar = sourceFile("features/sessions/components/SessionToolbar.vue");

  assert.match(workbench, /class="session-command-bar"/);
  assert.match(workbench, /class="session-rail"/);
  assert.match(workbench, /class="session-workspace"/);
  assert.match(workbench, /class="session-inspector"/);
  assert.doesNotMatch(workbench, /SessionEventPanel|session-event-drawer/);
  assert.match(toolbar, /sessions\.newConnection/);
});

test("Session connection entry and host form filling share the dialog while keeping the rail usable", () => {
  const appState = sourceFile("app/composables/useVncAppState.ts");
  const toolbar = sourceFile("features/sessions/components/SessionToolbar.vue");
  const tabs = sourceFile("features/sessions/components/SessionTabs.vue");
  const layoutStyles = sourceFile("app/styles/session-layout.css");
  const tabStyles = sourceFile("app/styles/session-tabs.css");

  assert.doesNotMatch(appState, /sessionConnectDrawerOpen|saveTemporaryHostFromSession/);
  assert.match(appState, /await options\.openTemporaryConnectionConfirm\(\)/);
  assert.match(toolbar, /@click="openTemporaryConnectionConfirm"/);
  assert.doesNotMatch(toolbar, /<details|NInput|NSwitch/);
  assert.match(tabs, /class="session-rail-header"/);
  assert.match(tabs, /class="session-rail-empty"/);
  assert.doesNotMatch(layoutStyles, /\.session-connect-/);
  assert.match(tabStyles, /\.session-tabs/);
  assert.match(tabStyles, /\.session-tab/);
});

test("Session toolbar primary connect action opens the connection confirm flow", () => {
  const toolbar = sourceFile("features/sessions/components/SessionToolbar.vue");

  assert.match(toolbar, /<NButton\s+type="primary"\s+@click="openTemporaryConnectionConfirm"/);
  assert.match(toolbar, /sessions\.newConnection/);
  assert.doesNotMatch(toolbar, /<summary|ChevronDownOutline/);
  assert.doesNotMatch(toolbar, /toolbar\.disconnect/);
});

test("Session workspace exposes manual screenshot and recording artifacts", () => {
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const toolbox = sourceFile("features/sessions/components/SessionToolbox.vue");
  const diagnosticsPanel = sourceFile("features/sessions/components/SessionDiagnosticsPanel.vue");
  const appState = sourceFile("app/composables/useVncAppState.ts");
  const settings = sourceFile("features/settings/components/SettingsWorkbench.vue");
  const settingsState = sourceFile("features/settings/composables/useSettingsWorkbench.ts");
  const artifacts = sourceFile("features/sessions/composables/useSessionArtifacts.ts");
  const capability = readFileSync(new URL("../../../src-tauri/capabilities/default.json", import.meta.url), "utf8");

  assert.match(viewportGrid, /SessionViewportChrome/);
  assert.match(toolbox, /captureSessionScreenshot/);
  assert.match(toolbox, /toggleSessionRecording/);
  assert.match(diagnosticsPanel, /session-artifacts/);
  assert.match(diagnosticsPanel, /activeSessionArtifacts/);
  assert.match(diagnosticsPanel, /copySessionArtifactPath/);
  assert.match(diagnosticsPanel, /revealSessionArtifactFile/);
  assert.match(diagnosticsPanel, /openSessionArtifactFile/);
  assert.doesNotMatch(diagnosticsPanel, /setSessionArtifactDirectory/);
  assert.match(settings, /artifactDirectory/);
  assert.match(settings, /readonly/);
  assert.doesNotMatch(settings, /@update:value="setSessionArtifactDirectory"/);
  assert.match(settings, /artifactDirectoryOptions/);
  assert.match(settings, /setSessionArtifactDirectory/);
  assert.match(settings, /revealSessionArtifactDirectory/);
  assert.match(settingsState, /inspect_session_artifact_directory/);
  assert.match(settingsState, /list_file_access_directories/);
  assert.match(settingsState, /grant_file_access_directory/);
  assert.match(settingsState, /revoke_file_access_directory/);
  assert.match(settingsState, /unavailableFileAccessDirectories/);
  assert.match(settingsState, /settings\.fileAccess\.unavailableCount/);
  assert.match(settingsState, /async function setSessionArtifactDirectory/);
  assert.match(settingsState, /await inspectSessionArtifactDirectory\(sessionSettings\.value\.artifactDirectory\);[\s\S]*await loadFileAccessDirectories\(\);/);
  assert.doesNotMatch(settingsState, /revealItemInDir/);
  assert.match(artifacts, /ensureArtifactDirectoryReady/);
  assert.match(artifacts, /reveal_authorized_path/);
  assert.match(artifacts, /open_authorized_path/);
  assert.match(artifacts, /grant_file_access_directory/);
  assert.doesNotMatch(artifacts, /@tauri-apps\/plugin-opener/);
  assert.doesNotMatch(capability, /"opener:allow-open-path"/);
  assert.doesNotMatch(capability, /"opener:allow-reveal-item-in-dir"/);
  assert.doesNotMatch(capability, /"\$DOWNLOAD\/\*\*"/);
  assert.doesNotMatch(capability, /"\$HOME\/\*\*"/);
  assert.match(appState, /useSessionArtifacts/);
});

test("Session external window keeps the glass status HUD and toolbox button visible", () => {
  const externalWindow = sourceFile("features/sessions/components/SessionExternalWindowPage.vue");
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const viewportChrome = sourceFile("features/sessions/components/SessionViewportChrome.vue");
  const layoutStyles = sourceFile("app/styles/session-layout.css");

  assert.match(viewportGrid, /import SessionViewportChrome/);
  assert.match(externalWindow, /import SessionViewportChrome/);
  assert.match(viewportGrid, /<SessionViewportChrome/);
  assert.match(externalWindow, /<SessionViewportChrome/);
  assert.match(viewportChrome, /import SessionToolbox/);
  assert.doesNotMatch(viewportChrome, /import GlobalTransferStatus/);
  assert.match(externalWindow, /const externalHudTab = computed/);
  assert.match(externalWindow, /createSessionViewportHudState\(externalWindowLocation\?\.sessionId\)/);
  assert.match(externalWindow, /sessionTabs\.value\.find[\s\S]*\?\? \{/);
  assert.match(viewportChrome, /class="remote-viewport-floating-stack"/);
  assert.match(viewportChrome, /'remote-viewport-hud'/);
  assert.match(viewportChrome, /<SessionToolbox/);
  assert.doesNotMatch(viewportChrome, /<GlobalTransferStatus/);
  assert.doesNotMatch(externalWindow, /SessionDiagnosticsPanel/);
  assert.doesNotMatch(externalWindow, /remote-viewport-hud-heading/);
  assert.doesNotMatch(viewportGrid, /remote-viewport-hud-heading/);
  assert.match(layoutStyles, /\.session-external-window-page \.remote-viewport-floating-stack/);
  assert.doesNotMatch(layoutStyles, /\.session-external-window-inspector/);
});

test("Session external window toolbox uses main-window control state and action proxy", () => {
  const externalState = sourceFile("session/external-window/sessionExternalWindowState.ts");
  const externalWindow = sourceFile("session/external-window/sessionExternalWindow.ts");
  const appState = sourceFile("app/composables/useVncAppState.ts");
  const sessionToolbox = sourceFile("features/sessions/components/SessionToolbox.vue");
  const externalWindowPage = sourceFile("features/sessions/components/SessionExternalWindowPage.vue");
  const viewportChrome = sourceFile("features/sessions/components/SessionViewportChrome.vue");

  assert.match(externalState, /controlSnapshot: SessionExternalWindowControlSnapshot/);
  assert.match(externalWindow, /export type SessionExternalWindowControlSnapshot/);
  assert.match(externalWindow, /statusHud: RemoteViewportStatusHudSummary/);
  assert.match(externalWindow, /viewportTone: RemoteViewportTone/);
  assert.match(externalWindow, /export function sessionExternalWindowToolboxCommandEvent/);
  assert.match(appState, /function createExternalWindowControlSnapshot\(sessionId: string\)/);
  assert.match(appState, /const statusHud = options\.getRemoteViewportStatusHud\(snapshotTab\)/);
  assert.match(appState, /const viewportSummary = options\.getRemoteViewportSummary\(snapshotTab\)/);
  assert.match(appState, /const viewportTone = viewportSummary\.tone/);
  assert.match(appState, /createExternalWindowStateBridge/);
  assert.match(sessionToolbox, /contextOverride/);
  assert.match(sessionToolbox, /sessionExternalWindowToolboxCommandEvent/);
  assert.match(sessionToolbox, /emit\(sessionExternalWindowToolboxCommandEvent\(props\.sessionId\)/);
  assert.match(sessionToolbox, /listen<SessionExternalWindowToolboxCommandPayload>/);
  assert.match(externalWindowPage, /externalState\.value\?\.controlSnapshot\.toolboxContext/);
  assert.match(externalWindowPage, /:status-hud-override="externalStatusHud"/);
  assert.match(externalWindowPage, /:viewport-tone-override="externalViewportTone"/);
  assert.match(externalWindowPage, /const externalStatusHud = computed/);
  assert.match(externalWindowPage, /if \(coreSurfacePhase\.value === "failed"\)[\s\S]*state: t\("sessions\.externalSurface\.interrupted"\), inputMode: "readonly"/);
  assert.match(viewportChrome, /statusHudOverride\?: RemoteViewportStatusHudSummary \| null/);
  assert.match(viewportChrome, /const statusHud = computed/);
  assert.match(externalWindowPage, /:toolbox-context-override=/);
});

test("Session rail focuses an externally owned child window when clicking its tab", () => {
  const tabs = sourceFile("features/sessions/components/SessionTabs.vue");
  const coordinator = sourceFile("features/sessions/composables/useSessionExternalWindowCoordinator.ts");
  const appState = sourceFile("app/composables/useVncAppState.ts");
  const command = readFileSync(new URL("../../../src-tauri/src/commands/mod.rs", import.meta.url), "utf8");
  const lib = readFileSync(new URL("../../../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(coordinator, /async function focusExternalWindowSession\(sessionId: string\): Promise<void>/);
  assert.match(coordinator, /sessionExternalWindowLabel/);
  assert.match(coordinator, /invoke\("focus_session_external_window", \{ label: sessionExternalWindowLabel\(/);
  assert.doesNotMatch(coordinator, /setAlwaysOnTop/);
  assert.match(command, /fn focus_session_external_window\(app: AppHandle, label: String\) -> Result<\(\), String>/);
  assert.match(command, /app\.get_webview_window\(&label\)/);
  assert.match(command, /window\s*\.show\(\)/);
  assert.match(command, /window[\s\S]*?\.unminimize\(\)/);
  assert.match(command, /window[\s\S]*?\.set_focus\(\)/);
  assert.match(lib, /focus_session_external_window,/);
  assert.match(appState, /focusExternalWindowSession,/);
  assert.match(tabs, /focusExternalWindowSession/);
  assert.match(tabs, /function handleSessionTabClick\(tabId: string\)/);
  assert.match(tabs, /if \(getWorkspaceRenderSlot\(tabId\)\.externalWindowOwned\) \{\s*void focusExternalWindowSession\(tabId\);\s*return;/);
  assert.match(tabs, /void activateSessionTab\(tabId\)/);
  assert.match(tabs, /<div[\s\S]*@click="handleSessionTabClick\(tab\.id\)"/);
  assert.match(tabs, /@click\.stop="handleSessionTabClick\(tab\.id\)"/);
});

test("Session external window keeps local window actions local and disables fullscreen exit", () => {
  const sessionToolbox = sourceFile("features/sessions/components/SessionToolbox.vue");
  const externalWindowPage = sourceFile("features/sessions/components/SessionExternalWindowPage.vue");
  const externalWindowRuntime = sourceFile("features/sessions/composables/sessionExternalWindowRuntime.ts");
  const appState = sourceFile("app/composables/useVncAppState.ts");
  const externalLocalCommands = constSetDeclaration(sessionToolbox, "EXTERNAL_WINDOW_LOCAL_COMMAND_KINDS");

  assert.match(externalLocalCommands, /const EXTERNAL_WINDOW_LOCAL_COMMAND_KINDS = new Set<MenuCommand\["kind"\]>\(\[/);
  assert.match(externalLocalCommands, /"display-scale"/);
  assert.doesNotMatch(externalLocalCommands, /"file-transfer-panel"/);
  assert.doesNotMatch(externalLocalCommands, /"reset-position"/);
  assert.doesNotMatch(externalLocalCommands, /"remote-upload-action",/);
  assert.match(sessionToolbox, /command\.actionId === "toggle-external-window"/);
  assert.match(sessionToolbox, /command\.actionId === "exit-fullscreen"/);
  assert.match(sessionToolbox, /return false;[\s\S]*toggle-external-window/);
  assert.match(sessionToolbox, /setSessionViewportScaleMode\(props\.sessionId, command\.value\)/);
  assert.match(sessionToolbox, /case "open-file-transfer-center":[\s\S]*sessionExternalTransferDrawerRequestEvent\(sessionId\)[\s\S]*openGlobalTransferDrawer\(sessionId\)/);
  assert.match(sessionToolbox, /props\.toggleExternalWindowFullscreen\(sessionId\)/);
  assert.match(externalWindowPage, /searchParams\.get\("fullscreen"\) === "true"/);
  assert.match(externalWindowPage, /fullscreen: externalWindowFullscreen\.value/);
  assert.match(externalWindowPage, /setSessionWindowExpanded\(currentWindow, !fullscreen\)/);
  assert.match(externalWindowRuntime, /nativeSessionWindowOptions\(request\.fullscreen\)/);
  assert.match(externalWindowRuntime, /setSessionWindowExpanded\(handle, fullscreen\)/);
  assert.match(appState, /fullscreen: false/);
});

test("Session external window proxies main-window actions with child-window feedback", () => {
  const externalWindow = sourceFile("session/external-window/sessionExternalWindow.ts");
  const sessionToolbox = sourceFile("features/sessions/components/SessionToolbox.vue");
  const externalWindowPage = sourceFile("features/sessions/components/SessionExternalWindowPage.vue");

  assert.match(externalWindow, /export type SessionExternalWindowToolboxCommandResultPayload/);
  assert.match(externalWindow, /export function sessionExternalWindowToolboxCommandResultEvent/);
  assert.match(sessionToolbox, /sessionExternalWindowToolboxCommandResultEvent/);
  assert.match(sessionToolbox, /feedbackTarget: "external"/);
  assert.match(sessionToolbox, /createToolboxFeedback/);
  assert.match(sessionToolbox, /showToolboxFeedback/);
  assert.match(sessionToolbox, /emit\(sessionExternalWindowToolboxCommandResultEvent\(props\.sessionId\)/);
  assert.match(externalWindowPage, /listen<SessionExternalWindowToolboxCommandResultPayload>/);
  assert.match(externalWindowPage, /showExternalWindowFeedback\(event\.payload\.message, event\.payload\.tone\)/);
});

test("Session external window carries file-transfer availability from main window", () => {
  const externalWindow = sourceFile("session/external-window/sessionExternalWindow.ts");
  const appState = sourceFile("app/composables/useVncAppState.ts");
  const sessionToolbox = sourceFile("features/sessions/components/SessionToolbox.vue");
  const externalWindowPage = sourceFile("features/sessions/components/SessionExternalWindowPage.vue");

  assert.match(externalWindow, /fileTransferAvailable: boolean/);
  assert.match(appState, /const fileTransferAvailable = sessionSupportsAction\(connection\?\.connectionModel\.capabilities, "sendFile"\)/);
  assert.match(appState, /fileTransferAvailable,/);
  assert.match(externalWindowPage, /fileTransferAvailable: synchronizedContext\.fileTransferAvailable \|\| externalState\.value\?\.controlSnapshot\.fileTransferAvailable/);
  assert.match(sessionToolbox, /resolveSessionToolboxPresentation\(toolboxContext\.value\)/);
  assert.match(sessionToolbox, /toolboxContext\.value\.fileTransferAvailable[\s\S]*presentation\.value\.connected/);
  assert.match(sessionToolbox, /action\.id === "send-file" && toolboxContext\.value\.fileTransferAvailable/);
});

test("Session external window handles local shortcuts instead of only dock shortcut", () => {
  const page = sourceFile("features/sessions/components/SessionExternalWindowPage.vue");
  assert.match(page, /matchSessionLocalShortcut/);
  assert.match(page, /executeExternalSessionLocalShortcut/);
  assert.match(page, /case "upload-file":[\s\S]*selectFileTransferSources/);
  assert.match(page, /case "toggle-external-window":[\s\S]*requestDockExternalWindowSession/);
  assert.ok(page.indexOf("const localShortcut = matchSessionLocalShortcut") < page.indexOf("if (!externalSurface.canForwardInput(externalState.value?.inputEnabled === true))"));
});

test("Settings workbench groups connection defaults, file access, shortcuts and diagnostics", () => {
  const settings = sourceFile("features/settings/components/SettingsWorkbench.vue");
  const sessionPanel = settingsPanel(settings, "session");
  const filesPanel = settingsPanel(settings, "files");
  const shortcutsPanel = settingsPanel(settings, "shortcuts");
  const sharingPanel = settingsPanel(settings, "sharingService");
  const aboutPanel = settingsPanel(settings, "about");
  const categories = [...settings.matchAll(/\{ id: "([a-zA-Z]+)", labelKey:/g)].map(match => match[1]);

  assert.deepEqual(categories, ["session", "files", "shortcuts", "sharingService", "about"]);
  assert.match(settings, /:aria-pressed="activeSettingsCategory === category\.id"/);
  assert.match(settings, /watch\(requestedSettingsCategory/);
  assert.doesNotMatch(settings, /data-settings-panel="security"/);
  assert.match(sessionPanel, /postConnectPresentation/);
  assert.match(sessionPanel, /NRadioGroup/);
  assert.match(sessionPanel, /:value="sessionSettings\.connectionDefaults\.readonly"/);
  assert.match(sessionPanel, /@update:value="setConnectionDefault\('readonly', \$event\)"/);
  assert.match(sessionPanel, /:value="sessionSettings\.confirmBeforeConnect"/);
  assert.match(sessionPanel, /@update:value="setConfirmBeforeConnect"/);
  assert.doesNotMatch(sessionPanel, /artifactDirectory|fileAccess/);

  assert.match(filesPanel, /aria-labelledby="settings-artifact-directory-title"/);
  assert.match(filesPanel, /aria-labelledby="settings-file-access-title"/);
  assert.match(filesPanel, /:value="sessionSettings\.artifactDirectory \|\| null"/);
  assert.match(filesPanel, /:options="artifactDirectoryOptions"/);
  assert.match(filesPanel, /@update:value="setSessionArtifactDirectory\(\$event as string\)"/);
  assert.match(filesPanel, /aria-describedby="settings-artifact-directory-description"/);
  assert.match(filesPanel, /:disabled="!canRevealSessionArtifactDirectory"/);
  assert.match(filesPanel, /@click\.stop="revealSessionArtifactDirectory"/);
  assert.match(filesPanel, /@click="chooseFileAccessDirectory"/);
  assert.match(filesPanel, /fileAccessMessageToneClass\(fileAccessMessageTone\)/);
  assert.match(filesPanel, /fileAccessDirectoryStatusToneClass\(directory\.status\)/);
  assert.match(filesPanel, /:disabled="directory\.status !== 'ready'"/);
  assert.match(filesPanel, /@click="revealFileAccessDirectory\(directory\.path\)"/);
  assert.match(filesPanel, /@click="revokeFileAccessDirectory\(directory\.path\)"/);
  assert.match(filesPanel, /@click="chooseSessionArtifactDirectory"/);
  assert.match(filesPanel, /:loading="fileAccessDirectoryChoosing"/);
  assert.match(filesPanel, /artifact-directory-empty-picker/);

  assert.match(shortcutsPanel, /sessionShortcutRows/);
  assert.match(shortcutsPanel, /row\.nameKey/);
  assert.match(shortcutsPanel, /<kbd/);
  assert.ok(shortcutsPanel.indexOf("sessionShortcutRows") < shortcutsPanel.indexOf("keyboardTransmission"));
  assert.match(shortcutsPanel, /<table/);
  assert.match(shortcutsPanel, /settings\.keyboardTransmission\.localKey/);
  assert.match(shortcutsPanel, /settings\.keyboardTransmission\.sentKey/);
  assert.match(sharingPanel, /:value="sharingAutoStart"/);
  assert.match(sharingPanel, /:loading="sharingAutoStartBusy"/);
  assert.match(sharingPanel, /:disabled="sharingAutoStartBusy \|\| !sharingAutoStartKnown"/);
  assert.match(sharingPanel, /@update:value="setSharingAutoStart"/);

  assert.match(settings, /fallbackAppMetadata, loadAppMetadata/);
  assert.match(settings, /onMounted\(async \(\) => \{[\s\S]*appMetadata\.value = await loadAppMetadata\(\)/);
  assert.match(aboutPanel, /appMetadata\.name/);
  assert.doesNotMatch(aboutPanel, /appMetadata\.description/);
  assert.match(aboutPanel, /appMetadata\.version/);
  assert.match(settings, /const showBuildRuntimeInfo = import\.meta\.env\.DEV;/);
  assert.match(aboutPanel, /<details v-if="showBuildRuntimeInfo" class="settings-about-debug"/);
  assert.doesNotMatch(aboutPanel, /showDeveloperDebugInfo/);
  assert.match(aboutPanel, /<summary>/);
  assert.match(aboutPanel, /platformLabel/);
});

test("Session toolbox centralizes viewport actions in per-viewport popup menus", () => {
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const viewportChrome = sourceFile("features/sessions/components/SessionViewportChrome.vue");
  const toolbox = sourceFile("features/sessions/components/SessionToolbox.vue");
  const toolboxState = sourceFile("features/sessions/composables/useSessionToolbox.ts");
  const toolboxDomain = sourceFile("session/transfers/sessionToolbox.ts");
  const styles = sourceFile("app/styles/session-viewport.css");

  assert.match(viewportChrome, /<SessionToolbox/);
  assert.match(viewportChrome, /:session-id="tab\.id"/);
  assert.doesNotMatch(viewportChrome, /<SessionToolbox[\s\S]*:workspace-target="workspaceTarget"/);
  assert.doesNotMatch(viewportGrid, /class="remote-viewport-actions"/);
  assert.match(toolbox, /const props = defineProps<\{[\s\S]*sessionId: string/);
  assert.match(toolbox, /NDropdown/);
  assert.match(toolbox, /NIcon/);
  assert.match(toolbox, /OptionsOutline/);
  assert.doesNotMatch(toolbox, /sessions\.toolbox\.shortTitle/);
  assert.match(toolbox, /toolboxMenuOptions/);
  assert.match(toolbox, /@select="handleMenuSelect"/);
  assert.match(toolbox, /@clickoutside="handleToolboxClickOutside"/);
  assert.match(toolbox, /function handleToolboxClickOutside\(event: Event\)/);
  assert.match(toolbox, /isToolboxTriggerEvent\(event\)/);
  assert.match(toolbox, /function isToolboxTriggerEvent\(event: Event\): boolean/);
  assert.match(toolbox, /\.session-toolbox-trigger/);
  assert.match(toolbox, /trigger="manual"/);
  assert.match(toolbox, /:show="toolboxOpen"/);
  assert.match(toolbox, /class="session-toolbox-trigger"[\s\S]*:style="buttonStyle"/);
  assert.match(toolbox, /@click\.stop="handleToolboxButtonClick"/);
  assert.match(toolbox, /@pointerdown\.stop="startToolboxDrag"/);
  assert.doesNotMatch(toolbox, /class="session-toolbox-panel"/);
  assert.match(toolbox, /resolveSessionToolboxActions/);
  assert.match(toolbox, /function actionLabelKey\(action: SessionToolboxAction\): string/);
  assert.match(toolbox, /case "toggle-fullscreen":[\s\S]*toolboxContext\.value\.fullscreen[\s\S]*sessions\.toolbox\.actions\.toggle-fullscreen\.exitLabel[\s\S]*sessions\.toolbox\.actions\.toggle-fullscreen\.enterLabel/);
  assert.match(toolbox, /function menuOptionLabel/);
  assert.match(toolbox, /formatKeyboardShortcutSymbolsForPlatform/);
  assert.match(toolbox, /formatKeyboardShortcutForPlatform/);
  assert.match(toolbox, /session-toolbox-option-shortcut/);
  assert.match(toolbox, /formatShortcutLabel\(shortcutLabel, shortcutDisplay/);
  assert.doesNotMatch(toolbox, /\$\{t\(labelKey\)\}\s+\$\{action\.shortcutLabel\}/);
  assert.match(toolbox, /case "toggle-external-window":[\s\S]*toolboxContext\.value\.externalWindow[\s\S]*sessions\.toolbox\.actions\.toggle-external-window\.exitLabel[\s\S]*sessions\.toolbox\.actions\.toggle-external-window\.enterLabel/);
  assert.match(toolbox, /case "toggle-hud":[\s\S]*props\.isHudHidden\(props\.sessionId\)[\s\S]*sessions\.toolbox\.actions\.toggle-hud\.showLabel[\s\S]*sessions\.toolbox\.actions\.toggle-hud\.hideLabel/);
  assert.match(toolbox, /case "toggle-readonly":[\s\S]*toolboxContext\.value\.readonly[\s\S]*sessions\.toolbox\.actions\.toggle-readonly\.controlLabel[\s\S]*sessions\.toolbox\.actions\.toggle-readonly\.readonlyLabel/);
  assert.match(toolbox, /sendRemoteMessage/);
  assert.doesNotMatch(toolbox, /USER_FACING_DIAGNOSTIC_ACTION_IDS|recover-secure-input|enumerate-agent-displays/);
  assert.match(toolbox, /actionOption\("open-diagnostics"\)/);
  assert.match(toolboxDomain, /id: "more"/);
  assert.match(toolboxState, /const BUTTON_WIDTH = 32/);
  assert.match(toolboxState, /const LONG_PRESS_DELAY_MS = \d+/);
  assert.match(toolboxState, /function handleToolboxButtonClick/);
  assert.match(toolboxState, /suppressNextClick/);
  assert.match(toolboxState, /button\.setPointerCapture\(event\.pointerId\)/);
  assert.match(toolboxState, /capturedPointerElement\.releasePointerCapture\(capturedPointerId\)/);
  assert.match(toolboxState, /pendingDragEvent = \{ \.\.\.pendingDragEvent, event \}/);
  assert.match(toolboxState, /function defaultToolboxPosition/);
  assert.match(toolboxState, /function visibleToolboxPosition/);
  assert.match(toolboxState, /snapDistance: 0/);
  assert.match(toolboxState, /function stopToolboxDrag\(\)[\s\S]*position\.value = visibleToolboxPosition/);
  assert.doesNotMatch(toolboxState, /right: `\$\{fallbackRight\}px`/);
  assert.match(toolboxState, /const buttonRect = button\.getBoundingClientRect\(\)/);
  assert.match(toolboxState, /new ResizeObserver/);
  assert.match(toolboxState, /workspaceRectVersion/);
  assert.match(toolboxState, /resolveVisibleSessionToolboxButton/);
  assert.match(toolboxState, /snapSessionToolboxButton/);
  assert.match(toolboxDomain, /Ctrl\+Alt\+Del/);
  assert.doesNotMatch(toolboxDomain, /lock-screen|logout|reboot|"ctrl-alt-del"|"lock-screen"/);
  assert.match(styles, /\.session-toolbox-button/);
  assert.match(styles, /\.session-toolbox-option-shortcut[\s\S]*font-size: 12px/);
  assert.match(styles, /\.session-toolbox-option-shortcut[\s\S]*justify-self: end/);
  assert.match(styles, /\.session-toolbox-trigger[\s\S]*position: absolute/);
  const toolboxButtonStyle = styles.match(/\.session-toolbox-button \{[\s\S]*?\n\}/)?.[0] ?? "";
  assert.doesNotMatch(toolboxButtonStyle, /position: absolute/);
  assert.match(styles, /\.session-toolbox-button-icon/);
  assert.match(styles, /\.session-toolbox-menu/);
});

test("Session toolbox exposes local display scale choices", () => {
  const toolbox = sourceFile("features/sessions/components/SessionToolbox.vue");

  assert.match(toolbox, /function displayScaleMenuOptions\(\): DropdownOption\[\]/);
  assert.match(toolbox, /submenuOption\("display-scale", t\("sessions\.displayScale"\), displayScaleMenuOptions\(\)\)/);
  assert.match(toolbox, /displayScaleCommandOption\("fit"\)/);
  assert.match(toolbox, /displayScaleCommandOption\("actual"\)/);
  assert.match(toolbox, /displayScaleCommandOption\("scale125"\)/);
  assert.match(toolbox, /displayScaleCommandOption\("scale150"\)/);
  assert.match(toolbox, /displayScaleCommandOption\("scale200"\)/);
  assert.doesNotMatch(toolbox, /resolution-standard|resolution-landscape|resolution-portrait|fhdLandscape|hdPortrait/);
});

test("Session toolbox exposes local Server display choices in the view menu", () => {
  const toolbox = sourceFile("features/sessions/components/SessionToolbox.vue");

  assert.match(toolbox, /submenuOption\("display", t\("sessions\.displaySelect"\), displayMenuOptions\(\)\)/);
  assert.doesNotMatch(toolbox, /getSessionLocalServerView\(props\.sessionId\)\s*\?\s*\[\s*submenuOption\("display"/);
  assert.match(toolbox, /function displayMenuOptions\(\): DropdownOption\[\]/);
  assert.match(toolbox, /displayChoices: createSessionDisplayChoices\(/);
  assert.match(toolbox, /getSessionLocalServerView\(props\.sessionId\)/);
  assert.match(toolbox, /getSessionRemoteDisplayInventory\(props\.sessionId\)/);
  assert.match(toolbox, /getSessionSelectedRemoteDisplayId\(props\.sessionId\)/);
  assert.match(toolbox, /toolboxContext\.value\.displayChoices \?\? \[\]/);
  assert.match(toolbox, /\{ kind: "display", value: display\.value \}/);
  assert.match(toolbox, /display\.selected/);
  assert.match(toolbox, /await selectRemoteAgentDisplay\(props\.sessionId, remoteDisplayId\)/);
  assert.match(toolbox, /setSessionSelectedRemoteDisplayId\(sessionId, displayId\)/);
  assert.doesNotMatch(toolbox, /showAppMessage\(t\("sessions\.toolbox\.disabled\.localServerOnly"\), "warning"\)/);
  assert.doesNotMatch(toolbox, /function canSwitchRemoteAgentDisplay\(\): boolean/);
  assert.doesNotMatch(toolbox, /sessionSupportsAction\(connection\?\.enhancedChannel, "agentScreenStream"\)/);
  assert.doesNotMatch(toolbox, /await connection\.setAgentDisplay/);
  assert.doesNotMatch(toolbox, /setSessionAgentScreenStreamMode/);
  assert.match(toolbox, /displayMenuOptions\(\)\.length > 1/);
  assert.doesNotMatch(toolbox, /getSessionDisplaySummary\(props\.sessionId\)|sessions\.toolbox\.currentDisplay/);
  assert.match(toolbox, /await updateSessionLocalServerView\(props\.sessionId, \{ displayId: command\.value \}\)/);
  assert.match(toolbox, /encodeURIComponent\(command\.value\)/);
  assert.match(toolbox, /decodeURIComponent\(value\)/);
  assert.doesNotMatch(toolbox, /submenuOption\("refresh"/);
  assert.doesNotMatch(toolbox, /kind: "refresh"/);
});

test("Session viewport target refs avoid recursive reactive updates", () => {
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const setTargetFunction = viewportGrid.match(/function setRemoteViewportTarget[\s\S]*?\n}\n/)?.[0] ?? "";

  assert.match(setTargetFunction, /const target = element instanceof HTMLElement \? element : null/);
  assert.match(setTargetFunction, /if \(viewportTargetsBySessionId\.value\[sessionId\] === target\) \{\s*return;\s*\}/);
  assert.match(setTargetFunction, /viewportTargetsBySessionId\.value = \{/);
});

test("Session viewport status HUD uses a compact bottom-right card layout", () => {
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const viewportChrome = sourceFile("features/sessions/components/SessionViewportChrome.vue");
  const styles = sourceFile("app/styles/session-viewport.css");
  const toolbox = sourceFile("features/sessions/components/SessionToolbox.vue");

  assert.match(viewportChrome, /function viewportStateClass\(state: string\): string/);
  assert.match(viewportGrid, /createSessionViewportHudState\(\)/);
  assert.match(viewportChrome, /const statusHud = computed\(\(\) => props\.statusHudOverride \?\? getRemoteViewportStatusHud\(props\.tab\)\)/);
  assert.doesNotMatch(viewportChrome, /showDeveloperDebugInfo/);
  assert.match(viewportChrome, /\{\{ statusHud\.title \}\}/);
  assert.match(viewportChrome, /\{\{ statusHud\.target \}\}/);
  assert.match(viewportChrome, /\{\{ statusHud\.duration \}\}/);
  assert.match(viewportChrome, /sessions\.inputModes\.\$\{statusHud\.inputMode\}/);
  assert.match(viewportChrome, /t\("sessions\.statusHud\.display"\)/);
  assert.doesNotMatch(viewportChrome, /statusHud\.transport|statusHud\.latestEvent/);
  assert.match(viewportChrome, /:title="statusHud\.display"/);
  assert.doesNotMatch(viewportChrome, /class="remote-viewport-hud-actions"/);
  assert.match(styles, /\.remote-viewport-floating-stack\s*\{[\s\S]*?right: 20px/);
  assert.match(styles, /\.remote-viewport-floating-stack\s*\{[\s\S]*?bottom: 20px/);
  assert.match(styles, /\.remote-viewport-hud\s*\{[\s\S]*?border: 1px solid rgba\(255, 255, 255, 0\.38\)/);
  assert.match(styles, /\.remote-viewport-hud\s*\{[\s\S]*?background: rgba\(248, 251, 255, 0\.74\)/);
  assert.match(styles, /\.remote-viewport-hud\s*\{[\s\S]*?backdrop-filter: blur\(18px\) saturate\(1\.25\)/);
  assert.match(styles, /\.remote-viewport-hud\.collapsed\s*\{[\s\S]*?width: 40px/);
  assert.match(styles, /\.remote-viewport-hud\.expanded\s*\{[\s\S]*?width: 220px;[\s\S]*?max-width: 100%/);
  assert.match(styles, /\.remote-viewport-hud-progress/);
  assert.match(styles, /\.remote-viewport-hud-badge/);
  assert.match(styles, /\.remote-viewport-state::before/);
  assert.match(styles, /\.remote-viewport-state\.state-disconnected/);
  assert.match(styles, /\.remote-viewport-state\.state-disconnected::before/);
  assert.match(toolbox, /actionOption\("exit-fullscreen"\)/);
});

test("Session viewport status HUD keeps its fullscreen layout", () => {
  const styles = sourceFile("app/styles/session-viewport.css");

  assert.match(styles, /\.viewport-fullscreen-active \.remote-viewport\.visible \.remote-viewport-floating-stack \{[\s\S]*?right: 16px/);
  assert.match(styles, /\.viewport-fullscreen-active \.remote-viewport\.visible \.remote-viewport-floating-stack \{[\s\S]*?bottom: 16px/);
});

test("Session viewport keeps only the status HUD in the bottom-right floating stack", () => {
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const viewportChrome = sourceFile("features/sessions/components/SessionViewportChrome.vue");
  const externalWindowPage = sourceFile("features/sessions/components/SessionExternalWindowPage.vue");
  const styles = sourceFile("app/styles/session-viewport.css");
  const layoutStyles = sourceFile("app/styles/session-layout.css");

  assert.doesNotMatch(viewportGrid, /:show-transfer-status=/);
  assert.match(viewportChrome, /<div class="remote-viewport-floating-stack">/);
  assert.doesNotMatch(viewportChrome, /<GlobalTransferStatus/);
  assert.doesNotMatch(viewportGrid, /:is-file-transfer-entry-hidden=/);
  assert.doesNotMatch(viewportGrid, /:toggle-file-transfer-entry-hidden=/);
  assert.match(viewportChrome, /<button\s+v-if="!props\.isHudHidden\(tab\.id\)"[\s\S]*remote-viewport-hud/);
  assert.match(styles, /\.remote-viewport-floating-stack\s*\{[\s\S]*?right: 20px/);
  assert.match(styles, /\.remote-viewport-floating-stack\s*\{[\s\S]*?bottom: 20px/);
  assert.match(viewportGrid, /'hud-visible': !isViewportHudHidden\(tab\.id\)/);
  assert.match(viewportGrid, /'hud-expanded': !isViewportHudHidden\(tab\.id\) && isViewportHudExpanded\(tab\.id\)/);
  assert.match(externalWindowPage, /'hud-visible': externalWindowLocation && !isViewportHudHidden\(externalWindowLocation\.sessionId\)/);
  assert.doesNotMatch(styles, /session-file-transfer-panel/);
  assert.doesNotMatch(styles, /session-file-transfer-inline-offset/);
  assert.doesNotMatch(layoutStyles, /session-file-transfer-inline-offset/);
});

test("Session viewport removes file-transfer floating-card styles", () => {
  const styles = sourceFile("app/styles/session-viewport.css");

  assert.doesNotMatch(styles, /session-file-transfer-icon-button/);
  assert.doesNotMatch(styles, /session-file-transfer-task-button/);
});

test("Session viewport removes the old bottom status footer", () => {
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const styles = sourceFile("app/styles/session-viewport.css");

  assert.doesNotMatch(viewportGrid, /remote-viewport-footer/);
  assert.doesNotMatch(viewportGrid, /getRemoteViewportStatusBar\(tab\)/);
  assert.doesNotMatch(styles, /\.remote-viewport-footer/);
});

test("Session viewport no longer measures a bottom status footer", () => {
  const viewportGrid = sourceFile("features/sessions/components/SessionViewportGrid.vue");
  const styles = sourceFile("app/styles/session-viewport.css");

  assert.doesNotMatch(viewportGrid, /new ResizeObserver/);
  assert.doesNotMatch(viewportGrid, /setRemoteViewportFooter/);
  assert.doesNotMatch(viewportGrid, /isStatusFooterCompact/);
  assert.doesNotMatch(viewportGrid, /measureRemoteViewportFooter/);
  assert.doesNotMatch(viewportGrid, /scheduleRemoteViewportFooterMeasurements/);
  assert.doesNotMatch(styles, /\.remote-viewport-footer\.compact/);
});

test("Session viewport removes the inline status-bar restore button when the HUD is hidden", () => {
  const viewportChrome = sourceFile("features/sessions/components/SessionViewportChrome.vue");
  const styles = sourceFile("app/styles/session-viewport.css");

  assert.match(viewportChrome, /v-if="!props\.isHudHidden\(tab\.id\)"[\s\S]*class="\[[\s\S]*'remote-viewport-hud'/);
  assert.doesNotMatch(viewportChrome, /remote-viewport-hud-toggle/);
  assert.doesNotMatch(styles, /\.remote-viewport-hud-toggle/);
});
