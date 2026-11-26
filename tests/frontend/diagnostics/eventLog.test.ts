import assert from "node:assert/strict";
import test from "node:test";

import { appendEventLog, eventsForSession } from "../../../src/diagnostics/eventLog.ts";

class LocalTimeDate extends Date {
  getHours(): number {
    return 23;
  }

  getMinutes(): number {
    return 4;
  }

  getSeconds(): number {
    return 5;
  }

  toISOString(): string {
    return "2026-06-28T10:01:02.000Z";
  }
}

test("appendEventLog puts new entries first and preserves older entries", () => {
  const next = appendEventLog(
    [{ id: "old", time: "10:00:00", level: "info", source: "bridge", message: "old event" }],
    { level: "success", source: "rfb-core", message: "connected" },
    { now: () => new Date(2026, 5, 28, 10, 1, 2), id: () => "new" },
  );

  assert.deepEqual(next, [
    { id: "new", time: "10:01:02", level: "success", source: "rfb-core", message: "connected" },
    { id: "old", time: "10:00:00", level: "info", source: "bridge", message: "old event" },
  ]);
});

test("appendEventLog formats event time from the local machine timezone", () => {
  const next = appendEventLog(
    [],
    { level: "info", source: "probe", message: "local time" },
    { now: () => new LocalTimeDate(), id: () => "local" },
  );

  assert.equal(next[0].time, "23:04:05");
});

test("appendEventLog trims entries to the requested limit", () => {
  const next = appendEventLog(
    [
      { id: "1", time: "10:00:01", level: "info", source: "probe", message: "one" },
      { id: "2", time: "10:00:02", level: "info", source: "probe", message: "two" },
    ],
    { level: "info", source: "bridge", message: "three" },
    { limit: 2, now: () => new Date("2026-06-28T10:00:03Z"), id: () => "3" },
  );

  assert.deepEqual(next.map((entry) => entry.id), ["3", "1"]);
});

test("session diagnostics use explicit session identity and retain a separate user message", () => {
  const entries = [
    { id: "a", time: "10:00:00", level: "error" as const, source: "会话", message: "failure_code=target_unreachable", userMessage: "无法连接到目标主机", origin: "conversation" as const, conversationId: "session-a" },
    { id: "b", time: "10:00:01", level: "warning" as const, source: "会话", message: "同一目标的另一会话", origin: "conversation" as const, conversationId: "session-b" },
    { id: "c", time: "10:00:02", level: "info" as const, source: "会话", message: "常规检测", origin: "conversation" as const, conversationId: "session-a" },
    { id: "d", time: "10:00:03", level: "error" as const, source: "主机", message: "主机错误", origin: "host" as const, conversationId: "session-a" },
  ];
  const scoped = eventsForSession(entries, "session-a");
  assert.deepEqual(scoped.map((entry) => entry.id), ["a", "c"]);
  assert.equal(scoped[0].userMessage, "无法连接到目标主机");
  assert.equal(scoped[0].message, "failure_code=target_unreachable");
});

test("appendEventLog stores user-facing text separately from technical detail", () => {
  const [entry] = appendEventLog([], {
    level: "success", source: "会话", message: "Native RFB Core transport ready; transport=native:id",
    eventKey: "vnc_connected", userMessage: "已建立 VNC 连接",
  });
  assert.equal(entry.eventKey, "vnc_connected");
  assert.equal(entry.userMessage, "已建立 VNC 连接");
  assert.match(entry.message, /transport=native:id/);
});
