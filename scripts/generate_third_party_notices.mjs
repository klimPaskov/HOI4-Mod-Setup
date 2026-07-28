import { mkdir, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const output = resolve(root, process.argv[2] ?? "dist/release/THIRD_PARTY_NOTICES.md");

function run(command, args) {
  const windowsPnpm = process.platform === "win32" && command === "pnpm";
  const executable = windowsPnpm ? (process.env.ComSpec ?? "cmd.exe") : command;
  const spawnArgs = windowsPnpm ? ["/d", "/s", "/c", "pnpm.cmd", ...args] : args;
  const result = spawnSync(executable, spawnArgs, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr.trim() || `exit ${result.status}`}`);
  }
  return result.stdout;
}

const npmGroups = JSON.parse(run("pnpm", ["licenses", "list", "--json"]));
const npmPackages = [];
for (const group of Object.values(npmGroups)) {
  for (const dependency of group) {
    for (const version of dependency.versions ?? []) {
      npmPackages.push({ name: dependency.name, version, license: dependency.license, homepage: dependency.homepage ?? "" });
    }
  }
}

const cargoMetadata = JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--locked"]));
const cargoPackages = cargoMetadata.packages
  .filter((dependency) => dependency.name !== "hoi4-mod-setup" && dependency.name !== "hoi4-mod-setup-fuzz")
  .map((dependency) => ({
    name: dependency.name,
    version: dependency.version,
    license: dependency.license ?? dependency.license_file ?? "",
    source: dependency.source ?? "path dependency",
  }));

const unknown = [...npmPackages, ...cargoPackages].filter((dependency) => !String(dependency.license).trim());
if (unknown.length > 0) {
  throw new Error(`dependency metadata has no license for: ${unknown.map((dependency) => `${dependency.name}@${dependency.version}`).join(", ")}`);
}

const cell = (value) => String(value ?? "").replaceAll("|", "\\|").replaceAll("\r", " ").replaceAll("\n", " ");
const table = (rows, rust = false) => [
  rust ? "| Package | Version | License | Source |" : "| Package | Version | License | Homepage |",
  "| --- | --- | --- | --- |",
  ...rows.map((dependency) => `| ${cell(dependency.name)} | ${cell(dependency.version)} | ${cell(dependency.license)} | ${cell(rust ? dependency.source : dependency.homepage)} |`),
].join("\n");

npmPackages.sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`));
cargoPackages.sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`));
const document = [
  "# Third-party notices",
  "",
  "This inventory is generated from the locked pnpm and Cargo dependency metadata by",
  "`scripts/generate_third_party_notices.mjs`. It records declared license expressions",
  "and source or homepage evidence for release review. It does not replace maintainer",
  "review of bundled assets or the complete license text supplied by each dependency",
  "before a public binary release.",
  "",
  `Inventory counts: ${npmPackages.length} JavaScript packages and ${cargoPackages.length} Rust packages.`,
  "",
  "## JavaScript dependencies",
  "",
  table(npmPackages),
  "",
  "## Rust dependencies",
  "",
  table(cargoPackages, true),
  "",
].join("\n");

await mkdir(resolve(output, ".."), { recursive: true });
await writeFile(output, document, "utf8");
console.log(`Generated third-party notice inventory at ${output}`);
