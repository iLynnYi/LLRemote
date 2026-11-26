import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const targets = ["aarch64-apple-darwin", "x86_64-apple-darwin", "aarch64-pc-windows-msvc", "i686-pc-windows-msvc", "x86_64-pc-windows-msvc"];
const inputFiles = ["package.json", "pnpm-lock.yaml", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "licenses/overrides.json"];
const hash = (data) => createHash("sha256").update(data).digest("hex");
const read = (path) => readFileSync(join(root, path), "utf8");
const normalized = (text) => text.replaceAll("\r\n", "\n").trim() + "\n";
const json = (data) => JSON.stringify(data, null, 2) + "\n";
const inputs = () => Object.fromEntries(inputFiles.map((path) => [path, hash(normalized(read(path)))]));
const inventoryPath = "licenses/inventory.json";
const noticePath = "licenses/THIRD-PARTY-NOTICES.txt";

function packageNotices(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (!/^(licen[cs]e|copying|notice|unlicense|copyright)([._-]|$)/i.test(entry.name)) return [];
    if (entry.isFile()) return [{ name: entry.name, text: normalized(readFileSync(join(directory, entry.name), "utf8")) }];
    if (!entry.isDirectory()) return [];
    return readdirSync(join(directory, entry.name), { withFileTypes: true })
      .filter((child) => child.isFile())
      .map((child) => ({ name: `${entry.name}/${child.name}`, text: normalized(readFileSync(join(directory, entry.name, child.name), "utf8")) }));
  }).sort((a, b) => a.name.localeCompare(b.name, "en"));
}

function check() {
  const inventory = JSON.parse(read(inventoryPath));
  const issues = [];
  if (inventory.schemaVersion !== 1) issues.push(`${inventoryPath}: unsupported schema version`);
  if (JSON.stringify(inventory.targets) !== JSON.stringify(targets)) issues.push(`${inventoryPath}: target graphs do not match release targets`);
  for (const [path, digest] of Object.entries(inputs())) {
    if (inventory.inputs?.[path] !== digest) issues.push(`${path}: changed since license materials were generated`);
  }
  if (inventory.noticeSha256 !== hash(normalized(read(noticePath)))) issues.push(`${noticePath}: content does not match inventory`);
  if (!inventory.packages?.length) issues.push(`${inventoryPath}: package inventory is empty`);
  if (issues.length) {
    throw new Error([
      "Distribution notices are stale or incomplete:",
      ...issues.map((issue) => `  - ${issue}`),
      "Run pnpm licenses:generate, review the changes, and sync licenses/ together with the dependency files before building on another machine.",
    ].join("\n"));
  }
  const overrides = JSON.parse(read("licenses/overrides.json"));
  for (const item of [...Object.values(overrides.packages), ...overrides.additional]) {
    for (const file of item.files) {
      if (hash(normalized(read(`licenses/${file.file}`))) !== file.sha256) throw new Error(`Upstream notice changed: licenses/${file.file}`);
    }
  }
  console.log(`License materials checked: ${inventory.packages.length} packages; ${inventory.targets.length} target graphs. No network required.`);
}

