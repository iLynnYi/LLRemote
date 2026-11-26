import { execFileSync } from "node:child_process";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const escapeXml = (value) => String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;")
  .replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&apos;");

export function macosSharingMetadata(base, platform, override = {}) {
  return {
    identifier: override.identifier ?? platform.identifier ?? base.identifier,
    version: override.version ?? platform.version ?? base.version,
    minimumSystemVersion: override.bundle?.macOS?.minimumSystemVersion
      ?? platform.bundle?.macOS?.minimumSystemVersion ?? base.bundle?.macOS?.minimumSystemVersion,
  };
}

// Runs after Cargo builds both binaries and before Tauri signs the outer app.
export async function prepareMacosSharingBundle({ binaryDirectory, outputDirectory, identifier, version, minimumSystemVersion, signingIdentity = "-" }) {
  const bundle = join(outputDirectory, "LLRemote Service.app");
  await rm(bundle, { recursive: true, force: true });
  const contents = join(bundle, "Contents");
  const binary = join(contents, "MacOS/LLRemoteService");
  await mkdir(dirname(binary), { recursive: true });
  await mkdir(join(contents, "Resources"), { recursive: true });
  await cp(join(binaryDirectory, "LLRemoteService"), binary);
  await cp(join(root, "src-tauri/icons/icon.icns"), join(contents, "Resources/icon.icns"));
  const fields = {
    CFBundleIdentifier: `${identifier}.sharing`,
    CFBundleName: "LLRemote Service",
    CFBundleDisplayName: "LLRemote Service",
    CFBundleExecutable: "LLRemoteService",
    CFBundleIconFile: "icon.icns",
    CFBundlePackageType: "APPL",
    LSMinimumSystemVersion: minimumSystemVersion,
    CFBundleVersion: version,
    CFBundleShortVersionString: version,
    NSScreenCaptureUsageDescription: "用于将本机屏幕共享给已通过 VNC 验证的设备。",
  };
  await writeFile(join(contents, "Info.plist"), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
${Object.entries(fields).map(([key, value]) => `<key>${key}</key><string>${escapeXml(value)}</string>`).join("\n")}
<key>LSUIElement</key><true/>
</dict></plist>\n`);
  execFileSync("/usr/bin/codesign", ["--force", "--sign", signingIdentity,
    "--options", "runtime", ...(signingIdentity === "-" ? [] : ["--timestamp"]), bundle], { stdio: "inherit" });
  return bundle;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (!process.env.LLREMOTE_MACOS_BINARY_DIR) throw new Error("Build the macOS package with pnpm tauri build or pnpm tauri bundle.");
    const config = JSON.parse(await readFile(join(root, "src-tauri/tauri.conf.json"), "utf8"));
    const mac = JSON.parse(await readFile(join(root, "src-tauri/tauri.macos.conf.json"), "utf8"));
    const metadata = macosSharingMetadata(config, mac, JSON.parse(process.env.TAURI_CONFIG || "{}"));
    await prepareMacosSharingBundle({
      binaryDirectory: process.env.LLREMOTE_MACOS_BINARY_DIR,
      outputDirectory: join(root, "src-tauri/target/macos-sharing"),
      ...metadata,
      signingIdentity: process.env.APPLE_SIGNING_IDENTITY || mac.bundle.macOS.signingIdentity,
    });
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
