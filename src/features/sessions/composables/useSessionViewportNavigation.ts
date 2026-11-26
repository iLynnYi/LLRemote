import { onBeforeUnmount, watch, type Ref } from "vue";
import { createViewportEdgeScrollController } from "../../../session/viewport/viewportScroll.ts";

/** Bind the same edge navigation to main, fullscreen and independent session viewports. */
export function useSessionViewportNavigation(options: {
  viewport: Ref<HTMLElement | null>;
  enabled: Ref<boolean>;
  onScroll: (event: PointerEvent) => void;
}) {
  let lastPointer: PointerEvent | null = null;
  let unbind = () => {};
  const capture = { capture: true };
  const controller = createViewportEdgeScrollController({
    getTarget: () => options.viewport.value?.querySelector<HTMLElement>(".session-viewport-scroll") ?? null,
    canScroll: (point) => {
      if (!options.enabled.value || document.hidden || !document.hasFocus()) return false;
      const target = options.viewport.value?.querySelector<HTMLElement>(".session-viewport-scroll");
      const hit = document.elementFromPoint(point.clientX, point.clientY);
      return Boolean(target && hit && target.contains(hit));
    },
    requestFrame: callback => window.requestAnimationFrame(callback),
    cancelFrame: id => window.cancelAnimationFrame(id),
    // Keep the remote pointer under the stationary local mouse as the canvas moves.
    onScroll: () => { if (lastPointer) options.onScroll(lastPointer); },
  });
  function stop() {
    controller.stop();
    lastPointer = null;
  }
  function move(event: PointerEvent) {
    lastPointer = event;
    controller.move(event);
  }
  watch(options.viewport, (viewport) => {
    unbind();
    stop();
    if (!viewport) return;
    // External windows capture and stop remote input on the page ancestor.
    // Observe it first so local edge navigation also runs in that window.
    document.addEventListener("pointermove", move, capture);
    document.addEventListener("pointerdown", move, capture);
    document.addEventListener("pointerup", move, capture);
    viewport.addEventListener("pointerleave", stop);
    viewport.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
    document.addEventListener("visibilitychange", stop);
    unbind = () => {
      document.removeEventListener("pointermove", move, capture);
      document.removeEventListener("pointerdown", move, capture);
      document.removeEventListener("pointerup", move, capture);
      viewport.removeEventListener("pointerleave", stop);
      viewport.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      document.removeEventListener("visibilitychange", stop);
    };
  }, { immediate: true, flush: "post" });
  watch(options.enabled, stop);
  onBeforeUnmount(() => { unbind(); stop(); });
}
