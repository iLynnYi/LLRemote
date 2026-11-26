<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isSessionWindowExpanded, setSessionWindowExpanded } from "../../../session/external-window/sessionWindowPresentation.ts";
import { shouldCleanupConnectionForSessionState } from "../../../session/core/sessionConnectionCleanup.ts";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import { useSessionExternalSurface } from "../composables/useSessionExternalSurface.ts";
import { useSessionExternalWindowLifetime } from "../composables/useSessionExternalWindowLifetime.ts";
import ConfirmDialog from "../../../app/components/ConfirmDialog.vue";
import PasswordPromptDialog from "../../../app/components/PasswordPromptDialog.vue";
import {
  createExternalKeyboardInputPayload,
  createExternalPointerInputPayload,
  createExternalWheelInputPayload,
  emitExternalInput,
} from "../composables/useSessionExternalInputBridge.ts";
import SessionViewportChrome from "./SessionViewportChrome.vue";
import {
  createExternalDesktopRecovery,
  sessionExternalWindowStateEvent,
  sessionExternalWindowStateRequestEvent,
  type SessionExternalWindowStatePayload,
} from "../../../session/external-window/sessionExternalWindowState.ts";
import {
  sessionExternalWindowFeedbackEvent,
  type SessionExternalWindowFeedbackPayload,
  type SessionExternalInputPayload,
} from "../../../session/external-window/sessionExternalInput.ts";
import {
  sessionExternalWindowToolboxCommandEvent,
  sessionExternalWindowFocusEvent,
  sessionExternalWindowToolboxCommandResultEvent,
  sessionExternalTransferDrawerControlEvent,
  sessionExternalTransferDrawerRequestEvent,
  sessionExternalTransferDrawerSnapshotEvent,
  type SessionExternalTransferTaskSnapshot,
  type SessionExternalTransferDrawerSnapshot,
  type SessionExternalWindowToolboxCommandPayload,
  type SessionExternalWindowToolboxCommandResultPayload,
} from "../../../session/external-window/sessionExternalWindow.ts";
import ExternalTransferDrawer from "../../transfers/components/ExternalTransferDrawer.vue";
import { matchSessionLocalShortcut, type SessionLocalShortcutActionId } from "../../../session/input/sessionLocalShortcuts.ts";
import { matchRemoteDesktopShortcut } from "../../../session/input/sessionRemoteDesktopShortcuts.ts";
import type { RemoteSessionTab } from "../../../session/core/sessionManager.ts";
import { sessionSupportsCapability } from "../../../session/connection-model/connectionModel.ts";
import { selectFileTransferSources } from "../../../session/transfers/fileTransferSources.ts";
import type { SessionToolboxContext } from "../../../session/transfers/sessionToolbox.ts";
import { sessionViewportScaleFactor, sessionViewportScaleUsesAutoFit } from "../../../session/viewport/sessionViewportScale.ts";
import { droppedFilePaths, isFileTransferDragEvent } from "./sessionViewport/dragDrop.ts";
import { createSessionViewportHudState } from "./sessionViewport/hudState.ts";
import { createExternalWindowFeedbackController } from "./sessionExternalWindowPage/feedback.ts";
import {
  captureForwardedEvent,
  isInteractiveTarget,
  isKeyboardEditableTarget,
} from "./sessionExternalWindowPage/inputForwarding.ts";
import { useSessionKeyboardCapture } from "../composables/useSessionKeyboardCapture.ts";
import { useSessionKeyboardPreferences } from "../composables/useSessionKeyboardPreferences.ts";
import { createRemoteKeyboardMapper } from "../../../session/input/keyboardMapping.ts";
import { isViewportContentPoint } from "../../../session/viewport/viewportScroll.ts";

const {
  confirmDialog,
  externalWindowLocation,
  fullscreenSessionId,
  getRemoteViewportStatusHud,
  getSessionViewportScaleMode,
  logEvent,
  passwordPrompt,
  openSessionMessageDialog,
  requestDockExternalWindowSession,
  sessionTabs,
  t,
} = useVncAppContext();

