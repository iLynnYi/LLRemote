import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { ref } from "vue";
import type { ConnectVncOptions, WorkspaceMode } from "../../../src/app/types.ts";
import { createHostRecord } from "../../../src/hosts/hostManager.ts";
import { useConnectionConfirmFlow } from "../../../src/features/sessions/composables/useConnectionConfirmFlow.ts";
import { createSessionSettings } from "../../../src/session/settings/sessionSettings.ts";
import {
  createHostConnectionConfirmDraft,
  createTemporaryConnectionConfirmDraft,
  toTemporaryHostCreateRequest,
  type ConnectionConfirmDraft,
} from "../../../src/session/core/connectionConfirm.ts";

function connectionFlow(confirmBeforeConnect = false) {
  const hostRecord = createHostRecord({
    id: "saved-host", name: "Saved host", address: "127.0.0.2", port: 5901,
    protocol: "vnc",
  });
  const requests: Array<{ source: string; host: string; port: number; options?: ConnectVncOptions }> = [];
  const saveRequests: ConnectionConfirmDraft[] = [];
  const state = {
    activeWorkspace: ref<WorkspaceMode>("hosts"),
    host: ref("127.0.0.1"), port: ref(5900), password: ref(""),
    hostRecords: ref([hostRecord]),
    sessionSettings: ref(createSessionSettings({
      confirmBeforeConnect, connectionDefaults: { saveTemporaryHost: true },
    })),
  };
  const flow = useConnectionConfirmFlow({
    ...state,
    connectSelectedHostRecord: async (record, options) => {
      requests.push({ source: "saved-host", host: record.address, port: record.port, options });
    },
    connectVnc: async (_password, _id, options) => {
      requests.push({ source: "temporary", host: state.host.value, port: state.port.value, options });
    },
    saveTemporaryConnectionHost: async (draft) => { saveRequests.push(draft); return undefined; },
  });
  return { ...state, ...flow, hostRecord, requests, saveRequests };
}

test("createHostConnectionConfirmDraft keeps saved-host metadata and credential status", () => {
  const host = createHostRecord({
    id: "host-1",
    name: "ops mac",
    address: "192.168.10.21",
    port: 5900,
    protocol: "vnc",
    group: "研发机房",
    credentialRef: "keychain:vnc:ops-mac",
  });

  assert.deepEqual(createHostConnectionConfirmDraft(host), {
    source: "saved-host",
    hostRecordId: "host-1",
    name: "ops mac",
    host: "192.168.10.21",
    port: 5900,
    target: "192.168.10.21:5900",
    protocol: "vnc",

    password: "",
    credentialStatus: "saved",
    options: {
      postConnectPresentation: "main",
      readonly: false,
      saveTemporaryHost: false,
    },
  });
});

test("createTemporaryConnectionConfirmDraft marks manual passwords as temporary and does not save by default", () => {
  assert.deepEqual(
    createTemporaryConnectionConfirmDraft({
      host: "10.0.0.8",
      port: 5901,

      password: "manual-password",
    }),
    {
      source: "temporary",
      name: "10.0.0.8:5901",
      host: "10.0.0.8",
      port: 5901,
      target: "10.0.0.8:5901",
      protocol: "vnc",


      password: "manual-password",
      credentialStatus: "temporary",
      options: {
        postConnectPresentation: "main",
        readonly: false,
        saveTemporaryHost: false,
      },
    },
  );
});

test("connection confirmation drafts can use settings-backed option defaults", () => {
  const draft = createTemporaryConnectionConfirmDraft(
    {
      host: "10.0.0.8",
      port: 5901,
    },
    {
      readonly: true,
      postConnectPresentation: "external",
      saveTemporaryHost: true,
    },
  );

  assert.deepEqual(draft.options, {
    postConnectPresentation: "external",
    readonly: true,
    saveTemporaryHost: true,
  });
});

