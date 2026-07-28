import { createHash } from "node:crypto";
import { cp, mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, sep } from "node:path";

const root = resolve(import.meta.dirname, "..");
const inputRoot = resolve(root, process.argv[2] ?? "release-artifacts");
const outputRoot = resolve(root, process.argv[3] ?? "publication-assets");
const expected = new Map([
  ["hoi4-mod-setup-windows-x64", { platform: "windows", architecture: "X64", extension: ".exe" }],
  ["hoi4-mod-setup-macos-arm64", { platform: "macos", architecture: "ARM64", extension: ".dmg" }],
  ["hoi4-mod-setup-macos-x64", { platform: "macos", architecture: "X64", extension: ".dmg" }],
]);

if (!existsSync(inputRoot) || !(await stat(inputRoot)).isDirectory()) {
  throw new Error(`release artifact directory does not exist: ${inputRoot}`);
}
await rm(outputRoot, { recursive: true, force: true });
await mkdir(outputRoot, { recursive: true });

const inputEntries = (await readdir(inputRoot, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();
const expectedNames = [...expected.keys()].sort();
if (inputEntries.length !== expectedNames.length || inputEntries.some((name, index) => name !== expectedNames[index])) {
  throw new Error(`release artifact set must contain exactly ${expectedNames.join(", ")}; found ${inputEntries.join(", ")}`);
}

async function walk(directory, prefix = "") {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name;
    const absolutePath = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(absolutePath, relativePath));
    else if (entry.isFile()) files.push({ relative: relativePath.replaceAll(sep, "/"), absolute: absolutePath });
  }
  return files;
}

