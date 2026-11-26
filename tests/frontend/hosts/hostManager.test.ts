import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  cancelHostCredentialEdit,
  createHostDraft,
  createHostCredentialEditor,
  createHostRecord,
  filterHostRecords,
  formatHostDateTime,
  getHostCredentialMethod,
  getHostCredentialMaskPlaceholder,
  markHostCredentialRemoval,
  startHostCredentialEdit,
  summarizeHostRecords,
  toConnectionDraft,
  toConnectTarget,
  toHostCredentialRequest,
  validateHostAddress,
  validateHostUniqueness,
  validateHostGroupName,
  type HostRecord,
} from "../../../src/hosts/hostManager.ts";

const hosts: HostRecord[] = [
  createHostRecord({
    id: "host-1",
    name: "ops-mac-01",
    address: "192.168.10.21",
    port: 5900,
    protocol: "vnc",
    group: "研发机房",
    credentialRef: "keychain:vnc:ops-mac-01",
    lastConnectedAt: "2026-06-29T02:18:00.000Z",
  }),
  createHostRecord({
    id: "host-2",
    name: "win-uac-test",
    address: "192.168.10.38",
    port: 5900,
    protocol: "vnc",
    group: "测试环境",
    lastConnectedAt: "2026-06-29T03:24:00.000Z",
  }),
  createHostRecord({
    id: "host-3",
    name: "qa-trollvnc",
    address: "10.0.2.18",
    port: 5901,
    protocol: "vnc",
    group: "测试环境",
  }),
];

function appHostSource(): string {
  return [
    "../../../src/app/types.ts",
    "../../../src/app/composables/useVncAppState.ts",
    "../../../src/features/hosts/composables/useHostResourceActions.ts",
    "../../../src/features/hosts/composables/useHostWorkbench.ts",
    "../../../src/features/hosts/components/HostWorkbench.vue",
    "../../../src/features/hosts/components/HostEditorDialog.vue",
  ]
    .map((path) => readFileSync(new URL(path, import.meta.url), "utf8"))
    .join("\n");
}

test("createHostDraft returns a conservative VNC host draft", () => {
  assert.deepEqual(createHostDraft(), {
    name: "",
    address: "",
    port: 5900,
    protocol: "vnc",
    group: "未分组",
  });
});

test("createHostRecord normalizes group and creates a stable target", () => {
  const host = createHostRecord({
    id: "host-4",
    name: "  ops extra  ",
    address: "  10.0.0.12 ",
    port: 5902,
    protocol: "vnc",
    group: "",
  });

  assert.equal(host.name, "ops extra");
  assert.equal(host.address, "10.0.0.12");
  assert.equal(host.group, "未分组");
  assert.equal(host.target, "10.0.0.12:5902");
});

test("filterHostRecords supports group and free text search regardless of previous results", () => {
  assert.deepEqual(filterHostRecords(hosts).map((host) => host.id), ["host-1", "host-2", "host-3"]);
  assert.deepEqual(filterHostRecords(hosts, { group: "测试环境" }).map((host) => host.id), ["host-2", "host-3"]);
  assert.deepEqual(filterHostRecords(hosts, { query: "5901" }).map((host) => host.id), ["host-3"]);
  assert.deepEqual(filterHostRecords(hosts, { query: "测试环境" }).map((host) => host.id), ["host-2", "host-3"]);
});

test("summarizeHostRecords returns sidebar counters and groups", () => {
  const summary = summarizeHostRecords(hosts);

  assert.equal(summary.total, 3);
  assert.equal(summary.recent, 2);
  assert.deepEqual(summary.groups, [
    { name: "测试环境", count: 2 },
    { name: "研发机房", count: 1 },
  ]);
});

test("validateHostGroupName limits group names to one through eight trimmed characters", () => {
  assert.equal(validateHostGroupName("").valid, false);
  assert.equal(validateHostGroupName("   ").valid, false);
  assert.equal(validateHostGroupName("123456789").valid, false);
  assert.deepEqual(validateHostGroupName(" ops "), {
    valid: true,
    name: "ops",
  });
  assert.deepEqual(validateHostGroupName("12345678"), {
    valid: true,
    name: "12345678",
  });
});

