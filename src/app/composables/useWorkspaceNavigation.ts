import type { Ref } from "vue";
import type { WorkspaceMode } from "@app/types";
import type { HostRecord } from "@hosts/hostManager";
import type { LocalVncServerOverview } from "@server/localShare";

type WorkspaceNavigationOptions = {
  activeWorkspace: Ref<WorkspaceMode>;
  confirmHostDraftBeforeSwitch: () => Promise<boolean>;
  hostStorageMessage: Ref<string>;
  localVncServerOverview: Ref<LocalVncServerOverview | null>;
  openHostCreateDraft: () => void;
  openHostEditDraft: (hostRecord: HostRecord) => void;
  refreshLocalVncServerStatus: () => Promise<void>;
  resetGroupEditingState: (nextRenameDraft?: string) => void;
  selectHostGroupState: (group: string) => void;
  selectAllHostsState: () => void;
  selectedGroup: Ref<string>;
  selectedHostId: Ref<string>;
  startGroupEditState: (group: string) => void;
};

export function useWorkspaceNavigation(options: WorkspaceNavigationOptions) {
  const {
    activeWorkspace,
    confirmHostDraftBeforeSwitch,
    hostStorageMessage,
    localVncServerOverview,
    openHostCreateDraft,
    openHostEditDraft,
    refreshLocalVncServerStatus,
    resetGroupEditingState,
    selectHostGroupState,
    selectAllHostsState,
    selectedGroup,
    selectedHostId,
    startGroupEditState,
  } = options;

  async function selectWorkspace(mode: WorkspaceMode) {
    if (mode !== activeWorkspace.value && !(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    activeWorkspace.value = mode;
    if (mode === "localShare" && !localVncServerOverview.value) {
      void refreshLocalVncServerStatus();
    }
    if (mode !== "hosts") {
      resetGroupEditingState(selectedGroup.value);
    }
  }

  async function selectLocalShareWorkspace() {
    await selectWorkspace("localShare");
  }

  async function selectAllHosts() {
    if (!(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    selectAllHostsState();
  }

  async function selectHostGroup(group: string) {
    if (!(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    selectHostGroupState(group);
  }

  async function startGroupEdit(group: string) {
    if (!(await confirmHostDraftBeforeSwitch())) {
      return false;
    }

    startGroupEditState(group);
    return true;
  }

  function cancelGroupEdit() {
    resetGroupEditingState(selectedGroup.value);
  }

  async function selectHostRecord(hostRecord: HostRecord) {
    if (!(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    selectedHostId.value = hostRecord.id;
  }

  async function openHostCreateForm() {
    if (!(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    openHostCreateDraft();
    hostStorageMessage.value = "";
  }

  async function openHostEditForm(hostRecord: HostRecord) {
    if (!(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    hostStorageMessage.value = "";
    openHostEditDraft(hostRecord);
  }

  return {
    cancelGroupEdit,
    openHostCreateForm,
    openHostEditForm,
    selectHostGroup,
    selectHostRecord,
    selectAllHosts,
    selectLocalShareWorkspace,
    selectWorkspace,
    startGroupEdit,
  };
}
