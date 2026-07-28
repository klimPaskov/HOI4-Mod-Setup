import { createHash } from "node:crypto";
import { lstat, readdir, readFile, writeFile } from "node:fs/promises";
import { resolve, sep } from "node:path";

const root = resolve(import.meta.dirname, "..");
const releaseRoot = resolve(root, process.env.HOI4_MOD_SETUP_RELEASE_ROOT ?? "dist/release");
const manifestPath = resolve(releaseRoot, process.env.HOI4_MOD_SETUP_RELEASE_MANIFEST ?? "ARTIFACTS.sha256");

if (process.env.HOI4_MOD_SETUP_WINDOWS_SIGNING_MODE !== "artifact-signing") {
  throw new Error("release manifest refresh is reserved for the Azure Artifact Signing route");
}

async function walk(directory, prefix = "") {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    const absolute = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(absolute, relative));
    else if (entry.isFile()) files.push({ relative: relative.replaceAll(sep, "/"), absolute });
  }
  return files;
}

function assertSafeRelativePath(path) {
  if (typeof path !== "string" || !path || path.startsWith("/") || path.includes("\\") || path.split("/").includes("..")) {
    throw new Error("release artifact manifest contains an unsafe relative path");
  }
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

const rootStat = await lstat(releaseRoot).catch(() => null);
if (!rootStat?.isDirectory()) throw new Error(`release root does not exist: ${releaseRoot}`);
const previous = JSON.parse(await readFile(manifestPath, "utf8"));
if (!Array.isArray(previous) || previous.length === 0) throw new Error("release artifact manifest is empty");

const previousByPath = new Map();
for (const entry of previous) {
  assertSafeRelativePath(entry?.path);
  if (entry.path === "ARTIFACTS.sha256" || previousByPath.has(entry.path) || !/^[0-9a-f]{64}$/i.test(entry.sha256)) {
    throw new Error("release artifact manifest contains an invalid or duplicate entry");
  }
  previousByPath.set(entry.path, entry.sha256.toLowerCase());
}

const files = (await walk(releaseRoot)).filter(({ relative }) => relative !== "ARTIFACTS.sha256");
const currentByPath = new Map();
for (const file of files) currentByPath.set(file.relative, digest(await readFile(file.absolute)));

const previousPaths = [...previousByPath.keys()].sort();
const currentPaths = [...currentByPath.keys()].sort();
if (previousPaths.length !== currentPaths.length || previousPaths.some((path, index) => path !== currentPaths[index])) {
  throw new Error("Azure signing changed the release file set; refusing to rewrite the manifest");
}

const changed = previousPaths.filter((path) => previousByPath.get(path) !== currentByPath.get(path));
if (changed.length === 0) throw new Error("Azure signing did not change any release artifact");
if (changed.some((path) => !/^packages\/[^/]+\.exe$/i.test(path))) {
  throw new Error(`Azure signing changed a non-Windows-package file: ${changed.join(", ")}`);
}

const refreshed = currentPaths.map((path) => ({ path, sha256: currentByPath.get(path) }));
await writeFile(manifestPath, JSON.stringify(refreshed, null, 2) + "\n", "utf8");
console.log(`Refreshed the release manifest after Azure signing (${changed.length} Windows package file${changed.length === 1 ? "" : "s"}).`);
