import { createHash } from "node:crypto";
import { cp, mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, basename, sep } from "node:path";

const root = resolve(import.meta.dirname, "..");
const inputRoot = resolve(root, process.argv[2] ?? "preview-artifacts");
const outputRoot = resolve(root, process.argv[3] ?? "publication-assets");
const expected = new Map([
  ["hoi4-mod-setup-preview-windows-x64", { platform: "windows", architecture: "X64", extension: ".exe" }],
  ["hoi4-mod-setup-preview-macos-arm64", { platform: "macos", architecture: "ARM64", extension: ".dmg" }],
  ["hoi4-mod-setup-preview-macos-x64", { platform: "macos", architecture: "X64", extension: ".dmg" }],
]);

if (!existsSync(inputRoot) || !(await stat(inputRoot)).isDirectory()) {
  throw new Error(`preview artifact directory does not exist: ${inputRoot}`);
}
await rm(outputRoot, { recursive: true, force: true });
await mkdir(outputRoot, { recursive: true });

async function walk(directory, prefix = "") {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    const absolute = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(absolute, path));
    else if (entry.isFile()) files.push({ relative: path.replaceAll(sep, "/"), absolute });
  }
  return files;
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function assertSafeRelativePath(path, label) {
  if (typeof path !== "string" || !path || path.startsWith("/") || path.includes("\\") || path.split("/").includes("..")) {
    throw new Error(`${label} contains an unsafe relative path`);
  }
}

const sourceRevision = process.env.GITHUB_SHA?.trim().toLowerCase();
if (!/^[0-9a-f]{40}$/.test(sourceRevision ?? "")) {
  throw new Error("preview publication requires the exact source commit in GITHUB_SHA");
}
const previewTag = process.env.PREVIEW_TAG?.trim() || null;
const inputEntries = (await readdir(inputRoot, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();
const expectedEntries = [...expected.keys()].sort();
if (inputEntries.length !== expectedEntries.length || inputEntries.some((name, index) => name !== expectedEntries[index])) {
  throw new Error(`preview artifact set must contain exactly ${expectedEntries.join(", ")}; found ${inputEntries.join(", ")}`);
}

const summaries = [];
for (const artifactDirectory of expectedEntries) {
  const descriptor = expected.get(artifactDirectory);
  const sourceRoot = resolve(inputRoot, artifactDirectory);
  const files = await walk(sourceRoot);
  const metadataFiles = files.filter(({ relative: path }) => path === "BUILD_METADATA.json");
  const manifestFiles = files.filter(({ relative: path }) => path === "ARTIFACTS.sha256");
  const noticeFiles = files.filter(({ relative: path }) => path === "THIRD_PARTY_NOTICES.md");
  if (metadataFiles.length !== 1 || manifestFiles.length !== 1 || noticeFiles.length !== 1) {
    throw new Error(`${artifactDirectory} must contain BUILD_METADATA.json, ARTIFACTS.sha256, and THIRD_PARTY_NOTICES.md`);
  }
  const metadata = JSON.parse(await readFile(metadataFiles[0].absolute, "utf8"));
  if (metadata.product !== "HOI4 Mod Setup" || metadata.frontendOnly || metadata.platform.toLowerCase() !== descriptor.platform || metadata.architecture.toUpperCase() !== descriptor.architecture) {
    throw new Error(`${artifactDirectory} metadata does not match its native preview target`);
  }
  if (metadata.sourceRevision?.toLowerCase() !== sourceRevision) {
    throw new Error(`${artifactDirectory} metadata source revision does not match GITHUB_SHA`);
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
  const packageName = `${artifactDirectory}-${basename(packages[0].absolute)}`;
  await cp(packages[0].absolute, resolve(outputRoot, packageName));
  await cp(metadataFiles[0].absolute, resolve(outputRoot, `${artifactDirectory}-BUILD_METADATA.json`));
  await cp(manifestFiles[0].absolute, resolve(outputRoot, `${artifactDirectory}-ARTIFACTS.sha256`));
  const noticeDestination = resolve(outputRoot, "THIRD_PARTY_NOTICES.md");
  if (existsSync(noticeDestination) && digest(await readFile(noticeDestination)) !== digest(await readFile(noticeFiles[0].absolute))) {
    throw new Error("platform third-party notice inventories do not match");
  }
  if (!existsSync(noticeDestination)) await cp(noticeFiles[0].absolute, noticeDestination);
  summaries.push({
    artifact_directory: artifactDirectory,
    platform: descriptor.platform,
    architecture: descriptor.architecture,
    source_revision: metadata.sourceRevision,
    signing: metadata.signing,
    package: packageName,
    package_sha256: digest(await readFile(resolve(outputRoot, packageName))),
  });
}

await writeFile(resolve(outputRoot, "PREVIEW_NOTICE.md"), [
  "HOI4 Mod Setup development preview",
  "",
  "This is a source-built development preview, not a stable release. Windows and macOS may display a platform security warning because the preview is not published with the stable release signing identities. Verify the GitHub source commit and SHA-256 manifest before installing.",
  "",
  "The Apache 2.0 source is available at https://github.com/klimPaskov/HOI4-Mod-Setup.",
  "",
].join("\n"), "utf8");

const provenance = {
  schema_version: "1.0.0",
  product: "HOI4 Mod Setup",
  source_revision: sourceRevision,
  preview_tag: previewTag,
  artifacts: summaries,
  generated_by: "scripts/prepare_preview_assets.mjs",
};
await writeFile(resolve(outputRoot, "PREVIEW_PROVENANCE.json"), JSON.stringify(provenance, null, 2) + "\n", "utf8");

const outputFiles = (await walk(outputRoot)).filter(({ relative: path }) => path !== "PREVIEW_ARTIFACTS.sha256").sort((left, right) => left.relative.localeCompare(right.relative));
const outputManifest = [];
for (const file of outputFiles) outputManifest.push({ path: file.relative, sha256: digest(await readFile(file.absolute)) });
await writeFile(resolve(outputRoot, "PREVIEW_ARTIFACTS.sha256"), JSON.stringify(outputManifest, null, 2) + "\n", "utf8");
console.log(`Prepared ${outputManifest.length} development-preview assets for ${summaries.length} platforms.`);
