import assert from "node:assert/strict";
import test from "node:test";
import { registerHooks } from "node:module";
import { ref } from "vue";
import { createSessionSettings } from "../../../src/session/settings/sessionSettings.ts";

const hooks = registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith("@session/")) return next(new URL(`../../../src/session/${specifier.slice(9)}.ts`, import.meta.url).href, context);
  return next(specifier, context);
} });
const { useSessionFullscreen } = await import("../../../src/features/sessions/composables/useSessionFullscreen.ts");
hooks.deregister();

test("fullscreen activates the chosen session and opens its existing external-window path", async () => {
  const calls: unknown[] = [];
  const fullscreen = useSessionFullscreen({
    activateSessionTab: async id => { calls.push(["activate", id]); },
    openExternalWindowSession: async (id, enabled) => { calls.push(["external", id, enabled]); },
    sessionSettings: ref(createSessionSettings()),
  });
  await fullscreen.toggleSessionFullscreen("one");
  assert.deepEqual(calls, [["activate", "one"], ["external", "one", true]]);
  assert.equal(fullscreen.fullscreenSessionId.value, null);
  assert.equal(fullscreen.fullscreenExitShortcutListening.value, false);
});

test("a failed external fullscreen request cannot claim a main-window fullscreen state", async () => {
  const fullscreen = useSessionFullscreen({
    activateSessionTab: async () => {},
    openExternalWindowSession: async () => { throw new Error("window-failed"); },
    sessionSettings: ref(createSessionSettings()),
  });
  await assert.rejects(fullscreen.enterSessionFullscreen("one"), /window-failed/);
  assert.equal(fullscreen.fullscreenSessionId.value, null);
  assert.equal(fullscreen.fullscreenExitShortcutListening.value, false);
});
