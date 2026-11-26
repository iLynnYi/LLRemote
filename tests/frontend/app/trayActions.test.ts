import assert from "node:assert/strict";
import test, { type TestContext } from "node:test";
import { createTrayStateSnapshot } from "../../../src/app/trayState.ts";
import { createTrayActions } from "../../../src/app/composables/vncAppState/tray.ts";

function fixture(t?: TestContext) {
  const snapshot = () => createTrayStateSnapshot({
    localVncServerOverview: null,
    activeSessionCount: 0,
    failedSessionCount: 0,
    canStartLocalShare: false,
  });
  const trayStateSnapshot = { value: snapshot() };
  const calls: unknown[] = [];
  const errors: string[] = [];
  const notifications: string[] = [];
  const opened: string[] = [];
  let start = async () => {};
  if (t) {
    const original = Object.getOwnPropertyDescriptor(globalThis, "Notification");
    Object.defineProperty(globalThis, "Notification", {
      configurable: true,
      value: class {
        static permission = "granted";
        constructor(title: string) { notifications.push(title); }
      },
    });
    t.after(() => {
      if (original) Object.defineProperty(globalThis, "Notification", original);
      else Reflect.deleteProperty(globalThis, "Notification");
    });
  }
  let menuOpen = false;
  let invoke = async (args: Record<string, unknown>) => {
    calls.push(args.snapshot);
    // Replacing the native macOS menu cancels its current tracking session.
    menuOpen = false;
  };
  const actions = createTrayActions({
    invoke: (_command, args) => invoke(args),
    isTauriUnavailableError: () => false,
    localVncServerMessage: { value: "" },
    logEvent: (_level, _source, message) => { errors.push(message); },
    refreshLocalVncServerStatus: async () => {},
    selectLocalShareWorkspace: async () => { opened.push("local-share"); },
    startLocalVncServer: () => start(),
    t: (key: string) => key,
    trayStateSnapshot,
  });
  return {
    actions, calls, errors, notifications, opened, snapshot, trayStateSnapshot,
    openMenu: () => { menuOpen = true; },
    isMenuOpen: () => menuOpen,
    setInvoke: (next: typeof invoke) => { invoke = next; },
    setStart: (next: typeof start) => { start = next; },
  };
}

test("unchanged polling snapshots leave an open tray menu untouched", async () => {
  const f = fixture();
  await f.actions.syncTrayState();
  f.openMenu();
  for (let poll = 0; poll < 3; poll++) {
    f.trayStateSnapshot.value = f.snapshot();
    await f.actions.syncTrayState();
  }
  assert.equal(f.isMenuOpen(), true, "status polling must not replace an open native menu");
  assert.equal(f.calls.length, 1);
});

test("tray startup opens configuration without reporting failure when input is incomplete", async t => {
  const f = fixture(t);
  f.setStart(async () => { assert.fail("incomplete configuration must open the page before starting"); });
  await f.actions.startLocalShareFromTray();
  assert.deepEqual(f.opened, ["local-share"]);
  assert.deepEqual(f.notifications, []);
});

for (const action of ["awaiting_permission", "awaiting_password"] as const) {
  test(`tray startup opens the ${action} guide without a failure notification`, async t => {
    const f = fixture(t);
    f.trayStateSnapshot.value = { ...f.snapshot(), canStartLocalShare: true };
    f.setStart(async () => {
      f.trayStateSnapshot.value = { ...f.snapshot(), localShareAction: action };
    });
    await f.actions.startLocalShareFromTray();
    assert.deepEqual(f.opened, ["local-share"]);
    assert.deepEqual(f.notifications, []);
    f.setStart(async () => { assert.fail("a pending guide must not create another startup intent"); });
    await f.actions.startLocalShareFromTray();
    assert.deepEqual(f.opened, ["local-share", "local-share"]);
  });
}

test("tray startup ignores duplicate clicks while the first attempt is pending", async t => {
  const f = fixture(t);
  f.trayStateSnapshot.value = { ...f.snapshot(), canStartLocalShare: true };
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  let starts = 0;
  f.setStart(async () => {
    starts++;
    await pending;
    f.trayStateSnapshot.value = { ...f.snapshot(), localShareStatus: "starting", localShareAction: "starting" };
  });
  const first = f.actions.startLocalShareFromTray();
  const duplicate = f.actions.startLocalShareFromTray();
  const simultaneousStarts = starts;
  release();
  await Promise.all([first, duplicate]);
  assert.equal(simultaneousStarts, 1);
  await f.actions.startLocalShareFromTray();
  assert.equal(starts, 1);
  assert.deepEqual(f.notifications, []);
});

test("tray startup reports a completed failure and leaves successful startup quiet", async t => {
  const f = fixture(t);
  f.trayStateSnapshot.value = { ...f.snapshot(), canStartLocalShare: true };
  f.setStart(async () => {
    f.trayStateSnapshot.value = { ...f.snapshot(), localShareStatus: "failed" };
  });
  await f.actions.startLocalShareFromTray();
  assert.deepEqual(f.notifications, ["本机共享启动失败"]);
  assert.deepEqual(f.opened, ["local-share"]);
  f.trayStateSnapshot.value = { ...f.snapshot(), canStartLocalShare: true };
  f.setStart(async () => {
    f.trayStateSnapshot.value = { ...f.snapshot(), localShareStatus: "running", localShareRunning: true };
  });
  await f.actions.startLocalShareFromTray();
  assert.equal(f.notifications.length, 1);
  assert.equal(f.opened.length, 1);
});

test("changed tray state is synced and failed updates can be retried", async () => {
  const f = fixture();
  let attempts = 0;
  f.setInvoke(async args => {
    attempts++;
    if (attempts === 1) throw new Error("tray_not_ready");
    f.calls.push(args.snapshot);
  });
  await f.actions.syncTrayState();
  await f.actions.syncTrayState();
  await f.actions.syncTrayState();
  assert.equal(attempts, 2);
  assert.equal(f.errors.length, 1);

  f.trayStateSnapshot.value = { ...f.snapshot(), activeSessionCount: 1 };
  await f.actions.syncTrayState();
  assert.equal(attempts, 3);
  assert.deepEqual(f.calls.at(-1), f.trayStateSnapshot.value);
});

test("overlapping polls are serialized and eventually sync the newest state", async () => {
  const f = fixture();
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  let inFlight = 0;
  let peakInFlight = 0;
  f.setInvoke(async args => {
    inFlight++;
    peakInFlight = Math.max(peakInFlight, inFlight);
    f.calls.push(args.snapshot);
    if (f.calls.length === 1) await pending;
    inFlight--;
  });
  const first = f.actions.syncTrayState();
  await new Promise<void>(resolve => setImmediate(resolve));
  const duplicate = f.actions.syncTrayState();
  f.trayStateSnapshot.value = { ...f.snapshot(), activeSessionCount: 1 };
  const changed = f.actions.syncTrayState();
  release();
  await Promise.all([first, duplicate, changed]);
  assert.equal(peakInFlight, 1);
  assert.equal(f.calls.length, 2);
  assert.deepEqual(f.calls.at(-1), f.trayStateSnapshot.value);
});
