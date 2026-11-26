import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  createRemoteSessionCoreClient,
  type RemoteSessionSnapshot,
} from "../../../src/session/protocol/remoteSessionCore.ts";

function appStateSource(): string {
  return readSources([
    "../../../src/app/composables/useVncAppState.ts",
    "../../../src/app/composables/vncAppState/lifecycle.ts",
  ]);
}

function derivedStateSource(): string {
  return readFileSync(new URL("../../../src/features/sessions/composables/useSessionDerivedState.ts", import.meta.url), "utf8");
}

function lifecycleSource(): string {
  return readSources([
    "../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts",
    "../../../src/features/sessions/composables/sessionConnectionLifecycle/coreSync.ts",
    "../../../src/features/sessions/composables/sessionConnectionLifecycle/connectionIssues.ts",
    "../../../src/features/sessions/composables/sessionConnectionLifecycle/sessionDisplayEvents.ts",
  ]);
}

function externalWindowCoordinatorSource(): string {
  return readFileSync(new URL("../../../src/features/sessions/composables/useSessionExternalWindowCoordinator.ts", import.meta.url), "utf8");
}

function readSources(paths: string[]): string {
  return paths
    .map((path) => readFileSync(new URL(path, import.meta.url), "utf8"))
    .join("\n");
}

test("remote session core client reads session snapshots through Tauri command boundary", async () => {
  const calls: string[] = [];
  const snapshot: RemoteSessionSnapshot = {
    sessionId: "session-1",
    target: "10.0.0.1:5900",
    transportKind: "vnc",
    state: "starting",
    baseTransport: { kind: "vnc" },
    enhancedChannel: {
      status: "available",
      port: 6900,
      capabilities: ["file.transfer.upload"],
    },
    displaySource: "vnc",
    inputSource: "vnc",
    activeSurfaceId: "surface:session-1:embedded:main",
    surfaces: [
      {
        surfaceId: "surface:session-1:embedded:main",
        owner: "embedded",
        windowLabel: "main",
        active: true,
      },
    ],
  };
  const client = createRemoteSessionCoreClient({
    invoke: async (command) => {
      calls.push(command);
      return [snapshot];
    },
  });

  assert.deepEqual(await client.listSnapshots(), [snapshot]);
  assert.deepEqual(calls, ["list_remote_session_snapshots"]);
});

test("remote session core client starts and updates base transport sidecar and active source state", async () => {
  const calls: Array<{ command: string; args: unknown }> = [];
  const startSnapshot: RemoteSessionSnapshot = {
    sessionId: "session-1",
    target: "10.0.0.1:5900",
    transportKind: "third-party-vnc",
    state: "starting",
    baseTransport: { kind: "vnc" },
    enhancedChannel: { status: "probing", port: 6900, capabilities: [] },
    displaySource: "vnc",
    inputSource: "vnc",
    activeSurfaceId: null,
    surfaces: [],
  };
  const connectedSnapshot: RemoteSessionSnapshot = {
    ...startSnapshot,
    state: "connected",
    enhancedChannel: {
      status: "available",
      port: 6900,
      capabilities: ["clipboard.text.write", "file.transfer.upload"],
    },
    displaySource: "vnc",
    inputSource: "vnc",
  };
  const client = createRemoteSessionCoreClient({
    invoke: async (command, args) => {
      calls.push({ command, args });
      return command === "update_remote_session" ? connectedSnapshot : startSnapshot;
    },
  });

  assert.deepEqual(await client.startSession({
    sessionId: "session-1",
    target: "10.0.0.1:5900",
    transportKind: "third-party-vnc",
    baseTransport: { kind: "vnc" },
    enhancedChannel: { status: "probing", port: 6900, capabilities: [] },
    displaySource: "vnc",
    inputSource: "vnc",
  }), startSnapshot);
  assert.deepEqual(await client.updateSession({
    sessionId: "session-1",
    state: "connected",
    enhancedChannel: connectedSnapshot.enhancedChannel,
    displaySource: "vnc",
    inputSource: "vnc",
  }), connectedSnapshot);
  assert.deepEqual(calls, [
    {
      command: "start_remote_session",
      args: {
        request: {
          sessionId: "session-1",
          target: "10.0.0.1:5900",
          transportKind: "third-party-vnc",
          baseTransport: { kind: "vnc" },
          enhancedChannel: { status: "probing", port: 6900, capabilities: [] },
          displaySource: "vnc",
          inputSource: "vnc",
        },
      },
    },
    {
      command: "update_remote_session",
      args: {
        request: {
          sessionId: "session-1",
          state: "connected",
          enhancedChannel: connectedSnapshot.enhancedChannel,
          displaySource: "vnc",
          inputSource: "vnc",
        },
      },
    },
  ]);
});

