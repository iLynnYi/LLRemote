import assert from "node:assert/strict";
import test from "node:test";
import {
  createSessionSettings,
  normalizeFullscreenExitShortcut,
  normalizeSessionSettings,
  updateSessionSettings,
  type SessionSettingsInput,
} from "../../../src/session/settings/sessionSettings.ts";

test("createSessionSettings returns conservative session workspace defaults", () => {
  assert.deepEqual(createSessionSettings(), {
    defaultQualityProfile: "balanced",
    confirmBeforeConnect: true,
    fullscreenExitShortcut: "Control+Shift+F",
    clipboardSyncMode: "local-to-active",
    artifactDirectory: "",
    artifactDirectoryMode: "builtin",
    fileTransferRenamePolicy: "autoRename",
    connectionDefaults: {
      postConnectPresentation: "main",
      readonly: false,
      saveTemporaryHost: false,
    },
  });
});

test("normalizeSessionSettings accepts supported defaults", () => {
  const input: SessionSettingsInput = {
    defaultQualityProfile: "sharp",
    confirmBeforeConnect: false,
    fullscreenExitShortcut: "Control",
    clipboardSyncMode: "bidirectional-active",
    artifactDirectory: "/Users/pro/Desktop/evidence",
    artifactDirectoryMode: "custom",
    fileTransferRenamePolicy: "overwrite",
    connectionDefaults: {
      postConnectPresentation: "external",
      readonly: true,
      saveTemporaryHost: true,
    },
  };

  assert.deepEqual(normalizeSessionSettings(input), input);
});

test("normalizeSessionSettings ignores unsupported values and preserves safe defaults", () => {
  assert.deepEqual(
    normalizeSessionSettings({
      defaultQualityProfile: "pixelated" as SessionSettingsInput["defaultQualityProfile"],
      confirmBeforeConnect: "nope" as unknown as boolean,
      fullscreenExitShortcut: "",
      clipboardSyncMode: "mirror" as SessionSettingsInput["clipboardSyncMode"],
      artifactDirectory: 123 as unknown as string,
      connectionDefaults: {
        postConnectPresentation: "detached" as unknown as SessionSettingsInput["connectionDefaults"]["postConnectPresentation"],
        readonly: true,
        saveTemporaryHost: true,
      },
    }),
    {
      defaultQualityProfile: "balanced",
      confirmBeforeConnect: true,
      fullscreenExitShortcut: "Control+Shift+F",
      clipboardSyncMode: "local-to-active",
      artifactDirectory: "",
      artifactDirectoryMode: "builtin",
      fileTransferRenamePolicy: "autoRename",
      connectionDefaults: {
        postConnectPresentation: "main",
        readonly: true,
        saveTemporaryHost: true,
      },
    },
  );
});

test("updateSessionSettings merges partial changes without resetting existing defaults", () => {
  const current = createSessionSettings({
    fullscreenExitShortcut: "Alt+F",
      connectionDefaults: {
        readonly: true,
        postConnectPresentation: "fullscreen",
      },
    });

  assert.deepEqual(
    updateSessionSettings(current, {
      defaultQualityProfile: "fast",
      artifactDirectory: "/tmp/evidence",
      connectionDefaults: {
      },
    }),
    {
      defaultQualityProfile: "fast",
      confirmBeforeConnect: true,
      fullscreenExitShortcut: "Alt+F",
      clipboardSyncMode: "local-to-active",
      artifactDirectory: "/tmp/evidence",
      fileTransferRenamePolicy: "autoRename",
      artifactDirectoryMode: "custom",
      connectionDefaults: {
        postConnectPresentation: "fullscreen",
        readonly: true,
        saveTemporaryHost: false,
      },
    },
  );
});

test("normalizeSessionSettings uses explicit clipboard modes", () => {
  for (const clipboardSyncMode of ["off", "local-to-active", "bidirectional-active", "local-to-all", "bidirectional-all"] as const) {
    assert.equal(normalizeSessionSettings({ clipboardSyncMode }).clipboardSyncMode, clipboardSyncMode);
  }
  assert.equal("clipboardSyncEnabled" in createSessionSettings(), false);
});

test("normalizeFullscreenExitShortcut accepts modifiers combos and standalone function keys", () => {
  assert.equal(normalizeFullscreenExitShortcut(undefined), "Control+Shift+F");
  assert.equal(normalizeFullscreenExitShortcut(""), "Control+Shift+F");
  assert.equal(normalizeFullscreenExitShortcut(" esc "), "Escape");
  assert.equal(normalizeFullscreenExitShortcut("F11"), "F11");
  assert.equal(normalizeFullscreenExitShortcut("A"), "Control+Shift+F");
  assert.equal(normalizeFullscreenExitShortcut("ctrl+shift+f"), "Control+Shift+F");
  assert.equal(normalizeFullscreenExitShortcut("Meta"), "Meta");
});
