import assert from "node:assert/strict";
import test from "node:test";

import { createInitialConnectionModel, sessionSupportsCapability } from "../../../src/session/connection-model/connectionModel.ts";
import { createVncProtocolAdapter, type ProtocolConnectOptions, type RemoteSessionRfbEvent } from "../../../src/session/protocol/protocolAdapter.ts";
import type { ConnectionIssue } from "../../../src/session/diagnostics/sessionDiagnostics.ts";
import type { SessionClipboardActivity } from "../../../src/session/clipboard/sessionClipboardActivity.ts";
import { createClipboardCoordinator } from "../../../src/session/clipboard/clipboardCoordinator.ts";

test("display wake reports a rejected Agent request", async () => {
  const fixture = setup({
    nativeAgentDescription: async () => ({ serverId: "server-1", viewOnly: false, capabilities: ["display.wake"] }),
    nativeAgentWake: async () => ({ ok: false, error: "display_wake_failed" }),
  });
  const connection = await fixture.connect();
  try {
    fixture.emit({ type: "discoveryManifest", manifest: {
      version: "1.0", server: { id: "server-1" },
      agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
    } });
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(connection.connectionModel.enhancedChannel.status, "available");
    await assert.rejects(connection.wakeRemoteDisplay!(), /display_wake_failed/);
    assert.equal(fixture.calls.some((call) => call.command === "send_remote_session_rfb_input"), false);
  } finally {
    await fixture.adapter.disconnect(connection);
  }
});

test("display wake submits ordered RFB pointer moves when the remote has no Agent wake capability", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  try {
    connection.setInputEnabled(true);
    assert.equal(await connection.wakeRemoteDisplay!(), "sent-unconfirmed");
    assert.deepEqual(fixture.calls.filter((call) => call.command === "send_remote_session_rfb_input")
      .map((call) => (call.args.request as { input: unknown }).input), [
      { type: "pointer", x: 1, y: 1, buttonMask: 0 },
      { type: "pointer", x: 2, y: 2, buttonMask: 0 },
    ]);
  } finally {
    await fixture.adapter.disconnect(connection);
  }
});

test("display wake cannot report an RFB attempt when viewport input is unavailable", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  try {
    await assert.rejects(connection.wakeRemoteDisplay!(), /display_wake_input_unavailable/);
    assert.equal(fixture.calls.some((call) => call.command === "send_remote_session_rfb_input"), false);
  } finally {
    await fixture.adapter.disconnect(connection);
  }
});

test("startup display wake waits for Agent authentication and a slow first frame, then sends once", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const description = Promise.withResolvers<unknown>();
  const fixture = setupDisplayWakeStartup({ nativeAgentDescription: () => description.promise });
  const connecting = fixture.connect();
  await flushStartupEvents();
  fixture.emitDisplayWakeDiscovery();
  await flushStartupEvents();
  context.mock.timers.tick(1500);
  assert.equal(fixture.wakeRequests().length, 0);

  description.resolve({ serverId: "server-1", viewOnly: false, capabilities: ["display.wake"] });
  await flushStartupEvents();
  context.mock.timers.tick(1499);
  assert.equal(fixture.wakeRequests().length, 0);
  context.mock.timers.tick(1);
  assert.deepEqual(fixture.wakeRequests().map((call) => call.args.request), [{
    host: "127.0.0.1", port: 6900, type: "display.wake", sessionId: "remote-session-1",
  }]);
  assert.equal(fixture.surfaceCreations, 0);

  fixture.emitDisplayWakeDiscovery();
  await flushStartupEvents();
  context.mock.timers.tick(1500);
  assert.equal(fixture.wakeRequests().length, 1);
  assert.equal(fixture.calls.some((call) => call.command === "send_remote_session_rfb_input"), false);
  fixture.finishStartup();
  const connection = await connecting;
  assert.deepEqual(fixture.statuses, ["connecting", "connected"]);
  await fixture.adapter.disconnect(connection);
});

test("startup display wake is skipped after first frame, cancellation, or startup failure", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  for (const outcome of ["first-frame", "cancelled", "failed"] as const) {
    const fixture = setupDisplayWakeStartup();
    const controller = new AbortController();
    const connecting = fixture.connect({ signal: controller.signal });
    await flushStartupEvents();
    fixture.emitDisplayWakeDiscovery();
    await flushStartupEvents();

    if (outcome === "first-frame") {
      fixture.finishStartup();
      const connection = await connecting;
      context.mock.timers.tick(1500);
      await fixture.adapter.disconnect(connection);
    } else if (outcome === "cancelled") {
      controller.abort();
      context.mock.timers.tick(1500);
      fixture.finishStartup();
      await assert.rejects(connecting, /session_connect_cancelled/);
    } else {
      fixture.failStartup({ code: "rfb_startup_timeout", category: "target_unreachable", retryable: true });
      await assert.rejects(connecting, /rfb_startup_timeout/);
      context.mock.timers.tick(1500);
    }
    assert.equal(fixture.wakeRequests().length, 0, outcome);
  }
});

test("startup display wake leaves readonly and ordinary VNC connections alone", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  for (const policy of ["client-readonly", "server-view-only", "no-agent", "no-wake-capability"] as const) {
    const fixture = setupDisplayWakeStartup({
      nativeAgentDescription: async () => ({
        serverId: "server-1", viewOnly: policy === "server-view-only",
        capabilities: policy === "no-wake-capability" ? [] : ["display.wake"],
      }),
    });
    const connecting = fixture.connect({ readonly: policy === "client-readonly" });
    await flushStartupEvents();
    if (policy !== "no-agent") fixture.emitDisplayWakeDiscovery(policy === "server-view-only");
    await flushStartupEvents();
    context.mock.timers.tick(1500);
    assert.equal(fixture.wakeRequests().length, 0, policy);
    assert.equal(fixture.calls.some((call) => call.command === "send_remote_session_rfb_input"), false);
    fixture.finishStartup();
    await fixture.adapter.disconnect(await connecting);
  }
});

test("startup display wake rejection still allows RFB startup to finish", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const fixture = setupDisplayWakeStartup({
    nativeAgentWake: async () => ({ ok: false, error: "display_wake_failed" }),
  });
  const connecting = fixture.connect();
  await flushStartupEvents();
  fixture.emitDisplayWakeDiscovery();
  await flushStartupEvents();
  context.mock.timers.tick(1500);
  await flushStartupEvents();
  assert.equal(fixture.wakeRequests().length, 1);
  assert.equal(fixture.issues.length, 0);
  fixture.finishStartup();
  const connection = await connecting;
  assert.deepEqual(fixture.statuses, ["connecting", "connected"]);
  await fixture.adapter.disconnect(connection);
});

