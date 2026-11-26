import { ref, type ComputedRef, type Ref } from "vue";
import type { ConfirmChoice, EditableHostDraft, HostGroupRecord, Translate } from "@app/types";
import {
  createHostRecord,
  toHostCredentialRequest,
  validateHostAddress,
  validateHostGroupName,
  validateHostUniqueness,
  type HostRecord,
  type HostRecordInput,
} from "@hosts/hostManager";
import type { EventLogLevel } from "@diagnostics/eventLog";
import {
  toTemporaryHostCreateRequest,
  type ConnectionConfirmDraft,
} from "@session/core/connectionConfirm";

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

type ConfirmRequest = {
  title: string;
  message: string;
  primaryLabel: string;
  secondaryLabel: string;
  danger?: boolean;
};

type HostResourceActionOptions = {
  discardHostDraftChanges: () => void;
  editingHostId: Ref<string>;
  filteredHosts: ComputedRef<HostRecord[]>;
  groupRenameDraft: Ref<string>;
  groupValidationMessage: Ref<string>;
  hostCreateVisible: Ref<boolean>;
  hostDraft: Ref<EditableHostDraft>;
  hostDraftDirty: ComputedRef<boolean>;
  hostDraftSnapshot: Ref<string>;
  hostEditorMode: Ref<"create" | "edit">;
  hostGroups: Ref<HostGroupRecord[]>;
  hostRecords: Ref<HostRecord[]>;
  hostStorageMessage: Ref<string>;
  invoke: Invoke;
  isTauriUnavailableError: (error: unknown) => boolean;
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  newGroupDraft: Ref<string>;
  notify: (content: string, tone: "success" | "warning" | "error") => void;
  requestConfirm: (options: ConfirmRequest) => Promise<ConfirmChoice>;
  resetGroupEditingState: (nextRenameDraft?: string) => void;
  resetHostDraft: () => void;
  selectedGroup: Ref<string>;
  selectedHostId: Ref<string>;
  serializeHostDraft: () => string;
  t: Translate;
  upsertHostRecord: (record: HostRecord) => void;
};

