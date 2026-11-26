import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, realpath, rm, writeFile, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

// Exercise the actual helper launcher, PE guard, file snapshots and installation
// ordering. The fixture replaces the application's SCM/IPC actions, not NSIS's
// CreateProcess, wait/exit handling or filesystem operations.
test("installer preflight, file rollback and service failures use the actual NSIS lifecycle", async t => {
  const windows = process.platform === "win32";
  const compiler = [
    ...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []),
    windows ? "makensis.exe" : "makensis",
  ].find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    assert.notEqual(process.env.LLREMOTE_REQUIRE_NSIS, "1", "NSIS execution is required for Windows candidate verification");
    t.skip("NSIS compiler is not installed"); return;
  }
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const section = template.match(/^Section Install\r?\n([\s\S]*?)^SectionEnd/m)?.[1];
  assert.ok(section);
  const functions = ["BackupInstallerPayload", "ApplyInstallerPayload", "RestoreInstallerPayload"]
    .map(name => template.match(new RegExp(`^Function ${name}\\r?\\n[\\s\\S]*?^FunctionEnd`, "m"))?.[0]).join("\n");
  const strings = template.match(/^LangString InstallInterruptsSharing .+$/gm)?.join("\n");
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-installer-")));
  const path = (value: string) => value.replaceAll("$", "$$");
  const stripLoops = (value: string) => value.replace(/{{#each [^}]+}}[\s\S]*?{{\/each}}/g, "").replaceAll("{{/each}}", "");
  const compile = async (file: string, source: string) => {
    await writeFile(join(root, file), "\uFEFF" + source);
    const result = spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, file)], { encoding: "utf8", timeout: 30_000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  };
  try {
    await compile("new.nsi", `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
Name "Installer maintenance fixture"
OutFile "${path(join(root, "new.exe"))}"
SilentInstall silent
RequestExecutionLevel user
Section
  FileOpen $0 "${path(join(root, "scenario.txt"))}" r
  FileRead $0 $2
  FileClose $0
  \${GetParameters} $1
  FileOpen $0 "${path(join(root, "calls.txt"))}" a
  FileSeek $0 0 END
  FileWrite $0 "$1$\\r$\\n"
  FileClose $0
  StrCpy $3 0
  \${GetOptions} $CMDLINE "--installer-state" $0
  \${IfNot} \${Errors}
    \${If} $2 == 1
    \${OrIf} $2 == 9
    \${OrIf} $2 == 11
      StrCpy $3 10
    \${ElseIf} $2 == 3
    \${OrIf} $2 == 4
    \${OrIf} $2 == 6
      StrCpy $3 11
    \${ElseIf} $2 == 8
      StrCpy $3 12
    \${EndIf}
  \${EndIf}
  \${GetOptions} $CMDLINE "--prepare-update" $0
  \${IfNot} \${Errors}
    \${If} $2 == 4
      StrCpy $3 85
    \${EndIf}
  \${EndIf}
  \${GetOptions} $CMDLINE "--installer-verify" $0
  \${IfNot} \${Errors}
    \${If} $2 == 5
      StrCpy $3 83
    \${EndIf}
  \${EndIf}
  \${GetOptions} $CMDLINE "--installer-service" $0
  \${IfNot} \${Errors}
    \${If} $2 == 6
      StrCpy $3 93
    \${EndIf}
  \${EndIf}
  \${GetOptions} $CMDLINE "--installer-owner-user" $0
  \${IfNot} \${Errors}
    \${If} $2 == 10
      StrCpy $3 95
    \${EndIf}
  \${EndIf}
  \${GetOptions} $CMDLINE "--installer-report " $4
  StrCpy $5 1
  \${If} $2 == 7
    StrCpy $5 0
  \${ElseIf} $2 == 9
  \${AndIf} $3 == 10
    StrCpy $5 0
  \${EndIf}
  \${If} $5 == 1
    WriteINIStr "$4" "maintenance" "protocol" "1"
    WriteINIStr "$4" "maintenance" "result" "$3"
    WriteINIStr "$4" "maintenance" "detail" "fixture"
  \${EndIf}
  SetErrorLevel $3
SectionEnd
`);
    // Metadata/shortcut publication is omitted so execution never touches a
    // real installation or registry. GUI enumeration is also a controlled seam.
    const body = stripLoops(section)
      .replace(/  ; Publish installation metadata[\s\S]*?  ; Auto close this page/, '  FileOpen $0 "$EXEDIR\\registered.txt" w\n  FileWrite $0 "registered"\n  FileClose $0\n  Delete "$INSTDIR\\.llremote-install-recovery.ini"\n  RMDir /r "$InstallBackup"\n  ; Auto close this page')
      .replace(/nsis_tauri_utils::(?:FindProcessCurrentUser|FindProcess) "[^"\r\n]+"/g, "Push 1")
      .replace(/nsis_tauri_utils::(?:KillProcessCurrentUser|KillProcess) "[^"\r\n]+"/g, "Push 0")
      .replace(/    WriteRegStr SHCTX [^\r\n]+/g, "");
    const hooks = fileURLToPath(new URL("../../../src-tauri/windows/service-hooks.nsh", import.meta.url));
    const lifecycleSource = `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
!define MAINBINARYNAME "app"
!define MAINBINARYSRCPATH "${path(join(root, "new.exe"))}"
!define VERSION "0.12.0"
!define BUNDLEID "com.llremote.installer-test"
!define ARCH "x86"
!include "${path(hooks)}"
Name "Actual installer lifecycle"
OutFile "${path(join(root, "installer.exe"))}"
RequestExecutionLevel user
SilentInstall silent
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\SimpChinese.nlf"
${strings}
Var MultiUser.InstallMode
Var PassiveMode
Var UpgradeHadService
Var ExistingInstall
Var ExistingInstallDir
!insertmacro LLREMOTE_INSTALLER_FUNCTIONS
Function ValidateScopeConflict
FunctionEnd
Function DetectExistingInstallation
  StrCpy $ExistingInstall 1
  StrCpy $ExistingInstallDir "$INSTDIR"
FunctionEnd
Function ValidateMachineDirectory
FunctionEnd
${stripLoops(functions)}
Function .onInit
  ; Native policy is tested separately; this maintenance fixture is always x86.
  StrCpy $InstallLog "$EXEDIR\\installation.log"
  StrCpy $INSTDIR "$EXEDIR\\installed"
  SetShellVarContext current
  StrCpy $MultiUser.InstallMode "CurrentUser"
  \${GetOptions} $CMDLINE "/CASE=" $0
  \${If} $0 == 2
    SetShellVarContext all
    StrCpy $MultiUser.InstallMode "AllUsers"
  \${EndIf}
  Call ProbeInstallerPayload
FunctionEnd
Section
${body}
SectionEnd
Section Uninstall
SectionEnd
`;
    await compile("installer.nsi", lifecycleSource);
    // Also compile the real metadata publication path. This binary is never
    // executed, so registry/shortcut writes cannot affect the test machine.
    const metadataBody = stripLoops(section)
      .replace(/nsis_tauri_utils::(?:FindProcessCurrentUser|FindProcess) "[^"\r\n]+"/g, "Push 1")
      .replace(/nsis_tauri_utils::(?:KillProcessCurrentUser|KillProcess) "[^"\r\n]+"/g, "Push 0")
      .replace(/^.*!insertmacro MUI_STARTMENU_WRITE_(?:BEGIN|END).*$/gm, "");
    const metadataSource = lifecycleSource.replace(body, metadataBody)
      .replace('OutFile "' + path(join(root, "installer.exe")) + '"', 'OutFile "' + path(join(root, "metadata.exe")) + '"')
      .replace('!define ARCH "x86"', `!define ARCH "x86"
!define PRODUCTNAME "LL Remote regression"
!define INSTALLMODE "both"
!define UNINSTKEY "Software\\LLRemoteInstallerRegression"
!define MANUPRODUCTKEY "Software\\LLRemoteInstallerRegressionProduct"
!define MANUFACTURER "regression"
!define HOMEPAGE ""
!define ESTIMATEDSIZE 1
Var UpdateMode
Var AppStartMenuFolder`)
      .replace("Function ValidateScopeConflict", `Function CreateOrUpdateStartMenuShortcut
FunctionEnd
Function CreateOrUpdateDesktopShortcut
FunctionEnd
Function ValidateScopeConflict`);
    await compile("metadata.nsi", metadataSource);
    for (const architecture of ["x64", "arm64"]) {
      await compile(`metadata-${architecture}.nsi`, metadataSource
        .replace('!define ARCH "x86"', `!define ARCH "${architecture}"`));
    }

    if (!windows) {
      t.diagnostic("Compiled the real compatibility/PE checks, CreateProcess launcher, rollback and service ordering. Runtime scenarios require Windows.");
      return;
    }
    for (let scenario = 0; scenario < 12; scenario++) {
      await rm(join(root, "calls.txt"), { force: true });
      await rm(join(root, "registered.txt"), { force: true });
      await rm(join(root, "installation.log"), { force: true });
      await rm(join(root, "installed"), { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
      await mkdir(join(root, "installed"));
      if (scenario !== 11) {
        await writeFile(join(root, "installed/app.exe"), "previous app");
        await writeFile(join(root, "installed/uninstall.exe"), "previous uninstaller");
      }
      await writeFile(join(root, "installed/personal.txt"), "personal data");
      await writeFile(join(root, "scenario.txt"), String(scenario));
      const result = spawnSync(join(root, "installer.exe"), ["/S", `/CASE=${scenario}`], { timeout: 60_000 });
      assert.equal(result.error, undefined, `scenario ${scenario}`);
      const calls = (await readFile(join(root, "calls.txt"), "utf8")).trim().split(/\r?\n/).map(line => line.split(" --installer-report")[0]);
      assert.equal(result.status, [4, 5, 6, 7, 9, 10].includes(scenario) ? 1 : 0, `scenario ${scenario}: ${calls}`);
      assert.equal(calls[0], "--installer-check");
      if ([1, 3, 4, 6, 8, 11].includes(scenario)) assert.ok(calls.includes("--prepare-update"));
      else assert.ok(!calls.includes("--prepare-update"));
      if ([1, 2, 3, 6, 11].includes(scenario)) {
        assert.ok(calls.indexOf("--installer-service") > calls.indexOf("--installer-verify"));
        const owner = calls.findIndex(call => call === "--installer-owner-user" || call === "--installer-owner-all");
        assert.ok(owner > calls.lastIndexOf("--installer-verify"));
        assert.ok(calls.indexOf("--installer-service") > owner);
      } else assert.ok(!calls.includes("--installer-service"));
      if ([4, 5, 7, 9].includes(scenario)) {
        assert.equal(await readFile(join(root, "installed/app.exe"), "utf8"), "previous app");
        assert.equal(await readFile(join(root, "installed/uninstall.exe"), "utf8"), "previous uninstaller");
      }
      if ([6, 10].includes(scenario)) assert.ok(await readFile(join(root, "installed/.llremote-install-recovery.ini"), "utf8"));
      if (scenario === 10) assert.ok(calls.includes("--installer-owner-user"));
      if (scenario === 11) {
        assert.deepEqual(await readFile(join(root, "installed/app.exe")), await readFile(join(root, "new.exe")));
        assert.ok(await readFile(join(root, "installed/uninstall.exe")));
        assert.equal(await readFile(join(root, "installed/personal.txt"), "utf8"), "personal data");
        const log = await readFile(join(root, "installation.log"), "utf8");
        assert.match(log, /repair: restore_missing_file: app\.exe/);
        assert.match(log, /repair: restore_missing_file: uninstall\.exe/);
      }
      if ([4, 5, 6, 7, 9, 10].includes(scenario)) await assert.rejects(readFile(join(root, "registered.txt")), { code: "ENOENT" });
      else assert.equal(await readFile(join(root, "registered.txt"), "utf8"), "registered");
    }
  } finally { await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }); }
});
