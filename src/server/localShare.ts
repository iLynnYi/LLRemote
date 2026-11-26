export type LocalAddressKind = "loopback" | "lan_candidate";

export type LocalAddress = {
  label: string;
  address: string;
  kind: LocalAddressKind;
};

export type LocalVncServerStatus =
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "permission_missing"
  | "port_conflict"
  | "failed"
  | "unknown";

export type LocalVncServerStartRequest = {
  port: number;
  bindAddress: string;
  password: string;
  viewOnly: boolean;
};

export type LocalVncServerStartValidationField = "port" | "bindAddress" | "password";

export type LocalVncServerStartValidationError = {
  field: LocalVncServerStartValidationField;
  messageKey: string;
};

export type LocalVncServerStartValidationResult =
  | {
      valid: true;
      value: Omit<LocalVncServerStartRequest, "viewOnly">;
    }
  | {
      valid: false;
      errors: LocalVncServerStartValidationError[];
    };

export type LocalVncServerResolutionMode =
  | "fit"
  | "native"
  | "hd"
  | "fhd"
  | "hdLandscape"
  | "hdPortrait"
  | "fhdLandscape"
  | "fhdPortrait";
export type LocalVncServerRefreshMode = "responsive" | "balanced" | "lowPower";

export type LocalVncServerViewRequest = {
  displayId?: string | null;
  resolutionMode?: LocalVncServerResolutionMode | null;
  refreshMode?: LocalVncServerRefreshMode | null;
};

export type LocalVncServerDisplayKind = "main" | "display";

export type LocalVncServerDisplay = {
  id: string;
  label: string;
  kind: LocalVncServerDisplayKind;
  isPrimary: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  pixelWidth: number;
  pixelHeight: number;
  scaleFactor: number;
};

export type LocalVncServerPermissionKind = "screen_recording" | "accessibility";

export type LocalVncServerPermissionStatus = "granted" | "missing" | "unknown" | "unsupported";

export type LocalVncServerPermission = {
  kind: LocalVncServerPermissionKind;
  status: LocalVncServerPermissionStatus;
  requiredFor: string;
};

export type LocalVncServerConnectionKind = "rfb" | "agent_enhanced";

export type LocalVncServerConnectionStatus = "online" | "closing";

export type LocalVncServerActiveConnection = {
  id: string;
  kind: LocalVncServerConnectionKind;
  deviceName?: string | null;
  platform?: string | null;
  ipAddress?: string | null;
  connectedAt: string;
  status: LocalVncServerConnectionStatus;
};

export type LocalAgentStatus = "stopped" | "running" | "failed";

export type LocalAgentOverview = {
  status: LocalAgentStatus;
  bindAddress: string;
  port?: number | null;
  capabilities: string[];
  lastError?: string | null;
  updatedAt: string;
};

export type LocalVncServerPermissionRequestResult = {
  opened: boolean;
  kind: LocalVncServerPermissionKind;
  platform: string;
  target: string;
  message: string;
};

export type LocalVncServerCaptureStatus =
  | "waiting"
  | "ready"
  | "permission_denied"
  | "unavailable"
  | "invalid_frame"
  | "unsupported";

export type LocalVncServerFrameDiagnostics = {
  captureStatus: LocalVncServerCaptureStatus;
  captureFrames: number;
  captureFailures: number;
  framebufferUpdateRequests: number;
  framebufferUpdates: number;
  lastCaptureDurationMs: number;
  lastFrameAgeMs: number | null;
  lastDiffDurationMs: number;
  lastWriteDurationMs: number;
  lastDirtyRects: number;
  lastFramebufferEncoding: string;
  lastRawBytesWritten: number;
  lastEncodedBytesWritten: number;
  lastBytesWritten: number;
};

export type LocalVncServerOverview = {
  status: LocalVncServerStatus;
  bindAddress: string;
  port: number;
  addresses: LocalAddress[];
  displays: LocalVncServerDisplay[];
  selectedDisplayId: string;
  selectedDisplayLabel: string;
  resolutionMode: LocalVncServerResolutionMode;
  refreshMode: LocalVncServerRefreshMode;
  framebufferWidth: number;
  framebufferHeight: number;
  securityType: "vnc_auth" | string;
  viewOnly: boolean;
  activeConnections: number;
  activeConnectionDevices: LocalVncServerActiveConnection[];
  permissions: LocalVncServerPermission[];
  agent: LocalAgentOverview;
  frameDiagnostics?: LocalVncServerFrameDiagnostics;
  startedAt?: string | null;
  updatedAt: string;
};

export type LocalVncServerStatusSummary = {
  tone: "success" | "warning" | "error" | "neutral";
  titleKey: string;
  detailKey: string;
};

export type LocalAgentStatusSummary = {
  tone: "success" | "warning" | "error" | "neutral";
  titleKey: string;
  detailKey: string;
};

export type LocalVncServerStartErrorCopy = {
  messageKey: string;
};

export function summarizeLocalVncServerStatus(status: LocalVncServerStatus): LocalVncServerStatusSummary {
  switch (status) {
    case "running":
      return {
        tone: "success",
        titleKey: "localShare.serverStatus.running",
        detailKey: "localShare.serverStatusDetail.running",
      };
    case "starting":
    case "stopping":
      return {
        tone: "warning",
        titleKey: `localShare.serverStatus.${status}`,
        detailKey: `localShare.serverStatusDetail.${status}`,
      };
    case "permission_missing":
    case "port_conflict":
    case "failed":
      return {
        tone: "error",
        titleKey: `localShare.serverStatus.${status}`,
        detailKey: `localShare.serverStatusDetail.${status}`,
      };
    case "stopped":
      return {
        tone: "neutral",
        titleKey: "localShare.serverStatus.stopped",
        detailKey: "localShare.serverStatusDetail.stopped",
      };
    default:
      return {
        tone: "neutral",
        titleKey: "localShare.serverStatus.unknown",
        detailKey: "localShare.serverStatusDetail.unknown",
      };
  }
}

