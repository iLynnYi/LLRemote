import assert from "node:assert/strict";
import test from "node:test";

import { isEnvFlagEnabled } from "../../../src/app/env.ts";

test("isEnvFlagEnabled accepts explicit truthy env values", () => {
  for (const value of ["1", "true", "TRUE", "yes", "on", " On "]) {
    assert.equal(isEnvFlagEnabled(value), true);
  }
});

test("isEnvFlagEnabled keeps unset or non-truthy values disabled", () => {
  for (const value of [undefined, null, "", "0", "false", "off", "debug", true]) {
    assert.equal(isEnvFlagEnabled(value), false);
  }
});
