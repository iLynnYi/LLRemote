import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { verifyMacosBundle } from "./verify-macos-bundle.mjs";
import { installNames, webview2RuntimeName } from "./windows-installers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const machines = { "aarch64-pc-windows-msvc": 0xaa64, "x86_64-pc-windows-msvc": 0x8664, "i686-pc-windows-msvc": 0x14c };
const supported = [...Object.keys(machines), "aarch64-apple-darwin", "x86_64-apple-darwin"];
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

export function nsisMainDigest(path) {
  const binary = readFileSync(path);
  // Tauri replaces the first bundle marker before packaging and restores the
  // original executable afterward. All other bytes must still match.
  const original = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_UNK");
  const offset = binary.indexOf(original);
  if (offset === -1) throw new Error("candidate_bundle_type_marker_missing");
  binary.write("__TAURI_BUNDLE_TYPE_VAR_NSS", offset, original.length, "ascii");
  return createHash("sha256").update(binary).digest("hex");
}

/** Explicit mirror mode fingerprints resident build inputs without inventing a Git revision. */
export function fingerprintBuildInputs(directory) {
  const files = [];
  const collect = (path) => {
    const absolute = join(directory, path);
    if (!existsSync(absolute)) return;
    const stat = lstatSync(absolute);
    if (stat.isSymbolicLink()) throw new Error(`candidate_source_symlink_requires_review: ${path}`);
    if (stat.isDirectory()) {
      for (const entry of readdirSync(absolute)) collect(`${path}/${entry}`);
    } else if (stat.isFile()) files.push(path);
  };
  for (const path of ["package.json", "pnpm-lock.yaml", "pnpm-workspace.yaml", "index.html", "vite.config.ts", "tsconfig.json", "tsconfig.node.json", "LICENSE",
    "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/build.rs", "src-tauri/tauri.conf.json", "src-tauri/tauri.macos.conf.json", "src-tauri/tauri.windows.conf.json",
    "src", "scripts", "tests", "licenses", "docs/distribution", "src-tauri/src", "src-tauri/native", "src-tauri/capabilities", "src-tauri/icons", "src-tauri/windows"]) collect(path);
  const fingerprint = createHash("sha256");
  for (const path of files.sort()) fingerprint.update(path).update("\0").update(readFileSync(join(directory, path)));
  return fingerprint.digest("hex");
}

function sourceState(mirror) {
  if (mirror) return { mode: "source-mirror", commit: null, dirty: null, scope: "resident-application-inputs", fingerprint: fingerprintBuildInputs(root) };
  const git = (args) => execFileSync("git", args, { cwd: root, maxBuffer: 32 * 1024 * 1024 });
  const commit = git(["rev-parse", "HEAD"]).toString().trim();
  const diff = git(["diff", "--binary", "HEAD", "--"]);
  const untracked = git(["ls-files", "--others", "--exclude-standard", "-z"]).toString().split("\0").filter(Boolean).sort();
  const fingerprint = createHash("sha256").update(commit).update(diff);
  for (const path of untracked) fingerprint.update(path).update("\0").update(readFileSync(join(root, path)));
  return { mode: "git-checkout", commit, dirty: diff.length > 0 || untracked.length > 0, fingerprint: fingerprint.digest("hex") };
}

/** Check actual PE headers, including bounds, rather than trusting its filename. */
export function verifyPeMachine(bytes, expected) {
  if (bytes.length < 64 || bytes.readUInt16LE(0) !== 0x5a4d) throw new Error("candidate_invalid_pe");
  const offset = bytes.readUInt32LE(0x3c);
  if (offset < 64 || offset + 24 > bytes.length || bytes.readUInt32LE(offset) !== 0x00004550) throw new Error("candidate_invalid_pe");
  if (bytes.readUInt16LE(offset + 4) !== expected) throw new Error("candidate_architecture_mismatch");
}

/** Inspect extracted payloads, not just the NSIS bootstrapper's x86 PE header. */
export function verifyWindowsPayload(directory, expected) {
  const found = new Map();
  const visit = (path) => {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const absolute = join(path, entry.name);
      if (entry.isSymbolicLink()) throw new Error("candidate_payload_symlink");
      if (entry.isDirectory()) visit(absolute);
      else if (entry.isFile()) {
        const path = relative(directory, absolute).replaceAll(sep, "/");
        // 7-Zip also lists NSIS rollback markers created by FileOpen.
        // Only complete paths identify the files embedded by File instructions.
        if (!expected.has(path)) continue;
        if (digest(absolute) !== expected.get(path)) throw new Error(`candidate_payload_mismatch: ${path}`);
        found.set(path, absolute);
      }
    }
  };
  visit(directory);
  for (const name of expected.keys()) {
    if (!found.has(name)) throw new Error(`candidate_payload_missing: ${name}`);
  }
  return found;
}

