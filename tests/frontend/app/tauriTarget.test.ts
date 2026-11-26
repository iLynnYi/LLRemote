import assert from "node:assert/strict";
import test from "node:test";
import { tauriBuildArgs, tauriBuildTarget } from "../../../scripts/tauri-target.mjs";

test("inherited Cargo targets also select the Tauri bundler architecture", () => {
  assert.deepEqual(tauriBuildArgs(["build", "--bundles", "nsis"], "x86_64-pc-windows-msvc"),
    ["build", "--bundles", "nsis", "--target", "x86_64-pc-windows-msvc"]);
  assert.deepEqual(tauriBuildArgs(["bundle"], "aarch64-apple-darwin"), ["bundle", "--target", "aarch64-apple-darwin"]);
  assert.deepEqual(tauriBuildArgs(["build", "--", "--locked"], "aarch64-apple-darwin"),
    ["build", "--target", "aarch64-apple-darwin", "--", "--locked"]);
});

test("explicit target flags retain priority over inherited targets", () => {
  for (const option of [["--target", "aarch64-apple-darwin"], ["-t", "aarch64-apple-darwin"],
    ["--target=aarch64-apple-darwin"], ["-taarch64-apple-darwin"]]) {
    const args = ["build", ...option];
    assert.deepEqual(tauriBuildArgs(args, "x86_64-apple-darwin"), args);
    assert.equal(tauriBuildTarget(args), "aarch64-apple-darwin");
  }
});

test("native defaults, development and help commands retain their arguments", () => {
  for (const [args, target] of [[["build"], undefined], [["dev"], "x86_64-apple-darwin"],
    [["build", "--help"], "x86_64-apple-darwin"]] as const) {
    assert.deepEqual(tauriBuildArgs([...args], target), args);
  }
});
