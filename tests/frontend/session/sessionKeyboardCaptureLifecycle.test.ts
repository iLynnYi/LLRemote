import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, ref } from "vue";
import { useSessionKeyboardCapture } from "../../../src/features/sessions/composables/useSessionKeyboardCapture.ts";
import type { NativeKeyboardMessage } from "../../../src/session/input/sessionNativeKeyboardCapture.ts";
import { createSessionWindowInputFocus } from "../../../src/session/input/sessionWindowInputFocus.ts";
import { createClipboardCoordinator } from "../../../src/session/clipboard/clipboardCoordinator.ts";

class Element {
  readonly name: string;
  constructor(name: string) { this.name = name; }
  contains(target: unknown) { return target === this; }
  closest() { return this.name === "button" ? this : null; }
  focus() { document.activeElement = this; document.dispatchEvent(new Event("focusin")); }
}
class Document extends EventTarget {
  body = new Element("body");
  documentElement = new Element("html");
  activeElement = this.body;
  visibilityState = "visible";
  focused = false;
  hasFocus() { return this.focused; }
}
const document = new Document();
class Keyboard extends Event {
  key = "";
  constructor(type: string, init: KeyboardEventInit) { super(type); Object.assign(this, init); }
}
Object.assign(globalThis, { HTMLElement: Element, Node: Element, KeyboardEvent: Keyboard });
const renderer = createRenderer<object, object>({
  createComment: () => ({}), insert() {}, remove() {},
  parentNode: () => null, nextSibling: () => null,
  createElement: () => ({}), createText: () => ({}), setText() {}, setElementText() {}, patchProp() {},
});
const settle = () => new Promise<void>(resolve => setImmediate(resolve));

async function fixture(failFirst = false, focusState: { documentFocused?: boolean; nativeFocused?: boolean } = {}, onWindowFocusChanged?: (focused: boolean) => void) {
  document.activeElement = document.body;
  document.visibilityState = "visible";
  document.focused = focusState.documentFocused ?? false;
  const window = new EventTarget();
  const element = new Element("canvas");
  const connected = ref(true);
  const blocked = ref(false);
  let focusQuery: (() => Promise<boolean>) | null = null;
  const commands: Array<{enabled: boolean; token: string}> = [];
  const keys: string[] = [];
  let releaseCount = 0;
  let capture!: ReturnType<typeof useSessionKeyboardCapture>;
  let focus!: (focused: boolean) => void;
  let nativeFocusEvent!: (focused: boolean) => void;
  let consume!: (message: NativeKeyboardMessage) => void;
  let sequence = 0;
  const app = renderer.createApp({setup() {
    capture = useSessionKeyboardCapture({
      getContext: () => connected.value && !blocked.value ? {sessionId: "test", target: "fixture", element: element as unknown as HTMLElement} : null,
      onKey: (_id, event) => keys.push(event.key), release: () => releaseCount++,
      onUnavailable() {}, onWindowFocusChanged,
      runtime: {
        document: document as unknown as globalThis.Document, window: window as unknown as Window,
        native: true, token: () => String(++sequence),
        setCapture: async (enabled, token) => { commands.push({enabled, token}); if (enabled && failFirst) { failFirst = false; throw "keyboard_capture_install_failed"; } return enabled; },
        listenKeyboard: async callback => { consume = callback; return () => {}; },
        listenFocus: async callback => { nativeFocusEvent = callback; focus = value => { focusState.nativeFocused = value; callback(value); }; return () => {}; },
        isWindowFocused: async () => focusQuery ? focusQuery() : focusState.nativeFocused ?? true,
      },
    });
    return () => null;
  }});
  app.mount({});
  await settle();
  const pointer = () => {
    const event = new Event("pointerdown"); Object.defineProperty(event, "target", {value: element});
    document.dispatchEvent(event);
  };
  return {capture, commands, keys, connected, blocked, queryWith: (query: () => Promise<boolean>) => {focusQuery = query;}, focus, nativeFocusEvent, consume, pointer, element,
    released: () => releaseCount, dispose: () => app.unmount()};
}

test("native focused session window starts capture with body focus, without clicking twice", async () => {
  const f = await fixture();
  try { assert.equal(f.commands.filter(command => command.enabled).length, 1); }
  finally { f.dispose(); }
});

test("a background window cannot acquire capture from document focus alone", async () => {
  const f = await fixture(false, { documentFocused: true, nativeFocused: false });
  try {
    assert.equal(f.commands.filter(command => command.enabled).length, 0);
    assert.equal(f.capture.status.value, "inactive");
  } finally { f.dispose(); }
});

test("one canvas click retries failed capture before the target focus handler runs", async () => {
  const f = await fixture(true);
  try {
    document.activeElement = new Element("button"); document.dispatchEvent(new Event("focusin"));
    f.pointer();
    await Promise.resolve(); // Real browser checkpoint before the target listener.
    f.element.focus(); await settle();
    assert.equal(f.commands.filter(command => command.enabled).length, 2);
  } finally { f.dispose(); }
});

test("local controls, blur, visibility and disconnect revoke native capture", async () => {
  const f = await fixture();
  try {
    f.focus(true); await settle();
    document.activeElement = new Element("button"); document.dispatchEvent(new Event("focusin")); await settle();
    assert.equal(f.commands.at(-1)?.enabled, false);
    f.element.focus(); await settle();
    assert.equal(f.commands.at(-1)?.enabled, true);
    f.focus(false); await settle();
    assert.equal(f.commands.at(-1)?.enabled, false);
    f.focus(true); await settle();
    document.visibilityState = "hidden"; document.dispatchEvent(new Event("visibilitychange")); await settle();
    assert.equal(f.commands.at(-1)?.enabled, false);
    document.visibilityState = "visible"; document.dispatchEvent(new Event("visibilitychange")); await settle();
    f.connected.value = false; await settle();
    assert.equal(f.commands.at(-1)?.enabled, false);
    assert.ok(f.released() > 0);
  } finally { f.dispose(); }
});