const externalState = ref<SessionExternalWindowStatePayload | null>(null);
const windowFeedback = ref<SessionExternalWindowFeedbackPayload | null>(null);
const dragUploadActive = ref(false);
const externalTransferDrawerOpen = ref(false);
const externalTransferTasks = ref<SessionExternalTransferTaskSnapshot[]>([]);
const externalClearingHistory = ref(false);
const externalWindowFullscreen = ref(
  new URL(window.location.href).searchParams.get("fullscreen") === "true",
);
const coreSurfaceMount = ref<HTMLElement | null>(null);
const windowLifetime = useSessionExternalWindowLifetime();
const desktopRecovery = createExternalDesktopRecovery(async (recoveryId, framePresented) => {
  if (!windowLifetime.isActive() || !externalWindowLocation) return;
  await emit(sessionExternalWindowStateRequestEvent(externalWindowLocation.sessionId), {
    sessionId: externalWindowLocation.sessionId, recoveryId, framePresented,
  });
});
const externalSurface = useSessionExternalSurface({
  sessionId: externalWindowLocation?.sessionId ?? "",
  getMountTarget: () => coreSurfaceMount.value,
  onDeactivate: releaseExternalInput,
  onFramePresented: () => desktopRecovery.framePresented(),
  onError: (code) => {
    if (!windowLifetime.isActive()) return;
    logEvent("error", t("log.sourceSession"), `External Surface failed; code=${code}`);
  },
});
const { canvas: surfaceCanvas, active: coreSurfaceActive, commit: coreSurfaceCommit } = externalSurface;
const coreSurfacePhase = externalSurface.phase;
const externalSessionEnded = computed(() => {
  const state = externalState.value?.controlSnapshot.toolboxContext.state;
  return state ? shouldCleanupConnectionForSessionState(state) : false;
});
watch(externalSessionEnded, (ended) => {
  if (ended) externalSurface.dispose();
}, { flush: "sync" });
const frameShell = ref<HTMLElement | null>(null);
const frameViewport = ref<HTMLElement | null>(null);
const { preferences: keyboardPreferences, windowsClient } = useSessionKeyboardPreferences(() => externalWindowLocation?.target ?? "");
const keyboardMapper = createRemoteKeyboardMapper();
const {
  clearFeedbackTimer,
  showExternalWindowFeedback,
  showWindowFeedback,
} = createExternalWindowFeedbackController({
  externalWindowLocation,
  windowFeedback,
});

const keyboardCapture = useSessionKeyboardCapture({
  getContext: () => {
    if (!externalWindowLocation || !frameShell.value
      || externalTransferDrawerOpen.value || confirmDialog.value.visible || passwordPrompt.value.visible
      || !externalSurface.canForwardInput(externalState.value?.inputEnabled === true)) return null;
    return { sessionId: externalWindowLocation.sessionId, target: externalWindowLocation.target, element: frameShell.value };
  },
  onKey: (_sessionId, event, down) => forwardExternalKeyboard(event, down),
  release: () => releaseExternalInput(),
  onWindowFocusChanged: publishWindowFocus,
  onUnavailable: reason => showExternalWindowFeedback(t(reason.includes("permission_required") ? "settings.keyboardTransmission.permissionRequired" : "settings.keyboardTransmission.captureUnavailable"), "warning"),
});
const {
  isViewportHudExpanded,
  isViewportHudHidden,
  toggleViewportHud,
  toggleViewportHudExpanded,
} = createSessionViewportHudState(externalWindowLocation?.sessionId);

const externalHudTab = computed<RemoteSessionTab | null>(() => {
  if (!externalWindowLocation) {
    return null;
  }
  return sessionTabs.value.find((tab) => tab.id === externalWindowLocation.sessionId) ?? {
    id: externalWindowLocation.sessionId,
    label: externalWindowLocation.target,
    target: externalWindowLocation.target,
    state: externalState.value?.controlSnapshot.toolboxContext.state ?? (coreSurfaceActive.value ? "connected" : "connecting"),
    active: true,
    liveCanvas: coreSurfaceActive.value,
    closable: false,
  };
});

