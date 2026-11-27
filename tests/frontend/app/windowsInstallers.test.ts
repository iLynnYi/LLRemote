import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, readdir, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("Windows packaging publishes one installer and removes failed builds", async () => {
  const source = fileURLToPath(new URL("../../../scripts/windows-installers.mjs", import.meta.url));
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-one-installer-")));
  const target = "x86_64-pc-windows-msvc";
  const output = join(root, "src-tauri/target", target, "release/bundle/nsis");
  try {
    await mkdir(join(root, "scripts"), { recursive: true });
    await mkdir(join(root, "src-tauri/windows"), { recursive: true });
    for (const name of ["installer.nsi", "service-hooks.nsh", "install-safety.nsh", "install-transaction.nsh"]) {
      await writeFile(join(root, "src-tauri/windows", name), "; fixture");
    }
    await copyFile(source, join(root, "scripts/windows-installers.mjs"));
    await writeFile(join(root, "package.json"), '{"type":"module"}');
    await writeFile(join(root, "src-tauri/tauri.conf.json"), '{"productName":"LL Remote","version":"0.12.0"}');
    await writeFile(join(root, "scripts/tauri.mjs"), `
      import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
      import { join } from 'node:path';
      import { fileURLToPath } from 'node:url';
      const root = fileURLToPath(new URL('../', import.meta.url));
      appendFileSync(join(root, 'tauri-invocations.txt'), process.argv.slice(2).join(' ') + String.fromCharCode(10));
      if (process.env.MOCK_TAURI_FAIL === '1') process.exit(1);
      const target = process.argv[process.argv.indexOf('--target') + 1];
      const output = join(root, 'src-tauri/target', target, 'release/bundle/nsis');
      mkdirSync(output, { recursive: true });
      writeFileSync(join(output, 'LL Remote_0.12.0_x64-setup.exe'), 'installer');
    `);
    const run = (fail: boolean) => spawnSync(process.execPath, [join(root, "scripts/windows-installers.mjs"), target], {
      cwd: root,
      env: { ...process.env, MOCK_TAURI_FAIL: fail ? "1" : "0" },
      encoding: "utf8",
    });

    const success = run(false);
    assert.equal(success.status, 0, success.stderr);
    assert.deepEqual(await readdir(output), ["LL Remote_0.12.0_x64-setup.exe"]);
    assert.equal(await readFile(join(output, "LL Remote_0.12.0_x64-setup.exe"), "utf8"), "installer");
    assert.equal((await readFile(join(root, "tauri-invocations.txt"), "utf8")).trim().split("\n").length, 1);

    const failed = run(true);
    assert.equal(failed.status, 1);
    await assert.rejects(readdir(output), { code: "ENOENT" });
    assert.equal((await readFile(join(root, "tauri-invocations.txt"), "utf8")).trim().split("\n").length, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("incomplete Windows source mirrors fail before invoking the Rust/Tauri build", async () => {
  const source = fileURLToPath(new URL("../../../scripts/windows-installers.mjs", import.meta.url));
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-incomplete-installer-")));
  const target = "x86_64-pc-windows-msvc";
  try {
    await mkdir(join(root, "scripts"), { recursive: true });
    await mkdir(join(root, "src-tauri/windows"), { recursive: true });
    await copyFile(source, join(root, "scripts/windows-installers.mjs"));
    await writeFile(join(root, "src-tauri/tauri.conf.json"), '{"productName":"LL Remote","version":"0.12.0"}');
    await writeFile(join(root, "src-tauri/windows/installer.nsi"), "; fixture");
    await writeFile(join(root, "src-tauri/windows/service-hooks.nsh"), "; fixture");
    await writeFile(join(root, "scripts/tauri.mjs"), `
      import { mkdirSync, writeFileSync } from 'node:fs';
      import { join } from 'node:path';
      writeFileSync('build-was-started.txt', 'started');
      const directory = join('src-tauri/target', '${target}', 'release/bundle/nsis');
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, 'LL Remote_0.12.0_x64-setup.exe'), 'fixture');
    `);
    const run = () => spawnSync(process.execPath, [join(root, "scripts/windows-installers.mjs"), target], { cwd: root, encoding: "utf8" });
    const missingBoth = run();
    assert.equal(missingBoth.status, 1, "missing NSIS includes must stop the build immediately");
    assert.match(missingBoth.stderr, /install-safety\.nsh/);
    assert.match(missingBoth.stderr, /install-transaction\.nsh/);
    await assert.rejects(readFile(join(root, "build-was-started.txt")), { code: "ENOENT" });
    await writeFile(join(root, "src-tauri/windows/install-safety.nsh"), "; fixture");
    const missingTransaction = run();
    assert.equal(missingTransaction.status, 1);
    assert.match(missingTransaction.stderr, /install-transaction\.nsh/);
    await assert.rejects(readFile(join(root, "build-was-started.txt")), { code: "ENOENT" });
    await writeFile(join(root, "src-tauri/windows/install-transaction.nsh"), "");
    assert.equal(run().status, 1, "an empty helper is also an incomplete source mirror");
    await writeFile(join(root, "src-tauri/windows/install-transaction.nsh"), "; fixture");
    const complete = run();
    assert.equal(complete.status, 0, complete.stderr);
    assert.equal(await readFile(join(root, "build-was-started.txt"), "utf8"), "started");
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("Windows directory validation accepts new destinations and rejects paths outside Program Files", {
  skip: process.platform !== "win32",
}, async (t) => {
  const compiler = [
    join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe"),
    "makensis.exe",
  ].find((candidate) => spawnSync(candidate, ["/VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    t.skip("NSIS compiler is not installed; run after Windows packaging has provisioned NSIS");
    return;
  }
  const template = await readFile(new URL("../../../src-tauri/windows/installer.nsi", import.meta.url), "utf8");
  const validation = template.match(/^Function ValidateMachineDirectory\r?\n[\s\S]*?^FunctionEnd/m)?.[0];
  assert.ok(validation, "the harness must exercise the installer's actual directory callback");
  const root = await realpath(await mkdtemp(join(tmpdir(), "llremote-installer-directory-")));
  const nsisPath = (path: string) => path.replaceAll("$", "$$");
  try {
    await writeFile(join(root, "directory.nsi"), `
Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
!include x64.nsh
!define ARCH "x64"
Name "LL Remote directory validation test"
OutFile "${nsisPath(join(root, "directory.exe"))}"
RequestExecutionLevel user
SilentInstall silent
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LangString ScopeMachineDirectory \${LANG_ENGLISH} "Choose a directory under Program Files."
Var MultiUser.InstallMode
${validation}
Function .onInit
  StrCpy $MultiUser.InstallMode "AllUsers"
  StrCpy $INSTDIR "$PROGRAMFILES64\\LLRemote-directory-test-${Date.now()}"
  \${GetOptions} $CMDLINE "/CASE=" $R8
  \${Switch} $R8
    \${Case} 1
      StrCpy $INSTDIR "$COMMONFILES64"
      \${Break}
    \${Case} 2
      StrCpy $INSTDIR "$INSTDIR\\nested\\app"
      \${Break}
    \${Case} 3
      StrCpy $INSTDIR "$TEMP\\LLRemote-directory-test"
      \${Break}
    \${Case} 4
      StrCpy $INSTDIR "$PROGRAMFILES64-other\\LLRemote-directory-test"
      \${Break}
    \${Case} 5
      StrCpy $INSTDIR "$PROGRAMFILES64\\..\\LLRemote-directory-test"
      \${Break}
    \${Case} 6
      StrCpy $INSTDIR "$PROGRAMFILES64"
      \${Break}
    \${Case} 7
      StrCpy $MultiUser.InstallMode "CurrentUser"
      StrCpy $INSTDIR "$TEMP\\LLRemote-directory-test"
      \${Break}
  \${EndSwitch}
  ; A previous failed registry lookup must not invalidate a valid destination.
  SetErrors
  Call ValidateMachineDirectory
  FileOpen $R7 "$EXEDIR\\accepted.txt" w
  FileWrite $R7 "$INSTDIR"
  FileClose $R7
  Quit
FunctionEnd
Section
SectionEnd
`);
    const compiled = spawnSync(compiler, ["/V2", join(root, "directory.nsi")], { encoding: "utf8", timeout: 30_000 });
    assert.equal(compiled.status, 0, `${compiled.stdout}\n${compiled.stderr}`);
    for (let scenario = 0; scenario < 8; scenario += 1) {
      const accepted = join(root, "accepted.txt");
      await rm(accepted, { force: true });
      const result = spawnSync(join(root, "directory.exe"), ["/S", `/CASE=${scenario}`], { timeout: 30_000 });
      assert.equal(result.error, undefined, `directory scenario ${scenario}: ${result.error?.message}`);
      if ([0, 1, 2, 7].includes(scenario)) {
        const destination = await readFile(accepted, "utf8").catch(() => "");
        assert.ok(destination, `valid destination ${scenario} was silently rejected`);
        assert.ok(!destination.endsWith("\\"), "normalization must preserve the directory without a trailing separator");
      } else {
        await assert.rejects(readFile(accepted), { code: "ENOENT" }, `unsafe destination ${scenario} was accepted`);
      }
    }
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});

test("sibling hook includes split into valid filenames with Windows NSIS path rules", async () => {
  const hook = await readFile(new URL("../../../src-tauri/windows/service-hooks.nsh", import.meta.url), "utf8");
  const includes = [...hook.matchAll(/^!include\s+"([^"\r\n]+)"/gm)].map(match => match[1]);
  assert.equal(includes.length, 2);
  // NSIS Source/util.cpp get_dir_name/get_file_name split only on the host's
  // PLATFORM_PATH_SEPARATOR_STR. Node's win32 path functions accept '/' too,
  // so using win32.basename here would hide the native compiler bug again.
  for (const directory of [String.raw`C:\Users\pro\Desktop\VNC\src-tauri\windows`, String.raw`C:\LL Remote 中文目录\src-tauri\windows`]) {
    const expected = ["install-safety.nsh", "install-transaction.nsh"];
    for (const [index, include] of includes.entries()) {
      const expanded = include.replaceAll("\${__FILEDIR__}", directory);
      const separator = expanded.lastIndexOf("\\");
      assert.equal(expanded.slice(separator + 1), expected[index], "Windows NSIS must see a bare filename, not windows/install-*.nsh");
      assert.equal(expanded.slice(0, separator), directory);
    }
  }
});

test("NSIS hook includes resolve from the hook directory after a complete source sync", async t => {
  const windows = process.platform === "win32";
  const compiler = [
    ...(windows ? [join(process.env.LOCALAPPDATA || "", "tauri", "NSIS", "makensis.exe")] : []),
    windows ? "makensis.exe" : "makensis",
  ].find(candidate => spawnSync(candidate, [windows ? "/VERSION" : "-VERSION"], { encoding: "utf8" }).status === 0);
  if (!compiler) {
    assert.notEqual(process.env.LLREMOTE_REQUIRE_NSIS, "1", "NSIS is required for candidate verification");
    t.skip("NSIS compiler is not installed"); return;
  }
  const root = await realpath(await mkdtemp(join(tmpdir(), "LL Remote 中文 NSIS source-")));
  const nsisPath = (value: string) => value.replaceAll("$", "$$");
  try {
    const source = fileURLToPath(new URL("../../../src-tauri/windows/", import.meta.url));
    await copyFile(join(source, "service-hooks.nsh"), join(root, "service-hooks.nsh"));
    await writeFile(join(root, "installer.nsi"), `\uFEFF
Unicode true
!include "${nsisPath(join(root, "service-hooks.nsh"))}"
Name "NSIS include regression"
OutFile "${nsisPath(join(root, "installer.exe"))}"
RequestExecutionLevel user
Section
SectionEnd
`);
    const compile = () => spawnSync(compiler, [windows ? "/V2" : "-V2", join(root, "installer.nsi")], {
      // Match Tauri: the generated script is not compiled from windows/.
      cwd: tmpdir(), encoding: "utf8", timeout: 30_000,
    });
    const missingSafety = compile();
    assert.notEqual(missingSafety.status, 0);
    assert.match(`${missingSafety.stdout}\n${missingSafety.stderr}`, /could not find[^\r\n]*install-safety\.nsh/i);
    await copyFile(join(source, "install-safety.nsh"), join(root, "install-safety.nsh"));
    const missingTransaction = compile();
    assert.notEqual(missingTransaction.status, 0);
    assert.match(`${missingTransaction.stdout}\n${missingTransaction.stderr}`, /could not find[^\r\n]*install-transaction\.nsh/i);
    await copyFile(join(source, "install-transaction.nsh"), join(root, "install-transaction.nsh"));
    const complete = compile();
    assert.equal(complete.status, 0, `${complete.stdout}\n${complete.stderr}`);
  } finally { await rm(root, { recursive: true, force: true }); }
});
