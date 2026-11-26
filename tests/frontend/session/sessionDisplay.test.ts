import assert from "node:assert/strict";
import test from "node:test";

import { createSessionDisplayInfo, summarizeSessionDisplay } from "../../../src/session/viewport/sessionDisplay.ts";

test("session display summary handles unknown dimensions", () => {
  assert.deepEqual(summarizeSessionDisplay(null), {
    width: 0,
    height: 0,
    aspectRatio: 0,
    multiDisplayHint: false,
    label: "sessions.display.unknown",
    detail: "sessions.display.unknownDetail",
  });

  assert.deepEqual(summarizeSessionDisplay(createSessionDisplayInfo(0, 1080)), {
    width: 0,
    height: 0,
    aspectRatio: 0,
    multiDisplayHint: false,
    label: "sessions.display.unknown",
    detail: "sessions.display.unknownDetail",
  });
});

test("session display summary treats normal aspect ratios as single display", () => {
  const info = createSessionDisplayInfo(1280, 800);

  assert.equal(info.aspectRatio, 1.6);
  assert.equal(info.multiDisplayHint, false);
  assert.equal(summarizeSessionDisplay(info).label, "sessions.display.singleHint");
});

test("session display summary labels ultra-wide framebuffers as multiple displays", () => {
  const info = createSessionDisplayInfo(3840, 1080);

  assert.equal(info.aspectRatio, 3840 / 1080);
  assert.equal(info.multiDisplayHint, true);
  assert.equal(summarizeSessionDisplay(info).detail, "sessions.display.multiHintDetail");
});

test("session display copy uses deterministic single or multiple display wording", async () => {
  const zh = await import("../../../src/i18n/messages/zh-CN.ts");

  assert.equal(zh.zhCN.sessions.display.singleHint, "单屏");
  assert.equal(zh.zhCN.sessions.display.multiHint, "多屏");
  assert.doesNotMatch(zh.zhCN.sessions.display.multiHint, /疑似|可能/);
});

test("session display info rounds and clamps unsafe dimensions", () => {
  assert.deepEqual(createSessionDisplayInfo(1279.7, -200), {
    width: 1280,
    height: 0,
    aspectRatio: 0,
    multiDisplayHint: false,
  });
});

test("session display info preserves real multi-display inventory over framebuffer ratio hints", async () => {
  const displayModule = await import("../../../src/session/viewport/sessionDisplay.ts");

  assert.equal(typeof displayModule.mergeSessionDisplayInfo, "function");

  const merged = displayModule.mergeSessionDisplayInfo(
    {
      width: 1671,
      height: 1080,
      aspectRatio: 1671 / 1080,
      multiDisplayHint: true,
    },
    createSessionDisplayInfo(1671, 1080),
    true,
  );

  assert.equal(merged.width, 1671);
  assert.equal(merged.height, 1080);
  assert.equal(merged.multiDisplayHint, true);
  assert.equal(summarizeSessionDisplay(merged).label, "sessions.display.multiHint");
});

test("session display info does not keep heuristic multi-display hints without inventory", async () => {
  const displayModule = await import("../../../src/session/viewport/sessionDisplay.ts");
  const merged = displayModule.mergeSessionDisplayInfo(
    createSessionDisplayInfo(3840, 1080),
    createSessionDisplayInfo(1671, 1080),
  );

  assert.equal(merged.multiDisplayHint, false);
  assert.equal(summarizeSessionDisplay(merged).label, "sessions.display.singleHint");
});
