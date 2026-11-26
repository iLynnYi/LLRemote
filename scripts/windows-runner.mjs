import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const psLiteral = (value) => `'${value.replaceAll("'", "''")}'`;

// SSH invokes a shell on Windows. Encoding avoids shell quoting and code pages.
export function encodeWindowsCheck(workspace, checks, rustTests = "") {
  const runner = `${workspace}\\scripts\\windows\\test.ps1`;
  const source = `$ProgressPreference='SilentlyContinue'; $OutputEncoding=[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); & ${psLiteral(runner)} -Checks ${psLiteral(checks)}${rustTests ? ` -RustTests ${psLiteral(rustTests)}` : ""}; exit $LASTEXITCODE`;
  return Buffer.from(source, "utf16le").toString("base64");
}

export function parseWindowsResult(output, checks, sshCode) {
  const lines = output.split(/\r?\n/).filter((line) => line.startsWith("VNC_WINDOWS_RESULT="));
  if (lines.length !== 1) throw new Error("runner_result_missing_or_ambiguous");
  const result = JSON.parse(lines[0].slice("VNC_WINDOWS_RESULT=".length));
  const requested = checks.split(",");
  if (result.schemaVersion !== 1 || result.platform !== "windows"
    || JSON.stringify(result.requestedChecks) !== JSON.stringify(requested)
    || !Array.isArray(result.checks) || result.checks.length !== requested.length
    || result.checks.some((entry, index) => entry.name !== requested[index] || !["passed", "failed"].includes(entry.status))) {
    throw new Error("runner_result_invalid");
  }
  const failed = result.checks.some((entry) => entry.status === "failed");
  if (result.status !== (failed ? "failed" : "passed") || sshCode !== (failed ? 10 : 0)) {
    throw new Error("runner_result_exit_code_mismatch");
  }
  return result;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [mode, ...args] = process.argv.slice(2);
    if (mode === "encode") console.log(encodeWindowsCheck(...args));
    else if (mode === "result") console.log(JSON.stringify(parseWindowsResult(readFileSync(0, "utf8"), args[0], Number(args[1]))));
    else throw new Error("runner_mode_invalid");
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
