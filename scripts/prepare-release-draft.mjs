import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, createReadStream, lstatSync, mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { installNames } from "./windows-installers.mjs";

const targets = [
  { target: "aarch64-apple-darwin", platform: "macos", arch: "arm64", dmgArch: "aarch64" },
  { target: "x86_64-apple-darwin", platform: "macos", arch: "x64", dmgArch: "x64" },
  { target: "aarch64-pc-windows-msvc", platform: "windows", arch: "arm64" },
  { target: "x86_64-pc-windows-msvc", platform: "windows", arch: "x64" },
  { target: "i686-pc-windows-msvc", platform: "windows", arch: "x86" },
];

const gh = (...args) => execFileSync("gh", args, { encoding: "utf8" }).trim();
const api = (path, ...options) => JSON.parse(gh("api", path, ...options));

function selectArtifact(artifacts, target, runAttempt) {
  const prefix = `LLRemote-${target}-`;
  const match = artifacts
    .filter(({ expired, name }) => !expired && name.startsWith(prefix))
    .map(({ name }) => ({ name, attempt: Number(name.slice(prefix.length).replace(/^attempt-/, "")) }))
    .filter(({ name, attempt }) => Number.isInteger(attempt) && attempt > 0
      && attempt <= runAttempt && (name === `${prefix}${attempt}` || name === `${prefix}attempt-${attempt}`))
    .sort((left, right) => right.attempt - left.attempt)[0];
  if (!match) throw new Error(`Missing or expired installer artifact for ${target}.`);
  return match.name;
}

async function digest(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

async function prepareDraft() {
  const { BUILD_RUN_ID: runId, DEFAULT_BRANCH: branch, GH_REPO: repo, GITHUB_STEP_SUMMARY: summary } = process.env;
  if (!runId || !/^[1-9]\d*$/.test(runId) || !branch || !repo) {
    throw new Error("Expected a numeric build Run ID and GitHub repository information.");
  }
  const root = `repos/${repo}`;
  const workflow = api(`${root}/actions/workflows/pre-release.yml`);
  const run = api(`${root}/actions/runs/${runId}`);
  if (run.workflow_id !== workflow.id || run.status !== "completed" || run.conclusion !== "success" || run.head_branch !== branch
    || run.head_repository?.full_name !== repo || !["push", "workflow_dispatch"].includes(run.event)
    || !/^[0-9a-f]{40}$/.test(run.head_sha)) {
    throw new Error(`Choose a successful Pre-release checks run from ${repo} on ${branch}.`);
  }

  const source = api(`${root}/contents/src-tauri/tauri.conf.json?ref=${run.head_sha}`);
  const { productName, version } = JSON.parse(Buffer.from(source.content, "base64").toString("utf8"));
  if (typeof version !== "string" || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)
    || typeof productName !== "string" || !productName || /[\\/]/.test(productName)) {
    throw new Error("Invalid product name or version in the build's Tauri configuration.");
  }
  const tag = `v${version}`;
  const releases = api(`${root}/releases?per_page=100`, "--paginate", "--slurp").flat();
  const release = releases.find(({ tag_name: name }) => name === tag);
  const provenance = `<!-- llremote-build-run:${runId}:${run.head_sha} -->`;
  if (release && (!release.draft || release.target_commitish !== run.head_sha || !release.body?.includes(provenance))) {
    throw new Error(`Release ${tag} already exists. To publish a new version, update the code version and build it first.`);
  }
  const refs = api(`${root}/git/matching-refs/tags/${encodeURIComponent(tag)}`);
  const ref = refs.find(({ ref: name }) => name === `refs/tags/${tag}`);
  const tagTarget = ref?.object.type === "tag" ? api(`${root}/git/tags/${ref.object.sha}`).object : ref?.object;
  if (tagTarget && (tagTarget.type !== "commit" || tagTarget.sha !== run.head_sha)) {
    throw new Error(`Tag ${tag} already points to another commit.`);
  }

  const artifacts = api(`${root}/actions/runs/${runId}/artifacts?per_page=100`, "--paginate", "--slurp")
    .flatMap((page) => page.artifacts);
  const selected = targets.map((target) => ({ ...target, artifact: selectArtifact(artifacts, target.target, run.run_attempt) }));
  const directory = mkdtempSync(join(tmpdir(), "llremote-release-"));
  try {
    const downloads = join(directory, "artifacts");
    const assets = join(directory, "assets");
    mkdirSync(assets);
    gh("run", "download", runId, "--repo", repo, "--dir", downloads, ...selected.flatMap(({ artifact }) => ["--name", artifact]));

    const checksums = new Map();
    for (const { artifact, platform, arch, dmgArch } of selected) {
      const filename = dmgArch ? `${productName}_${version}_${dmgArch}.dmg` : installNames(productName, version, arch).chooser;
      const input = join(downloads, artifact, filename);
      const stat = lstatSync(input);
      if (!stat.isFile() || stat.size === 0) throw new Error(`Missing or invalid installer in ${artifact}: ${filename}`);
      const output = join(assets, `LL.Remote_${version.replace("+", "-")}_${platform}_${arch}${dmgArch ? ".dmg" : ".exe"}`);
      renameSync(input, output);
      checksums.set(output, await digest(output));
    }
    const sums = join(assets, "SHA256SUMS");
    writeFileSync(sums, [...checksums].map(([path, hash]) => `${hash}  ${basename(path)}\n`).join(""));
    checksums.set(sums, await digest(sums));

    const uploaded = new Map(release?.assets.map(({ name, digest }) => [name, digest]));
    const pending = [...checksums].filter(([path, hash]) => {
      const name = basename(path);
      if (!uploaded.has(name)) return true;
      if (uploaded.get(name) !== `sha256:${hash}`) throw new Error(`Existing draft asset differs from this build: ${name}`);
      return false;
    }).map(([path]) => path);
    const notes = [
      provenance,
      `构建来源为 [Pre-release checks #${run.run_number}](${run.html_url})，提交为 \`${run.head_sha}\`。`,
      "各安装包的 SHA-256 见附件 `SHA256SUMS`。",
    ].join("\n\n");
    const draftUrl = release?.html_url ?? gh("release", "create", tag, "--repo", repo, "--draft",
      "--target", run.head_sha, "--title", `LL Remote ${tag}`, "--notes", notes,
      ...(version.split("+")[0].includes("-") ? ["--prerelease"] : []));
    console.log(`Release draft ${tag} for build ${runId}: ${draftUrl}`);
    if (pending.length) gh("release", "upload", tag, "--repo", repo, ...pending);
    if (summary) appendFileSync(summary, `## Release draft\n\n[${tag}](${draftUrl}) from [build ${runId}](${run.html_url}).\n`);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

prepareDraft().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
