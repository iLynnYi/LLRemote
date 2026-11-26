import { listen } from "@tauri-apps/api/event";
import {
  connectionModelFromDiscovery,
  connectionModelFromAuthenticatedAgent,
  createInitialConnectionModel,
  formatConnectionModelDiagnostics,
  sessionSupportsCapability,
} from "../../connection-model/connectionModel.ts";
import type { SessionConnectionModel } from "../../connection-model/connectionModel.ts";
import { parseServerDiscoveryManifest } from "../../discovery/serverDiscovery.ts";
import type {
  AgentFileUploadOptions,
  AgentFileUploadProgress,
  AgentFileUploadResult,
  InvokeCommand,
} from "./types.ts";

const AUTHENTICATION_ERROR_CODES = new Set([
  "agent_auth_failed", "agent_protocol_version_unsupported", "agent_ticket_expired",
  "agent_session_open_failed", "agent_session_describe_failed", "agent_session_invalid",
  "agent_session_required", "agent_session_state_poisoned", "agent_authentication_required",
]);

export type NativeAgentRuntime = {
  readonly connectionModel: SessionConnectionModel;
  consumeDiscovery: (manifest: unknown) => Promise<void>;
  invalidateDiscovery: (code: "server_discovery_error" | "server_discovery_failed" | "server_discovery_invalid_manifest") => void;
  invoke: (type: string, payload?: Record<string, unknown>) => Promise<Record<string, unknown>>;
  upload: (filePath: string, options?: AgentFileUploadOptions) => Promise<AgentFileUploadResult>;
  dispose: () => void;
};