test("a Client copy made outside the session reaches the remote clipboard before refocusing", async () => {
  const fixture = setup({ remoteEndpoint: true });
  const connection = await fixture.connect();
  let text = "";
  const coordinator = createClipboardCoordinator({ readText: async () => text, writeText: async () => {} });
  try {
    await coordinator.register("session", { sendText: async value => { await connection.sendRemoteClipboardText(value); } }, false);
    connection.setInputEnabled(false);
    await coordinator.setActiveSession(null);
    text = "本机复制的中文 clipboard";
    await coordinator.syncClientClipboard();
    const inputs = fixture.calls.filter(call => call.command === "send_remote_session_rfb_input");
    assert.deepEqual(inputs.map(call => (call.args.request as { input: unknown }).input), [
      { type: "clipboard", text },
    ]);
    // Keyboard focus must remain revoked while background clipboard sync runs.
    connection.sendRemoteKeyEvent({ down: true, keysym: 65, key: "a", code: "KeyA" });
    assert.equal(fixture.calls.filter(call => call.command === "send_remote_session_rfb_input").length, 1);
  } finally {
    coordinator.dispose();
    await fixture.adapter.disconnect(connection);
  }
});

test("background clipboard write uses authenticated Agent capability and waits for its acknowledgement", async () => {
  const fixture = setup({ remoteEndpoint: true, nativeAgentDescription: async () => ({
    serverId: "server-1", viewOnly: false, capabilities: ["clipboard.text.write"],
  }) });
  const connection = await fixture.connect();
  connection.setInputEnabled(false);
  fixture.emit({ type: "discoveryManifest", manifest: {
    version: "1.0", server: { id: "server-1" },
    agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
  } });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(connection.connectionModel.enhancedChannel.status, "available");
  assert.equal(await connection.sendRemoteClipboardText("明天你好"), "written");
  assert.equal(fixture.calls.filter((call) => call.command === "send_remote_session_rfb_input").length, 0);
  assert.equal((fixture.calls.at(-1)?.args.request as { type: string; text: string }).type, "clipboard.text.write");
  assert.equal((fixture.calls.at(-1)?.args.request as { type: string; text: string }).text, "明天你好");
  assert.equal(fixture.clipboardActivities.at(-1)?.channel, "agent");
  await fixture.adapter.disconnect(connection);
});

test("clipboard write falls back to RFB with an unconfirmed result when Agent is absent", async () => {
  const fixture = setup({ remoteEndpoint: true });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  assert.equal(await connection.sendRemoteClipboardText("hello"), "sent-unconfirmed");
  assert.deepEqual((fixture.calls.at(-1)?.args.request as { input: unknown }).input, { type: "clipboard", text: "hello" });
  assert.equal(fixture.clipboardActivities.at(-1)?.channel, "rfb");
  await fixture.adapter.disconnect(connection);
});

test("Agent clipboard rejection is not reported as a successful RFB fallback", async () => {
  const fixture = setup({ remoteEndpoint: true,
    nativeAgentDescription: async () => ({ serverId: "server-1", viewOnly: false, capabilities: ["clipboard.text.write"] }),
    nativeAgentWrite: async () => ({ ok: false, error: "agent_user_desktop_unavailable" }),
  });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  fixture.emit({ type: "discoveryManifest", manifest: {
    version: "1.0", server: { id: "server-1" },
    agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
  } });
  await new Promise<void>((resolve) => setImmediate(resolve));
  await assert.rejects(connection.sendRemoteClipboardText("hello"), /agent_user_desktop_unavailable/);
  assert.equal(fixture.calls.filter((call) => call.command === "send_remote_session_rfb_input").length, 0);
  assert.equal(fixture.clipboardActivities.length, 0);
  await fixture.adapter.disconnect(connection);
});

test("clipboard write does not report success when native enqueue fails", async () => {
  const fixture = setup({ nativeInputError: { code: "rfb_resource_limit", category: "resource_limit", retryable: false } });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  await assert.rejects(connection.sendRemoteClipboardText("hello"));
  assert.equal(fixture.clipboardActivities.some((activity) => activity.accepted), false);
  await fixture.adapter.disconnect(connection);
});

test("remote view-only policy disables input without Agent and cannot be overridden by client readonly", async () => {
  const fixture = setup({ remoteEndpoint: true });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  fixture.emit({ type: "discoveryManifest", manifest: {
    version: "1.0", server: { id: "remote-view-only" }, policy: { viewOnly: true },
  } });
  assert.deepEqual(fixture.viewOnlyChanges, [true]);
  connection.setReadonlyMode(false);
  assert.equal(await connection.sendRemoteClipboardText("readonly clipboard"), "skipped");
  const before = fixture.calls.length;
  connection.sendRemoteKeyEvent({ down: true, keysym: 65, key: "a", code: "KeyA" });
  connection.forwardExternalInput?.({ sessionId: "native-session", type: "pointer", action: "down", x: 10, y: 10, buttonMask: 1 });
  assert.equal(fixture.calls.length, before);
  assert.equal(connection.connectionModel.localEndpointMatch.state, "not-matched");
  await fixture.adapter.disconnect(connection);
});

test("view-only sessions can select an advertised display while keyboard input remains blocked", async () => {
  const fixture = setup({
    remoteEndpoint: true,
    nativeAgentDescription: async () => ({
      serverId: "server-1", viewOnly: true,
      capabilities: ["agent.display.enumerate", "agent.display.select"],
    }),
  });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  fixture.emit({ type: "discoveryManifest", manifest: {
    version: "1.0", server: { id: "server-1" }, policy: { viewOnly: true },
    agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
  } });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(sessionSupportsCapability(connection.connectionModel.capabilities, "agent.display.select"), true);
  connection.setReadonlyMode(true);
  const before = fixture.calls.length;
  connection.sendRemoteKeyEvent({ down: true, keysym: 65, key: "a", code: "KeyA" });
  assert.equal(fixture.calls.length, before);
  await connection.selectRemoteDisplay?.("display-2");
  assert.deepEqual((fixture.calls.at(-1)?.args.request as { type: string; displayId: string }), {
    host: "127.0.0.1", port: 6900, type: "agent.display.select", sessionId: "remote-session-1", displayId: "display-2",
  });
  await fixture.adapter.disconnect(connection);
});

test("remote view-only policy takes effect while optional Agent authentication is still pending", async () => {
  const fixture = setup({ remoteEndpoint: true, nativeAgentDescription: () => new Promise(() => {}) });
  const connection = await fixture.connect();
  fixture.emit({ type: "discoveryManifest", manifest: {
    version: "1.0", server: { id: "server-1" }, policy: { viewOnly: true },
    agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
  } });
  assert.deepEqual(fixture.viewOnlyChanges, [true]);
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(connection.connectionModel.enhancedChannel.status, "negotiating");
  await fixture.adapter.disconnect(connection);
});

