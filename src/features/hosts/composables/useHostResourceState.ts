import { computed, ref } from "vue";
import type { HostGroupRecord, Translate, WorkspaceMode } from "@app/types";
import {
  createHostDraft,
  filterHostRecords,
  summarizeHostRecords,
  validateHostGroupName,
  type HostRecord,
} from "@hosts/hostManager";

export function useHostResourceState(t: Translate, activeWorkspace: { value: WorkspaceMode }) {
  const hostRecords = ref<HostRecord[]>([]);
  const hostGroups = ref<HostGroupRecord[]>([]);
  const selectedHostId = ref(hostRecords.value[0]?.id ?? "");
  const selectedGroup = ref("");
  const editingGroupName = ref("");
  const groupRenameDraft = ref("");
  const newGroupDraft = ref("");
  const groupValidationMessage = ref("");
  const hostSearchQuery = ref("");
  const hostStorageMessage = ref("");

  const hostSummary = computed(() => summarizeHostRecords(hostRecords.value));
  const hostGroupOptions = computed(() => {
    const names = new Set<string>();

    hostGroups.value.forEach((group) => names.add(group.name));
    hostSummary.value.groups.forEach((group) => names.add(group.name));

    return [...names].map((name) => {
      const stored = hostGroups.value.find((group) => group.name === name);
      const summary = hostSummary.value.groups.find((group) => group.name === name);

      return {
        name,
        count: stored?.hostCount ?? summary?.count ?? 0,
      };
    });
  });
  const filteredHosts = computed(() =>
    filterHostRecords(hostRecords.value, {
      group: selectedGroup.value || undefined,
      query: hostSearchQuery.value,
    }),
  );
  const selectedHost = computed(() => {
    const selected = hostRecords.value.find((item) => item.id === selectedHostId.value);
    return selected ?? filteredHosts.value[0] ?? null;
  });
  const activeHostScopeLabel = computed(() => {
    if (selectedGroup.value) {
      return `${t("hosts.group")} · ${selectedGroup.value}`;
    }

    return t("hosts.views.all");
  });
  const selectedGroupIsDefault = computed(() => selectedGroup.value === createHostDraft().group);
  const newGroupValidation = computed(() => validateHostGroupName(newGroupDraft.value));
  const renameGroupValidation = computed(() => validateHostGroupName(groupRenameDraft.value));
  const canCreateGroup = computed(() => newGroupValidation.value.valid);
  const canSaveGroupRename = computed(() =>
    renameGroupValidation.value.valid && !selectedGroupIsDefault.value,
  );

  function resetGroupEditingState(nextRenameDraft = selectedGroup.value) {
    editingGroupName.value = "";
    groupRenameDraft.value = nextRenameDraft;
    groupValidationMessage.value = "";
  }

  function selectAllHostsState() {
    activeWorkspace.value = "hosts";
    selectedGroup.value = "";
    resetGroupEditingState("");
    selectedHostId.value = filteredHosts.value[0]?.id ?? "";
  }

  function selectHostGroupState(group: string) {
    activeWorkspace.value = "hosts";
    selectedGroup.value = group;
    resetGroupEditingState(group);
    selectedHostId.value = filteredHosts.value[0]?.id ?? "";
  }

  function startGroupEditState(group: string) {
    selectHostGroupState(group);
    editingGroupName.value = group;
    groupRenameDraft.value = group;
  }

  function upsertHostRecord(record: HostRecord) {
    hostRecords.value = [record, ...hostRecords.value.filter((item) => item.id !== record.id)];
    selectedHostId.value = record.id;
  }

  return {
    activeHostScopeLabel,
    canCreateGroup,
    canSaveGroupRename,
    editingGroupName,
    filteredHosts,
    groupRenameDraft,
    groupValidationMessage,
    hostGroupOptions,
    hostGroups,
    hostRecords,
    hostSearchQuery,
    hostStorageMessage,
    hostSummary,
    newGroupDraft,
    newGroupValidation,
    renameGroupValidation,
    resetGroupEditingState,
    selectedGroup,
    selectedGroupIsDefault,
    selectedHost,
    selectedHostId,
    selectHostGroupState,
    selectAllHostsState,
    startGroupEditState,
    upsertHostRecord,
  };
}
