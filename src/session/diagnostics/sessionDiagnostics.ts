import { formatNativeRfbFailure, type NativeRfbFailure } from "../protocol/rfbFailure.ts";

export type ConnectionPhase =
  | "idle"
  | "probing"
  | "resolving"
  | "tcpConnecting"
  | "protocolHandshake"
  | "securityNegotiating"
  | "authenticating"
  | "initializingFramebuffer"
  | "connected"
  | "disconnecting"
  | "disconnected"
  | "failed";

export type ConnectionAutoReconnectPolicy = "never" | "configurable" | "allowed";

export type ConnectionFailureCode =
  | "target_unreachable"
  | "service_unavailable"
  | "credentials_required"
  | "authentication_failed"
  | "authentication_cancelled"
  | "connection_cancelled"
  | "resource_limit"
  | "security_failure"
  | "unsupported_security"
  | "not_vnc_service"
  | "protocol_error"
  | "remote_closed"
  | "agent_stream_failed"
  | "session_unavailable"
  | "unknown_error";

export type ConnectionFailureClassification = {
  code: ConnectionFailureCode;
  title: string;
  messageKey: string;
};

export type DisconnectReasonCode =
  | "user_requested"
  | "idle_timeout"
  | "remote_closed"
  | "network_lost"
  | "session_closed"
  | "app_shutdown"
  | "unknown";

export type ConnectionIssue = {
  code: ConnectionFailureCode | DisconnectReasonCode;
  phase: ConnectionPhase;
  title: string;
  messageKey: string;
  message?: string;
  retryable: boolean;
  autoReconnect: ConnectionAutoReconnectPolicy;
  logLevel: "none" | "info" | "warning" | "error";
  technicalMessage?: string;
};

export type FollowupRemoteDisconnectInput = {
  state: string;
  lastIssue?: ConnectionIssue;
  incomingIssue: ConnectionIssue;
};

export type CreateConnectionIssueInput = {
  reason: string | NativeRfbFailure;
  phase?: ConnectionPhase;
};

export type StabilityCheckpointInput = {
  previousSeconds: number;
  currentSeconds: number;
  alreadyRecorded: boolean;
};

export type FormatStabilityCheckpointEventInput = {
  target: string;
  duration: string;
};

export type RemoteSessionTiming = {
  connectedAt?: string;
  closedAt?: string;
};

export function formatDuration(totalSeconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const seconds = safeSeconds % 60;

  return [hours, minutes, seconds].map((value) => value.toString().padStart(2, "0")).join(":");
}

export function elapsedRemoteSessionSeconds(
  session: RemoteSessionTiming | null | undefined,
  nowMs: number,
): number {
  if (!session?.connectedAt) {
    return 0;
  }

  const startedAt = Date.parse(session.connectedAt);
  const endedAt = session.closedAt ? Date.parse(session.closedAt) : nowMs;

  if (!Number.isFinite(startedAt) || !Number.isFinite(endedAt)) {
    return 0;
  }

  return Math.max(0, Math.floor((endedAt - startedAt) / 1000));
}

export function stabilityLabel(totalSeconds: number): "collecting" | "30m-ready" {
  return totalSeconds >= 30 * 60 ? "30m-ready" : "collecting";
}

export function shouldRecordStabilityCheckpoint(input: StabilityCheckpointInput): boolean {
  const checkpointSeconds = 30 * 60;

  return (
    !input.alreadyRecorded &&
    input.previousSeconds < checkpointSeconds &&
    input.currentSeconds >= checkpointSeconds
  );
}

export function formatStabilityCheckpointEvent(input: FormatStabilityCheckpointEventInput): string {
  return [
    "stability_check=30m_ready",
    `target=${input.target}`,
    `duration=${input.duration}`,
    "verify framebuffer refresh, mouse input, keyboard input, disconnect, and reconnect.",
  ].join("; ");
}

export function redactSensitiveText(value: string): string {
  return value
    .replace(/\b(password|passwd|pwd|secret|token)\s*=\s*[^;\s]+/gi, "$1=[redacted]")
    .replace(/\b(password|passwd|pwd|secret|token)\s*:\s*[^;\s]+/gi, "$1:[redacted]");
}

function connectionFailurePolicy(code: ConnectionFailureCode): Pick<ConnectionIssue, "retryable" | "autoReconnect" | "logLevel"> {
  if (code === "authentication_failed" || code === "credentials_required" || code === "authentication_cancelled" || code === "connection_cancelled") {
    return { retryable: true, autoReconnect: "never", logLevel: "none" };
  }

  if (code === "resource_limit") {
    return { retryable: false, autoReconnect: "never", logLevel: "error" };
  }

  if (code === "target_unreachable" || code === "remote_closed") {
    return { retryable: true, autoReconnect: "configurable", logLevel: "error" };
  }

  if (code === "unsupported_security" || code === "not_vnc_service") {
    return { retryable: false, autoReconnect: "never", logLevel: "error" };
  }

  if (code === "agent_stream_failed") {
    return { retryable: true, autoReconnect: "allowed", logLevel: "error" };
  }

  return { retryable: true, autoReconnect: "never", logLevel: "error" };
}

