import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { macosSharingMetadata, prepareMacosSharingBundle } from "../../../scripts/macos-sharing-bundle.mjs";
import { verifyMacosBundle } from "../../../scripts/verify-macos-bundle.mjs";

test("sharing metadata uses the same platform and CLI overrides as the outer bundle", () => {
  const base = { identifier: "com.example.base", version: "1.0.0", bundle: { macOS: { minimumSystemVersion: "15.0" } } };
  const platform = { version: "2.0.0", bundle: { macOS: { minimumSystemVersion: "15.1" } } };
  assert.deepEqual(macosSharingMetadata(base, platform), { identifier: "com.example.base", version: "2.0.0", minimumSystemVersion: "15.1" });
  assert.deepEqual(macosSharingMetadata(base, platform, { identifier: "com.example.override", version: "3.0.0", bundle: { macOS: { minimumSystemVersion: "15.2" } } }), {
    identifier: "com.example.override", version: "3.0.0", minimumSystemVersion: "15.2",
  });
  assert.deepEqual(macosSharingMetadata(base, {}), { identifier: "com.example.base", version: "1.0.0", minimumSystemVersion: "15.0" });
});

test("the packaged sharing app has a separate signed identity and no Dock entry", { skip: process.platform !== "darwin" }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "llremote-package-test-"));
  try {
    const source = join(directory, "service.c");
    await writeFile(source, "int main(void) { return 0; }\n");
    execFileSync("xcrun", ["clang", source, "-o", join(directory, "LLRemoteService")]);
    const bundle = await prepareMacosSharingBundle({ binaryDirectory: directory,
      outputDirectory: directory, identifier: "com.example.LLRemote", version: "1.2.3", minimumSystemVersion: "15.0" });
    const info = JSON.parse(execFileSync("plutil", ["-convert", "json", "-o", "-", join(bundle, "Contents/Info.plist")], { encoding: "utf8" }));
    assert.equal(info.CFBundleIdentifier, "com.example.LLRemote.sharing");
    assert.equal(info.CFBundleName, "LLRemote Service");
    assert.equal(info.CFBundleExecutable, "LLRemoteService");
    assert.equal(info.CFBundleShortVersionString, "1.2.3");
    assert.equal(info.LSMinimumSystemVersion, "15.0");
    assert.equal(info.LSUIElement, true);
    assert.ok((await readFile(join(bundle, "Contents/Resources/icon.icns"))).length > 0);
    execFileSync("codesign", ["--verify", "--deep", "--strict", bundle]);
    const signature = spawnSync("codesign", ["-dvv", bundle], { encoding: "utf8" });
    assert.equal(signature.status, 0);
    assert.match(signature.stderr, /Identifier=com\.example\.LLRemote\.sharing/);
    execFileSync(join(bundle, "Contents/MacOS/LLRemoteService"));
    const app = join(directory, "LL Remote.app");
    await mkdir(join(app, "Contents/MacOS"), { recursive: true });
    await cp(join(directory, "LLRemoteService"), join(app, "Contents/MacOS/LLRemote"));
    const helper = join(app, "Contents/Helpers/LLRemote Service.app");
    await cp(bundle, helper, { recursive: true });
    await writeFile(join(app, "Contents/Info.plist"), JSON.stringify({ ...info,
      CFBundleIdentifier: "com.example.LLRemote", CFBundleExecutable: "LLRemote", LSUIElement: false,
    }));
    execFileSync("plutil", ["-convert", "xml1", join(app, "Contents/Info.plist")]);
    execFileSync("codesign", ["--force", "--sign", "-", app]);
    assert.equal(verifyMacosBundle(app).helperVersion, "1.2.3");
    execFileSync("plutil", ["-replace", "LSMinimumSystemVersion", "-string", "14.0", join(helper, "Contents/Info.plist")]);
    assert.throws(() => verifyMacosBundle(app), /macos_helper_metadata_mismatch/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
