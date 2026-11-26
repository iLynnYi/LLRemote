/**
 * The complete in-memory description of a connected Session.  It deliberately
 * separates a target from evidence about the server behind it.
 */
export type BaseTransport = { kind: "vnc" };

export type ServerIdentity = {
  id?: string;
  kind: "managed" | "unknown";
  verification: "local-runtime" | "authenticated-agent" | "none";
  vendor?: string;
  product?: string;
  version?: string;
  protocolVersion?: string;
  hint?: "desktop-name" | "anonymous-agent";
};

/** Non-secret metadata declared by the RFB peer; it is not identity evidence. */
export type ServerDeclaration = { id: string } & Pick<ServerIdentity, "vendor" | "product" | "version" | "protocolVersion">;

export type LocalEndpointMatch =
  | {
      state: "matched";
      viewOnly: boolean;
      selectedDisplayId?: string;
      framebufferWidth?: number;
      framebufferHeight?: number;
    }
  | { state: "not-matched" };

export type EnhancedChannelStatus = "unavailable" | "negotiating" | "available" | "degraded";

export type CapabilityState = "supported" | "unsupported" | "unknown" | "degraded" | "best-effort";

export type CapabilityItem = {
  id: string;
  state: CapabilityState;
  source: "rfb" | "agent" | "local-runtime";
  reasonCode?: string;
};

export type CapabilityMatrix = {
  items: CapabilityItem[];
};

export type EnhancedChannel = {
  status: EnhancedChannelStatus;
  endpoint?: {
    hostMode: "rfb-peer";
    port: number;
    protocolVersion?: string;
    authenticationMethods?: string[];
    source: "server-discovery" | "local-runtime";
  };
  authentication: "not-attempted" | "verified" | "failed";
  lastErrorCode?: string;
};

export type SessionConnectionModel = {
  baseTransport: BaseTransport;
  serverIdentity: ServerIdentity;
  serverDeclaration?: ServerDeclaration;
  localEndpointMatch: LocalEndpointMatch;
  enhancedChannel: EnhancedChannel;
  capabilities: CapabilityMatrix;
};

/**
 * Produces a one-line, secret-free snapshot for the connection event log.
 * This is intentionally derived from the negotiated model, never from the
 * endpoint probing layer, so a connection makes its actual evidence visible.
 */
export function formatConnectionModelDiagnostics(
  model: SessionConnectionModel,
  stage: "connected" | "discovery" | "agent",
): string {
  const identity = model.serverIdentity;
  const declaration = model.serverDeclaration;
  const endpoint = model.enhancedChannel.endpoint;
  const capabilities = model.capabilities.items
    .map((item) => `${item.id}:${item.state}`)
    .join(",") || "none";
  const discovery = endpoint?.source === "server-discovery"
    ? `negotiated(v${endpoint.protocolVersion ?? "unknown"})`
    : "not-advertised";
  const endpointText = endpoint ? `${endpoint.hostMode}:${endpoint.port}` : "none";
  return [
    `Connection model ${stage}:`,
    `base_transport=${model.baseTransport.kind}`,
    `server_identity=${identity.kind}`,
    `identity_verification=${identity.verification}`,
    `identity_id=${identity.id ?? "none"}`,
    `identity_product=${[identity.vendor, identity.product, identity.version].filter(Boolean).join("/") || "unknown"}`,
    `declared_identity_id=${JSON.stringify(declaration?.id?.slice(0, 256) ?? "none")}`,
    `declared_product=${JSON.stringify([declaration?.vendor, declaration?.product, declaration?.version].filter(Boolean).join("/").slice(0, 256) || "unknown")}`,
    `discovery=${discovery}`,
    `enhanced_transport=${endpointText}`,
    `agent_authentication=${model.enhancedChannel.authentication}`,
    `enhanced_channel=${model.enhancedChannel.status}`,
    `capabilities=${capabilities}`,
    `local_endpoint_match=${model.localEndpointMatch.state}`,
    `failure=${model.enhancedChannel.lastErrorCode ?? "none"}`,
  ].join(" ");
}

const RFB_CAPABILITIES: CapabilityItem[] = [
  { id: "framebuffer", state: "supported", source: "rfb" },
  { id: "input.pointer", state: "supported", source: "rfb" },
  { id: "input.keyboard", state: "supported", source: "rfb" },
  { id: "clipboard.text.best-effort", state: "best-effort", source: "rfb" },
];

function identityWithoutAgent(local: LocalEndpointMatch): ServerIdentity {
  return local.state === "matched"
    ? { kind: "managed", verification: "local-runtime", vendor: "LLRemote", product: "embedded-server" }
    : { kind: "unknown", verification: "none" };
}