export function createConnectionIssue(input: CreateConnectionIssueInput): ConnectionIssue {
  const classification = classifyConnectionFailure(input.reason);
  return {
    ...classification,
    phase: input.phase ?? "failed",
    ...connectionFailurePolicy(classification.code),
    ...(typeof input.reason === "string" ? {} : { retryable: input.reason.retryable }),
    technicalMessage: typeof input.reason === "string" ? redactSensitiveText(input.reason) : formatNativeRfbFailure(input.reason),
  };
}

export function createDisconnectIssue(code: DisconnectReasonCode, technicalMessage?: string): ConnectionIssue {
  const base: Record<DisconnectReasonCode, Pick<ConnectionIssue, "title" | "retryable" | "autoReconnect" | "logLevel">> = {
    user_requested: {
      title: "User disconnected",
      retryable: true,
      autoReconnect: "never",
      logLevel: "none",
    },
    idle_timeout: {
      title: "Idle timeout",
      retryable: true,
      autoReconnect: "never",
      logLevel: "none",
    },
    remote_closed: {
      title: "Remote closed connection",
      retryable: true,
      autoReconnect: "never",
      logLevel: "error",
    },
    network_lost: {
      title: "Network connection lost",
      retryable: true,
      autoReconnect: "configurable",
      logLevel: "error",
    },
    session_closed: {
      title: "Session closed",
      retryable: true,
      autoReconnect: "never",
      logLevel: "none",
    },
    app_shutdown: {
      title: "App shutdown",
      retryable: false,
      autoReconnect: "never",
      logLevel: "none",
    },
    unknown: {
      title: "Unknown disconnect",
      retryable: true,
      autoReconnect: "never",
      logLevel: "error",
    },
  };

  const issue: ConnectionIssue = {
    code,
    phase: "disconnected",
    messageKey: `connectionDisconnectReasons.${code}`,
    ...base[code],
  };
  if (technicalMessage) {
    issue.technicalMessage = redactSensitiveText(technicalMessage);
  }
  return issue;
}

function connectionIssuePriority(issue: ConnectionIssue): number {
  switch (issue.code) {
    case "target_unreachable":
    case "service_unavailable":
      return 90;
    case "protocol_error":
    case "resource_limit":
      return 80;
    case "not_vnc_service":
      return 75;
    case "security_failure":
    case "unsupported_security":
      return 70;
    case "credentials_required":
    case "authentication_failed":
    case "authentication_cancelled":
    case "connection_cancelled":
      return 60;
    case "session_unavailable":
      return 50;
    case "agent_stream_failed":
      return 40;
    case "remote_closed":
    case "network_lost":
    case "session_closed":
    case "user_requested":
    case "idle_timeout":
    case "app_shutdown":
      return 10;
    case "unknown":
    case "unknown_error":
    default:
      return 0;
  }
}

export function shouldReplaceConnectionIssue(currentIssue: ConnectionIssue | undefined, incomingIssue: ConnectionIssue): boolean {
  if (!currentIssue) {
    return true;
  }

  return connectionIssuePriority(incomingIssue) >= connectionIssuePriority(currentIssue);
}

export function shouldIgnoreFollowupRemoteDisconnectIssue(input: FollowupRemoteDisconnectInput): boolean {
  if (!input.lastIssue) {
    return false;
  }

  if (input.incomingIssue.code !== "remote_closed" || input.incomingIssue.phase !== "disconnected") {
    return false;
  }

  return (
    ["authentication failed", "security failure", "failed", "disconnected unexpectedly"].includes(input.state)
    && !shouldReplaceConnectionIssue(input.lastIssue, input.incomingIssue)
  );
}