export function summarizeLocalAgentStatus(status: LocalAgentStatus): LocalAgentStatusSummary {
  switch (status) {
    case "running":
      return {
        tone: "success",
        titleKey: "localShare.agentStatus.running",
        detailKey: "localShare.agentStatusDetail.running",
      };
    case "failed":
      return {
        tone: "error",
        titleKey: "localShare.agentStatus.failed",
        detailKey: "localShare.agentStatusDetail.failed",
      };
    case "stopped":
      return {
        tone: "neutral",
        titleKey: "localShare.agentStatus.stopped",
        detailKey: "localShare.agentStatusDetail.stopped",
      };
    default:
      return {
        tone: "neutral",
        titleKey: "localShare.agentStatus.unknown",
        detailKey: "localShare.agentStatusDetail.unknown",
      };
  }
}

export function isLocalVncServerRunning(overview: LocalVncServerOverview | null | undefined): boolean {
  return overview?.status === "running";
}

export function canCopyLocalVncServerConnectionProfile(overview: LocalVncServerOverview | null | undefined): boolean {
  return isLocalVncServerRunning(overview);
}

export function getLocalVncServerToggleLabelKey(overview: LocalVncServerOverview | null | undefined): string {
  return isLocalVncServerRunning(overview) ? "localShare.stopServer" : "localShare.startServer";
}

export function getLocalVncServerCopyProfileTitleKey(overview: LocalVncServerOverview | null | undefined): string {
  return canCopyLocalVncServerConnectionProfile(overview)
    ? "localShare.copyServerProfile"
    : "localShare.copyServerProfileDisabled";
}

export function isLocalVncServerPasswordInputInvalid(
  password: string,
  prompted: boolean,
  overview: LocalVncServerOverview | null | undefined,
): boolean {
  return prompted && password.length === 0 && !isLocalVncServerRunning(overview);
}

export function isValidIpv4Address(value: string): boolean {
  const segments = value.trim().split(".");

  if (segments.length !== 4) {
    return false;
  }

  return segments.every((segment) => {
    if (!/^\d+$/.test(segment)) {
      return false;
    }

    const numericSegment = Number(segment);
    return numericSegment >= 0 && numericSegment <= 255;
  });
}

/** Validate both saved-credential and temporary starts from the share form. */
export function localVncServerPasswordValidationKey(password: string, allowSavedCredential = false): string | null {
  const bytes = new TextEncoder().encode(password).length;
  if (bytes === 0 && !allowSavedCredential) return "localShare.accessPasswordRequired";
  return bytes > 8 ? "localShare.accessPasswordTooLong" : null;
}

export function validateLocalVncServerStartInput(input: {
  port: unknown;
  bindAddress: string;
  password: string;
  allowSavedCredential?: boolean;
}): LocalVncServerStartValidationResult {
  const errors: LocalVncServerStartValidationError[] = [];
  const normalizedPort = typeof input.port === "number" ? String(input.port) : String(input.port ?? "").trim();
  const normalizedBindAddress = input.bindAddress.trim();
  const portIsNumeric = /^\d+$/.test(normalizedPort);
  const port = portIsNumeric ? Number(normalizedPort) : Number.NaN;

  if (!portIsNumeric || !Number.isInteger(port)) {
    errors.push({ field: "port", messageKey: "localShare.serverPortMustBeNumeric" });
  }

  if (!isValidIpv4Address(normalizedBindAddress)) {
    errors.push({ field: "bindAddress", messageKey: "localShare.serverBindAddressInvalid" });
  }

  const passwordError = localVncServerPasswordValidationKey(input.password, input.allowSavedCredential);
  if (passwordError) errors.push({ field: "password", messageKey: passwordError });

  if (errors.length > 0) {
    return { valid: false, errors };
  }

  return {
    valid: true,
    value: {
      port,
      bindAddress: normalizedBindAddress,
      password: input.password,
    },
  };
}

export function createLocalVncServerConnectionExport(
  overview: LocalVncServerOverview,
  host: string,
  name = "Local VNC Server",
): string {
  return JSON.stringify(
    {
      name,
      protocol: "vnc",
      host,
      port: overview.port,
      agentPort: overview.agent.port ?? null,
      security: overview.securityType,
      viewOnly: overview.viewOnly,
    },
    null,
    2,
  );
}

export function getLocalVncServerStartErrorCopy(error: unknown): LocalVncServerStartErrorCopy | null {
  const message = String(error);

  if (message.includes("password_required")) {
    return { messageKey: "localShare.accessPasswordRequired" };
  }

  if (message.includes("port_conflict")) {
    return { messageKey: "localShare.serverPortConflict" };
  }

  if (message.includes("already_running")) {
    return { messageKey: "localShare.serverAlreadyRunning" };
  }

  if (message.includes("local_vnc_server_stopping")) {
    return { messageKey: "localShare.serverStatusDetail.stopping" };
  }

  if (message.includes("permission_missing")) {
    return { messageKey: "localShare.serverPermissionMissing" };
  }

  return null;
}

export function localVncServerPermissionKindKey(kind: LocalVncServerPermissionKind): string {
  return `localShare.permissions.kind.${kind}`;
}

export function localVncServerPermissionStatusKey(status: LocalVncServerPermissionStatus): string {
  return `localShare.permissions.status.${status}`;
}
