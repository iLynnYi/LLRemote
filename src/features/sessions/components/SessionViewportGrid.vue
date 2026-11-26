<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NEmpty } from "naive-ui";
import { OpenOutline } from "@vicons/ionicons5";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import {
  createExternalPointerInputPayload,
  createExternalWheelInputPayload,
} from "../composables/useSessionExternalInputBridge.ts";
import SessionViewportChrome from "./SessionViewportChrome.vue";
import SessionViewportPlaceholder from "./SessionViewportPlaceholder.vue";
import { sessionSupportsAction } from "../../../session/transfers/sessionCapabilities.ts";
import { selectFileTransferSources } from "../../../session/transfers/fileTransferSources.ts";
import { matchSessionLocalShortcut, type SessionLocalShortcutActionId } from "../../../session/input/sessionLocalShortcuts.ts";
import { droppedFilePaths, isFileTransferDragEvent } from "./sessionViewport/dragDrop.ts";
import { createSessionViewportHudState } from "./sessionViewport/hudState.ts";
import { createViewportPanController } from "./sessionViewportGrid/inputForwarding.ts";
import { useSessionKeyboardCapture } from "../composables/useSessionKeyboardCapture.ts";
import { isViewportContentPoint } from "../../../session/viewport/viewportScroll.ts";

const {
  activeWorkspace,
  activeWorkspaceRenderSlot,
  activateSessionTab,
  claimMainWindowInput,
  setMainWindowFocused,
  confirmDialog,
  connectionConfirmDraft,
  enqueueSessionFileUploads,
  findSession,
  focusExternalWindowSession,
  getRemoteViewportSummary,
  getSessionViewportScaleMode,
  getWorkspaceRenderSlot,
  globalTransferDrawerOpen,
  globalTransferDrawerSessionId,
  handleSessionViewportKeyboardEvent,
  releaseSessionKeyboardInput,
  openGlobalTransferDrawer,
  openSessionMessageDialog,
  passwordPrompt,
  recordMainViewportInput,
  retrySessionFromViewport,
  sessionConnectionRegistry,
  sessionListTick,
  sessionTabs,
  setScreenTarget,
  setViewportTarget,
  showAppMessage,
  t,
  toggleSessionFullscreen,
  toggleExternalWindowSession,
  workspaceSessionTabs,
} = useVncAppContext();

const dragUploadSessionId = ref<string | null>(null);
const viewportTargetsBySessionId = ref<Record<string, HTMLElement | null>>({});

const keyboardCapture = useSessionKeyboardCapture({
  getContext: () => {
    if (activeWorkspace.value !== "sessions" || connectionConfirmDraft.value || confirmDialog.value.visible || passwordPrompt.value.visible || globalTransferDrawerOpen.value) return null;
    const tab = sessionTabs.value.find(tab => tab.active);
    if (!tab || findSession(tab.id)?.state !== "connected") return null;
    const slot = getWorkspaceRenderSlot(tab.id);
    const element = viewportTargetsBySessionId.value[tab.id];
    if (!slot.active || !slot.visible || !element || isReadonlyViewport(tab.id)) return null;
    return { sessionId: tab.id, target: tab.target, element };
  },
  onKey: handleRemoteViewportKeyboardEvent,
  release: releaseSessionKeyboardInput,
  onWindowFocusChanged: setMainWindowFocused,
  onUnavailable: reason => showAppMessage(t(reason.includes("permission_required") ? "settings.keyboardTransmission.permissionRequired" : "settings.keyboardTransmission.captureUnavailable"), "warning"),
});
let unlistenNativeFileDrop: UnlistenFn | null = null;
const {
  isViewportHudExpanded,
  isViewportHudHidden,
  toggleViewportHud,
  toggleViewportHudExpanded,
} = createSessionViewportHudState();
const {
  finishViewportPan,
  moveViewportPan,
  panningSessionId,
  shouldStartViewportPan,
  startViewportPan,
} = createViewportPanController({
  captureForwardedEvent,
  getSessionViewportScaleMode,
  getViewportScrollTarget,
});

function setRemoteViewportTarget(sessionId: string, element: Element | null) {
  setViewportTarget(sessionId, element);
  const target = element instanceof HTMLElement ? element : null;
  if (viewportTargetsBySessionId.value[sessionId] === target) {
    return;
  }

  viewportTargetsBySessionId.value = {
    ...viewportTargetsBySessionId.value,
    [sessionId]: target,
  };
}

