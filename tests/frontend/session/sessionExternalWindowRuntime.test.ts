import assert from "node:assert/strict";
import test from "node:test";
import { destroySessionExternalWindow } from "../../../src/features/sessions/composables/sessionExternalWindowRuntime.ts";

test("native destroy accepts an already absent label but retains failure when presence is known or unknown", async () => {
  for (const mode of ["destroyed", "absent", "present", "lookup-failed"] as const) {
    let lookups = 0;
    const closing = destroySessionExternalWindow({
      label: "session-test",
      destroy: async () => { if (mode !== "destroyed") throw new Error("native-detail"); },
    }, async (label) => {
      lookups += 1;
      assert.equal(label, "session-test");
      if (mode === "lookup-failed") throw new Error("lookup-detail");
      return mode === "absent" || mode === "destroyed" ? null : {};
    }, async () => () => undefined);
    if (mode === "present" || mode === "lookup-failed") {
      await assert.rejects(closing, (error: Error) => error.message === "external_window_destroy_failed");
    } else {
      await closing;
    }
    assert.equal(lookups, 1);
  }
});

test("a queued native destroy waits for Destroyed and verified registry absence", async () => {
  let onDestroyed!: () => void;
  let present = true;
  let cleaned = 0;
  let settled = false;
  let lookups = 0;
  const closing = destroySessionExternalWindow({
    label: "session-test",
    destroy: async () => undefined,
  }, async () => {
    lookups += 1;
    return present ? {} : null;
  }, async (handler) => {
    onDestroyed = handler;
    return () => { cleaned += 1; };
  }).then(() => { settled = true; });
  for (let index = 0; index < 10; index += 1) await Promise.resolve();
  assert.equal(settled, false);
  assert.equal(lookups, 1);
  assert.equal(cleaned, 0);
  present = false;
  onDestroyed();
  await closing;
  assert.equal(lookups, 2);
  assert.equal(cleaned, 1);
});

test("a missing native Destroyed event cannot leave child window cleanup pending forever", async () => {
  let cleaned = 0;
  await assert.rejects(destroySessionExternalWindow(
    { label: "session-test", destroy: async () => undefined },
    async () => ({}),
    async () => () => { cleaned += 1; },
    10,
  ), /external_window_destroy_failed/);
  assert.equal(cleaned, 1);
});

test("Destroyed cannot substitute for absence and listener failures cannot guess native closure", async () => {
  for (const mode of ["still-present", "lookup-failed", "listener-failed-present", "listener-failed-absent", "event-before-invoke"] as const) {
    let onDestroyed = () => undefined;
    let cleaned = 0;
    let destroys = 0;
    let lookups = 0;
    const closing = destroySessionExternalWindow({
      label: "session-test",
      destroy: async () => {
        destroys += 1;
        if (mode === "event-before-invoke") onDestroyed();
      },
    }, async () => {
      lookups += 1;
      if (mode === "lookup-failed") throw new Error("sensitive lookup detail");
      return mode === "event-before-invoke" || mode === "listener-failed-absent" ? null : {};
    }, async (handler) => {
      if (mode === "listener-failed-present" || mode === "listener-failed-absent") {
        throw new Error("sensitive listener detail");
      }
      onDestroyed = handler;
      return () => { cleaned += 1; };
    });
    const result = mode === "listener-failed-absent" || mode === "event-before-invoke"
      ? closing
      : assert.rejects(closing, (error: Error) => error.message === "external_window_destroy_failed");
    for (let index = 0; index < 10; index += 1) await Promise.resolve();
    if (mode === "still-present") onDestroyed();
    await result;
    assert.equal(destroys, mode.startsWith("listener-failed") ? 0 : 1);
    assert.equal(cleaned, mode.startsWith("listener-failed") ? 0 : 1);
    assert.equal(lookups, mode === "still-present" ? 2 : 1);
  }
});
