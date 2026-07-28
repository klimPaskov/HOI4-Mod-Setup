import { createHash } from "node:crypto";
import { cp, mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, sep } from "node:path";

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

function mergeSbomInventories(inventories) {
  const canonical = inventories[0]?.sbom;
  if (!canonical) throw new Error("preview SBOM inventory set is empty");
  const canonicalHeader = JSON.stringify({ ...canonical, components: [] });
  const components = new Map();
  for (const { artifactDirectory, sbom } of inventories) {
    if (JSON.stringify({ ...sbom, components: [] }) !== canonicalHeader) {
      throw new Error(`${artifactDirectory} SBOM metadata differs from the canonical inventory`);
    }
    for (const component of sbom.components) {
      const reference = component?.["bom-ref"];
      if (typeof reference !== "string" || !reference) throw new Error(`${artifactDirectory} SBOM contains a component without bom-ref`);
      const serialized = JSON.stringify(component);
      const existing = components.get(reference);
      if (existing && existing.serialized !== serialized) throw new Error(`${artifactDirectory} SBOM has conflicting component ${reference}`);
      components.set(reference, { component, serialized });
    }
  }
  return {
    ...canonical,
    components: [...components.values()].map(({ component }) => component).sort((left, right) => left["bom-ref"].localeCompare(right["bom-ref"])),
  };
}

function mergeThirdPartyNotices(inventories) {
  const canonical = inventories[0]?.notice;
  if (!canonical) throw new Error("preview third-party notice inventory set is empty");
  const javascriptRows = new Map();
  const rustRows = new Map();
  for (const { artifactDirectory, notice } of inventories) {
    let section = null;
    for (const line of notice.split(/\r?\n/)) {
      if (line === "## JavaScript dependencies") section = javascriptRows;
      else if (line === "## Rust dependencies") section = rustRows;
      else if (section && line.startsWith("| ") && !line.startsWith("| ---") && !line.startsWith("| Package |")) {
        const cells = line.slice(2, -2).split(" | ");
        if (cells.length !== 4) throw new Error(`${artifactDirectory} third-party notice row is invalid`);
        const key = `${cells[0]}@${cells[1]}`;
        const existing = section.get(key);
        if (existing && existing !== line) throw new Error(`${artifactDirectory} has conflicting third-party notice row ${key}`);
        section.set(key, line);
      }
    }
  }
  const preamble = canonical.split(/\r?\n/).slice(0, 7);
  return [
    ...preamble,
    "",
    `Inventory counts: ${javascriptRows.size} JavaScript packages and ${rustRows.size} Rust packages.`,
    "",
    "## JavaScript dependencies",
    "",
    "| Package | Version | License | Homepage |",
    "| --- | --- | --- | --- |",
    ...[...javascriptRows.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([, row]) => row),
    "",
    "## Rust dependencies",
    "",
    "| Package | Version | License | Source |",
    "| --- | --- | --- | --- |",
    ...[...rustRows.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([, row]) => row),
    "",
  ].join("\n");
}

