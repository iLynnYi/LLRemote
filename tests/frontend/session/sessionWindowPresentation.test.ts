import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  nativeSessionWindowOptions,
  isSessionWindowExpanded,
  setSessionWindowExpanded,
} from "../../../src/session/external-window/sessionWindowPresentation.ts";

test("session expansion uses the available work area with native controls on both platforms", () => {
  assert.deepEqual(nativeSessionWindowOptions(true), { fullscreen: false, maximized: true });
  assert.deepEqual(nativeSessionWindowOptions(false), { fullscreen: false, maximized: false });
});

test("macOS uses work-area maximization instead of creating a fullscreen Space", async () => {
  const calls: string[] = [];
  const window = {
    isFullscreen: async () => false,
    isMaximized: async () => true,
    setFullscreen: async () => { throw new Error("wrong state change"); },
    maximize: async () => { calls.push("maximize"); },
    unmaximize: async () => { calls.push("unmaximize"); },
  };
  assert.equal(await isSessionWindowExpanded(window), true);
  await setSessionWindowExpanded(window, false);
  await setSessionWindowExpanded(window, true);
  assert.deepEqual(calls, ["unmaximize", "maximize"]);
});

test("Windows child window toggle uses the same expanded state as direct opening", async () => {
  const calls: string[] = [];
  const window = {
    isMaximized: async () => true,
    isFullscreen: async () => false,
    maximize: async () => { calls.push("maximize"); },
    unmaximize: async () => { calls.push("unmaximize"); },
    setFullscreen: async () => { throw new Error("wrong state change"); },
  };
  assert.equal(await isSessionWindowExpanded(window), true);
  await setSessionWindowExpanded(window, false);
  await setSessionWindowExpanded(window, true);
  assert.deepEqual(calls, ["unmaximize", "maximize"]);
});

test("session window capability permits the state changes used by both paths", () => {
  const capability = JSON.parse(readFileSync(new URL("../../../src-tauri/capabilities/default.json", import.meta.url), "utf8"));
  for (const permission of ["core:window:allow-maximize", "core:window:allow-unmaximize", "core:window:allow-set-fullscreen"]) {
    assert.ok(capability.permissions.includes(permission), `${permission} is required`);
  }
});

test("a window in OS fullscreen can return to work-area presentation", async () => {
  const calls: string[] = [];
  const window = {
    isFullscreen: async () => true,
    isMaximized: async () => false,
    setFullscreen: async (value: boolean) => { calls.push(`fullscreen:${value}`); },
    maximize: async () => { calls.push("maximize"); },
    unmaximize: async () => { calls.push("unmaximize"); },
  };
  assert.equal(await isSessionWindowExpanded(window), true);
  await setSessionWindowExpanded(window, true);
  assert.deepEqual(calls, ["fullscreen:false", "maximize"]);
});
