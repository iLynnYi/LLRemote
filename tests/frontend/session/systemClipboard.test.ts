import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { createSystemClipboard } from "../../../src/session/clipboard/systemClipboard.ts";

test("system clipboard uses the Tauri clipboard bridge inside the desktop runtime", async () => {
  const calls: string[] = [];
  const clipboard = createSystemClipboard({
    browserClipboard: {
      readText: async () => "browser text",
      writeText: async (text) => {
        calls.push(`browser:${text}`);
      },
    },
    isTauriRuntime: () => true,
    tauriClipboard: {
      readText: async () => "tauri text",
      writeText: async (text) => {
        calls.push(`tauri:${text}`);
      },
    },
  });

  assert.equal(await clipboard?.readText?.(), "tauri text");
  await clipboard?.writeText("remote text");

  assert.deepEqual(calls, ["tauri:remote text"]);
});

test("system clipboard falls back to the browser clipboard outside Tauri", async () => {
  const clipboard = createSystemClipboard({
    browserClipboard: {
      readText: async () => "browser text",
      writeText: async () => {},
    },
    isTauriRuntime: () => false,
    tauriClipboard: {
      readText: async () => "tauri text",
      writeText: async () => {},
    },
  });

  assert.equal(await clipboard?.readText?.(), "browser text");
});

test("desktop capability grants clipboard text read and write commands", () => {
  const capability = JSON.parse(readFileSync("src-tauri/capabilities/default.json", "utf8")) as {
    permissions?: unknown[];
  };

  assert.ok(capability.permissions?.includes("clipboard-manager:allow-read-text"));
  assert.ok(capability.permissions?.includes("clipboard-manager:allow-write-text"));
});
