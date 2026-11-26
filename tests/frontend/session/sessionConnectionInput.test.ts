import assert from "node:assert/strict";
import test from "node:test";
import { ref } from "vue";
import { useSessionConnectionLifecycle } from "../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts";
import { createSessionManager, summarizeSessionTabs, type RemoteSession } from "../../../src/session/core/sessionManager.ts";
import { createSessionConnectionRegistry } from "../../../src/session/core/sessionConnectionRegistry.ts";
import { createInitialConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";
import type { ProtocolConnection, ProtocolConnectOptions } from "../../../src/session/protocol/protocolAdapter.ts";
import type { CoreTransportHandle, WorkspaceMode } from "../../../src/app/types.ts";
import { createSessionDisplayInfo, type SessionQualityProfile } from "../../../src/session/viewport/sessionDisplay.ts";
import { createDisconnectIssue } from "../../../src/session/diagnostics/sessionDiagnostics.ts";
import type { ClipboardSyncMode } from "../../../src/session/settings/sessionSettings.ts";
import type { RemoteSessionStartRequest } from "../../../src/session/protocol/remoteSessionCore.ts";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((complete) => { resolve = complete; });
  return { promise, resolve };
}

function fixture(stage: "local-prepare" | "core-start" | "session-clear" | "explicit-endpoint") {
  const waiting = deferred();
  const reached = deferred();
  const sessionManager = createSessionManager({ id: () => "binding-session" });
  const registry = createSessionConnectionRegistry();
  const nativeRequests: ProtocolConnectOptions[] = [];
  const coreRequests: RemoteSessionStartRequest[] = [];
  let clearCalls = 0;
  const stateChanges: string[] = [];
  let passwordReads = 0;
  // Observe the read time using an empty value; no credential is a fixture.
  const password = new Proxy(ref(""), {
    get(target, property, receiver) {
      if (property === "value") passwordReads += 1;
      return Reflect.get(target, property, receiver);
    },
  });
  const model = createInitialConnectionModel();
  // This boundary fake provides only the methods called by the lifecycle.
  const connection = {
    connectionModel: model, displaySource: "vnc", inputSource: "vnc",
    transport: { id: "binding-transport", surfaceUrl: "ws://127.0.0.1/surface" },
    disconnect: async () => undefined, setQualityProfile() {},
    getDisplayInfo: () => null, getRuntimeEvidence: () => null,
    refreshDisplay() {}, setInputEnabled() {}, focus() {},
  } as ProtocolConnection;
  const noop = () => undefined;
  const done = async () => undefined;
  const options: Parameters<typeof useSessionConnectionLifecycle>[0] = {
    activeRemoteSession: ref<RemoteSession | null>(null),
    activeWorkspace: ref<WorkspaceMode>("hosts"),
    transportHandle: ref<CoreTransportHandle | null>(null),
    clearSessionConnection: async (sessionId) => {
      clearCalls += 1;
      registry.detach(sessionId);
      if (stage === "session-clear") { reached.resolve(); await waiting.promise; }
    },
    clipboardSyncMode: ref<ClipboardSyncMode>("bidirectional-active"),
    enterSessionFullscreen: done,
    errorMessage: ref(""), exitSessionFullscreen: done,
    findSession: (id) => sessionManager.sessions.find((session) => session.id === id),
    fullscreenSessionId: ref<string | null>(null),
    host: ref(stage === "explicit-endpoint" ? "127.0.0.2" : "127.0.0.1"), port: ref(stage === "explicit-endpoint" ? 5901 : 5900), password,
    logEvent: noop, notify: noop,
    onSessionStateChanged: (sessionId) => { stateChanges.push(sessionId); },
    startRemoteSessionCore: async (request) => {
      coreRequests.push(request);
      if (stage === "core-start" || stage === "explicit-endpoint") { reached.resolve(); await waiting.promise; }
      return { ...request, state: "idle", surfaces: [] };
    },
    attachRemoteSessionCoreSurface: async (request) => ({
      surfaceId: request.surfaceId, owner: request.owner, windowLabel: request.windowLabel,
      active: true, surfaceUrl: "ws://127.0.0.1/surface",
    }),
    persistHostConnectionAttempt: done,
    prepareLocalVncServerForSession: async () => {
      if (stage === "local-prepare") { reached.resolve(); await waiting.promise; }
      return undefined;
    },
    refreshLocalVncServerStatus: done, refreshSessionTabs: noop,
    refreshVisibleSessionIds: () => new Set(["binding-session"]),
    requestPasswordForSession: async () => undefined,
    screenTargets: new Map([["binding-session", { replaceChildren() {} } as HTMLElement]]),
    sessionConnectionRegistry: registry, sessionManager,
    defaultQualityProfile: ref<SessionQualityProfile>("balanced"),
    setSessionReadonlyState: noop, setSessionViewOnlySharedState: noop,
    setSessionClipboardPolicy: noop, updateSessionClipboardActivity: noop,
    setSessionRemoteDisplayInventory: noop, startDurationTimer: noop,
    status: ref("idle"), t: (key) => key,
    updateSessionDisplayInfo: noop, updateSessionRuntimeEvidence: noop,
    vncAdapter: {
      protocol: "vnc",
      connect: async (request) => { nativeRequests.push(request); return connection; },
      disconnect: async (owner) => { await owner.disconnect(); },
    },
  };
  return { options, sessionManager, nativeRequests, coreRequests, registry, waiting, reached,
    clearCalls: () => clearCalls, stateChanges, passwordReads: () => passwordReads };
}