function generate() {
  const texts = new Map();
  const packages = new Map();
  const overrides = JSON.parse(read("licenses/overrides.json"));
  const add = (item, directory) => {
    const key = `${item.ecosystem}:${item.name}@${item.version}`;
    const existing = packages.get(key);
    if (existing) { existing.targets = [...new Set([...existing.targets, ...item.targets])].sort(); return; }
    const supplied = packageNotices(directory).map((file) => ({ ...file, source: `${item.source}#${file.name}` }));
    const extra = overrides.packages[key];
    for (const file of extra?.files ?? []) {
      const text = normalized(read(`licenses/${file.file}`));
      if (hash(text) !== file.sha256) throw new Error(`Upstream notice changed: ${file.file}`);
      supplied.push({ name: file.file, text, source: file.url });
    }
    if (!item.license || !supplied.length) throw new Error(`Missing license declaration or text: ${key}`);
    item.notices = supplied.map(({ text, source }) => {
      const id = hash(text); texts.set(id, text); return { id, source };
    });
    if (extra?.note) item.note = extra.note;
    packages.set(key, item);
  };
  for (const target of targets) {
    const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--manifest-path", "src-tauri/Cargo.toml", "--locked", "--filter-platform", target, "--format-version", "1"], { cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 }));
    for (const entry of metadata.packages) {
      if (!entry.source) continue;
      if (!entry.source.startsWith("registry+")) throw new Error(`Review non-registry source: ${entry.name}`);
      add({ ecosystem: "cargo", name: entry.name, version: entry.version, license: entry.license,
        authors: entry.authors, source: `https://crates.io/api/v1/crates/${entry.name}/${entry.version}/download`, targets: [target] }, dirname(entry.manifest_path));
    }
  }
  const pnpmPath = process.env.npm_execpath;
  if (!pnpmPath) throw new Error("Run this generator with pnpm licenses:generate so its package manager is explicit.");
  const cli = pnpmPath.endsWith(".exe") ? pnpmPath : process.execPath;
  const args = [...(cli === process.execPath ? [pnpmPath] : []), "licenses", "list", "--prod", "--json"];
  const npmLicenses = JSON.parse(execFileSync(cli, args, { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }));
  if (npmLicenses.error) throw new Error(`pnpm license inventory failed: ${npmLicenses.error.code}`);
  for (const group of Object.values(npmLicenses)) {
    for (const entry of group) {
      for (const directory of entry.paths) {
        const manifest = JSON.parse(readFileSync(join(directory, "package.json"), "utf8"));
        add({ ecosystem: "npm", name: manifest.name, version: manifest.version, license: entry.license,
          authors: entry.author ? [typeof entry.author === "string" ? entry.author : JSON.stringify(entry.author)] : [],
          source: `https://registry.npmjs.org/${manifest.name.replaceAll("/", "%2F")}/${manifest.version}`, targets: ["frontend"] }, directory);
      }
    }
  }
  for (const item of overrides.additional) {
    const notices = item.files.map((file) => {
      const text = normalized(read(`licenses/${file.file}`));
      if (hash(text) !== file.sha256) throw new Error(`Upstream notice changed: ${file.file}`);
      const id = hash(text); texts.set(id, text); return { id, source: file.url };
    });
    packages.set(`asset:${item.name}`, { ecosystem: "asset", name: item.name, version: "see source", license: item.license, targets: ["frontend"], authors: [], notices });
  }
  const inventory = { schemaVersion: 1, inputs: inputs(), targets, packages: [...packages.values()].sort((a, b) => `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`, "en")) };
  const sections = [
    "LL Remote — Third-party notices / 第三方许可与声明",
    "This inventory covers the locked frontend production dependency graph and Windows/macOS Rust graphs, including build dependencies. It is a conservative superset, not a claim that every package is linked into every binary. Third-party terms apply to their respective components; the project's MIT license does not replace them.",
    "Source archives for the exact crate versions are linked below, including MPL-2.0 components. Those components are used without local source changes. Some publishers omit license texts; reviewed upstream texts or explicitly marked published-metadata supplements are retained with provenance. See licenses/README.md in the source repository.",
    ...inventory.packages.map((item) => [
      `=== ${item.ecosystem}:${item.name}@${item.version} ===`, `License: ${item.license}`, `Authors (as published): ${item.authors.join("; ") || "see notices/source"}`,
      `Source: ${item.source ?? item.notices[0].source}`, `Graphs: ${item.targets.join(", ")}`, ...(item.note ? [item.note] : []),
      ...item.notices.map((notice) => `Text ${notice.id}\nProvenance: ${notice.source}`),
    ].join("\n")),
    ...[...texts].sort(([a], [b]) => a.localeCompare(b)).map(([id, text]) => `=== LICENSE TEXT ${id} ===\n${text}`),
  ];
  const notice = normalized(sections.join("\n\n"));
  inventory.noticeSha256 = hash(notice);
  writeFileSync(join(root, noticePath), notice);
  writeFileSync(join(root, inventoryPath), json(inventory));
  check();
}

try {
  const mode = process.argv[2];
  if (mode === "--check") check();
  else if (mode === "--generate") generate();
  else throw new Error("Usage: distribution-licenses.mjs --generate|--check");
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
