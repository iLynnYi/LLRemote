import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { createSSRApp, h, ref } from "vue";
import { renderToString } from "vue/server-renderer";
import { createLogger, createServer } from "vite";
import { NMessageProvider } from "naive-ui";
import type { WindowsServiceOverview, WindowsServiceRepairItem, WindowsServiceRepairReport } from "../../../src/server/windowsService.ts";
import { windowsServiceErrorKey, sharingRepairErrorKey } from "../../../src/server/windowsService.ts";

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith("@app/") || specifier.startsWith("@server/")) {
    return next(new URL(`../../../src/${specifier.slice(1)}.ts`, import.meta.url).href, context);
  }
  return next(specifier, context);
} });
const { useWindowsService } = await import("../../../src/features/local-share/composables/useWindowsService.ts");

async function fixture() {
  const calls: string[] = [];
  const overview: WindowsServiceOverview = {
    supported: true, installed: true, status: "running", workerSessionId: 8,
    workerRunning: true, bindAddress: "0.0.0.0", port: 5904, viewOnly: false,
    credentialConfigured: true, passwordManagementAuthorized: false, lastErrorCode: null,
    sasPolicy: "services_denied", userAgentRunning: false,
    userAgentErrorCode: "windows_service_user_agent_identity_failed", userAgentRepairSupported: true,
  };
  let report: WindowsServiceRepairReport = { overview: { ...overview }, items: [] };
  let error: string | null = null;
  let statusReply: WindowsServiceOverview | null = null;
  let state!: ReturnType<typeof useWindowsService>;
  await renderToString(createSSRApp({ setup() {
    state = useWindowsService({ t: key => key, normalServer: ref(null),
      invoke: async <T>(command: string): Promise<T> => {
        calls.push(command);
        if (command === "get_windows_service_status" && statusReply) return statusReply as T;
        if (error) throw new Error(error);
        return report as T;
      },
    });
    state.acceptOverview(overview);
    return () => null;
  } }));
  return { state, calls, overview,
    reply(value: Partial<WindowsServiceOverview>, items: WindowsServiceRepairItem[]) { report = { overview: { ...overview, ...value }, items }; },
    fail(value: string) { error = value; },
    status(value: Partial<WindowsServiceOverview>) { statusReply = { ...overview, ...value }; } };
}

const repaired: WindowsServiceRepairItem[] = [
  { kind: "sas", status: "repaired", errorCode: null },
  { kind: "agent", status: "repaired", errorCode: null },
];

test("one repair command accepts a verified report and never dispatches separate repair prompts", async () => {
  const f = await fixture();
  f.reply({ sasPolicy: "services_allowed", userAgentRunning: true, userAgentErrorCode: null }, repaired);
  assert.equal(await f.state.act("repair"), true);
  assert.deepEqual(f.calls, ["repair_windows_service"]);
  assert.deepEqual(f.state.repairItems.value, repaired);
  assert.equal(f.state.message.value, "localShare.windowsService.repairResult.repaired");
  assert.equal(f.state.messageTone.value, "success");
  assert.equal(f.state.repairing.value, false);
});

test("partial repair keeps successful results and exposes remaining failures instead of full success", async () => {
  const f = await fixture();
  f.reply({ sasPolicy: "services_allowed" }, [repaired[0]!,
    { kind: "agent", status: "failed", errorCode: "windows_service_user_agent_identity_failed:restricted_token:5" }]);
  assert.equal(await f.state.act("repair"), false);
  assert.equal(f.state.overview.value?.sasPolicy, "services_allowed");
  assert.equal(f.state.messageTone.value, "warning");
  assert.equal(f.state.message.value, "localShare.windowsService.repairResult.incomplete");
  assert.equal(f.state.repairItems.value[1]?.status, "failed");
});

