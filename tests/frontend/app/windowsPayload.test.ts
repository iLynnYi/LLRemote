import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("actual payload staging extracts nested resources and stops on the first failure before interrupting sharing", async t => {
  const windows = process.platform === "win32";
  const compiler = [...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []), windows ? "makensis.exe" : "makensis"]
    .find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    assert.notEqual(process.env.LLREMOTE_REQUIRE_NSIS, "1", "NSIS execution is required for candidate verification");
    t.skip("NSIS compiler is not installed"); return;
  }
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const section = template.match(/^Section Install\r?\n([\s\S]*?)^SectionEnd/m)?.[1];
  assert.ok(section);
  const staging = section.match(/  StrCpy \$InstallStage "extract_payload"[\s\S]*?(?=  ; Recheck after)/)?.[0];
  assert.ok(staging, "complete extraction must precede querying/closing the old runtime");
  assert.ok(section.indexOf(staging) < section.indexOf('"$(InstallInterruptsSharing)"'));
  const safety = await readFile(new URL("../../../src-tauri/windows/install-safety.nsh", import.meta.url), "utf8");
  const check = safety.match(/^Function CheckPayloadExtraction\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(check);
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-payload-")));
  const path = (value: string) => value.replaceAll("$", "$$");
  const destinations = ["legal\\THIRD-PARTY-NOTICES.txt", "docs\\nested\\INSTALL.md", "helpers\\nested\\helper.txt"];
  try {
    const input = join(root, "input.txt");
    await writeFile(input, "payload fixture");
    // Only bind the Tauri loop data; execute the production extraction commands,
    // macro and error checker, including real directory/file failures.
    const rendered = staging
      .replace(/{{#each resources}}([\s\S]*?){{\/each}}/, (_, body: string) => destinations.slice(0, 2).map(relative => body
        .replaceAll("{{no-escape @key}}", path(input)).replaceAll("{{this.[1]}}", relative)).join("\n"))
      .replace(/{{#each binaries}}([\s\S]*?){{\/each}}/, (_, body: string) => body
        .replaceAll("{{no-escape @key}}", path(input)).replaceAll("{{this}}", destinations[2]));
    assert.ok(!rendered.includes("{{"), "all resource paths must be bound");
    const transaction = fileURLToPath(new URL("../../../src-tauri/windows/install-transaction.nsh", import.meta.url));
    for (const scenario of ["success", "blocked-directory", "blocked-file"]) {
      const executable = join(root, `${scenario}.exe`);
      const report = join(root, `${scenario}.txt`);
      const script = join(root, `${scenario}.nsi`);
      const blocker = scenario === "blocked-directory"
        ? 'FileOpen $0 "$PLUGINSDIR\\payload\\legal" w\nFileClose $0'
        : scenario === "blocked-file" ? 'CreateDirectory "$PLUGINSDIR\\payload\\legal\\THIRD-PARTY-NOTICES.txt"' : "";
      await writeFile(script, `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
!include "${path(transaction)}"
Name "Payload extraction fixture"
OutFile "${path(executable)}"
RequestExecutionLevel user
SilentInstall silent
Var InstallStage
Var HelperDetail
Var HelperNativeError
Var HelperExit
${check}
Function FailInstaller
  FileOpen $R9 "${path(report)}" w
  FileWrite $R9 "$InstallStage;$HelperDetail;system=$HelperNativeError;exit=$HelperExit"
  FileClose $R9
  SetErrorLevel 1
  Quit
FunctionEnd
Section
  InitPluginsDir
  CreateDirectory "$PLUGINSDIR\\payload"
  ${blocker}
${rendered}
  FileOpen $R9 "${path(report)}" w
${destinations.map(relative => `
  FileOpen $0 "$PLUGINSDIR\\payload\\${relative}" r
  FileRead $0 $1
  FileClose $0
  FileWrite $R9 "$1;"`).join("\n")}
  FileClose $R9
SectionEnd
Section Uninstall
SectionEnd
`);
      const compiled = spawnSync(compiler, [windows ? "/V2" : "-V2", script], { encoding: "utf8", timeout: 30_000 });
      assert.equal(compiled.status, 0, `${compiled.stdout}\n${compiled.stderr}`);
      if (!windows) continue;
      const result = spawnSync(executable, ["/S"], { timeout: 15_000 });
      assert.equal(result.error, undefined, `${scenario}: no retry dialog may block extraction`);
      assert.equal(result.status, scenario === "success" ? 0 : 1, scenario);
      const outcome = await readFile(report, "utf8");
      if (scenario === "success") assert.equal(outcome, "payload fixture;".repeat(3));
      else {
        assert.match(outcome, /^extract_payload;/);
        assert.match(outcome, scenario === "blocked-directory" ? /create_payload_directory/ : /extract_payload_file: legal\\THIRD-PARTY-NOTICES.txt/);
        assert.match(outcome, /exit=-1$/);
      }
    }
    if (!windows) t.diagnostic("Compiled nested resources and both filesystem-failure fixtures; execution requires Windows.");
  } finally { await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }); }
});
