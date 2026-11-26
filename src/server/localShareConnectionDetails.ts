import type { LocalVncServerOverview } from "./localShare.ts";
import type { WindowsServiceOverview } from "./windowsService.ts";

type Availability = { valueKey: string; reasonKey?: string };
export type LocalShareConnectionFact = Availability & { id: string; labelKey: string };

type ConnectionDetailsOptions = {
  overview: LocalVncServerOverview | null;
  runtimeUnavailable: boolean;
  permissionUnavailable: boolean;
  service: WindowsServiceOverview | null;
};

const reason = (name: string) => `localShare.connectionCapabilityReasons.${name}`;
const unavailable = (reasonKey: string): Availability => ({ valueKey: "localShare.unavailable", reasonKey });
const unconfirmed = (reasonKey: string): Availability => ({ valueKey: "localShare.unconfirmed", reasonKey });

/** Uses the running instance, never the editable form or a stale capability list. */
export function localShareConnectionFacts(options: ConnectionDetailsOptions): LocalShareConnectionFact[] {
  const { overview, service } = options;
  function runtimeAvailability(): Availability | null {
    if (options.runtimeUnavailable || !overview || overview.status === "unknown") {
      return unconfirmed(reason("statusUnconfirmed"));
    }
    if (overview.status !== "running") {
      return unavailable(overview.status === "stopped" ? reason("sharingStopped")
        : `localShare.serverStatusDetail.${overview.status}`);
    }
    return null;
  }

  function controlAvailability(): Availability {
    const runtime = runtimeAvailability();
    if (runtime || !overview) return runtime ?? unconfirmed(reason("statusUnconfirmed"));
    if (overview.viewOnly) return { valueKey: "localShare.viewOnly", reasonKey: reason("viewOnlyControl") };
    if (service?.desktopRecovering) return unavailable(reason("desktopRecovering"));
    if (service?.inputStatus === "blocked") return unavailable(reason("inputBlocked"));
    if (service?.inputStatus === "waiting_for_frame") return unavailable(reason("inputWaiting"));
    if (service && !["ready", "injected"].includes(service.inputStatus ?? "")) {
      return unconfirmed(reason("inputUnconfirmed"));
    }
    const accessibility = overview.permissions.find(permission => permission.kind === "accessibility");
    if (options.permissionUnavailable || accessibility?.status === "unknown") {
      return unconfirmed(reason("permissionUnconfirmed"));
    }
    if (accessibility?.status === "missing") return unavailable(reason("accessibilityMissing"));
    if (accessibility?.status === "unsupported") return unavailable(reason("controlUnsupported"));
    return { valueKey: "localShare.controllable" };
  }

  function capabilityAvailability(required: string[], requiresControl: boolean, needsUserDesktop: boolean): Availability {
    const runtime = runtimeAvailability();
    if (runtime || !overview) return runtime ?? unconfirmed(reason("statusUnconfirmed"));
    if (requiresControl && overview.viewOnly) return unavailable(reason("viewOnly"));
    if (overview.agent.status === "failed") return unavailable(reason("agentFailed"));
    if (overview.agent.status !== "running" || !overview.agent.port) return unavailable(reason("agentNotReady"));
    // A secure Windows desktop can accept RFB input while user-desktop enhancements are unavailable.
    if (needsUserDesktop && service && ((service.desktop && service.desktop !== "Default")
      || service.userAgentErrorCode === "windows_service_user_agent_login_required")) {
      return unavailable(reason("userDesktopUnavailable"));
    }
    return required.every(capability => overview.agent.capabilities.includes(capability))
      ? { valueKey: "localShare.available" } : unavailable(reason("capabilityMissing"));
  }

  return [
    { id: "control", labelKey: "localShare.controlPermission", ...controlAvailability() },
    { id: "clipboardRead", labelKey: "localShare.clipboardRead",
      ...capabilityAvailability(["clipboard.text.read"], false, true) },
    { id: "clipboardWrite", labelKey: "localShare.clipboardWrite",
      ...capabilityAvailability(["clipboard.text.write"], true, true) },
    { id: "fileTransfer", labelKey: "localShare.fileTransfer",
      ...capabilityAvailability(["file.transfer.upload"], true, false) },
    { id: "sendMessage", labelKey: "localShare.sendMessage",
      ...capabilityAvailability(["message.show"], true, true) },
    { id: "displaySwitch", labelKey: "localShare.remoteDisplaySwitch",
      ...capabilityAvailability(["agent.display.enumerate", "agent.display.select"], false, true) },
  ];
}
