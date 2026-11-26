import { computed, onBeforeUnmount, ref, type Ref } from "vue";
import type { AppMessageTone, Translate } from "@app/types";
import { isValidIpv4Address, type LocalVncServerOverview } from "@server/localShare";
import {
  createWindowsServiceApi, windowsServiceErrorKey, sharingRepairErrorKey, type WindowsServiceAction, type WindowsServiceOverview,
  type WindowsServiceConfigureRequest, type WindowsServiceRepairItem,
} from "@server/windowsService";

export function useWindowsService(options: {
  t: Translate;
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  normalServer: Readonly<Ref<LocalVncServerOverview | null>>;
}) {
  const api = createWindowsServiceApi(options.invoke);
  const overview = ref<WindowsServiceOverview | null>(null);
  const busy = ref(false);
  const repairing = ref(false);
  const repairItems = ref<WindowsServiceRepairItem[]>([]);
  const bindAddress = ref("0.0.0.0");
  const port = ref<number | null>(5904);
  const viewOnly = ref(false);
  const password = ref("");
  const message = ref("");
  const messageTone = ref<AppMessageTone>("warning");
  let alive = true;
  let formInitialized = false;

  const running = computed(() => ["running", "start_pending", "stop_pending"].includes(overview.value?.status ?? ""));
  const configured = computed(() => Boolean(overview.value?.bindAddress && overview.value.port && overview.value.credentialConfigured));
  const needsSetup = computed(() => !overview.value?.installed || !configured.value);
  const bindAddressInvalid = computed(() => !isValidIpv4Address(bindAddress.value));
  const portInvalid = computed(() => port.value === null || !Number.isInteger(port.value) || port.value < 1 || port.value > 65535);
  const formValid = computed(() => !bindAddressInvalid.value && !portInvalid.value);
  const passwordValid = computed(() => password.value.length > 0 && new TextEncoder().encode(password.value).length <= 8);
  const canSetup = computed(() => !busy.value && overview.value?.supported === true && !running.value
    && formValid.value && (password.value.length > 0 ? passwordValid.value : Boolean(overview.value?.credentialConfigured)));
  const canStart = computed(() => !busy.value && overview.value?.status === "stopped" && Boolean(overview.value?.installed)
    && formValid.value && (password.value.length > 0 ? passwordValid.value : Boolean(overview.value?.credentialConfigured)));
  const configurationDirty = computed(() => bindAddress.value.trim() !== overview.value?.bindAddress
    || port.value !== overview.value?.port || viewOnly.value !== overview.value?.viewOnly);
  const endpointConflict = computed(() => {
    const normal = options.normalServer.value;
    const service = overview.value;
    return normal?.status === "running" && !service?.server && normal.port === service?.port
      && (normal.bindAddress === "0.0.0.0" || service.bindAddress === "0.0.0.0" || normal.bindAddress === service.bindAddress);
  });

  function acceptOverview(value: WindowsServiceOverview, updateForm = false) {
    if (!alive) return;
    if (overview.value && overview.value.installed !== value.installed) repairItems.value = [];
    overview.value = value;
    if (updateForm || !formInitialized) {
      bindAddress.value = value.bindAddress ?? "0.0.0.0";
      port.value = value.port ?? 5904;
      viewOnly.value = value.viewOnly;
      formInitialized = true;
    }
  }

  /** Start saves only the changed fields, then starts after a successful save and intent check. */
  async function act(action: WindowsServiceAction, shouldContinue: () => boolean = () => true, startAfterInstall = false,
    start: (request: WindowsServiceConfigureRequest) => Promise<WindowsServiceOverview> = () => api.start()): Promise<boolean> {
    if (busy.value || !overview.value?.supported) return false;
    const settingUp = (action === "install" && needsSetup.value)
      || (action === "start" && (!overview.value.bindAddress || !overview.value.port));
    if (settingUp && (!formValid.value || (password.value.length > 0 ? !passwordValid.value : !overview.value.credentialConfigured))) {
      const invalidInput = !formValid.value || password.value.length > 0;
      password.value = "";
      message.value = options.t(invalidInput
        ? "localShare.windowsService.errors.invalid_config" : "localShare.windowsService.errors.credential_missing");
      messageTone.value = "warning";
      return false;
    }
    if (action === "enableAutoStart" && !overview.value.credentialConfigured) {
      message.value = options.t("localShare.windowsService.errors.credential_missing");
      messageTone.value = "warning";
      return false;
    }
    if (action === "start" && (!canStart.value || endpointConflict.value)) {
      const invalidInput = !formValid.value || (password.value.length > 0 && !passwordValid.value);
      password.value = "";
      message.value = options.t(endpointConflict.value ? "localShare.windowsService.portConflict"
        : invalidInput
          ? "localShare.windowsService.errors.invalid_config" : "localShare.windowsService.errors.credential_missing");
      messageTone.value = "warning";
      return false;
    }
    const request: WindowsServiceConfigureRequest | null = action === "start" || settingUp ? {
      bindAddress: bindAddress.value.trim(), port: port.value!, viewOnly: viewOnly.value, password: password.value,
    } : null;
    const needsConfiguration = action === "start" && configurationDirty.value;
    // Keep one private copy for this attempt's handoff; release API argument
    // objects as soon as their own operation completes.
    const startRequest = request ? { ...request } : {
      bindAddress: bindAddress.value.trim(), port: port.value!, viewOnly: viewOnly.value, password: password.value,
    };
    if (request) password.value = "";
    busy.value = true;
    message.value = "";
    const updating = action === "install" && overview.value.installed;
    try {
      if (action === "repair") {
        repairing.value = true;
        repairItems.value = [];
        const report = await api.repair();
        if (!alive) return false;
        acceptOverview(report.overview);
        repairItems.value = report.items;
        const failed = report.items.some(item => item.status === "failed" || item.status === "unknown");
        const needsAdmin = report.items.some(item => item.status === "needsAdministrator");
        const restart = report.items.some(item => item.status === "restartRequired");
        const deferred = report.items.some(item => item.status === "deferred");
        const repaired = report.items.some(item => item.status === "repaired");
        const result = failed ? "incomplete" : needsAdmin ? "needsAdministrator" : restart ? "restartRequired"
          : deferred ? "deferred" : repaired ? "repaired" : "healthy";
        message.value = options.t(`localShare.windowsService.repairResult.${result}`);
        if (restart && result !== "restartRequired") message.value += ` ${options.t("localShare.windowsService.repairResult.restartRequired")}`;
        messageTone.value = failed || needsAdmin ? "warning" : restart || deferred ? "info" : "success";
        return !failed && !needsAdmin;
      }
      if (settingUp && request) {
        if (!alive || !shouldContinue()) return false;
        acceptOverview(await api.install(request), true);
        request.password = "";
        if ((action === "start" || startAfterInstall) && alive && shouldContinue()) acceptOverview(await start(startRequest));
      } else if (request) {
        if (!alive || !shouldContinue()) return false;
        if (needsConfiguration) {
          acceptOverview(await api.configure(request), true);
        } else if (request.password.length > 0) {
          if (!overview.value.passwordManagementAuthorized) throw new Error("windows_service_password_management_authorization_required");
          const passwordRequest = { password: request.password };
          try { acceptOverview(await api.setPassword(passwordRequest)); }
          finally { passwordRequest.password = ""; }
        }
        request.password = "";
        if (!alive || !shouldContinue()) return false;
        acceptOverview(await start(startRequest));
      } else {
        acceptOverview(await api[action](), action === "install");
        if (action === "install" && startAfterInstall && alive && shouldContinue()) acceptOverview(await start(startRequest));
      }
      if (alive) {
        message.value = options.t(`localShare.windowsService.done.${updating ? "update" : action}`);
        messageTone.value = "success";
      }
      return true;
    } catch (error) {
      const key = action === "repair" ? sharingRepairErrorKey(error) : windowsServiceErrorKey(error);
      if (action === "install" && (settingUp || key === "localShare.windowsService.errors.install_authorization_failed")) {
        // SCM registration has already completed. Reflect that partial result
        // while keeping the authorization failure visible and retryable.
        try { acceptOverview(await api.status(), true); } catch { /* Keep the last known state. */ }
      }
      if (alive) {
        message.value = options.t(key);
        messageTone.value = key === "localShare.windowsService.errors.uac_cancelled" ? "info" : "error";
      }
      return false;
    } finally {
      if (request) request.password = "";
      startRequest.password = "";
      busy.value = false;
      repairing.value = false;
    }
  }

  onBeforeUnmount(() => {
    alive = false;
    password.value = "";
  });

  return {
    overview, busy, repairing, repairItems, bindAddress, port, viewOnly, password, message, messageTone,
    running, configured, needsSetup, canSetup, formValid, bindAddressInvalid, portInvalid, passwordValid, canStart, endpointConflict,
    act, acceptOverview,
  };
}
