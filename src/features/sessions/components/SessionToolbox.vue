<script setup lang="ts">
import { remoteMessageTextError } from "../../../session/messages/remoteMessage.ts";
import { sendExternalRemoteMessage } from "../../../session/messages/externalRemoteMessage.ts";
import { createSystemClipboard } from "../../../session/clipboard/systemClipboard.ts";
import { editMessageDraftClipboard, messageDraftEditShortcut } from "../../../session/messages/messageDraftClipboard.ts";
import { computed, h, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { FolderOpenOutline, OptionsOutline, UnlinkOutline } from "@vicons/ionicons5";
import { NButton, NDropdown, NIcon, NInput, NModal } from "naive-ui";
import type { DropdownOption } from "naive-ui";
import SessionRecordingIndicator from "./SessionRecordingIndicator.vue";
import SessionViewportPlaceholder from "./SessionViewportPlaceholder.vue";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import {
  createSessionToolboxActionRequest,
  listSessionToolboxGroups,
  resolveSessionToolboxActions,
  resolveSessionToolboxPresentation,
  type SessionToolboxAction,
  type SessionToolboxActionId,
  type SessionToolboxContext,
  type SessionToolboxGroupId,
} from "../../../session/transfers/sessionToolbox.ts";
import {
  sessionExternalWindowToolboxCommandResultEvent,
  sessionExternalWindowToolboxCommandEvent,
  sessionExternalTransferDrawerControlEvent,
  sessionExternalTransferDrawerRequestEvent,
  sessionExternalTransferDrawerSnapshotEvent,
  type SessionExternalTransferDrawerControl,
  type SessionExternalWindowToolboxCommandPayload,
  type SessionExternalWindowToolboxCommandResultPayload,
} from "../../../session/external-window/sessionExternalWindow.ts";
import {
  formatKeyboardShortcutForPlatform,
  formatKeyboardShortcutSymbolsForPlatform,
  type KeyboardShortcutDisplayPlatform,
} from "../../../session/input/keyboardShortcut.ts";
import type { SessionViewportScaleMode } from "../../../session/viewport/sessionViewportScale.ts";
import { sessionSupportsCapability } from "../../../session/connection-model/connectionModel.ts";
import { createSessionDisplayChoices } from "../../../session/viewport/sessionDisplayChoices.ts";
import { useTransferTaskActions } from "../../transfers/composables/useTransferTaskActions.ts";
import { useSessionToolbox } from "../composables/useSessionToolbox.ts";
import { useSessionKeyboardPreferences } from "../composables/useSessionKeyboardPreferences.ts";
import { buildSessionDiagnosticView, type SessionDiagnosticView } from "../../../session/diagnostics/sessionDiagnosticView.ts";
import { summarizeSessionDiagnosticsPanel } from "../../../session/diagnostics/sessionDiagnosticsPanel.ts";
import SessionDiagnosticContent from "./SessionDiagnosticContent.vue";
import {
  MACOS_SHORTCUT_ACTION_IDS,
  WINDOWS_SHORTCUT_ACTION_IDS,
  commandKey,
  isDropdownOption,
  isToolboxTriggerEvent,
  isSessionToolboxEvent,
  parseCommandKey,
  remoteDisplayIdFromCommandValue,
  type MenuCommand,
} from "./sessionToolbox/menuOptions.ts";
import {
  dispatchFileTransferAction,
  resolveCurrentFileTransferReceiveDirectory,
} from "./sessionToolbox/fileTransfer.ts";

type ShortcutDisplay = KeyboardShortcutDisplayPlatform;

const EXTERNAL_WINDOW_LOCAL_COMMAND_KINDS = new Set<MenuCommand["kind"]>([
  "display-scale",
]);

const props = defineProps<{
  sessionId: string;
  viewportTarget: HTMLElement | null;
  isHudHidden: (sessionId: string) => boolean;
  toggleHud: (sessionId: string) => void;
  toggleExternalWindowFullscreen?: (sessionId: string) => Promise<void>;
  contextOverride?: SessionToolboxContext | null;
}>();

const viewportRef = computed(() => props.viewportTarget);
const recordingStopRequested = ref(false);
let recordingStopResetTimer: number | null = null;

function resetRecordingStopRequest() {
  if (recordingStopResetTimer !== null) window.clearTimeout(recordingStopResetTimer);
  recordingStopResetTimer = null;
  recordingStopRequested.value = false;
}

const {
  activateSessionTab,
  activeRemoteSession,
  captureSessionScreenshot,
  eventLog,
  exitSessionFullscreen,
  externalWindowLocation,
  findSession,
  fullscreenSessionId,
  getRemoteViewportSummary,
  getSessionLocalServerView,
  getSessionRemoteDisplayInventory,
  getSessionSelectedRemoteDisplayId,
  getSessionViewportScaleMode,
  globalTransferDrawerOpen,
  globalTransferDrawerSessionId,
  isSessionRecording,
  getSessionRecordingPhase,
  getSessionRecordingElapsedSeconds,
  getSessionRecordingSpace,
  logEvent,
  nowTick,
  openGlobalTransferDrawer,
  openExternalWindowSession,
  recordMainViewportInput,
  retrySessionFromViewport,
  sessionConnectionRegistry,
  sessionDiagnosticsSummary,
  sessionListTick,
  sessionMessageDialogRequest,
  sessionTabs,
  setSessionReadonlyMode,
  setSessionSelectedRemoteDisplayId,
  setSessionViewportScaleMode,
  showAppMessage,
  t,
  toggleExternalWindowSession,
  toggleSessionFullscreen,
  toggleSessionRecording,
  enqueueSessionFileUploads,
  sessionFileTransferManager,
  updateSessionLocalServerView,
} = useVncAppContext();

const { preferences: keyboardPreferences, windowsClient, setPreference: setKeyboardPreference } = useSessionKeyboardPreferences(
  () => findSession(props.sessionId)?.target ?? externalWindowLocation?.target ?? "",
);

const { runTaskPathAction } = useTransferTaskActions();

const textDraft = ref("");
const textDialogOpen = ref(false);
const textSending = ref(false);
let messageSendController: AbortController | null = null;
const diagnosticDialogOpen = ref(false);
const externalDiagnosticView = ref<SessionDiagnosticView | null>(null);
const externalFileTransferSourcePaths = ref<string[] | null>(null);
const externalTransferDrawerOpen = ref(false);
let externalToolboxFeedbackTarget: SessionExternalWindowToolboxCommandPayload["feedbackTarget"] | null = null;
let unlistenExternalToolboxCommand: UnlistenFn | null = null;
let unlistenExternalDiagnosticView: UnlistenFn | null = null;
let disposed = false;
let unlistenExternalTransferDrawerRequest: UnlistenFn | null = null;
let unlistenExternalTransferDrawerControl: UnlistenFn | null = null;

function publishExternalTransferTasks() {
  if (!externalTransferDrawerOpen.value) return;
  void emit(sessionExternalTransferDrawerSnapshotEvent(props.sessionId), {
    tasks: sessionFileTransferManager.tasks.value.filter((task) => task.sessionId === props.sessionId)
      .map(({ filePath: _filePath, ...task }) => task),
    clearingHistory: sessionFileTransferManager.clearingHistory.value,
  });
}

const {
  buttonStyle,
  closeToolbox,
  handleToolboxButtonClick,
  startToolboxDrag,
  toolboxOpen,
} = useSessionToolbox({
  workspaceRef: viewportRef,
  isInside: (event) => isSessionToolboxEvent(event, props.sessionId),
});

const toolboxMenuProps = () => ({ "data-session-toolbox-owner": props.sessionId });

const targetSession = computed(() => findSession(props.sessionId));
const targetTab = computed(() => sessionTabs.value.find((tab) => tab.id === props.sessionId) ?? null);
const diagnosticView = computed(() => {
  const session = targetSession.value;
  const tab = targetTab.value;
  if (!session || !tab) return null;
  const viewport = getRemoteViewportSummary(tab);
  const summary = activeRemoteSession.value?.id === session.id
    ? sessionDiagnosticsSummary.value
    : summarizeSessionDiagnosticsPanel({ viewport, events: eventLog.value });
  return buildSessionDiagnosticView({ session, viewport, summary, events: eventLog.value, nowMs: nowTick.value, t });
});
const visibleDiagnosticView = computed(() => externalWindowLocation ? externalDiagnosticView.value : diagnosticView.value);

const localToolboxContext = computed<SessionToolboxContext>(() => {
  const session = targetSession.value;
  const summary = targetTab.value ? getRemoteViewportSummary(targetTab.value) : null;
  sessionListTick.value;
  const connection = sessionConnectionRegistry.get(props.sessionId)?.connection;
  return {
    sessionId: props.sessionId,
    state: session?.state ?? null,
    readonly: summary?.inputMode === "readonly",
    viewOnlyShared: summary?.inputPolicy === "viewOnlyShared",
    capabilities: connection?.connectionModel.capabilities ?? null,
    active: Boolean(targetTab.value?.active),
    fullscreen: Boolean(fullscreenSessionId.value === props.sessionId),
    externalWindow: Boolean(externalWindowLocation?.sessionId === props.sessionId),
    localServer: Boolean(getSessionLocalServerView(props.sessionId)),
    displayChoices: createSessionDisplayChoices(
      getSessionLocalServerView(props.sessionId),
      getSessionRemoteDisplayInventory(props.sessionId),
      getSessionSelectedRemoteDisplayId(props.sessionId),
    ),
    recording: Boolean(isSessionRecording(props.sessionId)),
    recordingPhase: getSessionRecordingPhase(props.sessionId),
    recordingElapsedSeconds: getSessionRecordingElapsedSeconds(props.sessionId),
    recordingSpace: getSessionRecordingSpace(props.sessionId),
  };
});

const toolboxContext = computed<SessionToolboxContext>(() => ({
  ...localToolboxContext.value,
  ...(props.contextOverride ?? {}),
  sessionId: props.contextOverride?.sessionId ?? props.sessionId,
}));
watch(() => toolboxContext.value.recordingPhase, (phase) => {
  if (phase !== "recording") resetRecordingStopRequest();
});

const presentation = computed(() => resolveSessionToolboxPresentation(toolboxContext.value));

const actions = computed(() => resolveSessionToolboxActions(toolboxContext.value));

const actionsById = computed(() => new Map(actions.value.map((action) => [action.id, action])));

const toolboxMenuOptions = computed<DropdownOption[]>(() => {
  const groupOptions = listSessionToolboxGroups()
    .map((group) => createGroupOption(group.id, group.labelKey))
    .filter((option): option is DropdownOption => Boolean(option));
  const fullscreenExit = fullscreenSessionId.value === props.sessionId ? actionOption("exit-fullscreen") : null;

  return fullscreenExit
    ? [...groupOptions, dividerOption("toolbox-exit"), fullscreenExit].filter(isDropdownOption)
    : groupOptions;
});

function createGroupOption(groupId: SessionToolboxGroupId, labelKey: string): DropdownOption | null {
  if (groupId === "fileTransfer" && !presentation.value.fileTransfer) {
    return null;
  }

  const children = groupMenuOptions(groupId);
  if (!children.length) {
    return null;
  }

  return {
    key: `group:${groupId}`,
    label: t(labelKey),
    children,
  };
}

function groupMenuOptions(groupId: SessionToolboxGroupId): DropdownOption[] {
  switch (groupId) {
    case "viewWindow":
      return [
        actionOption("toggle-fullscreen"),
        actionOption("toggle-external-window"),
        submenuOption("display-scale", t("sessions.displayScale"), displayScaleMenuOptions()),
        displayMenuOptions().length > 1
          ? submenuOption("display", t("sessions.displaySelect"), displayMenuOptions())
          : null,
      ].filter(isDropdownOption);
    case "inputControl":
      return [
        actionOption("toggle-readonly"),
        ...(windowsClient ? [
          { key: "keyboard:rightControlAsCommand", label: `${keyboardPreferences.value.rightControlAsCommand ? "✓ " : ""}${t("settings.keyboardTransmission.rightControlAsCommand")}` },
        ] : []),
        submenuOption("shortcuts", t("sessions.toolbox.sendShortcuts"), [
          submenuOption("shortcuts-windows", t("sessions.toolbox.shortcutGroups.windows"), shortcutActionsById(WINDOWS_SHORTCUT_ACTION_IDS, "windows")),
          submenuOption("shortcuts-macos", t("sessions.toolbox.shortcutGroups.macos"), shortcutActionsById(MACOS_SHORTCUT_ACTION_IDS, "macos")),
        ]),
        commandOption({ kind: "open-text-dialog" }, t("sessions.toolbox.sendMessage"), false,
          actionsById.value.get("send-message")?.status !== "enabled", actionsById.value.get("send-message")?.shortcutLabel,
          actionsById.value.get("send-message")?.disabledReasonKey),
      ].filter(isDropdownOption);
    case "fileTransfer":
      return groupActions(groupId).map((action) => actionMenuOption(action));
    case "more":
      return [
        actionOption("capture-screenshot"),
        actionOption("toggle-recording"),
        actionOption("wake-display"),
        submenuOption("view-preferences", t("sessions.toolbox.viewPreferences"), [actionOption("toggle-hud")].filter(isDropdownOption)),
        actionOption("open-diagnostics"),
      ].filter(isDropdownOption);
  }
}

function groupActions(groupId: SessionToolboxGroupId): SessionToolboxAction[] {
  return actions.value.filter((action) => action.group === groupId);
}

function shortcutActionsById(actionIds: SessionToolboxActionId[], shortcutDisplay: ShortcutDisplay): DropdownOption[] {
  return actionIds
    .map((actionId) => actionsById.value.get(actionId))
    .filter((action): action is SessionToolboxAction => Boolean(action))
    .map((action) => actionMenuOption(action, shortcutDisplay));
}

function actionOption(actionId: SessionToolboxActionId): DropdownOption | null {
  const action = actionsById.value.get(actionId);
  return action ? actionMenuOption(action) : null;
}

function actionMenuOption(action: SessionToolboxAction, shortcutDisplay?: ShortcutDisplay): DropdownOption {
  const labelKey = actionLabelKey(action);
  return {
    key: commandKey({ kind: "action", actionId: action.id }),
    label: menuOptionLabel(t(labelKey), action.shortcutLabel, shortcutDisplay),
    disabled: actionDisabled(action),
  };
}

function actionLabelKey(action: SessionToolboxAction): string {
  switch (action.id) {
    case "toggle-fullscreen":
      return toolboxContext.value.fullscreen
        ? "sessions.toolbox.actions.toggle-fullscreen.exitLabel"
        : "sessions.toolbox.actions.toggle-fullscreen.enterLabel";
    case "toggle-external-window":
      return toolboxContext.value.externalWindow
        ? "sessions.toolbox.actions.toggle-external-window.exitLabel"
        : "sessions.toolbox.actions.toggle-external-window.enterLabel";
    case "toggle-recording":
      if (toolboxContext.value.recordingPhase === "failed") return "sessions.artifacts.recordingRetrySave";
      if (toolboxContext.value.recordingPhase === "starting") return "sessions.artifacts.recordingCancelStart";
      return toolboxContext.value.recording ? "sessions.toolbox.actions.toggle-recording.stopLabel" : "sessions.toolbox.actions.toggle-recording.startLabel";
    case "toggle-hud":
      return props.isHudHidden(props.sessionId)
        ? "sessions.toolbox.actions.toggle-hud.showLabel"
        : "sessions.toolbox.actions.toggle-hud.hideLabel";
    case "toggle-readonly":
      return toolboxContext.value.readonly
        ? "sessions.toolbox.actions.toggle-readonly.controlLabel"
        : "sessions.toolbox.actions.toggle-readonly.readonlyLabel";
    default:
      return action.labelKey;
  }
}

function displayScaleMenuOptions(): DropdownOption[] {
  return [
    displayScaleCommandOption("fit"),
    displayScaleCommandOption("actual"),
    displayScaleCommandOption("scale125"),
    displayScaleCommandOption("scale150"),
    displayScaleCommandOption("scale200"),
  ];
}

function displayScaleCommandOption(value: SessionViewportScaleMode): DropdownOption {
  return commandOption(
    { kind: "display-scale", value },
    t(`sessions.displayScaleModes.${value}`),
    getSessionViewportScaleMode(props.sessionId) === value,
  );
}

function displayMenuOptions(): DropdownOption[] {
  return (toolboxContext.value.displayChoices ?? []).map((display) => {
    const sizeText = display.remote && display.width > 0 && display.height > 0
      ? ` · ${display.width}x${display.height}` : "";
    const primaryText = display.remote && display.isPrimary ? ` · ${t("sessions.toolbox.primaryDisplay")}` : "";
    return commandOption(
      { kind: "display", value: display.value },
      `${display.label}${sizeText}${primaryText}`,
      display.selected,
      toolboxContext.value.state !== "connected"
        || (display.remote && !sessionSupportsCapability(toolboxContext.value.capabilities, "agent.display.select")),
    );
  });
}

function commandOption(
  command: MenuCommand,
  label: string,
  checked = false,
  disabled = false,
  shortcutLabel = "",
  disabledReasonKey?: string,
): DropdownOption {
  return {
    key: commandKey(command),
    label: disabled && disabledReasonKey
      ? () => h("span", { title: t(disabledReasonKey) }, [label,
        h("small", { class: "session-toolbox-disabled-hint" }, t(disabledReasonKey))])
      : menuOptionLabel(checked ? `✓ ${label}` : label, shortcutLabel),
    disabled,
  };
}

function menuOptionLabel(label: string, shortcutLabel?: string, shortcutDisplay?: ShortcutDisplay) {
  if (!shortcutLabel) {
    return label;
  }

  return () => h("span", { class: "session-toolbox-option-label" }, [
    h("span", { class: "session-toolbox-option-main" }, label),
    h("span", { class: "session-toolbox-option-shortcut" }, formatShortcutLabel(shortcutLabel, shortcutDisplay)),
  ]);
}

function formatShortcutLabel(shortcutLabel: string, shortcutDisplay?: ShortcutDisplay): string {
  if (shortcutDisplay === "windows") {
    return formatKeyboardShortcutForPlatform(shortcutLabel, "windows");
  }
  if (shortcutDisplay === "macos") {
    return formatKeyboardShortcutSymbolsForPlatform(shortcutLabel, "macos");
  }
  return formatKeyboardShortcutSymbolsForPlatform(shortcutLabel);
}

function submenuOption(id: string, label: string, children: DropdownOption[]): DropdownOption {
  return {
    key: `submenu:${id}`,
    label,
    children,
  };
}

function dividerOption(id: string): DropdownOption {
  return { key: `divider:${id}`, type: "divider" };
}

function actionDisabled(action: SessionToolboxAction): boolean {
  if (action.id === "toggle-recording" && toolboxContext.value.recordingPhase === "stopping") return true;
  if (action.id === "toggle-fullscreen" && toolboxContext.value.externalWindow && !props.toggleExternalWindowFullscreen) {
    return true;
  }
  if (action.id === "send-file" && toolboxContext.value.fileTransferAvailable
    && presentation.value.connected && !toolboxContext.value.readonly && !toolboxContext.value.viewOnlyShared) {
    return false;
  }
  return action.status === "disabled";
}

function handleToolboxClickOutside(event: Event) {
  if (isToolboxTriggerEvent(event) || isSessionToolboxEvent(event, props.sessionId)) {
    return;
  }

  closeToolbox();
}

function shouldProxyCommandToMainWindow(command: MenuCommand): boolean {
  if (externalWindowLocation?.sessionId !== props.sessionId) {
    return false;
  }
  if (EXTERNAL_WINDOW_LOCAL_COMMAND_KINDS.has(command.kind) || command.kind === "open-text-dialog") {
    return false;
  }
  if (command.kind === "action" && (
    command.actionId === "copy-target" ||
    command.actionId === "toggle-hud" ||
    command.actionId === "toggle-fullscreen" ||
    command.actionId === "toggle-external-window" ||
    command.actionId === "exit-fullscreen" ||
    command.actionId === "send-file" ||
    command.actionId === "open-file-transfer-center"
  )) {
    return false;
  }
  return true;
}

async function emitExternalToolboxCommand(
  key: string,
  extra: Partial<SessionExternalWindowToolboxCommandPayload> = {},
) {
  await emit(sessionExternalWindowToolboxCommandEvent(props.sessionId), {
    sessionId: props.sessionId,
    key,
    text: textDraft.value,
    feedbackTarget: "external",
    ...extra,
  } satisfies SessionExternalWindowToolboxCommandPayload).catch((error) => {
    showToolboxFeedback(t("sessions.toolbox.feedback.failed", { error: String(error) }), "error");
  });
}

function createToolboxFeedback() {
  const sessionId = props.sessionId;
  const external = externalToolboxFeedbackTarget === "external" || externalWindowLocation?.sessionId === sessionId;
  return (message: string, tone: "info" | "success" | "warning" | "error" = "info") => {
    if (external) {
      void emit(sessionExternalWindowToolboxCommandResultEvent(sessionId), { sessionId, tone, message });
    } else showAppMessage(message, tone);
  };
}

function showToolboxFeedback(message: string, tone: "info" | "success" | "warning" | "error") {
  createToolboxFeedback()(message, tone);
}

function consumeExternalFileTransferSourcePaths(): string[] | null {
  const paths = externalFileTransferSourcePaths.value;
  externalFileTransferSourcePaths.value = null;
  return paths;
}

async function handleMenuSelect(key: string) {
  if (key === "keyboard:rightControlAsCommand") {
    if (!setKeyboardPreference("rightControlAsCommand", !keyboardPreferences.value.rightControlAsCommand)) {
      showToolboxFeedback(t("settings.keyboardTransmission.saveFailed"), "error");
    }
    closeToolbox();
    return;
  }
  const command = parseCommandKey(key);
  if (!command || command.kind === "clipboard-text") {
    return;
  }

  activateSessionTab(props.sessionId);
  closeToolbox();

  if (command.kind === "action") {
    const action = actionsById.value.get(command.actionId);
    if (!action || actionDisabled(action)) {
      showToolboxFeedback(action?.disabledReasonKey ? t(action.disabledReasonKey) : t("sessions.toolbox.disabled.unavailable"), "warning");
      return;
    }
  }

  if (shouldProxyCommandToMainWindow(command)) {
    await emitExternalToolboxCommand(key);
    return;
  }

  switch (command.kind) {
    case "action": {
      const action = actionsById.value.get(command.actionId);
      if (action) {
        await executeAction(action);
      }
      return;
    }
    case "display-scale":
      setSessionViewportScaleMode(props.sessionId, command.value);
      sessionConnectionRegistry.get(props.sessionId)?.connection.setViewportScaleMode?.(command.value);
      return;
    case "display":
      {
        const remoteDisplayId = remoteDisplayIdFromCommandValue(command.value);
        if (remoteDisplayId) {
          await selectRemoteAgentDisplay(props.sessionId, remoteDisplayId);
          return;
        }
      }
      await updateSessionLocalServerView(props.sessionId, { displayId: command.value });
      return;
    case "open-text-dialog":
      openMessageDialog();
      return;
  }
}

async function stopRecordingFromIndicator() {
  if (toolboxContext.value.recordingPhase !== "recording" || recordingStopRequested.value) return;
  recordingStopRequested.value = true;
  try {
    await handleMenuSelect("action:toggle-recording");
  } finally {
    // An external window can lose its command before the main window changes phase.
    if (toolboxContext.value.recordingPhase === "recording") {
      recordingStopResetTimer = window.setTimeout(resetRecordingStopRequest, 3000);
    }
  }
}

async function selectRemoteAgentDisplay(sessionId: string, displayId: string) {
  const connection = sessionConnectionRegistry.get(sessionId)?.connection;
  if (!connection) {
    throw new Error(t("sessions.toolbox.disabled.disconnected"));
  }
  if (!connection.selectRemoteDisplay) {
    throw new Error(t("sessions.toolbox.disabled.unavailable"));
  }
  await connection.selectRemoteDisplay(displayId);
  setSessionSelectedRemoteDisplayId(sessionId, displayId);
}

async function executeAction(action: SessionToolboxAction) {
  if (actionDisabled(action)) {
    showToolboxFeedback(action.disabledReasonKey ? t(action.disabledReasonKey) : t("sessions.toolbox.disabled.unavailable"), "warning");
    return;
  }

  try {
    const completed = await dispatchAction(action, props.sessionId);
    if (completed && action.successKey) {
      showToolboxFeedback(t(action.successKey), "success");
    }
  } catch (error) {
    showToolboxFeedback(t("sessions.toolbox.feedback.failed", { error: String(error) }), "error");
  }
}

async function dispatchAction(action: SessionToolboxAction, sessionId: string, messageText = textDraft.value): Promise<boolean> {
  const notify = createToolboxFeedback();
  const feedbackTarget = externalToolboxFeedbackTarget;
  if (action.kind === "shortcut") {
    if (!action.dispatches?.length) {
      return false;
    }
    const connection = sessionConnectionRegistry.get(sessionId)?.connection;
    if (!connection) throw new Error(t("sessions.toolbox.disabled.disconnected"));
    if (action.id === "shortcut-windows-secure-attention") {
      const result = connection.sendSecureAttention ? await connection.sendSecureAttention()
        : (await connection.sendRemoteKeySequence(action.dispatches), "unconfirmed");
      if (result !== "submitted" && result !== "unconfirmed") {
        showToolboxFeedback(t(`sessions.secureAttention.${result}`), "warning");
      }
      recordMainViewportInput(sessionId, "keyboard");
      return false;
    }
    const desktop = connection.getServerRuntimeState?.()?.desktop;
    if (desktop && desktop !== "available") {
      return false;
    }
    await connection.sendRemoteKeySequence(action.dispatches);
    recordMainViewportInput(sessionId, "keyboard");
    return true;
  }

  if (action.kind === "remoteMessage") {
    const invalid = remoteMessageTextError(messageText);
    if (invalid) throw new Error(invalid);
    const request = createSessionToolboxActionRequest({
      context: toolboxContext.value,
      actionId: action.id,
      text: messageText,
    });
    if (!request || request.kind !== "remoteMessage") {
      throw new Error("remote_message_disconnected");
    }
    const connection = sessionConnectionRegistry.get(sessionId)?.connection;
    if (!connection) throw new Error("remote_message_disconnected");
    await connection.sendRemoteMessage(request.text);
    return true;
  }

  if (action.kind === "wakeDisplay") {
    const connection = sessionConnectionRegistry.get(sessionId)?.connection;
    if (!connection) throw new Error(t("sessions.toolbox.disabled.disconnected"));
    if (!connection.wakeRemoteDisplay) throw new Error(t("sessions.toolbox.disabled.unavailable"));
    const result = await connection.wakeRemoteDisplay();
    if (result === "sent-unconfirmed") {
      recordMainViewportInput(sessionId, "pointer");
      notify(t("sessions.toolbox.feedback.wakeDisplaySent"));
      return false;
    }
    return true;
  }

  if (action.kind === "fileTransfer") {
    return await dispatchFileTransferAction({
      commandKey,
      consumeExternalFileTransferSourcePaths,
      currentFileTransferReceiveDirectory,
      emitExternalToolboxCommand,
      externalToolboxFeedbackTarget: feedbackTarget,
      externalWindowSessionId: externalWindowLocation?.sessionId,
      enqueueSessionFileUploads,
      invoke,
      openTransferCenter: openGlobalTransferDrawer,
      isTransferCenterVisible: () => feedbackTarget === "external"
        ? externalTransferDrawerOpen.value
        : globalTransferDrawerOpen.value && (globalTransferDrawerSessionId.value === null || globalTransferDrawerSessionId.value === sessionId),
      recordFailure: (error) => logEvent("warning", t("log.sourceSession"), `Agent file transfer directory copy failed; error=${String(error)}`),
      sessionId,
      showMessage: showAppMessage,
      t,
    });
  }

  switch (action.id) {
    case "toggle-fullscreen":
      if (toolboxContext.value.externalWindow && props.toggleExternalWindowFullscreen) {
        await props.toggleExternalWindowFullscreen(sessionId);
        return true;
      }
      if (fullscreenSessionId.value === sessionId) {
        await exitSessionFullscreen();
      } else {
        await toggleSessionFullscreen(sessionId);
      }
      return true;
    case "exit-fullscreen":
      if (toolboxContext.value.externalWindow && toolboxContext.value.fullscreen && props.toggleExternalWindowFullscreen) {
        await props.toggleExternalWindowFullscreen(sessionId);
        return true;
      }
      await exitSessionFullscreen();
      return true;
    case "toggle-external-window":
      if (externalWindowLocation?.sessionId === sessionId) {
        await toggleExternalWindowSession(sessionId);
      } else {
        await openExternalWindowSession(sessionId);
      }
      return true;
    case "toggle-hud":
      props.toggleHud(sessionId);
      return true;
    case "open-file-transfer-center":
      if (externalWindowLocation?.sessionId === sessionId) {
        void emit(sessionExternalTransferDrawerRequestEvent(sessionId), { sessionId, open: true });
      } else {
        openGlobalTransferDrawer(sessionId);
      }
      return true;
    case "toggle-readonly":
      if (toolboxContext.value.viewOnlyShared) {
        showToolboxFeedback(t("sessions.toolbox.disabled.viewOnlyShared"), "warning");
        return false;
      }
      setSessionReadonlyMode(sessionId, !toolboxContext.value.readonly);
      return true;
    case "retry-session":
      await retrySessionFromViewport(sessionId);
      return true;
    case "capture-screenshot":
      await captureSessionScreenshot(sessionId, notify);
      return true;
    case "toggle-recording":
      await toggleSessionRecording(sessionId, notify);
      return true;
    case "copy-target": {
      const target = externalWindowLocation?.target ?? targetSession.value?.target ?? targetTab.value?.target;
      if (!target) return false;
      await navigator.clipboard.writeText(target);
      return true;
    }
    case "open-diagnostics":
      if (externalToolboxFeedbackTarget === "external") {
        if (!diagnosticView.value) return false;
        await emit(sessionExternalWindowToolboxCommandResultEvent(sessionId), {
          sessionId, tone: "info", message: "", diagnosticView: diagnosticView.value,
        } satisfies SessionExternalWindowToolboxCommandResultPayload);
      } else {
        diagnosticDialogOpen.value = true;
      }
      return false;
    case "copy-diagnostic-summary": {
      const view = visibleDiagnosticView.value ?? diagnosticView.value;
      if (!view) return false;
      await navigator.clipboard.writeText(view.summaryReport);
      notify(t("log.diagnosticCopied"), "success");
      return true;
    }
  }
  return false;
}

async function copyVisibleTechnicalReport() {
  const view = visibleDiagnosticView.value;
  if (!view) return;
  try {
    await navigator.clipboard.writeText(view.technicalReport);
    showToolboxFeedback("技术报告已复制。", "success");
  } catch (error) {
    showToolboxFeedback(`复制技术报告失败：${String(error)}`, "error");
  }
}

async function openLogDirectory() {
  try {
    await invoke("open_log_directory");
  } catch (error) {
    showToolboxFeedback(`打开日志文件夹失败：${String(error)}`, "error");
  }
}

function currentFileTransferReceiveDirectory(): string {
  return resolveCurrentFileTransferReceiveDirectory({
    session: targetSession.value,
    sessionId: props.sessionId,
    sessionFileTransferManager,
  });
}

function openMessageDialog() {
  const action = actionsById.value.get("send-message");
  if (!action || actionDisabled(action)) {
    showToolboxFeedback(t(action?.disabledReasonKey ?? "sessions.toolbox.disabled.unavailable"), "warning");
    return;
  }
  closeToolbox();
  textDialogOpen.value = true;
}

async function handleMessageDraftKeydown(event: KeyboardEvent) {
  // This input stops propagation to keep keystrokes local, so handle Escape
  // here as well as the modal's default handler outside the input.
  if (event.key === "Escape" && !event.isComposing && event.keyCode !== 229) {
    event.preventDefault();
    event.stopPropagation();
    if (!textSending.value) textDialogOpen.value = false;
    return;
  }
  const action = messageDraftEditShortcut(event);
  const input = event.target;
  if (!action || !(input instanceof HTMLTextAreaElement) || textSending.value) return;
  event.preventDefault();
  event.stopPropagation();
  try {
    await editMessageDraftClipboard(action, {
      get value() { return input.value; },
      get selectionStart() { return input.selectionStart; },
      get selectionEnd() { return input.selectionEnd; },
      replaceSelection(text) {
        // Native insertion keeps the textarea's undo history where supported.
        let inserted = false;
        try { inserted = input.ownerDocument.execCommand?.("insertText", false, text) ?? false; } catch { /* Use the standard selection fallback. */ }
        if (inserted) return;
        input.setRangeText(text, input.selectionStart, input.selectionEnd, "end");
        input.dispatchEvent(new Event("input", { bubbles: true }));
      },
      selectAll() { input.select(); },
    }, createSystemClipboard(), () => textDialogOpen.value && !textSending.value
      && input.isConnected && input.ownerDocument.activeElement === input);
  } catch {
    showToolboxFeedback(t("sessions.toolbox.messageClipboardFailed"), "error");
  }
}

watch(sessionMessageDialogRequest, (request) => {
  if (request?.sessionId === props.sessionId) openMessageDialog();
});

function remoteMessageFailureText(error: unknown): string {
  const code = error instanceof Error ? error.message : String(error);
  const keys: Record<string, string> = {
    remote_message_empty: "emptyText",
    remote_message_invalid: "messageInvalid",
    remote_message_too_large: "messageTooLarge",
    remote_message_busy: "messageBusy",
    remote_message_display_failed: "messageDisplayFailed",
    agent_user_desktop_unavailable: "messageDesktopUnavailable",
    remote_message_unsupported: "messageUnsupported",
    agent_unsupported_request: "messageUnsupported",
    remote_message_disconnected: "disconnected",
    agent_view_only: "readonly",
  };
  return t(`sessions.toolbox.disabled.${keys[code] ?? "messageFailed"}`);
}

async function sendTextDraft() {
  if (textSending.value) return;
  const action = actionsById.value.get("send-message");
  if (!action || actionDisabled(action) || remoteMessageTextError(textDraft.value)) return;
  const sessionId = props.sessionId;
  const text = textDraft.value;
  const controller = new AbortController();
  messageSendController = controller;
  textSending.value = true;
  try {
    if (shouldProxyCommandToMainWindow({ kind: "action", actionId: action.id })) {
      const succeeded = await sendExternalRemoteMessage({
        sessionId, text, signal: controller.signal,
        listen: (handler) => listen<SessionExternalWindowToolboxCommandResultPayload>(
          sessionExternalWindowToolboxCommandResultEvent(sessionId), (event) => handler(event.payload)),
        send: (payload) => emit(sessionExternalWindowToolboxCommandEvent(sessionId), payload),
      });
      if (!succeeded) return;
    } else {
      const completed = await dispatchAction(action, sessionId, text);
      if (disposed || sessionId !== props.sessionId) return;
      if (completed && action.successKey) showToolboxFeedback(t(action.successKey), "success");
    }
    if (disposed || sessionId !== props.sessionId) return;
    textDialogOpen.value = false;
    textDraft.value = "";
  } catch (error) {
    if (!disposed && sessionId === props.sessionId) showToolboxFeedback(remoteMessageFailureText(error), "error");
  } finally {
    if (messageSendController === controller) messageSendController = null;
    textSending.value = false;
  }
}

watch(() => props.sessionId, () => {
  closeToolbox();
  messageSendController?.abort();
  textDialogOpen.value = false;
  textDraft.value = "";
});
watch(() => toolboxContext.value.active, closeToolbox);
watch(textDialogOpen, (open) => { if (!open) textDraft.value = ""; });
watch(() => presentation.value.connected, (connected) => {
  if (!connected) {
    closeToolbox();
    textDialogOpen.value = false;
  }
});

onMounted(() => {
  if (externalWindowLocation) {
    void listen<SessionExternalWindowToolboxCommandResultPayload>(sessionExternalWindowToolboxCommandResultEvent(props.sessionId), (event) => {
      if (disposed || event.payload.sessionId !== props.sessionId || !event.payload.diagnosticView) return;
      externalDiagnosticView.value = event.payload.diagnosticView;
      diagnosticDialogOpen.value = true;
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenExternalDiagnosticView = unlisten;
    }).catch(() => undefined);
    return;
  }
  void listen<{ sessionId: string; open: boolean }>(sessionExternalTransferDrawerRequestEvent(props.sessionId), (event) => {
    if (disposed || event.payload.sessionId !== props.sessionId) return;
    externalTransferDrawerOpen.value = event.payload.open;
    publishExternalTransferTasks();
  }).then((unlisten) => { if (disposed) unlisten(); else unlistenExternalTransferDrawerRequest = unlisten; }).catch(() => undefined);
  void listen<SessionExternalTransferDrawerControl>(sessionExternalTransferDrawerControlEvent(props.sessionId), (event) => {
    if (disposed || event.payload.sessionId !== props.sessionId) return;
    switch (event.payload.action) {
      case "pause-all": sessionFileTransferManager.pauseAll(props.sessionId, event.payload.taskIds); break;
      case "resume-all": sessionFileTransferManager.resumeAll(props.sessionId, event.payload.taskIds); break;
      case "pause": sessionFileTransferManager.pauseTask(event.payload.taskId ?? ""); break;
      case "resume": sessionFileTransferManager.resumeTask(event.payload.taskId ?? ""); break;
      case "retry": sessionFileTransferManager.retryTask(event.payload.taskId ?? ""); break;
      case "cancel": sessionFileTransferManager.cancelTask(event.payload.taskId ?? ""); break;
      case "clear-history":
        void sessionFileTransferManager.clearHistory(props.sessionId, event.payload.taskIds).catch(() => {
          void emit(sessionExternalWindowToolboxCommandResultEvent(props.sessionId), {
            sessionId: props.sessionId, message: t("sessions.fileTransfers.clearHistoryFailed"), tone: "error",
          });
        });
        break;
      case "copy-path":
      case "open-directory":
        void runTaskPathAction(props.sessionId, event.payload.taskId ?? "", event.payload.action, (message, tone) => {
          void emit(sessionExternalWindowToolboxCommandResultEvent(props.sessionId), { sessionId: props.sessionId, message, tone });
        });
        break;
    }
    publishExternalTransferTasks();
  }).then((unlisten) => { if (disposed) unlisten(); else unlistenExternalTransferDrawerControl = unlisten; }).catch(() => undefined);
  watch([sessionFileTransferManager.tasks, sessionFileTransferManager.clearingHistory], publishExternalTransferTasks);
  void listen<SessionExternalWindowToolboxCommandPayload>(
    sessionExternalWindowToolboxCommandEvent(props.sessionId),
    (event) => {
      if (disposed || event.payload.sessionId !== props.sessionId) {
        return;
      }
      if (event.payload.key === "action:send-message" && event.payload.requestId) {
        const { requestId, text, sessionId } = event.payload;
        void (async () => {
          let succeeded = false;
          let message: string;
          try {
            const action = actionsById.value.get("send-message");
            if (!action || actionDisabled(action)) {
              message = t(action?.disabledReasonKey ?? "sessions.toolbox.disabled.unavailable");
            } else {
              await dispatchAction(action, sessionId, text);
              succeeded = true;
              message = "";
            }
          } catch (error) {
            message = remoteMessageFailureText(error);
          }
          await emit(sessionExternalWindowToolboxCommandResultEvent(sessionId), {
            sessionId, requestId, succeeded, message, tone: succeeded ? "success" : "error",
          } satisfies SessionExternalWindowToolboxCommandResultPayload);
        })().catch(() => undefined);
        return;
      }
      textDraft.value = event.payload.text;
      externalFileTransferSourcePaths.value = event.payload.selectedFileTransferSourcePaths ?? null;
      externalToolboxFeedbackTarget = event.payload.feedbackTarget ?? null;
      void handleMenuSelect(event.payload.key).finally(() => {
        textDraft.value = "";
        externalFileTransferSourcePaths.value = null;
        externalToolboxFeedbackTarget = null;
      });
    },
  ).then((unlisten) => {
    if (disposed) unlisten();
    else unlistenExternalToolboxCommand = unlisten;
  }).catch(() => undefined);
});

onBeforeUnmount(() => {
  messageSendController?.abort();
  disposed = true;
  if (recordingStopResetTimer !== null) window.clearTimeout(recordingStopResetTimer);
  unlistenExternalDiagnosticView?.();
  unlistenExternalToolboxCommand?.();
  unlistenExternalToolboxCommand = null;
  unlistenExternalTransferDrawerRequest?.();
  unlistenExternalTransferDrawerControl?.();
});

</script>

<template>
  <div class="session-toolbox-layer">
    <div v-if="presentation.recoverable" class="session-recovery-actions">
      <SessionViewportPlaceholder :title="t('sessions.toolbox.connectionUnavailable')" :description="t('sessions.toolbox.connectionUnavailableHint')">
        <template #icon><UnlinkOutline /></template>
        <NButton type="primary" @click.stop="handleMenuSelect('action:retry-session')">{{ t('sessions.retry') }}</NButton>
        <NButton secondary @click.stop="handleMenuSelect('action:copy-target')">{{ t('diagnostics.copyTarget') }}</NButton>
        <NButton secondary @click.stop="handleMenuSelect('action:open-diagnostics')">{{ t('sessions.toolbox.viewDiagnostics') }}</NButton>
        <NButton v-if="toolboxContext.fullscreen" secondary @click.stop="handleMenuSelect('action:exit-fullscreen')">{{ t('sessions.toolbox.actions.toggle-fullscreen.exitLabel') }}</NButton>
        <NButton v-if="toolboxContext.externalWindow" secondary @click.stop="handleMenuSelect('action:toggle-external-window')">{{ t('sessions.externalSurface.returnToMain') }}</NButton>
      </SessionViewportPlaceholder>
    </div>
    <div
      v-if="presentation.connected"
      class="session-toolbox-trigger"
      :data-session-toolbox-owner="sessionId"
      :style="buttonStyle"
      @pointerdown.stop="startToolboxDrag"
      @click.stop="handleToolboxButtonClick"
    >
      <NDropdown
        trigger="manual"
        placement="bottom-end"
        :show="toolboxOpen"
        :options="toolboxMenuOptions"
        :menu-props="toolboxMenuProps"
        class="session-toolbox-menu"
        @clickoutside="handleToolboxClickOutside"
        @select="handleMenuSelect"
      >
        <NButton
          type="primary"
          class="session-toolbox-button"
          :title="`${t('sessions.toolbox.title')} · ${targetSession?.target ?? t('sessions.toolbox.noSession')}`"
        >
          <NIcon class="session-toolbox-button-icon"><OptionsOutline /></NIcon>
        </NButton>
      </NDropdown>
      <SessionRecordingIndicator v-if="toolboxContext.recordingPhase === 'recording'"
        :elapsed-seconds="toolboxContext.recordingElapsedSeconds ?? 0" :space="toolboxContext.recordingSpace"
        :stopping="recordingStopRequested" @stop="stopRecordingFromIndicator" />
    </div>
  </div>
  <NModal v-model:show="textDialogOpen" preset="card" :title="t('sessions.toolbox.sendMessage')" :closable="!textSending" :mask-closable="!textSending" :close-on-esc="!textSending" style="width: min(480px, calc(100vw - 32px))">
    <div data-native-editing @keydown.stop="handleMessageDraftKeydown" @keyup.stop>
      <NInput v-model:value="textDraft" type="textarea" :autosize="{ minRows: 4, maxRows: 10 }" :placeholder="t('sessions.toolbox.messagePlaceholder')" :disabled="textSending" />
    </div>
    <p>{{ t('sessions.toolbox.messageHint') }}</p>
    <p v-if="textDraft && remoteMessageTextError(textDraft)" role="alert">{{ remoteMessageFailureText(remoteMessageTextError(textDraft)) }}</p>
    <template #footer>
      <NButton type="primary" :loading="textSending" :disabled="Boolean(remoteMessageTextError(textDraft)) || actionsById.get('send-message')?.status !== 'enabled'" @click="sendTextDraft">{{ t('sessions.toolbox.actions.send-message.label') }}</NButton>
    </template>
  </NModal>
  <NModal v-model:show="diagnosticDialogOpen" preset="card" :title="t('sessions.toolbox.viewDiagnostics')" style="width: min(860px, calc(100vw - 32px))">
    <SessionDiagnosticContent v-if="diagnosticDialogOpen && visibleDiagnosticView" :view="visibleDiagnosticView" />
    <p v-else>当前会话的诊断信息暂不可用。</p>
    <template #footer>
      <div class="session-diagnostic-copy-actions">
        <div class="session-diagnostic-copy-group">
          <NButton secondary :disabled="!visibleDiagnosticView" @click="handleMenuSelect('action:copy-diagnostic-summary')">{{ t('sessions.toolbox.actions.copy-diagnostic-summary.label') }}</NButton>
          <NButton secondary :disabled="!visibleDiagnosticView" @click="copyVisibleTechnicalReport">复制技术报告</NButton>
        </div>
        <NButton tertiary @click="openLogDirectory"><template #icon><NIcon><FolderOpenOutline /></NIcon></template>打开日志文件夹</NButton>
      </div>
    </template>
  </NModal>
</template>

<style scoped>
.session-diagnostic-copy-actions { display: flex; justify-content: space-between; align-items: center; gap: 10px; flex-wrap: wrap; width: 100%; }
.session-diagnostic-copy-group { display: flex; gap: 8px; flex-wrap: wrap; }
</style>
