import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("every helper failure has a distinct bilingual reason and survives the installer boundary", async () => {
  const [errors, hooks, template] = await Promise.all([
    readFile(new URL("../../../src-tauri/src/windows_uninstall/errors.rs", import.meta.url), "utf8"),
    readFile(new URL("../../../src-tauri/windows/service-hooks.nsh", import.meta.url), "utf8"),
    readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8"),
  ]);
  const categories = errors.match(/const ERRORS:[\s\S]*?\n\];/)?.[0];
  assert.ok(categories);
  const codes = [...categories.matchAll(/\("[^"]+",\s*(\d+)\)/g)].map(match => Number(match[1]));
  const lookup = hooks.match(/^!macro LLREMOTE_UNINSTALL_REASON\r?\n[\s\S]*?^!macroend/m)?.[0];
  assert.ok(lookup);
  const branches = [...lookup.matchAll(/\$\{(?:If|ElseIf)\} \$R0 = (\d+)\s+StrCpy \$R1 "\$\((Uninstall\w+)\)"/g)];
  assert.deepEqual(branches.map(match => Number(match[1])), codes, "helper and installer failure categories must agree");
  assert.ok(codes.length >= 30);
  for (const language of ["SIMPCHINESE", "ENGLISH"]) {
    const selected = branches.map(match => {
      const text = template.match(new RegExp(`^LangString ${match[2]} \\$\\{LANG_${language}\\} "([^\\r\\n]+)"$`, "m"))?.[1];
      assert.ok(text, `${match[2]} must have a ${language} reason and recovery message`);
      return text;
    });
    assert.equal(new Set(selected).size, codes.length, "different failures must not silently share the same generic message");
  }
  assert.match(lookup, /\$\{Else\}\s+StrCpy \$R1 "\$\(UninstallUnknownFailure\)"/);
  for (const language of ["SIMPCHINESE", "ENGLISH"]) {
    const context = template.match(new RegExp(`^LangString UninstallFailureContext \\$\\{LANG_${language}\\} "([^\\r\\n]+)"$`, "m"))?.[1];
    assert.ok(context?.includes("$R0") && context.includes("$INSTDIR"), "unidentified errors still show the actual code and install directory");
  }
});

test("uninstall hook keeps upgrades intact, distinguishes full cleanup, and stops on failure", async (t) => {
  const windows = process.platform === "win32";
  const compiler = [
    ...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []),
    windows ? "makensis.exe" : "makensis",
  ].find((candidate) => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    t.skip("NSIS compiler is not installed");
    return;
  }
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-uninstall-hook-")));
  const hooks = fileURLToPath(new URL("../../../src-tauri/windows/service-hooks.nsh", import.meta.url));
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const labels = template.match(/^LangString Uninstall\w+ .+$/gm)?.join("\n");
  const path = (value: string) => value.replaceAll("$", "$$");
  const compile = (file: string) => {
    const result = spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, file)], { encoding: "utf8", timeout: 30_000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  };
  try {
    await writeFile(join(root, "app.nsi"), `
Unicode true
!include FileFunc.nsh
Name "Uninstall cleanup fixture"
OutFile "${path(join(root, "app.exe"))}"
RequestExecutionLevel user
SilentInstall silent
Section
  FileOpen $0 "$EXEDIR\\calls.txt" a
  \${GetParameters} $1
  FileWrite $0 "$1$\\r$\\n"
  FileClose $0
  IfFileExists "$EXEDIR\\fail.txt" 0 +3
    SetErrorLevel 17
    Quit
  \${GetOptions} $CMDLINE "--uninstall-result " $2
  WriteINIStr $2 "cleanup" "version" "1"
  WriteINIStr $2 "cleanup" "count" "0"
  WriteINIStr $2 "cleanup" "reboot" "0"
  IfFileExists "$EXEDIR\\residue.txt" 0 done
    WriteINIStr $2 "cleanup" "count" "1"
    WriteINIStr $2 "warning0" "code" "115"
    WriteINIStr $2 "warning0" "path" "$EXEDIR\\locked-cache"
    SetErrorLevel 200
  done:
SectionEnd
`);
    compile("app.nsi");
    await writeFile(join(root, "hook.nsi"), `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
!define MAINBINARYNAME "app"
!include "${path(hooks)}"
Name "Uninstall hook test"
OutFile "${path(join(root, "hook.exe"))}"
RequestExecutionLevel user
SilentInstall silent
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\SimpChinese.nlf"
${labels}
Var UpdateMode
Var DeleteAppDataCheckboxState
Section
  StrCpy $INSTDIR "$EXEDIR"
  StrCpy $UninstallHelperPath "$EXEDIR\\app.exe"
  StrCpy $UpdateMode 0
  StrCpy $DeleteAppDataCheckboxState 0
  \${GetOptions} $CMDLINE "/TARGET" $0
  \${IfNot} \${Errors}
    StrCpy $UninstallTargetSid "S-1-5-21-1-2-3-1001"
  \${EndIf}
  \${GetOptions} $CMDLINE "/UPDATE" $0
  \${IfNot} \${Errors}
    StrCpy $UpdateMode 1
  \${EndIf}
  \${GetOptions} $CMDLINE "/PURGE" $0
  \${IfNot} \${Errors}
    StrCpy $DeleteAppDataCheckboxState 1
  \${EndIf}
  !insertmacro NSIS_HOOK_PREUNINSTALL
  FileOpen $0 "$EXEDIR\\continued.txt" w
  FileWrite $0 "continued"
  FileClose $0
  FileOpen $0 "$EXEDIR\\warnings.txt" w
  FileWrite $0 "$UninstallWarningCount"
  FileClose $0
SectionEnd
`);
    compile("hook.nsi");
    if (!windows) {
      t.diagnostic("Compiled the real NSIS hook; execution scenarios require Windows.");
      return;
    }
    const calls = join(root, "calls.txt");
    const continued = join(root, "continued.txt");
    for (const scenario of [
      { args: ["/UPDATE", "/PURGE"], expected: null, fail: false },
      { args: [], expected: "--prepare-uninstall", fail: false },
      { args: ["/PURGE"], expected: "--prepare-uninstall --delete-data", fail: false },
      { args: ["/TARGET"], expected: "--prepare-uninstall --target-user S-1-5-21-1-2-3-1001", fail: false },
      { args: ["/TARGET", "/PURGE"], expected: "--prepare-uninstall --target-user S-1-5-21-1-2-3-1001 --delete-data", fail: false },
      { args: ["/TARGET", "/PURGE"], expected: "--prepare-uninstall --target-user S-1-5-21-1-2-3-1001 --delete-data", fail: false, residue: true },
      { args: ["/PURGE"], expected: "--prepare-uninstall --delete-data", fail: true },
    ]) {
      await rm(calls, { force: true });
      await rm(continued, { force: true });
      await rm(join(root, "residue.txt"), { force: true });
      if ("residue" in scenario && scenario.residue) await writeFile(join(root, "residue.txt"), "fixture");
      if (scenario.fail) await writeFile(join(root, "fail.txt"), "fail");
      const result = spawnSync(join(root, "hook.exe"), ["/S", ...scenario.args], { timeout: 30_000 });
      assert.equal(result.error, undefined);
      assert.equal(result.status, scenario.fail ? 1 : 0);
      if (scenario.expected === null) await assert.rejects(readFile(calls), { code: "ENOENT" });
      else assert.equal((await readFile(calls, "utf8")).trim().replace(/ --uninstall-result .*$/, ""), scenario.expected);
      if (scenario.fail) await assert.rejects(readFile(continued), { code: "ENOENT" });
      else assert.equal(await readFile(continued, "utf8"), "continued");
      if ("residue" in scenario && scenario.residue) assert.equal(Number(await readFile(join(root, "warnings.txt"), "utf8")), 1);
    }
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});

