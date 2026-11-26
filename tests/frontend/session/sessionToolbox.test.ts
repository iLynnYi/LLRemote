import assert from "node:assert/strict";
import test from "node:test";

import {
  createSessionToolboxActionRequest,
  listSessionToolboxGroups,
  resolveVisibleSessionToolboxButton,
  resolveSessionToolboxActions,
  resolveSessionToolboxPresentation,
  snapSessionToolboxButton,
  type SessionToolboxContext,
} from "../../../src/session/transfers/sessionToolbox.ts";

const connectedContext: SessionToolboxContext = {
  sessionId: "session-1",
  state: "connected",
  readonly: false,
  capabilities: { items: [
    "file.transfer.upload",
    "message.show",
    "agent.permissions.diagnose",
    "agent.display.enumerate",
    "agent.secure_input.recover",
  ].map((id) => ({ id, state: "supported", source: "agent" })) },
  active: true,
  fullscreen: false,
  externalWindow: false,
  localServer: false,
  recording: false,
};

test("groups toolbox actions by user-facing remote-control function", () => {
  assert.deepEqual(listSessionToolboxGroups().map((group) => group.id), [
    "viewWindow",
    "inputControl",
    "fileTransfer",
    "more",
  ]);
});

test("resolves connected-session actions without unsafe system actions", () => {
  const actions = resolveSessionToolboxActions(connectedContext);
  const actionIds: string[] = actions.map((action) => action.id);

  assert.ok(actionIds.includes("toggle-fullscreen"));
  assert.ok(actionIds.includes("exit-fullscreen"));
  assert.ok(!actionIds.includes("toggle-agent-screen-stream"));
  assert.ok(actionIds.includes("wake-display"));
  assert.ok(actionIds.includes("shortcut-windows-secure-attention"));
  assert.ok(actionIds.includes("shortcut-macos-force-quit"));
  assert.ok(actionIds.includes("send-message"));
  assert.ok(actionIds.includes("send-file"));
  assert.ok(actionIds.includes("capture-screenshot"));
  assert.ok(!actionIds.includes("check-agent-permissions"));
  assert.ok(!actionIds.includes("enumerate-agent-displays"));
  assert.ok(!actionIds.includes("recover-secure-input"));
  assert.ok(!actionIds.includes("report-agent-capabilities"));
  assert.ok(!actionIds.includes("lock-screen"));
  assert.equal(actions.find((action) => action.id === "wake-display")?.status, "enabled");
  assert.equal(actions.find((action) => action.id === "shortcut-windows-secure-attention")?.status, "enabled");
});

test("local shortcut hints follow the client OS on both platforms", (context) => {
  for (const [platform, modifier] of [["MacIntel", "Control+Option"], ["Win32", "Ctrl+Alt"]]) {
    context.mock.getter(globalThis, "navigator", () => ({ platform, userAgent: "" }));
    const actions = resolveSessionToolboxActions(connectedContext);
    for (const [id, key] of [
      ["toggle-fullscreen", "Enter"], ["toggle-external-window", "W"],
      ["send-file", "U"], ["open-file-transfer-center", "T"],
      ["send-message", "M"],
    ]) {
      assert.equal(actions.find((action) => action.id === id)?.shortcutLabel, `${modifier}+${key}`);
    }
    context.mock.restoreAll();
  }
});

test("keeps menu actions available without retired local shortcut hints", () => {
  const actions = resolveSessionToolboxActions(connectedContext);
  for (const id of ["toggle-hud", "toggle-readonly"] as const) {
    const action = actions.find((action) => action.id === id);
    assert.ok(action, `${id} must remain available from the toolbox`);
    assert.equal(action.status, "enabled");
    assert.equal(action.shortcutLabel, undefined);
  }
});

test("keeps Agent diagnostics and unsupported recovery out of the toolbox", () => {
  const actionIds: string[] = resolveSessionToolboxActions(connectedContext).map((action) => action.id);

  assert.ok(!actionIds.includes("check-agent-permissions"));
  assert.ok(!actionIds.includes("enumerate-agent-displays"));
  assert.ok(!actionIds.includes("recover-secure-input"));
  assert.ok(!actionIds.includes("report-agent-capabilities"));
  assert.ok(actionIds.includes("copy-diagnostic-summary"));
});

