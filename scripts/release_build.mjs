import { cp, mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageMetadata = JSON.parse(await readFile(resolve(root, "package.json"), "utf8"));
const tauriMetadata = JSON.parse(await readFile(resolve(root, "src-tauri", "tauri.conf.json"), "utf8"));
const configuredVersion = packageMetadata.version;
if (typeof configuredVersion !== "string" || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(configuredVersion)) {
  throw new Error("package.json must declare a semantic release version");
}
if (tauriMetadata.version !== configuredVersion) {
  throw new Error("package.json and src-tauri/tauri.conf.json versions must match");
}
const tagVersion = process.env.GITHUB_REF_NAME?.startsWith("v")
  ? process.env.GITHUB_REF_NAME.slice(1)
  : undefined;
if (tagVersion && tagVersion !== configuredVersion) {
  throw new Error(`release tag ${process.env.GITHUB_REF_NAME} does not match configured version ${configuredVersion}`);
}
const tauriBuild = process.env.HOI4_MOD_SETUP_TAURI === "1";
const requestedBundle = process.env.HOI4_MOD_SETUP_BUNDLE;
if (requestedBundle && !["msi", "dmg"].includes(requestedBundle)) {
  throw new Error(`unsupported native bundle target: ${requestedBundle}`);
}
const buildArgs = tauriBuild
  ? ["tauri", "build", ...(requestedBundle ? ["--bundles", requestedBundle] : [])]
  : ["build"];
const command = process.platform === "win32" ? (process.env.ComSpec ?? "cmd.exe") : "pnpm";
const args = process.platform === "win32"
  ? ["/d", "/s", "/c", ["pnpm", ...buildArgs].join(" ")]
  : buildArgs;
const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
if (!existsSync(resolve(root, "dist", "index.html"))) throw new Error("frontend build did not produce dist/index.html");
const releaseRoot = resolve(root, "dist", "release", "frontend");
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
  await mkdir(packageRoot, { recursive: true });
  await cp(bundleRoot, packageRoot, { recursive: true, force: true });
}
const releaseRootPath = resolve(root, "dist", "release");
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
  sourceRevision: process.env.GITHUB_SHA ?? "unresolved-local",
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