test("terminal error and issue callbacks release an externally owned connection without a follow-up status", async () => {
  for (const kind of ["error", "issue"] as const) {
    const f = fixture("core-start");
    f.waiting.resolve();
    const connect = f.options.vncAdapter.connect;
    f.options.vncAdapter.connect = async (request) => {
      const connection = await connect(request);
      request.onStatus("connected");
      return connection;
    };
    const lifecycle = useSessionConnectionLifecycle(f.options);
    await lifecycle.connectVnc();
    const before = f.clearCalls();
    const publishedBefore = f.stateChanges.length;
    assert.ok(f.registry.get("binding-session"));
    if (kind === "error") f.nativeRequests[0]!.onError?.("surface_websocket_closed");
    else {
      f.nativeRequests[0]!.onConnectionIssue?.(createDisconnectIssue("remote_closed"));
      f.nativeRequests[0]!.onStatus("disconnected unexpectedly");
    }
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(f.registry.get("binding-session"), undefined, kind);
    assert.ok(f.clearCalls() > before, kind);
    assert.ok(f.stateChanges.length > publishedBefore, kind);
    if (kind === "issue") assert.equal(f.sessionManager.sessions[0]?.lastIssue?.code, "remote_closed");
    await lifecycle.dispose();
  }
});

test("recent host use is recorded once at startup for both successful and failed connections", async () => {
  for (const connected of [true, false]) {
    const f = fixture("core-start");
    const attempts: Array<string | undefined> = [];
    f.options.persistHostConnectionAttempt = async (hostId) => { attempts.push(hostId); };
    const connect = f.options.vncAdapter.connect;
    f.options.vncAdapter.connect = async (request) => {
      assert.deepEqual(attempts, ["host-1"], "recent use is recorded before the outcome");
      const connection = await connect(request);
      if (!connected) throw new Error("tcp_unreachable");
      request.onStatus("connected");
      return connection;
    };
    const lifecycle = useSessionConnectionLifecycle(f.options);
    const connecting = lifecycle.connectVnc(undefined, "host-1");
    await f.reached.promise;
    assert.deepEqual(attempts, ["host-1"]);
    const duplicate = lifecycle.connectVnc(undefined, "host-1", { retrySessionId: "binding-session" });
    f.waiting.resolve();
    await Promise.all([connecting, duplicate]);
    if (connected) f.nativeRequests[0]!.onError?.("tcp_unreachable");
    assert.deepEqual(attempts, ["host-1"], "outcomes and duplicate starts do not record another use");
    assert.equal(f.sessionManager.sessions[0]!.state, "failed");
    await lifecycle.dispose();
  }
});

