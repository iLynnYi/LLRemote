import assert from "node:assert/strict";
import test from "node:test";
import { useLocalShareStartFlow } from "../../../src/features/local-share/composables/useLocalShareStartFlow.ts";
import type { BackgroundShareOverview, BackgroundShareStartRequest } from "../../../src/server/backgroundShare.ts";
import type { LocalVncServerPermissionKind } from "../../../src/server/localShare.ts";

function fixture(screen = false, input = false) {
  const overview: BackgroundShareOverview = {
    server: {
      status: "stopped", bindAddress: "0.0.0.0", port: 5903, addresses: [], displays: [],
      selectedDisplayId: null, selectedDisplayLabel: null, resolutionMode: "fit", refreshMode: "balanced",
      framebufferWidth: 0, framebufferHeight: 0, securityType: "vnc_auth", viewOnly: false,
      activeConnections: 0, activeConnectionDevices: [],
      permissions: [
        { kind: "screen_recording", status: screen ? "granted" : "missing", requiredFor: "screen_capture" },
        { kind: "accessibility", status: input ? "granted" : "missing", requiredFor: "input_injection" },
      ],
      agent: { status: "stopped", bindAddress: "0.0.0.0", port: null, capabilities: [], lastError: null, updatedAt: "0" },
      startedAt: null, updatedAt: "0",
    },
    configured: true, credentialConfigured: true, autoStart: false, processRunning: true, lastErrorCode: null,
    runtime: { pid: 123, buildId: "older-compatible-build" }, currentBuildId: "current", permissionSource: "background",
    restartSupported: true, ipcVersion: 6,
  };
  const calls: string[] = [];
  const startedPorts: number[] = [];
  const errors: unknown[] = [];
  let clock = 0;
  let busy = false;
  let notices = 0;
  let grants = 0;
  let permissionFailure: string | null = null;
  let pendingPermission: Promise<void> | null = null;
  let preparation: Promise<BackgroundShareOverview> | null = null;
  let restartFailure: Error | null = null;
  let startFailure: string | null = null;
  let temporaryStartFailure: string | null = null;
  let pendingStart: Promise<void> | null = null;
  const sentRequests: BackgroundShareStartRequest[] = [];
  const sentPasswords: string[] = [];
  const startModes: boolean[] = [];
  let expire: (() => void) | null = null;
  const expiryDelays: number[] = [];
  let statusFailure: string | null = null;
  const readStatus = async () => {
    if (statusFailure) throw new Error(statusFailure);
    return structuredClone(overview);
  };
  const flow = useLocalShareStartFlow({
    api: {
      restart: async () => {
        calls.push("restart");
        if (restartFailure) throw restartFailure;
        if (preparation) return preparation;
        overview.server.status = "stopped";
        overview.server.activeConnections = 0;
        return readStatus();
      },
      prepare: async recover => { calls.push(recover ? "recover" : "prepare"); return preparation ? await preparation : readStatus(); },
      permission: async (kind, openSettings) => {
        calls.push(openSettings ? `settings_${kind}` : `authorize_${kind}`);
        if (pendingPermission) await pendingPermission;
        if (permissionFailure) throw new Error(permissionFailure);
        return { opened: true, kind, platform: "macos", target: "prompt", message: "" };
      },
      start: async request => {
        calls.push("start"); startedPorts.push(request.port);
        sentRequests.push(request);
        sentPasswords.push(request.password);
        startModes.push(request.temporaryPassword === true);
        if (pendingStart) await pendingStart;
        const failure = request.temporaryPassword ? temporaryStartFailure : startFailure;
        if (failure) throw new Error(failure);
        overview.server.status = "running";
        return readStatus();
      },
    },
    readStatus,
    acceptOverview: () => {},
    setBusy: value => { busy = value; },
    onStarted: () => { notices++; },
    onPermissionGranted: () => { grants++; },
    onError: error => errors.push(error),
    now: () => clock,
    scheduleExpiry: (callback, delay) => { expire = callback; expiryDelays.push(delay); return () => { expire = null; }; },
  });
  const request = (viewOnly = false): BackgroundShareStartRequest => ({ bindAddress: "0.0.0.0", port: 5903, password: "", viewOnly });
  const grant = (kind: LocalVncServerPermissionKind) => { overview.server.permissions.find(permission => permission.kind === kind)!.status = "granted"; };
  return { flow, overview, calls, errors, request, grant, startedPorts, busy: () => busy, notices: () => notices,
    grants: () => grants,
    holdPermission: (value: Promise<void>) => { pendingPermission = value; },
    failPermission: (code: string) => { permissionFailure = code; },
    failStatus: (code: string) => { statusFailure = code; },
    sentRequests, sentPasswords, startModes, failStart: (code: string | null) => { startFailure = code; },
    failTemporaryStart: (code: string) => { temporaryStartFailure = code; },
    holdStart: (operation: Promise<void>) => { pendingStart = operation; },
    expiryDelays, expire: () => expire?.(), expiryPending: () => expire !== null,
    clock: (value: number) => { clock = value; }, hold: (value: Promise<BackgroundShareOverview>) => { preparation = value; },
    failRestart: () => { restartFailure = new Error("background_share_restart_timeout"); } };
}

