import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, afterEach, test } from "node:test";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { createSessionSettings, type SessionSettingsInput } from "../../../src/session/settings/sessionSettings.ts";
import { SESSION_SETTINGS_STORAGE_KEY } from "../../../src/session/settings/sessionSettingsStore.ts";

const aliases = new Map([
  ["@session/", new URL("../../../src/session/", import.meta.url)],
  ["@app/", new URL("../../../src/app/", import.meta.url)],
]);
const hooks = registerHooks({
  resolve(specifier, context, nextResolve) {
    for (const [prefix, root] of aliases) {
      if (specifier.startsWith(prefix)) {
        return nextResolve(new URL(`${specifier.slice(prefix.length)}.ts`, root).href, context);
      }
    }
    return nextResolve(specifier, context);
  },
});
const { useSettingsWorkbench } = await import("../../../src/features/settings/composables/useSettingsWorkbench.ts");
hooks.deregister();

const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
const previousStorage = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
Object.defineProperty(globalThis, "window", { value: globalThis, configurable: true });
afterEach(() => clearMocks());
after(() => {
  if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
  else Reflect.deleteProperty(globalThis, "window");
  if (previousStorage) Object.defineProperty(globalThis, "localStorage", previousStorage);
  else Reflect.deleteProperty(globalThis, "localStorage");
});

async function fixture(selection: () => string | null | Promise<string | null>, canonicalPath?: string, loadFailure?: string,
  initial: { directories?: Array<{ path: string; addedAt: string; isBuiltin: boolean }>; settings?: SessionSettingsInput; builtinFailure?: boolean } = {}) {
  const entries = new Map<string, string>();
  if (initial.settings) entries.set(SESSION_SETTINGS_STORAGE_KEY, JSON.stringify(createSessionSettings(initial.settings)));
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: (key: string) => entries.get(key) ?? null, setItem: (key: string, value: string) => entries.set(key, value) },
    configurable: true,
  });
  const notices: string[] = [];
  const commands: string[] = [];
  const revealed: string[] = [];
  const granted: Array<{ path: string; addedAt: string; isBuiltin?: boolean }> = (initial.directories ?? []).map(entry => ({ ...entry }));
  mockIPC((command, args) => {
    commands.push(command);
    if (command === "plugin:dialog|open") return selection();
    if (command === "list_file_access_directories") {
      if (loadFailure) return Promise.reject(loadFailure);
      return granted.map(entry => ({ ...entry, status: "ready", exists: true, isDirectory: true, writable: true, message: "" }));
    }
    if (command === "grant_file_access_directory") {
      const request = args?.request;
      assert.ok(request && typeof request === "object" && "directory" in request);
      assert.equal(typeof request.directory, "string");
      const entry = { path: `\\\\?\\${request.directory}`, addedAt: "1" };
      if (canonicalPath !== undefined) entry.path = canonicalPath;
      granted.push(entry);
      return entry;
    }
    if (command === "set_builtin_file_access_directory") {
      if (initial.builtinFailure) return Promise.reject("file_access_directory_not_writable");
      const request = args?.request;
      assert.ok(request && typeof request === "object" && "directory" in request);
      const path = canonicalPath ?? String(request.directory);
      for (const entry of granted) entry.isBuiltin = entry.path === path;
      if (!granted.some(entry => entry.isBuiltin)) granted.push({ path, addedAt: "2", isBuiltin: true });
      granted.sort((left, right) => Number(right.isBuiltin) - Number(left.isBuiltin));
      return granted[0];
    }
    if (command === "revoke_file_access_directory") {
      const request = args?.request;
      assert.ok(request && typeof request === "object" && "directory" in request);
      const index = granted.findIndex(entry => entry.path === request.directory);
      assert.ok(index < 0 || !granted[index].isBuiltin);
      if (index >= 0) granted.splice(index, 1);
      return index >= 0;
    }
    if (command === "inspect_session_artifact_directory") {
      return { directory: args?.directory, status: "ready", exists: true, isDirectory: true, writable: true, message: "" };
    }
    if (command === "reveal_authorized_path") {
      const request = args?.request;
      assert.ok(request && typeof request === "object" && "path" in request && typeof request.path === "string");
      revealed.push(request.path);
      return { status: "opened", path: request.path, directory: request.path, message: "" };
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  const settings = useSettingsWorkbench({ t: key => key, getGlobalKeyboardShortcutRecords: () => [], notify: message => notices.push(message) });
  await new Promise(resolve => setImmediate(resolve));
  return { settings, commands, notices, revealed, entries };
}

test("the first Windows directory selection saves the canonical grant as the artifact option", async () => {
  const f = await fixture(() => "C:\\Users\\tester\\留痕 文件");
  await f.settings.chooseFileAccessDirectory();
  assert.equal(f.settings.artifactDirectoryOptions.value.length, 1);
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, f.settings.artifactDirectoryOptions.value[0].value);
  assert.equal(f.settings.canRevealSessionArtifactDirectory.value, true);
});

test("cancelling the directory picker does not grant access or change the save directory", async () => {
  const f = await fixture(() => null);
  await f.settings.chooseFileAccessDirectory();
  assert.equal(f.commands.includes("grant_file_access_directory"), false);
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, "");
  assert.equal(f.settings.fileAccessDirectories.value.length, 0);
});