test("retry keeps the original host association while a cancelled password prompt records no use", async () => {
  const f = fixture("core-start");
  f.waiting.resolve();
  const attempts: Array<string | undefined> = [];
  f.options.persistHostConnectionAttempt = async (hostId) => { attempts.push(hostId); };
  const lifecycle = useSessionConnectionLifecycle(f.options);
  await lifecycle.connectVnc(undefined, "host-1");
  await lifecycle.disconnectSessionFromViewport("binding-session");
  await lifecycle.retrySessionFromViewport("binding-session");
  assert.deepEqual(attempts, ["host-1"], "cancelled password input does not record a connection");
  await lifecycle.connectVnc(undefined, undefined, { retrySessionId: "binding-session" });
  assert.deepEqual(attempts, ["host-1", "host-1"]);
  await lifecycle.closeSessionTab("binding-session");
  await lifecycle.connectVnc(undefined, "host-1", { retrySessionId: "binding-session" });
  assert.deepEqual(attempts, ["host-1", "host-1"], "closed sessions do not record a connection");
  await lifecycle.dispose();
});

test("session rail names survive tab reuse and retries without inferring names from a host ID", async () => {
  for (const savedHostName of [undefined, "办公室电脑"]) {
    const f = fixture("core-start");
    f.waiting.resolve();
    const lifecycle = useSessionConnectionLifecycle(f.options);
    await lifecycle.connectVnc(undefined, "host-1", { savedHostName });
    const label = savedHostName ?? "127.0.0.1:5900";
    const assertName = () => {
      assert.equal(f.sessionManager.sessions.length, 1);
      assert.equal(summarizeSessionTabs(f.sessionManager.sessions)[0]!.railLabel, label);
    };
    assertName();
    await lifecycle.connectVnc(undefined, "host-1", { savedHostName: "改名后的电脑" });
    assertName();
    await lifecycle.disconnectSessionFromViewport("binding-session");
    await lifecycle.connectVnc(undefined, "host-1", { savedHostName: "改名后的电脑" });
    assertName();
    await lifecycle.disconnectSessionFromViewport("binding-session");
    await lifecycle.connectVnc(undefined, undefined, { retrySessionId: "binding-session" });
    assertName();
    await lifecycle.dispose();
  }
});

test("one connection keeps its initial target, policy and credential read across setup awaits", async () => {
  for (const stage of ["local-prepare", "core-start", "session-clear", "explicit-endpoint"] as const) {
    const f = fixture(stage);
    const lifecycle = useSessionConnectionLifecycle(f.options);
    const requested = {
      readonly: false,
      endpoint: stage === "explicit-endpoint" ? { host: "127.0.0.1", port: 5900 } : undefined,
    };
    const connecting = lifecycle.connectVnc(undefined, undefined, requested);
    await f.reached.promise;
    const readsAtWait = f.passwordReads();
    f.options.host.value = "127.0.0.2";
    f.options.port.value = 5901;
    requested.readonly = true;
    if (requested.endpoint) { requested.endpoint.host = "127.0.0.4"; requested.endpoint.port = 5904; }
    f.waiting.resolve();
    await connecting;
    assert.equal(f.nativeRequests.length, 1, f.options.errorMessage.value);
    assert.equal(f.nativeRequests[0]!.host, "127.0.0.1", stage);
    assert.equal(f.nativeRequests[0]!.port, 5900, stage);
    assert.equal(f.nativeRequests[0]!.readonly, false, stage);
    assert.equal(f.nativeRequests[0]!.password, undefined);
    assert.equal(f.coreRequests[0]!.target, "127.0.0.1:5900");
    assert.equal(f.sessionManager.sessions[0]!.target, "127.0.0.1:5900");
    assert.equal(f.sessionManager.sessions[0]!.transport?.id, "binding-transport");
    assert.equal(f.options.errorMessage.value, "");
    assert.equal(readsAtWait, stage === "explicit-endpoint" ? 0 : 1, stage);
    assert.equal(f.passwordReads(), stage === "explicit-endpoint" ? 0 : 1);

  }
  const closed = fixture("core-start");
  closed.waiting.resolve();
  const session = closed.sessionManager.create({
    protocol: "vnc", host: "127.0.0.1", port: 5900, qualityProfile: "balanced",
  });
  closed.sessionManager.close(session.id);
  const previousCount = closed.sessionManager.sessions.length;
  closed.options.host.value = "127.0.0.2";
  closed.options.port.value = 5901;
  closed.options.errorMessage.value = "existing-message";
  const lifecycle = useSessionConnectionLifecycle(closed.options);
  await lifecycle.connectVnc(undefined, undefined, { retrySessionId: session.id });
  assert.equal(closed.nativeRequests.length, 0);
  assert.equal(closed.sessionManager.sessions.length, previousCount);
  assert.equal(closed.passwordReads(), 0);
  assert.equal(closed.options.errorMessage.value, "existing-message");
  const conflicting = fixture("core-start");
  conflicting.waiting.resolve();
  const existing = conflicting.sessionManager.create({
    protocol: "vnc", host: "127.0.0.1", port: 5900, qualityProfile: "balanced",
  });
  await useSessionConnectionLifecycle(conflicting.options).connectVnc(undefined, undefined, {
    retrySessionId: existing.id, endpoint: { host: "127.0.0.2", port: 5901 },
  });
  assert.equal(conflicting.nativeRequests.length, 0);
  assert.equal(conflicting.passwordReads(), 0);
});