/** Keeps optional Agent lifecycle independent from the mandatory native RFB transport. */
export function createNativeAgentRuntime(options: {
  invokeCommand: InvokeCommand;
  host: string;
  vncPort: number;
  initialConnectionModel: SessionConnectionModel;
  onConnectionModelChanged?: (model: SessionConnectionModel) => void;
  onRemoteClipboardText?: (text: string, channel: "agent") => void;
  onEvent: (source: string, message: string, level?: "info" | "warning" | "error") => void;
  listenUploadProgress?: (handler: (progress: AgentFileUploadProgress) => void) => Promise<() => void>;
}): NativeAgentRuntime {
  let connectionModel = options.initialConnectionModel;
  let remoteAgentSessionId: string | undefined;
  let remoteAgentPort: number | undefined;
  let clipboardPoll: ReturnType<typeof setInterval> | null = null;
  let lastClipboardRevision = "";
  let lastClipboardError = "";
  let disposed = false;
  let discoveryGeneration = 0;
  const listenUploadProgress = options.listenUploadProgress ?? ((handler: (progress: AgentFileUploadProgress) => void) =>
    listen<AgentFileUploadProgress>("agent-file-upload-progress", (event) => handler(event.payload)));

  function publishModel(stage: "discovery" | "agent") {
    if (disposed) return;
    options.onEvent("connection-model", formatConnectionModelDiagnostics(connectionModel, stage), "info");
    options.onConnectionModelChanged?.(connectionModel);
  }

  async function invoke(type: string, payload: Record<string, unknown> = {}) {
    if (disposed) throw new Error("agent_channel_unavailable");
    if (connectionModel.enhancedChannel.status !== "available") {
      throw new Error(connectionModel.enhancedChannel.lastErrorCode ?? "agent_channel_unavailable");
    }
    const port = remoteAgentPort ?? connectionModel.enhancedChannel.endpoint?.port;
    if (!port) throw new Error("agent_endpoint_unavailable");
    if (!remoteAgentSessionId) throw new Error("agent_authentication_required");
    const generation = discoveryGeneration;
    const response = await options.invokeCommand("invoke_remote_agent", {
      request: { host: options.host, port, type, sessionId: remoteAgentSessionId, ...payload },
    }) as Record<string, unknown>;
    if (disposed || generation !== discoveryGeneration) throw new Error("agent_channel_unavailable");
    return response;
  }

  function startClipboardPolling(capabilities: string[]) {
    if (disposed || clipboardPoll || !capabilities.includes("clipboard.text.read")) return;
    const generation = discoveryGeneration;
    let pending = false;
    clipboardPoll = setInterval(() => {
      if (disposed || pending || generation !== discoveryGeneration) return;
      pending = true;
      void invoke("clipboard.text.read").then((response) => {
        if (disposed || generation !== discoveryGeneration) return;
        lastClipboardError = "";
        const text = typeof response.text === "string" ? response.text : "";
        const revision = typeof response.revision === "string" ? response.revision : "";
        if (text && revision && revision !== lastClipboardRevision) {
          lastClipboardRevision = revision;
          options.onRemoteClipboardText?.(text, "agent");
        }
      }).catch((error) => {
        if (disposed || generation !== discoveryGeneration) return;
        const message = error instanceof Error ? error.message : String(error);
        if (message !== lastClipboardError) {
          lastClipboardError = message;
          options.onEvent("clipboard", `Enhanced clipboard read failed: ${message}`, "warning");
        }
      }).finally(() => { pending = false; });
    }, 1000);
  }

  function resetAgentAttempt() {
    discoveryGeneration += 1;
    if (clipboardPoll) clearInterval(clipboardPoll);
    clipboardPoll = null;
    remoteAgentSessionId = undefined;
    remoteAgentPort = undefined;
    lastClipboardRevision = "";
    lastClipboardError = "";
    return discoveryGeneration;
  }

  function invalidateDiscovery(code: "server_discovery_error" | "server_discovery_failed" | "server_discovery_invalid_manifest") {
    if (disposed) return;
    resetAgentAttempt();
    connectionModel = createInitialConnectionModel(connectionModel.localEndpointMatch);
    connectionModel.enhancedChannel = { status: "degraded", authentication: "failed", lastErrorCode: code };
    publishModel("discovery");
  }

  async function consumeDiscovery(value: unknown) {
    if (disposed) return;
    const manifest = parseServerDiscoveryManifest(value);
    if (!manifest) {
      invalidateDiscovery("server_discovery_invalid_manifest");
      return;
    }
    const generation = resetAgentAttempt();
    connectionModel = connectionModelFromDiscovery(connectionModel, manifest);
    publishModel("discovery");
    const descriptor = manifest.agent;
    if (!descriptor) return;
    try {
      let sessionId: string | undefined;
      let description: unknown;
      if (descriptor.protocolVersion !== "1.0") throw new Error("agent_protocol_version_unsupported");
      if (descriptor.bootstrapKey && descriptor.authenticationMethods.includes("agent-ticket")) {
        if (descriptor.expiresAt !== undefined && Date.now() >= descriptor.expiresAt * 1000) {
          throw new Error("agent_ticket_expired");
        }
        const opened = await options.invokeCommand("invoke_remote_agent", {
          request: {
            host: options.host,
            port: descriptor.port,
            type: "agent.session.open",
            bootstrapKey: descriptor.bootstrapKey,
          },
        }) as { sessionId?: unknown; error?: unknown } | null;
        if (disposed || generation !== discoveryGeneration) return;
        if (typeof opened?.sessionId !== "string" || !opened.sessionId || opened.sessionId.length > 256) {
          throw new Error(opened?.error === "agent_auth_failed" ? "agent_auth_failed" : "agent_session_open_failed");
        }
        sessionId = opened.sessionId;
        const described = await options.invokeCommand("invoke_remote_agent", {
          request: {
            host: options.host,
            port: descriptor.port,
            type: "agent.session.describe",
            sessionId: opened.sessionId,
          },
        }) as { description?: unknown; error?: unknown } | null;
        if (disposed || generation !== discoveryGeneration) return;
        if (!described?.description) {
          const code = described?.error;
          throw new Error(code === "agent_session_invalid" || code === "agent_session_required" || code === "agent_session_state_poisoned"
            ? code : "agent_session_describe_failed");
        }
        description = described.description;
      } else {
        throw new Error("agent_authentication_required");
      }
      connectionModel = connectionModelFromAuthenticatedAgent(connectionModel, description);
      if (connectionModel.enhancedChannel.status === "available") {
        remoteAgentSessionId = sessionId;
        remoteAgentPort = descriptor.port;
        startClipboardPolling(connectionModel.capabilities.items.filter((item) => item.source === "agent").map((item) => item.id));
      }
    } catch (error) {
      if (disposed || generation !== discoveryGeneration) return;
      const code = error instanceof Error && AUTHENTICATION_ERROR_CODES.has(error.message)
        ? error.message : "agent_authentication_failed";
      connectionModel = {
        ...connectionModel,
        enhancedChannel: {
          ...connectionModel.enhancedChannel,
          status: "degraded",
          authentication: "failed",
          lastErrorCode: code,
        },
      };
    }
    publishModel("agent");
  }

  async function upload(filePath: string, uploadOptions?: AgentFileUploadOptions) {
    if (disposed) throw new Error("agent_channel_unavailable");
    const transferId = uploadOptions?.transferId ?? crypto.randomUUID();
    const generation = discoveryGeneration;
    const agentPort = connectionModel.enhancedChannel.endpoint?.port;
    if (!agentPort || connectionModel.enhancedChannel.status !== "available") {
      throw new Error(connectionModel.enhancedChannel.lastErrorCode ?? "agent_channel_unavailable");
    }
    if (uploadOptions?.relativePath !== undefined
      && !sessionSupportsCapability(connectionModel.capabilities, "file.transfer.relative-path")) {
      throw new Error("agent_file_directory_unsupported");
    }
    const unlisten = await listenUploadProgress((progress) => {
      if (progress.transferId === transferId) uploadOptions?.onProgress?.(progress);
    });
    try {
      // Disposal can overtake listener registration; finally still owns its release.
      if (disposed || generation !== discoveryGeneration) throw new Error("agent_channel_unavailable");
      if (!remoteAgentSessionId) throw new Error("agent_authentication_required");
      const request = { host: options.host, port: agentPort, sessionId: remoteAgentSessionId };
      return await options.invokeCommand("upload_remote_agent_file", {
        request: {
          ...request,
          filePath,
          transferId,
          targetDirectory: uploadOptions?.targetDirectory,
          relativePath: uploadOptions?.relativePath,
          renamePolicy: uploadOptions?.renamePolicy,
        },
      }) as AgentFileUploadResult;
    } finally {
      unlisten();
    }
  }

  return {
    get connectionModel() { return connectionModel; },
    consumeDiscovery,
    invalidateDiscovery,
    invoke,
    upload,
    dispose() {
      disposed = true;
      resetAgentAttempt();
    },
  };
}