const externalToolboxContext = computed<SessionToolboxContext | null>(() => {
  if (!externalWindowLocation) {
    return null;
  }
  const synchronizedContext = externalState.value?.controlSnapshot.toolboxContext;
  if (synchronizedContext) {
    return {
      ...synchronizedContext,
      readonly: Boolean(synchronizedContext.readonly) || !externalSurface.canForwardInput(externalState.value?.inputEnabled === true),
      externalWindow: true,
      fullscreen: externalWindowFullscreen.value,
      fileTransferAvailable: synchronizedContext.fileTransferAvailable || externalState.value?.controlSnapshot.fileTransferAvailable,
    };
  }

  return {
    sessionId: externalWindowLocation.sessionId,
    state: coreSurfaceActive.value ? "connected" : "connecting",
    readonly: !externalSurface.canForwardInput(externalState.value?.inputEnabled === true),
    enhancedChannel: null,
    active: true,
    fullscreen: externalWindowFullscreen.value,
    externalWindow: true,
    localServer: false,
    recording: false,
    fileTransferAvailable: false,
  };
});

const externalStatusHud = computed(() => {
  if (!externalHudTab.value) return null;
  const synchronized = externalState.value?.controlSnapshot.statusHud ?? getRemoteViewportStatusHud(externalHudTab.value);
  if (externalSessionEnded.value) {
    return { ...synchronized, state: t("sessions.externalSurface.disconnected"), inputMode: "readonly", display: "-" };
  }
  if (coreSurfacePhase.value === "failed") {
    return { ...synchronized, state: t("sessions.externalSurface.interrupted"), inputMode: "readonly", display: "-" };
  }
  if (!coreSurfaceActive.value) {
    return { ...synchronized, state: t("sessions.externalSurface.waiting"), inputMode: "readonly" };
  }
  return synchronized;
});

const externalViewportTone = computed(() => externalSessionEnded.value || coreSurfacePhase.value === "failed"
  ? "failed"
  : coreSurfaceActive.value ? externalState.value?.controlSnapshot.viewportTone : "working");

const externalCoreSurfaceStyle = computed(() => {
  if (!externalWindowLocation || !coreSurfaceCommit.value) {
    return {};
  }
  const scaleMode = getSessionViewportScaleMode(externalWindowLocation.sessionId);
  if (sessionViewportScaleUsesAutoFit(scaleMode)) {
    return {};
  }
  const scale = sessionViewportScaleFactor(scaleMode);
  return {
    width: `${Math.round(coreSurfaceCommit.value.width * scale)}px`,
    height: `${Math.round(coreSurfaceCommit.value.height * scale)}px`,
  };
});

const externalFrameScrollable = computed(() => {
  if (!externalWindowLocation) {
    return false;
  }
  return !sessionViewportScaleUsesAutoFit(getSessionViewportScaleMode(externalWindowLocation.sessionId));
});

function emitWindowInput(payload: SessionExternalInputPayload) {
  if (!externalWindowLocation?.windowInstanceId) return Promise.resolve();
  return emitExternalInput(emit, { ...payload, windowInstanceId: externalWindowLocation.windowInstanceId });
}

function releaseExternalInput() {
  keyboardMapper.clear();
  if (!externalWindowLocation) return;
  void emitWindowInput({ sessionId: externalWindowLocation.sessionId, type: "releaseInput" }).catch(() => undefined);
}

function publishWindowFocus(focused: boolean) {
  if (!externalWindowLocation?.windowInstanceId) return;
  if (!focused) releaseExternalInput();
  void emit(sessionExternalWindowFocusEvent(externalWindowLocation.sessionId), {
    sessionId: externalWindowLocation.sessionId,
    windowInstanceId: externalWindowLocation.windowInstanceId,
    focused,
  }).catch(() => undefined);
}

