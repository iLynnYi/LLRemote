export type HostProtocol = "vnc";

export type HostRecordInput = {
  id: string;
  name: string;
  address: string;
  port: number;
  protocol: HostProtocol;
  group?: string;
  credentialRef?: string;
  lastConnectedAt?: string;
};

export type HostRecord = Required<Pick<HostRecordInput, "id" | "name" | "address" | "port" | "protocol">> & {
  target: string;
  group: string;
  credentialRef?: string;
  lastConnectedAt?: string;
};

export type HostDraft = Omit<HostRecordInput, "id">;

export type HostFilter = {
  query?: string;
  group?: string;
};

export type NamedCount = {
  name: string;
  count: number;
};

export type HostSummary = {
  total: number;
  recent: number;
  groups: NamedCount[];
};

export type HostConnectTarget = {
  host: string;
  port: number;
};

export type HostConnectionDraft = HostConnectTarget & {
  password: string;
};

export type HostCredentialEditorMode = "idle" | "editing" | "removing";

export type HostCredentialDraft = {
  hasSavedCredential: boolean;
  mode: HostCredentialEditorMode;
  password: string;
};

export type HostCredentialRequest = {
  savePassword: boolean;
  password?: string;
};

const DEFAULT_GROUP = "未分组";
const SAVED_CREDENTIAL_MASK = "••••••••";
export const HOST_GROUP_NAME_MIN_LENGTH = 1;
export const HOST_GROUP_NAME_MAX_LENGTH = 8;

export type HostGroupNameValidation =
  | {
      valid: true;
      name: string;
    }
  | {
      valid: false;
      name: string;
    };

export type HostAddressValidation =
  | {
      valid: true;
      address: string;
    }
  | {
      valid: false;
      reason: "contains_chinese";
      address: string;
    };

export type HostUniquenessValidation =
  | {
      valid: true;
    }
  | {
      valid: false;
      reason: "duplicate_name" | "duplicate_target";
      conflictingHostId: string;
    };

export function validateHostGroupName(input: string): HostGroupNameValidation {
  const name = input.trim();
  const length = [...name].length;

  if (length < HOST_GROUP_NAME_MIN_LENGTH || length > HOST_GROUP_NAME_MAX_LENGTH) {
    return {
      valid: false,
      name,
    };
  }

  return {
    valid: true,
    name,
  };
}

export function validateHostAddress(input: string): HostAddressValidation {
  const address = input.trim();

  if (/[\u3400-\u9fff]/.test(address)) {
    return {
      valid: false,
      reason: "contains_chinese",
      address,
    };
  }

  return {
    valid: true,
    address,
  };
}

function normalizeHostUniqueText(value: string): string {
  return value.trim().toLocaleLowerCase();
}

export function validateHostUniqueness(options: {
  hosts: HostRecord[];
  draft: Pick<HostRecordInput, "name" | "address" | "port">;
  excludeHostId?: string;
}): HostUniquenessValidation {
  const draftName = normalizeHostUniqueText(options.draft.name);
  const draftAddress = normalizeHostUniqueText(options.draft.address);
  const draftPort = options.draft.port;

  for (const host of options.hosts) {
    if (host.id === options.excludeHostId) {
      continue;
    }

    if (normalizeHostUniqueText(host.name) === draftName) {
      return {
        valid: false,
        reason: "duplicate_name",
        conflictingHostId: host.id,
      };
    }

    if (normalizeHostUniqueText(host.address) === draftAddress && host.port === draftPort) {
      return {
        valid: false,
        reason: "duplicate_target",
        conflictingHostId: host.id,
      };
    }
  }

  return {
    valid: true,
  };
}

export function formatHostDateTime(value: string | undefined, timeZone?: string): string {
  const normalized = value?.trim();

  if (!normalized) {
    return "-";
  }

  const date = parseStoredHostDate(normalized);

  if (!date) {
    return normalized;
  }

  const parts = new Intl.DateTimeFormat("en-CA", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
    timeZone,
  }).formatToParts(date);
  const byType = Object.fromEntries(parts.map((part) => [part.type, part.value]));

  return `${byType.year}-${byType.month}-${byType.day} ${byType.hour}:${byType.minute}:${byType.second}`;
}