test("denied saved credentials wait for a password and resume the original startup snapshot", async () => {
  const f = fixture(true, true);
  const request = f.request();
  request.port = 5910;
  f.failStart("background_share_credential_unavailable");
  await f.flow.begin(request);
  assert.equal(f.flow.phase.value, "awaiting_password");
  assert.equal(f.busy(), false);
  assert.deepEqual(f.errors, []);
  assert.equal(f.expiryPending(), true);
  assert.ok(f.sentRequests.every(value => value.password === ""));
  const temporary = crypto.randomUUID().slice(0, 8);
  await f.flow.submitTemporaryPassword(temporary);
  assert.deepEqual(f.startModes, [false, true]);
  assert.deepEqual(f.startedPorts, [5910, 5910]);
  assert.deepEqual(f.sentPasswords, ["", temporary]);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.notices(), 1);
  assert.ok(f.sentRequests.every(value => value.password === ""));
  assert.equal(request.password, "");
});

test("a denied credential read or save retries once with the supplied password", async () => {
  for (const code of ["background_share_credential_unavailable", "background_share_initial_credential_rollback_failed"]) {
    const f = fixture(true, true);
    const password = crypto.randomUUID().slice(0, 8);
    const request = { ...f.request(), password };
    f.failStart(code);
    await f.flow.begin(request);
    assert.equal(f.flow.phase.value, "idle");
    assert.deepEqual(f.errors, []);
    assert.deepEqual(f.startModes, [false, true]);
    assert.deepEqual(f.sentPasswords, [password, password]);
    assert.ok(f.sentRequests.every(value => value.password === ""));
    assert.equal(request.password, "");
    assert.equal(f.notices(), 1);
  }
});

test("cancel, expiry and disposal discard the temporary startup intent", async () => {
  for (const finish of ["cancel", "expire", "dispose"] as const) {
    const f = fixture(true, true);
    f.failStart("background_share_credential_unavailable");
    await f.flow.begin(f.request());
    assert.equal(f.flow.phase.value, "awaiting_password");
    if (finish === "cancel") f.flow.cancel();
    else if (finish === "expire") f.expire();
    else f.flow.dispose();
    await f.flow.submitTemporaryPassword(crypto.randomUUID().slice(0, 8));
    assert.equal(f.flow.phase.value, "idle");
    assert.equal(f.calls.filter(value => value === "start").length, 1);
    assert.ok(f.sentRequests.every(value => value.password === ""));
  }
});

test("temporary startup retries on the current host without requesting a restart", async () => {
  const f = fixture(true, true);
  f.failStart("background_share_credential_unavailable");
  await f.flow.begin(f.request());
  await f.flow.submitTemporaryPassword(crypto.randomUUID().slice(0, 8));
  assert.deepEqual(f.calls, ["prepare", "start", "start"]);
  assert.deepEqual(f.startModes, [false, true]);
  assert.equal(f.flow.phase.value, "idle");
  assert.ok(f.sentRequests.every(value => value.password === ""));
});

test("leaving while the system credential prompt is open prevents an automatic temporary retry", async () => {
  const f = fixture(true, true);
  let release!: () => void;
  f.holdStart(new Promise(resolve => { release = resolve; }));
  f.failStart("background_share_credential_unavailable");
  const operation = f.flow.begin({ ...f.request(), password: "testpass" });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.flow.phase.value, "starting");
  f.flow.cancel();
  release();
  await operation;
  assert.equal(f.flow.phase.value, "idle");
  assert.deepEqual(f.startModes, [false]);
  assert.equal(f.expiryPending(), false);
  assert.ok(f.sentRequests.every(value => value.password === ""));
});

