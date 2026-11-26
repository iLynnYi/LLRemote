import assert from "node:assert/strict";
import test from "node:test";
import { createPortInputProps } from "../../../src/app/utils/portInput.ts";

function inputFixture(value = "5900") {
  const props = createPortInputProps();
  const input = { value };
  function fire(handler: keyof typeof props, type: string, fields: Record<string, unknown> = {}) {
    const event = new Event(type, { cancelable: true });
    Object.defineProperty(event, "currentTarget", { value: input });
    Object.assign(event, fields);
    const callback = props[handler];
    assert.equal(typeof callback, "function");
    (callback as (event: Event) => void)(event);
    return event;
  }
  fire("onFocusCapture", "focus");
  return { input, fire };
}

test("port typing accepts digits and deletion, rejecting all other characters", () => {
  const { fire } = inputFixture();
  for (const data of ["0", "9", "5901", "", null]) {
    assert.equal(fire("onBeforeinput", "beforeinput", { data }).defaultPrevented, false);
  }
  for (const data of ["a", "e", "E", "+", "-", ".", " ", "中", "５", "59a01"]) {
    assert.equal(fire("onBeforeinput", "beforeinput", { data }).defaultPrevented, true, String(data));
  }
});

test("port paste rejects mixed content rather than extracting a different port", () => {
  const { fire } = inputFixture();
  for (const text of ["5901", "65535", "1", ""]) {
    assert.equal(fire("onPaste", "paste", { clipboardData: { getData: () => text } }).defaultPrevented, false);
  }
  for (const text of ["5901abc", "1e3", "-1", "59.01", " 5901", "5901\n", "中文"]) {
    assert.equal(fire("onPaste", "paste", { clipboardData: { getData: () => text } }).defaultPrevented, true, text);
  }
});

test("port input capture restores the last accepted text before the numeric component processes it", () => {
  const { input, fire } = inputFixture();
  input.value = "5901";
  fire("onInputCapture", "input");
  input.value = "5901invalid";
  fire("onInputCapture", "input");
  assert.equal(input.value, "5901");

  input.value = "";
  fire("onInputCapture", "input");
  input.value = "wrong";
  fire("onInputCapture", "input");
  assert.equal(input.value, "");
});

test("port IME commits reject non-digits and accept digits", () => {
  const { input, fire } = inputFixture();
  input.value = "5900中";
  fire("onInputCapture", "input", { isComposing: true });
  fire("onCompositionendCapture", "compositionend");
  assert.equal(input.value, "5900");
  input.value = "5902";
  fire("onCompositionendCapture", "compositionend");
  assert.equal(input.value, "5902");
});

test("port filtering remembers spinner changes and keeps dialogs independent", () => {
  const first = inputFixture();
  const second = inputFixture("5905");
  first.input.value = "5901";
  first.fire("onBeforeinput", "beforeinput", { data: null });
  first.input.value = "invalid";
  first.fire("onInputCapture", "input");
  second.input.value = "invalid";
  second.fire("onInputCapture", "input");
  assert.equal(first.input.value, "5901");
  assert.equal(second.input.value, "5905");
});
