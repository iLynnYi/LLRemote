import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readdir, rm, stat, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("the real frontend builds from a Unicode path containing spaces and percent characters", async () => {
  const root = fileURLToPath(new URL("../../../", import.meta.url));
  const directory = await mkdtemp(join(tmpdir(), "LLRemote 中文 空格 50%-"));
  try {
    // Copy only build inputs; credentials, user data and Rust artifacts are excluded.
    for (const path of ["src", "index.html", "package.json", "tsconfig.json", "tsconfig.node.json", "vite.config.ts",
      "src-tauri/tauri.conf.json", "src-tauri/icons/icon-source.svg", "LICENSE", "docs/distribution", "licenses/THIRD-PARTY-NOTICES.txt"]) {
      const destination = join(directory, path);
      await mkdir(dirname(destination), { recursive: true });
      await cp(join(root, path), destination, { recursive: true });
    }
    await symlink(join(root, "node_modules"), join(directory, "node_modules"), process.platform === "win32" ? "junction" : "dir");
    // Use the real CLI cwd, which resolves macOS /var -> /private/var correctly.
    for (const args of [[join(root, "node_modules/typescript/bin/tsc"), "--noEmit", "--composite", "false", "-p", "tsconfig.node.json"],
      [join(root, "node_modules/vue-tsc/bin/vue-tsc.js"), "--noEmit"],
      [join(root, "node_modules/vite/bin/vite.js"), "build"]]) {
      const result = spawnSync(process.execPath, args, { cwd: directory, encoding: "utf8", timeout: 120000 });
      assert.equal(result.status, 0, `${result.error ?? ""}\n${result.stdout}\n${result.stderr}`);
    }
    assert.ok((await stat(join(directory, "dist/index.html"))).size > 0);
    assert.ok((await readdir(join(directory, "dist/assets"))).some((name) => name.endsWith(".js")));
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