export function classifyConnectionFailure(reason: string | NativeRfbFailure): ConnectionFailureClassification {
  if (typeof reason !== "string") {
    const titles: Record<NativeRfbFailure["category"], string> = {
      credentials_required: "Password required",
      authentication_failed: "Authentication failed",
      authentication_cancelled: "Authentication cancelled",
      connection_cancelled: "Connection cancelled",
      unsupported_security: "Unsupported security type",
      security_failure: "Security negotiation failed",
      not_vnc_service: "Not a VNC service",
      protocol_error: "Protocol error",
      resource_limit: "Connection resource limit exceeded",
      target_unreachable: "Target unreachable",
      remote_closed: "Remote closed connection",
      session_unavailable: "Session unavailable",
      unknown_error: "Unknown connection error",
    };
    return { code: reason.category, title: titles[reason.category], messageKey: `connectionErrors.${reason.category}` };
  }
  const normalized = reason.toLowerCase();

  if (
    normalized.includes("rfb_credentials_required") ||
    normalized.includes("requires credentials") ||
    normalized.includes("credentials required") ||
    normalized.includes("missing password")
  ) {
    return {
      code: "credentials_required",
      title: "Password required",
      messageKey: "connectionErrors.credentials_required",
    };
  }

  if (normalized.includes("authentication cancelled") || normalized.includes("authentication_cancelled")) {
    return {
      code: "authentication_cancelled",
      title: "Authentication cancelled",
      messageKey: "connectionErrors.authentication_cancelled",
    };
  }

  if (
    normalized.includes("authentication failed") ||
    normalized.includes("connectionerrors.authentication_failed") ||
    normalized.includes("authentication_failed") ||
    normalized.includes("auth failed") ||
    normalized.includes("wrong password")
  ) {
    return {
      code: "authentication_failed",
      title: "Authentication failed",
      messageKey: "connectionErrors.authentication_failed",
    };
  }

  if (
    normalized.includes("unsupported security") ||
    normalized.includes("unsupported_security_type")
  ) {
    return {
      code: "unsupported_security",
      title: "Unsupported security type",
      messageKey: "connectionErrors.unsupported_security",
    };
  }

  if (normalized.includes("security negotiation failed") || normalized.includes("security failure")) {
    return {
      code: "security_failure",
      title: "Security negotiation failed",
      messageKey: "connectionErrors.security_failure",
    };
  }

  if (
    normalized.includes("not vnc") ||
    normalized.includes("not a vnc") ||
    normalized.includes("non-vnc")
  ) {
    return {
      code: "not_vnc_service",
      title: "Not a VNC service",
      messageKey: "connectionErrors.not_vnc_service",
    };
  }

  if (
    normalized.includes("server closed") ||
    normalized.includes("server_closed_connection") ||
    normalized.includes("remote host closed") ||
    normalized.includes("socket closed") ||
    normalized.includes("socket_closed") ||
    normalized.includes("connection reset") ||
    normalized.includes("connection_reset")
  ) {
    return {
      code: "remote_closed",
      title: "Remote closed connection",
      messageKey: "connectionErrors.remote_closed",
    };
  }

  if (
    normalized.includes("unsupported protocol") ||
    normalized.includes("unsupported_protocol_version") ||
    normalized.includes("protocol handshake") ||
    normalized.includes("protocol_handshake_failed") ||
    normalized.includes("framebuffer") ||
    normalized.includes("framebuffer_init_failed")
  ) {
    return {
      code: "protocol_error",
      title: "Protocol error",
      messageKey: "connectionErrors.protocol_error",
    };
  }

  if (
    normalized.includes("connection refused") ||
    normalized.includes("connection_refused") ||
    normalized.includes("maximum connections") ||
    normalized.includes("too many connections") ||
    normalized.includes("server_max_connections_reached") ||
    normalized.includes("server rejected") ||
    normalized.includes("server_rejected_connection")
  ) {
    return {
      code: "service_unavailable",
      title: "VNC service unavailable",
      messageKey: "connectionErrors.service_unavailable",
    };
  }

  if (
    normalized.includes("dns_resolution_failed") ||
    normalized.includes("dns resolution failed") ||
    normalized.includes("failed to lookup address information") ||
    normalized.includes("nodename nor servname provided") ||
    normalized.includes("name or service not known") ||
    normalized.includes("host_not_found") ||
    normalized.includes("host not found") ||
    normalized.includes("timeout") ||
    normalized.includes("timed out") ||
    normalized.includes("network is unreachable") ||
    normalized.includes("network_unreachable") ||
    normalized.includes("no route to host") ||
    normalized.includes("host_unreachable") ||
    normalized.includes("tcp_unreachable") ||
    normalized.includes("reachable=false") ||
    normalized.includes("could not connect")
  ) {
    return {
      code: "target_unreachable",
      title: "Target unreachable",
      messageKey: "connectionErrors.target_unreachable",
    };
  }

  if (normalized.includes("agent enhanced stream failed") || normalized.includes("agent_stream_failed")) {
    return {
      code: "agent_stream_failed",
      title: "Agent stream failed",
      messageKey: "connectionErrors.agent_stream_failed",
    };
  }

  if (
    normalized.includes("session destroyed") ||
    normalized.includes("session_destroyed") ||
    normalized.includes("reconnect failed") ||
    normalized.includes("reconnect_failed")
  ) {
    return {
      code: "session_unavailable",
      title: "Session unavailable",
      messageKey: "connectionErrors.session_unavailable",
    };
  }

  return {
    code: "unknown_error",
    title: "Unknown connection error",
    messageKey: "connectionErrors.unknown_error",
  };
}