test("remote session core client attaches surfaces without connection secrets", async () => {
  const calls: Array<{ command: string; args: unknown }> = [];
  const client = createRemoteSessionCoreClient({
    invoke: async (command, args) => {
      calls.push({ command, args });
      return {
        surfaceId: "surface:session-1:external:session-session-1",
        owner: "external",
        windowLabel: "session-session-1",
        active: true,
      };
    },
  });

  assert.deepEqual(await client.attachSurface({
    sessionId: "session-1",
    surfaceId: "surface:session-1:external:session-session-1",
    owner: "external",
    windowLabel: "session-session-1",
    active: true,
  }), {
    surfaceId: "surface:session-1:external:session-session-1",
    owner: "external",
    windowLabel: "session-session-1",
    active: true,
  });
  assert.deepEqual(calls, [
    {
      command: "attach_session_surface",
      args: {
        request: {
          sessionId: "session-1",
          surfaceId: "surface:session-1:external:session-session-1",
          owner: "external",
          windowLabel: "session-session-1",
          active: true,
        },
      },
    },
  ]);
});

test("app state exposes a refresh path for remote session core snapshots", () => {
  const source = appStateSource();

  assert.match(source, /createRemoteSessionCoreClient/);
  assert.match(source, /remoteSessionCoreSnapshots = ref<RemoteSessionSnapshot\[\]>\(\[\]\)/);
  assert.match(source, /async function refreshRemoteSessionCoreSnapshots\(\)/);
  assert.match(source, /remoteSessionCoreSnapshots\.value = await remoteSessionCore\.listSnapshots\(\)/);
  assert.match(source, /remoteSessionCoreSnapshots,/);
  assert.match(source, /refreshRemoteSessionCoreSnapshots,/);
});

test("app state passes remote session core snapshots into diagnostics derived state", () => {
  const source = appStateSource();

  assert.match(source, /remoteSessionCoreSnapshots,/);
  assert.match(source, /const activeSessionId = activeRemoteSession\.value\?\.id/);
  assert.match(source, /remoteSessionCoreSnapshots\.value\.find\(\(snapshot\) => snapshot\.sessionId === activeSessionId\)/);
  assert.match(source, /remoteSessionCoreSnapshot: activeRemoteSessionCoreSnapshot/);
  assert.match(derivedStateSource(), /remoteSessionCore: activeRemoteSession\.value/);
});