test("directory labels hide Windows extended prefixes while selections and file actions keep canonical paths", async () => {
  const paths = [
    { canonical: "\\\\?\\C:\\Users\\tester\\留痕 文件", display: "C:\\Users\\tester\\留痕 文件" },
    { canonical: "\\\\?\\UNC\\server\\shared\\captures", display: "\\\\server\\shared\\captures" },
    { canonical: "\\\\?\\unc\\server\\shared\\captures", display: "\\\\server\\shared\\captures" },
    { canonical: "C:\\Users\\tester\\Downloads", display: "C:\\Users\\tester\\Downloads" },
    { canonical: "/Users/tester/captures", display: "/Users/tester/captures" },
  ];
  for (const { canonical, display } of paths) {
    const f = await fixture(() => display, canonical);
    await f.settings.chooseSessionArtifactDirectory();
    assert.deepEqual(f.settings.artifactDirectoryOptions.value, [{ label: display, value: canonical }]);
    const row = f.settings.fileAccessDirectories.value[0];
    assert.equal(row.path, canonical);
    assert.equal(row.displayPath, display);
    assert.equal(f.settings.sessionSettings.value.artifactDirectory, canonical);
    assert.equal(f.settings.sessionArtifactDirectoryDisplayPath.value, display);
    await f.settings.revealFileAccessDirectory(row.path);
    assert.deepEqual(f.revealed, [canonical]);
  }
});

test("choosing another save directory uses its canonical identity while adding access preserves the current save directory", async () => {
  let selected = "C:/captures/first";
  const f = await fixture(() => selected);
  await f.settings.chooseSessionArtifactDirectory();
  const first = f.settings.sessionSettings.value.artifactDirectory;
  selected = "C:/captures/second";
  await f.settings.chooseFileAccessDirectory();
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, first);
  selected = "C:/captures/third";
  await f.settings.chooseSessionArtifactDirectory();
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, f.settings.artifactDirectoryOptions.value[2].value);
});

test("a pending directory dialog is visible as busy and cannot open a second dialog", async () => {
  let finish: (value: string | null) => void = () => {};
  const pending = new Promise<string | null>(resolve => { finish = resolve; });
  const f = await fixture(() => pending);
  const first = f.settings.chooseFileAccessDirectory();
  assert.equal(f.settings.fileAccessDirectoryChoosing.value, true);
  await f.settings.chooseFileAccessDirectory();
  assert.equal(f.commands.filter(command => command === "plugin:dialog|open").length, 1);
  finish(null);
  await first;
  assert.equal(f.settings.fileAccessDirectoryChoosing.value, false);
});

test("a native dialog failure stays visible on the page and releases the picker", async () => {
  const f = await fixture(() => { throw new Error("native dialog unavailable"); });
  await f.settings.chooseSessionArtifactDirectory();
  assert.equal(f.settings.fileAccessMessage.value, "settings.fileAccess.chooseFailed");
  assert.equal(f.settings.fileAccessMessageTone.value, "error");
  assert.equal(f.settings.fileAccessDirectoryChoosing.value, false);
  assert.deepEqual(f.notices, [], "the inline error already explains the failed picker");
  assert.equal(f.commands.includes("grant_file_access_directory"), false);
});

test("damaged authorization settings show a localized error without creating replacement grants", async () => {
  const f = await fixture(() => null, undefined, "file_access_settings_invalid");
  await new Promise<void>(resolve => setImmediate(resolve));
  assert.equal(f.settings.fileAccessMessage.value, "settings.fileAccess.settingsInvalid");
  assert.equal(f.settings.fileAccessMessageTone.value, "error");
  assert.equal(f.commands.includes("grant_file_access_directory"), false);
});

const builtin = { path: "/app/Artifacts", addedAt: "1", isBuiltin: true };
const custom = { path: "/user/captures", addedAt: "2", isBuiltin: false };

test("startup selects the first builtin directory and persists a stable builtin choice", async () => {
  const f = await fixture(() => null, undefined, undefined, { directories: [builtin, custom] });
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, builtin.path);
  assert.equal(f.settings.sessionSettings.value.artifactDirectoryMode, "builtin");
  assert.equal(f.settings.artifactDirectoryOptions.value[0].value, builtin.path);
  assert.equal(f.settings.fileAccessDirectories.value[0].isBuiltin, true);
  assert.equal(JSON.parse(f.entries.get(SESSION_SETTINGS_STORAGE_KEY)!).artifactDirectoryMode, "builtin");
});

