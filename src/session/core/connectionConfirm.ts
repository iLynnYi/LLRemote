import type { HostProtocol, HostRecord } from "@hosts/hostManager";

export type ConnectionConfirmSource = "saved-host" | "temporary";

export type ConnectionCredentialStatus = "saved" | "temporary" | "missing";

export type PostConnectPresentation = "main" | "fullscreen" | "external";

export type ConnectionConfirmOptions = {
  postConnectPresentation: PostConnectPresentation;
  readonly: boolean;
  saveTemporaryHost: boolean;
};

export type ConnectionConfirmOptionsInput = Partial<ConnectionConfirmOptions>;

export type ConnectionConfirmDraft = {
  source: ConnectionConfirmSource;
  hostRecordId?: string;
  name: string;
  host: string;
  port: number;
  target: string;
  protocol: HostProtocol;
  password: string;
  credentialStatus: ConnectionCredentialStatus;
  options: ConnectionConfirmOptions;
};

export type TemporaryConnectionInput = {
  host: string;
  port: number;
  password?: string;
};

export type TemporaryHostCreateRequest = {
  name: string;
  address: string;
  port: number;
  protocol: HostProtocol;
  group: string;
  savePassword: boolean;
  password?: string;
};

const DEFAULT_GROUP = "未分组";

export function defaultConnectionConfirmOptions(
  defaults: ConnectionConfirmOptionsInput = {},
): ConnectionConfirmOptions {
  return {
    postConnectPresentation: normalizePostConnectPresentation(defaults),
    readonly: defaults.readonly ?? false,
    saveTemporaryHost: defaults.saveTemporaryHost ?? false,
  };
}

function normalizePostConnectPresentation(defaults: ConnectionConfirmOptionsInput): PostConnectPresentation {
  if (defaults.postConnectPresentation === "main" || defaults.postConnectPresentation === "fullscreen" || defaults.postConnectPresentation === "external") {
    return defaults.postConnectPresentation;
  }
  return "main";
}

export function createHostConnectionConfirmDraft(
  host: HostRecord,
  options: ConnectionConfirmOptionsInput = {},
): ConnectionConfirmDraft {
  return {
    source: "saved-host",
    hostRecordId: host.id,
    name: host.name,
    host: host.address,
    port: host.port,
    target: host.target,
    protocol: host.protocol,
    password: "",
    credentialStatus: host.credentialRef ? "saved" : "missing",
    options: defaultConnectionConfirmOptions(options),
  };
}

export function createTemporaryConnectionConfirmDraft(
  input: TemporaryConnectionInput,
  options: ConnectionConfirmOptionsInput = {},
): ConnectionConfirmDraft {
  const host = input.host.trim();
  const password = input.password ?? "";

  return {
    source: "temporary",
    name: `${host}:${input.port}`,
    host,
    port: input.port,
    target: `${host}:${input.port}`,
    protocol: "vnc",
    password,
    credentialStatus: password ? "temporary" : "missing",
    options: defaultConnectionConfirmOptions(options),
  };
}

export function toTemporaryHostCreateRequest(draft: ConnectionConfirmDraft): TemporaryHostCreateRequest | undefined {
  if (draft.source !== "temporary" || !draft.options.saveTemporaryHost) {
    return undefined;
  }

  const password = draft.password.trim();

  return {
    name: draft.name.trim() || `${draft.host.trim()}:${draft.port}`,
    address: draft.host.trim(),
    port: draft.port,
    protocol: draft.protocol,
    group: DEFAULT_GROUP,
    savePassword: Boolean(password),
    password: password || undefined,
  };
}
