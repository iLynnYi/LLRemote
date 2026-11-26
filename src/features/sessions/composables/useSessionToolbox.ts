import { computed, onBeforeUnmount, onMounted, ref, watch, type Ref } from "vue";
import {
  resolveVisibleSessionToolboxButton,
  snapSessionToolboxButton,
  type SessionToolboxButtonPosition,
} from "../../../session/transfers/sessionToolbox.ts";

export type SessionToolboxDragOptions = {
  workspaceRef: Ref<HTMLElement | null>;
  isInside: (event: Event) => boolean;
};

const BUTTON_WIDTH = 32;
const BUTTON_HEIGHT = 32;
const DEFAULT_INSET = 16;
const LONG_PRESS_DELAY_MS = 220;
const SNAP_DISTANCE = 32;

export function useSessionToolbox(options: SessionToolboxDragOptions) {
  const toolboxOpen = ref(false);
  const position = ref<SessionToolboxButtonPosition>({
    x: DEFAULT_INSET,
    y: DEFAULT_INSET,
    corner: "top-right",
  });
  const customPosition = ref(false);
  const workspaceRectVersion = ref(0);
  let workspaceResizeObserver: ResizeObserver | null = null;
  let dragging = false;
  let dragOffsetX = 0;
  let dragOffsetY = 0;
  let longPressTimer: ReturnType<typeof window.setTimeout> | null = null;
  let pendingDragEvent: { event: PointerEvent; button: HTMLElement } | null = null;
  let capturedPointerElement: HTMLElement | null = null;
  let capturedPointerId: number | null = null;
  let suppressNextClick = false;

  const buttonStyle = computed(() => {
    workspaceRectVersion.value;
    const workspace = options.workspaceRef.value;
    if (!workspace) {
      return {
        top: `${DEFAULT_INSET}px`,
        left: `${DEFAULT_INSET}px`,
      };
    }

    const visiblePosition = visibleToolboxPosition(workspace.getBoundingClientRect());

    return {
      top: `${visiblePosition.y}px`,
      left: `${visiblePosition.x}px`,
    };
  });

  function toggleToolbox() {
    toolboxOpen.value = !toolboxOpen.value;
  }

  function handleToolboxButtonClick() {
    if (suppressNextClick) {
      suppressNextClick = false;
      return;
    }

    toggleToolbox();
  }

  function closeToolbox() {
    toolboxOpen.value = false;
  }

  function handleWindowBlur() {
    closeToolbox();
    stopToolboxDrag();
  }

  function handleVisibilityChange() {
    if (document.visibilityState !== "visible") handleWindowBlur();
  }

  function handleDocumentInteraction(event: Event) {
    if (toolboxOpen.value && !options.isInside(event)) closeToolbox();
  }

  function observeWorkspaceResize(workspace: HTMLElement | null) {
    workspaceResizeObserver?.disconnect();
    workspaceResizeObserver = null;
    if (!workspace || typeof ResizeObserver === "undefined") {
      return;
    }

    workspaceResizeObserver = new ResizeObserver(() => {
      syncVisibleToolboxPosition();
    });
    workspaceResizeObserver.observe(workspace);
    syncVisibleToolboxPosition();
  }

  function syncVisibleToolboxPosition() {
    const workspace = options.workspaceRef.value;
    if (!workspace) {
      return;
    }

    const rect = workspace.getBoundingClientRect();
    if (customPosition.value) {
      position.value = visibleToolboxPosition(rect);
    }
    workspaceRectVersion.value += 1;
  }

  function startToolboxDrag(event: PointerEvent) {
    const button = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    if (!button) {
      return;
    }

    pendingDragEvent = { event, button };
    clearLongPressTimer();
    longPressTimer = window.setTimeout(() => {
      longPressTimer = null;
      if (pendingDragEvent) {
        beginToolboxDrag(pendingDragEvent.event, pendingDragEvent.button);
      }
    }, LONG_PRESS_DELAY_MS);
    window.addEventListener("pointermove", moveToolboxDrag);
    window.addEventListener("pointerup", stopToolboxDrag, { once: true });
    window.addEventListener("pointercancel", stopToolboxDrag, { once: true });
  }

  function beginToolboxDrag(event: PointerEvent, button: HTMLElement) {
    const workspace = options.workspaceRef.value;
    if (!workspace) {
      return;
    }

    const workspaceRect = workspace.getBoundingClientRect();
    const buttonRect = button.getBoundingClientRect();
    capturedPointerElement = button;
    capturedPointerId = event.pointerId;
    button.setPointerCapture(event.pointerId);
    dragOffsetX = event.clientX - buttonRect.left;
    dragOffsetY = event.clientY - buttonRect.top;
    position.value = snapSessionToolboxButton({
      x: buttonRect.left - workspaceRect.left,
      y: buttonRect.top - workspaceRect.top,
      workspaceWidth: workspaceRect.width,
      workspaceHeight: workspaceRect.height,
      buttonWidth: BUTTON_WIDTH,
      buttonHeight: BUTTON_HEIGHT,
      inset: DEFAULT_INSET,
      snapDistance: SNAP_DISTANCE,
    });
    customPosition.value = true;
    closeToolbox();
    dragging = true;
  }

  function moveToolboxDrag(event: PointerEvent) {
    if (!dragging) {
      if (pendingDragEvent) {
        pendingDragEvent = { ...pendingDragEvent, event };
      }
      return;
    }

    const workspace = options.workspaceRef.value;
    if (!workspace) {
      return;
    }

    const rect = workspace.getBoundingClientRect();
    position.value = snapSessionToolboxButton({
      x: event.clientX - rect.left - dragOffsetX,
      y: event.clientY - rect.top - dragOffsetY,
      workspaceWidth: rect.width,
      workspaceHeight: rect.height,
      buttonWidth: BUTTON_WIDTH,
      buttonHeight: BUTTON_HEIGHT,
      inset: DEFAULT_INSET,
      snapDistance: 0,
    });
    customPosition.value = true;
  }

  function stopToolboxDrag() {
    clearLongPressTimer();
    pendingDragEvent = null;
    if (dragging) {
      const workspace = options.workspaceRef.value;
      if (workspace) {
        position.value = visibleToolboxPosition(workspace.getBoundingClientRect());
      }
      suppressNextClick = true;
    }
    dragging = false;
    window.removeEventListener("pointermove", moveToolboxDrag);
    window.removeEventListener("pointerup", stopToolboxDrag);
    window.removeEventListener("pointercancel", stopToolboxDrag);
    releasePointerCapture();
  }

  function clearLongPressTimer() {
    if (longPressTimer) {
      window.clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  function releasePointerCapture() {
    if (capturedPointerElement && capturedPointerId !== null && capturedPointerElement.hasPointerCapture(capturedPointerId)) {
      capturedPointerElement.releasePointerCapture(capturedPointerId);
    }
    capturedPointerElement = null;
    capturedPointerId = null;
  }

  function defaultToolboxPosition(rect: DOMRect): SessionToolboxButtonPosition {
    return snapSessionToolboxButton({
      x: rect.width - BUTTON_WIDTH - DEFAULT_INSET,
      y: DEFAULT_INSET,
      workspaceWidth: rect.width,
      workspaceHeight: rect.height,
      buttonWidth: BUTTON_WIDTH,
      buttonHeight: BUTTON_HEIGHT,
      inset: DEFAULT_INSET,
      snapDistance: SNAP_DISTANCE,
    });
  }

  function visibleToolboxPosition(rect: DOMRect): SessionToolboxButtonPosition {
    const sourcePosition = customPosition.value ? position.value : defaultToolboxPosition(rect);
    const visiblePosition = resolveVisibleSessionToolboxButton({
      position: sourcePosition,
      workspaceWidth: rect.width,
      workspaceHeight: rect.height,
      buttonWidth: BUTTON_WIDTH,
      buttonHeight: BUTTON_HEIGHT,
      inset: DEFAULT_INSET,
      snapDistance: SNAP_DISTANCE,
    });

    return visiblePosition;
  }

  watch(
    () => options.workspaceRef.value,
    observeWorkspaceResize,
    { immediate: true, flush: "post" },
  );

  onMounted(() => {
    window.addEventListener("blur", handleWindowBlur);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    // Remote input cancels pointer defaults, so mouse-based clickoutside may never run.
    document.addEventListener("pointerdown", handleDocumentInteraction, { capture: true });
    document.addEventListener("focusin", handleDocumentInteraction, { capture: true });
  });

  onBeforeUnmount(() => {
    window.removeEventListener("blur", handleWindowBlur);
    document.removeEventListener("visibilitychange", handleVisibilityChange);
    document.removeEventListener("pointerdown", handleDocumentInteraction, { capture: true });
    document.removeEventListener("focusin", handleDocumentInteraction, { capture: true });
    stopToolboxDrag();
    workspaceResizeObserver?.disconnect();
    workspaceResizeObserver = null;
  });

  return {
    buttonStyle,
    closeToolbox,
    handleToolboxButtonClick,
    startToolboxDrag,
    toolboxOpen,
    toggleToolbox,
  };
}
