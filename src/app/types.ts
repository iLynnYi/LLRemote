import type { HostCredentialDraft, HostDraft } from "@hosts/hostManager";
import type { PostConnectPresentation } from "@session/core/connectionConfirm";
import type { RemoteSessionConfig } from "@session/core/sessionManager";

export type LocalVncServerDiagnosticView = {
  code: string;
  severity: "info" | "warning" | "error";
  messageKey: string;
  actionKey: string;
  values?: Record<string, string | number>;
};

export type CoreTransportHandle = {
  id: string;
  surfaceUrl: string;
};

export type ConfirmDialogOptions = {
  title: string;
  message: string;
  primaryLabel: string;
  secondaryLabel: string;
  dismissChoice?: ConfirmChoice;
  danger?: boolean;
};

export type ConfirmChoice = "primary" | "secondary" | "cancel";

export type AppMessageTone = "success" | "warning" | "error" | "info";

export type AppMessageEvent = {
  id: number;
  tone: AppMessageTone;
  content: string;
};

export type PasswordPromptState = {
  visible: boolean;
  target: string;
  password: string;
};

export type HostGroupRecord = {
  name: string;
  hostCount: number;
  createdAt: string;
  updatedAt: string;
};

export type EditableHostDraft = HostDraft & {
  credential: HostCredentialDraft;
};

export type WorkspaceMode = "hosts" | "sessions" | "localShare" | "settings";

export type ConnectVncOptions = {
  /** Uses an explicit credential or an RFB prompt, never the shared form password. */
  endpoint?: Pick<RemoteSessionConfig, "host" | "port">;
  /** Captured only when connecting from a saved host. */
  savedHostName?: string;
  postConnectPresentation?: PostConnectPresentation;
  readonly?: boolean;
  retrySessionId?: string;
};

export type Translate = (key: string, values?: Record<string, string | number>) => string;
