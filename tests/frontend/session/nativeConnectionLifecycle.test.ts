import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const lifecycle = readFileSync(
  new URL("../../../src/features/sessions/composables/useSessionConnectionLifecycle.ts", import.meta.url),
  "utf8",
);

test("synchronous native connection status does not read an uninitialized protocol connection", () => {
  const connectBody = lifecycle.match(/async function connectVnc[\s\S]*?\n  async function disconnectSessionFromViewport/)?.[0] ?? "";

  assert.match(connectBody, /let protocolConnection: ProtocolConnection \| undefined;/);
  assert.match(connectBody, /const connecting = attempt\.connect\(\(\) => vncAdapter\.connect\(/);
  assert.match(connectBody, /protocolConnection = await connecting;/);
  assert.match(connectBody, /connectionPassword = undefined;\s*protocolConnection = await connecting;/);
  assert.doesNotMatch(connectBody, /const protocolConnection = await vncAdapter\.connect\(/);
  assert.match(connectBody, /displaySource: protocolConnection\?\.displaySource \?\? "vnc"/);
  assert.match(connectBody, /inputSource: protocolConnection\?\.inputSource \?\? "vnc"/);
});
