import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, nextTick, ref, shallowRef } from "vue";
import { useSessionAdvancedDiagnostics } from "../../../src/features/sessions/composables/useSessionAdvancedDiagnostics.ts";
import { createSessionConnectionRegistry } from "../../../src/session/core/sessionConnectionRegistry.ts";
import { createInitialConnectionModel } from "../../../src/session/connection-model/connectionModel.ts";
import type { RemoteSession } from "../../../src/session/core/sessionManager.ts";
import type { AgentPermissionDiagnosticResult, ProtocolConnection } from "../../../src/session/protocol/protocolAdapter.ts";
import type { WorkspaceMode } from "../../../src/app/types.ts";

const renderer = createRenderer<object, object>({
  insert() {}, remove() {}, patchProp() {}, setText() {}, setElementText() {},
  createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  parentNode: () => null, nextSibling: () => null,
});

function session(id: string): RemoteSession {
  return { id, target: `${id}:5900`, host: id, port: 5900, state: "connected", createdAt: "", stateHistory: [],
    protocol: "vnc", qualityProfile: "balanced" };
}

function pendingResult() {
  let resolve!: (result: AgentPermissionDiagnosticResult) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<AgentPermissionDiagnosticResult>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

test("permission checks discard late results across session switches, replacements and unmount", async (t) => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "document");
  const document = Object.assign(new EventTarget(), { visibilityState: "visible" });
  Object.defineProperty(globalThis, "document", { value: document, configurable: true });
  t.after(() => { if (previous) Object.defineProperty(globalThis, "document", previous); else Reflect.deleteProperty(globalThis, "document"); });
  const registry = createSessionConnectionRegistry();
  const results = [pendingResult(), pendingResult(), pendingResult()];
  const model = createInitialConnectionModel();
  model.capabilities.items = [{ id: "agent.permissions.diagnose", state: "supported", source: "agent" }];
  const connection = (index: number) => ({ connectionModel: model, diagnoseRemotePermissions: () => results[index].promise }) as ProtocolConnection;
  registry.attach("a", connection(0));
  registry.attach("b", connection(1));
  const activeRemoteSession = shallowRef<RemoteSession | null>(session("a"));
  const context = {
    activeRemoteSession, activeWorkspace: ref<WorkspaceMode>("sessions"), fullscreenSessionId: ref<string | null>(null),
    sessionDeveloperDiagnosticsVisible: ref(false), sessionListTick: ref(0), sessionConnectionRegistry: registry,
    logEvent() {}, t: (key: string) => key,
  };
  let diagnostics!: ReturnType<typeof useSessionAdvancedDiagnostics>;
  const app = renderer.createApp({ setup() { diagnostics = useSessionAdvancedDiagnostics(context); return () => null; } });
  app.mount({});
  let mounted = true;
  const unmount = () => { if (mounted) { app.unmount(); mounted = false; } };
  t.after(unmount);
  assert.equal(diagnostics.canCheckPermissions.value, true);
  const first = diagnostics.checkPermissions();
  activeRemoteSession.value = session("b");
  const second = diagnostics.checkPermissions();
  results[0].resolve({ target: "a", checkedAt: "", items: [] });
  await first;
  assert.equal(diagnostics.permissionReport.value, null);
  assert.equal(diagnostics.checkingPermissions.value, true);
  results[1].resolve({ target: "untrusted-target", checkedAt: "", items: [] });
  await second;
  assert.equal(diagnostics.permissionReport.value?.target, "b:5900");
  model.capabilities.items = [];
  context.sessionListTick.value++;
  assert.equal(diagnostics.canCheckPermissions.value, false);
  assert.equal(diagnostics.permissionReport.value, null);
  model.capabilities.items = [{ id: "agent.permissions.diagnose", state: "supported", source: "agent" }];
  registry.attach("b", connection(2));
  context.sessionListTick.value++;
  assert.equal(diagnostics.permissionReport.value, null);
  const third = diagnostics.checkPermissions();
  unmount();
  results[2].reject(new Error("late-result"));
  await third;
  assert.equal(diagnostics.permissionReport.value, null);
  assert.equal(diagnostics.permissionError.value, "");
  assert.equal(context.sessionDeveloperDiagnosticsVisible.value, false);
});

test("sampling visibility follows expansion, workspace, window visibility and unmount", async (t) => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "document");
  const document = Object.assign(new EventTarget(), { visibilityState: "visible" });
  Object.defineProperty(globalThis, "document", { value: document, configurable: true });
  t.after(() => { if (previous) Object.defineProperty(globalThis, "document", previous); else Reflect.deleteProperty(globalThis, "document"); });
  const context = {
    activeRemoteSession: shallowRef<RemoteSession | null>(null), activeWorkspace: ref<WorkspaceMode>("sessions"), fullscreenSessionId: ref<string | null>(null),
    sessionDeveloperDiagnosticsVisible: ref(false), sessionListTick: ref(0), sessionConnectionRegistry: createSessionConnectionRegistry(),
    logEvent() {}, t: (key: string) => key,
  };
  let diagnostics!: ReturnType<typeof useSessionAdvancedDiagnostics>;
  const app = renderer.createApp({ setup() { diagnostics = useSessionAdvancedDiagnostics(context); return () => null; } });
  app.mount({});
  diagnostics.developerDiagnosticsExpanded.value = true;
  await nextTick();
  assert.equal(context.sessionDeveloperDiagnosticsVisible.value, true);
  document.visibilityState = "hidden";
  document.dispatchEvent(new Event("visibilitychange"));
  await nextTick();
  assert.equal(context.sessionDeveloperDiagnosticsVisible.value, false);
  document.visibilityState = "visible";
  document.dispatchEvent(new Event("visibilitychange"));
  context.activeWorkspace.value = "hosts";
  await nextTick();
  assert.equal(context.sessionDeveloperDiagnosticsVisible.value, false);
  context.activeWorkspace.value = "sessions";
  await nextTick();
  assert.equal(context.sessionDeveloperDiagnosticsVisible.value, true);
  app.unmount();
  assert.equal(context.sessionDeveloperDiagnosticsVisible.value, false);
});
