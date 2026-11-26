import { computed, ref } from "vue";
import {
  cancelHostCredentialEdit,
  createHostCredentialEditor,
  createHostDraft,
  getHostCredentialMethod,
  getHostCredentialMaskPlaceholder,
  markHostCredentialRemoval,
  startHostCredentialEdit,
  type HostRecord,
} from "@hosts/hostManager";
import type { EditableHostDraft } from "@app/types";

export function useHostWorkbench() {
  const hostCreateVisible = ref(false);
  const hostEditorMode = ref<"create" | "edit">("create");
  const editingHostId = ref("");
  const hostDraftSnapshot = ref("");
  const hostDraft = ref<EditableHostDraft>({
    ...createHostDraft(),
    credential: createHostCredentialEditor(false),
  });

  const hostDraftDirty = computed(() => hostCreateVisible.value && serializeHostDraft() !== hostDraftSnapshot.value);
  const hostCredentialMethod = computed<"prompt" | "password">({
    get() {
      return getHostCredentialMethod(hostDraft.value.credential);
    },
    set(value) {
      if (value === "password") {
        hostDraft.value = {
          ...hostDraft.value,
          credential: startHostCredentialEdit(hostDraft.value.credential),
        };
        return;
      }

      hostDraft.value = {
        ...hostDraft.value,
        credential: hostDraft.value.credential.hasSavedCredential
          ? markHostCredentialRemoval(hostDraft.value.credential)
          : cancelHostCredentialEdit(hostDraft.value.credential),
      };
    },
  });
  const hostCredentialPlaceholder = computed(() => getHostCredentialMaskPlaceholder(hostDraft.value.credential));

  function resetHostDraft() {
    hostDraft.value = {
      ...createHostDraft(),
      credential: createHostCredentialEditor(false),
    };
  }

  function serializeHostDraft() {
    return JSON.stringify({
      name: hostDraft.value.name,
      address: hostDraft.value.address,
      port: hostDraft.value.port,
      protocol: hostDraft.value.protocol,
      group: hostDraft.value.group,
      credential: hostDraft.value.credential,
    });
  }

  function openHostCreateDraft() {
    resetHostDraft();
    hostEditorMode.value = "create";
    editingHostId.value = "";
    hostCreateVisible.value = true;
    hostDraftSnapshot.value = serializeHostDraft();
  }

  function openHostEditDraft(hostRecord: HostRecord) {
    hostEditorMode.value = "edit";
    editingHostId.value = hostRecord.id;
    hostCreateVisible.value = true;
    hostDraft.value = {
      name: hostRecord.name,
      address: hostRecord.address,
      port: hostRecord.port,
      protocol: hostRecord.protocol,
      group: hostRecord.group,
      credential: createHostCredentialEditor(Boolean(hostRecord.credentialRef)),
    };
    hostDraftSnapshot.value = serializeHostDraft();
  }

  function discardHostDraftChanges() {
    hostCreateVisible.value = false;
    editingHostId.value = "";
    hostEditorMode.value = "create";
    resetHostDraft();
    hostDraftSnapshot.value = serializeHostDraft();
  }

  function markHostDraftSaved() {
    hostCreateVisible.value = false;
    editingHostId.value = "";
    hostEditorMode.value = "create";
    resetHostDraft();
    hostDraftSnapshot.value = serializeHostDraft();
  }

  return {
    discardHostDraftChanges,
    editingHostId,
    hostCreateVisible,
    hostCredentialMethod,
    hostCredentialPlaceholder,
    hostDraft,
    hostDraftDirty,
    hostDraftSnapshot,
    hostEditorMode,
    markHostDraftSaved,
    openHostCreateDraft,
    openHostEditDraft,
    resetHostDraft,
    serializeHostDraft,
  };
}
