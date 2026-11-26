import assert from "node:assert/strict";
import test from "node:test";
import { resolveArtifactDirectorySelection } from "../../../src/session/settings/artifactDirectorySelection.ts";

const builtin = { path: "/app/Artifacts", isBuiltin: true };
const custom = { path: "/user/captures", isBuiltin: false, status: "missing" };

test("unavailable custom directories stay selected while revoked grants fall back to builtin", () => {
  const current = { artifactDirectory: custom.path, artifactDirectoryMode: "custom" as const };
  assert.deepEqual(resolveArtifactDirectorySelection(current, [builtin, custom]), current);
  assert.deepEqual(resolveArtifactDirectorySelection(current, [builtin]), {
    artifactDirectory: builtin.path, artifactDirectoryMode: "builtin",
  });
  assert.deepEqual(resolveArtifactDirectorySelection(current, []), current);
});
