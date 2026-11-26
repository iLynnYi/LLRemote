import assert from "node:assert/strict";
import test from "node:test";
import { ref } from "vue";

import { useSessionLocalServerControls } from "../../../src/features/sessions/composables/useSessionLocalServerControls.ts";
import type { SessionConnectionRegistry } from "../../../src/session/core/sessionConnectionRegistry.ts";

function localServerOverview() {
  return {
    status: "running",
    bindAddress: "0.0.0.0",
    port: 5903,
    addresses: [
      { label: "en0", address: "192.168.64.6", kind: "lan_candidate" },
      { label: "Loopback", address: "127.0.0.1", kind: "loopback" },
    ],
    displays: [
      { id: "display:main", label: "Main" },
    ],
    selectedDisplayId: "display:main",
    selectedDisplayLabel: "Main",
    resolutionMode: "native",
    refreshMode: "balanced",
    framebufferWidth: 1428,
    framebufferHeight: 1010,
    securityType: "vnc_auth",
    viewOnly: false,
    activeConnections: 1,
    activeConnectionDevices: [],
    permissions: [],
    agent: {
      status: "running",
      bindAddress: "127.0.0.1",
      port: 6903,
      capabilities: ["file.transfer.upload", "agent.display.enumerate"],
      updatedAt: "2026-07-15T12:20:00.000Z",
    },
    updatedAt: "2026-07-15T12:20:00.000Z",
  };
}

test("self-connection detection records endpoint metadata without granting Agent authentication", async () => {
  const detectedPorts: Array<number | undefined> = [];
  const events: string[] = [];
  const overview = localServerOverview();
  const registry = {
    get: (sessionId: string) => sessionId === "session-1"
      ? {
          sessionId,
          connection: {
            setAgentPort: (port?: number) => detectedPorts.push(port),
          },
        }
      : undefined,
  } as Partial<SessionConnectionRegistry> as SessionConnectionRegistry;
  const controls = useSessionLocalServerControls({
    errorMessage: ref(""),
    readServerStatus: async () => overview,
    updateServerView: async () => overview,
    localVncServerOverview: ref(null),
    logEvent: (_level, _source, message) => events.push(message),
    sessionConnectionRegistry: registry,
    sessionLocalServerViewBySessionId: ref({}),
    setSessionLocalServerView: () => {},
    t: (key: string) => key,
    updateSessionDisplayInfoFromLocalServer: () => {},
  });

  const preparation = await controls.detectLocalVncServerForSession("session-1", "192.168.64.6", 5903);

  assert.deepEqual(preparation, {
    localEndpointMatch: { state: "matched", viewOnly: false, selectedDisplayId: "display:main", framebufferWidth: 1428, framebufferHeight: 1010 },
    selectedDisplayId: "display:main",
    framebufferWidth: 1428,
    framebufferHeight: 1010,
    viewOnlyShared: false,
  });
  assert.deepEqual(detectedPorts, []);
  assert.deepEqual(events, ["sessions.localServerDetected"]);
});

test("a nonmatching local server is recorded once during detection without changing the VNC connection", async () => {
  const events: Array<{ message: string; eventKey?: string }> = [];
  const overview = localServerOverview();
  const controls = useSessionLocalServerControls({
    errorMessage: ref(""),
    readServerStatus: async () => overview,
    updateServerView: async () => overview,
    localVncServerOverview: ref(null),
    logEvent: (_level, _source, message, context) => events.push({ message, eventKey: context?.eventKey }),
    sessionConnectionRegistry: { listLiveSessionIds: () => [] } as unknown as SessionConnectionRegistry,
    sessionLocalServerViewBySessionId: ref({}),
    setSessionLocalServerView: () => {},
    t: (key: string) => key,
    updateSessionDisplayInfoFromLocalServer: () => {},
  });

  assert.equal(await controls.detectLocalVncServerForSession("session-1", "192.168.85.128", 5904), undefined);
  assert.equal(events.length, 1);
  assert.equal(events[0].eventKey, "local_server_unmatched");
  assert.match(events[0].message, /目标 192\.168\.85\.128:5904 未匹配/);
});