function getFrameInputTarget(): {
  frameRect: DOMRect;
  framebufferWidth: number;
  framebufferHeight: number;
} | null {
  const canvas = surfaceCanvas.value;
  if (!canvas || canvas.width <= 0 || canvas.height <= 0) {
    return null;
  }

  const frameRect = canvas.getBoundingClientRect();
  if (frameRect.width <= 0 || frameRect.height <= 0) {
    return null;
  }

  return {
    frameRect,
    framebufferWidth: canvas.width,
    framebufferHeight: canvas.height,
  };
}

async function emitExternalToolboxCommand(
  sessionId: string,
  key: string,
  extra: Partial<SessionExternalWindowToolboxCommandPayload> = {},
) {
  if (!windowLifetime.isActive()) return;
  await emit(sessionExternalWindowToolboxCommandEvent(sessionId), {
    sessionId,
    key,
    text: "",
    feedbackTarget: "external",
    ...extra,
  } satisfies SessionExternalWindowToolboxCommandPayload).catch((error) => {
    if (windowLifetime.isActive()) showExternalWindowFeedback(String(error), "error");
  });
}

function externalSessionSupportsFileTransfer(): boolean {
  return Boolean(externalToolboxContext.value?.fileTransferAvailable);
}

async function requestExternalFileTransferUpload(sourcePaths: string[]) {
  if (!windowLifetime.isActive() || !externalWindowLocation) {
    return;
  }
  if (!externalSessionSupportsFileTransfer()) {
    showExternalWindowFeedback("文件传输当前不可用。", "warning");
    return;
  }
  if (!sourcePaths.length) {
    showExternalWindowFeedback(t("sessions.fileTransfers.dropUnsupported"), "warning");
    return;
  }

  openExternalTransferDrawer();

  await emitExternalToolboxCommand(externalWindowLocation.sessionId, "action:send-file", {
    selectedFileTransferSourcePaths: sourcePaths,
  });
}

function openExternalTransferDrawer() {
  if (!windowLifetime.isActive()) return;
  externalTransferDrawerOpen.value = true;
  if (externalWindowLocation) void emit(sessionExternalTransferDrawerRequestEvent(externalWindowLocation.sessionId), { sessionId: externalWindowLocation.sessionId, open: true });
}
function closeExternalTransferDrawer() {
  externalTransferDrawerOpen.value = false;
  if (externalWindowLocation) void emit(sessionExternalTransferDrawerRequestEvent(externalWindowLocation.sessionId), { sessionId: externalWindowLocation.sessionId, open: false });
}
function controlExternalTransfer(action: "pause" | "resume" | "retry" | "cancel", taskId: string) {
  if (externalWindowLocation) void emit(sessionExternalTransferDrawerControlEvent(externalWindowLocation.sessionId), { sessionId: externalWindowLocation.sessionId, taskId, action });
}
function controlExternalTransferBatch(action: "pause" | "resume", taskIds: string[]) {
  if (windowLifetime.isActive() && externalWindowLocation) {
    const sessionId = externalWindowLocation.sessionId;
    void emit(sessionExternalTransferDrawerControlEvent(sessionId), { sessionId, taskIds, action: action === "pause" ? "pause-all" : "resume-all" });
  }
}
function handleExternalTransferPathAction(action: "copy-path" | "open-directory", sessionId: string, taskId: string) {
  if (windowLifetime.isActive() && externalWindowLocation?.sessionId === sessionId) {
    void emit(sessionExternalTransferDrawerControlEvent(sessionId), { sessionId, taskId, action });
  }
}
function clearExternalTransferHistory(taskIds: string[]) {
  if (externalWindowLocation) void emit(sessionExternalTransferDrawerControlEvent(externalWindowLocation.sessionId), { sessionId: externalWindowLocation.sessionId, taskIds, action: "clear-history" });
}

