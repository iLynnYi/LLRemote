import type { LocalVncServerOverview } from "./localShare.ts";
import { backgroundShareErrorKey, type BackgroundShareOverview, type BackgroundShareStartRequest } from "./backgroundShare.ts";

export type WindowsServiceOverview = {
  supported: boolean;
  installed: boolean;
  ipcVersion?: number;
  autoStart?: boolean | null;
  lastServiceAttempt?: { startedAtMs: number; state: "starting" | "succeeded" | "failed"; errorCode: string | null } | null;
  status: string;
  workerSessionId: number | null;
  workerRunning: boolean;
  desktopRecovering?: boolean;
  bindAddress: string | null;
  port: number | null;
  viewOnly: boolean;
  credentialConfigured: boolean;
  passwordManagementAuthorized: boolean;
  passwordManagementSupported?: boolean;
  lastErrorCode: string | null;
  sasPolicy: string;
  sasErrorCode?: string | null;
  sasRequestCount?: number;
  desktop?: string | null;
  inputStatus?: string | null;
  inputFailures?: number;
  userAgentRunning?: boolean;
  userAgentPort?: number | null;
  userAgentErrorCode?: string | null;
  userAgentRepairSupported?: boolean;
  server?: LocalVncServerOverview | null;
};

export type WindowsServiceConfigureRequest = {
  bindAddress: string;
  port: number;
  viewOnly: boolean;
  password: string;
};

export type WindowsServicePasswordRequest = { password: string };
export type ServiceStartOutcome = {
  service: WindowsServiceOverview | null;
  background: BackgroundShareOverview | null;
  serviceError: string | null;
  fallbackError: string | null;
  passwordRequired: boolean;
};

export type WindowsServiceRepairItem = {
  kind: "background" | "service" | "permissions" | "sas" | "agent" | "elevation";
  status: "healthy" | "repaired" | "deferred" | "failed" | "needsAdministrator" | "restartRequired" | "unknown";
  errorCode: string | null;
};
export type WindowsServiceRepairReport = { overview: WindowsServiceOverview; items: WindowsServiceRepairItem[] };

type WindowsServiceInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export type WindowsServiceAction = "install" | "uninstall" | "start" | "stop" | "enableSas" | "repair" | "enableAutoStart" | "disableAutoStart" | "authorizePasswordManagement";

/** Exposes service maintenance independently of the user background runtime. */
export function createWindowsServiceApi(invoke: WindowsServiceInvoke) {
  return {
    status: () => invoke<WindowsServiceOverview>("get_windows_service_status"),
    serverStatus: () => invoke<LocalVncServerOverview>("get_windows_service_server_status"),
    disconnect: (connectionId: string) => invoke<LocalVncServerOverview>("disconnect_windows_service_connection", { connectionId }),
    install: (request?: WindowsServiceConfigureRequest, useBackgroundPassword = false) => invoke<WindowsServiceOverview>("install_windows_service", {
      request: request ?? null, ...(useBackgroundPassword ? { useBackgroundPassword: true } : {}),
    }),
    uninstall: () => invoke<WindowsServiceOverview>("uninstall_windows_service"),
    configure: (request: WindowsServiceConfigureRequest) => invoke<WindowsServiceOverview>(
      "configure_windows_service", { request },
    ),
    setPassword: (request: WindowsServicePasswordRequest) => invoke<WindowsServiceOverview>("set_windows_service_password", request),
    authorizePasswordManagement: () => invoke<WindowsServiceOverview>("authorize_windows_service_password_management"),
    start: () => invoke<WindowsServiceOverview>("start_windows_service"),
    startWithFallback: (request: BackgroundShareStartRequest, useBackgroundPassword = false) => invoke<ServiceStartOutcome>("start_windows_service_with_fallback", {
      request, ...(useBackgroundPassword ? { useBackgroundPassword: true } : {}),
    }),
    stop: () => invoke<WindowsServiceOverview>("stop_windows_service"),
    enableSas: () => invoke<WindowsServiceOverview>("enable_windows_service_sas"),
    repair: () => invoke<WindowsServiceRepairReport>("repair_windows_service"),
    enableAutoStart: () => invoke<WindowsServiceOverview>("set_windows_service_auto_start", { enabled: true }),
    disableAutoStart: () => invoke<WindowsServiceOverview>("set_windows_service_auto_start", { enabled: false }),
  };
}

export const WINDOWS_SERVICE_STATUSES = [
  "not_installed", "stopped", "start_pending", "running", "stop_pending", "unknown", "unsupported",
] as const;

export const WINDOWS_SERVICE_SAS_POLICIES = [
  "services_allowed", "services_denied", "not_configured", "unknown", "unsupported",
] as const;

const WINDOWS_SERVICE_ERROR_CODES = new Set([
  "unsupported", "admin_required", "uac_cancelled", "elevation_denied", "elevation_failed", "binary_missing", "not_installed",
  "uac_disabled", "uac_status_unknown", "uac_repair_failed",
  "credential_missing", "invalid_config", "ipc_unavailable", "ipc_timeout", "access_denied",
  "start_failed", "stop_failed", "install_failed", "uninstall_failed", "configure_failed",
  "install_copy_failed", "install_register_failed", "install_authorization_failed",
  "worker_start_failed", "worker_exited", "worker_identity_failed", "port_conflict", "sas_policy_denied", "sas_policy_configure_failed", "sas_unavailable", "internal",
  "user_agent_login_required", "user_agent_identity_failed", "user_agent_unavailable",
  "user_agent_repair_timeout",
  "desktop_relay_unavailable",
  "screen_lock_failed",
  "protocol_incompatible",
  "password_management_authorization_required",
  "password_management_authorization_failed", "password_management_unsupported",
]);
const WINDOWS_INPUT_ERROR_CODES = new Set(["desktop_unavailable", "desktop_bind_failed", "injection_failed"]);

/** Only stable backend categories reach the UI; native error text is never shown. */
export function windowsServiceErrorKey(error: unknown): string {
  const value = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  const code = /^windows_service_([a-z_]+)(?::|$)/.exec(value)?.[1];
  const inputCode = /^windows_input_([a-z_]+)(?::|$)/.exec(value)?.[1];
  if (inputCode && WINDOWS_INPUT_ERROR_CODES.has(inputCode)) return `localShare.windowsService.errors.input_${inputCode}`;
  return `localShare.windowsService.errors.${code && WINDOWS_SERVICE_ERROR_CODES.has(code) ? code : "internal"}`;
}

/** One repair entry covers both Windows service and ordinary user sharing. */
export function sharingRepairErrorKey(error: unknown): string {
  const value = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  return value.startsWith("background_share_") ? backgroundShareErrorKey(value) : windowsServiceErrorKey(error);
}