function isLocalInteractiveTarget(event: Event): boolean {
  const selectors = [
    "button",
    "input",
    "select",
    "textarea",
    "[contenteditable='true']",
    "[role='button']",
    ".n-button",
    ".n-dropdown-menu",
    ".n-input",
    ".n-select",
    ".session-toolbox-trigger",
    ".session-toolbox-button",
    ".session-toolbox-menu",
    ".remote-viewport-floating-stack",
    ".remote-viewport-hud",
    ".remote-viewport-mode-toggle",
  ].join(",");

  return event.composedPath().some((target) => (
    target instanceof Element && Boolean(target.closest(selectors))
  ));
}

function getMainFrameInputTarget(sessionId: string): {
  frameRect: DOMRect;
  framebufferWidth: number;
  framebufferHeight: number;
} | null {
  const canvas = viewportTargetsBySessionId.value[sessionId]?.querySelector<HTMLCanvasElement>(".screen-mount canvas");
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

function captureForwardedEvent(event: Event) {
  event.preventDefault();
  event.stopPropagation();
}

function focusRemoteViewport(sessionId: string) {
  viewportTargetsBySessionId.value[sessionId]?.focus({ preventScroll: true });
}

function getViewportScrollTarget(sessionId: string): HTMLElement | null {
  return viewportTargetsBySessionId.value[sessionId]?.querySelector<HTMLElement>(".session-viewport-scroll") ?? null;
}

function isKeyboardEditableTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  );
}

function isReadonlyViewport(sessionId: string): boolean {
  const tab = sessionTabs.value.find((sessionTab) => sessionTab.id === sessionId);
  return tab ? getRemoteViewportSummary(tab).inputMode === "readonly" : false;
}

function sessionSupportsFileTransfer(sessionId: string): boolean {
  sessionListTick.value;
  const connection = sessionConnectionRegistry.get(sessionId)?.connection;
  return sessionSupportsAction(connection?.connectionModel.capabilities, "sendFile");
}

function handleRemoteViewportKeyboardEvent(sessionId: string, event: KeyboardEvent, down: boolean) {
  const route = keyboardCapture.routeKeyboardEvent(sessionId, event, down);
  const actionId = matchSessionLocalShortcut({
    event,
    remotePriority: route === "remote",
    state: findSession(sessionId)?.state,
    active: Boolean(sessionTabs.value.find((tab) => tab.id === sessionId)?.active),
    visible: getWorkspaceRenderSlot(sessionId).visible,
    editableTarget: isKeyboardEditableTarget(event.target),
  });

  if (actionId) {
    captureForwardedEvent(event);
    if (down) {
      void executeSessionLocalShortcut(sessionId, actionId);
    }
    return;
  }

  if (route === "local") return;
  handleSessionViewportKeyboardEvent(sessionId, event, down, { remotePriority: route === "remote" });
}

async function executeSessionLocalShortcut(sessionId: string, actionId: SessionLocalShortcutActionId) {
  switch (actionId) {
    case "open-message-dialog":
      openSessionMessageDialog(sessionId);
      return;
    case "toggle-fullscreen":
      await toggleSessionFullscreen(sessionId);
      return;
    case "toggle-external-window":
      await toggleExternalWindowSession(sessionId);
      return;
    case "upload-file": {
      if (!sessionSupportsFileTransfer(sessionId) || isReadonlyViewport(sessionId)) {
        return;
      }
      let sourcePaths: string[];
      try { sourcePaths = await selectFileTransferSources(invoke, t); }
      catch (error) { showAppMessage(String(error), "error"); return; }
      if (!sourcePaths.length) {
        return;
      }
      await activateSessionTab(sessionId);
      try {
        await enqueueDroppedFileTransfer(sessionId, sourcePaths);
      } catch (error) {
        showAppMessage(t("sessions.toolbox.feedback.failed", { error: String(error) }), "error");
      }
      return;
    }
    case "open-file-transfer-center":
      openGlobalTransferDrawer(sessionId);
      return;
    case "retry-session":
      await retrySessionFromViewport(sessionId);
      return;
  }
}