test("a protocol error from the backend prevents a temporary start request", async () => {
  const f = fixture(true, true);
  f.failStart("background_share_credential_unavailable");
  await f.flow.begin(f.request());
  f.failStatus("background_share_protocol_incompatible");
  await f.flow.submitTemporaryPassword(crypto.randomUUID().slice(0, 8));
  assert.deepEqual(f.startModes, [false]);
  assert.equal(f.flow.phase.value, "idle");
  assert.match(String(f.errors[0]), /protocol_incompatible/);
});

test("temporary password validation, failures and other startup errors never loop credential prompts", async () => {
  const f = fixture(true, true);
  f.failStart("background_share_credential_unavailable");
  await f.flow.begin(f.request());
  await f.flow.submitTemporaryPassword("");
  await f.flow.submitTemporaryPassword("远程密码");
  assert.equal(f.flow.phase.value, "awaiting_password");
  assert.equal(f.calls.filter(value => value === "start").length, 1);
  f.failTemporaryStart("background_share_credential_unavailable");
  await f.flow.submitTemporaryPassword(crypto.randomUUID().slice(0, 8));
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.errors.length, 1);
  assert.ok(f.sentRequests.every(value => value.password === ""));
  for (const code of ["permission_missing", "port_conflict", "background_share_config_unavailable", "background_share_credential_rollback_failed"]) {
    const other = fixture(true, true);
    other.failStart(code);
    await other.flow.begin(other.request());
    assert.equal(other.flow.phase.value, "idle");
    assert.equal(other.errors.length, 1);
  }
});

test("maintenance replaces an active host then guides only missing grants without starting sharing", async () => {
  const f = fixture(true, false);
  f.overview.server.status = "running";
  f.overview.server.activeConnections = 2;
  await f.flow.beginRepair();
  assert.deepEqual(f.calls, ["restart"]);
  assert.equal(f.overview.server.activeConnections, 0);
  assert.equal(f.flow.purpose.value, "repair");
  assert.deepEqual(f.flow.missingPermissions.value, ["accessibility"]);
  await f.flow.requestPermission();
  assert.deepEqual(f.calls, ["restart", "authorize_accessibility"]);
  f.grant("accessibility");
  await f.flow.resume(true);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.grants(), 1);
  assert.equal(f.overview.server.status, "stopped");
});

test("maintenance with healthy grants only updates and checks the host", async () => {
  const f = fixture(true, true);
  await f.flow.beginRepair();
  assert.deepEqual(f.calls, ["restart"]);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.grants(), 1);
});

test("a failed maintenance stop never prompts, repairs or restarts sharing", async () => {
  const f = fixture();
  f.failRestart();
  await f.flow.beginRepair();
  assert.deepEqual(f.calls, ["restart"]);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.busy(), false);
  assert.match(String(f.errors[0]), /restart_timeout/);
});

test("leaving during maintenance prevents late restart results from opening a permission guide", async () => {
  const f = fixture();
  let release!: (overview: BackgroundShareOverview) => void;
  f.hold(new Promise(resolve => { release = resolve; }));
  const operation = f.flow.beginRepair();
  assert.equal(f.flow.phase.value, "updating");
  f.flow.cancel();
  assert.equal(f.busy(), true);
  release(f.overview);
  await operation;
  assert.deepEqual(f.calls, ["restart"]);
  assert.equal(f.flow.phase.value, "idle");
  assert.deepEqual(f.flow.missingPermissions.value, []);
  assert.equal(f.busy(), false);
});

test("compatible builds with granted permissions start without prompting or restarting", async () => {
  const f = fixture(true, true);
  await f.flow.begin(f.request());
  assert.deepEqual(f.calls, ["prepare", "start"]);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.busy(), false);
});

test("one startup intent guides screen then input permissions and resumes automatically", async () => {
  const f = fixture();
  await f.flow.begin(f.request());
  assert.equal(f.flow.phase.value, "awaiting_permission");
  assert.deepEqual(f.calls, ["prepare"]); // Guide is visible before opening system settings.
  await f.flow.requestPermission();
  assert.deepEqual(f.calls, ["prepare", "authorize_screen_recording"]);
  await f.flow.resume();
  assert.equal(f.calls.filter(call => call === "authorize_screen_recording").length, 1);
  f.grant("screen_recording");
  await f.flow.resume(true);
  assert.deepEqual(f.flow.missingPermissions.value, ["accessibility"]);
  assert.deepEqual(f.calls, ["prepare", "authorize_screen_recording"]);
  await f.flow.requestPermission();
  assert.deepEqual(f.calls, ["prepare", "authorize_screen_recording", "authorize_accessibility"]);
  f.grant("accessibility");
  await f.flow.resume(true);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.calls.filter(call => call === "start").length, 1);
  assert.equal(f.notices(), 1);
});