async function executeExternalSessionLocalShortcut(
  sessionId: string,
  actionId: SessionLocalShortcutActionId,
) {
  if (!windowLifetime.isActive()) return;
  switch (actionId) {
    case "open-message-dialog":
      openSessionMessageDialog(sessionId);
      return;
    case "toggle-fullscreen":
      await toggleExternalWindowFullscreen();
      return;
    case "toggle-external-window":
      await requestDockExternalWindowSession(sessionId);
      return;
    case "upload-file": {
      if (!externalSessionSupportsFileTransfer() || externalToolboxContext.value?.readonly) {
        showExternalWindowFeedback("文件传输当前不可用。", "warning");
        return;
      }
      let sourcePaths: string[];
      try { sourcePaths = await selectFileTransferSources(invoke, t); }
      catch (error) { showExternalWindowFeedback(String(error), "error"); return; }
      if (!windowLifetime.isActive() || !sourcePaths.length) return;
      await requestExternalFileTransferUpload(sourcePaths);
      return;
    }
    case "open-file-transfer-center":
      openExternalTransferDrawer();
      return;
    case "retry-session":
      await emitExternalToolboxCommand(sessionId, "action:retry-session");
      return;
  }
}

async function toggleExternalWindowFullscreen() {
  if (!windowLifetime.isActive()) return;
  try {
    const currentWindow = getCurrentWindow();
    const fullscreen = await isSessionWindowExpanded(currentWindow);
    if (!windowLifetime.isActive()) return;
    await setSessionWindowExpanded(currentWindow, !fullscreen);
    if (windowLifetime.isActive()) externalWindowFullscreen.value = !fullscreen;
  } catch {
    if (windowLifetime.isActive()) showExternalWindowFeedback("当前环境不支持切换独立会话窗口全屏。", "warning");
  }
}

async function closeDisconnectedExternalWindow() {
  try {
    await getCurrentWindow().close();
  } catch {
    showExternalWindowFeedback(t("sessions.externalSurface.closeFailed"), "error");
  }
}

function forwardExternalKeyboard(event: KeyboardEvent, down: boolean) {
  if (!externalWindowLocation || isInteractiveTarget(event)) {
    return;
  }

  const route = keyboardCapture.routeKeyboardEvent(externalWindowLocation.sessionId, event, down);
  const localShortcut = matchSessionLocalShortcut({
    event,
    remotePriority: route === "remote",
    state: externalHudTab.value?.state,
    active: true,
    visible: true,
    editableTarget: isKeyboardEditableTarget(event.target),
  });
  if (localShortcut) {
    captureForwardedEvent(event);
    if (down) {
      void executeExternalSessionLocalShortcut(externalWindowLocation.sessionId, localShortcut);
    }
    return;
  }

  if (route === "local" || !externalSurface.canForwardInput(externalState.value?.inputEnabled === true)) {
    return;
  }

  const remoteDesktopShortcut = matchRemoteDesktopShortcut({
    event,
    externalWindow: true,
    remotePriority: route === "remote",
  });
  if (remoteDesktopShortcut) {
    captureForwardedEvent(event);
    if (down) {
      void emitWindowInput({
        sessionId: externalWindowLocation.sessionId,
        type: "remoteDesktopShortcut",
        actionId: remoteDesktopShortcut,
      });
    }
    return;
  }

  const payload = createExternalKeyboardInputPayload({
    sessionId: externalWindowLocation.sessionId,
    event,
    down,
    mapKey: (key, pressed) => keyboardMapper.map(key, pressed, {
      rightControlAsCommand: windowsClient && keyboardPreferences.value.rightControlAsCommand,
    }),
  });
  if (!payload) {
    return;
  }

  captureForwardedEvent(event);
  void emitWindowInput(payload);
}

function forwardExternalPointer(event: PointerEvent, action: "move" | "down" | "up") {
  if (!externalWindowLocation || isInteractiveTarget(event)) {
    return;
  }
  if (!isViewportContentPoint(frameShell.value, event)) return;
  if (!externalSurface.canForwardInput(externalState.value?.inputEnabled === true)) {
    return;
  }

  const target = getFrameInputTarget();
  if (!target) {
    return;
  }

  captureForwardedEvent(event);
  // preventDefault suppresses the browser's normal focus transfer. Explicitly
  // return focus from toolbox buttons/inputs to the remote surface on a click.
  if (action === "down") frameShell.value?.focus({ preventScroll: true });
  void emitWindowInput(createExternalPointerInputPayload({
    sessionId: externalWindowLocation.sessionId,
    action,
    clientX: event.clientX,
    clientY: event.clientY,
    button: event.button,
    buttons: event.buttons,
    frameRect: target.frameRect,
    framebufferWidth: target.framebufferWidth,
    framebufferHeight: target.framebufferHeight,
  }));
}