test("invalid discovery cannot change readonly policy", async () => {
  const fixture = setup({ remoteEndpoint: true });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  fixture.emit({ type: "discoveryManifest", manifest: { version: "invalid", policy: { viewOnly: true } } });
  assert.deepEqual(fixture.viewOnlyChanges, []);
  const before = fixture.calls.length;
  connection.sendRemoteKeyEvent({ down: true, keysym: 65, key: "a", code: "KeyA" });
  assert.equal(fixture.calls.length, before + 1);
  await fixture.adapter.disconnect(connection);
});

test("losing input ownership releases held keys and pointer buttons before another window controls a session", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  connection.sendRemoteKeyEvent({ down: true, keysym: 0xffe1 });
  connection.forwardExternalInput({ sessionId: "native-session", type: "pointer", action: "down", x: 12, y: 34, buttonMask: 1 });
  const before = fixture.calls.length;
  connection.setInputEnabled(false);
  const released = fixture.calls.slice(before).filter((call) => call.command === "send_remote_session_rfb_input")
    .map((call) => (call.args.request as { input: unknown }).input);
  assert.deepEqual(released, [
    { type: "key", down: false, keysym: 0xffe1 },
    { type: "pointer", x: 12, y: 34, buttonMask: 0 },
  ]);
  connection.setInputEnabled(false);
  connection.sendRemoteKeyEvent({ down: true, keysym: 65 });
  assert.equal(fixture.calls.length, before + 2);
  await fixture.adapter.disconnect(connection);
});

test("authenticated runtime restriction gates input until a refreshed Surface commit", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  connection.sendRemoteKeyEvent({ down: true, keysym: 0xffe3, key: "Control", code: "ControlLeft" });
  fixture.emit({ type: "serverRuntime", notice: { kind: "desktop", desktop: "unavailable", secureAttentionAvailable: false } });
  const before = fixture.calls.length;
  connection.sendRemoteKeyEvent({ down: true, keysym: 65, key: "a", code: "KeyA" });
  assert.equal(fixture.calls.length, before);
  fixture.present();
  assert.equal(connection.getServerRuntimeState?.()?.desktop, "unavailable");
  fixture.emit({ type: "serverRuntime", notice: { kind: "desktop", desktop: "available", secureAttentionAvailable: false } });
  assert.equal(connection.getServerRuntimeState?.()?.desktop, "recovering");
  assert.deepEqual((fixture.calls.at(-1)?.args.request as { input: unknown }).input, { type: "refresh", incremental: false });
  fixture.present();
  assert.deepEqual((fixture.calls.at(-1)?.args.request as { input: unknown }).input, { type: "key", down: false, keysym: 0xffe3 });
  connection.sendRemoteKeyEvent({ down: true, keysym: 65, key: "a", code: "KeyA" });
  assert.deepEqual((fixture.calls.at(-1)?.args.request as { input: unknown }).input, { type: "key", down: true, keysym: 65 });
  await fixture.adapter.disconnect(connection);
});

test("explicit keyboard capture release clears held keys and buttons exactly once", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  connection.sendRemoteKeyEvent({ down: true, keysym: 0xffeb, code: "MetaLeft" });
  connection.sendRemoteKeyEvent({ down: true, keysym: 0x61, code: "KeyA" });
  connection.forwardExternalInput({ sessionId: "native-session", type: "pointer", action: "down", x: 10, y: 20, buttonMask: 1 });
  const before = fixture.calls.length;
  connection.releaseRemoteInput!();
  connection.releaseRemoteInput!();
  assert.deepEqual(fixture.calls.slice(before).map(call => (call.args.request as { input: unknown }).input), [
    { type: "key", down: false, keysym: 0xffeb },
    { type: "key", down: false, keysym: 0x61 },
    { type: "pointer", x: 10, y: 20, buttonMask: 0 },
  ]);
  await fixture.adapter.disconnect(connection);
});

test("SAS uses the typed result channel and never confuses native enqueue with remote submission", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  fixture.emit({ type: "serverRuntime", notice: { kind: "desktop", desktop: "available", secureAttentionAvailable: true } });
  const pending = connection.sendSecureAttention!();
  const input = (fixture.calls.at(-1)?.args.request as { input: { type: string; requestId: number } }).input;
  assert.equal(input.type, "secureAttention");
  assert.ok(input.requestId > 0);
  fixture.emit({ type: "serverRuntime", notice: { kind: "secureAttentionResponse", requestId: input.requestId, result: "denied" } });
  assert.equal(await pending, "denied");
  await fixture.adapter.disconnect(connection);
});

test("performance events and Surface cadence report numeric windows without transport secrets", async (context) => {
  let now = 0;
  context.mock.method(performance, "now", () => now);
  const messages: string[] = [];
  let present: ((commit: { frameId: bigint; width: number; height: number }) => void) | undefined;
  let emit: ((event: RemoteSessionRfbEvent) => void) | undefined;
  const adapter = createVncProtocolAdapter({
    invoke: async () => ({}),
    listenRfbEvent: async (_name, handler) => { emit = handler; return () => {}; },
    createSurfaceSession: ({ onFrameCommitted }) => {
      present = onFrameCommitted;
      return { canvas: { width: 1920, height: 1080, focus() {} } as HTMLCanvasElement, disconnect() {} };
    },
  });
  const connection = await adapter.connect({
    target: {} as HTMLElement, sessionId: "numeric-metrics",
    surfaceUrl: "ws://127.0.0.1:61001/surface/numeric-metrics/main",
    host: "127.0.0.1", port: 5900, connectionModel: createInitialConnectionModel(),
    onStatus() {}, onError() {}, onEvent(_source, message) { messages.push(message); },
  });
  for (let frame = 1; frame <= 11; frame++) {
    now = (frame - 1) * 500;
    present?.({ frameId: BigInt(frame), width: 1920, height: 1080 });
  }
  const surface = messages.filter((message) => message.startsWith("surface_metrics"));
  assert.equal(surface.length, 1);
  assert.match(surface[0]!, /window_ms=5000 presented_frames=11 fps=2.20 latest_interval_ms=500.00/);
  emit?.({ type: "performance", metrics: {
    windowMs: 5000, receivedBytes: 120, updates: 10, committedFrames: 10,
    decodeTotalUs: 2000, decodeMaxUs: 300,
    readCalls: 40, readTimeouts: 20, readTotalUs: 4000000, consumeTotalUs: 5000,
    writeTotalUs: 30, writeMaxUs: 12, eventTotalUs: 20,
    encodings: { rectangles: 20, copyRect: 5, tight: 15, jpeg: 2 },
  } });
  assert.ok(messages.some((message) => message.startsWith("rfb_metrics window_ms=5000 received_bytes=120")));
  assert.ok(messages.some((message) => message.includes("read_calls=40 read_timeouts=20 read_total_us=4000000 consume_total_us=5000 write_total_us=30 write_max_us=12 event_total_us=20")));
  assert.ok(messages.every((message) => !message.includes("surface/numeric-metrics/main")));
  await adapter.disconnect(connection);
});

