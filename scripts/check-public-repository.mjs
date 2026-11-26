import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const git = (args) => execFileSync("git", args, {
  cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024,
});
const privateDirectories = [
  "docs/local", "docs/plans", "docs/archive", "docs/benchmarks", "docs/mockups", "docs/superpowers",
  ".pnpm-store", ".playwright-mcp", ".superpowers", ".ssh", ".codex", ".agents",
];
const credentialPatterns = [
  "-----BEGIN (OPENSSH |RSA |EC |DSA |ENCRYPTED )?PRIVATE KEY-----",
  "(^|[^A-Za-z0-9_])(AKIA|ASIA)[A-Z0-9]{16}([^A-Z0-9]|$)",
  "(gh[pousr]_[A-Za-z0-9_]{30,}|github_pat_[A-Za-z0-9_]{30,})",
  "xox[baprs]-[0-9]{10,}-[A-Za-z0-9-]{10,}",
];
const privateFiles = new Set([
  "AGENTS.md", "AI_CONTEXT.md", "AI_INDEX.md", "PROJECT_STRUCTURE.md", "CONTEXT.md",
  "mutagen.yml", "mutagen.yml.lock", "git-finish.sh", "group-sidebar-review.png",
  "docs/codex-current-context.md", "docs/feature-backlog.md", "docs/file-transfer-design.md",
  "docs/product-design.md", "docs/server-mode-design.md", "docs/session-toolbox-design.md",
  "docs/session-workspace-design.md", "docs/spike-progress.md", "docs/technical-spike-plan.md",
  "docs/usage.md", "docs/vnc-base-agent-sidecar-plan.md", "docs/vnc-server-validation.md",
  "docs/design/file-transfer.md", "docs/design/remote-session-core.md", "docs/design/server-mode.md",
  "docs/design/ultravnc-official-source-trace.md", "docs/design/vnc-current-pipeline-audit.md",
]);

export function isPrivatePath(path) {
  return privateFiles.has(path)
    || privateDirectories.some((directory) => path === directory || path.startsWith(`${directory}/`))
    || /(^|\/)(?:node_modules|target|dist|dist-ssr|coverage|logs|\.idea|\.pnpm-store|\.playwright-mcp|\.ssh)\//.test(path)
    || /(^|\/)src-tauri\/gen\/schemas\//.test(path)
    || /(^|\/)(?:\.localized|\.DS_Store|\.env(?:\..*)?|id_rsa|id_ed25519)$/.test(path)
    || /\.(?:pem|key|p12|pfx|mobileprovision|log)$/i.test(path)
    || /(^|\/)src-tauri\/icons\/host\.png$/.test(path);
}

function credentialPaths(revision) {
  const patterns = credentialPatterns.flatMap((pattern) => ["-e", pattern]);
  try {
    return git(["grep", "--no-textconv", "-I", "-l", "-z", "-E", ...patterns, revision, "--"])
      .split("\0").filter(Boolean);
  } catch (error) {
    if (error.status === 1) return [];
    throw error;
  }
}

try {
  if (git(["rev-parse", "--is-shallow-repository"]).trim() === "true") {
    throw new Error("Public history check requires a full checkout (fetch-depth: 0).");
  }
  const paths = new Set(git(["ls-files", "-z"]).split("\0").filter(Boolean));
  // Codex checkpoints and other worktrees are private local state, not published refs.
  const commits = git(["rev-list", "HEAD", "--branches", "--tags", "--remotes"]).trim().split("\n").filter(Boolean);
  for (const commit of commits) {
    for (const path of git(["ls-tree", "-r", "--name-only", "-z", commit]).split("\0")) {
      if (path) paths.add(path);
    }
  }
  const blocked = [...paths].filter(isPrivatePath).sort();
  if (blocked.length) throw new Error(`Private paths in Git files/history:\n${blocked.join("\n")}`);
  const credentials = [...new Set(["--cached", ...commits].flatMap(credentialPaths))].sort();
  if (credentials.length) throw new Error(`Possible credentials in Git files/history:\n${credentials.join("\n")}`);
  console.log(`Public repository checked: ${commits.length} commits across branches and tags.`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