test("endpoint metadata refresh updates matching live sessions only", () => {
  const detectedPorts: Array<number | undefined> = [];
  let refreshCount = 0;
  const overview = localServerOverview();
  const registry = {
    listLiveSessionIds: () => ["session-1", "session-2", "session-3"],
    get: (sessionId: string) => ({
      sessionId,
      connection: {
        setAgentPort: (port?: number) => detectedPorts.push(port),
      },
    }),
  } as Partial<SessionConnectionRegistry> as SessionConnectionRegistry;
  const controls = useSessionLocalServerControls({
    errorMessage: ref(""),
    readServerStatus: async () => overview,
    updateServerView: async () => overview,
    localVncServerOverview: ref(null),
    logEvent: () => {},
    sessionConnectionRegistry: registry,
    sessionLocalServerViewBySessionId: ref({}),
    setSessionLocalServerView: () => {},
    t: (key: string) => key,
    updateSessionDisplayInfoFromLocalServer: () => {},
    getSessionTarget: (sessionId) => sessionId === "session-1"
      ? { host: "192.168.64.6", port: 5903 }
      : sessionId === "session-2"
        ? { host: "192.168.64.99", port: 5903 }
        : { host: "192.168.64.99", port: 5904 },
    onAgentChannelChanged: () => {
      refreshCount += 1;
    },
  });

  controls.syncLocalEndpointMatchesForSessions(overview);

  assert.deepEqual(detectedPorts, []);
  assert.equal(refreshCount, 1);
});

test("syncLocalEndpointMatchesForSessions applies view-only shared mode to existing live sessions", () => {
  const readonlyUpdates: Array<{ sessionId: string; viewOnly: boolean }> = [];
  const overview = {
    ...localServerOverview(),
    viewOnly: true,
  };
  const registry = {
    listLiveSessionIds: () => ["session-1", "session-2"],
    get: (sessionId: string) => ({
      sessionId,
      connection: {
        setAgentPort: () => {},
      },
    }),
  } as Partial<SessionConnectionRegistry> as SessionConnectionRegistry;
  const controls = useSessionLocalServerControls({
    errorMessage: ref(""),
    readServerStatus: async () => overview,
    updateServerView: async () => overview,
    localVncServerOverview: ref(null),
    logEvent: () => {},
    sessionConnectionRegistry: registry,
    sessionLocalServerViewBySessionId: ref({}),
    setSessionLocalServerView: () => {},
    t: (key: string) => key,
    updateSessionDisplayInfoFromLocalServer: () => {},
    getSessionTarget: (sessionId) => sessionId === "session-1"
      ? { host: "192.168.64.6", port: 5903 }
      : { host: "192.168.64.99", port: 5903 },
    syncSessionViewOnlyShared: (sessionId, viewOnly) => {
      readonlyUpdates.push({ sessionId, viewOnly });
    },
  });

  controls.syncLocalEndpointMatchesForSessions(overview);

  assert.deepEqual(readonlyUpdates, [{ sessionId: "session-1", viewOnly: true }]);
});

test("updateSessionLocalServerView refreshes the standard VNC display after switching displays", async () => {
  const displaySwitches: string[] = [];
  let refreshCount = 0;
  const currentOverview = localServerOverview();
  const nextOverview = {
    ...currentOverview,
    displays: [
      { id: "display:main", label: "Main" },
      { id: "display:secondary", label: "Secondary" },
    ],
    selectedDisplayId: "display:secondary",
    selectedDisplayLabel: "Secondary",
    framebufferWidth: 1920,
    framebufferHeight: 1080,
  };
  const registry = {
    get: () => ({
      sessionId: "session-1",
      connection: {
        refreshDisplay: () => {
          refreshCount += 1;
        },
      },
    }),
  } as Partial<SessionConnectionRegistry> as SessionConnectionRegistry;
  const controls = useSessionLocalServerControls({
    errorMessage: ref(""),
    readServerStatus: async () => currentOverview,
    updateServerView: async () => nextOverview,
    localVncServerOverview: ref(currentOverview),
    logEvent: () => {},
    sessionConnectionRegistry: registry,
    sessionLocalServerViewBySessionId: ref({ "session-1": currentOverview }),
    setSessionLocalServerView: () => {},
    t: (key: string) => key,
    updateSessionDisplayInfoFromLocalServer: () => {},
  });

  await controls.updateSessionLocalServerView("session-1", { displayId: "display:secondary" });
  await new Promise((resolve) => setTimeout(resolve, 150));

  assert.deepEqual(displaySwitches, []);
  assert.equal(refreshCount, 2);
});