test("exposes display wake with its acknowledgement feedback", () => {
  const action = resolveSessionToolboxActions(connectedContext)
    .find((item) => item.id === "wake-display");

  assert.equal(action?.kind, "wakeDisplay");
  assert.equal(action?.successKey, "sessions.toolbox.feedback.wake-display");
});

test("sending a message has no success toast while retaining its action", () => {
  const action = resolveSessionToolboxActions(connectedContext)
    .find((item) => item.id === "send-message");
  assert.equal(action?.kind, "remoteMessage");
  assert.equal(action?.successKey, undefined);
});

test("keeps OS-specific shortcut labels explicit without remote OS inference", () => {
  const actions = resolveSessionToolboxActions(connectedContext);
  assert.equal(actions.find((action) => action.id === "shortcut-macos-force-quit")?.shortcutLabel, "Command+Option+Esc");
  assert.equal(actions.find((action) => action.id === "shortcut-macos-window-switch")?.shortcutLabel, "Command+Tab");
  assert.equal(actions.find((action) => action.id === "shortcut-windows-task-manager")?.shortcutLabel, "Ctrl+Shift+Esc");
});

test("creates system-captured shortcut key sequences", () => {
  const actions = resolveSessionToolboxActions(connectedContext);

  assert.deepEqual(actions.find((action) => action.id === "shortcut-windows-secure-attention")?.dispatches?.map((dispatch) => [dispatch.code, dispatch.down]), [
    ["ControlLeft", true],
    ["AltLeft", true],
    ["Delete", true],
    ["Delete", false],
    ["AltLeft", false],
    ["ControlLeft", false],
  ]);

  assert.deepEqual(actions.find((action) => action.id === "shortcut-windows-window-switch")?.dispatches?.map((dispatch) => [dispatch.code, dispatch.down]), [
    ["AltLeft", true],
    ["Tab", true],
    ["Tab", false],
    ["AltLeft", false],
  ]);

  assert.deepEqual(actions.find((action) => action.id === "shortcut-windows-run")?.dispatches?.map((dispatch) => [dispatch.code, dispatch.down]), [
    ["MetaLeft", true],
    ["KeyR", true],
    ["KeyR", false],
    ["MetaLeft", false],
  ]);
  assert.deepEqual(actions.find((action) => action.id === "shortcut-windows-run")?.dispatches?.map((dispatch) => dispatch.keysym), [
    0xffeb,
    0x72,
    0x72,
    0xffeb,
  ]);

  assert.deepEqual(actions.find((action) => action.id === "shortcut-macos-window-switch")?.dispatches?.map((dispatch) => [dispatch.code, dispatch.keysym, dispatch.down]), [
    ["MetaLeft", 0xffeb, true],
    ["Tab", 0xff09, true],
    ["Tab", 0xff09, false],
    ["MetaLeft", 0xffeb, false],
  ]);

  assert.deepEqual(actions.find((action) => action.id === "shortcut-macos-next-desktop")?.dispatches?.map((dispatch) => [dispatch.code, dispatch.down]), [
    ["ControlLeft", true],
    ["ArrowRight", true],
    ["ArrowRight", false],
    ["ControlLeft", false],
  ]);
});