export function useHostResourceActions(options: HostResourceActionOptions) {
  const hostSavePending = ref(false);
  let hostClosePending = false;
  const {
    discardHostDraftChanges,
    editingHostId,
    filteredHosts,
    groupRenameDraft,
    groupValidationMessage,
    hostCreateVisible,
    hostDraft,
    hostDraftDirty,
    hostDraftSnapshot,
    hostEditorMode,
    hostGroups,
    hostRecords,
    hostStorageMessage,
    invoke,
    isTauriUnavailableError,
    logEvent,
    newGroupDraft,
    notify,
    requestConfirm,
    resetGroupEditingState,
    resetHostDraft,
    selectedGroup,
    selectedHostId,
    serializeHostDraft,
    t,
    upsertHostRecord,
  } = options;

  /** Adds a group without changing the host view's filter or selection. */
  async function createHostGroup(): Promise<boolean> {
    const validation = validateHostGroupName(newGroupDraft.value);

    if (!validation.valid) {
      groupValidationMessage.value = t("hosts.groupNameLength");
      return false;
    }

    if (!(await confirmHostDraftBeforeSwitch())) {
      return false;
    }

    const name = validation.name;
    groupValidationMessage.value = "";

    try {
      const group = await invoke<HostGroupRecord>("create_host_group", {
        request: { name },
      });
      hostGroups.value = [...hostGroups.value.filter((item) => item.name !== group.name), group];
      newGroupDraft.value = "";
      await loadGroups();
      logEvent("success", t("log.sourceHost"), t("log.groupCreated", { name: group.name }));
      return true;
    } catch (error) {
      hostStorageMessage.value = String(error);
      groupValidationMessage.value = t("hosts.groupCreateFailed");
      logEvent("error", t("log.sourceHost"), t("log.groupCreateFailed", { error: String(error) }));
      return false;
    }
  }

  async function renameSelectedGroup() {
    const validation = validateHostGroupName(groupRenameDraft.value);

    if (!selectedGroup.value) {
      return;
    }

    if (!validation.valid) {
      groupValidationMessage.value = t("hosts.groupNameLength");
      return;
    }

    const nextGroup = validation.name;

    if (nextGroup === selectedGroup.value) {
      resetGroupEditingState(selectedGroup.value);
      return;
    }

    if (!(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    groupValidationMessage.value = "";

    try {
      const changed = await invoke<number>("rename_host_group", {
        request: {
          from: selectedGroup.value,
          to: nextGroup,
        },
      });
      selectedGroup.value = nextGroup;
      resetGroupEditingState(nextGroup);
      await loadHostResources();
      selectedGroup.value = nextGroup;
      selectedHostId.value = filteredHosts.value[0]?.id ?? "";
      logEvent("success", t("log.sourceHost"), t("log.groupRenamed", { count: changed }));
    } catch (error) {
      hostStorageMessage.value = String(error);
      notify(hostStorageMessage.value, "error");
      logEvent("error", t("log.sourceHost"), t("log.groupRenameFailed", { error: String(error) }));
    }
  }

  async function deleteSelectedGroup() {
    if (!selectedGroup.value) {
      return;
    }

    if (!(await confirmHostDraftBeforeSwitch())) {
      return;
    }

    const groupToDelete = selectedGroup.value;
    const choice = await requestConfirm({
      title: t("hosts.deleteGroupTitle"),
      message: t("hosts.deleteGroupConfirm", { name: groupToDelete }),
      primaryLabel: t("hosts.delete"),
      secondaryLabel: t("hosts.cancel"),
      danger: true,
    });

    if (choice !== "primary") {
      return;
    }

    try {
      const changed = await invoke<number>("delete_host_group", {
        request: {
          group: groupToDelete,
        },
      });
      selectedGroup.value = "";
      resetGroupEditingState("");
      await loadHostResources();
      logEvent("success", t("log.sourceHost"), t("log.groupDeleted", { count: changed }));
    } catch (error) {
      hostStorageMessage.value = String(error);
      notify(hostStorageMessage.value, "error");
      logEvent("error", t("log.sourceHost"), t("log.groupDeleteFailed", { error: String(error) }));
    }
  }

  function mapPersistedHost(input: HostRecordInput): HostRecord {
    return createHostRecord(input);
  }

  async function loadHostResources() {
    const errors = await Promise.all([loadHosts(false), loadGroups(false)]);
    hostStorageMessage.value = [...new Set(errors.filter((message): message is string => message !== null))].join("\n");
    if (hostStorageMessage.value) notify(hostStorageMessage.value, "error");
  }

  async function loadHosts(reportError = true): Promise<string | null> {
    try {
      const persistedHosts = await invoke<HostRecordInput[]>("list_hosts");
      hostRecords.value = persistedHosts.map(mapPersistedHost);
      selectedHostId.value = hostRecords.value[0]?.id ?? "";
      if (reportError) hostStorageMessage.value = "";
      return null;
    } catch (error) {
      const message = t("hosts.storageUnavailable", { error: String(error) });
      if (reportError) {
        hostStorageMessage.value = message;
        notify(message, "error");
      }
      return message;
    }
  }

  async function loadGroups(reportError = true): Promise<string | null> {
    try {
      hostGroups.value = await invoke<HostGroupRecord[]>("list_host_groups");
      return null;
    } catch (error) {
      const message = t("hosts.storageUnavailable", { error: String(error) });
      if (reportError) {
        hostStorageMessage.value = message;
        notify(message, "error");
      }
      return message;
    }
  }

  async function saveTemporaryConnectionHost(draft: ConnectionConfirmDraft): Promise<HostRecord | undefined> {
    const request = toTemporaryHostCreateRequest(draft);

    if (!request) {
      return undefined;
    }

    try {
      const saved = await invoke<HostRecordInput>("create_host", { request });
      const record = mapPersistedHost(saved);
      upsertHostRecord(record);
      await loadGroups();
      logEvent("success", t("log.sourceHost"), t("log.hostCreated", { name: record.name }));
      return record;
    } catch (error) {
      hostStorageMessage.value = formatHostStorageError(error);
      notify(hostStorageMessage.value, "error");
      logEvent("error", t("log.sourceHost"), t("log.hostCreateFailed", { error: hostStorageMessage.value }));
      return undefined;
    }
  }

  async function resolveSelectedHostPassword(hostRecord: HostRecord): Promise<string | undefined> {
    if (!hostRecord.credentialRef) {
      return undefined;
    }

    try {
      const resolvedPassword = await invoke<string | null>("resolve_host_password", {
        id: hostRecord.id,
      });

      if (resolvedPassword) {
        logEvent("info", t("log.sourceHost"), t("log.hostCredentialResolved", { name: hostRecord.name }));
        return resolvedPassword;
      }

      logEvent("warning", t("log.sourceHost"), t("log.hostCredentialMissing", { name: hostRecord.name }));
      return undefined;
    } catch (error) {
      const message = formatHostStorageError(error);
      // Saved-password lookup can fall back to manual input without a second UI error.
      logEvent("error", t("log.sourceHost"), t("log.hostCredentialResolveFailed", { error: message }));
      return undefined;
    }
  }

  async function confirmHostDraftBeforeSwitch(): Promise<boolean> {
    if (hostSavePending.value || hostClosePending) {
      return false;
    }

    if (!hostDraftDirty.value) {
      if (hostCreateVisible.value) {
        discardHostDraftChanges();
      }

      return true;
    }

    const choice = await requestConfirm({
      title: t("hosts.unsavedTitle"),
      message: t("hosts.unsavedMessage"),
      primaryLabel: t("hosts.saveAndSwitch"),
      secondaryLabel: t("hosts.discardAndSwitch"),
    });

    if (choice === "primary") {
      return saveHost();
    }

    discardHostDraftChanges();
    return true;
  }

  /** All editor close paths confirm unsaved changes before clearing the credential draft. */
  async function closeHostEditor(): Promise<boolean> {
    if (!hostCreateVisible.value || hostSavePending.value || hostClosePending) {
      return false;
    }

    hostClosePending = true;
    try {
      if (hostDraftDirty.value) {
        const choice = await requestConfirm({
          title: t("hosts.unsavedTitle"),
          message: t("hosts.discardChangesMessage"),
          primaryLabel: t("hosts.discardChanges"),
          secondaryLabel: t("hosts.continueEditing"),
        });
        if (choice !== "primary") return false;
      }

      discardHostDraftChanges();
      hostStorageMessage.value = "";
      return true;
    } finally {
      hostClosePending = false;
    }
  }

  function formatHostStorageError(error: unknown): string {
    const message = String(error);

    if (message.toLowerCase().includes("credential vault")) {
      return t("hosts.credentialVaultError", { error: message });
    }

    return message;
  }

  async function saveHost(): Promise<boolean> {
    if (!hostCreateVisible.value || hostSavePending.value || hostClosePending) {
      return false;
    }

    hostSavePending.value = true;
    try {
      return await persistHostDraft();
    } finally {
      hostSavePending.value = false;
    }
  }

  async function persistHostDraft(): Promise<boolean> {
    hostStorageMessage.value = "";
    const isEditing = hostEditorMode.value === "edit";
    const addressValidation = validateHostAddress(hostDraft.value.address);

    if (!addressValidation.valid) {
      hostStorageMessage.value = t("hosts.addressContainsChinese");
      notify(hostStorageMessage.value, "warning");
      return false;
    }

    const uniqueness = validateHostUniqueness({
      hosts: hostRecords.value,
      draft: hostDraft.value,
      excludeHostId: isEditing ? editingHostId.value : undefined,
    });

    if (!uniqueness.valid) {
      hostStorageMessage.value = t(
        uniqueness.reason === "duplicate_name"
          ? "hosts.duplicateName"
          : "hosts.duplicateTarget",
      );
      notify(hostStorageMessage.value, "warning");
      return false;
    }

    const credentialRequest = toHostCredentialRequest(hostDraft.value.credential);
    const request = {
      name: hostDraft.value.name,
      address: hostDraft.value.address,
      port: hostDraft.value.port,
      protocol: hostDraft.value.protocol,
      group: hostDraft.value.group,
      savePassword: credentialRequest.savePassword,
      password: credentialRequest.password,
    };

    try {
      const saved = await invoke<HostRecordInput>(
        isEditing ? "update_host" : "create_host",
        {
          request:
            isEditing
              ? {
                  id: editingHostId.value,
                  ...request,
                }
              : request,
        },
      );
      const record = mapPersistedHost(saved);
      upsertHostRecord(record);
      await loadGroups();
      hostCreateVisible.value = false;
      editingHostId.value = "";
      hostEditorMode.value = "create";
      resetHostDraft();
      hostDraftSnapshot.value = serializeHostDraft();
      logEvent(
        "success",
        t("log.sourceHost"),
        t(isEditing ? "log.hostUpdated" : "log.hostCreated", { name: record.name }),
      );
      return true;
    } catch (error) {
      hostStorageMessage.value = formatHostStorageError(error);
      notify(hostStorageMessage.value, "error");
      logEvent("error", t("log.sourceHost"), t("log.hostCreateFailed", { error: hostStorageMessage.value }));
      return false;
    }
  }

  async function deleteHostRecord(hostRecord: HostRecord) {
    const choice = await requestConfirm({
      title: t("hosts.deleteTitle"),
      message: t("hosts.deleteConfirm", { name: hostRecord.name }),
      primaryLabel: t("hosts.delete"),
      secondaryLabel: t("hosts.cancel"),
      danger: true,
    });

    if (choice !== "primary") {
      return;
    }

    const previousHosts = [...hostRecords.value];
    hostRecords.value = hostRecords.value.filter((item) => item.id !== hostRecord.id);
    selectedHostId.value = filteredHosts.value[0]?.id ?? "";

    if (editingHostId.value === hostRecord.id) {
      discardHostDraftChanges();
    }

    try {
      await invoke("delete_host", { id: hostRecord.id });
      logEvent("success", t("log.sourceHost"), t("log.hostDeleted", { name: hostRecord.name }));
    } catch (error) {
      if (isTauriUnavailableError(error)) {
        hostStorageMessage.value = t("hosts.previewOnlyDelete");
        notify(hostStorageMessage.value, "warning");
        logEvent("warning", t("log.sourceHost"), t("log.hostDeletedPreviewOnly", { name: hostRecord.name }));
        return;
      }

      hostRecords.value = previousHosts;
      selectedHostId.value = hostRecord.id;
      hostStorageMessage.value = formatHostStorageError(error);
      notify(hostStorageMessage.value, "error");
      logEvent("error", t("log.sourceHost"), t("log.hostDeleteFailed", { error: hostStorageMessage.value }));
    }
  }

  /** Records recent use once per connection attempt, without changing the selected host. */
  async function persistHostConnectionAttempt(hostId: string | undefined) {
    if (!hostId) return;

    try {
      const updated = await invoke<HostRecordInput>("record_host_connection_attempt", { id: hostId });
      hostRecords.value = hostRecords.value.map((record) => record.id === hostId
        ? { ...record, lastConnectedAt: updated.lastConnectedAt }
        : record);
    } catch (error) {
      hostStorageMessage.value = String(error);
      notify(hostStorageMessage.value, "error");
      logEvent("error", t("log.sourceHost"), t("log.hostHistoryUpdateFailed", { error: String(error) }));
    }
  }

  return {
    closeHostEditor,
    confirmHostDraftBeforeSwitch,
    createHostGroup,
    deleteHostRecord,
    deleteSelectedGroup,
    formatHostStorageError,
    hostSavePending,
    loadGroups,
    loadHostResources,
    loadHosts,
    mapPersistedHost,
    persistHostConnectionAttempt,
    renameSelectedGroup,
    resolveSelectedHostPassword,
    saveHost,
    saveTemporaryConnectionHost,
  };
}
