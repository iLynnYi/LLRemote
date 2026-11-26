import assert from "node:assert/strict";
import test from "node:test";
import { ref } from "vue";
import { useHostGroupPanel } from "../../../src/features/hosts/composables/useHostGroupPanel.ts";

function fixture(storage?: Pick<Storage, "getItem" | "setItem">) {
  const groups = ref([{ name: "未分组", count: 1 }, { name: "Windows", count: 3 }, { name: "Mac", count: 2 }]);
  const editingGroupName = ref("");
  const actions: string[] = [];
  let allowEdit = true;
  const panel = useHostGroupPanel({
    groups, editingGroupName,
    startGroupEdit: async (group) => { if (allowEdit) editingGroupName.value = group; return allowEdit; },
    deleteSelectedGroup: async () => { actions.push(`delete:${editingGroupName.value}`); },
    cancelGroupEdit: () => { editingGroupName.value = ""; },
  }, storage);
  return { ...panel, groups, editingGroupName, actions, denyEdit() { allowEdit = false; } };
}

test("group search is case insensitive, keeps counts, and preserves the complete group source", () => {
  const f = fixture();
  f.hostGroupSearchQuery.value = "  WIN  ";
  assert.deepEqual(f.visibleHostGroups.value, [{ name: "Windows", count: 3 }]);
  assert.equal(f.groups.value.length, 3);
  f.hostGroupSearchQuery.value = "missing";
  assert.deepEqual(f.visibleHostGroups.value, []);
  f.hostGroupSearchQuery.value = "";
  assert.equal(f.visibleHostGroups.value.length, 3);
});

test("renaming remains visible while the group list is filtered", async () => {
  const f = fixture();
  await f.manageHostGroup("rename", "Mac");
  f.hostGroupSearchQuery.value = "Windows";
  assert.deepEqual(f.visibleHostGroups.value.map((group) => group.name), ["Windows", "Mac"]);
});

test("panel visibility persists independently of group search and survives unavailable storage", () => {
  const values = new Map<string, string>();
  const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
  const f = fixture(storage);
  f.hostGroupSearchQuery.value = "Mac";
  f.toggleHostGroups();
  assert.equal(f.hostGroupsCollapsed.value, true);
  assert.equal(f.hostGroupSearchQuery.value, "Mac");
  assert.equal(fixture(storage).hostGroupsCollapsed.value, true);
  const unavailable = fixture({ getItem() { throw new Error("unavailable"); }, setItem() { throw new Error("unavailable"); } });
  unavailable.toggleHostGroups();
  assert.equal(unavailable.hostGroupsCollapsed.value, true);
});

test("group menu protects the default group and cancelled transitions before deleting", async () => {
  const f = fixture();
  await f.manageHostGroup("delete", "未分组");
  assert.deepEqual(f.actions, []);
  f.denyEdit();
  f.editingGroupName.value = "Mac";
  await f.manageHostGroup("delete", "Mac");
  assert.deepEqual(f.actions, []);
  const allowed = fixture();
  await allowed.manageHostGroup("delete", "Mac");
  assert.deepEqual(allowed.actions, ["delete:Mac"]);
  assert.equal(allowed.editingGroupName.value, "");
});
