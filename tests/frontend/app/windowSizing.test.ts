import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

test("main window can fit a smaller logical desktop instead of enforcing a fixed 1280x768 frame", () => {
  const config = JSON.parse(readFileSync(new URL("../../../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const main = config.app.windows[0];
  assert.equal(main.preventOverflow, true);
  assert.equal(main.resizable, true);
  assert.equal(main.maximizable, true);
});