test("keeps the toolbox shortcut menu focused on system-captured actions", () => {
  const actionIds: string[] = resolveSessionToolboxActions(connectedContext).map((action) => action.id);

  assert.ok(actionIds.includes("shortcut-windows-secure-attention"));
  assert.ok(actionIds.includes("shortcut-windows-window-switch"));
  assert.ok(actionIds.includes("shortcut-windows-run"));
  assert.ok(actionIds.includes("shortcut-windows-task-manager"));
  assert.ok(actionIds.includes("shortcut-macos-force-quit"));
  assert.ok(actionIds.includes("shortcut-macos-window-switch"));
  assert.ok(actionIds.includes("shortcut-macos-next-desktop"));
  assert.ok(actionIds.includes("shortcut-macos-previous-desktop"));
  assert.ok(actionIds.includes("shortcut-macos-mission-control"));
  assert.ok(!actionIds.includes("shortcut-secure-attention"));
  assert.ok(!actionIds.includes("shortcut-task-manager"));
  assert.ok(!actionIds.includes("shortcut-window-switch"));
  assert.ok(!actionIds.includes("shortcut-system-menu"));
  assert.ok(!actionIds.includes("shortcut-previous-desktop"));
  assert.ok(!actionIds.includes("shortcut-next-desktop"));
  assert.ok(!actionIds.includes("shortcut-overview"));
  assert.ok(!actionIds.includes("shortcut-show-current-desktop"));
  assert.ok(!actionIds.includes("shortcut-refresh"));
  assert.ok(!actionIds.includes("shortcut-copy"));
  assert.ok(!actionIds.includes("shortcut-paste"));
  assert.ok(!actionIds.includes("shortcut-select-all"));
  assert.ok(!actionIds.includes("shortcut-terminal-interrupt"));
  assert.ok(!actionIds.includes("shortcut-escape"));
  assert.ok(!actionIds.includes("shortcut-enter"));
  assert.ok(!actionIds.includes("shortcut-tab"));
  assert.ok(!actionIds.includes("shortcut-arrow-up"));
  assert.ok(!actionIds.includes("shortcut-arrow-down"));
  assert.ok(!actionIds.includes("shortcut-f1"));
  assert.ok(!actionIds.includes("shortcut-f12"));
});

test("marks keyboard actions unavailable when session is read-only or disconnected", () => {
  const readonlyActions = resolveSessionToolboxActions({
    ...connectedContext,
    readonly: true,
  });
  const disconnectedActions = resolveSessionToolboxActions({
    ...connectedContext,
    state: "disconnected",
  });

  assert.equal(readonlyActions.find((action) => action.id === "shortcut-windows-secure-attention")?.status, "disabled");
  assert.equal(readonlyActions.find((action) => action.id === "shortcut-windows-secure-attention")?.disabledReasonKey, "sessions.toolbox.disabled.readonly");
  assert.equal(readonlyActions.find((action) => action.id === "wake-display")?.status, "disabled");
  assert.equal(readonlyActions.find((action) => action.id === "wake-display")?.disabledReasonKey, "sessions.toolbox.disabled.readonly");
  assert.equal(disconnectedActions.find((action) => action.id === "wake-display")?.status, "disabled");
  assert.equal(disconnectedActions.find((action) => action.id === "wake-display")?.disabledReasonKey, "sessions.toolbox.disabled.disconnected");
});

test("keeps view-only shared sessions from restoring control", () => {
  const actions = resolveSessionToolboxActions({
    ...connectedContext,
    readonly: true,
    viewOnlyShared: true,
  });

  assert.equal(actions.find((action) => action.id === "toggle-readonly")?.status, "disabled");
  assert.equal(actions.find((action) => action.id === "toggle-readonly")?.disabledReasonKey, "sessions.toolbox.disabled.viewOnlyShared");
});

test("marks enhanced actions unavailable when the required Agent capability is missing", () => {
  const actions = resolveSessionToolboxActions({
    ...connectedContext,
    capabilities: { items: [] },
  });

  for (const actionId of ["send-file"] as const) {
    const action = actions.find((item) => item.id === actionId);
    assert.equal(action?.status, "disabled");
    assert.equal(action?.disabledReasonKey, "sessions.toolbox.disabled.enhancedCapability");
  }

  assert.equal(actions.find((action) => String(action.id) === "report-agent-capabilities"), undefined);
});

test("requires an authenticated message capability instead of falling back to the clipboard", () => {
  const actions = resolveSessionToolboxActions({ ...connectedContext, capabilities: { items: [] } });
  assert.equal(actions.find((action) => action.id === "send-message")?.status, "disabled");
  assert.equal(actions.find((action) => action.id === "send-message")?.disabledReasonKey, "sessions.toolbox.disabled.messageUnsupported");
  assert.ok(!actions.some((action) => String(action.id) === "send-clipboard-text"));
});

test("keeps wake display enabled without Agent wake capability for VNC input fallback", () => {
  const actions = resolveSessionToolboxActions({
    ...connectedContext,
    capabilities: { items: [] },
  });

  assert.equal(actions.find((action) => action.id === "wake-display")?.status, "enabled");
});

test("allows retry from an idle viewport that has not reached the remote yet", () => {
  const actions = resolveSessionToolboxActions({
    ...connectedContext,
    state: "idle",
  });

  assert.equal(actions.find((action) => action.id === "retry-session")?.status, "enabled");
  assert.equal(actions.find((action) => action.id === "retry-session")?.shortcutLabel, undefined);
});

