import { createHash } from "node:crypto";
import { cp, mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, sep } from "node:path";

const root = resolve(import.meta.dirname, "..");
const inputRoot = resolve(root, process.argv[2] ?? "release-artifacts");
const outputRoot = resolve(root, process.argv[3] ?? "publication-assets");
const expected = new Map([
  ["hoi4-mod-setup-windows-x64", { platform: "windows", architecture: "X64", extension: ".exe", updaterPlatform: "windows-x86_64" }],
  ["hoi4-mod-setup-macos-arm64", { platform: "macos", architecture: "ARM64", extension: ".dmg", updaterPlatform: "darwin-aarch64" }],
  ["hoi4-mod-setup-macos-x64", { platform: "macos", architecture: "X64", extension: ".dmg", updaterPlatform: "darwin-x86_64" }],
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

function mergeSbomInventories(inventories) {
  const canonical = inventories[0]?.sbom;
  if (!canonical) throw new Error("release SBOM inventory set is empty");
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
  if (!canonical) throw new Error("release third-party notice inventory set is empty");
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

const releaseRevision = process.env.GITHUB_SHA?.trim().toLowerCase();
const releaseTag = process.env.GITHUB_REF_NAME?.trim();
const downloadBase = releaseTag
  ? `https://github.com/klimPaskov/HOI4-Mod-Setup/releases/download/${encodeURIComponent(releaseTag)}`
  : null;
const releaseVersion = releaseTag?.replace(/^v/, "") ?? "release";
const platformSummaries = [];
const sbomInventories = [];
const noticeInventories = [];
const updaterPlatforms = {};

for (const artifactDirectory of expectedNames) {
  const descriptor = expected.get(artifactDirectory);
  const sourceRoot = resolve(inputRoot, artifactDirectory);
  const files = await walk(sourceRoot);
  const metadataFiles = files.filter(({ relative: path }) => path === "BUILD_METADATA.json");
  const manifestFiles = files.filter(({ relative: path }) => path === "ARTIFACTS.sha256");
  const noticeFiles = files.filter(({ relative: path }) => path === "THIRD_PARTY_NOTICES.md");
  const sbomFiles = files.filter(({ relative: path }) => path === "SBOM.cdx.json");
  if (metadataFiles.length !== 1 || manifestFiles.length !== 1 || noticeFiles.length !== 1 || sbomFiles.length !== 1) {
    throw new Error(`${artifactDirectory} must contain one root BUILD_METADATA.json, ARTIFACTS.sha256, THIRD_PARTY_NOTICES.md, and SBOM.cdx.json`);
  }
  const sbom = JSON.parse(await readFile(sbomFiles[0].absolute, "utf8"));
  if (sbom.bomFormat !== "CycloneDX" || sbom.specVersion !== "1.5" || !Array.isArray(sbom.components) || sbom.components.length === 0) {
    throw new Error(`${artifactDirectory} SBOM is not a populated CycloneDX 1.5 document`);
  }
  sbomInventories.push({ artifactDirectory, sbom });
  const metadata = JSON.parse(await readFile(metadataFiles[0].absolute, "utf8"));
  noticeInventories.push({ artifactDirectory, notice: await readFile(noticeFiles[0].absolute, "utf8") });
  if (metadata.product !== "HOI4 Mod Setup" || metadata.platform.toLowerCase() !== descriptor.platform || metadata.architecture.toUpperCase() !== descriptor.architecture) {
    throw new Error(`${artifactDirectory} metadata does not match its expected platform and architecture`);
  }
  if (!["configured", "community"].includes(metadata.signing)) {
    throw new Error(`${artifactDirectory} is not signed for publication`);
  }
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
  const evidenceFiles = files.filter(({ relative: path }) => path === "SIGNING_VERIFICATION.json");
  if (evidenceFiles.length !== 1) throw new Error(`${artifactDirectory} is missing signing verification evidence`);
  const evidence = JSON.parse(await readFile(evidenceFiles[0].absolute, "utf8"));
  const packageRelative = packages[0].relative.replace(/^packages\//, "");
  const packageDigest = digest(await readFile(packages[0].absolute));
  if (evidence.source_revision?.toLowerCase() !== metadata.sourceRevision?.toLowerCase() || evidence.platform !== metadata.platform || evidence.architecture !== metadata.architecture || evidence.package_sha256?.[packageRelative] !== packageDigest) {
    throw new Error(`${artifactDirectory} signing evidence does not bind the verified package to its metadata`);
  }
  const packageName = publicPackageName(descriptor);
  await cp(packages[0].absolute, resolve(outputRoot, packageName));
  const updaterBundle = descriptor.platform === "windows"
    ? packages[0]
    : files.find(({ relative: path }) => path.startsWith("packages/") && path.endsWith(".app.tar.gz"));
  if (!updaterBundle) throw new Error(`${artifactDirectory} is missing its updater bundle`);
  const updaterSignature = files.find(({ relative: path }) => path === `${updaterBundle.relative}.sig`);
  if (!updaterSignature) throw new Error(`${artifactDirectory} is missing its updater signature`);
  const signature = (await readFile(updaterSignature.absolute, "utf8")).trim();
  if (!signature || signature.length > 16_384) throw new Error(`${artifactDirectory} updater signature is invalid`);
  const updaterName = descriptor.platform === "windows"
    ? packageName
    : `HOI4-Mod-Setup-${descriptor.platform}-${descriptor.architecture.toLowerCase()}.app.tar.gz`;
  if (descriptor.platform !== "windows") await cp(updaterBundle.absolute, resolve(outputRoot, updaterName));
  updaterPlatforms[descriptor.updaterPlatform] = {
    signature,
    url: downloadBase ? `${downloadBase}/${encodeURIComponent(updaterName)}` : updaterName,
  };
  platformSummaries.push({
    artifact_directory: artifactDirectory,
    platform: descriptor.platform,
    architecture: descriptor.architecture,
    source_revision: metadata.sourceRevision,
    package: packageName,
    package_sha256: digest(await readFile(resolve(outputRoot, packageName))),
  });
}

mergeSbomInventories(sbomInventories);
mergeThirdPartyNotices(noticeInventories);

await writeFile(resolve(outputRoot, "latest.json"), JSON.stringify({
  version: releaseVersion,
  notes: "A newer version of HOI4 Mod Setup is available.",
  platforms: updaterPlatforms,
}, null, 2) + "\n", "utf8");
const downloadBullets = platformSummaries.map((summary) => {
  const label = summary.platform === "windows"
    ? "Windows"
    : summary.architecture === "ARM64" ? "Mac (Apple silicon)" : "Mac (Intel)";
  const packageLink = downloadBase ? `[${summary.package}](${downloadBase}/${encodeURIComponent(summary.package)})` : `\`${summary.package}\``;
  return `- ${label}: ${packageLink}`;
});
await writeFile(resolve(outputRoot, "RELEASE_NOTES.md"), [
  `# HOI4 Mod Setup ${releaseVersion}`,
  "",
  "Download the installer for your computer:",
  "",
  ...downloadBullets,
  "",
  "Windows uses the `.exe` installer. On macOS, open the `.dmg` and move the app to Applications.",
  "",
  "Your computer may occasionally flag a new community build as harmful. This can be a false positive; this is the app's official open-source GitHub release.",
  "",
  "The source is public under the Apache License 2.0: <https://github.com/klimPaskov/HOI4-Mod-Setup>.",
].join("\n") + "\n", "utf8");

console.log(`Prepared ${platformSummaries.length} user-facing installers for ${platformSummaries.length} platforms.`);
