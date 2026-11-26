import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("offline distribution gate rejects changed dependencies and tampered notice texts", async () => {
  const root = fileURLToPath(new URL("../../../", import.meta.url));
  const directory = await mkdtemp(join(tmpdir(), "LLRemote 许可校验-"));
  const inputs = ["package.json", "pnpm-lock.yaml", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "licenses", "scripts/distribution-licenses.mjs"];
  try {
    for (const path of inputs) {
      await mkdir(dirname(join(directory, path)), { recursive: true });
      await cp(join(root, path), join(directory, path), { recursive: true });
    }
    const check = () => spawnSync(process.execPath, ["scripts/distribution-licenses.mjs", "--check"], { cwd: directory, encoding: "utf8", timeout: 15000 });
    const clean = check();
    assert.equal(clean.status, 0, clean.stderr);
    const overrides = JSON.parse(await readFile(join(directory, "licenses/overrides.json"), "utf8"));
    const firstOverride = Object.values(overrides.packages)[0] as { files: Array<{ file: string }> };
    for (const path of ["package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "pnpm-lock.yaml", "licenses/overrides.json", "licenses/THIRD-PARTY-NOTICES.txt", `licenses/${firstOverride.files[0]!.file}`]) {
      const original = await readFile(join(directory, path), "utf8");
      await writeFile(join(directory, path), original + "\nchanged\n");
      const changed = check();
      assert.equal(changed.status, 1, path);
      assert.ok(changed.stderr.includes(path), `Expected diagnostic for ${path}: ${changed.stderr}`);
      // Windows checkouts may use CRLF; the semantic input is unchanged.
      await writeFile(join(directory, path), original.replaceAll("\r\n", "\n").replaceAll("\n", "\r\n"));
      const restored = check();
      assert.equal(restored.status, 0, restored.stderr);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