function forwardExternalWheel(event: WheelEvent) {
  if (!externalWindowLocation || isInteractiveTarget(event)) {
    return;
  }
  if (!isViewportContentPoint(frameShell.value, event)) return;
  if (!externalSurface.canForwardInput(externalState.value?.inputEnabled === true)) {
    return;
  }

  const target = getFrameInputTarget();
  if (!target) {
    return;
  }

  captureForwardedEvent(event);
  void emitWindowInput(createExternalWheelInputPayload({
    sessionId: externalWindowLocation.sessionId,
    clientX: event.clientX,
    clientY: event.clientY,
    buttons: event.buttons,
    deltaX: event.deltaX,
    deltaY: event.deltaY,
    frameRect: target.frameRect,
    framebufferWidth: target.framebufferWidth,
    framebufferHeight: target.framebufferHeight,
  }));
}

function handleWindowKeydown(event: KeyboardEvent) {
  forwardExternalKeyboard(event, true);
}

function handleWindowKeyup(event: KeyboardEvent) {
  forwardExternalKeyboard(event, false);
}

async function installNativeFileTransferDragListener() {
  try {
    await windowLifetime.register(() => getCurrentWebview().onDragDropEvent((event) => {
      if (windowLifetime.isActive()) void handleNativeFileTransferDragEvent(event.payload);
    }));
  } catch { /* Native drag/drop remains optional. */ }
}

async function handleNativeFileTransferDragEvent(payload: DragDropEvent) {
  if (!windowLifetime.isActive()) return;
  if (payload.type === "leave") {
    dragUploadActive.value = false;
    return;
  }

  dragUploadActive.value = externalSessionSupportsFileTransfer();
  if (payload.type !== "drop") {
    return;
  }

  dragUploadActive.value = false;
  await requestExternalFileTransferUpload(payload.paths);
}

function handleFileTransferDragEnter(event: DragEvent) {
  if (!isFileTransferDragEvent(event)) {
    return;
  }
  if (!externalSessionSupportsFileTransfer()) {
    event.dataTransfer!.dropEffect = "none";
    dragUploadActive.value = false;
    return;
  }
  event.dataTransfer!.dropEffect = "copy";
  dragUploadActive.value = true;
}

function handleFileTransferDragOver(event: DragEvent) {
  if (!isFileTransferDragEvent(event)) {
    return;
  }
  if (!externalSessionSupportsFileTransfer()) {
    event.dataTransfer!.dropEffect = "none";
    dragUploadActive.value = false;
    return;
  }
  event.dataTransfer!.dropEffect = "copy";
  dragUploadActive.value = true;
}

function handleFileTransferDragLeave(event: DragEvent) {
  const nextTarget = event.relatedTarget;
  const currentTarget = event.currentTarget;
  if (nextTarget instanceof Node && currentTarget instanceof Node && currentTarget.contains(nextTarget)) {
    return;
  }
  dragUploadActive.value = false;
}

async function handleFileTransferDrop(event: DragEvent) {
  if (!isFileTransferDragEvent(event)) {
    return;
  }
  dragUploadActive.value = false;
  await requestExternalFileTransferUpload(droppedFilePaths(event));
}

