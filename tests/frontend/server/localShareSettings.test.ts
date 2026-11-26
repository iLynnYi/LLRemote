import assert from "node:assert/strict";
import test from "node:test";
import {
  createLocalShareSettings,
  loadLocalShareSettings,
  LOCAL_SHARE_SETTINGS_STORAGE_KEY,
  saveLocalShareSettings,
} from "../../../src/server/localShareSettings.ts";

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

test("local share settings storage key is versioned", () => {
  assert.equal(LOCAL_SHARE_SETTINGS_STORAGE_KEY, "llremote.local-share-settings.v1");
});

test("loadLocalShareSettings returns local VNC defaults without saved settings", () => {
  assert.deepEqual(loadLocalShareSettings(createMemoryStorage()), createLocalShareSettings());
});

test("saveLocalShareSettings writes normalized local server form settings", () => {
  const storage = createMemoryStorage();
  const settings = createLocalShareSettings({
    port: 5904,
    bindAddress: "127.0.0.1",
    viewOnly: true,
  });

  assert.equal(saveLocalShareSettings(storage, settings), true);
  assert.deepEqual(loadLocalShareSettings(storage), settings);
  assert.equal(Object.prototype.hasOwnProperty.call(JSON.parse(storage.getItem(LOCAL_SHARE_SETTINGS_STORAGE_KEY)!), "password"), false);
});

test("malformed local share settings fail instead of changing the listener address or port", () => {
  const defaults = createLocalShareSettings();
  for (const raw of ["{", "", "null", "[]", "{}",
    JSON.stringify({ ...defaults, port: 70000 }),
    JSON.stringify({ ...defaults, bindAddress: "not-an-ip" }),
    JSON.stringify({ ...defaults, viewOnly: "false" }),
  ]) {
    const storage = createMemoryStorage({ [LOCAL_SHARE_SETTINGS_STORAGE_KEY]: raw });
    assert.throws(() => loadLocalShareSettings(storage), { message: "local_share_settings_invalid" });
    assert.equal(storage.getItem(LOCAL_SHARE_SETTINGS_STORAGE_KEY), raw);
  }
});

test("saving local share settings whitelists only non-sensitive fields", () => {
  const storage = createMemoryStorage();
  const legacySettings = { ...createLocalShareSettings(), password: "obsolete-password" };
  assert.equal(saveLocalShareSettings(storage, legacySettings), true);
  assert.equal(storage.getItem(LOCAL_SHARE_SETTINGS_STORAGE_KEY), JSON.stringify(createLocalShareSettings()));
});

test("local share settings storage failures propagate to the caller", () => {
  const throwingStorage = {
    getItem() {
      throw new Error("storage unavailable");
    },
    setItem() {
      throw new Error("storage unavailable");
    },
  };

  assert.throws(() => loadLocalShareSettings(throwingStorage), /storage unavailable/);
  assert.throws(() => saveLocalShareSettings(throwingStorage, createLocalShareSettings()), /storage unavailable/);
});
