import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

function fixture(run: (directory: string, git: (...args: string[]) => string) => void) {
  const directory = mkdtempSync(join(tmpdir(), "llremote-public-history-"));
  const git = (...args: string[]) => {
    const result = spawnSync("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", ...args], {
      cwd: directory, encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  try {
    mkdirSync(join(directory, "scripts"));
    copyFileSync(new URL("../../../scripts/check-public-repository.mjs", import.meta.url), join(directory, "scripts/check-public-repository.mjs"));
    git("init", "--initial-branch=main");
    git("add", "scripts");
    git("commit", "-m", "public source");
    run(directory, git);
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

const check = (directory: string) => spawnSync(process.execPath, ["scripts/check-public-repository.mjs"], {
  cwd: directory, encoding: "utf8",
});

test("public repository gate rejects private files staged after the last commit", () => {
  fixture((directory, git) => {
    writeFileSync(join(directory, ".env"), "fixture\n");
    mkdirSync(join(directory, ".idea"));
    mkdirSync(join(directory, "dist"));
    writeFileSync(join(directory, ".idea/workspace.xml"), "fixture\n");
    writeFileSync(join(directory, "dist/index.html"), "fixture\n");
    git("add", ".env", ".idea", "dist");
    const result = check(directory);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /\.env/);
    assert.match(result.stderr, /\.idea\/workspace\.xml/);
    assert.match(result.stderr, /dist\/index\.html/);
  });
});

test("public repository gate rejects internal files removed only from the current tree", () => {
  fixture((directory, git) => {
    mkdirSync(join(directory, "docs/local"), { recursive: true });
    writeFileSync(join(directory, "docs/local/notes.md"), "fixture\n");
    git("add", "docs");
    git("commit", "-m", "private notes");
    git("rm", "docs/local/notes.md");
    git("commit", "-m", "remove notes");
    assert.equal(check(directory).status, 1);
  });
});

test("public repository gate checks detached HEAD and retains private Codex checkpoints", () => {
  fixture((directory, git) => {
    const clean = git("rev-parse", "HEAD");
    git("checkout", "--detach");
    writeFileSync(join(directory, "AGENTS.md"), "fixture\n");
    git("add", "AGENTS.md");
    git("commit", "-m", "local checkpoint");
    git("update-ref", "refs/codex/test-checkpoint", "HEAD");
    assert.equal(check(directory).status, 1);
    git("checkout", "--detach", clean);
    const result = check(directory);
    assert.equal(result.status, 0, result.stderr);
  });
});

test("public repository gate reports credential locations without printing their contents", () => {
  fixture((directory, git) => {
    const marker = ["-----BEGIN", "PRIVATE KEY-----"].join(" ");
    const path = "example.txt";
    writeFileSync(join(directory, path), `${marker}\n`);
    git("add", path);
    const staged = check(directory);
    assert.equal(staged.status, 1);
    assert.match(staged.stderr, /example\.txt/);
    assert.ok(!staged.stderr.includes(marker));

    git("commit", "-m", "add credential fixture");
    git("rm", path);
    git("commit", "-m", "remove fixture");
    const historical = check(directory);
    assert.equal(historical.status, 1);
    assert.match(historical.stderr, /example\.txt/);
    assert.ok(!historical.stderr.includes(marker));
  });
});
