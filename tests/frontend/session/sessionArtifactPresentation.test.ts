import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { artifactDirectoryName, formatArtifactCreatedAt, formatArtifactDuration } from "../../../src/session/artifacts/sessionArtifactPresentation.ts";

let previousTimezone: string | undefined;
before(() => { previousTimezone = process.env.TZ; process.env.TZ = "Asia/Shanghai"; });
after(() => {
  if (previousTimezone === undefined) delete process.env.TZ;
  else process.env.TZ = previousTimezone;
});

test("artifact times convert UTC to client time and use the local calendar day", () => {
  const time = formatArtifactCreatedAt("2026-10-30T14:22:04.001Z", Date.parse("2026-10-30T15:00:00Z"), "zh-CN");
  assert.equal(time.isToday, true);
  assert.equal(time.time, "22:22:04");
  assert.match(time.full, /22:22:04/);
  const afterMidnight = formatArtifactCreatedAt("2026-10-30T14:22:04.001Z", Date.parse("2026-10-30T16:00:00Z"), "zh-CN");
  assert.equal(afterMidnight.isToday, false);
  assert.equal(afterMidnight.date, "10月30日");
  const localToday = formatArtifactCreatedAt("2026-10-30T17:01:02Z", Date.parse("2026-10-31T02:00:00Z"), "zh-CN");
  assert.equal(localToday.isToday, true);
  assert.equal(localToday.time, "01:01:02");
});

test("older years and malformed timestamps stay identifiable", () => {
  assert.equal(formatArtifactCreatedAt("2025-10-30T14:22:04Z", Date.parse("2026-10-30T15:00:00Z"), "zh-CN").date, "2025年10月30日");
  assert.deepEqual(formatArtifactCreatedAt("invalid", Date.now()), { isToday: false, date: "-", time: "", full: "-" });
});

test("save locations shorten Windows, UNC and Mac paths without losing root names", () => {
  assert.equal(artifactDirectoryName(String.raw`C:\Users\pro\AppData\Roaming\Artifacts`), "Artifacts");
  assert.equal(artifactDirectoryName(String.raw`\\server\shared\Artifacts`), "Artifacts");
  assert.equal(artifactDirectoryName("/Users/pro/Library/Application Support/Artifacts/"), "Artifacts");
  assert.equal(artifactDirectoryName("/"), "/");
  assert.equal(artifactDirectoryName("C:\\"), "C:\\");
});

test("recording durations show minutes and seconds, adding hours for long recordings", () => {
  assert.equal(formatArtifactDuration(156), "02:36");
  assert.equal(formatArtifactDuration(0), "00:00");
  assert.equal(formatArtifactDuration(3661.9), "01:01:01");
  assert.equal(formatArtifactDuration(-1), "00:00");
});
