import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  canCopyLocalVncServerConnectionProfile,
  createLocalVncServerConnectionExport,
  getLocalVncServerCopyProfileTitleKey,
  getLocalVncServerStartErrorCopy,
  getLocalVncServerToggleLabelKey,
  isLocalVncServerPasswordInputInvalid,
  isValidIpv4Address,
  localVncServerPermissionKindKey,
  localVncServerPermissionStatusKey,
  summarizeLocalAgentStatus,
  summarizeLocalVncServerStatus,
  validateLocalVncServerStartInput,
  type LocalVncServerOverview,
} from "../../../src/server/localShare.ts";

const sourceRoot = new URL("../../../src/", import.meta.url);

function sourceFile(path: string): string {
  return readFileSync(new URL(path, sourceRoot), "utf8");
}

function tauriSourceFile(path: string): string {
  return readFileSync(new URL(`../../../src-tauri/src/${path}`, import.meta.url), "utf8");
}

function tauriSourceFiles(paths: string[]): string {
  return paths.map((path) => tauriSourceFile(path)).join("\n");
}

function createServerOverview(status: LocalVncServerOverview["status"] = "running"): LocalVncServerOverview {
  return {
    status,
    bindAddress: "0.0.0.0",
    port: 5903,
    addresses: [
      {
        label: "en0",
        address: "192.168.1.10",
        kind: "lan_candidate",
      },
    ],
    displays: [],
    selectedDisplayId: "main",
    selectedDisplayLabel: "Main display",
    resolutionMode: "fit",
    refreshMode: "balanced",
    framebufferWidth: 1280,
    framebufferHeight: 800,
    securityType: "vnc_auth",
    viewOnly: false,
    activeConnections: 0,
    activeConnectionDevices: [],
    permissions: [],
    agent: {
      status: status === "running" ? "running" : "stopped",
      bindAddress: "0.0.0.0",
      port: status === "running" ? 6903 : null,
      capabilities: status === "running" ? ["clipboard.text.write", "file.transfer.upload"] : [],
      lastError: null,
      updatedAt: "1782813601",
    },
    startedAt: status === "running" ? "1782813600" : null,
    updatedAt: "1782813601",
  };
}

test("summarizeLocalVncServerStatus maps embedded server states", () => {
  assert.equal(summarizeLocalVncServerStatus("running").tone, "success");
  assert.equal(summarizeLocalVncServerStatus("port_conflict").tone, "error");
  assert.equal(summarizeLocalVncServerStatus("stopped").tone, "neutral");
});

test("summarizeLocalAgentStatus maps enhanced service states", () => {
  assert.equal(summarizeLocalAgentStatus("running").tone, "success");
  assert.equal(summarizeLocalAgentStatus("failed").tone, "error");
  assert.equal(summarizeLocalAgentStatus("stopped").tone, "neutral");
});

test("local VNC server action helpers follow running state", () => {
  const running = createServerOverview("running");
  const stopped = createServerOverview("stopped");

  assert.equal(getLocalVncServerToggleLabelKey(running), "localShare.stopServer");
  assert.equal(getLocalVncServerToggleLabelKey(stopped), "localShare.startServer");
  assert.equal(canCopyLocalVncServerConnectionProfile(running), true);
  assert.equal(canCopyLocalVncServerConnectionProfile(stopped), false);
  assert.equal(getLocalVncServerCopyProfileTitleKey(running), "localShare.copyServerProfile");
  assert.equal(getLocalVncServerCopyProfileTitleKey(stopped), "localShare.copyServerProfileDisabled");
});

test("local VNC server password invalid state only appears after an empty start attempt", () => {
  assert.equal(isLocalVncServerPasswordInputInvalid("", false, createServerOverview("stopped")), false);
  assert.equal(isLocalVncServerPasswordInputInvalid("", true, createServerOverview("stopped")), true);
  assert.equal(isLocalVncServerPasswordInputInvalid("secret", true, createServerOverview("stopped")), false);
  assert.equal(isLocalVncServerPasswordInputInvalid("", true, createServerOverview("running")), false);
});

