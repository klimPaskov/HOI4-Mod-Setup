import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const outputPath = resolve(root, process.argv[2] ?? "dist/release/SBOM.cdx.json");
const packageJson = JSON.parse(await readFile(resolve(root, "package.json"), "utf8"));
const pnpmLock = await readFile(resolve(root, "pnpm-lock.yaml"));
const cargoLock = await readFile(resolve(root, "Cargo.lock"));
const cargoMetadataResult = spawnSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
  cwd: root,
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
  stdio: ["ignore", "pipe", "pipe"],
});
if (cargoMetadataResult.status !== 0) {
  throw new Error(`cargo metadata failed: ${cargoMetadataResult.stderr?.trim() || "unknown error"}`);
}
const cargoMetadata = JSON.parse(cargoMetadataResult.stdout);
const pnpmResult = process.platform === "win32"
  ? spawnSync(process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", "pnpm licenses list --json"], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  })
  : spawnSync("pnpm", ["licenses", "list", "--json"], {
  cwd: root,
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
  stdio: ["ignore", "pipe", "pipe"],
});
if (pnpmResult.status !== 0) {
  throw new Error(`pnpm license inventory failed: ${pnpmResult.stderr?.trim() || "unknown error"}`);
}
const pnpmLicenses = JSON.parse(pnpmResult.stdout);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function componentLicense(value) {
  if (typeof value !== "string" || !value.trim()) return undefined;
  return [{ license: { name: value.trim() } }];
}

function npmPurl(name, version) {
  return `pkg:npm/${name.replaceAll("@", "%40")}@${encodeURIComponent(version)}`;
}

function cargoPurl(name, version) {
  return `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`;
}

const npmComponents = [];
const npmSeen = new Set();
for (const [licenseGroup, entries] of Object.entries(pnpmLicenses)) {
  if (!Array.isArray(entries)) continue;
  for (const entry of entries) {
    if (typeof entry?.name !== "string" || !Array.isArray(entry.versions)) continue;
    for (const version of entry.versions) {
      if (typeof version !== "string") continue;
      const key = `${entry.name}@${version}`;
      if (npmSeen.has(key)) continue;
      npmSeen.add(key);
      npmComponents.push({
        type: "library",
        "bom-ref": `npm:${key}`,
        name: entry.name,
        version,
        purl: npmPurl(entry.name, version),
        licenses: componentLicense(entry.license ?? licenseGroup),
        properties: [{ name: "hoi4.mod.setup.package_manager", value: "pnpm" }],
      });
    }
  }
}

const cargoComponents = [];
for (const packageEntry of cargoMetadata.packages ?? []) {
  if (typeof packageEntry?.name !== "string" || typeof packageEntry.version !== "string") continue;
  const sourceKind = packageEntry.source === null
    ? "workspace"
    : String(packageEntry.source).includes("crates.io") ? "crates.io" : "registry";
  cargoComponents.push({
    type: "library",
    "bom-ref": `cargo:${packageEntry.name}@${packageEntry.version}`,
    name: packageEntry.name,
    version: packageEntry.version,
    purl: cargoPurl(packageEntry.name, packageEntry.version),
    licenses: componentLicense(packageEntry.license),
    properties: [{ name: "hoi4.mod.setup.source", value: sourceKind }],
  });
}

const sourceRevisionResult = spawnSync("git", ["rev-parse", "HEAD"], {
  cwd: root,
  encoding: "utf8",
  stdio: ["ignore", "pipe", "ignore"],
});
const sourceRevision = process.env.GITHUB_SHA?.trim().toLowerCase()
  || (sourceRevisionResult.status === 0 ? sourceRevisionResult.stdout.trim().toLowerCase() : "unresolved-local");
if (!/^[0-9a-f]{40}$/.test(sourceRevision)) throw new Error("SBOM generation requires an exact source revision");

const rootComponent = {
  type: "application",
  "bom-ref": "hoi4-mod-setup",
  name: packageJson.name,
  version: packageJson.version,
  licenses: componentLicense(packageJson.license),
};
const sbom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  version: 1,
  metadata: {
    component: rootComponent,
    tools: [{ vendor: "HOI4 Mod Setup", name: "scripts/generate_sbom.mjs" }],
    properties: [
      { name: "hoi4.mod.setup.source_revision", value: sourceRevision },
      { name: "hoi4.mod.setup.lock.pnpm-lock.yaml.sha256", value: sha256(pnpmLock) },
      { name: "hoi4.mod.setup.lock.Cargo.lock.sha256", value: sha256(cargoLock) },
    ],
  },
  components: [...npmComponents, ...cargoComponents].sort((left, right) => left["bom-ref"].localeCompare(right["bom-ref"])),
};

await writeFile(outputPath, JSON.stringify(sbom, null, 2) + "\n", "utf8");
console.log(`Generated CycloneDX dependency inventory (${sbom.components.length} components).`);
