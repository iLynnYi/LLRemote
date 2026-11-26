import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import test from "node:test";

const sourceRoot = new URL("../../../src/", import.meta.url);
const projectRoot = new URL("../../../", import.meta.url);

function sourceFile(path: string): URL {
  return new URL(path, sourceRoot);
}

function projectFile(path: string): URL {
  return new URL(path, projectRoot);
}

function collectVueFiles(dir: URL): URL[] {
  const entries = readdirSync(dir, { withFileTypes: true });
  const files: URL[] = [];

  for (const entry of entries) {
    const child = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, dir);
    if (entry.isDirectory()) {
      files.push(...collectVueFiles(child));
    } else if (entry.name.endsWith(".vue")) {
      files.push(child);
    }
  }

  return files;
}

test("App.vue delegates feature workbenches to focused components", () => {
  const appVue = readFileSync(sourceFile("App.vue"), "utf8");

  assert.ok(appVue.split("\n").length < 240, "App.vue should stay as the root orchestrator");
  assert.match(appVue, /<AppShell/);
  assert.match(appVue, /<SessionExternalWindowPage/);
  assert.doesNotMatch(appVue, /class="host-workbench"/);
  assert.doesNotMatch(appVue, /class="session-workbench"/);
});

test("feature workbench components exist for the App.vue split", () => {
  const expectedFiles = [
    "app/components/AppMessageBridge.vue",
    "app/components/AppShell.vue",
    "app/components/ConfirmDialog.vue",
    "app/components/ConnectionDialog.vue",
    "features/hosts/components/HostGroupSidebar.vue",
    "app/components/ModuleRail.vue",
    "app/components/PasswordPromptDialog.vue",
    "features/hosts/components/HostWorkbench.vue",
    "features/local-share/components/LocalShareWorkbench.vue",
    "features/sessions/components/SessionDiagnosticsPanel.vue",
    "features/sessions/components/SessionExternalWindowPage.vue",
    "features/sessions/components/SessionTabs.vue",
    "features/sessions/components/SessionToolbar.vue",
    "features/sessions/components/SessionTopbar.vue",
    "features/sessions/components/SessionViewportGrid.vue",
    "features/sessions/components/SessionWorkbench.vue",
    "features/settings/components/SettingsWorkbench.vue",
  ];

  for (const file of expectedFiles) {
    assert.equal(existsSync(sourceFile(file)), true, `${file} should exist`);
  }
});

test("app state delegates domain state to focused composables", () => {
  const appState = readFileSync(sourceFile("app/composables/useVncAppState.ts"), "utf8");
  const expectedFiles = [
    "app/types.ts",
    "app/composables/useAppConfirmDialog.ts",
    "app/composables/useWorkspaceNavigation.ts",
    "features/hosts/composables/useHostWorkbench.ts",
    "features/hosts/composables/useHostGroupPanel.ts",
    "features/local-share/composables/useLocalShareWorkbench.ts",
    "features/sessions/composables/useSessionExternalWindow.ts",
    "features/sessions/composables/useSessionClipboardReports.ts",
    "features/sessions/composables/useSessionDisplayControls.ts",
    "features/sessions/composables/useSessionLocalServerControls.ts",
    "features/settings/composables/useSettingsWorkbench.ts",
  ];

  assert.ok(appState.split("\n").length < 2600, "useVncAppState.ts should keep shrinking as domain state moves out");

  for (const file of expectedFiles) {
    assert.equal(existsSync(sourceFile(file)), true, `${file} should exist`);
  }

  assert.match(appState, /useAppConfirmDialog/);
  assert.match(appState, /useWorkspaceNavigation/);
  assert.match(appState, /useHostWorkbench/);
  assert.match(appState, /useLocalShareWorkbench/);
  assert.match(appState, /useSessionClipboardReports/);
  assert.match(appState, /useSessionDisplayControls/);
  assert.match(appState, /useSessionExternalWindow/);
  assert.match(appState, /useSessionLocalServerControls/);
  assert.match(appState, /useSettingsWorkbench/);
});

test("app styles are split by workspace responsibility", () => {
  const stylesEntry = readFileSync(sourceFile("app/styles.css"), "utf8");
  const expectedFiles = [
    "app/styles/base.css",
    "app/styles/shell.css",
    "app/styles/workbench-shared.css",
    "app/styles/local-share.css",
    "app/styles/settings.css",
    "app/styles/session-layout.css",
    "app/styles/host-workbench.css",
    "app/styles/host-group-panel.css",
    "app/styles/host-form-controls.css",
    "app/styles/dialogs.css",
    "app/styles/session-workbench.css",
    "app/styles/session-tabs.css",
    "app/styles/session-diagnostics.css",
    "app/styles/session-viewport.css",
  ];

  assert.ok(stylesEntry.split("\n").length < 32, "app/styles.css should stay as the style entrypoint");

  for (const file of expectedFiles) {
    assert.equal(existsSync(sourceFile(file)), true, `${file} should exist`);
    if (!file.match(/session-(tabs|diagnostics|viewport)\.css$/)) {
      assert.match(stylesEntry, new RegExp(file.replace("app/", "(?:\\./)?")));
    }
  }

  const sessionWorkbenchStyles = readFileSync(sourceFile("app/styles/session-workbench.css"), "utf8");
  assert.match(sessionWorkbenchStyles, /@import "(?:\.\/)?session-tabs\.css";/);
  assert.match(sessionWorkbenchStyles, /@import "(?:\.\/)?session-diagnostics\.css";/);
  assert.match(sessionWorkbenchStyles, /@import "(?:\.\/)?session-viewport\.css";/);
});

test("Naive UI is the app-level component foundation", () => {
  const packageJson = JSON.parse(readFileSync(projectFile("package.json"), "utf8")) as {
    dependencies?: Record<string, string>;
  };
  const appVue = readFileSync(sourceFile("App.vue"), "utf8");

  assert.ok(packageJson.dependencies?.["naive-ui"], "naive-ui should be a runtime dependency");
  assert.ok(packageJson.dependencies?.["@vicons/ionicons5"], "@vicons/ionicons5 should provide Naive UI icons");
  assert.match(appVue, /NConfigProvider/);
  assert.match(appVue, /NMessageProvider/);
  assert.match(appVue, /AppMessageBridge/);
});

test("Tauri icon resources include app and tray status sources", () => {
  const expectedFiles = [
    "src-tauri/icons/icon-source.svg",
    "src-tauri/icons/icon.png",
    "src-tauri/icons/icon.icns",
    "src-tauri/icons/icon.ico",
    "src-tauri/icons/32x32.png",
    "src-tauri/icons/128x128.png",
    "src-tauri/icons/128x128@2x.png",
    "src-tauri/icons/tray/idle.png",
    "src-tauri/icons/tray/sharing.png",
    "src-tauri/icons/tray/sharing-active.png",
    "src-tauri/icons/tray/error.png",
  ];

  for (const file of expectedFiles) {
    assert.equal(existsSync(projectFile(file)), true, `${file} should exist`);
  }
});

test("Vue form fields use Naive UI primitives", () => {
  for (const file of collectVueFiles(sourceRoot)) {
    const source = readFileSync(file, "utf8");
    const relativePath = file.pathname.slice(sourceRoot.pathname.length);

    assert.doesNotMatch(
      source,
      /<(input|select|textarea)\b/i,
      `${relativePath} should use Naive UI components for form fields`,
    );
  }
});
