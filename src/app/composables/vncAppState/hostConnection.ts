import type { Ref } from "vue";
import type { AppMessageTone, ConnectVncOptions, Translate, WorkspaceMode } from "@app/types";
import type { HostRecord } from "@hosts/hostManager";
import { toConnectionDraft } from "../../../hosts/hostManager.ts";

export function createHostConnectionActions(options: {
  activeWorkspace: Ref<WorkspaceMode>;
  connectVnc: (passwordOverride?: string, hostRecordId?: string, options?: ConnectVncOptions) => Promise<void>;
  host: Ref<string>;
  openHostConnectionConfirm: (hostRecord: HostRecord) => void;
  openTemporaryConnectionConfirm: () => Promise<void>;
  password: Ref<string>;
  port: Ref<number>;
  requestSessionPassword: (target: string, initialPassword?: string, signal?: AbortSignal, onUserCancel?: () => void) => Promise<string | undefined>;
  resolveSelectedHostPassword: (hostRecord: HostRecord) => Promise<string | undefined>;
  sessionSettings: Ref<{ connectionDefaults: { postConnectPresentation: ConnectVncOptions["postConnectPresentation"] } }>;
  showAppMessage: (content: string, tone?: AppMessageTone) => void;
  t: Translate;
}) {
  function applyHostToConnection(hostRecord: HostRecord) {
    const draft = toConnectionDraft(hostRecord);
    options.host.value = draft.host;
    options.port.value = draft.port;
  }

  async function fillSelectedHostConnectionForm(hostRecord: HostRecord) {
    hostRecord = { ...hostRecord };
    const resolvedPassword = hostRecord.credentialRef ? await options.resolveSelectedHostPassword(hostRecord) : "";
    const draft = toConnectionDraft(hostRecord, resolvedPassword ?? "");
    options.host.value = draft.host;
    options.port.value = draft.port;
    options.password.value = draft.password;
    options.activeWorkspace.value = "sessions";
    await options.openTemporaryConnectionConfirm();
  }

  function connectSelectedHost(hostRecord: HostRecord | null) {
    if (hostRecord) options.openHostConnectionConfirm(hostRecord);
  }

  async function connectSelectedHostRecord(hostRecord: HostRecord, connectOptions: ConnectVncOptions = {}, passwordOverride?: string) {
    hostRecord = { ...hostRecord };
    const { host, port } = toConnectionDraft(hostRecord);
    connectOptions = {
      ...connectOptions,
      endpoint: { host, port },
      savedHostName: hostRecord.name,
      postConnectPresentation: connectOptions.postConnectPresentation ?? options.sessionSettings.value.connectionDefaults.postConnectPresentation,
    };
    applyHostToConnection(hostRecord);
    let savedPassword = passwordOverride;
    passwordOverride = undefined;
    if (savedPassword === undefined && hostRecord.credentialRef) savedPassword = await options.resolveSelectedHostPassword(hostRecord);
    if (hostRecord.credentialRef && !savedPassword) {
      savedPassword = await options.requestSessionPassword(hostRecord.target, "");
      if (!savedPassword) return;
    }
    const connecting = options.connectVnc(savedPassword ?? "", hostRecord.id, connectOptions);
    savedPassword = undefined;
    await connecting;
  }

  return {
    applyHostToConnection,
    connectSelectedHost,
    connectSelectedHostRecord,
    fillSelectedHostConnectionForm,
  };
}