test("native disconnect delivers a typed issue without copying its raw reason", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  const raw = crypto.randomUUID();
  fixture.emit({ type: "disconnected", reason: raw,
    failure: { code: "rfb_security_failed", category: "authentication_failed", retryable: true } });
  assert.equal(fixture.issues.length, 1);
  assert.equal(fixture.issues[0]?.code, "authentication_failed");
  assert.equal(fixture.issues[0]?.technicalMessage, "rfb_security_failed");
  assert.ok(!JSON.stringify(fixture.issues).includes(raw));
  await fixture.adapter.disconnect(connection);
});

test("native command failure uses a single structured terminal issue", async () => {
  const fixture = setup({ nativeInputError: { code: "rfb_resource_limit", category: "resource_limit", retryable: false } });
  const connection = await fixture.connect();
  connection.refreshDisplay();
  connection.refreshDisplay();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(fixture.issues.length, 1);
  assert.equal(fixture.issues[0]?.code, "resource_limit");
  assert.equal(fixture.issues[0]?.retryable, false);
  await fixture.adapter.disconnect(connection);
});

test("Ctrl Alt Delete events wait for native acknowledgement before sending the next key", async () => {
  const acknowledgements: Array<() => void> = [];
  const fixture = setup({ nativeInput: () => new Promise<void>((resolve) => acknowledgements.push(resolve)) });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  const keys = [
    { key: "Control", code: "ControlLeft", keysym: 0xffe3 },
    { key: "Alt", code: "AltLeft", keysym: 0xffe9 },
    { key: "Delete", code: "Delete", keysym: 0xffff },
  ];
  const sequence = [
    ...keys.map((key) => ({ ...key, down: true })),
    ...[...keys].reverse().map((key) => ({ ...key, down: false })),
  ];
  connection.sendRemoteKeySequence(sequence);
  const sent = () => fixture.calls.filter((call) => call.command === "send_remote_session_rfb_input");
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(sent().length, 1, "a shortcut must not race six independent IPC requests");
  for (let index = 0; index < sequence.length; index++) {
    const request = sent()[index]!.args.request as { input: { down: boolean; keysym: number } };
    assert.equal(request.input.keysym, sequence[index]!.keysym);
    assert.equal(request.input.down, sequence[index]!.down);
    acknowledgements[index]!();
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(sent().length, Math.min(index + 2, sequence.length));
  }
  await fixture.adapter.disconnect(connection);
});

test("failed native disconnect stays retryable while concurrent and successful closes share completion", async () => {
  let fail = true;
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => { finish = resolve; });
  const fixture = setup({ nativeDisconnect: async () => {
    if (fail) throw { code: "rfb_transport_failed", category: "target_unreachable", retryable: true };
    await pending;
  } });
  const connection = await fixture.connect();
  await assert.rejects(fixture.adapter.disconnect(connection), /rfb_transport_failed/);
  fail = false;
  let done = false;
  const retry = fixture.adapter.disconnect(connection).then(() => { done = true; });
  const direct = connection.disconnect();
  const repeated = fixture.adapter.disconnect(connection);
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(done, false);
  assert.equal(fixture.calls.filter((call) => call.command === "disconnect_remote_session_rfb").length, 2);
  finish();
  await Promise.all([retry, direct, repeated]);
  await fixture.adapter.disconnect(connection);
  await connection.disconnect();
  assert.equal(fixture.calls.filter((call) => call.command === "disconnect_remote_session_rfb").length, 2);
  assert.equal(fixture.unlistenCount, 1);
});

