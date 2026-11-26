import assert from "node:assert/strict";
import test from "node:test";

import { qualityProfileSettings } from "../../../src/session/settings/sessionQuality.ts";

test("qualityProfileSettings maps profiles to RFB quality and compression values", () => {
  assert.deepEqual(qualityProfileSettings("sharp"), { qualityLevel: 9, compressionLevel: 0 });
  assert.deepEqual(qualityProfileSettings("fast"), { qualityLevel: 4, compressionLevel: 1 });
  assert.deepEqual(qualityProfileSettings("balanced"), { qualityLevel: 6, compressionLevel: 2 });
});
