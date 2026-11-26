import assert from "node:assert/strict";
import test from "node:test";
import { ref } from "vue";
import { createHostConnectionActions } from "../../../src/app/composables/vncAppState/hostConnection.ts";
import { createHostRecord } from "../../../src/hosts/hostManager.ts";
import type { ConnectVncOptions, WorkspaceMode } from "../../../src/app/types.ts";
import { createPasswordPromptController } from "../../../src/app/composables/vncAppState/passwordPrompt.ts";
import type { PasswordPromptState } from "../../../src/app/types.ts";

test("host actions pass an explicit endpoint without a form credential and keep credential prompts on the original record", async () => {
  const requests: Array<{ id: string | undefined; options: ConnectVncOptions | undefined }> = [];
  const prompts: string[] = [];
  let finishLookup!: (value: undefined) => void;
  const lookup = new Promise<undefined>((resolve) => { finishLookup = resolve; });
  const options: Parameters<typeof createHostConnectionActions>[0] = {
    activeWorkspace: ref<WorkspaceMode>("hosts"),
    connectVnc: async (password, id, request) => {
      assert.equal(password, "");
      requests.push({ id, options: request });
    },
    host: ref("127.0.0.2"), port: ref(5901), password: ref(""),
    openHostConnectionConfirm() {},
    openTemporaryConnectionConfirm: async () => {},
    requestSessionPassword: async (target) => { prompts.push(target); return undefined; },
    resolveSelectedHostPassword: async () => lookup,
    sessionSettings: ref({ connectionDefaults: { postConnectPresentation: "main" as const } }),
    showAppMessage() {}, t: (key) => key,
  };
  const actions = createHostConnectionActions(options);
  const host = createHostRecord({
    id: "host-1", name: "Original host", address: "127.0.0.1", port: 5900,
    protocol: "vnc",
  });
  await actions.connectSelectedHostRecord(host);
  assert.deepEqual(requests, [{ id: "host-1", options: {
    endpoint: { host: "127.0.0.1", port: 5900 }, savedHostName: "Original host", postConnectPresentation: "main",
  } }]);

  host.credentialRef = "credential-reference";
  const pending = actions.connectSelectedHostRecord(host);
  host.address = "127.0.0.2";
  host.port = 5901;
  host.target = "127.0.0.2:5901";
  host.id = "host-2";
  options.host.value = "127.0.0.2";
  options.port.value = 5901;
  finishLookup(undefined);
  await pending;
  assert.deepEqual(prompts, ["127.0.0.1:5900"]);
  assert.equal(requests.length, 1, "cancelled credential prompt does not start a connection");
});

test("saved-host quick connections inherit presentation defaults and honor an explicit override", async () => {
  const requests: Array<ConnectVncOptions | undefined> = [];
  const options: Parameters<typeof createHostConnectionActions>[0] = {
    activeWorkspace: ref<WorkspaceMode>("hosts"), host: ref(""), port: ref(5900), password: ref(""),
    connectVnc: async (_password, _id, request) => { requests.push(request); },
    openHostConnectionConfirm() {}, openTemporaryConnectionConfirm: async () => {},
    requestSessionPassword: async () => undefined, resolveSelectedHostPassword: async () => undefined,
    sessionSettings: ref({ connectionDefaults: { postConnectPresentation: "external" } }),
    showAppMessage() {}, t: key => key,
  };
  const host = createHostRecord({ id: "one", name: "Host", address: "127.0.0.1", port: 5900, protocol: "vnc", status: "needs_probe" });
  const actions = createHostConnectionActions(options);
  await actions.connectSelectedHostRecord(host);
  await actions.connectSelectedHostRecord(host, { postConnectPresentation: "main" });
  assert.deepEqual(requests.map(request => request?.postConnectPresentation), ["external", "main"]);
  assert.deepEqual(requests.map(request => request?.savedHostName), ["Host", "Host"]);
});

test("filling a saved host opens the temporary form without starting a connection", async () => {
  let opened = 0;
  const options: Parameters<typeof createHostConnectionActions>[0] = {
    activeWorkspace: ref<WorkspaceMode>("hosts"),
    connectVnc: async () => { assert.fail("filling the form must not connect"); },
    host: ref(""), port: ref(5900), password: ref(""),
    openHostConnectionConfirm() { assert.fail("filling is not a saved-host quick connection"); },
    openTemporaryConnectionConfirm: async () => {
      opened += 1;
      assert.equal(options.host.value, "127.0.0.2");
      assert.equal(options.port.value, 5901);
      assert.equal(options.activeWorkspace.value, "sessions");
    },
    requestSessionPassword: async () => undefined,
    resolveSelectedHostPassword: async () => undefined,
    sessionSettings: ref({ connectionDefaults: { postConnectPresentation: "main" as const } }),
    showAppMessage() {}, t: (key) => key,
  };
  const host = createHostRecord({
    id: "host-1", name: "Saved host", address: "127.0.0.2", port: 5901,
    protocol: "vnc",
  });
  await createHostConnectionActions(options).fillSelectedHostConnectionForm(host);
  assert.equal(opened, 1);
});

test("a refused saved credential opens the password dialog and explicit cancellation stays quiet", async () => {
  const promptState = ref<PasswordPromptState>({ visible: false, target: "", password: "" });
  const prompt = createPasswordPromptController({
    passwordPrompt: promptState, t: key => key,
    notify() { assert.fail("cancelling or entering a password needs no warning"); },
  });
  const messages: Array<{ content: string; tone: string | undefined }> = [];
  let connections = 0;
  const options: Parameters<typeof createHostConnectionActions>[0] = {
    activeWorkspace: ref<WorkspaceMode>("hosts"),
    connectVnc: async () => { connections++; },
    host: ref(""), port: ref(5900), password: ref(""),
    openHostConnectionConfirm() {}, openTemporaryConnectionConfirm: async () => {},
    requestSessionPassword: prompt.requestSessionPassword,
    resolveSelectedHostPassword: async () => undefined,
    sessionSettings: ref({ connectionDefaults: { postConnectPresentation: "main" as const } }),
    showAppMessage: (content, tone) => { messages.push({ content, tone }); }, t: key => key,
  };
  const host = createHostRecord({
    id: "host-1", name: "Saved host", address: "192.0.2.1", port: 5900,
    protocol: "vnc", credentialRef: "credential-reference",
  });
  const actions = createHostConnectionActions(options);
  const cancelled = actions.connectSelectedHostRecord(host);
  await Promise.resolve();
  assert.equal(promptState.value.visible, true);
  assert.equal(promptState.value.target, host.target);
  assert.deepEqual(messages, []);
  prompt.cancelPasswordPrompt();
  prompt.cancelPasswordPrompt();
  await cancelled;
  assert.deepEqual(messages, []);
  assert.equal(connections, 0);

  messages.length = 0;
  const connecting = actions.connectSelectedHostRecord(host);
  await Promise.resolve();
  promptState.value.password = crypto.randomUUID();
  prompt.submitPasswordPrompt();
  await connecting;
  assert.equal(connections, 1);
  assert.deepEqual(messages, []);
  assert.equal(promptState.value.password, "");
  assert.equal(options.password.value, "");
});