test("creates a remote message request and rejects invalid text, readonly or disconnected sends", () => {
  assert.deepEqual(createSessionToolboxActionRequest({
    context: connectedContext, actionId: "send-message", text: "sample text",
  }), { sessionId: "session-1", actionId: "send-message", kind: "remoteMessage", text: "sample text" });

  for (const context of [
    { ...connectedContext, readonly: true },
    { ...connectedContext, state: "disconnected" as const },
    { ...connectedContext, sessionId: null },
  ]) {
    assert.equal(createSessionToolboxActionRequest({ context, actionId: "send-message", text: "sample text" }), null);
  }
  for (const text of ["", " \n\t", "hi\0there", "你".repeat(6000)]) {
    assert.equal(createSessionToolboxActionRequest({ context: connectedContext, actionId: "send-message", text }), null);
  }
  assert.equal(createSessionToolboxActionRequest({ context: { ...connectedContext, capabilities: { items: [] } }, actionId: "send-message", text: "hello" }), null);
});

test("shows recovery actions for failed sessions and hides the connected toolbox during connection attempts", () => {
  for (const state of ["idle", "failed", "authentication failed", "security failure", "disconnected", "disconnected unexpectedly"] as const) {
    assert.deepEqual(resolveSessionToolboxPresentation({ ...connectedContext, state }), {
      connected: false, recoverable: true, fileTransfer: false,
    });
  }
  for (const state of ["connecting", "probing", "closed"] as const) {
    assert.deepEqual(resolveSessionToolboxPresentation({ ...connectedContext, state }), {
      connected: false, recoverable: false, fileTransfer: false,
    });
  }
});

test("shows file actions only for a connected session with upload capability", () => {
  assert.equal(resolveSessionToolboxPresentation(connectedContext).fileTransfer, true);
  assert.equal(resolveSessionToolboxPresentation({ ...connectedContext, capabilities: { items: [] } }).fileTransfer, false);
  assert.equal(resolveSessionToolboxPresentation({ ...connectedContext, capabilities: null, fileTransferAvailable: true }).fileTransfer, true);
  assert.equal(resolveSessionToolboxPresentation({ ...connectedContext, state: "failed", fileTransferAvailable: true }).fileTransfer, false);
});

test("keeps local fullscreen exit and external docking available after connection failure", () => {
  const actions = resolveSessionToolboxActions({ ...connectedContext, state: "failed", fullscreen: true, externalWindow: true });
  assert.equal(actions.find((action) => action.id === "exit-fullscreen")?.status, "enabled");
  assert.equal(actions.find((action) => action.id === "toggle-external-window")?.status, "enabled");
  const embeddedActions = resolveSessionToolboxActions({ ...connectedContext, state: "failed" });
  assert.equal(embeddedActions.find((action) => action.id === "toggle-external-window")?.status, "disabled");
});

test("keeps toolbox button inside the workspace and snaps to nearby corners", () => {
  assert.deepEqual(snapSessionToolboxButton({
    x: 900,
    y: 20,
    workspaceWidth: 960,
    workspaceHeight: 640,
    buttonWidth: 44,
    buttonHeight: 44,
    inset: 16,
    snapDistance: 32,
  }), {
    x: 900,
    y: 16,
    corner: "top-right",
  });

  assert.deepEqual(snapSessionToolboxButton({
    x: -100,
    y: 900,
    workspaceWidth: 960,
    workspaceHeight: 640,
    buttonWidth: 44,
    buttonHeight: 44,
    inset: 16,
    snapDistance: 32,
  }), {
    x: 16,
    y: 580,
    corner: "bottom-left",
  });
});

test("repositions toolbox button when the session viewport shrinks", () => {
  assert.deepEqual(resolveVisibleSessionToolboxButton({
    position: {
      x: 900,
      y: 16,
      corner: "top-right",
    },
    workspaceWidth: 430,
    workspaceHeight: 420,
    buttonWidth: 32,
    buttonHeight: 32,
    inset: 16,
    snapDistance: 32,
  }), {
    x: 382,
    y: 16,
    corner: "top-right",
  });
});
