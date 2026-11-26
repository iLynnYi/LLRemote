import { ref, type Ref } from "vue";

export type ViewportPanController = {
  finishViewportPan: (event?: PointerEvent) => void;
  moveViewportPan: (event: PointerEvent) => void;
  panningSessionId: Ref<string | null>;
  shouldStartViewportPan: (sessionId: string, event: PointerEvent) => boolean;
  startViewportPan: (sessionId: string, event: PointerEvent) => void;
};

type ViewportPanState = {
  sessionId: string;
  pointerId: number;
  originClientX: number;
  originClientY: number;
  originScrollLeft: number;
  originScrollTop: number;
  scrollTarget: HTMLElement;
  captureTarget: HTMLElement | null;
};

export function createViewportPanController(options: {
  captureForwardedEvent: (event: Event) => void;
  getSessionViewportScaleMode: (sessionId: string) => string;
  getViewportScrollTarget: (sessionId: string) => HTMLElement | null;
}): ViewportPanController {
  const panningSessionId = ref<string | null>(null);
  let viewportPanState: ViewportPanState | null = null;

  function shouldStartViewportPan(sessionId: string, event: PointerEvent): boolean {
    return options.getSessionViewportScaleMode(sessionId) !== "fit"
      && (event.button === 1 || (event.altKey && event.button === 0));
  }

  function startViewportPan(sessionId: string, event: PointerEvent) {
    const scrollTarget = options.getViewportScrollTarget(sessionId);
    if (!scrollTarget) {
      return;
    }

    options.captureForwardedEvent(event);
    const captureTarget = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    captureTarget?.setPointerCapture?.(event.pointerId);
    viewportPanState = {
      sessionId,
      pointerId: event.pointerId,
      originClientX: event.clientX,
      originClientY: event.clientY,
      originScrollLeft: scrollTarget.scrollLeft,
      originScrollTop: scrollTarget.scrollTop,
      scrollTarget,
      captureTarget,
    };
    panningSessionId.value = sessionId;
  }

  function moveViewportPan(event: PointerEvent) {
    const state = viewportPanState;
    if (!state || state.pointerId !== event.pointerId) {
      return;
    }

    options.captureForwardedEvent(event);
    state.scrollTarget.scrollLeft = state.originScrollLeft - (event.clientX - state.originClientX);
    state.scrollTarget.scrollTop = state.originScrollTop - (event.clientY - state.originClientY);
  }

  function finishViewportPan(event?: PointerEvent) {
    const state = viewportPanState;
    if (!state || (event && state.pointerId !== event.pointerId)) {
      return;
    }

    if (event) {
      options.captureForwardedEvent(event);
    }
    state.captureTarget?.releasePointerCapture?.(state.pointerId);
    viewportPanState = null;
    panningSessionId.value = null;
  }

  return {
    finishViewportPan,
    moveViewportPan,
    panningSessionId,
    shouldStartViewportPan,
    startViewportPan,
  };
}
