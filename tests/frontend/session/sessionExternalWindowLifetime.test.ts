import assert from "node:assert/strict";
import test from "node:test";

import { useSessionExternalWindowLifetime } from "../../../src/features/sessions/composables/useSessionExternalWindowLifetime.ts";

test("window disposal releases a late listener registration without running ready or starting another registration", async () => {
  const lifetime = useSessionExternalWindowLifetime();
  let finishRegistration!: (cleanup: () => void) => void;
  const pendingCleanup = new Promise<() => void>((resolve) => { finishRegistration = resolve; });
  let registrations = 0;
  let cleanups = 0;
  let ready = 0;

  const pending = lifetime.register(() => {
    registrations += 1;
    return pendingCleanup;
  }, () => { ready += 1; });
  lifetime.dispose();
  finishRegistration(() => { cleanups += 1; });
  await pending;
  await lifetime.register(async () => {
    registrations += 1;
    return () => { cleanups += 1; };
  }, () => { ready += 1; });
  lifetime.dispose();

  assert.deepEqual({ active: lifetime.isActive(), registrations, cleanups, ready }, {
    active: false, registrations: 1, cleanups: 1, ready: 0,
  });
});

test("an active window owns completed registrations and disposes each cleanup once", async () => {
  const lifetime = useSessionExternalWindowLifetime();
  const ready: string[] = [];
  const cleanups: string[] = [];
  await lifetime.register(async () => () => { cleanups.push("state"); }, () => { ready.push("state"); });
  await lifetime.register(async () => () => { cleanups.push("feedback"); }, () => { ready.push("feedback"); });

  assert.equal(lifetime.isActive(), true);
  assert.deepEqual(ready, ["state", "feedback"]);
  assert.deepEqual(cleanups, []);
  lifetime.dispose();
  lifetime.dispose();

  assert.equal(lifetime.isActive(), false);
  assert.deepEqual(cleanups, ["state", "feedback"]);
});