test("isValidIpv4Address accepts only dotted IPv4 addresses", () => {
  assert.equal(isValidIpv4Address("0.0.0.0"), true);
  assert.equal(isValidIpv4Address("127.0.0.1"), true);
  assert.equal(isValidIpv4Address("192.168.1.10"), true);
  assert.equal(isValidIpv4Address("localhost"), false);
  assert.equal(isValidIpv4Address("999.1.1.1"), false);
  assert.equal(isValidIpv4Address("192.168.1"), false);
});

test("validateLocalVncServerStartInput reports field errors before starting the server", () => {
  assert.deepEqual(
    validateLocalVncServerStartInput({
      port: "abc",
      bindAddress: "localhost",
      password: "",
    }),
    {
      valid: false,
      errors: [
        { field: "port", messageKey: "localShare.serverPortMustBeNumeric" },
        { field: "bindAddress", messageKey: "localShare.serverBindAddressInvalid" },
        { field: "password", messageKey: "localShare.accessPasswordRequired" },
      ],
    },
  );
});

test("validateLocalVncServerStartInput rejects passwords longer than eight UTF-8 bytes", () => {
  assert.deepEqual(
    validateLocalVncServerStartInput({
      port: "5903",
      bindAddress: "0.0.0.0",
      password: "123456789",
    }),
    {
      valid: false,
      errors: [
        { field: "password", messageKey: "localShare.accessPasswordTooLong" },
      ],
    },
  );
});

test("password validation keeps whitespace and measures UTF-8 bytes", () => {
  for (const password of [" ", " 界 ", "界界ab"]) {
    const result = validateLocalVncServerStartInput({ port: 5903, bindAddress: "0.0.0.0", password });
    assert.equal(result.valid, true);
    if (result.valid) assert.equal(result.value.password, password);
  }
  const result = validateLocalVncServerStartInput({ port: 5903, bindAddress: "0.0.0.0", password: "界界界" });
  assert.deepEqual(result, { valid: false, errors: [{ field: "password", messageKey: "localShare.accessPasswordTooLong" }] });
  assert.equal(isLocalVncServerPasswordInputInvalid(" ", true, createServerOverview("stopped")), false);
});

test("validateLocalVncServerStartInput normalizes valid start input", () => {
  assert.deepEqual(
    validateLocalVncServerStartInput({
      port: "5903",
      bindAddress: " 127.0.0.1 ",
      password: "secret",
    }),
    {
      valid: true,
      value: {
        port: 5903,
        bindAddress: "127.0.0.1",
        password: "secret",
      },
    },
  );
});

test("createLocalVncServerConnectionExport excludes temporary password", () => {
  const exported = createLocalVncServerConnectionExport(
    {
      status: "running",
      bindAddress: "0.0.0.0",
      port: 5903,
      addresses: [
        {
          label: "en0",
          address: "192.168.1.10",
          kind: "lan_candidate",
        },
      ],
      displays: [
        {
          id: "main",
          label: "Main display",
          kind: "main",
          isPrimary: true,
          x: 0,
          y: 0,
          width: 1280,
          height: 800,
          pixelWidth: 1280,
          pixelHeight: 800,
          scaleFactor: 1,
        },
      ],
      selectedDisplayId: "main",
      selectedDisplayLabel: "Main display",
      resolutionMode: "fit",
      refreshMode: "balanced",
      framebufferWidth: 1280,
      framebufferHeight: 800,
      securityType: "vnc_auth",
      viewOnly: false,
      activeConnections: 0,
      activeConnectionDevices: [],
      permissions: [],
      agent: {
        status: "running",
        bindAddress: "0.0.0.0",
        port: 6903,
        capabilities: ["clipboard.text.write"],
        lastError: null,
        updatedAt: "1782813601",
      },
      startedAt: "1782813600",
      updatedAt: "1782813601",
    },
    "192.168.1.10",
    "Local Mac",
  );

  assert.deepEqual(JSON.parse(exported), {
    name: "Local Mac",
    protocol: "vnc",
    host: "192.168.1.10",
    port: 5903,
    agentPort: 6903,
    security: "vnc_auth",
    viewOnly: false,
  });
  assert.equal(exported.includes("password"), false);
  assert.equal(exported.includes("credential"), false);
});