// The toolbox menu is outside the viewport; keep its transfer shortcut local.
function handleWindowSessionShortcut(event: KeyboardEvent, down: boolean) {
  if (!event.composedPath().some((target) => target instanceof Element
    && target.closest(".session-toolbox-trigger, .session-toolbox-button, .session-toolbox-menu"))) {
    return;
  }
  const sessionId = sessionTabs.value.find((tab) => tab.active)?.id;
  if (!sessionId) return;
  const actionId = matchSessionLocalShortcut({
    event,
    state: findSession(sessionId)?.state,
    active: true,
    visible: getWorkspaceRenderSlot(sessionId).visible,
    editableTarget: isKeyboardEditableTarget(event.target),
  });
  if (actionId !== "open-file-transfer-center" && actionId !== "upload-file" && actionId !== "open-message-dialog") return;
  captureForwardedEvent(event);
  if (down) void executeSessionLocalShortcut(sessionId, actionId);
}

async function installNativeFileTransferDragListener() {
  try {
    unlistenNativeFileDrop = await getCurrentWebview().onDragDropEvent((event) => {
      void handleNativeFileTransferDragEvent(event.payload);
    });
  } catch {
    unlistenNativeFileDrop = null;
  }
}

async function handleNativeFileTransferDragEvent(payload: DragDropEvent) {
  if (payload.type === "leave") {
    dragUploadSessionId.value = null;
    return;
  }

  const sessionId = sessionIdFromDragPosition(payload.position);
  dragUploadSessionId.value = sessionId && sessionSupportsFileTransfer(sessionId) ? sessionId : null;

  if (payload.type !== "drop") {
    return;
  }

  dragUploadSessionId.value = null;
  if (!sessionId || !payload.paths.length) {
    showAppMessage(t("sessions.fileTransfers.dropUnsupported"), "warning");
    return;
  }
  if (!sessionSupportsFileTransfer(sessionId)) {
    showAppMessage(t("sessions.toolbox.disabled.enhancedCapability"), "warning");
    return;
  }

  await activateSessionTab(sessionId);
  try {
    await enqueueDroppedFileTransfer(sessionId, payload.paths);
  } catch (error) {
    showAppMessage(t("sessions.toolbox.feedback.failed", { error: String(error) }), "error");
  }
}

function sessionIdFromDragPosition(position: { x: number; y: number }): string | null {
  const scale = window.devicePixelRatio || 1;
  const element = document.elementFromPoint(position.x / scale, position.y / scale);
  const viewport = element?.closest<HTMLElement>(".remote-viewport.visible");
  return viewport?.dataset.sessionId ?? null;
}

function handleFileTransferDragEnter(event: DragEvent, sessionId: string) {
  if (!isFileTransferDragEvent(event)) {
    return;
  }
  if (!sessionSupportsFileTransfer(sessionId)) {
    event.dataTransfer!.dropEffect = "none";
    dragUploadSessionId.value = null;
    return;
  }
  event.dataTransfer!.dropEffect = "copy";
  dragUploadSessionId.value = sessionId;
}

function handleFileTransferDragOver(event: DragEvent, sessionId: string) {
  if (!isFileTransferDragEvent(event)) {
    return;
  }
  if (!sessionSupportsFileTransfer(sessionId)) {
    event.dataTransfer!.dropEffect = "none";
    dragUploadSessionId.value = null;
    return;
  }
  event.dataTransfer!.dropEffect = "copy";
  dragUploadSessionId.value = sessionId;
}

function handleFileTransferDragLeave(event: DragEvent, sessionId: string) {
  if (dragUploadSessionId.value !== sessionId) {
    return;
  }
  const nextTarget = event.relatedTarget;
  const currentTarget = event.currentTarget;
  if (nextTarget instanceof Node && currentTarget instanceof Node && currentTarget.contains(nextTarget)) {
    return;
  }
  dragUploadSessionId.value = null;
}

async function handleFileTransferDrop(event: DragEvent, sessionId: string) {
  if (!isFileTransferDragEvent(event)) {
    return;
  }
  dragUploadSessionId.value = null;
  await activateSessionTab(sessionId);

  const sourcePaths = droppedFilePaths(event);
  if (!sourcePaths.length) {
    showAppMessage(t("sessions.fileTransfers.dropUnsupported"), "warning");
    return;
  }

  try {
    await enqueueDroppedFileTransfer(sessionId, sourcePaths);
  } catch (error) {
    showAppMessage(t("sessions.toolbox.feedback.failed", { error: String(error) }), "error");
  }
}

