import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const revision = "a".repeat(40);
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

async function writeArtifact(input, name, platform, architecture, installerName) {
  const artifact = join(input, name);
  const packages = join(artifact, "packages");
  await mkdir(packages, { recursive: true });
  const installer = join(packages, installerName);
  await writeFile(installer, `${name}-installer`);
  let updater = installer;
  if (platform === "macos") {
    updater = join(packages, "HOI4 Mod Setup.app.tar.gz");
    await writeFile(updater, `${name}-signed-app-archive`);
  }
  await writeFile(`${updater}.sig`, `signed-${name}`);
  await writeFile(join(artifact, "BUILD_METADATA.json"), JSON.stringify({
    product: "HOI4 Mod Setup", platform, architecture, sourceRevision: revision,
    version: "0.2.0", signing: "community",
  }));
  await writeFile(join(artifact, "THIRD_PARTY_NOTICES.md"), "# Third-party notices\n\nReviewed test inventory.\n");
  await writeFile(join(artifact, "SBOM.cdx.json"), JSON.stringify({
    bomFormat: "CycloneDX", specVersion: "1.5", version: 1,
    metadata: { component: { name: "hoi4-mod-setup", version: "0.2.0" } },
    components: [{ "bom-ref": "pkg:test/shared@1", name: "shared", version: "1", type: "library" }],
  }));
  await writeFile(join(artifact, "SIGNING_VERIFICATION.json"), JSON.stringify({
    schema_version: "1.0.0", source_revision: revision, platform, architecture,
    package_sha256: { [installerName]: digest(await readFile(installer)) }, method: "test-signing",
  }));
  const files = [];
  async function walk(directory, prefix = "") {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
      const absolute = join(directory, entry.name);
      if (entry.isDirectory()) await walk(absolute, relative);
      else if (relative !== "ARTIFACTS.sha256") files.push({ path: relative, sha256: digest(await readFile(absolute)) });
    }
  }
  await walk(artifact);
  await writeFile(join(artifact, "ARTIFACTS.sha256"), JSON.stringify(files));
}

test("curates signed installers and complete updater metadata", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-release-test-"));
  try {
    const input = join(temporary, "input");
    const output = join(temporary, "output");
    await mkdir(input);
    await writeArtifact(input, "hoi4-mod-setup-windows-x64", "windows", "X64", "setup.exe");
    await writeArtifact(input, "hoi4-mod-setup-macos-arm64", "macos", "ARM64", "setup.dmg");
    await writeArtifact(input, "hoi4-mod-setup-macos-x64", "macos", "X64", "setup.dmg");
    const result = spawnSync(process.execPath, [join(root, "scripts", "prepare_release_assets.mjs"), input, output], {
      cwd: root, encoding: "utf8",
      env: { ...process.env, GITHUB_SHA: revision, GITHUB_REF_NAME: "v0.2.0" },
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.deepEqual((await readdir(output)).sort(), [
      "HOI4-Mod-Setup-macos-arm64.app.tar.gz", "HOI4-Mod-Setup-macos-arm64.dmg",
      "HOI4-Mod-Setup-macos-x64.app.tar.gz", "HOI4-Mod-Setup-macos-x64.dmg",
      "HOI4-Mod-Setup-windows-x64-setup.exe", "RELEASE_NOTES.md", "latest.json",
    ]);
    const metadata = JSON.parse(await readFile(join(output, "latest.json"), "utf8"));
    assert.equal(metadata.version, "0.2.0");
    assert.deepEqual(Object.keys(metadata.platforms).sort(), ["darwin-aarch64", "darwin-x86_64", "windows-x86_64"]);
    for (const target of Object.values(metadata.platforms)) {
      assert.match(target.url, /^https:\/\/github\.com\/klimPaskov\/HOI4-Mod-Setup\/releases\/download\/v0\.2\.0\//);
      assert.match(target.signature, /^signed-/);
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});