test("a stopped service remains stopped and deferred checks use informative feedback", async () => {
  const f = await fixture();
  f.reply({ status: "stopped", sasPolicy: "services_allowed" }, [repaired[0]!,
    { kind: "agent", status: "deferred", errorCode: null }]);
  assert.equal(await f.state.act("repair"), true);
  assert.equal(f.state.overview.value?.status, "stopped");
  assert.deepEqual(f.calls, ["repair_windows_service"]);
  assert.equal(f.state.messageTone.value, "info");
  assert.equal(f.state.message.value, "localShare.windowsService.repairResult.deferred");
});

test("cancelling the single elevation prompt preserves runtime state and releases the action gate", async () => {
  const f = await fixture();
  const items: WindowsServiceRepairItem[] = [
    { kind: "service", status: "healthy", errorCode: null },
    { kind: "permissions", status: "needsAdministrator", errorCode: null },
    { kind: "sas", status: "needsAdministrator", errorCode: null },
    { kind: "elevation", status: "needsAdministrator", errorCode: "windows_service_uac_cancelled" },
  ];
  f.reply({}, items);
  assert.equal(await f.state.act("repair"), false);
  assert.equal(f.state.overview.value?.sasPolicy, "services_denied");
  assert.deepEqual(f.state.repairItems.value, items);
  assert.deepEqual(f.calls, ["repair_windows_service"]);
  assert.equal(f.state.busy.value, false);
  assert.equal(f.state.repairing.value, false);
  assert.equal(f.state.message.value, "localShare.windowsService.repairResult.needsAdministrator");
  assert.equal(f.state.messageTone.value, "warning");
});

test("restoring UAC reports the Windows restart separately from deferred sharing checks", async () => {
  const f = await fixture();
  const items: WindowsServiceRepairItem[] = [
    { kind: "elevation", status: "restartRequired", errorCode: null },
    { kind: "agent", status: "deferred", errorCode: null },
  ];
  f.reply({ status: "stopped" }, items);
  assert.equal(await f.state.act("repair"), true);
  assert.equal(f.state.overview.value?.status, "stopped");
  assert.deepEqual(f.state.repairItems.value, items);
  assert.equal(f.state.message.value, "localShare.windowsService.repairResult.restartRequired");
});

test("installation exposes elevation and installation stages without changing the accepted state", async () => {
  for (const code of ["uac_cancelled", "elevation_denied", "elevation_failed",
    "install_copy_failed", "install_register_failed", "install_authorization_failed"]) {
    const f = await fixture();
    f.fail(`windows_service_${code}`);
    assert.equal(await f.state.act("install"), false);
    assert.deepEqual(f.calls, code === "install_authorization_failed"
      ? ["install_windows_service", "get_windows_service_status"] : ["install_windows_service"]);
    assert.equal(f.state.message.value, `localShare.windowsService.errors.${code}`);
    assert.equal(f.state.messageTone.value, code === "uac_cancelled" ? "info" : "error");
    assert.equal(f.state.busy.value, false);
    assert.equal(f.state.overview.value?.status, "running");
  }
});

test("an installation that registered the service but failed delegation refreshes the partial result", async () => {
  const f = await fixture();
  f.state.acceptOverview({ ...f.overview, installed: false, status: "not_installed" });
  f.fail("windows_service_install_authorization_failed");
  f.status({ installed: true, status: "stopped", workerRunning: false });
  assert.equal(await f.state.act("install"), false);
  assert.deepEqual(f.calls, ["install_windows_service", "get_windows_service_status"]);
  assert.equal(f.state.overview.value?.installed, true);
  assert.equal(f.state.overview.value?.status, "stopped");
  assert.equal(f.state.message.value, "localShare.windowsService.errors.install_authorization_failed");
  assert.equal(f.state.messageTone.value, "error");
  assert.equal(f.state.busy.value, false);
});

test("native token failure stages remain structured errors", () => {
  assert.equal(windowsServiceErrorKey("windows_service_user_agent_identity_failed:linked_token:1312"),
    "localShare.windowsService.errors.user_agent_identity_failed");
  assert.equal(sharingRepairErrorKey("background_share_user_context_required:set_integrity:5"),
    "localShare.background.errors.user_context_required");
});