async function enqueueDroppedFileTransfer(sessionId: string, sourcePaths: string[]) {
  await enqueueSessionFileUploads(sessionId, sourcePaths, {
    onBatchFinished: (completed, failed, skipped) => {
      const visible = globalTransferDrawerOpen.value
        && (globalTransferDrawerSessionId.value === null || globalTransferDrawerSessionId.value === sessionId);
      if (!failed && !skipped && visible) return;
      showAppMessage(t("sessions.toolbox.feedback.fileUploadBatchFinished", { completed, failed, skipped }),
        failed ? "error" : skipped ? "warning" : "success");
    },
  });
  openGlobalTransferDrawer(sessionId);
}

async function forwardMainPointer(tabId: string, event: PointerEvent, action: "move" | "down" | "up") {
  if (isLocalInteractiveTarget(event)) {
    return;
  }

  if (panningSessionId.value) {
    if (action === "move") {
      moveViewportPan(event);
      return;
    }
    if (action === "up") {
      finishViewportPan(event);
      return;
    }
  }

  if (!isViewportContentPoint(getViewportScrollTarget(tabId), event)) return;

  if (action === "down" && shouldStartViewportPan(tabId, event)) {
    startViewportPan(tabId, event);
    return;
  }

  if (isReadonlyViewport(tabId)) {
    return;
  }

  const target = getMainFrameInputTarget(tabId);
  const connection = sessionConnectionRegistry.get(tabId)?.connection;
  if (!target || !connection) {
    return;
  }

  captureForwardedEvent(event);
  if (action === "down") claimMainWindowInput();
  connection.forwardExternalInput(createExternalPointerInputPayload({
    sessionId: tabId,
    action,
    clientX: event.clientX,
    clientY: event.clientY,
    button: event.button,
    buttons: event.buttons,
    frameRect: target.frameRect,
    framebufferWidth: target.framebufferWidth,
    framebufferHeight: target.framebufferHeight,
  }));
  recordMainViewportInput(tabId, "pointer");

  if (action === "down") {
    focusRemoteViewport(tabId);
    void activateSessionTab(tabId);
  }
}

function forwardMainWheel(tabId: string, event: WheelEvent) {
  if (isLocalInteractiveTarget(event)) {
    return;
  }

  if (!isViewportContentPoint(getViewportScrollTarget(tabId), event)) return;

  if (isReadonlyViewport(tabId)) {
    return;
  }

  const target = getMainFrameInputTarget(tabId);
  const connection = sessionConnectionRegistry.get(tabId)?.connection;
  if (!target || !connection) {
    return;
  }

  captureForwardedEvent(event);
  connection.forwardExternalInput(createExternalWheelInputPayload({
    sessionId: tabId,
    clientX: event.clientX,
    clientY: event.clientY,
    buttons: event.buttons,
    deltaX: event.deltaX,
    deltaY: event.deltaY,
    frameRect: target.frameRect,
    framebufferWidth: target.framebufferWidth,
    framebufferHeight: target.framebufferHeight,
  }));
  recordMainViewportInput(tabId, "wheel");
}

const handleWindowSessionKeydown = (event: KeyboardEvent) => handleWindowSessionShortcut(event, true);
const handleWindowSessionKeyup = (event: KeyboardEvent) => handleWindowSessionShortcut(event, false);

onMounted(() => {
  window.addEventListener("keydown", handleWindowSessionKeydown, true);
  window.addEventListener("keyup", handleWindowSessionKeyup, true);
  void installNativeFileTransferDragListener();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleWindowSessionKeydown, true);
  window.removeEventListener("keyup", handleWindowSessionKeyup, true);
  unlistenNativeFileDrop?.();
  unlistenNativeFileDrop = null;
});

</script>

