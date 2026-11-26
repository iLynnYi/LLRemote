import { execFileSync } from "node:child_process";
import { copyFileSync, createWriteStream, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const targets = new Map([
  ["i686-pc-windows-msvc", "x86"],
  ["x86_64-pc-windows-msvc", "x64"],
  ["aarch64-pc-windows-msvc", "arm64"],
]);
const webview2LinkIds = { x86: 2099617, x64: 2124701, arm64: 2099616 };
export const webview2RuntimeName = "MicrosoftEdgeWebView2RuntimeInstaller.exe";

export async function prepareWindowsWebView2(directory, target) {
  const architecture = targets.get(target);
  if (!architecture) throw new Error(`Unsupported Windows WebView2 target: ${target}`);
  const name = `MicrosoftEdgeWebView2RuntimeInstaller${architecture.toUpperCase()}.exe`;
  const destination = join(directory, "webview2", webview2RuntimeName);
  mkdirSync(dirname(destination), { recursive: true });
  const staging = mkdtempSync(join(dirname(destination), "download-"));
  const installer = join(staging, name);
  try {
    const supplied = process.env.LLREMOTE_WEBVIEW2_OFFLINE_DIR;
    if (supplied) {
      copyFileSync(resolve(supplied, name), installer);
    } else {
      const response = await fetch(`https://go.microsoft.com/fwlink/?linkid=${webview2LinkIds[architecture]}`, {
        signal: AbortSignal.timeout(300_000),
      });
      const url = new URL(response.url);
      if (!response.ok || !response.body || url.protocol !== "https:" || !url.hostname.endsWith(".microsoft.com")
        || basename(url.pathname).toLowerCase() !== name.toLowerCase()) {
        throw new Error(`Unexpected Microsoft WebView2 offline installer response for ${architecture}`);
      }
      await pipeline(Readable.fromWeb(response.body), createWriteStream(installer));
    }
    const literal = installer.replaceAll("'", "''");
    const signatureCheck = `$ErrorActionPreference='Stop'; $s=Get-AuthenticodeSignature -LiteralPath '${literal}'; if ($s.Status -ne 'Valid' -or $s.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName,$false) -ne 'Microsoft Corporation') { throw 'Expected a valid Microsoft WebView2 installer signature' }`;
    execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(signatureCheck, "utf16le").toString("base64")], { stdio: "inherit" });
    renameSync(installer, destination);
    return destination;
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

export function installNames(productName, version, architecture) {
  return { chooser: `${productName}_${version}_${architecture}-setup.exe` };
}

export function validateWindowsInstallerSources(directory = root) {
  const required = ["installer.nsi", "service-hooks.nsh", "install-safety.nsh", "install-transaction.nsh"];
  const incomplete = required.filter((name) => {
    try {
      return readFileSync(join(directory, "src-tauri/windows", name), "utf8").trim().length === 0;
    } catch {
      return true;
    }
  });
  if (incomplete.length) {
    throw new Error(`Windows 安装脚本缺失、为空或无法读取：\n${incomplete.map((name) => `  - src-tauri/windows/${name}`).join("\n")}\n请完整同步 src-tauri/windows 目录（包括新增文件），不要只复制已有文件或只应用 git diff 补丁。`);
  }
}

function main() {
  if (process.argv[2] === "--check" && process.argv.length === 3) {
    validateWindowsInstallerSources();
    console.log("Windows installer sources are complete.");
    return;
  }
  const target = process.argv[2];
  const architecture = targets.get(target);
  if (!architecture || process.argv.length !== 3) {
    throw new Error("Usage: node scripts/windows-installers.mjs <i686-pc-windows-msvc|x86_64-pc-windows-msvc|aarch64-pc-windows-msvc>");
  }
  const config = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
  const name = installNames(config.productName, config.version, architecture).chooser;
  const directory = join(root, "src-tauri/target", target, "release/bundle/nsis");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  try {
    validateWindowsInstallerSources();
    execFileSync(process.execPath, [join(root, "scripts/tauri.mjs"), "build", "--target", target, "--bundles", "nsis", "--", "--locked"], {
      cwd: root,
      stdio: "inherit",
    });
    const files = readdirSync(directory).filter((file) => file.endsWith(".exe"));
    if (files.length !== 1 || files[0] !== name) {
      throw new Error(`Expected one Windows installer, ${name}; found ${files.join(", ") || "none"}`);
    }
  } catch (error) {
    rmSync(directory, { recursive: true, force: true });
    throw error;
  }
  console.log(`Windows installer: ${resolve(directory, name)}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
