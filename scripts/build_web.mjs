// Build the web edition: compile the setup core to WebAssembly, generate the
// JavaScript bindings with the wasm-bindgen CLI that matches Cargo.lock, and
// bundle the page into dist-web/.
//
//   node scripts/build_web.mjs            full build
//   node scripts/build_web.mjs --core     only the WebAssembly core

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const coreOnly = process.argv.includes("--core");

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit", shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function output(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8", shell: false });
  return result.status === 0 ? result.stdout.trim() : "";
}

const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
const locked = /name = "wasm-bindgen"\r?\nversion = "([^"]+)"/.exec(lock)?.[1];
if (!locked) throw new Error("Cargo.lock does not record wasm-bindgen.");
const installed = /wasm-bindgen (\S+)/.exec(output("wasm-bindgen", ["--version"]))?.[1];
if (installed !== locked) {
  console.error(`wasm-bindgen CLI ${installed ?? "is not installed"}; the lockfile needs ${locked}.`);
  console.error(`Install it with: cargo install wasm-bindgen-cli --version ${locked} --locked`);
  process.exit(1);
}

run("cargo", ["build", "--locked", "--release", "-p", "hoi4-setup-web", "--target", "wasm32-unknown-unknown"]);
const coreDir = join(root, "web", "public", "core");
rmSync(coreDir, { recursive: true, force: true });
mkdirSync(coreDir, { recursive: true });
run("wasm-bindgen", [
  "--target", "web",
  "--no-typescript",
  "--out-dir", coreDir,
  join(root, "target", "wasm32-unknown-unknown", "release", "hoi4_setup_web.wasm"),
]);
copyFileSync(join(root, "src-tauri", "icons", "128x128.png"), join(root, "web", "public", "icon.png"));

if (!coreOnly) {
  const vite = join(root, "node_modules", "vite", "bin", "vite.js");
  run(process.execPath, [vite, "build", "--config", "vite.web.config.ts"]);
  const dist = join(root, "dist-web");
  addSubresourceIntegrity(dist);
  writeBuildInfo(dist);
}

function digest(algorithm, bytes, encoding) {
  return createHash(algorithm).update(bytes).digest(encoding);
}

/** Pin every bundled script and stylesheet each page loads by hash. */
function addSubresourceIntegrity(dist) {
  for (const page of ["index.html", "openrouter-callback.html"]) pinPage(dist, join(dist, page));
}

function pinPage(dist, indexPath) {
  const base = process.env.WEB_BASE ?? "/HOI4-Mod-Setup/app/";
  const html = readFileSync(indexPath, "utf8").replace(/<(script|link)\b([^>]*?)\s(src|href)="([^"]+)"([^>]*)>/g, (tag, name, before, attribute, url, after) => {
    if (!url.startsWith(base) || /integrity=/.test(tag) || url.endsWith(".png")) return tag;
    const file = join(dist, url.slice(base.length));
    const integrity = `sha384-${digest("sha384", readFileSync(file), "base64")}`;
    return `<${name}${before} ${attribute}="${url}" integrity="${integrity}"${after}>`;
  });
  writeFileSync(indexPath, html);
}

/** Publish the hash of every built file so anyone can compare the deployed
 * page with the CI artifact for the same commit. */
function writeBuildInfo(dist) {
  const files = {};
  const walk = (directory, prefix) => {
    for (const name of readdirSync(directory).sort()) {
      const path = join(directory, name);
      const relative = prefix ? `${prefix}/${name}` : name;
      if (statSync(path).isDirectory()) walk(path, relative);
      else if (relative !== "build-info.json") files[relative] = digest("sha256", readFileSync(path), "hex");
    }
  };
  walk(dist, "");
  const commit = process.env.GITHUB_SHA ?? output("git", ["rev-parse", "HEAD"]);
  writeFileSync(join(dist, "build-info.json"), `${JSON.stringify({ commit, wasm_bindgen: locked, files }, null, 2)}\n`);
}