test("validateHostAddress rejects Chinese characters in the address field", () => {
  assert.deepEqual(validateHostAddress(" 192.168.10.21 "), {
    valid: true,
    address: "192.168.10.21",
  });
  assert.deepEqual(validateHostAddress("测试.local"), {
    valid: false,
    reason: "contains_chinese",
    address: "测试.local",
  });
  assert.deepEqual(validateHostAddress("host-研发-01.local"), {
    valid: false,
    reason: "contains_chinese",
    address: "host-研发-01.local",
  });
});

test("validateHostUniqueness rejects duplicate names and address ports while editing can keep itself", () => {
  assert.deepEqual(validateHostUniqueness({
    hosts,
    draft: {
      ...createHostDraft(),
      name: " OPS-MAC-01 ",
      address: "10.10.10.10",
      port: 5900,
    },
  }), {
    valid: false,
    reason: "duplicate_name",
    conflictingHostId: "host-1",
  });

  assert.deepEqual(validateHostUniqueness({
    hosts,
    draft: {
      ...createHostDraft(),
      name: "new-name",
      address: " 192.168.10.38 ",
      port: 5900,
    },
  }), {
    valid: false,
    reason: "duplicate_target",
    conflictingHostId: "host-2",
  });

  assert.deepEqual(validateHostUniqueness({
    hosts,
    draft: {
      ...createHostDraft(),
      name: "ops-mac-01",
      address: "192.168.10.21",
      port: 5900,
    },
    excludeHostId: "host-1",
  }), {
    valid: true,
  });
});

test("formatHostDateTime renders stored timestamps as date and time to seconds", () => {
  assert.equal(formatHostDateTime("1719821045", "UTC"), "2024-07-01 08:04:05");
  assert.equal(formatHostDateTime("2026-06-29T03:24:00.000Z", "UTC"), "2026-06-29 03:24:00");
  assert.equal(formatHostDateTime(""), "-");
  assert.equal(formatHostDateTime(undefined), "-");
  assert.equal(formatHostDateTime("not-a-date"), "not-a-date");
});

test("toConnectTarget exposes only the values needed by session connection", () => {
  assert.deepEqual(toConnectTarget(hosts[0]), {
    host: "192.168.10.21",
    port: 5900,
  });
});

test("toConnectionDraft includes a resolved saved password and clears missing passwords", () => {
  assert.deepEqual(toConnectionDraft(hosts[0], "saved-password"), {
    host: "192.168.10.21",
    port: 5900,
    password: "saved-password",
  });
  assert.deepEqual(toConnectionDraft(hosts[1]), {
    host: "192.168.10.38",
    port: 5900,
    password: "",
  });
});

test("toHostCredentialRequest maps explicit credential UI modes to storage requests", () => {
  assert.deepEqual(toHostCredentialRequest(createHostCredentialEditor(false)), {
    savePassword: false,
    password: undefined,
  });
  assert.deepEqual(toHostCredentialRequest(createHostCredentialEditor(true)), {
    savePassword: true,
    password: "",
  });
  assert.deepEqual(toHostCredentialRequest({
    ...startHostCredentialEdit(createHostCredentialEditor(true)),
    password: "new-password",
  }), {
    savePassword: true,
    password: "new-password",
  });
  assert.deepEqual(toHostCredentialRequest(markHostCredentialRemoval(createHostCredentialEditor(true))), {
    savePassword: false,
    password: undefined,
  });
});

test("host credential editor enters editing mode for new and saved credentials", () => {
  assert.deepEqual(startHostCredentialEdit(createHostCredentialEditor(false)), {
    hasSavedCredential: false,
    mode: "editing",
    password: "",
  });
  assert.deepEqual(startHostCredentialEdit(createHostCredentialEditor(true)), {
    hasSavedCredential: true,
    mode: "editing",
    password: "",
  });
});