test("closing while Core starts waits for the late Core and never attaches or connects it", async () => {
  const f = fixture("core-start");
  let corePresent = false;
  const start = f.options.startRemoteSessionCore!;
  f.options.startRemoteSessionCore = async (request) => {
    const result = await start(request);
    corePresent = true;
    return result;
  };
  f.options.stopRemoteSessionCore = async () => { corePresent = false; };
  const lifecycle = useSessionConnectionLifecycle(f.options);
  const connecting = lifecycle.connectVnc();
  await f.reached.promise;
  let closed = false;
  const closing = lifecycle.closeSessionTab("binding-session").then(() => { closed = true; });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(closed, false);
  f.waiting.resolve();
  await Promise.all([connecting, closing]);
  assert.equal(corePresent, false);
  assert.equal(f.nativeRequests.length, 0);
  assert.equal(f.options.sessionConnectionRegistry.get("binding-session"), undefined);
  assert.equal(f.options.errorMessage.value, "");
});

test("closing during Adapter startup stops Core, drains the old promise and discards its late connection", async () => {
  const f = fixture("core-start");
  f.waiting.resolve();
  const started = deferred();
  const startup = deferred();
  let nativeOptions: ProtocolConnectOptions | undefined;
  let stops = 0;
  let localCloses = 0;
  let displayUpdates = 0;
  const connect = f.options.vncAdapter.connect;
  f.options.vncAdapter.connect = async (request) => {
    nativeOptions = request;
    const connection = await connect(request);
    connection.disconnect = async () => { localCloses += 1; };
    started.resolve();
    await startup.promise;
    return connection;
  };
  f.options.stopRemoteSessionCore = async () => { stops += 1; };
  f.options.updateSessionDisplayInfo = () => { displayUpdates += 1; };
  const lifecycle = useSessionConnectionLifecycle(f.options);
  const connecting = lifecycle.connectVnc();
  await started.promise;
  let closed = false;
  const closing = lifecycle.closeSessionTab("binding-session").then(() => { closed = true; });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(nativeOptions!.signal!.aborted, true);
  assert.ok(stops > 0);
  assert.equal(closed, false);
  nativeOptions!.onDisplayInfo?.(createSessionDisplayInfo(1, 1));
  assert.equal(displayUpdates, 0);
  startup.resolve();
  await Promise.all([connecting, closing]);
  assert.equal(localCloses, 1);
  assert.equal(f.options.sessionConnectionRegistry.get("binding-session"), undefined);
  await lifecycle.dispose();
  await lifecycle.connectVnc();
  assert.equal(f.nativeRequests.length, 1);
});

test("failed disconnect remains owned after registry removal until a retry releases it", async () => {
  const f = fixture("local-prepare");
  f.waiting.resolve();
  let fail = true;
  let disconnects = 0;
  const connect = f.options.vncAdapter.connect;
  f.options.vncAdapter.connect = async (request) => {
    const connection = await connect(request);
    connection.disconnect = async () => {
      disconnects += 1;
      if (fail) throw new Error("disconnect_failed");
    };
    return connection;
  };
  const lifecycle = useSessionConnectionLifecycle(f.options);
  await lifecycle.connectVnc();
  await assert.rejects(lifecycle.disconnectSessionFromViewport("binding-session"), /disconnect_failed/);
  assert.equal(f.options.sessionConnectionRegistry.get("binding-session"), undefined);
  assert.equal(disconnects, 1);
  fail = false;
  await lifecycle.closeSessionTab("binding-session");
  assert.ok(disconnects > 1);
  assert.equal(f.sessionManager.sessions.length, 0);
});
