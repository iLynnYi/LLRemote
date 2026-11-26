import assert from "node:assert/strict";
import test from "node:test";

import { createInitialConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";
import { createNativeAgentRuntime } from "../../../src/session/protocol/protocolAdapter/nativeAgentRuntime.ts";
import type {
  AgentFileUploadProgress,
  AgentFileUploadResult,
  InvokeCommand,
} from "../../../src/session/protocol/protocolAdapter/types.ts";
import type { ServerDiscoveryManifest } from "../../../src/session/discovery/serverDiscovery.ts";

type RuntimeOptions = Parameters<typeof createNativeAgentRuntime>[0];

function createRuntime(options: Pick<RuntimeOptions, "invokeCommand" | "listenUploadProgress"> & Partial<Pick<RuntimeOptions, "initialConnectionModel">>) {
  return createNativeAgentRuntime({
    ...options,
    host: "127.0.0.1",
    vncPort: 5900,
    initialConnectionModel: options.initialConnectionModel ?? {
      ...createInitialConnectionModel({ state: "matched", viewOnly: false }),
      enhancedChannel: {
        status: "available", authentication: "verified",
        endpoint: { hostMode: "rfb-peer", port: 6200, source: "local-runtime" },
      },
    },
    onEvent() {},
  });
}

function manifest(port?: number): ServerDiscoveryManifest {
  return {
    version: "1.0",
    server: { id: "managed-server", identityVerification: "server-discovery" },
    policy: { viewOnly: false },
    ...(port ? { agent: {
      hostMode: "rfb-peer" as const, port, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: crypto.randomUUID(),
    } } : {}),
  };
}

test("withdrawing an Agent invalidates a pending authentication without changing RFB", async () => {
  let finishDescription!: (value: unknown) => void;
  const description = new Promise((resolve) => { finishDescription = resolve; });
  const runtime = createRuntime({ invokeCommand: async (_command, args) => {
    const request = args.request as { type: string };
    return request.type === "agent.session.open" ? { sessionId: "test-session" } : { description: await description };
  } });
  const authenticating = runtime.consumeDiscovery(manifest(6200));
  await runtime.consumeDiscovery(manifest());
  finishDescription({ serverId: "managed-server", viewOnly: false, capabilities: ["file.open"] });
  await authenticating;
  assert.equal(runtime.connectionModel.enhancedChannel.status, "unavailable");
  assert.equal(runtime.connectionModel.capabilities.items.some(item => item.id === "file.open"), false);
  runtime.dispose();
});

test("a late description cannot replace the newest Agent endpoint capabilities", async () => {
  let finishOldDescription!: (value: unknown) => void;
  const oldDescription = new Promise((resolve) => { finishOldDescription = resolve; });
  const runtime = createRuntime({ invokeCommand: async (_command, args) => {
    const request = args.request as { port: number; type: string };
    if (request.type === "agent.session.open") return { sessionId: "test-session" };
    return { description: request.port === 6200 ? await oldDescription : {
      serverId: "managed-server", viewOnly: false, capabilities: ["file.reveal"],
    } };
  } });
  const oldAttempt = runtime.consumeDiscovery(manifest(6200));
  await runtime.consumeDiscovery(manifest(6201));
  finishOldDescription({ serverId: "managed-server", viewOnly: false, capabilities: ["file.open"] });
  await oldAttempt;
  assert.equal(runtime.connectionModel.enhancedChannel.endpoint?.port, 6201);
  assert.equal(runtime.connectionModel.capabilities.items.some(item => item.id === "file.open"), false);
  assert.equal(runtime.connectionModel.capabilities.items.some(item => item.id === "file.reveal"), true);
  runtime.dispose();
});

test("disposed Agent runtimes reject new actions before native calls or upload listener registration", async () => {
  const calls: string[] = [];
  let registrations = 0;
  const runtime = createRuntime({
    invokeCommand: async (command) => { calls.push(command); return {}; },
    listenUploadProgress: async () => { registrations += 1; return () => {}; },
  });
  runtime.dispose();
  const errors = await Promise.all([
    runtime.invoke("file.reveal", { path: "example.txt" }),
    runtime.upload("example.txt", { transferId: "disposed-transfer" }),
  ].map((action) => action.then(() => null, (error: Error) => error.message)));
  assert.deepEqual({ errors, calls, registrations }, {
    errors: ["agent_channel_unavailable", "agent_channel_unavailable"], calls: [], registrations: 0,
  });
});

test("disposal during upload listener registration blocks a new transfer while in-flight uploads still finish", async () => {
  const calls: string[] = [];
  let finishRegistration!: (unlisten: () => void) => void;
  let unlistenCount = 0;
  const registration = new Promise<() => void>((resolve) => { finishRegistration = resolve; });
  const runtime = createRuntime({
    invokeCommand: async (command) => { calls.push(command); return {}; },
    listenUploadProgress: () => registration,
  });
  const upload = runtime.upload("example.txt", { transferId: "pending-transfer" });
  runtime.dispose();
  finishRegistration(() => { unlistenCount += 1; });
  const error = await upload.then(() => null, (failure: Error) => failure.message);
  assert.deepEqual({ error, calls, unlistenCount }, {
    error: "agent_channel_unavailable", calls: [], unlistenCount: 1,
  });

  let finishUpload!: (result: AgentFileUploadResult) => void;
  let emitProgress: ((progress: AgentFileUploadProgress) => void) | undefined;
  let runningUnlistenCount = 0;
  const progress: AgentFileUploadProgress[] = [];
  const nativeUpload = new Promise<AgentFileUploadResult>((resolve) => { finishUpload = resolve; });
  const runningCalls: Array<Parameters<InvokeCommand>[0]> = [];
  const runningRuntime = createRuntime({
    invokeCommand: async (command, args) => {
      if (command === "invoke_remote_agent") {
        const request = args.request as { type: string };
        return request.type === "agent.session.open" ? { sessionId: "test-session" }
          : { description: { serverId: "managed-server", viewOnly: false, capabilities: ["file.transfer.upload"] } };
      }
      runningCalls.push(command); return nativeUpload;
    },
    listenUploadProgress: async (handler) => {
      emitProgress = handler;
      return () => { runningUnlistenCount += 1; };
    },
  });
  await runningRuntime.consumeDiscovery(manifest(6200));
  const runningUpload = runningRuntime.upload("example.txt", {
    transferId: "running-transfer", onProgress: (update) => { progress.push(update); },
  });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.deepEqual(runningCalls, ["upload_remote_agent_file"]);
  runningRuntime.dispose();
  const update = { transferId: "running-transfer", fileName: "example.txt", transferredBytes: 8, totalBytes: 8 };
  emitProgress?.(update);
  const result = { fileName: "example.txt", remotePath: "example.txt", bytesWritten: 8, sha256: "test-digest" };
  finishUpload(result);
  assert.deepEqual(await runningUpload, result);
  assert.deepEqual(progress, [update]);
  assert.equal(runningUnlistenCount, 1);
});

test("directory uploads forward relative paths and refuse receivers that would flatten them", async () => {
  for (const supportsRelativePaths of [true, false]) {
    const uploads: unknown[] = [];
    const runtime = createRuntime({
      invokeCommand: async (command, args) => {
        if (command === "upload_remote_agent_file") { uploads.push(args.request); return {}; }
        const request = args.request as { type: string };
        return request.type === "agent.session.open" ? { sessionId: "test-session" }
          : { description: { serverId: "managed-server", viewOnly: false, capabilities: [
            "file.transfer.upload", ...(supportsRelativePaths ? ["file.transfer.relative-path"] : []),
          ] } };
      },
      listenUploadProgress: async () => () => {},
    });
    await runtime.consumeDiscovery(manifest(6200));
    const upload = runtime.upload("/local/Project/src/main.ts", {
      transferId: "directory-transfer", relativePath: "Project/src/main.ts", targetDirectory: "/receive",
    });
    if (supportsRelativePaths) {
      await upload;
      assert.deepEqual(uploads, [{
        host: "127.0.0.1", port: 6200, sessionId: "test-session",
        filePath: "/local/Project/src/main.ts", transferId: "directory-transfer",
        targetDirectory: "/receive", relativePath: "Project/src/main.ts", renamePolicy: undefined,
      }]);
    } else {
      await assert.rejects(upload, /agent_file_directory_unsupported/);
      assert.equal(uploads.length, 0);
      await runtime.upload("/local/main.ts", { transferId: "single-file" });
      assert.equal(uploads.length, 1);
    }
    runtime.dispose();
  }
});

test("clipboard polling keeps only one request in flight and discards replies after disposal", async (context) => {
  context.mock.timers.enable({ apis: ["setInterval"] });
  let reads = 0;
  let finishRead!: (response: unknown) => void;
  const pending = new Promise((resolve) => { finishRead = resolve; });
  const clipboard: string[] = [];
  const runtime = createNativeAgentRuntime({
    host: "127.0.0.1", vncPort: 5900,
    initialConnectionModel: createInitialConnectionModel(),
    onEvent() {}, onRemoteClipboardText(text) { clipboard.push(text); },
    invokeCommand: async (_command, args) => {
      const request = args.request as { type: string };
      if (request.type === "agent.session.open") return { sessionId: "test-session" };
      if (request.type === "agent.session.describe") return {
        description: { serverId: "managed-server", viewOnly: false, capabilities: ["clipboard.text.read"] },
      };
      reads += 1; return pending;
    },
  });
  await runtime.consumeDiscovery(manifest(6200));
  context.mock.timers.tick(5000);
  assert.equal(reads, 1);
  runtime.dispose();
  finishRead({ text: "late clipboard", revision: "late" });
  await new Promise<void>((resolve) => setImmediate(resolve));
  context.mock.timers.tick(5000);
  assert.deepEqual(clipboard, []);
  assert.equal(reads, 1);
});

test("an in-flight Agent action rejects a result from a withdrawn channel", async () => {
  let finishAction!: (response: unknown) => void;
  const pending = new Promise((resolve) => { finishAction = resolve; });
  const runtime = createRuntime({ invokeCommand: async (_command, args) => {
    const request = args.request as { type: string };
    if (request.type === "agent.session.open") return { sessionId: "test-session" };
    if (request.type === "agent.session.describe") return {
      description: { serverId: "managed-server", viewOnly: false, capabilities: ["file.reveal"] },
    };
    return pending;
  } });
  await runtime.consumeDiscovery(manifest(6200));
  const action = runtime.invoke("file.reveal", { path: "example.txt" });
  await runtime.consumeDiscovery(manifest());
  finishAction({ ok: true });
  await assert.rejects(action, /agent_channel_unavailable/);
  runtime.dispose();
});

test("clipboard polling resumes after a failed read settles", async (context) => {
  context.mock.timers.enable({ apis: ["setInterval"] });
  let reads = 0;
  let failRead!: (reason: Error) => void;
  const pending = new Promise((_resolve, reject) => { failRead = reject; });
  const clipboard: string[] = [];
  const runtime = createNativeAgentRuntime({
    host: "127.0.0.1", vncPort: 5900,
    initialConnectionModel: createInitialConnectionModel(),
    onEvent() {}, onRemoteClipboardText(text) { clipboard.push(text); },
    invokeCommand: async (_command, args) => {
      const request = args.request as { type: string };
      if (request.type === "agent.session.open") return { sessionId: "test-session" };
      if (request.type === "agent.session.describe") return {
        description: { serverId: "managed-server", viewOnly: false, capabilities: ["clipboard.text.read"] },
      };
      reads += 1;
      return reads === 1 ? pending : { text: "recovered", revision: "2" };
    },
  });
  try {
    await runtime.consumeDiscovery(manifest(6200));
    context.mock.timers.tick(1000);
    failRead(new Error("synthetic timeout"));
    await new Promise<void>((resolve) => setImmediate(resolve));
    context.mock.timers.tick(1000);
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(reads, 2);
    assert.deepEqual(clipboard, ["recovered"]);
  } finally {
    runtime.dispose();
  }
});

test("remote identity stays unverified during authentication and is promoted after matching describe", async () => {
  let finishOpen!: (value: unknown) => void;
  const opened = new Promise((resolve) => { finishOpen = resolve; });
  const runtime = createRuntime({ initialConnectionModel: createInitialConnectionModel(),
    invokeCommand: async (_command, args) => (args.request as { type: string }).type === "agent.session.open"
      ? opened : { description: { serverId: "managed-server", viewOnly: false, capabilities: ["file.open"] } },
  });
  try {
    const pending = runtime.consumeDiscovery(manifest(6200));
    assert.equal(runtime.connectionModel.serverIdentity.kind, "unknown");
    assert.equal(runtime.connectionModel.enhancedChannel.status, "negotiating");
    finishOpen({ sessionId: crypto.randomUUID() });
    await pending;
    assert.equal(runtime.connectionModel.serverIdentity.kind, "managed");
    assert.equal(runtime.connectionModel.serverIdentity.verification, "authenticated-agent");
    await runtime.consumeDiscovery(manifest());
    assert.equal(runtime.connectionModel.serverIdentity.kind, "unknown");
    assert.equal(runtime.connectionModel.baseTransport.kind, "vnc");
  } finally { runtime.dispose(); }
});

test("a rejected Agent authentication never promotes a remote Discovery claim", async () => {
  const runtime = createRuntime({ initialConnectionModel: createInitialConnectionModel(),
    invokeCommand: async () => { throw new Error("agent_auth_failed"); },
  });
  try {
    await runtime.consumeDiscovery(manifest(6200));
    assert.equal(runtime.connectionModel.serverIdentity.kind, "unknown");
    assert.equal(runtime.connectionModel.serverIdentity.verification, "none");
    assert.equal(runtime.connectionModel.enhancedChannel.authentication, "failed");
    assert.equal(runtime.connectionModel.baseTransport.kind, "vnc");
  } finally { runtime.dispose(); }
});

test("malformed Discovery input resets enhancements without native requests or RFB failure", async () => {
  let calls = 0;
  const runtime = createRuntime({ initialConnectionModel: createInitialConnectionModel(),
    invokeCommand: async () => { calls += 1; return {}; },
  });
  try {
    for (const value of [null, [], {}, { version: "1.0", server: { id: "" } },
      { ...manifest(6200), agent: { ...manifest(6200).agent, port: "6200" } }]) {
      await runtime.consumeDiscovery(value);
      assert.equal(runtime.connectionModel.serverIdentity.kind, "unknown");
      assert.equal(runtime.connectionModel.enhancedChannel.lastErrorCode, "server_discovery_invalid_manifest");
      assert.equal(runtime.connectionModel.baseTransport.kind, "vnc");
    }
    assert.equal(calls, 0);
  } finally { runtime.dispose(); }
});

test("opaque remote authentication errors cannot echo bootstrap credentials into the model or diagnostics", async () => {
  for (const thrown of [false, true]) {
    const key = crypto.randomUUID();
    const messages: string[] = [];
    const runtime = createNativeAgentRuntime({ host: "127.0.0.1", vncPort: 5900,
      initialConnectionModel: createInitialConnectionModel(),
      invokeCommand: async () => {
        if (thrown) throw new Error(`rejected ${key}`);
        return { error: `rejected ${key}` };
      },
      onEvent(_source, message) { messages.push(message); },
    });
    try {
      const value = manifest(6200);
      value.agent!.bootstrapKey = key;
      await runtime.consumeDiscovery(value);
      assert.equal(runtime.connectionModel.enhancedChannel.lastErrorCode, thrown ? "agent_authentication_failed" : "agent_session_open_failed");
      assert.equal(JSON.stringify(runtime.connectionModel).includes(key), false);
      assert.ok(messages.every((message) => !message.includes(key)));
    } finally { runtime.dispose(); }
  }
});

test("unsupported Agent protocol versions leave base VNC available without a native request", async () => {
  let calls = 0;
  const runtime = createRuntime({ initialConnectionModel: createInitialConnectionModel(),
    invokeCommand: async () => { calls += 1; return {}; },
  });
  try {
    const value = manifest(6200);
    value.agent!.protocolVersion = "2.0";
    await runtime.consumeDiscovery(value);
    assert.equal(calls, 0);
    assert.equal(runtime.connectionModel.enhancedChannel.lastErrorCode, "agent_protocol_version_unsupported");
    assert.equal(runtime.connectionModel.serverIdentity.kind, "unknown");
    assert.equal(runtime.connectionModel.baseTransport.kind, "vnc");
  } finally { runtime.dispose(); }
});
