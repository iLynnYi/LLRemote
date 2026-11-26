import type { ClipboardSyncMode } from "../settings/sessionSettings.ts";
import type { LocalClipboardSyncSource } from "./clipboardTypes.ts";

export type SessionClipboardActivityDirection = "client-to-server" | "server-to-client";
export type SessionClipboardActivityChannel = "agent" | "rfb" | "unknown";

export type SessionClipboardActivity = {
  direction: SessionClipboardActivityDirection;
  atMs: number;
  accepted: boolean;
  reason: string;
  mode: ClipboardSyncMode;
  bytes: number;
  channel: SessionClipboardActivityChannel;
  source?: LocalClipboardSyncSource;
};

export type SessionClipboardState = {
  mode: ClipboardSyncMode;
  enabled: boolean;
  readonly: boolean;
  clientToServer: SessionClipboardActivity | null;
  serverToClient: SessionClipboardActivity | null;
};
