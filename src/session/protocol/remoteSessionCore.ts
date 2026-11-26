export type RemoteSessionSurfaceOwner = "embedded" | "external" | "recording";

export type RemoteSessionDisplaySource = "vnc";
export type RemoteSessionInputSource = "vnc";

export type RemoteSessionSurfaceSnapshot = {
  surfaceId: string;
  owner: RemoteSessionSurfaceOwner;
  windowLabel: string;
  active: boolean;
  surfaceUrl: string;
};

export type RemoteSessionSnapshot = {
  sessionId: string;
  target: string;
  state: string;
  connectionModel: SessionConnectionModel;
  displaySource: RemoteSessionDisplaySource;
  inputSource: RemoteSessionInputSource;
  activeSurfaceId?: string | null;
  surfaces: RemoteSessionSurfaceSnapshot[];
};

export type RemoteSessionStartRequest = {
  sessionId: string;
  target: string;
  connectionModel: SessionConnectionModel;
  displaySource: RemoteSessionDisplaySource;
  inputSource: RemoteSessionInputSource;
};

export type RemoteSessionUpdateRequest = {
  sessionId: string;
  state?: string;
  connectionModel?: SessionConnectionModel;
  displaySource?: RemoteSessionDisplaySource;
  inputSource?: RemoteSessionInputSource;
};

export type RemoteSessionRfbConnectRequest = {
  sessionId: string;
  host: string;
  port: number;
  password?: string;
};

export type RemoteSessionRfbInput =
  | { type: "key"; down: boolean; keysym: number }
  | { type: "pointer"; x: number; y: number; buttonMask: number }
  | { type: "clipboard"; text: string }
  | { type: "refresh"; incremental: boolean }
  | { type: "setQuality"; qualityLevel: number; compressionLevel: number };

export type SessionSurfaceAttachRequest = {
  sessionId: string;
  surfaceId: string;
  owner: RemoteSessionSurfaceOwner;
  windowLabel: string;
  active?: boolean;
};

export type RemoteSessionCoreInvoke = (
  command:
    | "start_remote_session"
    | "update_remote_session"
    | "connect_remote_session_rfb"
    | "disconnect_remote_session_rfb"
    | "send_remote_session_rfb_input"
    | "stop_remote_session"
    | "attach_session_surface"
    | "detach_session_surface"
    | "set_active_session_surface"
    | "get_remote_session_snapshot"
    | "list_remote_session_snapshots",
  args?: Record<string, unknown>,
) => Promise<unknown>;

export type RemoteSessionCoreClient = {
  startSession: (request: RemoteSessionStartRequest) => Promise<RemoteSessionSnapshot>;
  updateSession: (request: RemoteSessionUpdateRequest) => Promise<RemoteSessionSnapshot>;
  connectRfb: (request: RemoteSessionRfbConnectRequest) => Promise<void>;
  disconnectRfb: (sessionId: string) => Promise<void>;
  sendRfbInput: (sessionId: string, input: RemoteSessionRfbInput) => Promise<void>;
  stopSession: (sessionId: string) => Promise<void>;
  attachSurface: (request: SessionSurfaceAttachRequest) => Promise<RemoteSessionSurfaceSnapshot>;
  detachSurface: (sessionId: string, surfaceId: string) => Promise<RemoteSessionSnapshot>;
  setActiveSurface: (sessionId: string, surfaceId: string) => Promise<RemoteSessionSnapshot>;
  getSnapshot: (sessionId: string) => Promise<RemoteSessionSnapshot | null>;
  listSnapshots: () => Promise<RemoteSessionSnapshot[]>;
};

export function createRemoteSessionCoreClient(options: {
  invoke: RemoteSessionCoreInvoke;
}): RemoteSessionCoreClient {
  return {
    startSession: async (request) =>
      await options.invoke("start_remote_session", { request }) as RemoteSessionSnapshot,
    updateSession: async (request) =>
      await options.invoke("update_remote_session", { request }) as RemoteSessionSnapshot,
    connectRfb: async (request) => {
      await options.invoke("connect_remote_session_rfb", { request });
    },
    disconnectRfb: async (sessionId) => {
      await options.invoke("disconnect_remote_session_rfb", { sessionId });
    },
    sendRfbInput: async (sessionId, input) => {
      await options.invoke("send_remote_session_rfb_input", { request: { sessionId, input } });
    },
    stopSession: async (sessionId) => {
      await options.invoke("stop_remote_session", { sessionId });
    },
    attachSurface: async (request) =>
      await options.invoke("attach_session_surface", { request }) as RemoteSessionSurfaceSnapshot,
    detachSurface: async (sessionId, surfaceId) =>
      await options.invoke("detach_session_surface", { sessionId, surfaceId }) as RemoteSessionSnapshot,
    setActiveSurface: async (sessionId, surfaceId) =>
      await options.invoke("set_active_session_surface", { sessionId, surfaceId }) as RemoteSessionSnapshot,
    getSnapshot: async (sessionId) =>
      await options.invoke("get_remote_session_snapshot", { sessionId }) as RemoteSessionSnapshot | null,
    listSnapshots: async () =>
      await options.invoke("list_remote_session_snapshots") as RemoteSessionSnapshot[],
  };
}
import type { SessionConnectionModel } from "../connection-model/connectionModel.ts";