test("view-only sharing does not require Accessibility", async () => {
  const f = fixture(true, false);
  await f.flow.begin(f.request(true));
  assert.deepEqual(f.calls, ["prepare", "start"]);
});

test("returning with stale grants performs only one idle recovery and never loops prompts", async () => {
  const f = fixture();
  await f.flow.begin(f.request());
  await f.flow.requestPermission();
  await f.flow.resume(true);
  await f.flow.resume(true);
  await f.flow.resume();
  assert.equal(f.calls.filter(call => call === "recover").length, 1);
  assert.equal(f.calls.filter(call => call === "authorize_screen_recording").length, 1);
  assert.ok(!f.calls.includes("start"));
  f.flow.cancel();
});

test("manual permission guide checks and prompts only its selected permission without starting sharing", async () => {
  const f = fixture();
  await f.flow.beginPermission("accessibility");
  assert.equal(f.flow.purpose.value, "permission");
  assert.deepEqual(f.flow.requiredPermissions.value, ["accessibility"]);
  assert.deepEqual(f.calls, ["prepare"]);
  await f.flow.requestPermission();
  f.grant("accessibility");
  await f.flow.resume(true);
  assert.deepEqual(f.calls, ["prepare", "authorize_accessibility"]);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.grants(), 1);
  assert.equal(f.notices(), 0);
});

test("opening settings is an explicit action separate from requesting the system prompt", async () => {
  const f = fixture();
  await f.flow.beginPermission("accessibility");
  assert.deepEqual(f.calls, ["prepare"]);
  await f.flow.requestPermission(true);
  assert.deepEqual(f.calls, ["prepare", "settings_accessibility"]);
  assert.equal(f.flow.phase.value, "awaiting_permission");
  await f.flow.requestPermission();
  assert.deepEqual(f.calls, ["prepare", "settings_accessibility", "authorize_accessibility"]);
  f.flow.cancel();
});

test("manual guidance never stops an existing listener to refresh a missing grant", async () => {
  const f = fixture();
  f.overview.server.status = "running";
  await f.flow.beginPermission("accessibility");
  await f.flow.requestPermission();
  await f.flow.resume(true);
  assert.equal(f.flow.phase.value, "awaiting_permission");
  assert.ok(!f.calls.includes("recover"));
  assert.ok(!f.calls.includes("start"));
  f.flow.cancel();
});

test("a listener started elsewhere is accepted without restart or duplicate start", async () => {
  const f = fixture();
  await f.flow.begin(f.request());
  f.overview.server.status = "running";
  await f.flow.resume(true);
  assert.ok(!f.calls.includes("recover"));
  assert.ok(!f.calls.includes("start"));
  assert.equal(f.flow.phase.value, "idle");
});

test("cancellation prevents a late preparation from reviving startup", async () => {
  const f = fixture(true, true);
  let release!: (overview: BackgroundShareOverview) => void;
  f.hold(new Promise(resolve => { release = resolve; }));
  const request = f.request();
  const begin = f.flow.begin(request);
  assert.equal(f.flow.cancel(), true);
  release(f.overview);
  await begin;
  assert.deepEqual(f.calls, ["prepare"]);
  assert.equal(request.password, "");
  assert.equal(f.flow.phase.value, "idle");
});

test("retrying before a cancelled preparation completes does not leave the form busy", async () => {
  const f = fixture(true, true);
  let release!: (overview: BackgroundShareOverview) => void;
  f.hold(new Promise(resolve => { release = resolve; }));
  const begin = f.flow.begin(f.request());
  f.flow.cancel();
  await f.flow.begin(f.request());
  assert.equal(f.busy(), false);
  assert.equal(f.flow.phase.value, "idle");
  release(f.overview);
  await begin;
  assert.deepEqual(f.calls, ["prepare"]);
});

test("startup expires after two minutes and late permission grants cannot revive it", async () => {
  const f = fixture();
  const request = f.request();
  await f.flow.begin(request);
  f.clock(120_001);
  f.grant("screen_recording");
  f.grant("accessibility");
  await f.flow.resume(true);
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(request.password, "");
  assert.match(String(f.errors[0]), /permission_timeout/);
  assert.ok(!f.calls.includes("start"));
});