onMounted(() => {
  void isSessionWindowExpanded(getCurrentWindow()).then((fullscreen) => {
    if (windowLifetime.isActive()) externalWindowFullscreen.value = fullscreen;
  }).catch(() => undefined);
  if (!externalWindowLocation) {
    return;
  }

  window.addEventListener("keydown", handleWindowKeydown, true);
  window.addEventListener("keyup", handleWindowKeyup, true);
  void installNativeFileTransferDragListener();

  void externalSurface.start();

  void windowLifetime.register(() => listen<SessionExternalWindowStatePayload>(
    sessionExternalWindowStateEvent(externalWindowLocation.sessionId),
    (event) => {
      if (!windowLifetime.isActive()) return;
      if (event.payload.sessionId === externalWindowLocation.sessionId) {
        externalState.value = event.payload;
        desktopRecovery.consume(event.payload.controlSnapshot.statusHud);
      }
    },
  ), () => {
    void emit(sessionExternalWindowStateRequestEvent(externalWindowLocation.sessionId), externalWindowLocation.sessionId);
  }).catch(() => undefined);

  void windowLifetime.register(() => listen<SessionExternalWindowFeedbackPayload>(
    sessionExternalWindowFeedbackEvent(externalWindowLocation.sessionId),
    (event) => {
      if (!windowLifetime.isActive()) return;
      if (event.payload.sessionId === externalWindowLocation.sessionId) {
        showWindowFeedback(event.payload);
      }
    },
  )).catch(() => undefined);

  void windowLifetime.register(() => listen<SessionExternalWindowToolboxCommandResultPayload>(
    sessionExternalWindowToolboxCommandResultEvent(externalWindowLocation.sessionId),
    (event) => {
      if (!windowLifetime.isActive()) return;
      if (event.payload.sessionId === externalWindowLocation.sessionId && event.payload.diagnosticView === undefined && event.payload.message) {
        showExternalWindowFeedback(event.payload.message, event.payload.tone);
      }
    },
  )).catch(() => undefined);

  void windowLifetime.register(() => listen<SessionExternalTransferDrawerSnapshot>(
    sessionExternalTransferDrawerSnapshotEvent(externalWindowLocation.sessionId),
    (event) => {
      if (!windowLifetime.isActive()) return;
      externalTransferTasks.value = event.payload.tasks.filter((task) => task.sessionId === externalWindowLocation?.sessionId);
      externalClearingHistory.value = event.payload.clearingHistory;
    },
  ), () => {
    // The main window may answer the first open request before this async listener is ready.
    if (externalTransferDrawerOpen.value) {
      void emit(sessionExternalTransferDrawerRequestEvent(externalWindowLocation.sessionId), { sessionId: externalWindowLocation.sessionId, open: true });
    }
  }).catch(() => undefined);
  void windowLifetime.register(() => listen<{ sessionId: string; open: boolean }>(
    sessionExternalTransferDrawerRequestEvent(externalWindowLocation.sessionId),
    (event) => {
      if (windowLifetime.isActive() && event.payload.sessionId === externalWindowLocation?.sessionId) externalTransferDrawerOpen.value = event.payload.open;
    },
  )).catch(() => undefined);

});

onBeforeUnmount(() => {
  publishWindowFocus(false);
  windowLifetime.dispose();
  window.removeEventListener("keydown", handleWindowKeydown, true);
  window.removeEventListener("keyup", handleWindowKeyup, true);
  externalSurface.dispose();
  if (externalWindowLocation) void emit(sessionExternalTransferDrawerRequestEvent(externalWindowLocation.sessionId), { sessionId: externalWindowLocation.sessionId, open: false }).catch(() => undefined);
  clearFeedbackTimer();
});
</script>

