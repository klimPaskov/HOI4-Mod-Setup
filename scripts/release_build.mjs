import { cp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageMetadata = JSON.parse(await readFile(resolve(root, "package.json"), "utf8"));
const tauriMetadata = JSON.parse(await readFile(resolve(root, "src-tauri", "tauri.conf.json"), "utf8"));
const cargoMetadata = await readFile(resolve(root, "src-tauri", "Cargo.toml"), "utf8");
const cargoLock = await readFile(resolve(root, "Cargo.lock"), "utf8");
const configuredVersion = packageMetadata.version;
if (typeof configuredVersion !== "string" || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(configuredVersion)) {
  throw new Error("package.json must declare a semantic release version");
}
if (tauriMetadata.version !== configuredVersion) {
  throw new Error("package.json and src-tauri/tauri.conf.json versions must match");
}
const cargoVersion = cargoMetadata.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const cargoPackage = cargoLock.split("[[package]]").find((entry) => /^\s*name\s*=\s*"hoi4-mod-setup"/m.test(entry));
const lockVersion = cargoPackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (cargoVersion !== configuredVersion || lockVersion !== configuredVersion) {
  throw new Error("package.json, src-tauri/Cargo.toml, and Cargo.lock versions must match");
}
const tagVersion = process.env.GITHUB_REF?.startsWith("refs/tags/") && process.env.GITHUB_REF_NAME?.startsWith("v")
  ? process.env.GITHUB_REF_NAME.slice(1)
  : undefined;
if (tagVersion && tagVersion !== configuredVersion) {
  throw new Error(`release tag ${process.env.GITHUB_REF_NAME} does not match configured version ${configuredVersion}`);
}
const tauriBuild = process.env.HOI4_MOD_SETUP_TAURI === "1";
const releaseBuild = process.env.HOI4_MOD_SETUP_RELEASE === "1";
const requestedBundle = process.env.HOI4_MOD_SETUP_BUNDLE;
if (requestedBundle && !["nsis", "dmg"].includes(requestedBundle)) {
  throw new Error(`unsupported native bundle target: ${requestedBundle}`);
}
const buildArgs = tauriBuild
  ? ["tauri", "build", ...(requestedBundle ? ["--bundles", requestedBundle] : [])]
  : ["build"];
const tauriConfig = process.env.HOI4_MOD_SETUP_TAURI_CONFIG?.trim();
if (tauriBuild && tauriConfig) {
  if (!/^(?:[A-Za-z]:[\\/]|[\\/])/.test(tauriConfig)) {
    throw new Error("HOI4_MOD_SETUP_TAURI_CONFIG must be an absolute runner-local path");
  }
  if (!existsSync(tauriConfig)) {
    throw new Error(`Tauri release config does not exist: ${tauriConfig}`);
  }
  buildArgs.push("--config", tauriConfig);
}
const command = process.platform === "win32" ? (process.env.ComSpec ?? "cmd.exe") : "pnpm";
function windowsCommandArgument(value) {
  if (/[%!]/.test(value)) {
    throw new Error("Windows release arguments may not contain cmd expansion characters");
  }
  return /^[A-Za-z0-9_./:\\-]+$/.test(value) ? value : `"${value.replaceAll('"', '""')}"`;
}
const args = process.platform === "win32"
  ? ["/d", "/s", "/c", ["pnpm", ...buildArgs].map(windowsCommandArgument).join(" ")]
  : buildArgs;
function gitRevision(ref = "HEAD") {
  const revision = spawnSync("git", ["rev-parse", ref], {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  return revision.status === 0 ? revision.stdout.trim().toLowerCase() : undefined;
}
const headRevision = gitRevision();
const sourceRevision = process.env.GITHUB_SHA?.trim().toLowerCase() || headRevision;
if (releaseBuild) {
  if (!/^[0-9a-f]{40}$/.test(process.env.GITHUB_SHA?.trim().toLowerCase() ?? "")) {
    throw new Error("release build requires an exact checked-out commit revision");
  }
  if (headRevision !== process.env.GITHUB_SHA.trim().toLowerCase()) {
    throw new Error(`release source revision ${sourceRevision} does not match checked-out HEAD ${headRevision}`);
  }
  const tag = process.env.GITHUB_REF_NAME;
  if (!process.env.GITHUB_REF?.startsWith("refs/tags/") || !tag || !/^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
    throw new Error("release build requires a semantic version tag ref");
  }
  if (process.env.GITHUB_REF !== `refs/tags/${tag}`) {
    throw new Error("release build ref does not match the release tag");
  }
  const tagRevision = gitRevision(`refs/tags/${tag}^{commit}`);
  if (tagRevision !== sourceRevision) {
    throw new Error(`release tag ${tag} does not point to checked-out revision ${sourceRevision}`);
  }
  const status = spawnSync("git", ["status", "--porcelain", "--untracked-files=all"], {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (status.status !== 0 || status.stdout.trim()) {
    throw new Error("release build requires a clean checked-out worktree");
  }
}
if (tauriBuild && requestedBundle) {
  // Tauri can leave packages from an earlier local build in the target tree.
  // Clear only the requested generated bundle before rebuilding so stale
  // installers cannot be copied into the current release evidence.
  for (const bundlePath of [
    resolve(root, "target", "release", "bundle", requestedBundle),
    resolve(root, "src-tauri", "target", "release", "bundle", requestedBundle),
  ]) {
    await rm(bundlePath, { recursive: true, force: true });
  }
}
const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
if (!existsSync(resolve(root, "dist", "index.html"))) throw new Error("frontend build did not produce dist/index.html");
const releaseRootPath = resolve(root, "dist", "release");
// Release output is generated state. Clear it before copying so a repeated
// build cannot silently carry an artifact from a different platform or commit.
await rm(releaseRootPath, { recursive: true, force: true });
const releaseRoot = resolve(releaseRootPath, "frontend");
await mkdir(releaseRoot, { recursive: true });
await cp(resolve(root, "dist", "index.html"), resolve(releaseRoot, "index.html"), { force: true });
if (existsSync(resolve(root, "dist", "assets"))) {
  await cp(resolve(root, "dist", "assets"), resolve(releaseRoot, "assets"), { recursive: true, force: true });
}
const packageRoot = resolve(root, "dist", "release", "packages");
if (tauriBuild) {
  const bundleRoot = [
    resolve(root, "target", "release", "bundle"),
    resolve(root, "src-tauri", "target", "release", "bundle"),
  ].find((candidate) => existsSync(candidate));
  if (!bundleRoot) throw new Error("Tauri build did not produce a bundle directory");
  const selectedBundle = requestedBundle ? resolve(bundleRoot, requestedBundle) : bundleRoot;
  if (!existsSync(selectedBundle)) {
    throw new Error(`Tauri build did not produce the requested ${requestedBundle} bundle`);
  }
  await mkdir(packageRoot, { recursive: true });
  await cp(
    selectedBundle,
    requestedBundle ? resolve(packageRoot, requestedBundle) : packageRoot,
    { recursive: true, force: true },
  );
}
const noticeResult = spawnSync(process.execPath, [resolve(root, "scripts", "generate_third_party_notices.mjs"), resolve(root, "dist", "release", "THIRD_PARTY_NOTICES.md")], {
  cwd: root,
  stdio: "inherit",
});
if (noticeResult.error) throw noticeResult.error;
if (noticeResult.status !== 0) process.exit(noticeResult.status ?? 1);
const sbomResult = spawnSync(process.execPath, [resolve(root, "scripts", "generate_sbom.mjs"), resolve(root, "dist", "release", "SBOM.cdx.json")], {
  cwd: root,
  stdio: "inherit",
});
if (sbomResult.error) throw sbomResult.error;
if (sbomResult.status !== 0) process.exit(sbomResult.status ?? 1);
async function walk(directory, prefix = "") {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    const absolute = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(absolute, relative));
    else if (entry.isFile()) files.push({ relative, absolute });
  }
  return files;
}
const artifactFiles = (await walk(releaseRootPath))
  .filter(({ relative }) => relative !== "ARTIFACTS.sha256")
  .sort((left, right) => left.relative.localeCompare(right.relative));
const artifactManifest = [];
for (const artifact of artifactFiles) {
  const digest = createHash("sha256").update(await readFile(artifact.absolute)).digest("hex");
  artifactManifest.push({ path: artifact.relative, sha256: digest });
}
await writeFile(resolve(releaseRootPath, "ARTIFACTS.sha256"), JSON.stringify(artifactManifest, null, 2) + "\n", "utf8");
await writeFile(resolve(root, "dist", "release", "BUILD_METADATA.json"), JSON.stringify({
  product: "HOI4 Mod Setup",
  version: configuredVersion,
  frontendOnly: !tauriBuild,
  tauriPackaging: tauriBuild ? "built by the selected platform runner; signing state is reported separately" : "not requested",
  bundle: requestedBundle ?? "all",
  sourceRevision: sourceRevision ?? "unresolved-local",
  platform: process.env.RUNNER_OS ?? process.platform,
  architecture: process.env.RUNNER_ARCH ?? "unresolved-local",
  signing: process.env.HOI4_MOD_SETUP_SIGNING_CONFIGURED === "1" ? "configured" : "not_configured",
  artifact_manifest: "ARTIFACTS.sha256",
}, null, 2) + "\n", "utf8");
const metadataBytes = await readFile(resolve(releaseRootPath, "BUILD_METADATA.json"));
artifactManifest.push({
  path: "BUILD_METADATA.json",
  sha256: createHash("sha256").update(metadataBytes).digest("hex"),
});
artifactManifest.sort((left, right) => left.path.localeCompare(right.path));
await writeFile(resolve(releaseRootPath, "ARTIFACTS.sha256"), JSON.stringify(artifactManifest, null, 2) + "\n", "utf8");