export function createInitialConnectionModel(
  localEndpointMatch: LocalEndpointMatch = { state: "not-matched" },
): SessionConnectionModel {
  const local = localEndpointMatch.state === "matched";
  return {
    baseTransport: { kind: "vnc" },
    serverIdentity: identityWithoutAgent(localEndpointMatch),
    localEndpointMatch,
    enhancedChannel: {
      status: "unavailable",
      authentication: "not-attempted",
      lastErrorCode: local ? "agent_requires_server_discovery" : "discovery_not_negotiated",
    },
    capabilities: { items: RFB_CAPABILITIES.map((item) => ({ ...item })) },
  };
}

export function connectionModelFromDiscovery(
  current: SessionConnectionModel,
  manifest: {
    server: { id: string; vendor?: string; product?: string; buildVersion?: string; identityVerification?: "server-discovery" | "local-runtime" | "authenticated-agent" };
    policy?: { viewOnly?: boolean };
    capabilities?: Array<{ id: string; state: "supported" | "unsupported" | "degraded"; reasonCode?: string }>;
    agent?: { hostMode: "rfb-peer"; port: number; protocolVersion: string; authenticationMethods: string[]; bootstrapKey?: string; expiresAt?: number };
  },
): SessionConnectionModel {
  const canUseAgent = Boolean(
    manifest.agent?.bootstrapKey
      && manifest.agent.authenticationMethods.includes("agent-ticket"),
  ) || Boolean(
    current.localEndpointMatch.state === "matched"
      && manifest.agent?.authenticationMethods.includes("local-runtime"),
  );
  return {
    ...current,
    serverIdentity: identityWithoutAgent(current.localEndpointMatch),
    serverDeclaration: { id: manifest.server.id, vendor: manifest.server.vendor, product: manifest.server.product,
      version: manifest.server.buildVersion, protocolVersion: "1.0" },
    enhancedChannel: {
      status: canUseAgent ? "negotiating" : "unavailable",
      authentication: "not-attempted",
      endpoint: manifest.agent
        ? {
            hostMode: manifest.agent.hostMode,
            port: manifest.agent.port,
            protocolVersion: manifest.agent.protocolVersion,
            authenticationMethods: manifest.agent.authenticationMethods,
            source: "server-discovery",
          }
        : undefined,
      lastErrorCode: manifest.agent ? (canUseAgent ? undefined : "authentication_required") : "agent_not_advertised",
    },
    // Discovery declarations are not executable capabilities. Only a later
    // authenticated Agent session may add effective Agent capabilities.
    capabilities: { items: RFB_CAPABILITIES.map((item) => ({ ...item })) },
  };
}

type AuthenticatedAgentDescription = { serverId: string; viewOnly: boolean; capabilities: string[] };

function validAgentDescription(value: unknown): value is AuthenticatedAgentDescription {
  if (typeof value !== "object" || value === null) return false;
  const description = value as Record<string, unknown>;
  return typeof description.serverId === "string" && description.serverId.trim().length > 0
    && typeof description.viewOnly === "boolean" && Array.isArray(description.capabilities)
    && description.capabilities.length <= 256
    && description.capabilities.every((id) => typeof id === "string" && id.length > 0 && id.length <= 256);
}

/** Called after session authentication; binds its description to this RFB declaration. */
export function connectionModelFromAuthenticatedAgent(
  current: SessionConnectionModel,
  description: unknown,
): SessionConnectionModel {
  const valid = validAgentDescription(description);
  const identityMatches = valid && current.serverDeclaration?.id === description.serverId;
  const policyMatches = valid && (current.localEndpointMatch.state !== "matched"
    || current.localEndpointMatch.viewOnly === description.viewOnly);
  if (!valid || !identityMatches || !policyMatches) {
    return {
      ...current,
      serverIdentity: identityWithoutAgent(current.localEndpointMatch),
      capabilities: { items: RFB_CAPABILITIES.map((item) => ({ ...item })) },
      enhancedChannel: {
        ...current.enhancedChannel,
        status: "degraded",
        authentication: "failed",
        lastErrorCode: !valid ? "agent_description_invalid" : identityMatches ? "agent_policy_mismatch" : "agent_identity_mismatch",
      },
    };
  }
  return {
    ...current,
    serverIdentity: { ...identityWithoutAgent(current.localEndpointMatch), id: description.serverId,
      kind: "managed", verification: "authenticated-agent" },
    enhancedChannel: { ...current.enhancedChannel, status: "available", authentication: "verified", lastErrorCode: undefined },
    capabilities: {
      items: [
        ...RFB_CAPABILITIES.map((item) => ({ ...item })),
        ...description.capabilities.map((id) => ({ id, state: "supported" as const, source: "agent" as const })),
      ],
    },
  };
}

export function sessionSupportsCapability(
  capabilities: CapabilityMatrix | undefined | null,
  capability: string,
): boolean {
  return capabilities?.items.some((item) => item.id === capability && item.state === "supported") ?? false;
}
