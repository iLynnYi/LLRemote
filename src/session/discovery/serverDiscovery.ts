export type ServerDiscoveryManifest = {
  version: string;
  server: {
    id: string;
    vendor?: string;
    product?: string;
    buildVersion?: string;
    identityVerification?: "server-discovery" | "local-runtime" | "authenticated-agent";
  };
  policy?: { viewOnly?: boolean };
  capabilities?: Array<{
    id: string;
    state: "supported" | "unsupported" | "degraded";
    reasonCode?: string;
  }>;
  agent?: {
    hostMode: "rfb-peer";
    port: number;
    protocolVersion: string;
    authenticationMethods: string[];
    bootstrapKey?: string;
    expiresAt?: number;
  };
};

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function strings(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

const optionalString = (value: unknown) => typeof value === "string" ? value : undefined;

/** Validates the untrusted wire boundary; no claimed verification is copied. */
export function parseServerDiscoveryManifest(value: unknown): ServerDiscoveryManifest | undefined {
  if (!record(value) || value.version !== "1.0" || !record(value.server)
    || typeof value.server.id !== "string" || !value.server.id.trim()) return undefined;
  const manifest: ServerDiscoveryManifest = {
    version: "1.0",
    server: { id: value.server.id, vendor: optionalString(value.server.vendor),
      product: optionalString(value.server.product), buildVersion: optionalString(value.server.buildVersion) },
  };
  if (value.policy != null) {
    if (!record(value.policy) || (value.policy.viewOnly !== undefined && typeof value.policy.viewOnly !== "boolean")) return undefined;
    manifest.policy = { viewOnly: value.policy.viewOnly };
  }
  if (value.capabilities != null) {
    if (!Array.isArray(value.capabilities)) return undefined;
    manifest.capabilities = [];
    for (const capability of value.capabilities) {
      if (!record(capability) || typeof capability.id !== "string"
        || (capability.state !== "supported" && capability.state !== "unsupported" && capability.state !== "degraded")) return undefined;
      manifest.capabilities.push({ id: capability.id, state: capability.state, reasonCode: optionalString(capability.reasonCode) });
    }
  }
  if (value.agent != null) {
    const agent = value.agent;
    if (!record(agent) || agent.hostMode !== "rfb-peer" || typeof agent.port !== "number"
      || !Number.isInteger(agent.port) || agent.port < 1 || agent.port > 65535
      || typeof agent.protocolVersion !== "string" || !agent.protocolVersion
      || !strings(agent.authenticationMethods)
      || (agent.bootstrapKey != null && typeof agent.bootstrapKey !== "string")
      || (agent.expiresAt != null && (typeof agent.expiresAt !== "number" || !Number.isSafeInteger(agent.expiresAt) || agent.expiresAt < 0))) return undefined;
    manifest.agent = { hostMode: "rfb-peer", port: agent.port, protocolVersion: agent.protocolVersion,
      authenticationMethods: [...agent.authenticationMethods], bootstrapKey: optionalString(agent.bootstrapKey),
      expiresAt: typeof agent.expiresAt === "number" ? agent.expiresAt : undefined };
  }
  return manifest;
}