test("startup keeps an existing custom selection", async () => {
  const f = await fixture(() => null, undefined, undefined, {
    directories: [builtin, custom], settings: { artifactDirectory: custom.path },
  });
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, custom.path);
  assert.equal(f.settings.sessionSettings.value.artifactDirectoryMode, "custom");
});

test("a builtin choice resolves its latest path after restart instead of using a cached old location", async () => {
  const f = await fixture(() => null, undefined, undefined, {
    directories: [builtin, { ...custom, path: "/old/builtin" }],
    settings: { artifactDirectory: "/old/builtin", artifactDirectoryMode: "builtin" },
  });
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, builtin.path);
});

test("changing the builtin follows the active builtin selection and merges existing grants", async () => {
  const f = await fixture(() => custom.path, custom.path, undefined, { directories: [builtin, custom] });
  await f.settings.chooseBuiltinFileAccessDirectory();
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, custom.path);
  assert.equal(f.settings.sessionSettings.value.artifactDirectoryMode, "builtin");
  assert.deepEqual(f.settings.fileAccessDirectories.value.map(entry => [entry.path, entry.isBuiltin]), [
    [custom.path, true], [builtin.path, false],
  ]);
  assert.equal(f.settings.canRevealSessionArtifactDirectory.value, true);
});

test("changing builtin while a custom save location is selected preserves that custom location", async () => {
  const f = await fixture(() => "/new/builtin", "/new/builtin", undefined, {
    directories: [builtin, custom], settings: { artifactDirectory: custom.path },
  });
  await f.settings.chooseBuiltinFileAccessDirectory();
  assert.equal(f.settings.fileAccessDirectories.value[0].path, "/new/builtin");
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, custom.path);
  assert.equal(f.settings.sessionSettings.value.artifactDirectoryMode, "custom");
});

test("revoking the current custom location falls back to builtin and builtin cannot be revoked", async () => {
  const f = await fixture(() => null, undefined, undefined, {
    directories: [builtin, custom], settings: { artifactDirectory: custom.path },
  });
  await f.settings.revokeFileAccessDirectory(custom.path);
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, builtin.path);
  assert.equal(f.settings.sessionSettings.value.artifactDirectoryMode, "builtin");
  const calls = f.commands.filter(command => command === "revoke_file_access_directory").length;
  await f.settings.revokeFileAccessDirectory(builtin.path);
  assert.equal(f.commands.filter(command => command === "revoke_file_access_directory").length, calls);
  assert.equal(f.notices.at(-1), "settings.fileAccess.builtinCannotRemove");
});

test("cancelled or failed builtin edits preserve grants and selection and release the picker", async () => {
  for (const fail of [false, true]) {
    const f = await fixture(() => fail ? "/new/builtin" : null, undefined, undefined, {
      directories: [builtin, custom], builtinFailure: fail,
    });
    await f.settings.chooseBuiltinFileAccessDirectory();
    assert.equal(f.settings.fileAccessDirectories.value[0].path, builtin.path);
    assert.equal(f.settings.sessionSettings.value.artifactDirectory, builtin.path);
    assert.equal(f.settings.fileAccessDirectoryChoosing.value, false);
    if (fail) {
      assert.equal(f.settings.fileAccessMessage.value, "settings.fileAccess.builtinUpdateFailed");
      assert.deepEqual(f.notices, [], "the inline error already explains the failed edit");
    }
    else assert.equal(f.commands.includes("set_builtin_file_access_directory"), false);
  }
});

test("a delayed check of an old directory cannot replace the current directory status", async () => {
  const f = await fixture(() => null, undefined, undefined, { directories: [builtin, custom] });
  let finishOldCheck: (status: unknown) => void = () => {};
  mockIPC((command, args) => {
    assert.equal(command, "inspect_session_artifact_directory");
    if (args?.directory === custom.path) return new Promise(resolve => { finishOldCheck = resolve; });
    return { directory: args?.directory, status: "ready", exists: true, isDirectory: true, writable: true, message: "" };
  });
  const oldCheck = f.settings.setSessionArtifactDirectory(custom.path);
  await f.settings.setSessionArtifactDirectory(builtin.path);
  finishOldCheck({ directory: custom.path, status: "missing", exists: false, isDirectory: false, writable: false, message: "" });
  await oldCheck;
  assert.equal(f.settings.sessionSettings.value.artifactDirectory, builtin.path);
  assert.equal(f.settings.sessionArtifactDirectoryStatus.value?.directory, builtin.path);
  assert.equal(f.settings.canRevealSessionArtifactDirectory.value, true);
});
