import assert from "node:assert/strict";
import test from "node:test";

import {
  createDisplayInfoFromLocalVncServer,
  isLoopbackHost,
  matchesLocalVncServerTarget,
  type LocalVncServerSessionOverview,
} from "../../../src/session/core/localVncServerSession.ts";

function overview(
  overrides: Partial<LocalVncServerSessionOverview> = {},
): LocalVncServerSessionOverview {
  return {
    status: "running",
    bindAddress: "0.0.0.0",
    port: 5903,
    addresses: [
      { label: "Loopback", address: "127.0.0.1", kind: "loopback" },
      { label: "LAN", address: "192.168.2.10", kind: "lan_candidate" },
    ],
    displays: [
      { id: "main", label: "Main display" },
    ],
    framebufferWidth: 1280,
    framebufferHeight: 800,
    ...overrides,
  };
}

test("isLoopbackHost accepts localhost and IPv4 or IPv6 loopback spellings", () => {
  assert.equal(isLoopbackHost("localhost"), true);
  assert.equal(isLoopbackHost(" 127.0.0.1 "), true);
  assert.equal(isLoopbackHost("127.12.0.3"), true);
  assert.equal(isLoopbackHost("::1"), true);
  assert.equal(isLoopbackHost("[::1]"), true);
  assert.equal(isLoopbackHost("192.168.2.10"), false);
});

test("matchesLocalVncServerTarget requires a managed server address and port match", () => {
  const running = overview();

  assert.equal(matchesLocalVncServerTarget(running, "127.0.0.1", 5903), true);
  assert.equal(matchesLocalVncServerTarget(running, "192.168.2.10", 5903), true);
  assert.equal(matchesLocalVncServerTarget(running, "192.168.2.11", 5903), false);
  assert.equal(matchesLocalVncServerTarget(running, "192.168.64.6", 5903), false);
  assert.equal(matchesLocalVncServerTarget(running, "127.0.0.1", 5904), false);
  assert.equal(matchesLocalVncServerTarget(overview({ status: "stopped" }), "127.0.0.1", 5903), false);
});

test("matchesLocalVncServerTarget accepts an explicit local server bind address without broad wildcard matching", () => {
  const running = overview({
    bindAddress: "192.168.64.6",
    addresses: [
      { label: "Loopback", address: "127.0.0.1", kind: "loopback" },
    ],
  });

  assert.equal(matchesLocalVncServerTarget(running, "192.168.64.6", 5903), true);
  assert.equal(matchesLocalVncServerTarget(running, "192.168.64.7", 5903), false);
  assert.equal(matchesLocalVncServerTarget(overview({ bindAddress: "0.0.0.0" }), "192.168.64.7", 5903), false);
});

test("createDisplayInfoFromLocalVncServer uses server framebuffer and real display count for session controls", () => {
  assert.deepEqual(createDisplayInfoFromLocalVncServer(overview({
    displays: [
      { id: "display:1", label: "Built-in" },
      { id: "display:2", label: "External" },
    ],
    framebufferWidth: 1920,
    framebufferHeight: 1080,
  })), {
    width: 1920,
    height: 1080,
    aspectRatio: 1920 / 1080,
    multiDisplayHint: true,
  });

  assert.deepEqual(createDisplayInfoFromLocalVncServer(overview({
    framebufferWidth: 0,
    framebufferHeight: 0,
  })), {
    width: 0,
    height: 0,
    aspectRatio: 0,
    multiDisplayHint: false,
  });
});
