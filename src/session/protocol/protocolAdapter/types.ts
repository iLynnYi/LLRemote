import type { EventLogLevel } from "@diagnostics/eventLog";
import type { SessionClipboardActivity } from "../../clipboard/sessionClipboardActivity.ts";
import type { ViewportRuntimeEvidence } from "../../diagnostics/runtimeEvidence.ts";
import type { SessionExternalInputPayload } from "../../external-window/sessionExternalInput.ts";
import type { RemoteKeyDispatch } from "../../input/keyboardMapping.ts";
import type { ClipboardSyncMode } from "../../settings/sessionSettings.ts";
import type { SessionDisplayInfo, SessionQualityProfile } from "../../viewport/sessionDisplay.ts";
import type { SessionViewportScaleMode } from "../../viewport/sessionViewportScale.ts";
import type { SessionConnectionModel } from "../../connection-model/connectionModel.ts";
import type { ConnectionIssue } from "../../diagnostics/sessionDiagnostics.ts";
import type { CoreSurfaceCommit, CoreSurfaceSession } from "../../renderer/coreSurfaceSession.ts";
import type { CoreSurfaceCursor } from "../../renderer/coreSurfaceCursor.ts";
import type { NativeRfbFailure } from "../rfbFailure.ts";
import type { ServerRuntimeNotice, ServerRuntimeState, SecureAttentionOutcome } from "./serverRuntime.ts";

export type ProtocolTransportHandle = {
  id: string;
  surfaceUrl: string;
};

export type ProtocolConnection = {
  protocol: "vnc";
  connectionModel: SessionConnectionModel;
  baseTransport: SessionConnectionModel["baseTransport"];
  displaySource: "vnc";
  inputSource: "vnc";
  transport: ProtocolTransportHandle;
  disconnect: () => void | Promise<void>;
  focus: () => void;
  refreshDisplay: () => void;
  setInputEnabled: (enabled: boolean) => void;
  setReadonlyMode: (enabled: boolean) => void;
  setClipboardSyncMode: (mode: ClipboardSyncMode) => void;
  forwardExternalInput: (payload: SessionExternalInputPayload) => void;
  sendRemoteKeyEvent: (payload: RemoteKeyDispatch) => void;
  releaseRemoteInput?: () => void;
  sendRemoteKeySequence: (payloads: RemoteKeyDispatch[]) => void | Promise<void>;
  getServerRuntimeState?: () => ServerRuntimeState | null;
  notifySurfacePresented?: (recoveryId: number) => void;
  sendSecureAttention?: () => Promise<SecureAttentionOutcome>;
  controlRemoteDesktop?: (actionId: AgentRemoteDesktopControlActionId) => Promise<void>;
  sendRemoteClipboardText: (text: string) => Promise<"written" | "sent-unconfirmed" | "skipped">;
  sendRemoteMessage: (text: string) => Promise<void>;
  hasAgentChannel?: () => boolean;
  uploadRemoteFile: (filePath: string, options?: AgentFileUploadOptions) => Promise<AgentFileUploadResult>;
  controlRemoteFileTransfer: (transferId: string, action: AgentFileTransferControlAction) => Promise<void>;
  revealRemoteFile: (remotePath: string) => Promise<void>;
  openRemoteFile: (remotePath: string) => Promise<void>;
  diagnoseRemotePermissions: () => Promise<AgentPermissionDiagnosticResult>;
  enumerateRemoteDisplays: () => Promise<AgentDisplayInventoryResult>;
  selectRemoteDisplay?: (displayId: string) => Promise<void>;
  recoverSecureInput: (reason: AgentSecureInputRecoveryReason) => Promise<AgentSecureInputRecoveryResult>;
  wakeRemoteDisplay?: () => Promise<"submitted" | "sent-unconfirmed">;
  notifyRemoteSessionEnd?: () => Promise<void>;
  getDisplayInfo: () => SessionDisplayInfo | null;
  getRuntimeEvidence: () => ViewportRuntimeEvidence | null;
  setQualityProfile: (profile: SessionQualityProfile) => void;
  setViewportScaleMode?: (mode: SessionViewportScaleMode) => void;
};

export type ProtocolConnectOptions = {
  target: HTMLElement;
  sessionId?: string;
  surfaceUrl?: string;
  host: string;
  port: number;
  password?: string;
  signal?: AbortSignal;
  connectionModel: SessionConnectionModel;
  readonly?: boolean;
  clipboardSyncMode?: ClipboardSyncMode;
  onCredentialsRequired?: () => Promise<string | undefined>;
  onStatus: (status: string) => void;
  onError: (message: string) => void;
  onDisplayInfo?: (info: SessionDisplayInfo) => void;
  onRuntimeEvidence?: (evidence: ViewportRuntimeEvidence) => void;
  onServerRuntimeChanged?: () => void;
  onClipboardActivity?: (activity: SessionClipboardActivity) => void;
  onRemoteClipboardText?: (text: string, channel: "agent" | "rfb") => boolean | Promise<boolean>;
  onConnectionIssue?: (issue: ConnectionIssue) => void;
  onConnectionModelChanged?: (model: SessionConnectionModel) => void;
  onViewOnlySharedChanged?: (viewOnly: boolean) => void;
  onEvent: (source: string, message: string, level?: EventLogLevel) => void;
};

