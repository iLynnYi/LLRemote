import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { accessSync, constants, mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseWindowsResult } from "../../../scripts/windows-runner.mjs";

const runner = fileURLToPath(new URL("../../../scripts/test-win.sh", import.meta.url));
const unixOnly = { skip: process.platform === "win32" };

test("Windows verification entry points expose the supported check selections", () => {
  accessSync(runner, process.platform === "win32" ? constants.R_OK : constants.X_OK);
  const powershell = readFileSync(new URL("../../../scripts/windows/test.ps1", import.meta.url), "utf8");
  for (const check of ["rust", "frontend", "tauri-build", "rust-check", "service-build"]) {
    assert.ok(powershell.includes(`'${check}'`));
  }
});

function run(args: string[], options: { host?: string; workspace?: string; result?: unknown; code?: number; invalidBytes?: boolean } = {}) {
  const directory = mkdtempSync(join(tmpdir(), "vnc-runner-check-"));
  const capture = join(directory, "ssh-args");
  const rawChecks = args[args.indexOf("--checks") + 1];
  const checks = rawChecks === "all" ? ["rust", "frontend", "tauri-build"] : rawChecks.split(",");
  const report = options.result ?? {
    schemaVersion: 1, platform: "windows", status: "passed", requestedChecks: checks,
    checks: checks.map((name) => ({ name, status: "passed" })),
  };
  writeFileSync(join(directory, "ssh"), `#!/bin/sh
printf '%s\\n' "$@" > "$VNC_RUNNER_TEST_CAPTURE"
${options.invalidBytes ? "printf '\\377\\n'" : ""}
printf '%s\\n' "$VNC_RUNNER_TEST_RESULT"
exit ${options.code ?? 0}
`, { mode: 0o755 });
  try {
    const result = spawnSync("bash", [runner, ...args], {
      encoding: "utf8", env: { ...process.env, PATH: `${directory}:${process.env.PATH}`, WIN_TEST_HOST: options.host ?? "windows-example", VNC_RUNNER_TEST_CAPTURE: capture, VNC_RUNNER_TEST_RESULT: `VNC_WINDOWS_RESULT=${JSON.stringify(report)}`, ...(options.workspace ? { WIN_TEST_WORKSPACE: options.workspace } : {}) },
    });
    return { status: result.status, stdout: result.stdout,
      sshArgs: existsSync(capture) ? readFileSync(capture, "utf8").trim().split("\n") : null };
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

test("Windows runner forwards exact Rust targets and preserves the unfiltered default", unixOnly, () => {
  const selected = run(["--checks", "rust", "--rust-tests", "lib,remote_session_tests,rfb_failure_tests"]);
  assert.equal(selected.status, 0);
  assert.ok(selected.sshArgs);
  assert.ok(decodedCommand(selected.sshArgs).includes("-Checks 'rust' -RustTests 'lib,remote_session_tests,rfb_failure_tests'"));
  const unfiltered = run(["--checks", "rust"]);
  assert.equal(unfiltered.status, 0);
  assert.ok(!decodedCommand(unfiltered.sshArgs).includes("-RustTests"));
});

test("Windows runner requires an explicit SSH destination", unixOnly, () => {
  const missing = run(["--checks", "frontend"], { host: "" });
  assert.equal(missing.status, 30);
  assert.equal(missing.sshArgs, null);
  const configured = run(["--checks", "frontend"], { host: "qa@windows.example" });
  assert.equal(configured.status, 0);
  assert.ok(configured.sshArgs?.includes("qa@windows.example"));
});

test("all checks can include an explicit Rust target selection", unixOnly, () => {
  const result = run(["--checks", "all", "--rust-tests", "lib"]);
  assert.equal(result.status, 0);
  assert.ok(decodedCommand(result.sshArgs).includes("-Checks 'rust,frontend,tauri-build' -RustTests 'lib'"));
});

test("compile and service build checks forward without selecting tests", unixOnly, () => {
  for (const checks of ["rust-check", "service-build", "rust-check,service-build"]) {
    const result = run(["--checks", checks]);
    assert.equal(result.status, 0);
    assert.ok(decodedCommand(result.sshArgs).includes(`-Checks '${checks}'`));
    assert.ok(!decodedCommand(result.sshArgs).includes("-RustTests"));
  }
});

test("invalid target selections never contact SSH or silently run the full suite", unixOnly, () => {
  for (const args of [
    ["--checks", "unknown"],
    ["--checks", "frontend", "--rust-tests", "lib"],
    ["--checks", "rust-check", "--rust-tests", "lib"],
    ["--checks", "service-build", "--rust-tests", "lib"],
    ["--checks", "rust", "--rust-tests", ""],
    ["--checks", "rust", "--rust-tests", "a;other"],
    ["--checks", "rust", "--rust-tests", "-package"],
    ["--checks", "rust", "--rust-tests", "a,"],
    ["--checks", "rust", "--rust-tests", "a,,b"],
    ["--checks", "rust", "--rust-tests", "a".repeat(65)],
    ["--checks", "rust", "--rust-tests", Array(33).fill("lib").join(",")],
  ]) {
    const result = run(args);
    assert.equal(result.status, 30);
    assert.equal(result.sshArgs, null);
  }
});

function decodedCommand(args: string[] | null): string {
  assert.ok(args?.includes("-EncodedCommand"));
  return Buffer.from(args[args.indexOf("-EncodedCommand") + 1], "base64").toString("utf16le");
}

test("SSH runner protects Unicode, spaces and shell metacharacters in the workspace", unixOnly, () => {
  const result = run(["--checks", "frontend"], { workspace: "C:\\中文 space\\O'Brien;$(example)" });
  assert.equal(result.status, 0);
  assert.ok(decodedCommand(result.sshArgs).includes("& 'C:\\中文 space\\O''Brien;$(example)\\scripts\\windows\\test.ps1'"));
});

test("SSH runner rejects empty, mismatched or inconsistent success reports", unixOnly, () => {
  const base = { schemaVersion: 1, platform: "windows", status: "passed", requestedChecks: ["frontend"], checks: [{ name: "frontend", status: "passed" }] };
  for (const options of [
    { result: { ...base, checks: [] } },
    { result: { ...base, requestedChecks: ["rust"] } },
    { result: { ...base, status: "failed", checks: [{ name: "frontend", status: "failed" }] } },
    { result: base, code: 10 },
  ]) {
    const result = run(["--checks", "frontend"], options);
    assert.equal(result.status, 40);
    assert.equal(JSON.parse(result.stdout.slice("VNC_WINDOWS_RESULT=".length)).status, "blocked");
  }
});

test("non-UTF8 diagnostics before the structured line do not break result extraction", unixOnly, () => {
  assert.equal(run(["--checks", "frontend"], { invalidBytes: true }).status, 0);
});

test("runner result parsing rejects absent, malformed and duplicate structured lines", () => {
  for (const output of ["", "VNC_WINDOWS_RESULT={", "VNC_WINDOWS_RESULT={}\nVNC_WINDOWS_RESULT={}"]) {
    assert.throws(() => parseWindowsResult(output, "frontend", 0));
  }
});
