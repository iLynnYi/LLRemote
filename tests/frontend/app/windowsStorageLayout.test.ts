import assert from "node:assert/strict";
import test from "node:test";

test("the locked Tauri CLI accepts the actual Windows config before starting a build", async () => {
  const { readFile, mkdtemp, mkdir, writeFile, access, rm } = await import("node:fs/promises");
  const { spawnSync } = await import("node:child_process");
  const { createRequire } = await import("node:module");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const cli = createRequire(import.meta.url).resolve("@tauri-apps/cli/main.js");
  const root = await mkdtemp(join(tmpdir(), "LLRemote-cli-config-"));
  try {
    const common = JSON.parse(await readFile(new URL("../../../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
    const windows = await readFile(new URL("../../../src-tauri/tauri.windows.conf.json", import.meta.url), "utf8");
    common.build.beforeBuildCommand = "node config-gate.cjs";
    common.bundle = { active: false };
    await mkdir(join(root, "src-tauri/src"), { recursive: true });
    await writeFile(join(root, "package.json"), '{"name":"cli-config-fixture","version":"0.0.0"}');
    await writeFile(join(root, "config-gate.cjs"), "require('node:fs').writeFileSync('cli-config-accepted', 'yes'); process.exit(23);");
    await writeFile(join(root, "src-tauri/Cargo.toml"), '[package]\nname="cli-config-fixture"\nversion="0.0.0"\nedition="2021"\n');
    await writeFile(join(root, "src-tauri/src/main.rs"), "fn main() {}\n");
    await writeFile(join(root, "src-tauri/tauri.conf.json"), JSON.stringify(common));
    await writeFile(join(root, "src-tauri/tauri.windows.conf.json"), windows);
    const source = `require(${JSON.stringify(cli)}).run(['build','--config','src-tauri/tauri.windows.conf.json','--no-bundle'],'tauri').catch(e=>{console.error(e);process.exit(1)});`;
    const result = spawnSync(process.execPath, ["-e", source], { cwd: root, encoding: "utf8", timeout: 30_000 });
    assert.equal(result.error, undefined, String(result.error));
    // Deliberately stop at the hook: test real CLI schema validation without
    // compiling, linking, downloading dependencies or packaging a fixture.
    assert.doesNotMatch(result.stdout + result.stderr, /Additional properties are not allowed|appDirectoriesOverride.*unexpected/);
    await assert.doesNotReject(access(join(root, "cli-config-accepted")), result.stdout + result.stderr);
    assert.notEqual(result.status, 0, "the sentinel hook must stop the fixture build");
  } finally { await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 }); }
});
