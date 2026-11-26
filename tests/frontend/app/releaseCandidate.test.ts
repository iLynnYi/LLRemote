import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { fingerprintBuildInputs, requireFreshArtifact, verifyPeMachine } from "../../../scripts/release-candidate.mjs";

test("candidate verification rejects truncated, forged and wrong-architecture PE headers", () => {
  const image = Buffer.alloc(160);
  image.writeUInt16LE(0x5a4d, 0);
  image.writeUInt32LE(128, 0x3c);
  image.writeUInt32LE(0x4550, 128);
  image.writeUInt16LE(0xaa64, 132);
  verifyPeMachine(image, 0xaa64);
  assert.throws(() => verifyPeMachine(image, 0x8664), /architecture_mismatch/);
  assert.throws(() => verifyPeMachine(image.subarray(0, 140), 0xaa64), /invalid_pe/);
  image.writeUInt32LE(0xffffffff, 0x3c);
  assert.throws(() => verifyPeMachine(image, 0xaa64), /invalid_pe/);
  assert.throws(() => verifyPeMachine(Buffer.alloc(64), 0xaa64), /invalid_pe/);
});

test("candidate verification refuses stale and empty artifacts instead of reusing a prior build", () => {
  const directory = mkdtempSync(join(tmpdir(), "LLRemote-candidate-"));
  try {
    const file = join(directory, "candidate.exe");
    writeFileSync(file, "candidate");
    const startedAt = Date.now();
    requireFreshArtifact(file, startedAt);
    utimesSync(file, new Date(0), new Date(0));
    assert.throws(() => requireFreshArtifact(file, startedAt), /stale_or_empty/);
    writeFileSync(file, "");
    assert.throws(() => requireFreshArtifact(file, startedAt), /stale_or_empty/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test("a failed candidate recheck clears prior success records", () => {
  const root = fileURLToPath(new URL("../../../", import.meta.url));
  const directory = mkdtempSync(join(tmpdir(), "LLRemote-candidate-gate-"));
  try {
    mkdirSync(join(directory, "scripts"));
    for (const file of ["release-candidate.mjs", "verify-macos-bundle.mjs", "windows-installers.mjs"]) cpSync(join(root, "scripts", file), join(directory, "scripts", file));
    const output = join(directory, "src-tauri/target/aarch64-apple-darwin/release");
    mkdirSync(output, { recursive: true });
    for (const file of ["candidate-verification.json", "SHA256SUMS"]) writeFileSync(join(output, file), "previous success");
    const result = spawnSync(process.execPath, ["scripts/release-candidate.mjs", "verify", "aarch64-apple-darwin"], { cwd: directory, encoding: "utf8", timeout: 10000 });
    assert.equal(result.status, 1);
    for (const file of ["candidate-verification.json", "SHA256SUMS"]) assert.equal(existsSync(join(output, file)), false);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test("source mirror fingerprints detect build input changes and exclude credentials and outputs", () => {
  const directory = mkdtempSync(join(tmpdir(), "LLRemote-source-mirror-"));
  try {
    mkdirSync(join(directory, "src"));
    writeFileSync(join(directory, "src/main.ts"), "original");
    const original = fingerprintBuildInputs(directory);
    writeFileSync(join(directory, ".env"), crypto.randomUUID());
    mkdirSync(join(directory, "src-tauri/target"), { recursive: true });
    writeFileSync(join(directory, "src-tauri/target/output"), "artifact");
    assert.equal(fingerprintBuildInputs(directory), original);
    writeFileSync(join(directory, "src/main.ts"), "changed");
    assert.notEqual(fingerprintBuildInputs(directory), original);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test("Windows installer inspection accepts only the Tauri bundle marker change in embedded executables", async () => {
  const { nsisMainDigest, verifyWindowsPayload } = await import("../../../scripts/release-candidate.mjs");
  const directory = mkdtempSync(join(tmpdir(), "LLRemote-payload-test-"));
  try {
    const payload = join(directory, "extracted");
    mkdirSync(join(payload, "$PLUGINSDIR/payload/legal"), { recursive: true });
    const main = join(directory, "main.exe");
    const executable = "built __TAURI_BUNDLE_TYPE_VAR_UNK executable __TAURI_BUNDLE_TYPE_VAR_UNK";
    const packaged = executable.replace("__TAURI_BUNDLE_TYPE_VAR_UNK", "__TAURI_BUNDLE_TYPE_VAR_NSS");
    writeFileSync(main, executable);
    const licenseDigest = createHash("sha256").update("license").digest("hex");
    const expected = new Map([["LLRemote.exe", nsisMainDigest(main)], ["legal/LICENSE.txt", licenseDigest]]);
    const check = () => verifyWindowsPayload(payload, expected);
    assert.throws(check, /payload_missing/);
    const extractedMain = join(payload, "$PLUGINSDIR/payload/LLRemote.exe");
    writeFileSync(extractedMain, executable);
    writeFileSync(join(payload, "$PLUGINSDIR/payload/legal/LICENSE.txt"), "license");
    assert.throws(check, /payload_mismatch/);
    writeFileSync(extractedMain, packaged);
    check();
    writeFileSync(main, "missing bundle marker");
    assert.throws(() => nsisMainDigest(main), /candidate_bundle_type_marker_missing/);
    writeFileSync(join(payload, "LLRemote.exe"), "stale second copy");
    assert.throws(check, /payload_mismatch/);
    cpSync(extractedMain, join(payload, "LLRemote.exe"));
    writeFileSync(join(payload, "$PLUGINSDIR/payload/legal/LICENSE.txt"), "wrong resource");
    assert.throws(check, /payload_mismatch/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
