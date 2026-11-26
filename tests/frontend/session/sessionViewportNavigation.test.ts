import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, nextTick, ref, shallowRef } from "vue";
import { useSessionViewportNavigation } from "../../../src/features/sessions/composables/useSessionViewportNavigation.ts";

const renderer = createRenderer<object, object>({
  insert() {}, remove() {}, patchProp() {}, setText() {}, setElementText() {},
  createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  parentNode: () => null, nextSibling: () => null,
});

test("edge navigation observes captured input, refreshes released buttons and cleans up its lifetime", async (t) => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  let id = 0;
  const frames = new Map<number, FrameRequestCallback>();
  const area = {
    clientLeft: 0, clientTop: 0, clientWidth: 800, clientHeight: 600,
    scrollWidth: 1600, scrollHeight: 1200, scrollLeft: 0, scrollTop: 0,
    getBoundingClientRect: () => ({ left: 0, top: 0 }),
    contains: (element: object) => element === area,
  };
  let hit: object | null = area;
  const window = Object.assign(new EventTarget(), {
    requestAnimationFrame(callback: FrameRequestCallback) { frames.set(++id, callback); return id; },
    cancelAnimationFrame(key: number) { frames.delete(key); },
  });
  const document = Object.assign(new EventTarget(), {
    hidden: false,
    hasFocus: () => true,
    elementFromPoint: () => hit,
  });
  const element = Object.assign(new EventTarget(), { querySelector: () => area });
  Object.defineProperty(globalThis, "window", { value: window, configurable: true });
  Object.defineProperty(globalThis, "document", { value: document, configurable: true });
  const enabled = ref(true);
  const viewport = shallowRef(element as unknown as HTMLElement);
  const forwardedButtons: number[] = [];
  const app = renderer.createApp({
    setup() {
      useSessionViewportNavigation({ viewport, enabled, onScroll: event => forwardedButtons.push(event.buttons) });
      return () => null;
    },
  });
  app.mount({});
  let mounted = true;
  t.after(() => {
    if (mounted) app.unmount();
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else Reflect.deleteProperty(globalThis, "window");
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else Reflect.deleteProperty(globalThis, "document");
  });
  const pointer = (type = "pointermove", buttons = 0) => {
    document.dispatchEvent(Object.assign(new Event(type), {
      clientX: 795, clientY: 300, pointerType: "mouse", altKey: false, buttons,
    }));
  };
  const tick = (time = 0) => {
    const callbacks = [...frames.values()]; frames.clear();
    callbacks.forEach(callback => callback(time));
  };

  // Page-level remote forwarding can stop propagation before the viewport.
  // Listening on the document still sees input before those ancestor handlers.
  pointer("pointerdown", 1); tick();
  pointer("pointerup", 0); tick(16);
  assert.deepEqual(forwardedButtons, [1, 0], "a stationary mouse never retains the pressed button mask");

  for (const event of ["pointerleave", "pointercancel"]) {
    element.dispatchEvent(new Event(event));
    assert.equal(frames.size, 0);
    pointer();
    assert.equal(frames.size, 1);
  }
  window.dispatchEvent(new Event("blur"));
  assert.equal(frames.size, 0);
  pointer();
  document.hidden = true;
  document.dispatchEvent(new Event("visibilitychange"));
  assert.equal(frames.size, 0);
  pointer();
  assert.equal(frames.size, 0);
  document.hidden = false;

  pointer();
  hit = {}; // A toolbox, HUD or teleported menu is now above the canvas.
  tick();
  assert.equal(frames.size, 0);
  hit = area;
  pointer();
  enabled.value = false;
  await nextTick();
  assert.equal(frames.size, 0);
  enabled.value = true;
  pointer();

  const nextElement = Object.assign(new EventTarget(), { querySelector: () => area });
  viewport.value = nextElement as unknown as HTMLElement;
  await nextTick();
  assert.equal(frames.size, 0, "replacing a viewport cancels its old animation");
  pointer();
  element.dispatchEvent(new Event("pointerleave"));
  assert.equal(frames.size, 1, "the old viewport no longer has listeners");
  app.unmount(); mounted = false;
  assert.equal(frames.size, 0);
  pointer();
  assert.equal(frames.size, 0, "unmount removes document listeners too");
});
