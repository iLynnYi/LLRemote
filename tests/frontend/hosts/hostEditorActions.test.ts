import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { ref } from "vue";
import type { ConfirmChoice, HostGroupRecord, WorkspaceMode } from "../../../src/app/types.ts";
import { createHostRecord } from "../../../src/hosts/hostManager.ts";

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith("@hosts/")) {
    return next(new URL(`../../../src/hosts/${specifier.slice(7)}.ts`, import.meta.url).href, context);
  }
  if (specifier.startsWith("@session/")) {
    return next(new URL(`../../../src/session/${specifier.slice(9)}.ts`, import.meta.url).href, context);
  }
  return next(specifier, context);
} });

const { useHostWorkbench } = await import("../../../src/features/hosts/composables/useHostWorkbench.ts");
const { useHostResourceState } = await import("../../../src/features/hosts/composables/useHostResourceState.ts");
const { useHostResourceActions } = await import("../../../src/features/hosts/composables/useHostResourceActions.ts");

const storedHost = createHostRecord({
  id: "host-1", name: "Development Mac", address: "127.0.0.1", port: 5900,
  protocol: "vnc", credentialRef: "credential-reference",
});

test("loading failed host and group resources reports one combined error", async () => {
  const f = fixture();
  f.failGroupRefresh();
  await f.loadHostResources();
  assert.ok(f.calls.includes("list_hosts"));
  assert.ok(f.calls.includes("list_host_groups"));
  assert.equal(f.notifications.length, 1);
  assert.equal(f.notifications[0].tone, "error");
  assert.equal(f.notifications[0].content, f.hostStorageMessage.value);
});

function fixture() {
  const activeWorkspace = ref<WorkspaceMode>("hosts");
  const editor = useHostWorkbench();
  const resources = useHostResourceState((key) => key, activeWorkspace);
  const calls: string[] = [];
  const confirmations: string[] = [];
  let choice: ConfirmChoice = "secondary";
  let pendingConfirmation: Promise<ConfirmChoice> | undefined;
  let pendingSave: Promise<void> | undefined;
  let saveFailure = false;
  let groupRefreshFailure = false;
  let credentialFailure = false;
  const notifications: Array<{ content: string; tone: string }> = [];
  const createdGroups: HostGroupRecord[] = [];
  const actions = useHostResourceActions({
    ...editor,
    ...resources,
    async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
      calls.push(command);
      let result: unknown;
      if (command === "create_host" || command === "update_host") {
        if (pendingSave) await pendingSave;
        if (saveFailure) throw new Error("storage unavailable");
        result = storedHost;
      } else if (command === "create_host_group") {
        if (saveFailure) throw new Error("storage unavailable");
        const request = args?.request as { name: string };
        const group = { name: request.name, hostCount: 0 };
        createdGroups.push(group);
        result = group;
      } else if (command === "list_host_groups") {
        if (groupRefreshFailure) throw new Error("refresh unavailable");
        result = createdGroups;
      } else if (command === "resolve_host_password") {
        if (credentialFailure) throw new Error("credential vault read failed: access denied");
        result = null;
      } else {
        throw new Error(`Unexpected command: ${command}`);
      }
      return result as T;
    },
    isTauriUnavailableError: () => false,
    logEvent() {},
    notify(content, tone) { notifications.push({ content, tone }); },
    requestConfirm: async (request) => {
      confirmations.push(request.message);
      return pendingConfirmation ?? choice;
    },
    t: (key) => key,
  });
  return {
    ...editor, ...resources, ...actions, activeWorkspace, calls, confirmations, notifications,
    choose(next: ConfirmChoice) { choice = next; },
    holdConfirmation(pending: Promise<ConfirmChoice>) { pendingConfirmation = pending; },
    holdSave(pending: Promise<void>) { pendingSave = pending; },
    failSave() { saveFailure = true; },
    failGroupRefresh() { groupRefreshFailure = true; },
    denyCredential() { credentialFailure = true; },
  };
}

test("unavailable saved credentials fall back quietly without a host-storage error message", async () => {
  const f = fixture();
  f.hostStorageMessage.value = "existing-resource-feedback";
  f.denyCredential();
  assert.equal(await f.resolveSelectedHostPassword(storedHost), undefined);
  assert.deepEqual(f.calls, ["resolve_host_password"]);
  assert.deepEqual(f.notifications, []);
  assert.equal(f.hostStorageMessage.value, "existing-resource-feedback");
});

test("closing an unchanged editor needs no confirmation and clears its editing identity", async () => {
  const f = fixture();
  f.openHostEditDraft(storedHost);
  assert.equal(f.hostDraftDirty.value, false);
  assert.equal(await f.closeHostEditor(), true);
  assert.equal(f.hostCreateVisible.value, false);
  assert.equal(f.editingHostId.value, "");
  assert.equal(f.hostDraft.value.credential.hasSavedCredential, false);
  assert.deepEqual(f.confirmations, []);
  assert.deepEqual(f.calls, []);
});

