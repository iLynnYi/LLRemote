import { execFileSync } from "node:child_process";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));

export async function brandMacInstallers(paths) {
  if (paths.length === 0) return;
  if (process.platform !== "darwin") {
    throw new Error("DMG file icons must be set on macOS. Windows installer icons are configured in tauri.windows.conf.json.");
  }
  const images = paths.map((path) => resolve(path));
  for (const path of images) {
    if (extname(path).toLowerCase() !== ".dmg" || !(await stat(path)).isFile()) {
      throw new Error(`Expected a DMG file: ${path}`);
    }
  }
  const temporary = await mkdtemp(join(tmpdir(), "LLRemote-installer-icon-"));
  try {
    const helper = join(temporary, "set-installer-icon");
    execFileSync("xcrun", [
      "clang", "-fobjc-arc", "-Wall", "-Werror",
      resolve(root, "scripts/macos/set-installer-icon.m"),
      "-framework", "AppKit", "-o", helper,
    ], { stdio: "inherit" });
    execFileSync(helper, [resolve(root, "src-tauri/icons/icon.icns"), ...images], { stdio: "inherit" });
    for (const image of images) {
      // A plain download can discard the DMG's custom-icon resource fork.
      // ditto stores it in AppleDouble metadata for Finder to restore on unzip.
      const archive = `${image}.zip`;
      await rm(archive, { force: true });
      execFileSync("/usr/bin/ditto", ["-c", "-k", "--sequesterRsrc", image, archive], {
        stdio: "inherit",
      });
      console.log(`Installer icon: ${image}\nIcon-preserving archive: ${archive}`);
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length < 3) throw new Error("Usage: pnpm installer:icons <installer.dmg> [...]");
    await brandMacInstallers(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