test("host credential editor can mark removal and cancel back to idle state", () => {
  const removing = markHostCredentialRemoval(createHostCredentialEditor(true));

  assert.deepEqual(removing, {
    hasSavedCredential: true,
    mode: "removing",
    password: "",
  });
  assert.deepEqual(cancelHostCredentialEdit({
    ...startHostCredentialEdit(createHostCredentialEditor(false)),
    password: "temporary",
  }), {
    hasSavedCredential: false,
    mode: "idle",
    password: "",
  });
  assert.deepEqual(cancelHostCredentialEdit(removing), {
    hasSavedCredential: true,
    mode: "idle",
    password: "",
  });
});

test("host credential method shows prompt while a saved credential is pending removal", () => {
  assert.equal(getHostCredentialMethod(createHostCredentialEditor(false)), "prompt");
  assert.equal(getHostCredentialMethod(createHostCredentialEditor(true)), "password");
  assert.equal(getHostCredentialMethod(startHostCredentialEdit(createHostCredentialEditor(true))), "password");
  assert.equal(getHostCredentialMethod(markHostCredentialRemoval(createHostCredentialEditor(true))), "prompt");
});

test("host credential placeholder uses masked value only when a saved password exists", () => {
  assert.equal(getHostCredentialMaskPlaceholder(createHostCredentialEditor(false)), "");
  assert.equal(getHostCredentialMaskPlaceholder(createHostCredentialEditor(true)), "••••••••");
});

test("App renders host auth method with Naive UI select and conditional password fields", () => {
  const appVue = appHostSource();

  assert.match(appVue, /type EditableHostDraft = HostDraft & \{/);
  assert.match(appVue, /const hostDraft = ref<EditableHostDraft>\(/);
  assert.match(appVue, /hosts\.authMethod/);
  assert.match(appVue, /NSelect/);
  assert.match(appVue, /v-model:value="hostCredentialMethod"/);
  assert.match(appVue, /value: "prompt"/);
  assert.match(appVue, /value: "password"/);
  assert.match(appVue, /hosts\.authMethodPrompt/);
  assert.match(appVue, /hosts\.authMethodPassword/);
  assert.match(appVue, /v-if="hostCredentialMethod === 'password'"/);
  assert.match(appVue, /:placeholder="hostCredentialPlaceholder"/);
  assert.match(appVue, /{{ t\("toolbar\.password"\) }}/);
  assert.match(appVue, /hostCredentialMethod === 'password'/);
  assert.doesNotMatch(appVue, /class="credential-fieldset"/);
  assert.doesNotMatch(appVue, /v-model="hostDraft\.credentialMode"/);
  assert.doesNotMatch(appVue, /hosts\.passwordNote/);
  assert.doesNotMatch(appVue, /class="host-password-tag"/);
});

test("App saves host credentials through the editor draft object", () => {
  const appVue = appHostSource();

  assert.match(appVue, /createHostCredentialEditor/);
  assert.match(appVue, /startHostCredentialEdit/);
  assert.match(appVue, /cancelHostCredentialEdit/);
  assert.match(appVue, /markHostCredentialRemoval/);
  assert.match(appVue, /const credentialRequest = toHostCredentialRequest\(hostDraft\.value\.credential\);/);
});

test("App blocks duplicate host names and targets before saving", () => {
  const appVue = appHostSource();

  assert.match(appVue, /validateHostAddress/);
  assert.match(appVue, /hosts\.addressContainsChinese/);
  assert.match(appVue, /validateHostUniqueness/);
  assert.match(appVue, /const uniqueness = validateHostUniqueness\(\{/);
  assert.match(appVue, /excludeHostId: isEditing \? editingHostId\.value : undefined/);
  assert.match(appVue, /hosts\.duplicateName/);
  assert.match(appVue, /hosts\.duplicateTarget/);
});