test("the one-shot deadline expires a wait without any polling or return event", async () => {
  const f = fixture();
  await f.flow.begin(f.request());
  assert.deepEqual(f.expiryDelays, [120_000]);
  assert.equal(f.expiryPending(), true);
  f.expire();
  assert.equal(f.flow.phase.value, "idle");
  assert.equal(f.expiryPending(), false);
  assert.deepEqual(f.calls, ["prepare"]);
  assert.match(String(f.errors[0]), /permission_timeout/);
});

test("cancellation clears the deadline and cannot affect a later attempt", async () => {
  const f = fixture();
  await f.flow.begin(f.request());
  f.flow.cancel();
  assert.equal(f.expiryPending(), false);
  f.expire();
  assert.equal(f.errors.length, 0);
  f.grant("screen_recording");
  f.grant("accessibility");
  await f.flow.begin(f.request());
  assert.equal(f.expiryPending(), false);
  assert.equal(f.notices(), 1);
});

test("disposing a pending intent suppresses late UI updates and startup", async () => {
  const f = fixture(true, true);
  let release!: (overview: BackgroundShareOverview) => void;
  f.hold(new Promise(resolve => { release = resolve; }));
  const begin = f.flow.begin(f.request());
  f.flow.dispose();
  release(f.overview);
  await begin;
  assert.deepEqual(f.calls, ["prepare"]);
  assert.equal(f.notices(), 0);
});


test("signature changes use the same authorization while entry, rechecks and opening settings never reset", async () => {
  const f = fixture(true, false);
  f.overview.signatureChangedPermissions = ["screen_recording", "accessibility"];
  await f.flow.beginPermission("accessibility");
  assert.deepEqual(f.flow.signatureChangedPermissions.value, ["accessibility"]);
  await f.flow.resume();
  await f.flow.requestPermission(true);
  assert.deepEqual(f.calls, ["prepare", "settings_accessibility"]);
  await f.flow.requestPermission();
  assert.deepEqual(f.calls, ["prepare", "settings_accessibility", "authorize_accessibility"]);
  f.grant("accessibility");
  await f.flow.resume(true);
  assert.equal(f.flow.phase.value, "idle");
  assert.deepEqual(f.flow.signatureChangedPermissions.value, []);
  assert.ok(!f.calls.includes("start"));
});

test("a changed signature with valid permissions continues sharing without repair", async () => {
  const f = fixture(true, true);
  f.overview.signatureChangedPermissions = ["screen_recording", "accessibility"];
  await f.flow.begin(f.request());
  assert.deepEqual(f.calls, ["prepare", "start"]);
});

test("authorization resets a missing grant without signature history and starts only after approval", async () => {
  const f = fixture(false, true);
  await f.flow.begin(f.request());
  assert.deepEqual(f.flow.signatureChangedPermissions.value, []);
  assert.deepEqual(f.calls, ["prepare"]);
  await f.flow.requestPermission();
  assert.deepEqual(f.calls, ["prepare", "authorize_screen_recording"]);
  assert.equal(f.flow.phase.value, "awaiting_permission");
  f.grant("screen_recording");
  await f.flow.resume(true);
  assert.deepEqual(f.calls, ["prepare", "authorize_screen_recording", "start"]);
});

test("authorization errors finish the guide without retrying or starting sharing", async () => {
  for (const code of ["background_share_permission_identity_unavailable", "background_share_permission_reset_failed", "background_share_permission_repair_busy"]) {
    const f = fixture(true, false);
    await f.flow.beginPermission("accessibility");
    f.failPermission(code);
    await f.flow.requestPermission();
    assert.equal(f.flow.phase.value, "idle");
    assert.equal(f.busy(), false);
    assert.equal(String(f.errors[0]), `Error: ${code}`);
    await f.flow.resume(true);
    assert.deepEqual(f.calls, ["prepare", "authorize_accessibility"]);
  }
});

test("cancelling in-flight authorization keeps lifecycle controls locked until it settles", async () => {
  for (const purpose of ["start", "permission"]) {
    const f = fixture();
    let release!: () => void;
    f.holdPermission(new Promise(resolve => { release = resolve; }));
    if (purpose === "start") await f.flow.begin(f.request());
    else await f.flow.beginPermission("screen_recording");
    const operation = f.flow.requestPermission();
    assert.equal(f.flow.phase.value, "requesting_permission");
    f.flow.cancel();
    assert.equal(f.busy(), true);
    await f.flow.begin(f.request());
    assert.equal(f.busy(), true);
    release();
    await operation;
    assert.equal(f.busy(), false);
    assert.equal(f.flow.phase.value, "idle");
    assert.deepEqual(f.calls, ["prepare", "authorize_screen_recording"]);
  }
});
