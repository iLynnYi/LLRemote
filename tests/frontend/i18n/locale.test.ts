import assert from "node:assert/strict";
import test from "node:test";

import { supportedLocales } from "../../../src/i18n/locale.ts";
import { zhCN } from "../../../src/i18n/messages/zh-CN.ts";

test("supportedLocales exposes Chinese only", () => {
  assert.deepEqual(supportedLocales, ["zh-CN"]);
});

test("locale no longer exposes Agent screen stream copy", () => {
  assert.equal("toggle-agent-screen-stream" in zhCN.sessions.toolbox.actions, false);
  assert.equal("agentScreenStream" in zhCN.sessions, false);
});