test("connection confirmation dialog exposes one post-connect presentation choice", () => {
  const dialogSource = readFileSync(
    new URL("../../../src/app/components/ConnectionDialog.vue", import.meta.url),
    "utf8",
  );
  const zh = readFileSync(
    new URL("../../../src/i18n/messages/zh-CN.ts", import.meta.url),
    "utf8",
  );

  assert.match(dialogSource, /connectionConfirmDraft\.options\.postConnectPresentation/);
  assert.match(dialogSource, /NRadioGroup/);
  assert.match(dialogSource, /NRadio/);
  assert.match(dialogSource, /class="connection-option-label"/);
  assert.doesNotMatch(dialogSource, /NRadioButton/);
  assert.doesNotMatch(dialogSource, /options\.fullscreenAfterConnect/);
  assert.doesNotMatch(dialogSource, /options\.openInExternalWindowAfterConnect/);
  assert.match(zh, /postConnectPresentation: "连接后展示方式"/);
  assert.match(zh, /external: "独立窗口"/);
});

test("connection confirmation dialog removes duplicate preview cards and groups controls", () => {
  const dialogSource = readFileSync(
    new URL("../../../src/app/components/ConnectionDialog.vue", import.meta.url),
    "utf8",
  );
  const styles = readFileSync(
    new URL("../../../src/app/styles/dialogs.css", import.meta.url),
    "utf8",
  );
  const zh = readFileSync(
    new URL("../../../src/i18n/messages/zh-CN.ts", import.meta.url),
    "utf8",
  );

  assert.doesNotMatch(dialogSource, /class="connection-summary"/);
  assert.doesNotMatch(dialogSource, /<dl/);
  assert.doesNotMatch(dialogSource, /NTag/);
  assert.match(dialogSource, /class="connection-section"/);
  assert.match(dialogSource, /<details class="connection-more-options">[\s\S]*sessions\.moreOptions[\s\S]*connectionConfirmDraft\.options\.readonly/);
  assert.match(dialogSource, /v-if="connectionConfirmDraft\.options\.saveTemporaryHost"[\s\S]*connectionConfirmDraft\.name/);
  assert.match(dialogSource, /class="connection-host-port-row"/);
  assert.doesNotMatch(dialogSource, /:show-button="false"/);
  assert.match(styles, /\.connection-host-port-row/);
  assert.match(styles, /\.connection-option-choice \{[\s\S]*display: flex;[\s\S]*align-items: center;/);
  assert.match(styles, /\.connection-option-label/);
  assert.match(zh, /moreOptions: "更多选项"/);
});

test("connection confirmation dialog keeps the cancel action visually secondary", () => {
  const dialogSource = readFileSync(
    new URL("../../../src/app/components/ConnectionDialog.vue", import.meta.url),
    "utf8",
  );

  assert.match(dialogSource, /<NButton quaternary @click="closeConnectionConfirm">/);
  assert.match(dialogSource, /<NButton attr-type="submit" type="primary">/);
});

test("toTemporaryHostCreateRequest saves temporary hosts and only stores non-empty passwords", () => {
  const draft = createTemporaryConnectionConfirmDraft({
    host: "10.0.0.8",
    port: 5901,
    password: "manual-password",
  });
  draft.options.saveTemporaryHost = true;

  assert.deepEqual(toTemporaryHostCreateRequest(draft), {
    name: "10.0.0.8:5901",
    address: "10.0.0.8",
    port: 5901,
    protocol: "vnc",
    group: "未分组",
    savePassword: true,
    password: "manual-password",
  });

  draft.password = "";
  assert.deepEqual(toTemporaryHostCreateRequest(draft), {
    name: "10.0.0.8:5901",
    address: "10.0.0.8",
    port: 5901,
    protocol: "vnc",
    group: "未分组",
    savePassword: false,
    password: undefined,
  });
});

test("toTemporaryHostCreateRequest returns undefined when save is not requested or source is saved-host", () => {
  const temporary = createTemporaryConnectionConfirmDraft({
    host: "10.0.0.8",
    port: 5901,
  });
  assert.equal(toTemporaryHostCreateRequest(temporary), undefined);

  const saved = createHostConnectionConfirmDraft(
    createHostRecord({
      id: "host-1",
      name: "saved",
      address: "10.0.0.9",
      port: 5900,
      protocol: "vnc",
    }),
  );
  saved.options.saveTemporaryHost = true;
  assert.equal(toTemporaryHostCreateRequest(saved), undefined);
});




test("new temporary connections always open a form and require an explicit save choice", async () => {
  for (const confirmBeforeConnect of [false, true]) {
    const flow = connectionFlow(confirmBeforeConnect);
    await flow.openTemporaryConnectionConfirm();
    assert.equal(flow.activeWorkspace.value, "sessions");
    assert.equal(flow.connectionConfirmDraft.value?.source, "temporary");
    assert.equal(flow.connectionConfirmDraft.value?.name, "");
    assert.equal(flow.connectionConfirmDraft.value?.options.saveTemporaryHost, false);
    assert.equal(flow.requests.length, 0);
    assert.equal(flow.saveRequests.length, 0);

    flow.connectionConfirmDraft.value!.options.saveTemporaryHost = true;
    flow.closeConnectionConfirm();
    assert.equal(flow.connectionConfirmDraft.value, null);
    await flow.openTemporaryConnectionConfirm();
    assert.equal(flow.connectionConfirmDraft.value?.options.saveTemporaryHost, false);
    assert.equal(flow.requests.length, 0, "cancelling does not connect");
    assert.equal(flow.saveRequests.length, 0, "cancelling does not save a host");
  }
});

test("submitting the temporary form starts one connection with its edited target and options", async () => {
  const flow = connectionFlow();
  await flow.openTemporaryConnectionConfirm();
  const draft = flow.connectionConfirmDraft.value!;
  draft.host = "127.0.0.3";
  draft.port = 5902;
  draft.options.readonly = true;
  draft.options.postConnectPresentation = "external";

  await flow.submitConnectionConfirm();
  await flow.submitConnectionConfirm();
  assert.equal(flow.connectionConfirmDraft.value, null);
  assert.equal(flow.saveRequests.length, 1);
  assert.deepEqual(flow.requests, [{
    source: "temporary", host: "127.0.0.3", port: 5902,
    options: {
      readonly: true, postConnectPresentation: "external", saveTemporaryHost: false,
    },
  }]);
});

test("saved-host connections continue to respect the confirmation setting", async () => {
  for (const confirmBeforeConnect of [false, true]) {
    const flow = connectionFlow(confirmBeforeConnect);
    await flow.openHostConnectionConfirm(flow.hostRecord);
    if (confirmBeforeConnect) {
      assert.equal(flow.requests.length, 0);
      assert.equal(flow.connectionConfirmDraft.value?.hostRecordId, flow.hostRecord.id);
      await flow.submitConnectionConfirm();
    }
    assert.equal(flow.connectionConfirmDraft.value, null);
    assert.equal(flow.requests.length, 1);
    assert.equal(flow.requests[0].source, "saved-host");
    assert.equal(flow.saveRequests.length, 0);
  }
});

test("saving a temporary host without a name uses the submitted endpoint", () => {
  const draft = createTemporaryConnectionConfirmDraft({ host: "127.0.0.1", port: 5900 });
  draft.name = "";
  draft.host = " 127.0.0.3 ";
  draft.port = 5902;
  draft.options.saveTemporaryHost = true;
  const request = toTemporaryHostCreateRequest(draft);
  assert.equal(request?.name, "127.0.0.3:5902");
  assert.equal(request?.address, "127.0.0.3");
  assert.equal(request?.port, 5902);
});

test("temporary connection UI hides first-version Agent port controls and the modal close icon", () => {
  const dialogSource = readFileSync(
    new URL("../../../src/app/components/ConnectionDialog.vue", import.meta.url),
    "utf8",
  );
  const toolbarSource = readFileSync(
    new URL("../../../src/features/sessions/components/SessionToolbar.vue", import.meta.url),
    "utf8",
  );

  assert.match(dialogSource, /:closable="false"/);
  assert.doesNotMatch(dialogSource, /connectionConfirmDraft\.credentialStatus/);
  assert.doesNotMatch(dialogSource, /t\("toolbar\.agentPort"\)/);
  assert.doesNotMatch(dialogSource, /connectionConfirmDraft\.agentPort/);
  assert.doesNotMatch(toolbarSource, /t\("toolbar\.agentPort"\)/);
  assert.doesNotMatch(toolbarSource, /agentPort/);
});