export type ProtocolAdapter = {
  protocol: "vnc";
  connect: (options: ProtocolConnectOptions) => Promise<ProtocolConnection>;
  disconnect: (connection: ProtocolConnection) => Promise<void>;
};

export type InvokeCommand = (
  command: "upload_remote_agent_file" | "invoke_remote_agent" | "log_connection_model" | "connect_remote_session_rfb" | "disconnect_remote_session_rfb" | "send_remote_session_rfb_input",
  args: Record<string, unknown>,
) => Promise<unknown>;

export type AgentFileUploadOptions = {
  transferId: string;
  targetDirectory?: string;
  relativePath?: string;
  renamePolicy?: "autoRename" | "overwrite" | "skip" | "ask";
  signal?: AbortSignal;
  onProgress?: (progress: AgentFileUploadProgress) => void;
};

export type AgentFileUploadProgress = {
  transferId: string;
  fileName: string;
  transferredBytes: number;
  totalBytes: number;
  phase?: "preparing" | "transferring" | "verifying";
};

export type AgentFileTransferControlAction = "pause" | "cancel";

export type AgentFileUploadResult = {
  fileName: string;
  remotePath: string;
  bytesWritten: number;
  sha256: string;
};

export type AgentPermissionDiagnosticStatus = "granted" | "missing" | "unsupported" | "unknown";

export type AgentPermissionDiagnosticItem = {
  kind: "agent_service" | "clipboard" | "file_system" | "screen_recording" | "accessibility" | "audio_capture";
  status: AgentPermissionDiagnosticStatus;
  requiredFor: string;
  message: string;
};

export type AgentPermissionDiagnosticResult = {
  target: string;
  checkedAt: string;
  items: AgentPermissionDiagnosticItem[];
};

export type AgentDisplayInventoryItem = {
  id: string;
  label: string;
  isPrimary: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  pixelWidth: number;
  pixelHeight: number;
  scaleFactor: number;
};

export type AgentDisplayInventoryResult = {
  target: string;
  checkedAt: string;
  source: "agent";
  displays: AgentDisplayInventoryItem[];
};

export type AgentRemoteDesktopControlActionId =
  | "previous-desktop"
  | "next-desktop"
  | "overview"
  | "show-current-desktop";

export type AgentSecureInputRecoveryReason = "uac_or_secure_desktop" | "task_manager_or_system_surface" | "input_injection_lost";

export type AgentSecureInputRecoveryStatus = "recovered" | "unsupported" | "failed";

export type AgentSecureInputRecoveryResult = {
  target: string;
  checkedAt: string;
  status: AgentSecureInputRecoveryStatus;
  action: "service_sequence" | "agent_restart_input" | "none";
  message: string;
};

export type CreateVncProtocolAdapterOptions = {
  invoke: InvokeCommand;
  listenRfbEvent?: (
    eventName: string,
    handler: (event: RemoteSessionRfbEvent) => void,
  ) => Promise<() => void>;
  createSurfaceSession?: (options: {
    target: HTMLElement;
    surfaceUrl: string;
    onFrameCommitted?: (commit: CoreSurfaceCommit) => void;
    onError?: (code: string) => void;
  }) => CoreSurfaceSession;
};

export type RemoteSessionRfbEvent =
  | { type: "serverRuntime"; notice: ServerRuntimeNotice }
  | { type: "performance"; metrics: {
      windowMs: number; receivedBytes: number; updates: number; committedFrames: number;
      decodeTotalUs: number; decodeMaxUs: number;
      readCalls: number; readTimeouts: number; readTotalUs: number; consumeTotalUs: number;
      writeTotalUs: number; writeMaxUs: number; eventTotalUs: number;
      encodings: { rectangles: number; copyRect: number; tight: number; jpeg: number };
    } }
  | { type: "bell" }
  | { type: "clipboard"; text: string }
  | { type: "cursor"; cursor: CoreSurfaceCursor }
  | { type: "pointerPosition"; x: number; y: number }
  | { type: "desktopLayout"; screens: Array<{ id: number; x: number; y: number; width: number; height: number; flags: number }> }
  | { type: "discoveryManifest"; manifest: unknown }
  | { type: "discoveryError"; error: unknown }
  | { type: "discoveryFailure"; code: string }
  | { type: "disconnected"; reason: string; failure: NativeRfbFailure };