function setup(options: {
  nativeStartupError?: unknown;
  nativeInputError?: unknown;
  nativeStartup?: () => Promise<unknown>;
  nativeInput?: () => Promise<unknown>;
  nativeDisconnect?: () => Promise<unknown>;
  nativeAgentDescription?: () => Promise<unknown>;
  nativeAgentWrite?: () => Promise<unknown>;
  nativeAgentMessage?: () => Promise<unknown>;
  nativeAgentWake?: () => Promise<unknown>;
  nativeAgentEnd?: () => Promise<unknown>;
  remoteEndpoint?: boolean;
  autoPresent?: boolean;
  surfaceStartupError?: string;
  surfaceStartupThrow?: string;
} = {}) {
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  const issues: ConnectionIssue[] = [];
  const statuses: string[] = [];
  const errors: string[] = [];
  const messages: string[] = [];
  const viewOnlyChanges: boolean[] = [];
  const clipboardActivities: SessionClipboardActivity[] = [];
  let present: ((commit: { frameId: bigint; width: number; height: number }) => void) | undefined;
  let surfaceError: ((code: string) => void) | undefined;
  let surfaceCreations = 0;
  let unlistenCount = 0;
  let modelPublications = 0;
  let eventHandler: ((event: RemoteSessionRfbEvent) => void) | undefined;
  let disconnected = false;
  const invoke = async (command: string, args: Record<string, unknown>) => {
    calls.push({ command, args });
    if (command === "connect_remote_session_rfb" && options.nativeStartupError) {
      throw options.nativeStartupError;
    }
    if (command === "connect_remote_session_rfb" && options.nativeStartup) return options.nativeStartup();
    if (command === "send_remote_session_rfb_input" && options.nativeInputError) throw options.nativeInputError;
    if (command === "send_remote_session_rfb_input" && options.nativeInput) return options.nativeInput();
    if (command === "disconnect_remote_session_rfb" && options.nativeDisconnect) return options.nativeDisconnect();
    if (command === "invoke_remote_agent") {
      const request = args.request as { type?: string };
      if (request.type === "remote.session.end" && options.nativeAgentEnd) return options.nativeAgentEnd();
      if (request.type === "clipboard.text.write" && options.nativeAgentWrite) return options.nativeAgentWrite();
      if (request.type === "message.show" && options.nativeAgentMessage) return options.nativeAgentMessage();
      if (request.type === "display.wake" && options.nativeAgentWake) return options.nativeAgentWake();
      if (request.type === "agent.session.open") return { ok: true, sessionId: "remote-session-1" };
      if (request.type === "agent.session.describe") {
        if (options.nativeAgentDescription) return { ok: true, description: await options.nativeAgentDescription() };
        return { ok: true, description: { serverId: "server-1", viewOnly: false, capabilities: ["file.transfer.upload"] } };
      }
    }
    return { ok: true, permissions: [], displays: [] };
  };
  const adapter = createVncProtocolAdapter({
    invoke: invoke as never,
    listenRfbEvent: async (_eventName, handler) => {
      eventHandler = handler;
      return () => { eventHandler = undefined; unlistenCount += 1; };
    },
    createSurfaceSession: ({ onFrameCommitted, onError }) => {
      surfaceCreations += 1;
      present = onFrameCommitted;
      surfaceError = onError;
      if (options.surfaceStartupThrow) throw new Error(options.surfaceStartupThrow);
      if (options.surfaceStartupError) onError?.(options.surfaceStartupError);
      if (options.autoPresent !== false) onFrameCommitted?.({ frameId: 1n, width: 1920, height: 1080 });
      return {
        canvas: { focus() {} } as HTMLCanvasElement,
        disconnect() { disconnected = true; },
      };
    },
  });
  const connect = (overrides: Pick<ProtocolConnectOptions, "readonly" | "signal"> = {}) => adapter.connect({
    target: {} as HTMLElement,
    sessionId: "native-session",
    surfaceUrl: "ws://127.0.0.1:61001/surface/native-session/main?token=test",
    host: "127.0.0.1",
    port: 5900,
    connectionModel: createInitialConnectionModel(options.remoteEndpoint ? undefined : { state: "matched", viewOnly: false }),
    onStatus(status) { statuses.push(status); }, onError(code) { errors.push(code); }, onEvent(_source, message) { messages.push(message); },
    onConnectionIssue(issue) { issues.push(issue); },
    onConnectionModelChanged() { modelPublications += 1; },
    onClipboardActivity(activity) { clipboardActivities.push(activity); },
    onViewOnlySharedChanged(value) { viewOnlyChanges.push(value); },
    ...overrides,
  });
  return {
    adapter,
    calls,
    issues,
    statuses,
    errors,
    messages,
    viewOnlyChanges,
    clipboardActivities,
    connect,
    present() { present?.({ frameId: 2n, width: 1920, height: 1080 }); },
    failSurface(code: string) { surfaceError?.(code); },
    emit(event: RemoteSessionRfbEvent) { eventHandler?.(event); },
    get disconnected() { return disconnected; },
    get surfaceCreations() { return surfaceCreations; },
    get unlistenCount() { return unlistenCount; },
    get modelPublications() { return modelPublications; },
  };
}

function setupDisplayWakeStartup(options: Parameters<typeof setup>[0] = {}) {
  const startup = Promise.withResolvers<void>();
  const fixture = setup({
    remoteEndpoint: true,
    nativeAgentDescription: async () => ({ serverId: "server-1", viewOnly: false, capabilities: ["display.wake"] }),
    ...options,
    nativeStartup: () => startup.promise,
  });
  return Object.assign(fixture, {
    finishStartup: () => startup.resolve(),
    failStartup: startup.reject,
    emitDisplayWakeDiscovery(viewOnly = false) {
      fixture.emit({
        type: "discoveryManifest",
        manifest: {
          version: "1.0", server: { id: "server-1" }, policy: { viewOnly },
          agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
        },
      });
    },
    wakeRequests: () => fixture.calls.filter((call) => call.command === "invoke_remote_agent"
      && (call.args.request as { type?: string }).type === "display.wake"),
  });
}

const flushStartupEvents = () => new Promise<void>((resolve) => setImmediate(resolve));

const remoteClosed: RemoteSessionRfbEvent = {
  type: "disconnected", reason: "rfb_remote_closed",
  failure: { code: "rfb_remote_closed", category: "remote_closed", retryable: true },
};

test("startup abort keeps native completion owned and releases late listeners without creating a Surface", async () => {
  for (const stage of ["before", "listener", "native"] as const) {
    const controller = new AbortController();
    let finishListener!: (unlisten: () => void) => void;
    const listener = new Promise<() => void>((resolve) => { finishListener = resolve; });
    let finishNative!: () => void;
    const native = new Promise<void>((resolve) => { finishNative = resolve; });
    let starts = 0;
    let surfaces = 0;
    let released = 0;
    const adapter = createVncProtocolAdapter({
      invoke: async (command) => {
        if (command === "connect_remote_session_rfb") { starts += 1; if (stage === "native") await native; }
        return {};
      },
      listenRfbEvent: async () => stage === "listener" ? listener : () => { released += 1; },
      createSurfaceSession: () => {
        surfaces += 1;
        return { canvas: {} as HTMLCanvasElement, disconnect() {} };
      },
    });
    if (stage === "before") controller.abort();
    const connecting = adapter.connect({
      target: {} as HTMLElement, host: "127.0.0.1", port: 5900,
      sessionId: "cancelled-start", surfaceUrl: "ws://127.0.0.1/surface",
      signal: controller.signal, connectionModel: createInitialConnectionModel(),
      onStatus() {}, onError() {}, onEvent() {},
    });
    const rejection = assert.rejects(connecting, /session_connect_cancelled/);
    void rejection.catch(() => undefined);
    let settled = false;
    const observed = connecting.then(() => { settled = true; }, () => { settled = true; });
    await new Promise<void>((resolve) => setImmediate(resolve));
    controller.abort();
    await new Promise<void>((resolve) => setImmediate(resolve));
    if (stage !== "before") assert.equal(settled, false);
    finishListener(() => { released += 1; });
    finishNative();
    await Promise.all([rejection, observed]);
    assert.equal(starts, stage === "native" ? 1 : 0);
    assert.equal(surfaces, 0);
    assert.equal(released, stage === "before" ? 0 : 1);
  }
});

test("a terminal event before native startup returns cannot recreate a connected Surface", async () => {
  let release!: () => void;
  const startup = new Promise<void>((resolve) => { release = resolve; });
  const fixture = setup({ nativeStartup: () => startup });
  const connecting = fixture.connect();
  await new Promise<void>((resolve) => setImmediate(resolve));
  fixture.emit(remoteClosed);
  release();
  const result = await connecting.then(() => "resolved", (error: Error) => error.message);
  assert.equal(result, "rfb_remote_closed");
  assert.equal(fixture.surfaceCreations, 0);
  assert.equal(fixture.unlistenCount, 1);
  assert.equal(fixture.issues.length, 0, "startup rejection is reported by the caller, not twice");
  assert.deepEqual(fixture.statuses, ["connecting"]);
});

