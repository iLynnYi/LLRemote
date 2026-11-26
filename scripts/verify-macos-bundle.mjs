import { execFileSync } from "node:child_process";
import { existsSync, realpathSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Checks the actual packaged app. A valid ad hoc signature is integrity evidence,
// not proof of Developer ID signing, notarization or Gatekeeper acceptance.
export function verifyMacosBundle(appPath) {
  const app = realpathSync(resolve(appPath));
  const helper = join(app, "Contents/Helpers/LLRemote Service.app");
  if (!realpathSync(helper).startsWith(`${app}/`)) throw new Error("macos_helper_outside_bundle");
  const plist = (bundle) => JSON.parse(execFileSync("/usr/bin/plutil", [
    "-convert", "json", "-o", "-", join(bundle, "Contents/Info.plist"),
  ], { encoding: "utf8" }));
  const outerInfo = plist(app);
  const helperInfo = plist(helper);
  if (helperInfo.CFBundleIdentifier !== `${outerInfo.CFBundleIdentifier}.sharing`
    || helperInfo.CFBundleVersion !== outerInfo.CFBundleVersion
    || helperInfo.CFBundleShortVersionString !== outerInfo.CFBundleShortVersionString
    || !outerInfo.LSMinimumSystemVersion
    || helperInfo.LSMinimumSystemVersion !== outerInfo.LSMinimumSystemVersion
    || helperInfo.LSUIElement !== true) throw new Error("macos_helper_metadata_mismatch");
  const main = join(app, "Contents/MacOS", outerInfo.CFBundleExecutable);
  const service = join(helper, "Contents/MacOS", helperInfo.CFBundleExecutable);
  for (const binary of [main, service]) {
    if (!realpathSync(binary).startsWith(`${app}/`)) throw new Error("macos_binary_outside_bundle");
  }
  const architectures = (binary) => execFileSync("/usr/bin/lipo", ["-archs", binary], { encoding: "utf8" }).trim().split(/\s+/).sort();
  const arch = architectures(main);
  if (JSON.stringify(architectures(service)) !== JSON.stringify(arch)) throw new Error("macos_helper_architecture_mismatch");
  for (const binary of [main, service]) {
    const libraries = execFileSync("/usr/bin/otool", ["-L", binary], { encoding: "utf8" }).split("\n").slice(1);
    for (const line of libraries) {
      const dependency = line.trim().split(" (compatibility version")[0];
      if (!dependency || dependency.startsWith("/usr/lib/") || dependency.startsWith("/System/Library/")) continue;
      // Current binaries use only system libraries. Unknown rpath searches need
      // explicit LC_RPATH validation before this gate can certify them.
      if (dependency.startsWith("@rpath")) throw new Error(`macos_rpath_requires_validation: ${dependency}`);
      const bundled = resolve(dependency.replace(/^@(?:executable|loader)_path/, dirname(binary)));
      if (!existsSync(bundled) || !realpathSync(bundled).startsWith(`${app}/`)) throw new Error(`macos_unbundled_dependency: ${dependency}`);
    }
  }
  execFileSync("/usr/bin/codesign", ["--verify", "--deep", "--strict", app]);
  return { app, identifier: outerInfo.CFBundleIdentifier, version: outerInfo.CFBundleShortVersionString,
    architectures: arch, helperVersion: helperInfo.CFBundleShortVersionString, integrity: "verified",
    distributionSigning: "not-verified", notarization: "not-verified" };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (!process.argv[2]) throw new Error("Usage: node scripts/verify-macos-bundle.mjs <app>");
    console.log(JSON.stringify(verifyMacosBundle(process.argv[2])));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
