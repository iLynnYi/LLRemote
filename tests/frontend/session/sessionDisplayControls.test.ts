import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { ref } from "vue";
import { createSessionManager, summarizeSessionTabs } from "../../../src/session/core/sessionManager.ts";
import { createSessionConnectionRegistry } from "../../../src/session/core/sessionConnectionRegistry.ts";
import { createInitialConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";
import { summarizeRemoteViewport } from "../../../src/session/viewport/remoteViewport.ts";
import { summarizeRemoteViewportStatusHud } from "../../../src/session/viewport/remoteViewportStatusBar.ts";
import type { ProtocolConnection } from "../../../src/session/protocol/protocolAdapter.ts";
import { useSessionDisplayControls } from "../../../src/features/sessions/composables/useSessionDisplayControls.ts";
import { resolveSessionToolboxActions } from "../../../src/session/transfers/sessionToolbox.ts";
import type { AgentDisplayInventoryResult } from "../../../src/session/protocol/protocolAdapter.ts";

const eventsModule = `data:text/javascript,${encodeURIComponent("export const events = []; export async function emit(name, payload) { events.push({ name, payload }); }")}`;
registerHooks({ resolve(specifier, context, next) {
  if (specifier === "@tauri-apps/api/event") return next(eventsModule, context);
  if (specifier.startsWith("@session/")) {
    const path = specifier.slice(9);
    return next(new URL(`../../../src/session/${path}${path.endsWith(".ts") ? "" : ".ts"}`, import.meta.url).href, context);
  }
  return next(specifier, context);
} });
const { createExternalWindowStateBridge } = await import("../../../src/app/composables/vncAppState/viewport.ts");
const { events } = await import(eventsModule);

test("readonly changes publish current toolbox and HUD state to the external window in both directions", () => {
  events.length = 0;
  const sessionManager = createSessionManager();
  const session = sessionManager.create({ protocol: "vnc", host: "192.168.85.128", port: 5904, qualityProfile: "balanced" });
  sessionManager.transition(session.id, "connected");
  const readonlyById = ref<Record<string, boolean>>({ [session.id]: false });
  let viewOnlyShared = false;
  const appliedModes: boolean[] = [];
  const sessionConnectionRegistry = createSessionConnectionRegistry();
  sessionConnectionRegistry.attach(session.id, {
    connectionModel: createInitialConnectionModel(),
    setReadonlyMode: (readonly: boolean) => { appliedModes.push(readonly); },
  } as ProtocolConnection);
  const getSummary = (tab: ReturnType<typeof summarizeSessionTabs>[number]) =>
    summarizeRemoteViewport(tab, { ...session, readonly: readonlyById.value[session.id], viewOnlyShared });
  const { publishExternalWindowState } = createExternalWindowStateBridge({
    findSession: id => sessionManager.sessions.find(item => item.id === id),
    getSessionTabs: () => summarizeSessionTabs(sessionManager.sessions, session.id),
    getRemoteViewportSummary: getSummary,
    getRemoteViewportStatusHud: tab => summarizeRemoteViewportStatusHud({
      viewport: getSummary(tab), events: [], nowMs: Date.now(),
    }),
    getSessionLocalServerView: () => null,
    isSessionRecording: () => false,
    sessionConnectionRegistry,
    sessionReadonlyBySessionId: readonlyById,
  });
  const controls = useSessionDisplayControls({
    sessionManager, sessionConnectionRegistry,
    setSessionReadonlyState: (id, readonly) => { readonlyById.value = { ...readonlyById.value, [id]: readonly }; },
    onReadonlyChanged: publishExternalWindowState,
    isSessionViewOnlyShared: () => viewOnlyShared,
    getClipboardSyncMode: () => "off",
    setSessionClipboardPolicy() {}, syncSessionInputOwnership() {}, refreshSessionTabs() {}, logEvent() {},
    t: key => key,
  });
  publishExternalWindowState(session.id);
  for (const readonly of [true, false, true]) {
    const previousCount = events.length;
    controls.setSessionReadonlyMode(session.id, readonly);
    assert.equal(appliedModes.at(-1), readonly, "transport input mode must change");
    assert.equal(events.length, previousCount + 1, "external window must receive the updated state");
    const { name, payload } = events.at(-1);
    assert.equal(name, `session-window-state:${session.id}`);
    assert.equal(payload.controlSnapshot.toolboxContext.readonly, readonly);
    assert.equal(payload.controlSnapshot.statusHud.inputMode, readonly ? "readonly" : "control");
    assert.equal(payload.inputEnabled, !readonly);
  }
  viewOnlyShared = true;
  publishExternalWindowState(session.id);
  const context = events.at(-1).payload.controlSnapshot.toolboxContext;
  for (const externalWindow of [false, true]) {
    const action = resolveSessionToolboxActions({ ...context, externalWindow }).find(action => action.id === "toggle-readonly");
    assert.equal(action?.status, "disabled");
    assert.equal(action?.disabledReasonKey, "sessions.toolbox.disabled.viewOnlyShared");
  }
  const callsBefore = appliedModes.length;
  controls.setSessionReadonlyMode(session.id, false);
  assert.equal(readonlyById.value[session.id], true);
  assert.equal(appliedModes.length, callsBefore, "remote policy must reject a client control override");
});

test("external window snapshots preserve both remote monitor choices and the current selection", () => {
  events.length = 0;
  const sessionManager = createSessionManager();
  const session = sessionManager.create({ protocol: "vnc", host: "192.168.85.128", port: 5903, qualityProfile: "balanced" });
  sessionManager.transition(session.id, "connected");
  const sessionConnectionRegistry = createSessionConnectionRegistry();
  const model = createInitialConnectionModel();
  model.capabilities = { items: ["agent.display.enumerate", "agent.display.select"].map(id => ({ id, state: "supported", source: "agent" })) };
  sessionConnectionRegistry.attach(session.id, { connectionModel: model } as ProtocolConnection);
  let inventory: AgentDisplayInventoryResult = {
    target: session.target, source: "agent", checkedAt: "1",
    displays: ["main", "secondary"].map((id, index) => ({
      id, label: id, isPrimary: index === 0, x: index * 1920, y: 0,
      width: 1920, height: 1080, pixelWidth: 1920, pixelHeight: 1080, scaleFactor: 1,
    })),
  };
  let selectedId = "secondary";
  const getSummary = (tab: ReturnType<typeof summarizeSessionTabs>[number]) => summarizeRemoteViewport(tab, session);
  const bridge = createExternalWindowStateBridge({
    findSession: id => sessionManager.sessions.find(item => item.id === id),
    getSessionTabs: () => summarizeSessionTabs(sessionManager.sessions, session.id),
    getRemoteViewportSummary: getSummary,
    getRemoteViewportStatusHud: tab => summarizeRemoteViewportStatusHud({ viewport: getSummary(tab), events: [], nowMs: 1 }),
    getSessionLocalServerView: () => null,
    getSessionRemoteDisplayInventory: () => inventory,
    getSessionSelectedRemoteDisplayId: () => selectedId,
    isSessionRecording: () => false, sessionConnectionRegistry, sessionReadonlyBySessionId: ref({}),
  });
  bridge.publishExternalWindowState(session.id);
  let context = events.at(-1).payload.controlSnapshot.toolboxContext;
  assert.equal(context.displayChoices?.length, 2, "two discovered monitors must reach the external toolbox");
  assert.deepEqual(context.displayChoices.map(item => [item.value, item.selected]), [["remote:main", false], ["remote:secondary", true]]);
  selectedId = "main";
  bridge.publishExternalWindowState(session.id);
  context = events.at(-1).payload.controlSnapshot.toolboxContext;
  assert.deepEqual(context.displayChoices.map(item => item.selected), [true, false]);
  inventory = { ...inventory, displays: [] };
  bridge.publishExternalWindowState(session.id);
  assert.deepEqual(events.at(-1).payload.controlSnapshot.toolboxContext.displayChoices, [], "empty updates must clear stale choices");
});