test("service authorization gates uninstall initialization and does not affect updates", async t => {
  const windows = process.platform === "win32";
  const compiler = [...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []), windows ? "makensis.exe" : "makensis"]
    .find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) { t.skip("NSIS compiler is not installed"); return; }
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const init = template.match(/^Function un\.onInit\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(init);
  // Exercise the actual initialization as a regular function, simulating only
  // the external runas failure so the test never prompts for real credentials.
  const callback = init.replace("Function un.onInit", "Function RunUninstallInit")
    .replace("Call un.ValidateInstallPath", "Call ValidateInstallPath")
    .replace(/^\s*ExecShellWait "runas"[^\r\n]*/m, "      SetErrors");
  assert.notEqual(callback, init.replace("Function un.onInit", "Function RunUninstallInit"));
  const hooks = fileURLToPath(new URL("../../../src-tauri/windows/service-hooks.nsh", import.meta.url));
  const labels = template.match(/^LangString (?:Uninstall\w+|ScopeElevationFailed) .+$/gm)?.join("\n");
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-uninstall-authorization-")));
  const path = (value: string) => value.replaceAll("$", "$$");
  const compile = async (name: string, source: string) => {
    await writeFile(join(root, name), "\uFEFF" + source);
    const result = spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, name)], { encoding: "utf8", timeout: 30_000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  };
  try {
    await compile("inspect.nsi", `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
Name "Read-only uninstall inspection fixture"
OutFile "${path(join(root, "LLRemote.exe"))}"
RequestExecutionLevel user
SilentInstall silent
Section
  \${GetOptions} $CMDLINE "--uninstall-inspect " $R0
  FileOpen $0 "${path(join(root, "mode.txt"))}" r
  FileRead $0 $1
  FileClose $0
  StrCpy $2 1
  StrCpy $3 1
  \${If} $1 == "no-service"
    StrCpy $2 0
    StrCpy $3 0
  \${ElseIf} $1 == "authorized"
    StrCpy $3 0
  \${EndIf}
  WriteINIStr $R0 "uninstall" "scope" "user"
  WriteINIStr $R0 "uninstall" "sid" "S-1-5-21-1-2-3-1001"
  WriteINIStr $R0 "uninstall" "user" "Owner"
  WriteINIStr $R0 "uninstall" "elevate" "0"
  WriteINIStr $R0 "uninstall" "service" $2
  WriteINIStr $R0 "uninstall" "service_elevate" $3
SectionEnd
`);
    await compile("init.nsi", `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
!define MAINBINARYNAME "LLRemote"
!define MAINBINARYSRCPATH "${path(join(root, "LLRemote.exe"))}"
!include "${path(hooks)}"
!macro SetContext
!macroend
!macro MUI_UNGETLANGUAGE
!macroend
Name "Early uninstall authorization regression"
OutFile "${path(join(root, "init.exe"))}"
RequestExecutionLevel user
SilentInstall silent
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\SimpChinese.nlf"
${labels}
Var UpdateMode
Var PassiveMode
Var UninstallScope
Var UninstallUserLabel
Var UninstallScopeLabel
Var UninstallServiceLabel
Var UninstallResultFile
Function ValidateInstallPath
  !insertmacro LLREMOTE_VALIDATE_INSTALL_PATH
FunctionEnd
${callback}
Section
  StrCpy $INSTDIR "$EXEDIR"
  Call RunUninstallInit
  FileOpen $0 "$EXEDIR\\continued.txt" w
  FileWrite $0 "cleanup can start"
  FileClose $0
SectionEnd
`);
    if (!windows) {
      t.diagnostic("Compiled the actual initialization with cancelled runas; execution requires Windows.");
      return;
    }
    for (const scenario of [
      { mode: "authorization-cancelled", args: [], allowed: false },
      { mode: "authorization-cancelled", args: ["/UPDATE"], allowed: true },
      { mode: "no-service", args: [], allowed: true },
      { mode: "authorized", args: [], allowed: true },
    ]) {
      await writeFile(join(root, "mode.txt"), scenario.mode);
      await rm(join(root, "continued.txt"), { force: true });
      const result = spawnSync(join(root, "init.exe"), ["/S", ...scenario.args], { timeout: 30_000 });
      assert.equal(result.error, undefined);
      assert.equal(result.status, scenario.allowed ? 0 : 1);
      if (scenario.allowed) assert.equal(await readFile(join(root, "continued.txt"), "utf8"), "cleanup can start");
      else await assert.rejects(readFile(join(root, "continued.txt")), { code: "ENOENT" });
      assert.ok(await readFile(join(root, "LLRemote.exe")));
    }
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});

