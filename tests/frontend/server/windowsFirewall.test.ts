import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, ref } from "vue";
import { useWindowsFirewall } from "../../../src/features/local-share/composables/useWindowsFirewall.ts";
import type { WindowsFirewallOverview } from "../../../src/server/windowsFirewall.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

type HostNode = { parent: HostNode | null; children: HostNode[] };
const node = (): HostNode => ({ parent: null, children: [] });
const renderer = createRenderer<HostNode, HostNode>({
  createElement: node, createText: node, createComment: node,
  setText() {}, setElementText() {}, patchProp() {},
  parentNode: value => value.parent, nextSibling: () => null,
  insert(child, parent) { child.parent = parent; parent.children.push(child); },
  remove(child) { if (child.parent) child.parent.children = child.parent.children.filter(item => item !== child); },
});

const overview: WindowsFirewallOverview = { profiles: [
  { name: "domain", connected: true, enabled: false },
  { name: "private", connected: false, enabled: true },
  { name: "public", connected: true, enabled: false },
] };

function fixture() {
  const calls: { command: string; args?: Record<string, unknown> }[] = [];
  const active = ref(true);
  let reply = async (_command: string): Promise<WindowsFirewallOverview | void> => overview;
  let state!: ReturnType<typeof useWindowsFirewall>;
  const app = renderer.createApp({ setup() {
    state = useWindowsFirewall({ active, t: key => key,
      invoke: async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
        calls.push({ command, args }); return await reply(command) as T;
      },
    });
    return () => null;
  } });
  app.mount(node());
  return { state, calls, active, dispose: () => app.unmount(),
    reply(handler: typeof reply) { reply = handler; } };
}

test("inspection displays detected network types and distinguishes disconnected from disabled", async () => {
  const f = fixture();
  try {
    assert.deepEqual(f.state.profiles.value, []);
    await f.state.refresh();
    assert.deepEqual(f.calls, [{ command: "inspect_windows_firewall", args: undefined }]);
    assert.deepEqual(f.state.profiles.value, overview.profiles);
    f.active.value = false;
    await f.state.refresh();
    assert.equal(f.calls.length, 1);
  } finally { f.dispose(); }
});

test("workgroup machines show private and public profiles without inventing a domain profile", async () => {
  const f = fixture();
  const workgroup = overview.profiles.filter(profile => profile.name !== "domain");
  f.reply(async () => ({ profiles: [...workgroup].reverse() }));
  try {
    await f.state.refresh();
    assert.deepEqual(f.state.profiles.value, workgroup);
    assert.equal(f.state.profiles.value[0]!.name, "private");
    assert.equal(f.state.profiles.value[0]!.connected, false, "disconnected private network remains applicable");
  } finally { f.dispose(); }
});

test("a domain member retains its domain profile while disconnected", async () => {
  const f = fixture();
  f.reply(async () => ({ profiles: overview.profiles.map(profile => ({ ...profile, connected: false })) }));
  try {
    await f.state.refresh();
    assert.deepEqual(f.state.profiles.value.map(profile => profile.name), ["domain", "private", "public"]);
    assert.ok(f.state.profiles.value.every(profile => profile.connected === false));
  } finally { f.dispose(); }
});

test("overlapping header refreshes share one system query", async () => {
  const f = fixture();
  const pending = deferred<WindowsFirewallOverview>();
  f.reply(async () => pending.promise);
  try {
    const first = f.state.refresh();
    const second = f.state.refresh();
    assert.equal(f.calls.length, 1);
    pending.resolve(overview);
    await Promise.all([first, second]);
    assert.deepEqual(f.state.profiles.value, overview.profiles);
  } finally { f.dispose(); }
});

test("ordinary window switches and page changes do not trigger firewall inspection", async () => {
  const f = fixture();
  try {
    f.state.markSettingsDeparture();
    await f.state.refreshAfterSettings();
    f.active.value = false;
    f.active.value = true;
    await f.state.refreshAfterSettings();
    assert.deepEqual(f.calls, []);
  } finally { f.dispose(); }
});

test("returning from settings refreshes once after an actual departure", async () => {
  const f = fixture();
  try {
    await f.state.openSettings();
    await f.state.refreshAfterSettings();
    assert.deepEqual(f.calls.map(call => call.command), ["open_windows_firewall_settings"]);
    f.state.markSettingsDeparture();
    await Promise.all([f.state.refreshAfterSettings(), f.state.refreshAfterSettings()]);
    assert.deepEqual(f.calls.map(call => call.command), ["open_windows_firewall_settings", "inspect_windows_firewall"]);
  } finally { f.dispose(); }
});

test("a settings return while another page is active waits until local sharing is visible", async () => {
  const f = fixture();
  try {
    await f.state.openSettings();
    f.state.markSettingsDeparture();
    f.active.value = false;
    await f.state.refreshAfterSettings();
    assert.equal(f.calls.length, 1);
    f.active.value = true;
    await f.state.refreshAfterSettings();
    assert.equal(f.calls.length, 2);
  } finally { f.dispose(); }
});

test("returning from settings waits for an older read and then queries the changed state", async () => {
  const f = fixture();
  const pending = deferred<WindowsFirewallOverview>();
  f.reply(async command => command === "inspect_windows_firewall" ? pending.promise : undefined);
  try {
    const oldRead = f.state.refresh();
    await f.state.openSettings();
    f.state.markSettingsDeparture();
    const returning = f.state.refreshAfterSettings();
    assert.equal(f.calls.filter(call => call.command === "inspect_windows_firewall").length, 1);
    const changed: WindowsFirewallOverview = { profiles: overview.profiles.map(profile => ({ ...profile, enabled: true })) };
    f.reply(async () => changed);
    pending.resolve(overview);
    await Promise.all([oldRead, returning]);
    assert.equal(f.calls.filter(call => call.command === "inspect_windows_firewall").length, 2);
    assert.ok(f.state.profiles.value.every(profile => profile.enabled === true));
  } finally { f.dispose(); }
});

test("failed settings launch does not arm a later window-switch inspection", async () => {
  const f = fixture();
  f.reply(async () => { throw new Error("windows_firewall_settings_failed"); });
  try {
    await f.state.openSettings();
    assert.equal(f.state.settingsError.value, "localShare.firewall.errors.settings_failed");
    f.state.markSettingsDeparture();
    await f.state.refreshAfterSettings();
    assert.deepEqual(f.calls.map(call => call.command), ["open_windows_firewall_settings"]);
  } finally { f.dispose(); }
});

test("partial reads keep detected types and a failed read does not fabricate network types", async () => {
  const f = fixture();
  try {
    f.reply(async () => ({ profiles: [{ name: "public", connected: true, enabled: null },
      { name: "private", connected: null, enabled: false }] }));
    await f.state.refresh();
    assert.deepEqual(f.state.profiles.value, [
      { name: "private", connected: null, enabled: false },
      { name: "public", connected: true, enabled: null },
    ]);
    f.reply(async () => { throw new Error("windows_firewall_inspect_failed"); });
    await f.state.refresh();
    assert.equal(f.state.error.value, "localShare.firewall.errors.inspect_failed");
    assert.deepEqual(f.state.profiles.value, []);
  } finally { f.dispose(); }
});

test("unmounting ignores late inspection responses", async () => {
  const f = fixture();
  const pending = deferred<WindowsFirewallOverview>();
  f.reply(async () => pending.promise);
  const inspecting = f.state.refresh();
  f.dispose();
  pending.resolve(overview);
  await inspecting;
  assert.deepEqual(f.state.profiles.value, []);
});