test("getLocalVncServerStartErrorCopy maps backend start errors to i18n keys", () => {
  assert.deepEqual(getLocalVncServerStartErrorCopy("password_required: temporary VNC password is required"), {
    messageKey: "localShare.accessPasswordRequired",
  });
  assert.deepEqual(getLocalVncServerStartErrorCopy("port_conflict: address already in use"), {
    messageKey: "localShare.serverPortConflict",
  });
  assert.deepEqual(getLocalVncServerStartErrorCopy("already_running: local VNC Server is running"), {
    messageKey: "localShare.serverAlreadyRunning",
  });
  assert.deepEqual(getLocalVncServerStartErrorCopy("permission_missing: screen_recording permission is required for screen capture"), {
    messageKey: "localShare.serverPermissionMissing",
  });
  assert.equal(getLocalVncServerStartErrorCopy("unexpected"), null);
});

test("local VNC server permission helpers return i18n keys", () => {
  assert.equal(
    localVncServerPermissionKindKey("screen_recording"),
    "localShare.permissions.kind.screen_recording",
  );
  assert.equal(localVncServerPermissionStatusKey("missing"), "localShare.permissions.status.missing");
});

test("local share server form persists non-sensitive settings and keeps controls in one row", () => {
  const workbench = sourceFile("features/local-share/components/LocalShareWorkbench.vue");
  const composable = sourceFile("features/local-share/composables/useLocalShareWorkbench.ts");
  assert.match(composable, /loadLocalShareSettings/);
  assert.match(composable, /saveLocalShareSettings/);
  assert.match(workbench, /type="password"/);
  assert.match(workbench, /show-password-on="click"/);
  assert.match(workbench, /:disabled="formLocked"/);
  assert.match(workbench, /v-model:value="formPassword"/);
});
test("local Agent no longer registers enhanced screen streams in the local VNC connection registry", () => {
  const agentService = tauriSourceFiles([
    "local_agent_service/mod.rs",
    "local_agent_service/protocol.rs",
    "local_agent_service/display/handlers.rs",
  ]);
  const localServer = tauriSourceFiles([
    "local_vnc_server/mod.rs",
    "local_vnc_server/manager.rs",
  ]);

  assert.doesNotMatch(agentService, /LocalVncServerConnectionKind::AgentEnhanced/);
  assert.doesNotMatch(agentService, /stream\.shutdown\(Shutdown::Both\)/);
  assert.doesNotMatch(agentService, /struct AgentActiveConnectionGuard/);
  assert.doesNotMatch(agentService, /LocalVncServerConnectionRegistry|AgentActiveConnectionCallback/);
  assert.match(localServer, /start_local_agent\(/);
  assert.match(localServer, /LocalVncServerConnectionRegistry::new\(\s*active_connections_changed/);
});

test("local VNC server overview carries active connection device records", () => {
  const overview = createServerOverview("running");
  overview.activeConnections = 1;
  overview.activeConnectionDevices = [
    {
      id: "agent-1",
      kind: "agent_enhanced",
      deviceName: null,
      platform: null,
      ipAddress: "192.168.1.23",
      connectedAt: "1782813600",
      status: "online",
    },
  ];

  assert.equal(overview.activeConnectionDevices.length, overview.activeConnections);
  assert.equal(overview.activeConnectionDevices[0]?.deviceName, null);
  assert.equal(overview.activeConnectionDevices[0]?.platform, null);
});

test("local share workbench renders connection devices above platform permissions", () => {
  const workbench = sourceFile("features/local-share/components/LocalShareWorkbench.vue");
  const composable = sourceFile("features/local-share/composables/useLocalShareWorkbench.ts");
  const appState = sourceFile("app/composables/useVncAppState.ts");
  const zhCN = sourceFile("i18n/messages/zh-CN.ts");

  assert.match(workbench, /localShare\.connectionDevices\.title/);
  assert.match(workbench, /localVncServerOverview\.activeConnectionDevices/);
  assert.ok(
    workbench.indexOf("localShare.connectionDevices.title") < workbench.indexOf("localShare.permissions.title"),
  );
  assert.match(composable, /disconnectLocalVncServerConnection/);
  assert.match(composable, /backgroundApi\.disconnect/);
  assert.doesNotMatch(appState, /localAgentEnhancedActiveConnections/);
  assert.match(zhCN, /connectionDevices: \{/);
  assert.match(zhCN, /empty: "暂无活动连接"/);
});

test("local VNC backend keeps authoritative connection registry and disconnect command", () => {
  const types = tauriSourceFile("local_vnc_server/types.rs");
  const localServer = tauriSourceFiles([
    "local_vnc_server/mod.rs",
    "local_vnc_server/connections.rs",
    "local_vnc_server/manager.rs",
  ]);
  const protocol = tauriSourceFiles([
    "local_vnc_server_protocol/mod.rs",
    "local_vnc_server_protocol/listener.rs",
    "local_vnc_server_protocol/client_loop.rs",
  ]);
  const agentService = tauriSourceFiles([
    "local_agent_service/mod.rs",
    "local_agent_service/protocol.rs",
    "local_agent_service/display/handlers.rs",
  ]);
  const commands = tauriSourceFile("commands/background_share.rs");
  const lib = tauriSourceFile("lib.rs");

  assert.match(types, /struct LocalVncServerActiveConnection/);
  assert.match(types, /active_connection_devices: Vec<LocalVncServerActiveConnection>/);
  assert.match(localServer, /struct LocalVncServerConnectionRegistry/);
  assert.match(localServer, /pub fn disconnect_connection/);
  assert.match(protocol, /connections\.register\(/);
  assert.doesNotMatch(agentService, /connections\.register\(/);
  assert.match(commands, /fn disconnect_background_share_connection/);
  assert.match(lib, /commands::background_share::disconnect_background_share_connection/);
});

test("local VNC backend groups transport sockets from the same device", () => {
  const localServer = tauriSourceFile("local_vnc_server/connections.rs");

  assert.match(localServer, /fn device_group_key/);
  assert.match(localServer, /fn merge_connection_devices/);
  assert.match(localServer, /disconnect_group/);
  assert.doesNotMatch(localServer, /accepts_agent_input_from_peer/);
  assert.match(localServer, /connections\.remove\(&id\)/);
});

test("local Agent service no longer accepts remote desktop input events", () => {
  const agentService = tauriSourceFiles([
    "local_agent_service/mod.rs",
    "local_agent_service/protocol.rs",
    "local_agent_service/display/handlers.rs",
  ]);

  assert.doesNotMatch(agentService, /AGENT_INPUT_EVENT/);
  assert.doesNotMatch(agentService, /send_agent_input_event/);
  assert.doesNotMatch(agentService, /accepts_agent_input_from_peer/);
  assert.doesNotMatch(agentService, /is_loopback_agent_host/);
});

test("local share connection duration refreshes from a live timer", () => {
  const composable = sourceFile("features/local-share/composables/useLocalShareWorkbench.ts");

  assert.match(composable, /localVncServerConnectionDurationTick/);
  assert.match(composable, /window\.setInterval/);
  assert.match(composable, /window\.clearInterval/);
  assert.match(composable, /localVncServerConnectionDurationTick\.value;/);
});