test("maintenance uninstall exits after success and stops on cancellation, failed launch or leftovers", async (t) => {
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const callback = template.match(/^Function PageLeaveReinstall\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(callback);
  const launch = template.match(/^Function LaunchPackagedUninstaller\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(launch);
  // Supply the selected radio button and a fixture installation directory.
  const harnessCallback = callback
    .replace("StrCpy $R1 1", "StrCpy $R1 $HarnessSelection")
    .replace("\${NSD_GetState} $R2 $R1", "StrCpy $R1 $HarnessSelection");
  const labels = template.match(/^LangString Uninstall(?:LaunchFailed|ReturnedError|MainStillExists) .+$/gm)?.join("\n");
  assert.ok(labels);
  const windows = process.platform === "win32";
  const compiler = [
    ...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []),
    windows ? "makensis.exe" : "makensis",
  ].find((candidate) => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    t.skip("NSIS compiler is not installed");
    return;
  }
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-maintenance-uninstall-")));
  const installed = join(root, "installation with spaces");
  const path = (value: string) => value.replaceAll("$", "$$");
  const compile = (file: string) => {
    const result = spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, file)], { encoding: "utf8", timeout: 30_000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  };
  try {
    await writeFile(join(root, "maintenance.nsi"), `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
Name "Maintenance uninstall regression"
OutFile "${path(join(root, "maintenance.exe"))}"
RequestExecutionLevel user
SilentInstall silent
SilentUninstall silent
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\SimpChinese.nlf"
!define MAINBINARYNAME "LLRemote"
Var UpdateMode
Var PassiveMode
Var ExistingComparison
Var HarnessSelection
Var ExistingInstallDir
Var InstallMutex
${labels}
${harnessCallback}
${launch.replace('  WriteUninstaller "$R1"', `  \${If} $R8 = 4
    StrCpy $R1 "$PLUGINSDIR\\missing\\uninstall.exe"
  \${EndIf}
  WriteUninstaller "$R1"`)}
Function LockInstaller
FunctionEnd
Section
  StrCpy $INSTDIR "$EXEDIR\\unused relocation target"
  StrCpy $ExistingInstallDir "${path(installed)}"
  StrCpy $R0 0
  StrCpy $ExistingComparison 0
  StrCpy $UpdateMode 0
  StrCpy $PassiveMode 0
  StrCpy $HarnessSelection 0
  \${GetOptions} $CMDLINE "/CASE=" $R8
  \${If} $R8 = 5
    StrCpy $HarnessSelection 1
  \${ElseIf} $R8 = 6
    StrCpy $UpdateMode 1
  \${ElseIf} $R8 = 7
    StrCpy $ExistingComparison 1
    StrCpy $HarnessSelection 1
  \${EndIf}
  Call PageLeaveReinstall
  FileOpen $0 "$EXEDIR\\continued.txt" w
  FileWrite $0 "continued"
  FileClose $0
SectionEnd
Section Uninstall
  FileOpen $0 "${path(join(root, "called.txt"))}" w
  FileWrite $0 "$INSTDIR"
  FileClose $0
  FileOpen $0 "${path(join(root, "mode.txt"))}" r
  FileRead $0 $1
  FileClose $0
  \${If} $1 = 0
    Delete "$INSTDIR\\LLRemote.exe"
  \${ElseIf} $1 = 1
    SetErrorLevel 1
    Quit
  \${ElseIf} $1 = 2
    SetErrorLevel 7
    Quit
  \${EndIf}
SectionEnd
`);
    compile("maintenance.nsi");
    if (!windows) {
      t.diagnostic("Compiled the actual maintenance callback and diagnostic strings; execution scenarios require Windows.");
      return;
    }
    await mkdir(installed);
    for (let scenario = 0; scenario < 9; scenario += 1) {
      const main = join(installed, "LLRemote.exe");
      if (scenario === 8) await rm(main, { force: true });
      else await writeFile(main, "fixture, never executed");
      await writeFile(join(root, "mode.txt"), String(scenario));
      await rm(join(root, "continued.txt"), { force: true });
      await rm(join(root, "called.txt"), { force: true });
      const result = spawnSync(join(root, "maintenance.exe"), ["/S", `/CASE=${scenario}`], { timeout: 30_000 });
      assert.equal(result.error, undefined);
      if (scenario === 0 || scenario === 8) {
        assert.equal(result.status, 0);
        await assert.rejects(readFile(main), { code: "ENOENT" });
      } else {
        assert.ok(await readFile(main));
      }
      if (scenario >= 5 && scenario <= 7) {
        assert.equal(result.status, 0);
        assert.equal(await readFile(join(root, "continued.txt"), "utf8"), "continued");
        await assert.rejects(readFile(join(root, "called.txt")), { code: "ENOENT" });
      } else {
        await assert.rejects(readFile(join(root, "continued.txt")), { code: "ENOENT" });
        if (scenario !== 0 && scenario !== 8) assert.notEqual(result.status, 0);
        if (scenario !== 4) assert.equal(await readFile(join(root, "called.txt"), "utf8"), installed);
      }
    }
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});

test("uninstaller relaunch preserves the installed directory from its temporary copy", async (t) => {
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const init = template.match(/^Function un\.onInit\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(init);
  const argumentsExpression = init.match(/ExecShell(?:Wait)? "runas" "\$EXEPATH" "([^\r\n]*)"/)?.[1];
  assert.ok(argumentsExpression, "exercise the actual elevation arguments from un.onInit");
  // NSIS requires an unquoted, final _?= parameter to carry $INSTDIR across
  // a restart of its temporary uninstaller: https://nsis.sourceforge.io/Docs/Chapter3.html#3.2.2
  assert.match(argumentsExpression, /(?:^|\s)_\?=\$INSTDIR$/, "elevation must retain the original installation directory");
  assert.match(init, /ExecShellWait "runas"/, "the caller must wait for the elevated uninstaller before checking removal");
  const windows = process.platform === "win32";
  const compiler = [
    ...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []),
    windows ? "makensis.exe" : "makensis",
  ].find((candidate) => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    t.skip("NSIS compiler is not installed; relaunch argument contract passed");
    return;
  }
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-uninstall-relaunch-")));
  const installed = join(root, "custom install with spaces");
  const path = (value: string) => value.replaceAll("$", "$$");
  try {
    await mkdir(installed);
    await writeFile(join(installed, "LLRemote.exe"), "fixture, never executed");
    await writeFile(join(root, "relaunch.nsi"), `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
Name "Uninstaller directory regression"
OutFile "${path(join(root, "seed.exe"))}"
RequestExecutionLevel user
SilentInstall silent
SilentUninstall silent
Var ReportDirectory
Var UninstallTargetSid
Section
  WriteUninstaller "${path(join(installed, "uninstall.exe"))}"
SectionEnd
Function un.onInit
  StrCpy $ReportDirectory "${path(root)}"
  StrCpy $UninstallTargetSid "S-1-5-21-1-2-3-1001"
  \${GetOptions} $CMDLINE "/RESUMED" $0
  \${If} \${Errors}
    \${GetParameters} $1
    StrCpy $1 "$1 /RESUMED"
    ; Execute the real parameter expression without prompting for UAC in a test.
    ; The initial uninstaller still relocates itself to Temp just like production.
    ExecShellWait "open" "$EXEPATH" "${argumentsExpression}"
    \${If} \${Errors}
      SetErrorLevel 17
    \${EndIf}
    Quit
  \${EndIf}
FunctionEnd
Section Uninstall
  IfFileExists "$INSTDIR\\LLRemote.exe" 0 missing_main
    StrCpy $2 "found main"
    Goto report
  missing_main:
    StrCpy $2 "missing main"
    SetErrorLevel 17
  report:
  FileOpen $0 "$ReportDirectory\\result.pending" w
  FileWrite $0 "$INSTDIR$\\r$\\n$2$\\r$\\n$CMDLINE"
  FileClose $0
  Rename "$ReportDirectory\\result.pending" "$ReportDirectory\\result.txt"
SectionEnd
`);
    const compiled = spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, "relaunch.nsi")], { encoding: "utf8", timeout: 30_000 });
    assert.equal(compiled.status, 0, `${compiled.stdout}\n${compiled.stderr}`);
    if (!windows) {
      t.diagnostic("Validated the elevation argument contract and compiled a real self-copying uninstaller; process execution requires Windows.");
      return;
    }
    const seed = spawnSync(join(root, "seed.exe"), ["/S"], { timeout: 30_000 });
    assert.equal(seed.error, undefined);
    assert.equal(seed.status, 0);
    for (const args of [["/S", "/P"], ["/S", "/P", "/UPDATE"]]) {
      const report = join(root, "result.txt");
      await rm(report, { force: true });
      const result = spawnSync(join(installed, "uninstall.exe"), args, { timeout: 30_000 });
      assert.equal(result.error, undefined);
      let output = "";
      const deadline = Date.now() + 30_000;
      while (Date.now() < deadline) {
        try { output = await readFile(report, "utf8"); break; }
        catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
          await new Promise((resolve) => setTimeout(resolve, 50));
        }
      }
      assert.ok(output, "the resumed uninstaller must finish");
      const [directory, main, command] = output.split(/\r?\n/);
      assert.equal(directory, installed);
      assert.equal(main, "found main");
      for (const flag of args) assert.ok(command.includes(flag), `lost relaunch option ${flag}`);
      assert.ok(command.includes("/TARGETUSER=S-1-5-21-1-2-3-1001"), "lost personal cleanup identity across elevation");
    }
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});

