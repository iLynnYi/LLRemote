import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

test("real NSIS compatibility policy requires an exact match between native Windows and payload architectures", async t => {
  const windows = process.platform === "win32";
  const compiler = [...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []), windows ? "makensis.exe" : "makensis"]
    .find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    assert.notEqual(process.env.LLREMOTE_REQUIRE_NSIS, "1", "NSIS execution is required for Windows candidate verification");
    t.skip("NSIS compiler is not installed"); return;
  }
  const source = await readFile(new URL("../../../src-tauri/windows/install-safety.nsh", import.meta.url), "utf8");
  const policy = source.match(/^Function CheckPayloadMachineSupport\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(policy);
  // Include every native/payload pairing and fail closed for unknown machines.
  const machines = [0x14c, 0x8664, 0xaa64];
  const cases = machines.flatMap(native => machines.map(payload => [native, payload, Number(native === payload)]));
  cases.push([0, 0x14c, 0], [0x8664, 0, 0], [0, 0, 0]);
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-architecture-")));
  try {
    const path = join(root, "policy.nsi");
    await writeFile(path, `
Unicode true
!include LogicLib.nsh
Name "Actual architecture policy"
OutFile "${join(root, "policy.exe")}"
SilentInstall silent
RequestExecutionLevel user
Var InstallNativeMachine
Var PeMachine
Var MachineSupported
${policy}
Section
${cases.map(([native, payload, expected]) => `
  StrCpy $InstallNativeMachine ${native}
  StrCpy $PeMachine ${payload}
  Call CheckPayloadMachineSupport
  \${If} $MachineSupported != ${expected}
    SetErrorLevel 1
    Quit
  \${EndIf}`).join("\n")}
SectionEnd
`);
    const compiled = spawnSync(compiler, [windows ? "/V2" : "-V2", path], { encoding: "utf8", timeout: 30_000 });
    assert.equal(compiled.status, 0, `${compiled.stdout}\n${compiled.stderr}`);
    if (!windows) { t.diagnostic(`Compiled ${cases.length} compatibility scenarios; execution requires Windows.`); return; }
    const result = spawnSync(join(root, "policy.exe"), ["/S"], { timeout: 15_000 });
    assert.equal(result.error, undefined);
    assert.equal(result.status, 0);
  } finally { await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }); }
});

test("native NSIS preflight calls Windows API addresses without crashing before the wizard", async t => {
  const windows = process.platform === "win32";
  const compiler = [...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []), windows ? "makensis.exe" : "makensis"]
    .find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    assert.notEqual(process.env.LLREMOTE_REQUIRE_NSIS, "1", "NSIS execution is required for candidate verification");
    t.skip("NSIS compiler is not installed"); return;
  }
  const source = await readFile(new URL("../../../src-tauri/windows/install-safety.nsh", import.meta.url), "utf8");
  const functions = ["PreflightCompatibility", "CheckPayloadMachineSupport"].map(name => {
    const body = source.match(new RegExp(`^Function ${name}\\r?\\n[\\s\\S]*?^FunctionEnd`, "m"))?.[0];
    assert.ok(body, `exercise the actual ${name} implementation`);
    return body;
  }).join("\n");
  const variables = source.match(/^Var .+$/gm)?.join("\n");
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-native-preflight-")));
  const path = (value: string) => value.replaceAll("$", "$$");
  try {
    for (const architecture of ["x86", "x64", "arm64"]) {
      const executable = join(root, `${architecture}.exe`);
      const report = join(root, `${architecture}.txt`);
      const script = join(root, `${architecture}.nsi`);
      await writeFile(script, `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
!define ARCH "${architecture}"
Name "Native preflight regression"
OutFile "${path(executable)}"
RequestExecutionLevel user
SilentInstall silent
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LangString InstallerIncompatible \${LANG_ENGLISH} "incompatible"
${variables}
${functions}
Function WriteOutcome
  FileOpen $R9 "${path(report)}" w
  FileWrite $R9 "$MachineSupported,$InstallNativeMachine,$InstallStage,$InstallSystem"
  FileClose $R9
FunctionEnd
Function LogInstallerStage
FunctionEnd
Function FailInstaller
  Call WriteOutcome
  SetErrorLevel 1
  Quit
FunctionEnd
Function .onInit
  Call PreflightCompatibility
  Call WriteOutcome
  SetErrorLevel 0
  Quit
FunctionEnd
Section
SectionEnd
`);
      const compiled = spawnSync(compiler, [windows ? "/V2" : "-V2", script], { encoding: "utf8", timeout: 30_000 });
      assert.equal(compiled.status, 0, `${compiled.stdout}\n${compiled.stderr}`);
      if (!windows) continue;
      const result = spawnSync(executable, ["/S"], { timeout: 15_000 });
      assert.equal(result.error, undefined, `${architecture}: ${result.error?.message}`);
      assert.ok(result.status === 0 || result.status === 1,
        `${architecture}: preflight crashed before reporting compatibility (exit ${result.status})`);
      const fields = (await readFile(report, "utf8")).split(",");
      const [supported, native] = fields.slice(0, 2).map(Number);
      assert.ok([0x14c, 0x8664, 0xaa64].includes(native), "the actual native architecture must be returned");
      const expected = ({ x86: 0x14c, x64: 0x8664, arm64: 0xaa64 })[architecture];
      assert.equal(supported, Number(expected === native), `${architecture} must match native Windows`);
      assert.equal(result.status, expected === native ? 0 : 1);
      assert.equal(fields[2], expected === native ? "system_compatibility" : "architecture_mismatch");
    }
    if (!windows) t.diagnostic("Compiled actual native-API preflight for all payload architectures; executing it requires Windows.");
  } finally {
    // Windows Error Reporting can retain a crashed fixture's image handle.
    // Preserve the crash assertion instead of masking it with an unlink error.
    try { await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }); }
    catch (error) {
      if (windows && ["EPERM", "EBUSY"].includes((error as NodeJS.ErrnoException).code || "")) {
        t.diagnostic(`Windows retained the temporary crash fixture: ${root}`);
      } else throw error;
    }
  }
});
