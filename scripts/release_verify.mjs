import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageMetadata = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const tauriMetadata = JSON.parse(readFileSync(resolve(root, "src-tauri", "tauri.conf.json"), "utf8"));
const required = [
  resolve(root, "dist", "index.html"),
  resolve(root, "dist", "release", "BUILD_METADATA.json"),
];
for (const path of required) if (!existsSync(path)) throw new Error(`missing release output: ${path}`);
const metadata = JSON.parse(readFileSync(required[1], "utf8"));
if (metadata.product !== "HOI4 Mod Setup" || metadata.version !== packageMetadata.version || tauriMetadata.version !== packageMetadata.version) {
  throw new Error("release metadata and configured application versions do not match");
}
const tagVersion = process.env.GITHUB_REF_NAME?.startsWith("v")
  ? process.env.GITHUB_REF_NAME.slice(1)
  : undefined;
if (tagVersion && metadata.version !== tagVersion) {
  throw new Error(`release tag ${process.env.GITHUB_REF_NAME} does not match release metadata ${metadata.version}`);
}
const artifactManifestPath = resolve(root, "dist", "release", metadata.artifact_manifest ?? "ARTIFACTS.sha256");
if (!existsSync(artifactManifestPath)) throw new Error("release artifact manifest is missing");
const artifactManifest = JSON.parse(readFileSync(artifactManifestPath, "utf8"));
if (!Array.isArray(artifactManifest) || artifactManifest.length === 0) throw new Error("release artifact manifest is empty");
const seen = new Set();
for (const artifact of artifactManifest) {
  if (!artifact || typeof artifact.path !== "string" || !/^[^\\/][^:*?"<>|]*$/.test(artifact.path) || seen.has(artifact.path)) {
    throw new Error("release artifact manifest contains an invalid or duplicate path");
  }
  seen.add(artifact.path);
  const absolute = resolve(root, "dist", "release", artifact.path);
  if (!absolute.startsWith(resolve(root, "dist", "release") + (process.platform === "win32" ? "\\" : "/"))) throw new Error("release artifact escaped its root");
  if (!existsSync(absolute)) throw new Error(`missing release artifact: ${artifact.path}`);
  const digest = createHash("sha256").update(readFileSync(absolute)).digest("hex");
  if (digest !== artifact.sha256) throw new Error(`release artifact hash mismatch: ${artifact.path}`);
}
if (process.env.HOI4_MOD_SETUP_REQUIRE_TAURI === "1") {
  if (metadata.frontendOnly) throw new Error("release metadata is frontend-only");
  if (!/^[0-9a-f]{40}$/i.test(metadata.sourceRevision ?? "") || metadata.sourceRevision === "unresolved-local") {
    throw new Error("platform release is missing the exact source revision");
  }
  if (metadata.platform === "unresolved-local" || metadata.architecture === "unresolved-local") {
    throw new Error("platform release is missing runner identity");
  }
  const packageRoot = resolve(root, "dist", "release", "packages");
  if (!existsSync(packageRoot)) throw new Error("Tauri package directory is missing");
  const packages = readdirSync(packageRoot, { recursive: true });
  const platform = String(metadata.platform).toLowerCase();
  const packagePattern = platform.includes("windows")
    ? /\.(msi|exe)$/i
    : platform.includes("mac") || platform.includes("darwin")
      ? /\.(dmg|app)$/i
      : /\.(msi|dmg|app|exe)$/i;
  if (!packages.some((path) => packagePattern.test(path))) {
    throw new Error(`no ${platform} package was found in the Tauri bundle output`);
  }
  if (process.env.HOI4_MOD_SETUP_REQUIRE_SIGNING === "1" && metadata.signing !== "configured") {
    throw new Error("platform package is unsigned; configure signing before publication");
  }
}
console.log(process.env.HOI4_MOD_SETUP_REQUIRE_TAURI === "1"
  ? `Tauri package verified; signing=${metadata.signing}. Publication remains protected.`
  : "Release frontend artifact verified; platform package verification is an explicit CI step.");