function assertSafeRelativePath(path, label) {
  if (typeof path !== "string" || !path || path.startsWith("/") || path.includes("\\") || path.split("/").includes("..")) {
    throw new Error(`${label} contains an unsafe relative path`);
  }
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function publicPackageName(descriptor) {
  const target = `${descriptor.platform}-${descriptor.architecture.toLowerCase()}`;
  return descriptor.platform === "windows"
    ? `HOI4-Mod-Setup-${target}-setup.exe`
    : `HOI4-Mod-Setup-${target}.dmg`;
}

const releaseRevision = process.env.GITHUB_SHA?.trim().toLowerCase();
const releaseTag = process.env.GITHUB_REF_NAME?.trim();
const platformSummaries = [];
const outputEntries = [];

for (const artifactDirectory of expectedNames) {
  const descriptor = expected.get(artifactDirectory);
  const sourceRoot = resolve(inputRoot, artifactDirectory);
  const files = await walk(sourceRoot);
  const metadataFiles = files.filter(({ relative: path }) => path === "BUILD_METADATA.json");
  const manifestFiles = files.filter(({ relative: path }) => path === "ARTIFACTS.sha256");
  const noticeFiles = files.filter(({ relative: path }) => path === "THIRD_PARTY_NOTICES.md");
  if (metadataFiles.length !== 1 || manifestFiles.length !== 1 || noticeFiles.length !== 1) {
    throw new Error(`${artifactDirectory} must contain one root BUILD_METADATA.json, ARTIFACTS.sha256, and THIRD_PARTY_NOTICES.md`);
  }
  const metadata = JSON.parse(await readFile(metadataFiles[0].absolute, "utf8"));
  if (metadata.product !== "HOI4 Mod Setup" || metadata.platform.toLowerCase() !== descriptor.platform || metadata.architecture.toUpperCase() !== descriptor.architecture) {
    throw new Error(`${artifactDirectory} metadata does not match its expected platform and architecture`);
  }
  if (metadata.signing !== "configured") throw new Error(`${artifactDirectory} is not signed for publication`);
  if (releaseRevision && metadata.sourceRevision?.toLowerCase() !== releaseRevision) {
    throw new Error(`${artifactDirectory} metadata source revision does not match GITHUB_SHA`);
  }
  if (releaseTag && metadata.version !== releaseTag.replace(/^v/, "")) {
    throw new Error(`${artifactDirectory} metadata version does not match the release tag`);
  }
  const manifest = JSON.parse(await readFile(manifestFiles[0].absolute, "utf8"));
  if (!Array.isArray(manifest) || manifest.length === 0) throw new Error(`${artifactDirectory} artifact manifest is empty`);
  const manifestPaths = new Set();
  for (const entry of manifest) {
    assertSafeRelativePath(entry?.path, `${artifactDirectory} artifact manifest`);
    if (manifestPaths.has(entry.path) || !/^[0-9a-f]{64}$/i.test(entry.sha256)) throw new Error(`${artifactDirectory} artifact manifest is invalid`);
    manifestPaths.add(entry.path);
    const file = files.find(({ relative: path }) => path === entry.path);
    if (!file || digest(await readFile(file.absolute)) !== entry.sha256.toLowerCase()) {
      throw new Error(`${artifactDirectory} artifact hash mismatch: ${entry.path}`);
    }
  }
  const packages = files.filter(({ relative: path }) => path.startsWith("packages/") && path.toLowerCase().endsWith(descriptor.extension));
  if (packages.length !== 1) throw new Error(`${artifactDirectory} must contain exactly one ${descriptor.extension} package`);
  const evidenceFiles = files.filter(({ relative: path }) => path === "SIGNING_VERIFICATION.json");
  if (evidenceFiles.length !== 1) throw new Error(`${artifactDirectory} is missing signing verification evidence`);
  const evidence = JSON.parse(await readFile(evidenceFiles[0].absolute, "utf8"));
  const packageRelative = packages[0].relative.replace(/^packages\//, "");
  const packageDigest = digest(await readFile(packages[0].absolute));
  if (evidence.source_revision?.toLowerCase() !== metadata.sourceRevision?.toLowerCase() || evidence.platform !== metadata.platform || evidence.architecture !== metadata.architecture || evidence.package_sha256?.[packageRelative] !== packageDigest) {
    throw new Error(`${artifactDirectory} signing evidence does not bind the verified package to its metadata`);
  }
  const packageName = publicPackageName(descriptor);
  const metadataName = `${artifactDirectory}-BUILD_METADATA.json`;
  const manifestName = `${artifactDirectory}-ARTIFACTS.sha256`;
  const evidenceName = `${artifactDirectory}-SIGNING_VERIFICATION.json`;
  await cp(packages[0].absolute, resolve(outputRoot, packageName));
  await cp(metadataFiles[0].absolute, resolve(outputRoot, metadataName));
  await cp(manifestFiles[0].absolute, resolve(outputRoot, manifestName));
  await cp(evidenceFiles[0].absolute, resolve(outputRoot, evidenceName));
  const publicationNotice = resolve(outputRoot, "THIRD_PARTY_NOTICES.md");
  if (existsSync(publicationNotice) && digest(await readFile(publicationNotice)) !== digest(await readFile(noticeFiles[0].absolute))) {
    throw new Error("platform third-party notice inventories do not match");
  }
  if (!existsSync(publicationNotice)) await cp(noticeFiles[0].absolute, publicationNotice);
  platformSummaries.push({
    artifact_directory: artifactDirectory,
    platform: descriptor.platform,
    architecture: descriptor.architecture,
    source_revision: metadata.sourceRevision,
    package: packageName,
    package_sha256: digest(await readFile(resolve(outputRoot, packageName))),
    metadata: metadataName,
    manifest: manifestName,
    signing_evidence: evidenceName,
  });
}

const provenance = {
  schema_version: "1.0.0",
  product: "HOI4 Mod Setup",
  source_revision: releaseRevision ?? platformSummaries[0]?.source_revision ?? "unresolved-local",
  release_tag: releaseTag ?? null,
  artifacts: platformSummaries,
  generated_by: "scripts/prepare_release_assets.mjs",
};
await writeFile(resolve(outputRoot, "RELEASE_PROVENANCE.json"), JSON.stringify(provenance, null, 2) + "\n", "utf8");

const downloadBase = releaseTag
  ? `https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/${encodeURIComponent(releaseTag)}`
  : null;
const downloadRows = platformSummaries.map((summary) => {
  const label = summary.platform === "windows"
    ? "Windows x64"
    : summary.architecture === "ARM64" ? "macOS Apple silicon" : "macOS Intel";
  const packageLink = downloadBase ? `[${summary.package}](${downloadBase}/${encodeURIComponent(summary.package)})` : `\`${summary.package}\``;
  const manifestLink = downloadBase ? `[manifest](${downloadBase}/${encodeURIComponent(summary.manifest)})` : `\`${summary.manifest}\``;
  return `| ${label} | ${packageLink} | ${summary.package_sha256} | ${manifestLink} |`;
});
const verificationLinks = downloadBase
  ? `[RELEASE_PROVENANCE.json](${downloadBase}/RELEASE_PROVENANCE.json), [RELEASE_ARTIFACTS.sha256](${downloadBase}/RELEASE_ARTIFACTS.sha256), and [THIRD_PARTY_NOTICES.md](${downloadBase}/THIRD_PARTY_NOTICES.md)`
  : "`RELEASE_PROVENANCE.json`, `RELEASE_ARTIFACTS.sha256`, and `THIRD_PARTY_NOTICES.md`";
await writeFile(resolve(outputRoot, "RELEASE_NOTES.md"), [
  "# HOI4 Mod Setup",
  "",
  "This release contains native installers built on the matching Windows and macOS runners from one exact Git commit.",
  "",
  "## Downloads",
  "",
  "| Platform | Installer | SHA-256 | Package manifest |",
  "| --- | --- | --- | --- |",
  ...downloadRows,
  "",
  "Windows uses the `.exe` installer. On macOS, open the `.dmg` and move the app to Applications.",
  "",
  `Verify ${verificationLinks} and the source tag before installing.`,
  "",
  "The source is public under the Apache License 2.0: <https://github.com/klimPaskov/HOI4-Mod-Setup>.",
].join("\n") + "\n", "utf8");

const outputFiles = (await walk(outputRoot)).filter(({ relative: path }) => path !== "RELEASE_ARTIFACTS.sha256").sort((left, right) => left.relative.localeCompare(right.relative));
const outputManifest = [];
for (const file of outputFiles) outputManifest.push({ path: file.relative, sha256: digest(await readFile(file.absolute)) });
await writeFile(resolve(outputRoot, "RELEASE_ARTIFACTS.sha256"), JSON.stringify(outputManifest, null, 2) + "\n", "utf8");
console.log(`Prepared ${outputManifest.length} uniquely named release assets for ${platformSummaries.length} platforms.`);
