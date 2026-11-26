import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

const windowsOnly = { skip: process.platform !== "win32" };

function runFixture(checks: string, commands: Record<string, string>) {
  const directory = mkdtempSync(join(tmpdir(), "llremote 脚本 space-"));
  const runner = join(directory, "scripts", "windows", "test.ps1");
  mkdirSync(join(directory, "scripts", "windows"), { recursive: true });
  copyFileSync(new URL("../../../scripts/windows/test.ps1", import.meta.url), runner);
  for (const [name, body] of Object.entries(commands)) {
    writeFileSync(join(directory, `${name}.cmd`), `@echo off\r\n${body}\r\n`);
  }
  const source = `$ProgressPreference='SilentlyContinue'; $global:LASTEXITCODE=0; & '${runner.replaceAll("'", "''")}' -Checks '${checks}'; exit $LASTEXITCODE`;
  try {
    const result = spawnSync(join(process.env.SystemRoot ?? "C:\\Windows", "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), [
      "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", Buffer.from(source, "utf16le").toString("base64"),
    ], { encoding: "utf8", env: { ...process.env, PATH: directory }, timeout: 15000 });
    const line = result.stdout.split(/\r?\n/).find((line) => line.startsWith("VNC_WINDOWS_RESULT="));
    assert.ok(line, result.stderr);
    return { stderr: result.stderr, code: result.status, result: JSON.parse(line.slice("VNC_WINDOWS_RESULT=".length)) };
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

test("Windows runner rejects a missing pnpm even after a preceding successful command", windowsOnly, () => {
  const { code, result, stderr } = runFixture("rust-check,frontend", { cargo: "exit /b 0" });
  assert.equal(code, 10, JSON.stringify({ result, stderr }));
  assert.equal(result.status, "failed");
  assert.equal(result.checks[0].status, "passed");
  assert.equal(result.checks[1].status, "failed");
});

test("Windows runner rejects successful processes without test execution evidence", windowsOnly, () => {
  for (const [check, command] of [["rust", "cargo"], ["frontend", "pnpm"]]) {
    const { code, result, stderr } = runFixture(check, { [command]: "exit /b 0" });
    assert.equal(code, 10, JSON.stringify({ result, stderr }));
    assert.equal(result.checks[0].status, "failed");
  }
});

test("Windows runner records actual test counts from a cmd fallback in a Unicode path", windowsOnly, () => {
  const { code, result, stderr } = runFixture("frontend", { pnpm: 'if not "%~1"=="test:ci" exit /b 7\r\necho # tests 3\r\necho # pass 2\r\necho # fail 0\r\necho # cancelled 0\r\necho # skipped 1\r\nexit /b 0' });
  assert.equal(code, 0, JSON.stringify({ result, stderr }));
  assert.deepEqual(result.checks[0].testResults, { total: 3, passed: 2, failed: 0, cancelled: 0, skipped: 1 });
});

test("Windows runner accepts and totals successful Rust test evidence", windowsOnly, () => {
  const { code, result, stderr } = runFixture("rust", { cargo: "echo test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\r\nexit /b 0" });
  assert.equal(code, 0, JSON.stringify({ result, stderr }));
  assert.equal(result.checks[0].testResults[0].passed, 2);
  assert.equal(result.checks[0].testResults[0].ignored, 1);
});