function verifyWindowsInstaller(installer, main, resources, webview2) {
  const directory = mkdtempSync(join(tmpdir(), "LLRemote-payload-"));
  try {
    // GitHub's Windows image includes 7-Zip. Local candidate verification also
    // requires it; do not downgrade this gate to a filename-only check.
    const sevenZip = existsSync("C:/Program Files/7-Zip/7z.exe") ? "C:/Program Files/7-Zip/7z.exe" : "7z.exe";
    execFileSync(sevenZip, ["x", "-y", `-o${directory}`, installer], { stdio: "pipe", timeout: 60_000 });
    const payloadRoot = "$PLUGINSDIR/payload";
    const mainPath = `${payloadRoot}/LLRemote.exe`;
    const expected = new Map([
      [mainPath, nsisMainDigest(main)],
      ...resources.map(([name, source]) => [`${payloadRoot}/${name}`, digest(source)]),
      [`$PLUGINSDIR/${webview2RuntimeName}`, digest(webview2)],
    ]);
    const extracted = verifyWindowsPayload(directory, expected);
    // Loading the actual packaged main is side-effect free; installer checks
    // SCM/configuration separately after its scope/UAC choice.
    const report = join(directory, "loader-check.ini");
    execFileSync(extracted.get(mainPath), ["--installer-check", "--installer-report", report], { timeout: 15_000, stdio: "pipe" });
    const handshake = readFileSync(report, "utf8");
    if (!/(?:^|\r?\n)protocol=1(?:\r?\n|$)/.test(handshake) || !/(?:^|\r?\n)result=0(?:\r?\n|$)/.test(handshake)) throw new Error("candidate_loader_handshake_invalid");
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

export function requireFreshArtifact(path, startedAt) {
  const stat = statSync(path);
  if (!stat.isFile() || stat.size === 0 || stat.mtimeMs < startedAt - 2000) throw new Error(`candidate_stale_or_empty_artifact: ${path}`);
}

function candidatePaths(target) {
  if (!supported.includes(target)) throw new Error("candidate_target_invalid");
  const directory = join(root, "src-tauri/target", target, "release");
  return { directory, marker: join(directory, ".candidate-start.json"), report: join(directory, "candidate-verification.json"), sums: join(directory, "SHA256SUMS") };
}

function begin(target, mirror) {
  const paths = candidatePaths(target);
  mkdirSync(paths.directory, { recursive: true });
  for (const path of [paths.report, paths.sums]) rmSync(path, { force: true });
  writeFileSync(paths.marker, JSON.stringify({ target, source: sourceState(mirror), startedAt: Date.now() }));
  console.log(`Candidate build started for ${target}; previous verification cleared.`);
}

function verify(target, mirror) {
  const paths = candidatePaths(target);
  for (const path of [paths.report, paths.sums]) rmSync(path, { force: true });
  const marker = JSON.parse(readFileSync(paths.marker, "utf8"));
  if (marker.target !== target || JSON.stringify(marker.source) !== JSON.stringify(sourceState(mirror))
    || !Number.isFinite(marker.startedAt) || marker.startedAt > Date.now()) throw new Error("candidate_build_marker_invalid");
  const base = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
  const mac = target.endsWith("apple-darwin");
  const config = { ...base, ...JSON.parse(readFileSync(join(root, `src-tauri/tauri.${mac ? "macos" : "windows"}.conf.json`), "utf8")) };
  const artifacts = [];
  let metadata;
  if (mac) {
    if (process.platform !== "darwin") throw new Error("candidate_requires_macos");
    const app = join(paths.directory, "bundle/macos", `${config.productName}.app`);
    metadata = verifyMacosBundle(app);
    const expectedArchitecture = target.startsWith("aarch64-") ? "arm64" : "x86_64";
    if (metadata.version !== config.version || JSON.stringify(metadata.architectures) !== JSON.stringify([expectedArchitecture])) throw new Error("candidate_metadata_mismatch");
    for (const [source, destination] of Object.entries(base.bundle.resources ?? {})) {
      const packaged = join(app, "Contents/Resources", destination);
      if (digest(packaged) !== digest(resolve(root, "src-tauri", source))) throw new Error(`candidate_resource_mismatch: ${destination}`);
    }
    const images = readdirSync(join(paths.directory, "bundle/dmg")).filter((name) => name.endsWith(".dmg"));
    if (images.length !== 1) throw new Error("candidate_disk_image_missing_or_ambiguous");
    for (const image of images) {
      const path = join(paths.directory, "bundle/dmg", image);
      requireFreshArtifact(path, marker.startedAt);
      execFileSync("/usr/bin/hdiutil", ["verify", path], { stdio: "inherit" });
      const archive = `${path}.zip`;
      requireFreshArtifact(archive, marker.startedAt);
      execFileSync("/usr/bin/unzip", ["-tq", archive], { stdio: "inherit" });
      artifacts.push(path, archive);
    }
  } else {
    if (process.platform !== "win32") throw new Error("candidate_requires_windows");
    const webview2 = join(paths.directory, "webview2", webview2RuntimeName);
    requireFreshArtifact(webview2, marker.startedAt);
    const binaries = [join(paths.directory, "LLRemote.exe"), join(paths.directory, "LLRemoteService.exe")];
    for (const path of binaries) verifyPeMachine(readFileSync(path), machines[target]);
    const literal = JSON.stringify(binaries).replaceAll("'", "''");
    const script = `$ErrorActionPreference='Stop';$paths=ConvertFrom-Json '${literal}';@($paths | ForEach-Object { $v=[Diagnostics.FileVersionInfo]::GetVersionInfo($_);[PSCustomObject]@{fileVersion=$v.FileVersion;productVersion=$v.ProductVersion} }) | ConvertTo-Json -Compress`;
    const versions = JSON.parse(execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], { encoding: "utf8" }));
    if (versions.length !== 2 || versions.some((version) => [version.fileVersion, version.productVersion].some((value) => value !== config.version && value !== `${config.version}.0`))) throw new Error("candidate_metadata_mismatch");
    const directory = join(paths.directory, "bundle/nsis");
    const names = installNames(config.productName, config.version, target.startsWith("aarch64-") ? "arm64" : target.startsWith("i686-") ? "x86" : "x64");
    const expected = [names.chooser];
    const installers = readdirSync(directory).filter((name) => name.endsWith(".exe"));
    if (JSON.stringify(installers.sort()) !== JSON.stringify(expected.sort())) throw new Error("candidate_installer_missing_or_ambiguous");
    for (const name of expected) {
      const installer = join(directory, name);
      requireFreshArtifact(installer, marker.startedAt);
      verifyPeMachine(readFileSync(installer), 0x14c); // NSIS itself is x86, including ARM64 packages.
      const resources = Object.entries(base.bundle.resources ?? {}).map(
        ([source, destination]) => [destination.replaceAll("\\", "/"), resolve(root, "src-tauri", source)],
      );
      verifyWindowsInstaller(installer, binaries[0], [...resources, ["LLRemoteService.exe", binaries[1]]], webview2);
      artifacts.push(installer);
    }
    metadata = { version: config.version, architectures: [target], binaryVersions: versions, installerRuntime: "x86", installChoices: ["currentUser", "perMachine"], installerContents: "payload-hashes-verified", loaderCheck: "passed" };
    artifacts.unshift(...binaries);
  }
  const report = { schemaVersion: 1, status: "candidate-build-verified", source: marker.source, target, metadata,
    signing: "not-verified", notarization: "not-verified", desktopAcceptance: "pending", published: false,
    artifacts: artifacts.map((path) => ({ path: relative(root, path).replaceAll("\\", "/"), bytes: statSync(path).size, sha256: digest(path) })) };
  writeFileSync(paths.report, JSON.stringify(report, null, 2) + "\n");
  writeFileSync(paths.sums, report.artifacts.map((item) => `${item.sha256}  ${item.path}`).join("\n") + "\n");
  console.log(JSON.stringify(report, null, 2));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [mode, target, option, ...extra] = process.argv.slice(2);
    if (extra.length || (option && option !== "--source-mirror")) throw new Error("candidate_option_invalid");
    if (option && process.env.GITHUB_ACTIONS === "true") throw new Error("candidate_ci_requires_git_checkout");
    if (mode === "begin") begin(target, Boolean(option));
    else if (mode === "verify") verify(target, Boolean(option));
    else throw new Error("Usage: release-candidate.mjs begin|verify <target> [--source-mirror]");
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
