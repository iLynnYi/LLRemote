import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const security = JSON.parse(readFileSync(new URL("../../../src-tauri/tauri.conf.json", import.meta.url), "utf8")).app.security;
const tokens = (policy: Record<string, string>, directive: string) => policy[directive]?.split(/\s+/) ?? [];

test("production WebView CSP denies implicit sources and executable inline content", () => {
  assert.ok(security.csp && typeof security.csp === "object", "CSP must be explicitly enabled");
  assert.deepEqual(tokens(security.csp, "default-src"), ["'none'"]);
  assert.deepEqual(tokens(security.csp, "script-src"), ["'self'"]);
  assert.notEqual(security.dangerousDisableAssetCspModification, true, "Tauri must retain nonce/hash injection");
  for (const directive of ["base-uri", "object-src", "frame-src", "form-action"]) assert.deepEqual(tokens(security.csp, directive), ["'none'"]);
});

test("production connect sources cover IPC and ephemeral IPv4 Surface sockets without remote origins", () => {
  const allowed = new Set(["'self'", "ipc:", "http://ipc.localhost", "ws://127.0.0.1:*"]);
  assert.deepEqual(new Set(tokens(security.csp, "connect-src")), allowed);
  assert.ok(tokens(security.csp, "img-src").includes("data:"), "canvas cursor images need data URLs");
  assert.ok(tokens(security.csp, "style-src").includes("'unsafe-inline'"), "Vue styles and Naive UI CSS-in-JS need inline styles");
  for (const directive of ["img-src", "font-src", "style-src"]) {
    assert.ok(tokens(security.csp, directive).every((value) => ["'self'", "data:", "'unsafe-inline'"].includes(value)), directive);
  }
});

test("development HMR permissions stay separate from the production policy", () => {
  assert.ok(security.devCsp && typeof security.devCsp === "object");
  assert.deepEqual(tokens(security.devCsp, "script-src"), ["'self'"]);
  assert.ok(tokens(security.devCsp, "connect-src").includes("ws:"), "explicit TAURI_DEV_HOST may use a LAN HMR WebSocket");
  assert.ok(!tokens(security.csp, "connect-src").includes("ws:"));
});
