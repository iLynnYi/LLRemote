import type { LocalVncServerOverview, LocalVncServerViewRequest, LocalVncServerPermissionKind, LocalVncServerPermissionRequestResult } from "./localShare.ts";

export type SharingMode = "windowsService" | "background";

export type BackgroundShareOverview = {
  server: LocalVncServerOverview;
  serviceFallback?: boolean;
  temporaryShare?: boolean;
  configured: boolean;
  credentialConfigured: boolean;
  autoStart: boolean;
  startupRegistration?: "enabled" | "missing" | "disabled" | "unknown";
  lastLoginAttempt?: { startedAtMs: number; state: "starting" | "succeeded" | "failed"; errorCode: string | null } | null;
  processRunning: boolean;
  lastErrorCode: string | null;
  runtime: { pid: number; buildId: string } | null;
  currentBuildId: string;
  permissionSource: "background" | "background_cached" | "pending";
  restartSupported: boolean;
  ipcVersion: number;
  signatureChangedPermissions?: LocalVncServerPermissionKind[];
};

export type BackgroundShareStartRequest = {
  bindAddress: string;
  port: number;
  password: string;
  viewOnly: boolean;
  temporaryPassword?: boolean;
};

type BackgroundShareInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

/** Background sharing owns the Server and its optional Agent outside the App. */
export function createBackgroundShareApi(invoke: BackgroundShareInvoke) {
  return {
    status: (inspectPermissions = false) => invoke<BackgroundShareOverview>("get_background_share_status", { inspectPermissions }),
    prepare: (recover = false) => invoke<BackgroundShareOverview>("prepare_background_share", { recover }),
    start: (request: BackgroundShareStartRequest) => invoke<BackgroundShareOverview>("start_background_share", { request }),
    setAutoStart: (enabled: boolean) => invoke<BackgroundShareOverview>("set_background_share_auto_start", { enabled }),
    stop: () => invoke<BackgroundShareOverview>("stop_background_share"),
    restart: () => invoke<BackgroundShareOverview>("restart_background_share"),
    view: (request: LocalVncServerViewRequest) => invoke<BackgroundShareOverview>("update_background_share_view", { request }),
    disconnect: (connectionId: string) => invoke<BackgroundShareOverview>("disconnect_background_share_connection", { connectionId }),
    permission: (kind: LocalVncServerPermissionKind, openSettings = false) => invoke<LocalVncServerPermissionRequestResult>("request_background_share_permission", { kind, openSettings }),
  };
}

const ERRORS = new Set([
  "unsupported", "dispatch_failed", "invalid_request", "invalid_reply", "access_denied", "ipc_unavailable",
  "already_running", "binary_unavailable", "startup_unavailable", "runtime_failed", "user_context_required",
  "user_directory_unavailable", "config_invalid", "config_unavailable", "credential_missing",
  "credential_unavailable", "auto_start_credential_required", "auto_start_permission_required", "auto_start_disabled",
  "service_installed",
  "runtime_outdated", "restart_timeout", "protocol_incompatible", "ipc_busy", "bundle_unavailable", "permission_timeout",
  "permission_reset_failed", "permission_identity_unavailable", "permission_repair_busy",
  "credential_rollback_failed", "initial_credential_rollback_failed", "startup_rollback_failed",
  "temporary_password_timeout",
  "screen_lock_failed", "enhancement_unavailable",
]);

export function backgroundShareErrorKey(error: unknown): string {
  const text = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  const code = /^background_share_([a-z_]+)(?::|$)/.exec(text)?.[1];
  return `localShare.background.errors.${code && ERRORS.has(code) ? code : "runtime_failed"}`;
}
