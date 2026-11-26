import assert from "node:assert/strict";
import test from "node:test";
import { createSessionSettings } from "../../../src/session/settings/sessionSettings.ts";
import { loadSessionSettings, saveSessionSettings, SESSION_SETTINGS_STORAGE_KEY } from "../../../src/session/settings/sessionSettingsStore.ts";

function createMemoryStorage(initial: Record<string, string> = {}) {
  const entries = new Map(Object.entries(initial));

  return {
    getItem(key: string) {
      return entries.get(key) ?? null;
    },
    setItem(key: string, value: string) {
      entries.set(key, value);
    },
  };
}

test("loadSessionSettings returns defaults when storage has no saved settings", () => {
  assert.deepEqual(loadSessionSettings(createMemoryStorage()), createSessionSettings());
});

test("session settings storage key is versioned for default-behavior migrations", () => {
  assert.equal(SESSION_SETTINGS_STORAGE_KEY, "llremote.session-settings.v2");
});

test("saveSessionSettings writes normalized settings and loadSessionSettings restores them", () => {
  const storage = createMemoryStorage();
  const settings = createSessionSettings({
    defaultQualityProfile: "fast",
    confirmBeforeConnect: false,
    fullscreenExitShortcut: "Control+Shift+F",
    clipboardSyncMode: "off",
    artifactDirectory: "/tmp/evidence",
    fileTransferRenamePolicy: "skip",
    connectionDefaults: {
      postConnectPresentation: "external",
      readonly: true,
      saveTemporaryHost: true,
    },
  });

  assert.equal(saveSessionSettings(storage, settings), true);
  assert.deepEqual(loadSessionSettings(storage), settings);
});

test("malformed and obsolete persisted session settings fail instead of enabling defaults", () => {
  const defaults = createSessionSettings();
  for (const raw of ["{", "", "null", "[]", "{}",
    JSON.stringify({ clipboardSyncEnabled: false }),
    JSON.stringify({ ...defaults, defaultQualityProfile: "unsupported" }),
    JSON.stringify({ ...defaults, clipboardSyncMode: "mirror" }),
    JSON.stringify({ ...defaults, fullscreenExitShortcut: "A" }),
    JSON.stringify({ ...defaults, confirmBeforeConnect: "false" }),
    JSON.stringify({ ...defaults, fileTransferRenamePolicy: "ask" }),
    JSON.stringify({ ...defaults, artifactDirectoryMode: "unknown" }),
    JSON.stringify({ ...defaults, connectionDefaults: { ...defaults.connectionDefaults, postConnectPresentation: ["main"] } }),
  ]) {
    const storage = createMemoryStorage({ [SESSION_SETTINGS_STORAGE_KEY]: raw });
    assert.throws(() => loadSessionSettings(storage), { message: "session_settings_invalid" });
    assert.equal(storage.getItem(SESSION_SETTINGS_STORAGE_KEY), raw);
  }
});

test("existing session settings without a global upload policy keep the safe rename default", () => {
  const { fileTransferRenamePolicy: _policy, ...existing } = createSessionSettings();
  const storage = createMemoryStorage({ [SESSION_SETTINGS_STORAGE_KEY]: JSON.stringify(existing) });
  assert.equal(loadSessionSettings(storage).fileTransferRenamePolicy, "autoRename");
});

test("session settings storage failures propagate to the caller", () => {
  const throwingStorage = {
    getItem() {
      throw new Error("storage unavailable");
    },
    setItem() {
      throw new Error("storage unavailable");
    },
  };

  assert.throws(() => loadSessionSettings(throwingStorage), /storage unavailable/);
  assert.throws(() => saveSessionSettings(throwingStorage, createSessionSettings()), /storage unavailable/);
});

test("legacy artifact paths retain custom selection while empty paths default to the builtin", () => {
  for (const artifactDirectory of ["", "/Users/tester/captures"]) {
    const { artifactDirectoryMode: _mode, ...legacy } = createSessionSettings({ artifactDirectory });
    const storage = createMemoryStorage({ [SESSION_SETTINGS_STORAGE_KEY]: JSON.stringify(legacy) });
    const restored = loadSessionSettings(storage);
    assert.equal(restored.artifactDirectory, artifactDirectory);
    assert.equal(restored.artifactDirectoryMode, artifactDirectory ? "custom" : "builtin");
  }
});
