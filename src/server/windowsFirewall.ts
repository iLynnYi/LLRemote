export const WINDOWS_FIREWALL_NETWORKS = ["domain", "private", "public"] as const;
export type WindowsFirewallNetworkKind = typeof WINDOWS_FIREWALL_NETWORKS[number];
export type WindowsFirewallNetwork = {
  name: WindowsFirewallNetworkKind;
  connected: boolean | null;
  enabled: boolean | null;
};
export type WindowsFirewallOverview = { profiles: WindowsFirewallNetwork[] };

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export function createWindowsFirewallApi(invoke: Invoke) {
  return {
    inspect: () => invoke<WindowsFirewallOverview>("inspect_windows_firewall"),
    openSettings: () => invoke<void>("open_windows_firewall_settings"),
  };
}

const ERRORS = new Set(["unsupported", "unavailable", "network_types_unavailable", "inspect_failed", "settings_failed", "timeout"]);

export function windowsFirewallErrorKey(error: unknown): string {
  const code = /^windows_firewall_([a-z_]+)$/.exec(error instanceof Error ? error.message : String(error))?.[1];
  return `localShare.firewall.errors.${code && ERRORS.has(code) ? code : "unavailable"}`;
}
