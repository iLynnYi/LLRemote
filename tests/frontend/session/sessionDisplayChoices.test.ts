import assert from "node:assert/strict";
import test from "node:test";
import { createSessionDisplayChoices } from "../../../src/session/viewport/sessionDisplayChoices.ts";
import type { AgentDisplayInventoryResult } from "../../../src/session/protocol/protocolAdapter.ts";

function inventory(): AgentDisplayInventoryResult {
  return {
    target: "192.168.1.10:5903", source: "agent", checkedAt: "1",
    displays: ["display:1", "display:2"].map((id, index) => ({
      id, label: `Screen ${index + 1}`, isPrimary: index === 0, x: (index - 1) * 1920, y: 0,
      width: 1920, height: 1080, pixelWidth: 3840, pixelHeight: 2160, scaleFactor: 2,
    })),
  };
}

test("remote choices preserve two monitors, their command values and selection across window serialization", () => {
  const choices = createSessionDisplayChoices(null, inventory(), "display:2");
  const serialized = JSON.parse(JSON.stringify(choices));
  assert.equal(serialized.length, 2);
  assert.deepEqual(serialized.map(item => [item.value, item.selected, item.remote]),
    [["remote:display:1", false, true], ["remote:display:2", true, true]]);
  assert.equal(serialized[0].width, 1920);
  assert.equal(serialized[0].isPrimary, true);
});

test("local server choices retain the local command route and take priority over optional remote metadata", () => {
  const local = {
    selectedDisplayId: "display:2",
    displays: inventory().displays.map(display => ({ ...display, kind: display.isPrimary ? "main" as const : "display" as const })),
  };
  const choices = createSessionDisplayChoices(local, inventory(), "display:1");
  assert.deepEqual(choices.map(item => [item.value, item.selected, item.remote]),
    [["display:1", false, false], ["display:2", true, false]]);
});

test("missing or unplugged selections fall back to an available primary monitor", () => {
  for (const selected of [null, "unplugged"]) {
    assert.deepEqual(createSessionDisplayChoices(null, inventory(), selected).map(item => item.selected), [true, false]);
  }
  const current = inventory();
  current.displays.forEach(display => { display.isPrimary = false; });
  assert.deepEqual(createSessionDisplayChoices(null, current, null).map(item => item.selected), [true, false]);
});

test("empty inventory never invents a second monitor just to show a switch menu", () => {
  assert.deepEqual(createSessionDisplayChoices(null, null, null), []);
  const current = inventory();
  current.displays.length = 1;
  assert.equal(createSessionDisplayChoices(null, current, null).length, 1);
  current.displays.length = 0;
  assert.deepEqual(createSessionDisplayChoices(null, current, "display:2"), []);
});
