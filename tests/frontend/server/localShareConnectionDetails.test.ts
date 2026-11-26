import assert from "node:assert/strict";
import test from "node:test";
import { localShareConnectionFacts } from "../../../src/server/localShareConnectionDetails.ts";
import type { LocalVncServerOverview } from "../../../src/server/localShare.ts";
import type { WindowsServiceOverview } from "../../../src/server/windowsService.ts";

function overview(): LocalVncServerOverview {
  return {
    status: "running", bindAddress: "0.0.0.0", port: 5903, addresses: [], displays: [],
    selectedDisplayId: "main", selectedDisplayLabel: "Main display", resolutionMode: "fit", refreshMode: "balanced",
    framebufferWidth: 1920, framebufferHeight: 1080, securityType: "vnc_auth", viewOnly: false,
    activeConnections: 0, activeConnectionDevices: [], permissions: [{ kind: "accessibility", status: "granted", requiredFor: "input_injection" }],
    agent: { status: "running", bindAddress: "0.0.0.0", port: 6200, lastError: null, updatedAt: "1",
      capabilities: ["clipboard.text.read", "clipboard.text.write", "message.show", "file.transfer.upload", "agent.display.enumerate", "agent.display.select"] },
    startedAt: "1", updatedAt: "1",
  };
}

function service(overrides: Partial<WindowsServiceOverview> = {}): WindowsServiceOverview {
  return {
    supported: true, installed: true, status: "running", workerSessionId: 1, workerRunning: true,
    bindAddress: "0.0.0.0", port: 5903, viewOnly: false, credentialConfigured: true,
    passwordManagementAuthorized: true, lastErrorCode: null, sasPolicy: "services_allowed",
    desktop: "Default", inputStatus: "ready", ...overrides,
  };
}

function facts(current: LocalVncServerOverview | null, overrides: Partial<Parameters<typeof localShareConnectionFacts>[0]> = {}) {
  return localShareConnectionFacts({ overview: current, runtimeUnavailable: false, permissionUnavailable: false, service: null, ...overrides });
}

function fact(current: LocalVncServerOverview, id: string, overrides: Partial<Parameters<typeof localShareConnectionFacts>[0]> = {}) {
  const found = facts(current, overrides).find(item => item.id === id);
  assert.ok(found);
  return found;
}

test("connection details distinguish keyboard control and reliable clipboard read and write capabilities", () => {
  const current = overview();
  assert.equal(fact(current, "control").valueKey, "localShare.controllable");
  assert.equal(fact(current, "clipboardRead").labelKey, "localShare.clipboardRead");
  assert.equal(fact(current, "clipboardWrite").labelKey, "localShare.clipboardWrite");
  for (const item of facts(current).filter(item => item.id !== "control")) {
    assert.equal(item.valueKey, "localShare.available");
    assert.equal(item.reasonKey, undefined);
  }
});

test("read-only sharing still allows reading clipboard text and selecting a display", () => {
  const current = overview();
  current.viewOnly = true;
  current.agent.capabilities = ["clipboard.text.read", "agent.display.enumerate", "agent.display.select"];
  assert.equal(fact(current, "control").valueKey, "localShare.viewOnly");
  for (const id of ["clipboardRead", "displaySwitch"]) assert.equal(fact(current, id).valueKey, "localShare.available");
  for (const id of ["clipboardWrite", "sendMessage", "fileTransfer"]) {
    assert.equal(fact(current, id).valueKey, "localShare.unavailable");
    assert.equal(fact(current, id).reasonKey, "localShare.connectionCapabilityReasons.viewOnly");
  }
});

test("stopped and transitional instances never advertise stale capabilities as available", () => {
  const current = overview();
  for (const status of ["stopped", "starting", "stopping", "permission_missing", "port_conflict", "failed"] satisfies LocalVncServerOverview["status"][]) {
    current.status = status;
    for (const item of facts(current)) {
      assert.equal(item.valueKey, "localShare.unavailable");
      assert.equal(item.reasonKey, status === "stopped" ? "localShare.connectionCapabilityReasons.sharingStopped" : `localShare.serverStatusDetail.${status}`);
    }
  }
});