test("ordinary repair remains callable without a Windows service and retains its failure report", async () => {
  const f = await fixture();
  f.state.acceptOverview({ ...f.overview, installed: false, status: "not_installed" });
  const errorCode = "background_share_user_context_required:set_integrity:5";
  f.reply({ installed: false, status: "not_installed" }, [
    { kind: "background", status: "failed", errorCode },
    { kind: "agent", status: "deferred", errorCode: null },
  ]);
  assert.equal(await f.state.act("repair"), false);
  assert.deepEqual(f.calls, ["repair_windows_service"]);
  assert.equal(f.state.overview.value?.installed, false);
  assert.equal(f.state.repairItems.value[0]?.errorCode, errorCode);
  assert.equal(f.state.message.value, "localShare.windowsService.repairResult.incomplete");
});

test("ordinary repair command failures use the background-sharing message category", async () => {
  const f = await fixture();
  f.state.acceptOverview({ ...f.overview, installed: false });
  f.fail("background_share_user_context_required:restricted_primary_token:5");
  assert.equal(await f.state.act("repair"), false);
  assert.equal(f.state.message.value, "localShare.background.errors.user_context_required");
});

test("settings present one repair action even when stopped and keep technical detail collapsed", async () => {
  const f = await fixture();
  f.overview.status = "stopped";
  f.overview.userAgentRepairSupported = false;
  const logger = createLogger("error");
  const reportError = logger.error.bind(logger);
  logger.error = (message, options) => {
    if (message.startsWith("WebSocket server error:") && message.includes("listen EPERM")) return;
    reportError(message, options);
  };
  const vite = await createServer({ customLogger: logger,
    optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false } });
  try {
    const { default: Settings } = await vite.ssrLoadModule("/src/features/settings/components/WindowsServiceSettings.vue");
    const html = await renderToString(createSSRApp({ render: () => h(NMessageProvider, null, {
      default: () => h(Settings, { overview: f.overview, busy: false, running: false, backgroundRunning: false,
        active: true, canStart: true, repairItems: repaired, t: (key: string) => key }),
    }) }));
    assert.ok(html.includes("localShare.windowsService.repairTitle"));
    assert.ok(!html.includes("localShare.windowsService.start</"));
    assert.ok(!html.includes("localShare.windowsService.stop</"));
    const repairButtons = html.match(/<button\b[^>]*>[\s\S]*?<\/button>/g)?.filter(button => /localShare\.windowsService\.repair</.test(button)) ?? [];
    assert.equal(repairButtons.length, 1);
    assert.ok(!/^<button\b[^>]*\sdisabled(?:[\s=>])/.test(repairButtons[0]!));
    assert.ok(!html.includes("localShare.windowsService.repairSas"));
    assert.ok(!html.includes("localShare.windowsService.repairAgent"));
    assert.ok(!html.includes("localShare.windowsService.authorizePasswordManagement"));
    assert.ok(!html.includes("windows_service_user_agent_identity_failed"));
    f.overview.installed = false;
    f.overview.status = "not_installed";
    const withoutService = await renderToString(createSSRApp({ render: () => h(NMessageProvider, null, {
      default: () => h(Settings, { overview: f.overview, busy: false, running: false, backgroundRunning: false,
        active: true, canStart: false, t: (key: string) => key }),
    }) }));
    const ordinaryRepair = withoutService.match(/<button\b[^>]*>[\s\S]*?<\/button>/g)?.filter(button => /localShare\.windowsService\.repair</.test(button)) ?? [];
    assert.equal(ordinaryRepair.length, 1, "uninstalling the Windows service must not remove the repair entry");
    assert.ok(withoutService.includes("localShare.windowsService.goToInstall"));
  } finally { await vite.close(); }
});