function publicPackageName(descriptor) {
  const target = `${descriptor.platform}-${descriptor.architecture.toLowerCase()}`;
  return descriptor.platform === "windows"
    ? `HOI4-Mod-Setup-${target}-setup.exe`
    : `HOI4-Mod-Setup-${target}.dmg`;
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
const expectedPreviewTag = `preview-${sourceRevision}`;
if (previewTag !== expectedPreviewTag) {
  throw new Error(`preview publication tag must be exactly ${expectedPreviewTag}`);
}
const inputEntries = (await readdir(inputRoot, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();
const expectedEntries = [...expected.keys()].sort();
if (inputEntries.length !== expectedEntries.length || inputEntries.some((name, index) => name !== expectedEntries[index])) {
  throw new Error(`preview artifact set must contain exactly ${expectedEntries.join(", ")}; found ${inputEntries.join(", ")}`);
}

const summaries = [];
const sbomInventories = [];
const noticeInventories = [];
let previewVersion = null;
for (const artifactDirectory of expectedEntries) {
  const descriptor = expected.get(artifactDirectory);
  const sourceRoot = resolve(inputRoot, artifactDirectory);
  const files = await walk(sourceRoot);
  const metadataFiles = files.filter(({ relative: path }) => path === "BUILD_METADATA.json");
  const manifestFiles = files.filter(({ relative: path }) => path === "ARTIFACTS.sha256");
  const noticeFiles = files.filter(({ relative: path }) => path === "THIRD_PARTY_NOTICES.md");
  const sbomFiles = files.filter(({ relative: path }) => path === "SBOM.cdx.json");
  if (metadataFiles.length !== 1 || manifestFiles.length !== 1 || noticeFiles.length !== 1 || sbomFiles.length !== 1) {
    throw new Error(`${artifactDirectory} must contain BUILD_METADATA.json, ARTIFACTS.sha256, THIRD_PARTY_NOTICES.md, and SBOM.cdx.json`);
  }
  const sbom = JSON.parse(await readFile(sbomFiles[0].absolute, "utf8"));
  if (sbom.bomFormat !== "CycloneDX" || sbom.specVersion !== "1.5" || !Array.isArray(sbom.components) || sbom.components.length === 0) {
    throw new Error(`${artifactDirectory} SBOM is not a populated CycloneDX 1.5 document`);
  }
  sbomInventories.push({ artifactDirectory, sbom });
  const metadata = JSON.parse(await readFile(metadataFiles[0].absolute, "utf8"));
  noticeInventories.push({ artifactDirectory, notice: await readFile(noticeFiles[0].absolute, "utf8") });
  if (metadata.product !== "HOI4 Mod Setup" || metadata.frontendOnly || metadata.platform.toLowerCase() !== descriptor.platform || metadata.architecture.toUpperCase() !== descriptor.architecture) {
    throw new Error(`${artifactDirectory} metadata does not match its native preview target`);
  }
  if (metadata.signing !== "not_configured") {
    throw new Error(`${artifactDirectory} preview metadata must report signing=not_configured`);
  }
  if (previewVersion === null) previewVersion = metadata.version;
  else if (metadata.version !== previewVersion) throw new Error("preview platform metadata versions do not match");
  if (metadata.sourceRevision?.toLowerCase() !== sourceRevision) {
    throw new Error(`${artifactDirectory} metadata source revision does not match GITHUB_SHA`);
  }
  const manifest = JSON.parse(await readFile(manifestFiles[0].absolute, "utf8"));
  if (!Array.isArray(manifest) || manifest.length === 0) throw new Error(`${artifactDirectory} artifact manifest is empty`);
  const manifestPaths = new Set();
  for (const entry of manifest) {
    assertSafeRelativePath(entry?.path, `${artifactDirectory} artifact manifest`);
    if (entry.path === "ARTIFACTS.sha256" || manifestPaths.has(entry.path) || !/^[0-9a-f]{64}$/i.test(entry.sha256)) throw new Error(`${artifactDirectory} artifact manifest is invalid`);
    manifestPaths.add(entry.path);
    const file = files.find(({ relative: path }) => path === entry.path);
    if (!file || digest(await readFile(file.absolute)) !== entry.sha256.toLowerCase()) {
      throw new Error(`${artifactDirectory} artifact hash mismatch: ${entry.path}`);
    }
  }
  const sourceFilePaths = files.filter(({ relative: path }) => path !== "ARTIFACTS.sha256").map(({ relative: path }) => path);
  if (manifestPaths.size !== sourceFilePaths.length || sourceFilePaths.some((path) => !manifestPaths.has(path))) {
    throw new Error(`${artifactDirectory} artifact manifest does not cover the complete artifact directory`);
  }
  const packages = files.filter(({ relative: path }) => path.startsWith("packages/") && path.toLowerCase().endsWith(descriptor.extension));
  if (packages.length !== 1) throw new Error(`${artifactDirectory} must contain exactly one ${descriptor.extension} package`);
  const packageName = publicPackageName(descriptor);
  const metadataName = `${artifactDirectory}-BUILD_METADATA.json`;
  const manifestName = `${artifactDirectory}-ARTIFACTS.sha256`;
  await cp(packages[0].absolute, resolve(outputRoot, packageName));
  await cp(metadataFiles[0].absolute, resolve(outputRoot, metadataName));
  await cp(manifestFiles[0].absolute, resolve(outputRoot, manifestName));
  summaries.push({
    artifact_directory: artifactDirectory,
    platform: descriptor.platform,
    architecture: descriptor.architecture,
    source_revision: metadata.sourceRevision,
    signing: metadata.signing,
    package: packageName,
    package_sha256: digest(await readFile(resolve(outputRoot, packageName))),
    metadata: metadataName,
    manifest: manifestName,
  });
}

await writeFile(resolve(outputRoot, "SBOM.cdx.json"), JSON.stringify(mergeSbomInventories(sbomInventories), null, 2) + "\n", "utf8");
await writeFile(resolve(outputRoot, "THIRD_PARTY_NOTICES.md"), mergeThirdPartyNotices(noticeInventories), "utf8");

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
  sbom: "SBOM.cdx.json",
  artifacts: summaries,
  generated_by: "scripts/prepare_preview_assets.mjs",
};
await writeFile(resolve(outputRoot, "PREVIEW_PROVENANCE.json"), JSON.stringify(provenance, null, 2) + "\n", "utf8");

const downloadBase = previewTag
  ? `https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/${encodeURIComponent(previewTag)}`
  : null;
const downloadRows = summaries.map((summary) => {
  const label = summary.platform === "windows"
    ? "Windows x64"
    : summary.architecture === "ARM64" ? "macOS Apple silicon" : "macOS Intel";
  const packageLink = downloadBase ? `[${summary.package}](${downloadBase}/${encodeURIComponent(summary.package)})` : `\`${summary.package}\``;
  const manifestLink = downloadBase ? `[manifest](${downloadBase}/${encodeURIComponent(summary.manifest)})` : `\`${summary.manifest}\``;
  return `| ${label} | ${packageLink} | ${summary.package_sha256} | ${manifestLink} |`;
});
const verificationLinks = downloadBase
  ? `[PREVIEW_PROVENANCE.json](${downloadBase}/PREVIEW_PROVENANCE.json), [PREVIEW_ARTIFACTS.sha256](${downloadBase}/PREVIEW_ARTIFACTS.sha256), [SBOM.cdx.json](${downloadBase}/SBOM.cdx.json), and [THIRD_PARTY_NOTICES.md](${downloadBase}/THIRD_PARTY_NOTICES.md)`
  : "`PREVIEW_PROVENANCE.json`, `PREVIEW_ARTIFACTS.sha256`, `SBOM.cdx.json`, and `THIRD_PARTY_NOTICES.md`";
await writeFile(resolve(outputRoot, "PREVIEW_RELEASE_NOTES.md"), [
  "# HOI4 Mod Setup development preview",
  "",
  "This prerelease is built from one exact public Git commit and includes native installers for Windows and macOS.",
  "",
  "## Downloads",
  "",
  "| Platform | Installer | SHA-256 | Package manifest |",
  "| --- | --- | --- | --- |",
  ...downloadRows,
  "",
  "Windows uses the `.exe` installer. On macOS, open the `.dmg` and move the app to Applications.",
  "",
  `This is a development preview, not a stable release. Windows and macOS may show a platform security warning because stable publisher signing and notarization are separate release gates. Verify ${verificationLinks} and the source commit before installing.`,
  "",
  "The source is public under the Apache License 2.0: <https://github.com/klimPaskov/HOI4-Mod-Setup>.",
].join("\n") + "\n", "utf8");

const outputFiles = (await walk(outputRoot)).filter(({ relative: path }) => path !== "PREVIEW_ARTIFACTS.sha256").sort((left, right) => left.relative.localeCompare(right.relative));
const outputManifest = [];
for (const file of outputFiles) outputManifest.push({ path: file.relative, sha256: digest(await readFile(file.absolute)) });
await writeFile(resolve(outputRoot, "PREVIEW_ARTIFACTS.sha256"), JSON.stringify(outputManifest, null, 2) + "\n", "utf8");
console.log(`Prepared ${outputManifest.length} development-preview assets for ${summaries.length} platforms.`);
