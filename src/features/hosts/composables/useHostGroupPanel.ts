import { computed, ref, type Ref } from "vue";
import { createHostDraft, type NamedCount } from "../../../hosts/hostManager.ts";

type PanelStorage = Pick<Storage, "getItem" | "setItem">;
const COLLAPSED_KEY = "llremote.host-groups-collapsed";

/** Keeps group discovery independent of the selected host scope and remembers panel visibility. */
export function useHostGroupPanel(options: {
  groups: Readonly<Ref<NamedCount[]>>;
  editingGroupName: Ref<string>;
  startGroupEdit: (group: string) => Promise<boolean>;
  deleteSelectedGroup: () => Promise<void>;
  cancelGroupEdit: () => void;
}, storage?: PanelStorage) {
  let collapsed = false;
  try { collapsed = storage?.getItem(COLLAPSED_KEY) === "true"; } catch { /* Storage is optional. */ }
  const hostGroupsCollapsed = ref(collapsed);
  const defaultGroup = createHostDraft().group;
  const hostGroupSearchQuery = ref("");
  const visibleHostGroups = computed(() => {
    const query = hostGroupSearchQuery.value.trim().toLocaleLowerCase();
    return options.groups.value.filter((group) =>
      group.name === options.editingGroupName.value || group.name.toLocaleLowerCase().includes(query),
    );
  });

  function toggleHostGroups() {
    hostGroupsCollapsed.value = !hostGroupsCollapsed.value;
    try { storage?.setItem(COLLAPSED_KEY, String(hostGroupsCollapsed.value)); } catch { /* Keep the in-memory preference. */ }
  }

  function canManageHostGroup(group: string) {
    return group !== defaultGroup;
  }

  async function manageHostGroup(action: string, group: string) {
    if ((action !== "rename" && action !== "delete") || !canManageHostGroup(group)) return;
    if (!(await options.startGroupEdit(group))) return;
    if (options.editingGroupName.value !== group) return;
    if (action === "delete") {
      await options.deleteSelectedGroup();
      options.cancelGroupEdit();
    }
  }

  return { hostGroupsCollapsed, hostGroupSearchQuery, visibleHostGroups, toggleHostGroups, canManageHostGroup, manageHostGroup };
}