<template>
  <section class="screen-frame">
    <article
      :data-keyboard-capture-state="tab.active ? keyboardCapture.status.value : 'inactive'"
      v-for="tab in sessionTabs"
      :key="tab.id"
      :ref="(element) => setRemoteViewportTarget(tab.id, element as Element | null)"
      tabindex="0"
      :data-session-id="tab.id"
      :class="[
        'remote-viewport',
        getWorkspaceRenderSlot(tab.id).renderState,
        {
          visible: getWorkspaceRenderSlot(tab.id).visible,
          parked: getWorkspaceRenderSlot(tab.id).renderState === 'parked',
          external: getWorkspaceRenderSlot(tab.id).renderState === 'external',
          active: tab.active,
          'external-window-candidate': getWorkspaceRenderSlot(tab.id).externalWindowCandidate,
          'external-window-owned': getWorkspaceRenderSlot(tab.id).externalWindowOwned,
          'file-transfer-drag-active': dragUploadSessionId === tab.id,
          'is-panning': panningSessionId === tab.id,
          'hud-visible': !isViewportHudHidden(tab.id),
          'hud-expanded': !isViewportHudHidden(tab.id) && isViewportHudExpanded(tab.id),
        },
      ]"
      @click="activateSessionTab(tab.id)"
      @dragenter.prevent="handleFileTransferDragEnter($event, tab.id)"
      @dragover.prevent="handleFileTransferDragOver($event, tab.id)"
      @dragleave="handleFileTransferDragLeave($event, tab.id)"
      @drop.prevent="handleFileTransferDrop($event, tab.id)"
      @pointerdown.capture="forwardMainPointer(tab.id, $event, 'down')"
      @pointermove.capture="forwardMainPointer(tab.id, $event, 'move')"
      @pointerup.capture="forwardMainPointer(tab.id, $event, 'up')"
      @pointercancel.capture="finishViewportPan($event)"
      @wheel.capture="forwardMainWheel(tab.id, $event)"
      @keydown.capture="handleRemoteViewportKeyboardEvent(tab.id, $event, true)"
      @keyup.capture="handleRemoteViewportKeyboardEvent(tab.id, $event, false)"
    >
      <div
        :class="[
          'screen',
          'session-viewport-scroll',
          `scale-${getSessionViewportScaleMode(tab.id)}`,
          getWorkspaceRenderSlot(tab.id).renderState,
          {
            visible: getWorkspaceRenderSlot(tab.id).visible,
            parked: getWorkspaceRenderSlot(tab.id).renderState === 'parked',
            external: getWorkspaceRenderSlot(tab.id).renderState === 'external',
            'input-active': tab.active,
          },
        ]"
      >
        <div
          :ref="(element) => setScreenTarget(tab.id, element as Element | null)"
          class="screen-mount"
        ></div>
      </div>
      <div
        v-if="tab.state === 'probing' || tab.state === 'connecting'"
        class="session-connection-loading"
        role="status"
        aria-live="polite"
      >
        <span class="session-connection-spinner" aria-hidden="true"></span>
        <strong>{{ t("sessions.connectionLoading.title") }}</strong>
        <span class="session-connection-target">{{ tab.target }}</span>
        <p>{{ t("sessions.connectionLoading.hint") }}</p>
      </div>
      <div v-if="dragUploadSessionId === tab.id && sessionSupportsFileTransfer(tab.id)" class="session-file-transfer-drop-overlay">
        <strong>{{ t("sessions.fileTransfers.dropTitle") }}</strong>
        <span>{{ t("sessions.fileTransfers.dropHint") }}</span>
      </div>
      <p v-if="keyboardCapture.status.value === 'unavailable'" class="session-keyboard-capture-warning" role="status">{{ t('settings.keyboardTransmission.captureUnavailable') }}</p>
      <SessionViewportChrome
        :tab="tab"
        :viewport-target="viewportTargetsBySessionId[tab.id] ?? null"
        :is-hud-hidden="isViewportHudHidden"
        :is-hud-expanded="isViewportHudExpanded"
        :toggle-hud="toggleViewportHud"
        :toggle-hud-expanded="toggleViewportHudExpanded"
        :on-edge-scroll="event => forwardMainPointer(tab.id, event, 'move')"
      />
    </article>
    <div v-if="!workspaceSessionTabs.length && activeWorkspaceRenderSlot?.externalWindowOwned" class="session-external-window-placeholder">
      <SessionViewportPlaceholder
        :title="t('sessions.externalWindowPlaceholder.title')"
        :description="t('sessions.externalWindowPlaceholder.hint')"
        :target="activeWorkspaceRenderSlot.target"
        tone="info"
      >
        <template #icon><OpenOutline /></template>
        <NButton type="primary" @click="focusExternalWindowSession(activeWorkspaceRenderSlot.id)">{{ t('sessions.externalWindowPlaceholder.focus') }}</NButton>
        <NButton secondary @click="toggleExternalWindowSession(activeWorkspaceRenderSlot.id)">{{ t('sessions.externalSurface.returnToMain') }}</NButton>
      </SessionViewportPlaceholder>
    </div>
    <NEmpty v-else-if="!workspaceSessionTabs.length" class="screen-empty" :description="t('sessions.emptyWorkspace')" />
  </section>
</template>