test("scoped uninstaller preserves recovery and registration until file deletion succeeds", async t => {
  const windows = process.platform === "win32";
  const compiler = [...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []), windows ? "makensis.exe" : "makensis"]
    .find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) { t.skip("NSIS compiler is not installed"); return; }
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const callbacks = ["un.onInit", "un.ValidateInstallPath", "un.ConfirmShow", "un.ConfirmLeave", "un.SkipIfPassive", "un.RecordCompletion"]
    .map(name => template.match(new RegExp(`^Function ${name.replaceAll(".", "\\.")}(?: .*)?\\r?\\n[\\s\\S]*?^FunctionEnd`, "m"))?.[0]);
  assert.ok(callbacks.every(Boolean));
  const labels = template.match(/^LangString (?:CompleteUninstallLabel|Uninstall\w+) .+$/gm)?.join("\n");
  let section = template.match(/^Section Uninstall\r?\n[\s\S]*?^SectionEnd/m)?.[0];
  assert.ok(section);
  section = section.replace(/{{#each resources}}([\s\S]*?){{\/each}}/, (_, body: string) => body.replaceAll("{{this.[1]}}", "resource.txt"))
    .replace(/{{#each [^}]+}}[\s\S]*?{{\/each}}/g, "").replaceAll("{{/each}}", "");
  assert.ok(!section.includes("{{"));
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-scoped-uninstall-")));
  const installed = join(root, "installed with spaces");
  const path = (value: string) => value.replaceAll("$", "$$");
  const hooks = fileURLToPath(new URL("../../../src-tauri/windows/service-hooks.nsh", import.meta.url));
  const compile = async (name: string, source: string) => {
    await writeFile(join(root, name), "\uFEFF" + source);
    const result = spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, name)], { encoding: "utf8", timeout: 30_000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  };
  try {
    await compile("helper.nsi", `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
Name "Scoped cleanup fixture"
OutFile "${path(join(root, "helper.exe"))}"
RequestExecutionLevel user
SilentInstall silent
Section
  \${GetParameters} $1
  FileOpen $0 "${path(join(installed, "calls.txt"))}" a
  FileSeek $0 0 END
  FileWrite $0 "$1$\\r$\\n"
  FileClose $0
  ClearErrors
  \${GetOptions} $CMDLINE "--uninstall-inspect " $3
  \${IfNot} \${Errors}
    WriteINIStr $3 "uninstall" "scope" "user"
    WriteINIStr $3 "uninstall" "sid" "S-1-5-21-1-2-3-1001"
    WriteINIStr $3 "uninstall" "user" "Alice"
    WriteINIStr $3 "uninstall" "elevate" "0"
    WriteINIStr $3 "uninstall" "service" "0"
    SetErrorLevel 0
    Quit
  \${EndIf}
  \${GetOptions} $CMDLINE "--prepare-uninstall" $3
  \${IfNot} \${Errors}
    IfFileExists "${path(join(installed, "fail.txt"))}" 0 +3
      SetErrorLevel 17
      Quit
  \${EndIf}
  \${GetOptions} $CMDLINE "--uninstall-result " $3
  WriteINIStr $3 "cleanup" "version" "1"
  WriteINIStr $3 "cleanup" "count" "0"
  WriteINIStr $3 "cleanup" "reboot" "0"
  IfFileExists "${path(join(installed, "residue.txt"))}" 0 cleanup_done
    WriteINIStr $3 "cleanup" "count" "1"
    WriteINIStr $3 "warning0" "code" "115"
    WriteINIStr $3 "warning0" "path" "$EXEDIR\\locked-cache"
    SetErrorLevel 200
  cleanup_done:
SectionEnd
`);
    await writeFile(join(root, "resource.txt"), "owned resource");
    await compile("uninstaller.nsi", `
Unicode true
!include MUI2.nsh
!include FileFunc.nsh
!include nsDialogs.nsh
!define MAINBINARYNAME "LLRemote"
!define MAINBINARYSRCPATH "${path(join(root, "helper.exe"))}"
!define PRODUCTNAME "LL Remote"
!define MANUPRODUCTKEY "Software\\Fixture"
!define MANUKEY "Software\\Fixture"
!define UNINSTKEY "Software\\Fixture"
!include "${path(hooks)}"
!macro SetContext
!macroend
!macro DeleteAppUserModelId
!macroend
!macro IsShortcutTarget shortcut target
  Push 0
!macroend
!macro UnpinShortcut shortcut
!macroend
Name "Scoped uninstall regression"
OutFile "${path(join(root, "seed.exe"))}"
InstallDir "${path(installed)}"
RequestExecutionLevel user
SilentInstall silent
SilentUninstall silent
Var PassiveMode
Var UpdateMode
Var UninstallScope
Var UninstallUserLabel
Var UninstallScopeLabel
Var UninstallServiceLabel
Var UninstallResultFile
Var DeleteAppDataCheckbox
Var DeleteAppDataCheckboxState
Var AppStartMenuFolder
UninstPage custom un.ConfirmShow un.ConfirmLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
${labels}
${callbacks.join("\n")}
Section
  \${GetOptions} $CMDLINE "/UNINSTALL" $0
  \${IfNot} \${Errors}
    StrCpy $0 -1
    \${GetOptions} $CMDLINE "/LOCK-RESOURCE" $2
    \${IfNot} \${Errors}
      ; Open with FILE_SHARE_READ | FILE_SHARE_WRITE, excluding FILE_SHARE_DELETE.
      System::Call 'kernel32::CreateFileW(w "$INSTDIR\\resource.txt", i 0x80000000, i3, p0, i3, i0x80, p0) p.r0'
      \${If} $0 == -1
        SetErrorLevel 91
        Quit
      \${EndIf}
    \${EndIf}
    ClearErrors
    ; _?= keeps NSIS in this process so ExecWait gets its actual exit code.
    ExecWait '"$EXEDIR\\run-uninstall.exe" /S _?=$INSTDIR' $1
    \${If} \${Errors}
      StrCpy $1 92
    \${EndIf}
    \${If} $0 != -1
      System::Call 'kernel32::CloseHandle(p r0)'
    \${EndIf}
    SetErrorLevel $1
    Quit
  \${EndIf}
  ClearErrors
  SetOutPath "$INSTDIR"
  File /oname=LLRemote.exe "${path(join(root, "helper.exe"))}"
  File /oname=resource.txt "${path(join(root, "resource.txt"))}"
  WriteUninstaller "$INSTDIR\\uninstall.exe"
  WriteUninstaller "$EXEDIR\\run-uninstall.exe"
SectionEnd
${section}
Function un.onUninstSuccess
  !insertmacro LLREMOTE_UNINSTALL_COMPLETION
  Call un.RecordCompletion
  FileOpen $0 "${path(join(root, "warnings.txt"))}" w
  FileWrite $0 "$UninstallWarningCount"
  FileClose $0
  FileOpen $0 "${path(join(root, "result.txt"))}" w
  FileWrite $0 "success"
  FileClose $0
FunctionEnd
`);
    if (!windows) {
      t.diagnostic("Compiled the real ownership initialization, confirmation page, scoped hooks and complete removal section; execution requires Windows.");
      return;
    }
    for (const scenario of ["cleanup-failed", "resource-in-use", "data-residue", "success", "missing-app"]) {
      await rm(installed, { recursive: true, force: true });
      await rm(join(root, "result.txt"), { force: true });
      const seed = spawnSync(join(root, "seed.exe"), ["/S"], { timeout: 30_000 });
      assert.equal(seed.status, 0);
      if (scenario === "cleanup-failed") await writeFile(join(installed, "fail.txt"), "fixture");
      if (scenario === "data-residue") await writeFile(join(installed, "residue.txt"), "fixture");
      if (scenario === "missing-app") {
        await rm(join(installed, "LLRemote.exe"));
        await rm(join(installed, "resource.txt"));
      }
      const args = ["/S", "/UNINSTALL"];
      if (scenario === "resource-in-use") args.push("/LOCK-RESOURCE");
      const result = spawnSync(join(root, "seed.exe"), args, { timeout: 30_000 });
      assert.ifError(result.error);
      const completed = scenario === "success" || scenario === "data-residue" || scenario === "missing-app";
      const calls = await readFile(join(installed, "calls.txt"), "utf8").catch(error => {
        if (error.code !== "ENOENT") throw error;
        return "";
      });
      assert.equal(result.status, completed ? 0 : 1, `${scenario}: ${calls}`);
      assert.match(calls, /--prepare-uninstall --target-user S-1-5-21-1-2-3-1001/, scenario);
      assert.ok(calls.includes(`--uninstall-directory "${installed}"`));
      if (completed) {
        assert.equal(await readFile(join(root, "result.txt"), "utf8"), "success", scenario);
        assert.match(calls, /--finish-uninstall --target-user S-1-5-21-1-2-3-1001/);
        await assert.rejects(readFile(join(installed, "LLRemote.exe")), { code: "ENOENT" });
        if (scenario === "data-residue") {
          assert.ok(Number(await readFile(join(root, "warnings.txt"), "utf8")) > 0);
          assert.ok(await readFile(join(installed, "residue.txt")));
        }
      } else {
        assert.doesNotMatch(calls, /--finish-uninstall/);
        assert.ok(await readFile(join(installed, "resource.txt")));
        assert.ok(await readFile(join(installed, "LLRemote.exe")));
        assert.ok(await readFile(join(installed, "uninstall.exe")));
      }
    }
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});

test("elevation reports a failure after main-file removal unless the child completed", async t => {
  const windows = process.platform === "win32";
  const compiler = [...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []), windows ? "makensis.exe" : "makensis"]
    .find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) { t.skip("NSIS compiler is not installed"); return; }
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const init = template.match(/^Function un\.onInit\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  const receipt = template.match(/^Function un\.RecordCompletion\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(init && receipt);
  const wait = init.match(/  \$\{If\} \$0 == "1"\r?\n[\s\S]*?^  \$\{EndIf\}/m)?.[0];
  assert.ok(wait);
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-uninstall-result-")));
  const installed = join(root, "installed with spaces");
  const path = (value: string) => value.replaceAll("$", "$$");
  const compile = async (name: string, source: string) => {
    await writeFile(join(root, name), "\uFEFF" + source);
    const result = spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, name)], { encoding: "utf8", timeout: 30_000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  };
  try {
    await mkdir(installed);
    await compile("child.nsi", `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
Name "Completion receipt fixture"
OutFile "${path(join(root, "child-seed.exe"))}"
RequestExecutionLevel user
SilentInstall silent
SilentUninstall silent
Var UninstallResultFile
${receipt}
Section
  WriteUninstaller "$EXEDIR\\child.exe"
SectionEnd
Section Uninstall
  \${GetOptions} $CMDLINE "/RESULTFILE=" $UninstallResultFile
  Delete "$INSTDIR\\LLRemote.exe"
  \${GetOptions} $CMDLINE "/CASE=" $4
  \${If} $4 == "failed-after-main"
    SetErrorLevel 17
    Quit
  \${EndIf}
  \${If} $4 == "restart-required"
    SetRebootFlag true
  \${EndIf}
  Call un.RecordCompletion
SectionEnd
`);
    await compile("parent.nsi", `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
Name "Elevated uninstall result regression"
OutFile "${path(join(root, "parent.exe"))}"
RequestExecutionLevel user
SilentInstall silent
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LangString ScopeElevationFailed \${LANG_ENGLISH} "Elevation failed"
LangString UninstallServiceAuthorizationRequired \${LANG_ENGLISH} "Service authorization required"
Var UninstallTargetSid
Section
  InitPluginsDir
  StrCpy $INSTDIR "${path(installed)}"
  StrCpy $UninstallTargetSid "S-1-5-21-1-2-3-1001"
  StrCpy $0 1
  ${wait.replace('ExecShellWait "runas" "$EXEPATH"', 'ExecShellWait "open" "$EXEDIR\\child.exe"')}
SectionEnd
`);
    if (!windows) {
      t.diagnostic("Compiled the production elevated-parent result check and atomic completion receipt; execution requires Windows.");
      return;
    }
    const seed = spawnSync(join(root, "child-seed.exe"), ["/S"], { timeout: 30_000 });
    assert.ifError(seed.error);
    assert.equal(seed.status, 0);
    for (const scenario of ["success", "failed-after-main", "restart-required"]) {
      await writeFile(join(installed, "LLRemote.exe"), "fixture");
      const result = spawnSync(join(root, "parent.exe"), ["/S", `/CASE=${scenario}`], { timeout: 30_000 });
      assert.equal(result.error, undefined);
      assert.equal(result.status, scenario === "success" ? 0 : scenario === "restart-required" ? 3010 : 1, scenario);
      await assert.rejects(readFile(join(installed, "LLRemote.exe")), { code: "ENOENT" });
    }
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});