test("failed status reads, missing instances and unknown status hide last-confirmed permissions and capabilities", () => {
  const current = overview();
  for (const result of [facts(current, { runtimeUnavailable: true }), facts(null), facts({ ...current, status: "unknown" })]) {
    assert.ok(result.every(item => item.valueKey === "localShare.unconfirmed"));
    assert.ok(result.every(item => item.reasonKey === "localShare.connectionCapabilityReasons.statusUnconfirmed"));
  }
});

test("base RFB control stays independent of a stopped or failed enhancement channel", () => {
  const current = overview();
  for (const status of ["stopped", "failed"] satisfies LocalVncServerOverview["agent"]["status"][]) {
    current.agent.status = status;
    assert.equal(fact(current, "control").valueKey, "localShare.controllable");
    const item = fact(current, "sendMessage");
    assert.equal(item.valueKey, "localShare.unavailable");
    assert.equal(item.reasonKey, `localShare.connectionCapabilityReasons.${status === "failed" ? "agentFailed" : "agentNotReady"}`);
  }
  current.agent.status = "running";
  current.agent.port = null;
  assert.equal(fact(current, "clipboardRead").reasonKey, "localShare.connectionCapabilityReasons.agentNotReady");
});

test("secure Windows desktops preserve RFB control and uploads while user-desktop features are unavailable", () => {
  const current = overview();
  current.agent.capabilities = ["file.transfer.upload", "agent.display.enumerate"];
  const options = { service: service({ desktop: "Winlogon" }) };
  assert.equal(fact(current, "control", options).valueKey, "localShare.controllable");
  assert.equal(fact(current, "fileTransfer", options).valueKey, "localShare.available");
  for (const id of ["clipboardRead", "clipboardWrite", "sendMessage", "displaySwitch"]) {
    assert.equal(fact(current, id, options).reasonKey, "localShare.connectionCapabilityReasons.userDesktopUnavailable");
  }
});

test("blocked, recovering and unconfirmed Windows input cannot be labelled controllable", () => {
  const current = overview();
  for (const [inputStatus, reasonName] of [["blocked", "inputBlocked"], ["waiting_for_frame", "inputWaiting"], ["", "inputUnconfirmed"]]) {
    const options = { service: service({ inputStatus }) };
    const item = fact(current, "control", options);
    assert.equal(item.reasonKey, `localShare.connectionCapabilityReasons.${reasonName}`);
    assert.notEqual(item.valueKey, "localShare.controllable");
    assert.equal(fact(current, "sendMessage", options).valueKey, "localShare.available");
  }
  assert.equal(fact(current, "control", { service: service({ desktopRecovering: true }) }).reasonKey,
    "localShare.connectionCapabilityReasons.desktopRecovering");
});

test("missing or unconfirmed accessibility permissions do not masquerade as control permission", () => {
  const current = overview();
  current.permissions[0].status = "missing";
  assert.equal(fact(current, "control").reasonKey, "localShare.connectionCapabilityReasons.accessibilityMissing");
  current.permissions[0].status = "unknown";
  assert.equal(fact(current, "control").valueKey, "localShare.unconfirmed");
  current.permissions[0].status = "granted";
  assert.equal(fact(current, "control", { permissionUnavailable: true }).valueKey, "localShare.unconfirmed");
  assert.equal(fact(current, "sendMessage", { permissionUnavailable: true }).valueKey, "localShare.available");
});

test("display enumeration alone and unrelated capabilities cannot promise display switching or clipboard writes", () => {
  const current = overview();
  current.agent.capabilities = ["agent.display.enumerate", "clipboard.text.read"];
  assert.equal(fact(current, "displaySwitch").reasonKey, "localShare.connectionCapabilityReasons.capabilityMissing");
  assert.equal(fact(current, "clipboardRead").valueKey, "localShare.available");
  assert.equal(fact(current, "clipboardWrite").reasonKey, "localShare.connectionCapabilityReasons.capabilityMissing");
  assert.equal(fact(current, "sendMessage").reasonKey, "localShare.connectionCapabilityReasons.capabilityMissing");
});
