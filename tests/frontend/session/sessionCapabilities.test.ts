import { sessionSupportsCapability } from "../../../src/session/connection-model/connectionModel.ts";
import assert from "node:assert/strict";
import test from "node:test";

import {
  requiredCapabilitiesForAction,
  sessionSupportsAction,
} from "../../../src/session/transfers/sessionCapabilities.ts";

test("session capability helper maps enhanced actions to concrete Agent capabilities", () => {
  assert.deepEqual(requiredCapabilitiesForAction("sendFile"), ["file.transfer.upload"]);
  assert.deepEqual(requiredCapabilitiesForAction("checkAgentPermissions"), ["agent.permissions.diagnose"]);
  assert.deepEqual(requiredCapabilitiesForAction("enumerateAgentDisplays"), ["agent.display.enumerate"]);
  assert.deepEqual(requiredCapabilitiesForAction("wakeDisplay"), ["display.wake"]);
});

test("session capability helper requires a supported capability in the current matrix", () => {
  assert.equal(sessionSupportsCapability({
    items: [{ id: "file.transfer.upload", state: "supported", source: "agent" }],
  }, "file.transfer.upload"), true);

  assert.equal(sessionSupportsCapability({
    items: [{ id: "file.transfer.upload", state: "unsupported", source: "agent" }],
  }, "file.transfer.upload"), false);
});

test("session capability helper does not enable file transfer from Agent presence alone", () => {
  assert.equal(sessionSupportsAction({
    items: [{ id: "clipboard.text.write", state: "supported", source: "agent" }],
  }, "sendFile"), false);

  assert.equal(sessionSupportsAction({
    items: [{ id: "file.transfer.upload", state: "supported", source: "agent" }],
  }, "sendFile"), true);
});
