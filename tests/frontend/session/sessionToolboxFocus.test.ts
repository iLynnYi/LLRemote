import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, ref } from "vue";
import { useSessionToolbox } from "../../../src/features/sessions/composables/useSessionToolbox.ts";

const renderer = createRenderer<object, object>({
  insert() {}, remove() {}, patchProp() {}, setText() {}, setElementText() {},
  createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  parentNode: () => null, nextSibling: () => null,
});

test("toolbox closes on window blur or hiding and removes its listeners on unmount", (t) => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const window = new EventTarget();
  const document = Object.assign(new EventTarget(), { visibilityState: "visible" });
  let unmount = () => {};
  Object.defineProperty(globalThis, "window", { value: window, configurable: true });
  Object.defineProperty(globalThis, "document", { value: document, configurable: true });
  t.after(() => {
    unmount();
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else Reflect.deleteProperty(globalThis, "window");
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else Reflect.deleteProperty(globalThis, "document");
  });
  let toolbox!: ReturnType<typeof useSessionToolbox>;
  const insideEvents = new WeakSet<Event>();
  const app = renderer.createApp({
    setup() {
      toolbox = useSessionToolbox({
        workspaceRef: ref(null),
        isInside: (event) => insideEvents.has(event),
      });
      return () => null;
    },
  });
  app.mount({});
  let mounted = true;
  unmount = () => {
    if (mounted) {
      app.unmount();
      mounted = false;
    }
  };

  toolbox.toggleToolbox();
  window.dispatchEvent(new Event("blur"));
  assert.equal(toolbox.toolboxOpen.value, false);
  window.dispatchEvent(new Event("focus"));
  assert.equal(toolbox.toolboxOpen.value, false, "returning does not reopen the menu");

  toolbox.toggleToolbox();
  document.dispatchEvent(new Event("visibilitychange"));
  assert.equal(toolbox.toolboxOpen.value, true, "visible documents keep their menu open");
  document.visibilityState = "hidden";
  document.dispatchEvent(new Event("visibilitychange"));
  assert.equal(toolbox.toolboxOpen.value, false);

  toolbox.toggleToolbox();
  for (const type of ["pointerdown", "focusin"]) {
    const inside = new Event(type);
    insideEvents.add(inside);
    document.dispatchEvent(inside);
    assert.equal(toolbox.toolboxOpen.value, true, "trigger and teleported submenus stay open");
  }
  const remotePointer = new Event("pointerdown", { cancelable: true });
  remotePointer.preventDefault();
  document.dispatchEvent(remotePointer);
  assert.equal(toolbox.toolboxOpen.value, false, "remote input closes the menu even without mouse events");
  toolbox.toggleToolbox();
  document.dispatchEvent(new Event("focusin"));
  assert.equal(toolbox.toolboxOpen.value, false, "keyboard focus outside closes the menu");

  unmount();
  toolbox.toggleToolbox();
  window.dispatchEvent(new Event("blur"));
  document.dispatchEvent(new Event("visibilitychange"));
  document.dispatchEvent(new Event("pointerdown"));
  document.dispatchEvent(new Event("focusin"));
  assert.equal(toolbox.toolboxOpen.value, true, "unmounted toolboxes no longer listen to focus events");
});