test("connection lifecycle synchronizes base sidecar and source state into remote session core", () => {
  const source = lifecycleSource();

  assert.match(source, /startRemoteSessionCore\?: \(request: RemoteSessionStartRequest\) => Promise<RemoteSessionSnapshot>;/);
  assert.match(source, /updateRemoteSessionCore\?: \(request: RemoteSessionUpdateRequest\) => Promise<RemoteSessionSnapshot>;/);
  assert.match(source, /stopRemoteSessionCore\?: \(sessionId: string\) => Promise<void>;/);
  assert.match(source, /await attempt\.startCore\(async \(\) => startRemoteSessionCore\?\.\(\{/);
  assert.match(source, /const connectionModel = createInitialConnectionModel\(localPreparation\?\.localEndpointMatch\)/);
  assert.match(source, /startRemoteSessionCore\?\.\(\{[\s\S]*connectionModel,/);
  assert.match(source, /displaySource: "vnc"/);
  assert.match(source, /inputSource: "vnc"/);
  assert.match(source, /Remote Session Core start failed/);
  assert.match(source, /await attempt\.wait\(async \(\) => attachRemoteSessionCoreSurface\?\.\(\{/);
  assert.match(source, /owner: "embedded"/);
  assert.match(source, /windowLabel: "main"/);
  assert.match(source, /Remote Session Core surface attach failed/);
  assert.match(source, /syncAttemptCore\(\{\s*sessionId,\s*state: nextState,[\s\S]*displaySource: protocolConnection\?\.displaySource \?\? "vnc",\s*inputSource: protocolConnection\?\.inputSource \?\? "vnc",/);
  assert.match(source, /attempt\.track<unknown>\(updateRemoteSessionCore\?\.\(request\)/);
  assert.match(source, /onDisconnected: async \(sessionId, connection\) => \{\s*await updateRemoteSessionCore\?\.\(\{\s*sessionId,\s*state: "disconnected",\s*connectionModel: connection\.connectionModel,\s*displaySource: connection\.displaySource,\s*inputSource: connection\.inputSource,/);
  assert.match(source, /stopCore: \(sessionId\) => stopRemoteSessionCore\?\.\(sessionId\)/);
  assert.match(source, /const releasing = userDisconnector\.disconnect\(sessionId, true\);[\s\S]*await releasing;\s*const closed = sessionManager\.close\(sessionId\)/);
});

test("app wires remote session core commands without passing connection secrets", () => {
  const source = appStateSource();

  assert.match(source, /startRemoteSessionCore: \(request\) => remoteSessionCore\.startSession\(request\)/);
  assert.match(source, /updateRemoteSessionCore: \(request\) => remoteSessionCore\.updateSession\(request\)/);
  assert.match(source, /stopRemoteSessionCore: \(sessionId\) => remoteSessionCore\.stopSession\(sessionId\)/);
  assert.doesNotMatch(source, /remoteSessionCore\.startSession\(\{[\s\S]*password/);
  assert.doesNotMatch(source, /remoteSessionCore\.updateSession\(\{[\s\S]*password/);
});

test("external windows update core surfaces instead of creating another connection", () => {
  const source = externalWindowCoordinatorSource();

  assert.match(source, /attachRemoteSessionCoreSurface\?: \(request: SessionSurfaceAttachRequest\) => Promise<RemoteSessionSurfaceSnapshot>;/);
  assert.match(source, /detachRemoteSessionCoreSurface\?: \(sessionId: string, surfaceId: string\) => Promise<RemoteSessionSnapshot>;/);
  assert.match(source, /await attachRemoteSessionCoreSurface!?\(\{/);
  assert.match(source, /if \(!attachRemoteSessionCoreSurface\)/);
  assert.match(source, /onExternalWindowError\?\.\(/);
  assert.match(source, /独立窗口创建失败，请重试。/);
  assert.match(source, /surfaceId: createSessionSurfaceId\(\{/);
  assert.match(source, /owner: "external"/);
  assert.match(source, /windowLabel: request\.label/);
  assert.match(source, /active: true/);
  assert.match(source, /await detachRemoteSessionCoreSurface(?:\?\.)?\(/);
  const appState = appStateSource();
  assert.match(appState, /attachRemoteSessionCoreSurface: \(request\) => remoteSessionCore\.attachSurface\(request\)/);
  assert.match(appState, /detachRemoteSessionCoreSurface: \(sessionId, surfaceId\) => remoteSessionCore\.detachSurface\(sessionId, surfaceId\)/);
  assert.doesNotMatch(source, /connectVnc\(/);
  assert.doesNotMatch(source, /recordSuccessfulConnectionHistory/);
  assert.doesNotMatch(source, /recordFailedConnectionHistory/);
});