test("focusout does not steal focus back before a local input receives focus", async () => {
  const f = await fixture();
  try {
    document.activeElement = document.body;
    document.dispatchEvent(new Event("focusout"));
    await settle();
    assert.equal(document.activeElement, document.body);
    assert.equal(f.commands.at(-1)?.enabled, false);
  } finally { f.dispose(); }
});

test("native events reach the active session and cannot cross disconnect ownership", async () => {
  const f = await fixture();
  try {
    const token = f.commands.find(command => command.enabled)!.token;
    f.consume({token, key:{key:"Meta",code:"MetaLeft",down:true},reason:null});
    assert.deepEqual(f.keys,["Meta"]);
    f.connected.value = false; await settle();
    f.consume({token, key:{key:"a",down:true},reason:null});
    assert.deepEqual(f.keys,["Meta"]);
  } finally { f.dispose(); }
});


test("clicking the canvas recovers native foreground focus even when the focus event was missed", async () => {
  const focusState = {documentFocused:true,nativeFocused:false};
  const f = await fixture(false, focusState);
  try {
    assert.equal(f.capture.status.value, "inactive");
    focusState.nativeFocused = true;
    f.pointer(); await settle();
    assert.equal(f.capture.status.value, "ready");
  } finally {f.dispose();}
});

test("a WebView focus-loss notification cannot permanently override native foreground ownership", async () => {
  const f = await fixture();
  try {
    f.nativeFocusEvent(false); await settle();
    assert.equal(f.capture.status.value, "ready");
    document.activeElement = new Element("button");
    document.dispatchEvent(new Event("focusin")); await settle();
    assert.equal(f.capture.status.value, "inactive");
  } finally {f.dispose();}
});

test("remote clipboard ownership follows verified window focus, including WebView blur recovery", async () => {
  const writes: string[] = [];
  const coordinator = createClipboardCoordinator({ readText: async () => "", writeText: async text => { writes.push(text); } });
  const inputFocus = createSessionWindowInputFocus({
    getMainSessionId: () => "test",
    onChange: sessionId => { void coordinator.setActiveSession(sessionId); },
  });
  inputFocus.mainFocusChanged(false);
  await coordinator.register("test", { sendText() {} }, false);
  const f = await fixture(false, {}, focused => inputFocus.mainFocusChanged(focused));
  try {
    assert.equal(await coordinator.receiveRemoteText("test", "远端复制一"), true);
    f.nativeFocusEvent(false); await settle();
    assert.equal(await coordinator.receiveRemoteText("test", "远端复制二"), true);
    f.focus(false);
    document.dispatchEvent(new Event("focusout"));
    await settle();
    assert.equal(await coordinator.receiveRemoteText("test", "后台复制"), false);
    f.focus(true); await settle();
    assert.equal(await coordinator.receiveRemoteText("test", "远端复制三"), true);
    assert.deepEqual(writes, ["远端复制一", "远端复制二", "远端复制三"]);
  } finally { f.dispose(); coordinator.dispose(); }
});

test("local editing releases shortcuts while preserving the focused window's clipboard ownership", async () => {
  const focusChanges: boolean[] = [];
  const f = await fixture(false, {}, focused => focusChanges.push(focused));
  try {
    assert.deepEqual(focusChanges, [true]);
    document.activeElement = new Element("button");
    document.dispatchEvent(new Event("focusin")); await settle();
    assert.equal(f.capture.status.value, "inactive");
    assert.deepEqual(focusChanges, [true]);
    f.element.focus(); await settle();
    assert.equal(f.capture.status.value, "ready");
    f.nativeFocusEvent(false); await settle();
    assert.deepEqual(focusChanges, [true], "spurious blur must not push the old clipboard again");
  } finally { f.dispose(); }
  assert.deepEqual(focusChanges, [true, false]);
});

test("a pointer event in a background document cannot enable capture", async () => {
  const f = await fixture(false, {documentFocused:true,nativeFocused:false});
  try {
    f.pointer(); await settle();
    assert.equal(f.commands.filter(command => command.enabled).length, 0);
  } finally {f.dispose();}
});


test("a modal revokes ownership even before its input receives DOM focus", async () => {
  const f = await fixture();
  try {
    assert.equal(f.capture.status.value, "ready");
    f.blocked.value = true; await settle();
    assert.equal(f.capture.status.value, "inactive");
    f.blocked.value = false; await settle();
    assert.equal(f.capture.status.value, "ready");
  } finally {f.dispose();}
});

test("a delayed foreground result cannot take focus back from a local control", async () => {
  const f = await fixture();
  try {
    let finish!: (focused: boolean) => void;
    f.queryWith(() => new Promise(resolve => {finish = resolve;}));
    f.pointer();
    document.dispatchEvent(new Event("focusout"));
    const button = new Element("button");
    document.activeElement = button;
    document.dispatchEvent(new Event("focusin"));
    finish(true); await settle();
    assert.equal(document.activeElement, button);
    assert.equal(f.capture.status.value, "inactive");
  } finally {f.dispose();}
});