<template>
  <main
    :class="[
      'session-external-window-page',
      { 'viewport-fullscreen-active': fullscreenSessionId },
    ]"
    tabindex="0"
    @pointerdown.capture="forwardExternalPointer($event, 'down')"
    @pointermove.capture="forwardExternalPointer($event, 'move')"
    @pointerup.capture="forwardExternalPointer($event, 'up')"
    @wheel.capture="forwardExternalWheel"
    @dragenter.prevent="handleFileTransferDragEnter"
    @dragover.prevent="handleFileTransferDragOver"
    @dragleave="handleFileTransferDragLeave"
    @drop.prevent="handleFileTransferDrop"
  >
    <section class="workspace session-external-window-workspace">
      <div ref="frameViewport" class="session-external-viewport">
        <div
          ref="frameShell"
          :data-keyboard-capture-state="keyboardCapture.status.value"
          tabindex="0"
          :class="[
            'session-external-frame-shell',
            'session-viewport-scroll',
            {
              'is-scrollable': externalFrameScrollable,
              'hud-visible': externalWindowLocation && !isViewportHudHidden(externalWindowLocation.sessionId),
              'hud-expanded': externalWindowLocation && !isViewportHudHidden(externalWindowLocation.sessionId) && isViewportHudExpanded(externalWindowLocation.sessionId),
            },
          ]"
        >
          <div
            ref="coreSurfaceMount"
            v-show="coreSurfaceActive && !externalSessionEnded"
            class="session-external-core-surface"
            :style="externalCoreSurfaceStyle"
          ></div>
          <p v-if="keyboardCapture.status.value === 'unavailable'" class="session-keyboard-capture-warning" role="status">{{ t('settings.keyboardTransmission.captureUnavailable') }}</p>
          <div v-if="externalSessionEnded || !coreSurfaceActive" class="session-external-frame-waiting">
            <p>{{ t(externalSessionEnded ? 'sessions.externalSurface.disconnected' : coreSurfacePhase === 'failed' ? 'sessions.externalSurface.interrupted' : 'sessions.externalSurface.waiting') }}</p>
            <p v-if="externalSessionEnded">{{ t('sessions.externalSurface.retryInMain') }}</p>
            <button v-if="externalSessionEnded" type="button" @click="closeDisconnectedExternalWindow">
              {{ t('sessions.externalSurface.closeWindow') }}
            </button>
            <button v-else-if="coreSurfacePhase === 'failed' && externalWindowLocation" type="button"
              @click="requestDockExternalWindowSession(externalWindowLocation.sessionId)">
              {{ t('sessions.externalSurface.returnToMain') }}
            </button>
          </div>
        </div>
        <SessionViewportChrome
          v-if="externalHudTab && !externalSessionEnded"
          :tab="externalHudTab"
          :viewport-target="frameViewport"
          :is-hud-hidden="isViewportHudHidden"
          :is-hud-expanded="isViewportHudExpanded"
          :toggle-hud="toggleViewportHud"
          :toggle-hud-expanded="toggleViewportHudExpanded"
          :toggle-external-window-fullscreen="toggleExternalWindowFullscreen"
          :toolbox-context-override="externalToolboxContext"
          :status-hud-override="externalStatusHud"
          :viewport-tone-override="externalViewportTone"
          :on-edge-scroll="event => forwardExternalPointer(event, 'move')"
        />
        <div v-if="dragUploadActive && externalSessionSupportsFileTransfer()" class="session-file-transfer-drop-overlay">
          <strong>{{ t("sessions.fileTransfers.dropTitle") }}</strong>
          <span>{{ t("sessions.fileTransfers.dropHint") }}</span>
        </div>
      </div>
    </section>

    <p
      v-if="windowFeedback"
      :class="['session-external-window-feedback', `tone-${windowFeedback.tone}`]"
    >
      {{ windowFeedback.message }}
    </p>

    <ExternalTransferDrawer
      v-if="externalWindowLocation"
      :show="externalTransferDrawerOpen"
      :tasks="externalTransferTasks"
      :clearing-history="externalClearingHistory"
      :can-open-remote-directory="externalToolboxContext?.state === 'connected' && !externalToolboxContext.readonly && sessionSupportsCapability(externalToolboxContext.capabilities, 'file.reveal')"
      :title="`${t('sessions.fileTransfers.globalDrawerTitle')} · ${externalWindowLocation.target}`"
      :empty-description="t('sessions.fileTransfers.globalEmpty')"
      @update:show="value => value ? openExternalTransferDrawer() : closeExternalTransferDrawer()"
      @task-action="controlExternalTransfer"
      @batch-action="controlExternalTransferBatch"
      @path-action="handleExternalTransferPathAction"
      @clear-history="clearExternalTransferHistory"
    />

    <PasswordPromptDialog v-if="passwordPrompt.visible" />
    <ConfirmDialog v-if="confirmDialog.visible" />
  </main>
</template>
