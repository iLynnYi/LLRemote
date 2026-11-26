import { glob, stat } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { brandMacInstallers } from "./installer-icons.mjs";
import { tauriBuildArgs, tauriBuildTarget } from "./tauri-target.mjs";
import { prepareWindowsWebView2 } from "./windows-installers.mjs";

const require = createRequire(import.meta.url);
const { run } = require("@tauri-apps/cli");
const root = fileURLToPath(new URL("../", import.meta.url));
const args = tauriBuildArgs(process.argv.slice(2), process.env.CARGO_BUILD_TARGET);

async function diskImages(targetDirectory) {
  const images = new Map();
  for await (const path of glob("**/bundle/dmg/*.dmg", {
    cwd: targetDirectory,
    exclude: ["**/build/**", "**/deps/**", "**/incremental/**"],
  })) {
    const absolute = resolve(targetDirectory, path);
    const metadata = await stat(absolute, { bigint: true });
    images.set(absolute, `${metadata.ino}:${metadata.size}:${metadata.mtimeNs}:${metadata.ctimeNs}`);
  }
  return images;
}

try {
  const build = ["build", "bundle"].includes(args[0])
    && !args.some((arg) => ["--help", "-h", "--version", "-V"].includes(arg));
  if (build) {
    execFileSync(process.execPath, [resolve(root, "scripts/distribution-licenses.mjs"), "--check"], { cwd: root, stdio: "inherit" });
  }
  const packaging = process.platform === "darwin" && build;
  let targetDirectory;
  let before;
  const windowsPackaging = process.platform === "win32" && build && !args.includes("--no-bundle");
  if (packaging || windowsPackaging) {
    const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
      cwd: resolve(root, "src-tauri"),
      encoding: "utf8",
    }));
    targetDirectory = metadata.target_directory;
    const option = (name) => {
      const index = args.indexOf(name);
      return index === -1 ? args.find((arg) => arg.startsWith(`${name}=`))?.slice(name.length + 1) : args[index + 1];
    };
    const profile = option("--profile") || (args.includes("--debug") || args.includes("-d") ? "debug" : "release");
    const target = tauriBuildTarget(args);
    const binaryDirectory = resolve(targetDirectory, target || "", profile === "dev" ? "debug" : profile);
    if (windowsPackaging) {
      const host = target || /^host: (\S+)/m.exec(execFileSync("rustc", ["--version", "--verbose"], { encoding: "utf8" }))?.[1];
      process.env.LLREMOTE_WEBVIEW2_INSTALLER = await prepareWindowsWebView2(binaryDirectory, host);
    } else {
      process.env.LLREMOTE_MACOS_BINARY_DIR = binaryDirectory;
      before = await diskImages(targetDirectory);
    }
  }
  await run(args, "pnpm tauri");
  if (packaging) {
    const after = await diskImages(targetDirectory);
    const updated = [...after].filter(([path, fingerprint]) => before.get(path) !== fingerprint);
    await brandMacInstallers(updated.map(([path]) => path));
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
