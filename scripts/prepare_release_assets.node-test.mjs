import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFile, mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const revision = "a".repeat(40);
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

async function writeArtifact(input, name, platform, architecture, installerName, signing = "community") {
  const artifact = join(input, name);
  const packages = join(artifact, "packages");
  await mkdir(packages, { recursive: true });
  const installer = join(packages, installerName);
  await writeFile(installer, `${name}-installer`);
  const installerNames = platform === "linux" ? [installerName, "setup.deb"] : [installerName];
  if (platform === "linux") await writeFile(join(packages, "setup.deb"), `${name}-deb-installer`);
  let updater = installer;
  if (platform === "macos") {
    updater = join(packages, "HOI4 Mod Setup.app.tar.gz");
    await writeFile(updater, `${name}-signed-app-archive`);
  }
  await writeFile(`${updater}.sig`, `signed-${name}`);
  if (platform === "linux") await writeFile(join(packages, "setup.deb.sig"), `signed-${name}-deb`);
  await writeFile(join(artifact, "BUILD_METADATA.json"), JSON.stringify({
    product: "HOI4 Mod Setup", platform, architecture, sourceRevision: revision,
    version: "0.2.0", signing: platform === "linux" && signing !== "not_configured" ? "updater_signed" : signing,
  }));
  await writeFile(join(artifact, "THIRD_PARTY_NOTICES.md"), [
    "# Third-party notices", "", "Reviewed test inventory.", "", "", "", "",
    "## JavaScript dependencies", "", "| Package | Version | License | Homepage |", "| --- | --- | --- | --- |",
    "| shared | 1 | MIT | https://example.invalid/shared |",
    `| ${platform}-${architecture} | 1 | MIT | https://example.invalid/platform |`,
    "", "## Rust dependencies", "", "| Package | Version | License | Source |", "| --- | --- | --- | --- |", "",
  ].join("\n"));
  await writeFile(join(artifact, "SBOM.cdx.json"), JSON.stringify({
    bomFormat: "CycloneDX", specVersion: "1.5", version: 1,
    metadata: { component: { name: "hoi4-mod-setup", version: "0.2.0" } },
    components: [
      { "bom-ref": "pkg:test/shared@1", name: "shared", version: "1", type: "library" },
      { "bom-ref": `pkg:test/${platform}-${architecture}@1`, name: `${platform}-${architecture}`, version: "1", type: "library" },
    ],
  }));
  await writeFile(join(artifact, "SIGNING_VERIFICATION.json"), JSON.stringify({
    schema_version: "1.0.0", source_revision: revision, platform, architecture,
    package_sha256: Object.fromEntries(await Promise.all(installerNames.map(async (filename) => [filename, digest(await readFile(join(packages, filename)))]))), method: platform === "linux" ? "tauri-updater-signature" : "test-signing",
  }));
  await refreshManifest(artifact);
  return artifact;
}