test("late Surface commits and repeated terminal events cannot revive a failed connection", async () => {
  const fixture = setup({ autoPresent: false });
  const connection = await fixture.connect();
  fixture.emit(remoteClosed);
  fixture.emit(remoteClosed);
  fixture.present();
  const evidence = connection.getRuntimeEvidence?.();
  await fixture.adapter.disconnect(connection);
  assert.equal(fixture.issues.length, 1);
  assert.deepEqual(fixture.statuses, ["connecting", "disconnected unexpectedly"]);
  assert.equal(evidence, null);
  assert.equal(fixture.disconnected, true);
});

test("a returned connection stops input and ignores late commits after its first Surface failure", async () => {
  const fixture = setup({ autoPresent: false });
  const connection = await fixture.connect();
  connection.setInputEnabled(true);
  fixture.failSurface("surface_canvas_context_lost");
  const cleanup = { unlistenCount: fixture.unlistenCount, disconnected: fixture.disconnected };
  connection.setInputEnabled(true);
  connection.sendRemoteKeyEvent({ down: true, keysym: 0x61 });
  connection.refreshDisplay();
  connection.setQualityProfile("sharp");
  fixture.present();
  fixture.failSurface("surface_transport_failed");
  fixture.emit(remoteClosed);
  const evidence = connection.getRuntimeEvidence?.();
  const inputs = fixture.calls.filter((call) => call.command === "send_remote_session_rfb_input");
  await fixture.adapter.disconnect(connection);
  assert.deepEqual({ inputs: inputs.length, hasEvidence: evidence !== null, statuses: fixture.statuses }, {
    inputs: 0, hasEvidence: false, statuses: ["connecting"],
  });
  assert.deepEqual(fixture.errors, ["surface_canvas_context_lost"]);
  assert.equal(fixture.issues.length, 0);
  assert.deepEqual(cleanup, { unlistenCount: 1, disconnected: true });
  assert.equal(fixture.unlistenCount, 1);
  assert.equal(fixture.disconnected, true);
  assert.equal(fixture.calls.at(-1)?.command, "disconnect_remote_session_rfb");
});

test("a Surface failure during startup rejects with its original code and is reported by the caller", async () => {
  const fixture = setup({ surfaceStartupError: "surface_canvas_context_lost" });
  await assert.rejects(fixture.connect(), (error: Error) => error.constructor === Error
    && error.message === "surface_canvas_context_lost");
  assert.deepEqual(fixture.statuses, ["connecting"]);
  assert.deepEqual(fixture.errors, []);
  assert.equal(fixture.issues.length, 0);
  assert.equal(fixture.unlistenCount, 1);
  assert.equal(fixture.disconnected, true);
});

test("a synchronous Surface construction error releases the native event listener before rejecting", async () => {
  const fixture = setup({ surfaceStartupThrow: "surface_canvas_context_unavailable" });
  await assert.rejects(fixture.connect(), /surface_canvas_context_unavailable/);
  fixture.present();
  assert.deepEqual(fixture.statuses, ["connecting"]);
  assert.deepEqual(fixture.errors, []);
  assert.equal(fixture.issues.length, 0);
  assert.equal(fixture.unlistenCount, 1);
});

test("a pending input rejection after explicit disconnect cannot emit another failure", async () => {
  let rejectInput!: (error: unknown) => void;
  const input = new Promise<unknown>((_resolve, reject) => { rejectInput = reject; });
  const fixture = setup({ nativeInput: () => input });
  const connection = await fixture.connect();
  connection.refreshDisplay();
  await fixture.adapter.disconnect(connection);
  rejectInput({ code: "rfb_remote_closed", category: "remote_closed", retryable: true });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(fixture.issues.length, 0);
  assert.ok(!fixture.statuses.includes("disconnected unexpectedly"));
});

test("pending Agent discovery cannot restart clipboard polling after transport termination", async (context) => {
  context.mock.timers.enable({ apis: ["setInterval"] });
  let release!: (description: unknown) => void;
  const description = new Promise<unknown>((resolve) => { release = resolve; });
  const fixture = setup({ nativeAgentDescription: () => description });
  const connection = await fixture.connect();
  fixture.emit({
    type: "discoveryManifest",
    manifest: {
      version: "1.0", server: { id: "server-1", identityVerification: "server-discovery" },
      agent: { hostMode: "rfb-peer", port: 6200, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
    },
  });
  await new Promise<void>((resolve) => setImmediate(resolve));
  const publications = fixture.modelPublications;
  fixture.emit(remoteClosed);
  release({ serverId: "server-1", viewOnly: false, capabilities: ["clipboard.text.read"] });
  await new Promise<void>((resolve) => setImmediate(resolve));
  context.mock.timers.tick(1000);
  await new Promise<void>((resolve) => setImmediate(resolve));
  const latePublications = fixture.modelPublications;
  const reads = fixture.calls.filter((call) => call.command === "invoke_remote_agent"
    && (call.args.request as { type: string }).type === "clipboard.text.read");
  await fixture.adapter.disconnect(connection);
  assert.equal(latePublications, publications);
  assert.equal(reads.length, 0);
});

test("native RFB startup failure is terminal and does not start another transport", async () => {
  const fixture = setup({ nativeStartupError: { code: "rfb_protocol_error", category: "protocol_error", retryable: false } });
  await assert.rejects(fixture.connect(), /rfb_protocol_error/);
  assert.equal(fixture.calls.filter((call) => call.command === "connect_remote_session_rfb").length, 1);
  assert.deepEqual(fixture.calls.map((call) => call.command), ["connect_remote_session_rfb"]);
});

test("native RFB prompts once and retries when the Core requires credentials", async () => {
  const password = crypto.randomUUID();
  const requests: Array<{ password?: string }> = [];
  let promptCount = 0;
  const adapter = createVncProtocolAdapter({
    invoke: async (command, args) => {
      if (command !== "connect_remote_session_rfb") return {};
      const request = args.request as { password?: string };
      requests.push(request);
      if (requests.length === 1) throw { code: "rfb_credentials_required", category: "credentials_required", retryable: true };
      return {};
    },
    listenRfbEvent: async () => () => {},
    createSurfaceSession: () => ({
      canvas: { focus() {} } as HTMLCanvasElement,
      disconnect() {},
    }),
  });

  await adapter.connect({
    target: {} as HTMLElement,
    sessionId: "credential-session",
    surfaceUrl: "ws://127.0.0.1:61001/surface/credential-session/main?token=test",
    host: "127.0.0.1",
    port: 5900,
    connectionModel: createInitialConnectionModel(),
    onCredentialsRequired: async () => {
      promptCount += 1;
      return password;
    },
    onStatus() {}, onError() {}, onEvent() {},
  });

  assert.equal(promptCount, 1);
  assert.equal(requests.length, 2);
  assert.equal(requests[0]?.password, undefined);
  assert.equal(requests[1]?.password, password);
});

test("native RFB treats a cancelled credential prompt as authentication cancellation", async () => {
  let connectCalls = 0;
  const adapter = createVncProtocolAdapter({
    invoke: async (command) => {
      if (command === "connect_remote_session_rfb") {
        connectCalls += 1;
        throw { code: "rfb_credentials_required", category: "credentials_required", retryable: true };
      }
      return {};
    },
    listenRfbEvent: async () => () => {},
  });

  await assert.rejects(adapter.connect({
    target: {} as HTMLElement,
    sessionId: "cancelled-credential-session",
    surfaceUrl: "ws://127.0.0.1:61001/surface/cancelled-credential-session/main?token=test",
    host: "127.0.0.1",
    port: 5900,
    connectionModel: createInitialConnectionModel(),
    onCredentialsRequired: async () => undefined,
    onStatus() {}, onError() {}, onEvent() {},
  }), /rfb_authentication_cancelled/);
  assert.equal(connectCalls, 1);
});

test("refresh and quality negotiation use the native transport before input is enabled", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  connection.refreshDisplay();
  connection.setQualityProfile("sharp");
  await new Promise((resolve) => setTimeout(resolve, 0));
  const inputs = fixture.calls
    .filter((call) => call.command === "send_remote_session_rfb_input")
    .map((call) => (call.args.request as { input: unknown }).input);
  assert.deepEqual(inputs, [
    { type: "refresh", incremental: false },
    { type: "setQuality", qualityLevel: 9, compressionLevel: 0 },
  ]);
});

