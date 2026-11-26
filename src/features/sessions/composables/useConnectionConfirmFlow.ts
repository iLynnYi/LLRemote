import { ref, type ComputedRef, type Ref } from "vue";
import type { ConnectVncOptions, WorkspaceMode } from "@app/types";
import type { HostRecord } from "@hosts/hostManager";
import {
  createHostConnectionConfirmDraft,
  createTemporaryConnectionConfirmDraft,
  type ConnectionConfirmDraft,
} from "../../../session/core/connectionConfirm.ts";
import type { SessionSettings } from "@session/settings/sessionSettings";

type ConnectionConfirmFlowOptions = {
  activeWorkspace: Ref<WorkspaceMode>;
  connectSelectedHostRecord: (hostRecord: HostRecord, options?: ConnectVncOptions, passwordOverride?: string) => Promise<void>;
  connectVnc: (passwordOverride?: string, hostRecordId?: string, options?: ConnectVncOptions) => Promise<void>;
  host: Ref<string>;
  hostRecords: Ref<HostRecord[]>;
  password: Ref<string>;
  port: Ref<number>;
  saveTemporaryConnectionHost: (draft: ConnectionConfirmDraft) => Promise<HostRecord | undefined>;
  sessionSettings: ComputedRef<SessionSettings> | Ref<SessionSettings>;
};

export function useConnectionConfirmFlow(options: ConnectionConfirmFlowOptions) {
  const {
    activeWorkspace,
    connectSelectedHostRecord,
    connectVnc,
    host,
    hostRecords,
    password,
    port,
    saveTemporaryConnectionHost,
    sessionSettings,
  } = options;
  const connectionConfirmDraft = ref<ConnectionConfirmDraft | null>(null);

  async function openHostConnectionConfirm(hostRecord: HostRecord) {
    const draft = createHostConnectionConfirmDraft(hostRecord, sessionSettings.value.connectionDefaults);
    activeWorkspace.value = "sessions";

    if (!sessionSettings.value.confirmBeforeConnect) {
      await startConnectionFromDraft(draft);
      return;
    }

    connectionConfirmDraft.value = draft;
  }

  async function openTemporaryConnectionConfirm() {
    const draft = createTemporaryConnectionConfirmDraft(
      {
        host: host.value,
        port: port.value,
        password: password.value,
      },
      {
        ...sessionSettings.value.connectionDefaults,
        saveTemporaryHost: false,
      },
    );
    // 临时连接始终先填写表单；名称留空时以提交时的地址命名。
    draft.name = "";
    password.value = "";
    activeWorkspace.value = "sessions";
    connectionConfirmDraft.value = draft;
  }

  function closeConnectionConfirm() {
    connectionConfirmDraft.value = null;
  }

  function updateConnectionCredentialStatus() {
    if (!connectionConfirmDraft.value) {
      return;
    }

    if (connectionConfirmDraft.value.password) {
      connectionConfirmDraft.value.credentialStatus = "temporary";
      return;
    }

    if (connectionConfirmDraft.value.source === "saved-host" && connectionConfirmDraft.value.hostRecordId) {
      const hostRecord = hostRecords.value.find((item) => item.id === connectionConfirmDraft.value?.hostRecordId);
      connectionConfirmDraft.value.credentialStatus = hostRecord?.credentialRef ? "saved" : "missing";
      return;
    }

    connectionConfirmDraft.value.credentialStatus = "missing";
  }

  async function submitConnectionConfirm() {
    if (!connectionConfirmDraft.value) {
      return;
    }

    const draft = connectionConfirmDraft.value;
    connectionConfirmDraft.value = null;
    await startConnectionFromDraft(draft);
  }

  async function startConnectionFromDraft(draft: ConnectionConfirmDraft) {
    host.value = draft.host;
    port.value = draft.port;
    password.value = draft.password;
    activeWorkspace.value = "sessions";

    if (draft.source === "saved-host" && draft.hostRecordId) {
      const hostRecord = hostRecords.value.find((item) => item.id === draft.hostRecordId);

      if (!hostRecord) {
        return;
      }

      await connectSelectedHostRecord(hostRecord, {
        ...draft.options,
      }, draft.password || undefined);
      return;
    }

    const savedHost = await saveTemporaryConnectionHost(draft);
    await connectVnc(draft.password, savedHost?.id, {
      ...draft.options,
    });
  }

  return {
    closeConnectionConfirm,
    connectionConfirmDraft,
    openHostConnectionConfirm,
    openTemporaryConnectionConfirm,
    startConnectionFromDraft,
    submitConnectionConfirm,
    updateConnectionCredentialStatus,
  };
}
