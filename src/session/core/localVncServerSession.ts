import type { SessionDisplayInfo } from "../viewport/sessionDisplay.ts";

export type LocalVncServerSessionOverview = {
  status: string;
  bindAddress?: string;
  port: number;
  addresses: Array<{
    address: string;
  }>;
  displays: Array<{
    id: string;
    label: string;
  }>;
  framebufferWidth: number;
  framebufferHeight: number;
  viewOnly?: boolean;
};

export function isLoopbackHost(value: string): boolean {
  const normalized = normalizeHost(value);
  return normalized === "localhost"
    || normalized === "::1"
    || normalized.startsWith("127.");
}

function normalizeHost(value: string): string {
  const normalized = value.trim().toLowerCase();
  return normalized.startsWith("[") && normalized.endsWith("]")
    ? normalized.slice(1, -1)
    : normalized;
}

export function matchesLocalVncServerTarget(
  overview: LocalVncServerSessionOverview,
  targetHost: string,
  targetPort: number,
): boolean {
  if (overview.status !== "running" || overview.port !== targetPort) {
    return false;
  }

  if (isLoopbackHost(targetHost)) {
    return true;
  }

  const normalizedTargetHost = normalizeHost(targetHost);
  const normalizedBindAddress = overview.bindAddress ? normalizeHost(overview.bindAddress) : "";
  if (
    normalizedBindAddress
    && normalizedBindAddress !== "0.0.0.0"
    && normalizedBindAddress !== "::"
    && normalizedBindAddress === normalizedTargetHost
  ) {
    return true;
  }

  return overview.addresses.some((item) => normalizeHost(item.address) === normalizedTargetHost);
}

export function createDisplayInfoFromLocalVncServer(
  overview: LocalVncServerSessionOverview,
): SessionDisplayInfo {
  const width = Math.max(0, Math.round(overview.framebufferWidth));
  const height = Math.max(0, Math.round(overview.framebufferHeight));

  return {
    width,
    height,
    aspectRatio: height > 0 ? width / height : 0,
    multiDisplayHint: overview.displays.length > 1,
  };
}