async function refreshManifest(artifact) {
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

for (const signing of ["community", "configured"]) {
test(`curates ${signing} installers, release notes, and complete updater metadata`, async () => {
  const temporary = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-release-test-"));
  try {
    const input = join(temporary, "input");
    const output = join(temporary, "output");
    await mkdir(input);
    await writeArtifact(input, "hoi4-mod-setup-linux-x64", "linux", "X64", "setup.AppImage", signing);
    await writeArtifact(input, "hoi4-mod-setup-windows-x64", "windows", "X64", "setup.exe", signing);
    await writeArtifact(input, "hoi4-mod-setup-macos-arm64", "macos", "ARM64", "setup.dmg", signing);
    await writeArtifact(input, "hoi4-mod-setup-macos-x64", "macos", "X64", "setup.dmg", signing);
    const result = spawnSync(process.execPath, [join(root, "scripts", "prepare_release_assets.mjs"), input, output], {
      cwd: root, encoding: "utf8", windowsHide: true,
      env: { ...process.env, GITHUB_SHA: revision, GITHUB_REF_NAME: "v0.2.0" },
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.deepEqual((await readdir(output)).sort(), [
      "HOI4-Mod-Setup-linux-x64.AppImage", "HOI4-Mod-Setup-linux-x64.deb",
      "HOI4-Mod-Setup-macos-arm64.app.tar.gz", "HOI4-Mod-Setup-macos-arm64.dmg",
      "HOI4-Mod-Setup-macos-x64.app.tar.gz", "HOI4-Mod-Setup-macos-x64.dmg",
      "HOI4-Mod-Setup-windows-x64-setup.exe", "RELEASE_NOTES.md", "latest.json",
    ]);
    const releaseNotes = await readFile(join(output, "RELEASE_NOTES.md"), "utf8");
    assert.ok(releaseNotes.includes("## Changes\n"));
    assert.equal(releaseNotes.split("\n").filter((line) => /^- (Windows|Mac|Linux)/.test(line)).length, 5);
    for (const feature of ["GPTImage is the default", "Coding clients remain independent", "Update and repair", "Clausewitz MCP by smerss", "native Linux HOI4 1.19.2", "manual Linux game/server setup", "supports Windows only", "README", "Quickstart"]) assert.ok(releaseNotes.includes(feature), feature);
    assert.equal(releaseNotes.includes("community certificate named ChaosX"), signing === "community");
    assert.equal(releaseNotes.includes("not Apple-notarized"), signing === "community");
    assert.doesNotMatch(releaseNotes, /false positive|flag.*harmful/i);
    const metadata = JSON.parse(await readFile(join(output, "latest.json"), "utf8"));
    assert.equal(metadata.version, "0.2.0");
    assert.deepEqual(Object.keys(metadata.platforms).sort(), ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "linux-x86_64-appimage", "linux-x86_64-deb", "windows-x86_64"]);
    assert.deepEqual(metadata.platforms["linux-x86_64"], metadata.platforms["linux-x86_64-appimage"]);
    assert.match(metadata.platforms["linux-x86_64-deb"].url, /\.deb$/);
    assert.notEqual(metadata.platforms["linux-x86_64-deb"].signature, metadata.platforms["linux-x86_64-appimage"].signature);
    for (const target of Object.values(metadata.platforms)) {
      assert.match(target.url, /^https:\/\/github\.com\/klimPaskov\/HOI4-Mod-Setup\/releases\/download\/v0\.2\.0\//);
      assert.match(target.signature, /^signed-/);
    }
    const combined = JSON.parse(await readFile(join(input, "SBOM.cdx.json"), "utf8"));
    assert.deepEqual(combined.components.map((component) => component["bom-ref"]), [
      "pkg:test/linux-X64@1", "pkg:test/macos-ARM64@1", "pkg:test/macos-X64@1", "pkg:test/shared@1", "pkg:test/windows-X64@1",
    ]);
    const notices = await readFile(join(input, "THIRD_PARTY_NOTICES.md"), "utf8");
    for (const dependency of ["shared", "linux-X64", "macos-ARM64", "macos-X64", "windows-X64"]) {
      assert.match(notices, new RegExp(`\\| ${dependency} \\| 1 \\| MIT`));
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

}

for (const [label, changelog, expectedError] of [
  ["missing", "# Changelog\n\n## 0.3.0\n\n- Another version.\n", /missing release 0\.2\.0/],
  ["empty", "# Changelog\n\n## 0.2.0 - 2026-10-13\n\n## 0.1.0\n\n- Older changes.\n", /no changes for release 0\.2\.0/],
]) {
  test(`release curation rejects a ${label} version changelog section`, async () => {
    const temporary = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-changelog-test-"));
    try {
      const input = join(temporary, "input");
      const output = join(temporary, "output");
      const scripts = join(temporary, "scripts");
      await mkdir(input);
      await mkdir(scripts);
      await copyFile(join(root, "scripts/prepare_release_assets.mjs"), join(scripts, "prepare_release_assets.mjs"));
      await writeFile(join(temporary, "CHANGELOG.md"), changelog);
      for (const [platform, architecture, extension] of [["linux", "X64", "AppImage"], ["windows", "X64", "exe"], ["macos", "ARM64", "dmg"], ["macos", "X64", "dmg"]]) {
        await writeArtifact(input, `hoi4-mod-setup-${platform}-${architecture.toLowerCase()}`, platform, architecture, `setup.${extension}`);
      }
      const result = spawnSync(process.execPath, [join(scripts, "prepare_release_assets.mjs"), input, output], {
        cwd: temporary, encoding: "utf8", windowsHide: true,
        env: { ...process.env, GITHUB_SHA: revision, GITHUB_REF_NAME: "v0.2.0" },
      });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr || result.stdout, expectedError);
      assert.ok(!(await readdir(output)).includes("RELEASE_NOTES.md"));
    } finally { await rm(temporary, { recursive: true, force: true }); }
  });
}

test("preview curation retains platform dependency unions outside public installers", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-preview-test-"));
  try {
    const input = join(temporary, "input");
    const output = join(temporary, "output");
    await mkdir(input);
    for (const [platform, architecture, extension] of [["linux", "X64", "AppImage"], ["windows", "X64", "exe"], ["macos", "ARM64", "dmg"], ["macos", "X64", "dmg"]]) {
      await writeArtifact(input, `hoi4-mod-setup-preview-${platform}-${architecture.toLowerCase()}`, platform, architecture, `setup.${extension}`, "not_configured");
    }
    const result = spawnSync(process.execPath, [join(root, "scripts/prepare_preview_assets.mjs"), input, output], {
      cwd: root, encoding: "utf8", windowsHide: true,
      env: { ...process.env, GITHUB_SHA: revision, PREVIEW_TAG: "0.2.0" },
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.deepEqual((await readdir(output)).sort(), [
      "HOI4-Mod-Setup-linux-x64.AppImage", "HOI4-Mod-Setup-linux-x64.deb",
      "HOI4-Mod-Setup-macos-arm64.dmg", "HOI4-Mod-Setup-macos-x64.dmg",
      "HOI4-Mod-Setup-windows-x64-setup.exe", "PREVIEW_RELEASE_NOTES.md",
    ]);
    const combined = JSON.parse(await readFile(join(input, "SBOM.cdx.json"), "utf8"));
    assert.equal(combined.components.length, 5);
    const notices = await readFile(join(input, "THIRD_PARTY_NOTICES.md"), "utf8");
    for (const dependency of ["shared", "linux-X64", "macos-ARM64", "macos-X64", "windows-X64"]) {
      assert.match(notices, new RegExp(`\\| ${dependency} \\| 1 \\| MIT`));
    }
  } finally { await rm(temporary, { recursive: true, force: true }); }
});

const rejectionCases = [
  ["missing Linux platform", async (linux) => rm(linux, { recursive: true }), /artifact set must contain exactly/],
  ["missing Debian package", async (linux) => { await rm(join(linux, "packages", "setup.deb")); await refreshManifest(linux); }, /exactly one .deb package/],
  ["missing Debian updater signature", async (linux) => { await rm(join(linux, "packages", "setup.deb.sig")); await refreshManifest(linux); }, /missing its updater signature/],
  ["foreign package", async (linux) => { await writeFile(join(linux, "packages", "other.exe"), "foreign"); await refreshManifest(linux); }, /unexpected native package/],
  ["changed signed Debian bytes", async (linux) => { await writeFile(join(linux, "packages", "setup.deb"), "changed after signing"); await refreshManifest(linux); }, /signing evidence does not bind/],
  ["invented Linux OS signing", async (linux) => { const file = join(linux, "SIGNING_VERIFICATION.json"); const value = JSON.parse(await readFile(file, "utf8")); value.method = "codesign"; await writeFile(file, JSON.stringify(value)); await refreshManifest(linux); }, /no Linux updater-signing evidence/],
];
for (const [label, mutate, expectedError] of rejectionCases) {
  test(`release curation rejects ${label}`, async () => {
    const temporary = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-reject-test-"));
    try {
      const input = join(temporary, "input");
      await mkdir(input);
      const linux = await writeArtifact(input, "hoi4-mod-setup-linux-x64", "linux", "X64", "setup.AppImage");
      await writeArtifact(input, "hoi4-mod-setup-windows-x64", "windows", "X64", "setup.exe");
      await writeArtifact(input, "hoi4-mod-setup-macos-arm64", "macos", "ARM64", "setup.dmg");
      await writeArtifact(input, "hoi4-mod-setup-macos-x64", "macos", "X64", "setup.dmg");
      await mutate(linux);
      const result = spawnSync(process.execPath, [join(root, "scripts", "prepare_release_assets.mjs"), input, join(temporary, "output")], {
        cwd: root, encoding: "utf8", windowsHide: true,
        env: { ...process.env, GITHUB_SHA: revision, GITHUB_REF_NAME: "v0.2.0" },
      });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, expectedError);
    } finally { await rm(temporary, { recursive: true, force: true }); }
  });
}