function parseStoredHostDate(value: string): Date | null {
  if (/^\d+$/.test(value)) {
    const numeric = Number(value);

    if (!Number.isFinite(numeric)) {
      return null;
    }

    return new Date(value.length <= 10 ? numeric * 1000 : numeric);
  }

  const parsed = Date.parse(value);

  if (Number.isNaN(parsed)) {
    return null;
  }

  return new Date(parsed);
}

export function createHostDraft(): HostDraft {
  return {
    name: "",
    address: "",
    port: 5900,
    protocol: "vnc",
    group: DEFAULT_GROUP,
  };
}

export function createHostRecord(input: HostRecordInput): HostRecord {
  const name = input.name.trim();
  const address = input.address.trim();
  const group = input.group?.trim() || DEFAULT_GROUP;

  return {
    id: input.id,
    name,
    address,
    port: input.port,
    protocol: input.protocol,
    group,
    target: `${address}:${input.port}`,
    credentialRef: input.credentialRef,
    lastConnectedAt: input.lastConnectedAt,
  };
}

export function filterHostRecords(hosts: HostRecord[], filter: HostFilter = {}): HostRecord[] {
  const query = filter.query?.trim().toLocaleLowerCase();

  const filtered = hosts.filter((host) => {
    if (filter.group && host.group !== filter.group) {
      return false;
    }

    if (!query) {
      return true;
    }

    return [
      host.name,
      host.address,
      String(host.port),
      host.target,
      host.group,
    ].some((value) => value.toLocaleLowerCase().includes(query));
  });

  return filtered;
}

export function summarizeHostRecords(hosts: HostRecord[]): HostSummary {
  return {
    total: hosts.length,
    recent: hosts.filter((host) => host.lastConnectedAt).length,
    groups: summarizeCounts(hosts.map((host) => host.group)),
  };
}

export function toConnectTarget(host: HostRecord): HostConnectTarget {
  return {
    host: host.address,
    port: host.port,
  };
}

export function toConnectionDraft(host: HostRecord, resolvedPassword = ""): HostConnectionDraft {
  return {
    ...toConnectTarget(host),
    password: resolvedPassword,
  };
}

export function createHostCredentialEditor(hasSavedCredential: boolean): HostCredentialDraft {
  return {
    hasSavedCredential,
    mode: "idle",
    password: "",
  };
}

export function startHostCredentialEdit(draft: HostCredentialDraft): HostCredentialDraft {
  return {
    ...draft,
    mode: "editing",
    password: "",
  };
}

export function cancelHostCredentialEdit(draft: HostCredentialDraft): HostCredentialDraft {
  return {
    ...draft,
    mode: "idle",
    password: "",
  };
}

export function markHostCredentialRemoval(draft: HostCredentialDraft): HostCredentialDraft {
  return {
    ...draft,
    mode: "removing",
    password: "",
  };
}

export function getHostCredentialMethod(draft: HostCredentialDraft): "prompt" | "password" {
  if (draft.mode === "removing") {
    return "prompt";
  }

  return draft.mode === "editing" || draft.hasSavedCredential ? "password" : "prompt";
}

export function getHostCredentialMaskPlaceholder(draft: HostCredentialDraft): string {
  return draft.hasSavedCredential ? SAVED_CREDENTIAL_MASK : "";
}

export function toHostCredentialRequest(draft: HostCredentialDraft): HostCredentialRequest {
  if (draft.mode === "editing") {
    return {
      savePassword: true,
      password: draft.password,
    };
  }

  if (draft.mode === "removing") {
    return {
      savePassword: false,
      password: undefined,
    };
  }

  if (draft.hasSavedCredential) {
    return {
      savePassword: true,
      password: "",
    };
  }

  return {
    savePassword: false,
    password: undefined,
  };
}

function summarizeCounts(values: string[]): NamedCount[] {
  const counts = new Map<string, number>();

  values.forEach((value) => {
    counts.set(value, (counts.get(value) ?? 0) + 1);
  });

  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((left, right) => right.count - left.count);
}