test("native RFB Discovery authenticates a user background Agent with its ticket", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  fixture.emit({
    type: "discoveryManifest",
    manifest: {
      version: "1.0",
      server: { id: "server-1", identityVerification: "server-discovery" },
      agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
    },
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(connection.connectionModel.enhancedChannel.status, "available");
  assert.equal(connection.connectionModel.serverIdentity.verification, "authenticated-agent");
  const describe = fixture.calls.find((call) => call.command === "invoke_remote_agent"
    && (call.args.request as { type: string }).type === "agent.session.describe");
  assert.deepEqual(describe?.args, { request: { host: "127.0.0.1", port: 6900,
    type: "agent.session.describe", sessionId: "remote-session-1" } });
});

test("Discovery failures revoke Agent identity and pending authentication without closing RFB", async () => {
  for (const pending of [false, true]) {
    for (const type of ["discoveryError", "discoveryFailure"] as const) {
      let release!: (description: unknown) => void;
      const description = new Promise<unknown>((resolve) => { release = resolve; });
      const fixture = setup({ remoteEndpoint: true, nativeAgentDescription: pending ? () => description : undefined });
      const connection = await fixture.connect();
      const ticket = crypto.randomUUID();
      fixture.emit({ type: "discoveryManifest", manifest: {
        version: "1.0", server: { id: "server-1" },
        agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: ticket },
      } });
      await new Promise<void>((resolve) => setImmediate(resolve));
      assert.equal(connection.connectionModel.enhancedChannel.status, pending ? "negotiating" : "available");
      fixture.emit(type === "discoveryError" ? { type, error: { message: ticket } } : { type, code: ticket });
      release({ serverId: "server-1", viewOnly: false, capabilities: ["file.transfer.upload"] });
      await new Promise<void>((resolve) => setImmediate(resolve));
      assert.equal(connection.connectionModel.serverIdentity.verification, "none");
      assert.equal(connection.connectionModel.serverIdentity.kind, "unknown");
      assert.equal(connection.connectionModel.enhancedChannel.status, "degraded");
      assert.equal(connection.hasAgentChannel?.(), false);
      assert.equal(connection.connectionModel.capabilities.items.some((item) => item.source === "agent"), false);
      assert.equal(JSON.stringify(fixture.messages).includes(ticket), false);
      assert.equal(fixture.disconnected, false);
      assert.equal(fixture.errors.length, 0);
      assert.equal(fixture.calls.some((call) => call.command === "disconnect_remote_session_rfb"), false);
      await fixture.adapter.disconnect(connection);
    }
  }
});

