import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const version = process.argv[2] === "--" ? process.argv[3] : process.argv[2];
if (typeof version !== "string" || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error("usage: pnpm version:set -- MAJOR.MINOR.PATCH[-prerelease]");
}

async function updateJson(relativePath) {
  const path = resolve(root, relativePath);
  const source = await readFile(path, "utf8");
  const updated = source.replace(/("version"\s*:\s*")[^"]+("\s*[,}])/, `$1${version}$2`);
  if (updated === source) throw new Error(`${relativePath} version was not found`);
  await writeFile(path, updated, "utf8");
}

async function updateCargoToml() {
  const path = resolve(root, "src-tauri", "Cargo.toml");
  const source = await readFile(path, "utf8");
  const updated = source.replace(/(^\[package\][\s\S]*?^version\s*=\s*")[^"]+("\s*$)/m, `$1${version}$2`);
  if (updated === source) throw new Error("src-tauri/Cargo.toml package version was not found");
  await writeFile(path, updated, "utf8");
}

async function updateCargoLock() {
  const path = resolve(root, "Cargo.lock");
  const source = await readFile(path, "utf8");
  const pattern = /(\[\[package\]\]\r?\nname = "hoi4-mod-setup"\r?\nversion = ")[^"]+("\r?\n)/;
  const updated = source.replace(pattern, `$1${version}$2`);
  if (updated === source) throw new Error("Cargo.lock application package version was not found");
  await writeFile(path, updated, "utf8");
}

await updateJson("package.json");
await updateJson("src-tauri/tauri.conf.json");
await updateCargoToml();
await updateCargoLock();
console.log(`Set HOI4 Mod Setup version to ${version}.`);