test("creating a group preserves the current scope, search, selected host, and rename draft", async () => {
  for (const scope of ["", "未分组"]) {
    const f = fixture();
    f.hostRecords.value = [storedHost];
    f.selectedGroup.value = scope;
    f.selectedHostId.value = storedHost.id;
    f.hostSearchQuery.value = "Development";
    f.groupRenameDraft.value = "Rename draft";
    f.newGroupDraft.value = "  新分组  ";
    assert.equal(await f.createHostGroup(), true);
    assert.equal(f.selectedGroup.value, scope);
    assert.equal(f.selectedHostId.value, storedHost.id);
    assert.equal(f.hostSearchQuery.value, "Development");
    assert.equal(f.groupRenameDraft.value, "Rename draft");
    assert.equal(f.activeWorkspace.value, "hosts");
    assert.deepEqual(f.filteredHosts.value.map((host) => host.id), [storedHost.id]);
    assert.deepEqual(f.hostGroupOptions.value.find((group) => group.name === "新分组"), { name: "新分组", count: 0 });
    assert.equal(f.newGroupDraft.value, "");
  }
});

test("invalid group names stay in the form with an inline validation message and no backend call", async () => {
  for (const name of [" ", "一二三四五六七八九"]) {
    const f = fixture();
    f.newGroupDraft.value = name;
    assert.equal(await f.createHostGroup(), false);
    assert.equal(f.newGroupDraft.value, name);
    assert.equal(f.groupValidationMessage.value, "hosts.groupNameLength");
    assert.deepEqual(f.calls, []);
  }
});

test("a failed group creation retains the entered name and current selection for retry", async () => {
  const f = fixture();
  f.selectedGroup.value = "未分组";
  f.selectedHostId.value = storedHost.id;
  f.newGroupDraft.value = "新分组";
  f.failSave();
  assert.equal(await f.createHostGroup(), false);
  assert.equal(f.newGroupDraft.value, "新分组");
  assert.equal(f.groupValidationMessage.value, "hosts.groupCreateFailed");
  assert.equal(f.selectedGroup.value, "未分组");
  assert.equal(f.selectedHostId.value, storedHost.id);
});

test("a successful group creation remains visible if the following list refresh fails", async () => {
  const f = fixture();
  f.newGroupDraft.value = "新分组";
  f.failGroupRefresh();
  assert.equal(await f.createHostGroup(), true);
  assert.equal(f.hostGroupOptions.value.some((group) => group.name === "新分组"), true);
  assert.equal(f.selectedGroup.value, "");
});

test("continuing an unsaved edit preserves the form; confirmed discard resets it before reopening", async () => {
  const f = fixture();
  f.openHostEditDraft(storedHost);
  f.hostDraft.value.name = "Changed name";
  assert.equal(await f.closeHostEditor(), false);
  assert.equal(f.hostCreateVisible.value, true);
  assert.equal(f.hostDraft.value.name, "Changed name");
  assert.equal(f.editingHostId.value, storedHost.id);

  f.choose("primary");
  assert.equal(await f.closeHostEditor(), true);
  assert.equal(f.hostCreateVisible.value, false);
  assert.equal(f.hostDraft.value.credential.password, "");
  assert.equal(f.hostDraftDirty.value, false);
  f.openHostCreateDraft();
  assert.equal(f.hostEditorMode.value, "create");
  assert.equal(f.hostDraft.value.name, "");
  assert.equal(f.hostCredentialMethod.value, "prompt");
  assert.deepEqual(f.calls, [], "discard never persists the draft");
});

test("a pending close confirmation cannot be replaced by another close, navigation, or save", async () => {
  const f = fixture();
  f.openHostCreateDraft();
  f.hostDraft.value.name = "New host";
  let finish!: (choice: ConfirmChoice) => void;
  f.holdConfirmation(new Promise((resolve) => { finish = resolve; }));
  const closing = f.closeHostEditor();
  assert.equal(await f.closeHostEditor(), false);
  assert.equal(await f.confirmHostDraftBeforeSwitch(), false);
  assert.equal(await f.saveHost(), false);
  assert.equal(f.confirmations.length, 1);
  finish("secondary");
  assert.equal(await closing, false);
  assert.equal(f.hostCreateVisible.value, true);
});

test("saving blocks duplicate submissions and closing, then closes and resets the editor", async () => {
  const f = fixture();
  f.openHostEditDraft(storedHost);
  f.hostDraft.value.name = "Changed name";
  let finish!: () => void;
  f.holdSave(new Promise((resolve) => { finish = resolve; }));
  const saving = f.saveHost();
  assert.equal(f.hostSavePending.value, true);
  assert.equal(await f.saveHost(), false);
  assert.equal(await f.closeHostEditor(), false);
  assert.equal(await f.confirmHostDraftBeforeSwitch(), false);
  assert.deepEqual(f.calls, ["update_host"]);
  finish();
  assert.equal(await saving, true);
  assert.equal(f.hostSavePending.value, false);
  assert.equal(f.hostCreateVisible.value, false);
  assert.equal(f.hostDraft.value.name, "");
  assert.equal(f.hostDraft.value.credential.hasSavedCredential, false);
  assert.equal(await f.saveHost(), false, "a closed editor cannot submit again");
});

test("validation and backend failures keep the draft open and release the saving state", async () => {
  const f = fixture();
  f.openHostCreateDraft();
  f.hostDraft.value.name = "New host";
  f.hostDraft.value.address = "主机";
  assert.equal(await f.saveHost(), false);
  assert.equal(f.hostSavePending.value, false);
  assert.equal(f.hostCreateVisible.value, true);
  assert.deepEqual(f.calls, []);

  f.hostDraft.value.address = "127.0.0.1";
  f.failSave();
  assert.equal(await f.saveHost(), false);
  assert.equal(f.hostSavePending.value, false);
  assert.equal(f.hostCreateVisible.value, true);
  assert.equal(f.hostDraft.value.name, "New host");
  assert.match(f.hostStorageMessage.value, /storage unavailable/);
});