test("native Agent operations carry the authenticated session and disconnect the Rust TCP loop", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  fixture.emit({
    type: "discoveryManifest",
    manifest: {
      version: "1.0", server: { id: "server-1", identityVerification: "server-discovery" },
      agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
    },
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  await connection.revealRemoteFile("~/Downloads/example.txt");
  const operation = fixture.calls.find((call) => call.command === "invoke_remote_agent"
    && (call.args.request as { type: string }).type === "file.reveal");
  assert.deepEqual(operation?.args, {
    request: { host: "127.0.0.1", port: 6900, sessionId: "remote-session-1", type: "file.reveal", path: "~/Downloads/example.txt" },
  });
  assert.equal(JSON.stringify(operation?.args).includes("token"), false);
  await fixture.adapter.disconnect(connection);
  assert.equal(fixture.disconnected, true);
  assert.equal(fixture.calls.at(-1)?.command, "disconnect_remote_session_rfb");
});

test("closed attempts cannot advertise an Agent channel or start new enhanced actions", async () => {
  for (const terminal of ["manual", "native", "surface"] as const) {
    const fixture = setup();
    const connection = await fixture.connect();
    fixture.emit({
      type: "discoveryManifest",
      manifest: {
        version: "1.0", server: { id: "server-1", identityVerification: "server-discovery" },
        agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
      },
    });
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(connection.hasAgentChannel?.(), true, terminal);
    if (terminal === "manual") await fixture.adapter.disconnect(connection);
    else if (terminal === "native") fixture.emit(remoteClosed);
    else fixture.failSurface("surface_canvas_context_lost");
    assert.equal(connection.hasAgentChannel?.(), false, terminal);
    // The retained model describes negotiation history, not a live action gate.
    assert.equal(connection.connectionModel.enhancedChannel.status, "available");
    const before = { commands: fixture.calls.length, issues: fixture.issues.length, errors: fixture.errors.length };
    await assert.rejects(connection.revealRemoteFile("~/Downloads/example.txt"), /agent_channel_unavailable/);
    await assert.rejects(connection.uploadRemoteFile("/tmp/example.txt"), /agent_channel_unavailable/);
    assert.deepEqual({ commands: fixture.calls.length, issues: fixture.issues.length, errors: fixture.errors.length }, before);
    await fixture.adapter.disconnect(connection);
  }
});

test("remote Agent authentication uses the native Fence Discovery ticket", async () => {
  const fixture = setup();
  const connection = await fixture.connect();
  fixture.emit({
    type: "discoveryManifest",
    manifest: {
      version: "1.0",
      server: { id: "server-1", identityVerification: "server-discovery" },
      agent: {
        hostMode: "rfb-peer", port: 6200, protocolVersion: "1.0",
        authenticationMethods: ["agent-ticket"], bootstrapKey: "short-lived-ticket", expiresAt: 1_800_000_000,
      },
    },
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(connection.connectionModel.enhancedChannel.status, "available");
  assert.equal(connection.connectionModel.enhancedChannel.authentication, "verified");
  assert.equal(fixture.calls.some((call) => call.command === "describe_local_runtime_agent"), false);
  assert.equal(fixture.calls.filter((call) => call.command === "invoke_remote_agent").length, 2);
});


test("explicit session end only notifies authenticated supporting Agents and does not depend on client readonly", async () => {
  for (const supported of [true, false]) {
    const fixture = setup({ remoteEndpoint: true,
      nativeAgentDescription: async () => ({ serverId: "server-1", viewOnly: false, capabilities: supported ? ["remote.session.end"] : [] }),
    });
    const connection = await fixture.connect();
    await connection.notifyRemoteSessionEnd!();
    fixture.emit({ type: "discoveryManifest", manifest: {
      version: "1.0", server: { id: "server-1" },
      agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
    } });
    await new Promise<void>((resolve) => setImmediate(resolve));
    if (supported) {
      await connection.notifyRemoteSessionEnd!();
      assert.deepEqual(fixture.calls.at(-1)?.args.request, {
        host: "127.0.0.1", port: 6900, type: "remote.session.end", sessionId: "remote-session-1",
      });
      connection.setReadonlyMode(true);
      await connection.notifyRemoteSessionEnd!();
      connection.setReadonlyMode(false);
    } else {
      await connection.notifyRemoteSessionEnd!();
      assert.equal(fixture.calls.some((call) => (call.args.request as { type?: string } | undefined)?.type === "remote.session.end"), false);
    }
    await fixture.adapter.disconnect(connection);
    await assert.rejects(connection.notifyRemoteSessionEnd!(), /agent_channel_unavailable/);
  }
});

test("session end policy errors are reported instead of assuming the host locked", async () => {
  const fixture = setup({ remoteEndpoint: true,
    nativeAgentDescription: async () => ({ serverId: "server-1", viewOnly: false, capabilities: ["remote.session.end"] }),
    nativeAgentEnd: async () => ({ ok: false, error: "screen_lock_permission_missing" }),
  });
  const connection = await fixture.connect();
  fixture.emit({ type: "discoveryManifest", manifest: {
    version: "1.0", server: { id: "server-1" },
    agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
  } });
  await new Promise<void>((resolve) => setImmediate(resolve));
  await assert.rejects(connection.notifyRemoteSessionEnd!(), /screen_lock_permission_missing/);
  await fixture.adapter.disconnect(connection);
});

async function connectMessageFixture(response: () => Promise<unknown> = async () => ({ ok: true })) {
  const fixture = setup({ remoteEndpoint: true,
    nativeAgentDescription: async () => ({ serverId: "server-1", viewOnly: false, capabilities: ["message.show"] }),
    nativeAgentMessage: response,
  });
  const connection = await fixture.connect();
  fixture.emit({ type: "discoveryManifest", manifest: {
    version: "1.0", server: { id: "server-1" },
    agent: { hostMode: "rfb-peer", port: 6900, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID() },
  } });
  await new Promise<void>((resolve) => setImmediate(resolve));
  return { fixture, connection };
}

test("remote messages wait for receiver acceptance and do not use clipboard or keyboard input", async () => {
  let acknowledge!: (value: unknown) => void;
  const response = new Promise((resolve) => { acknowledge = resolve; });
  const { fixture, connection } = await connectMessageFixture(() => response);
  let delivered = false;
  const sending = connection.sendRemoteMessage("你好\n<script>plain text</script>").then(() => { delivered = true; });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(delivered, false);
  const request = fixture.calls.at(-1)?.args.request as { type: string; text: string };
  assert.equal(request.type, "message.show");
  assert.equal(request.text, "你好\n<script>plain text</script>");
  acknowledge({ ok: true });
  await sending;
  assert.equal(fixture.calls.filter((call) => call.command === "send_remote_session_rfb_input").length, 0);
  assert.equal(fixture.clipboardActivities.length, 0);
  assert.ok(!fixture.messages.some((message) => message.includes("plain text")));
  await fixture.adapter.disconnect(connection);
});

test("popup rejections and missing acknowledgements are failures, with no clipboard fallback", async () => {
  for (const response of [{ ok: false, error: "remote_message_busy" }, {}]) {
    const { fixture, connection } = await connectMessageFixture(async () => response);
    await assert.rejects(connection.sendRemoteMessage("hello"), /remote_message_busy|remote_message_display_failed/);
    assert.equal(fixture.calls.filter((call) => call.command === "send_remote_session_rfb_input").length, 0);
    assert.equal(fixture.clipboardActivities.length, 0);
    await fixture.adapter.disconnect(connection);
  }
});

test("messages reject unsupported peers, invalid text, readonly mode and closed sessions before dispatch", async () => {
  const unsupported = setup({ remoteEndpoint: true });
  const unsupportedConnection = await unsupported.connect();
  const beforeUnsupported = unsupported.calls.length;
  await assert.rejects(unsupportedConnection.sendRemoteMessage("hello"), /remote_message_unsupported/);
  assert.equal(unsupported.calls.length, beforeUnsupported);
  await unsupported.adapter.disconnect(unsupportedConnection);
  const { fixture, connection } = await connectMessageFixture();
  const before = fixture.calls.length;
  for (const text of [" \n", "hi\0there", "你".repeat(6000)]) {
    await assert.rejects(connection.sendRemoteMessage(text), /remote_message_empty|remote_message_invalid|remote_message_too_large/);
  }
  connection.setReadonlyMode(true);
  await assert.rejects(connection.sendRemoteMessage("hello"), /agent_view_only/);
  connection.setReadonlyMode(false);
  fixture.emit({ type: "serverRuntime", notice: { kind: "desktop", desktop: "unavailable" } });
  await assert.rejects(connection.sendRemoteMessage("hello"), /agent_user_desktop_unavailable/);
  assert.equal(fixture.calls.length, before);
  await fixture.adapter.disconnect(connection);
  await assert.rejects(connection.sendRemoteMessage("hello"), /remote_message_disconnected/);
});
