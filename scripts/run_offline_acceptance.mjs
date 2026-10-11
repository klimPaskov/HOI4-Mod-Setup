import { spawnSync } from "node:child_process";
import { link, lstat, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { isAbsolute, join, relative, resolve } from "node:path";

const source = process.env.HOI4_MOD_SETUP_SOURCE_REPOSITORY;
if (!source || !isAbsolute(source)) {
  throw new Error("Set HOI4_MOD_SETUP_SOURCE_REPOSITORY to the absolute path of an explicit committed package checkout.");
}
const gitMetadata = await lstat(join(source, ".git")).catch(() => {
  throw new Error("HOI4_MOD_SETUP_SOURCE_REPOSITORY must name an existing Git checkout.");
});
if (!gitMetadata.isFile() && !gitMetadata.isDirectory()) {
  throw new Error("The selected source is not a Git checkout.");
}
const safeEnvironment = Object.fromEntries(Object.keys(process.env)
  .filter((name) => !/(?:TOKEN|SECRET|PASSWORD|API.?KEY|COOKIE|AUTHORIZATION|CREDENTIAL|PROXY|^ANTHROPIC_|^OPENAI_|^MESHY_|^HF_|^GIT_|^CODEX_HOME$)/i.test(name))
  .map((name) => [name, process.env[name]]));
const history = spawnSync("git", ["-C", source, "cat-file", "-e", "0bb4917dca228886aa9a52963bc09e7078666d64^{commit}"], {
  stdio: "ignore", windowsHide: true, timeout: 30_000, env: safeEnvironment,
});
if (history.error || history.status !== 0) {
  throw new Error("The checkout must retain public 0.3.5 source commit 0bb4917dca228886aa9a52963bc09e7078666d64. Fetch the missing source history before offline acceptance.");
}
const taskRoot = await mkdtemp(join(tmpdir(), "hoi4-setup-offline-"));
const repo = resolve(import.meta.dirname, "..");
// Keep the installed toolchain discoverable while application state is isolated.
const toolchain = {
  CARGO_HOME: process.env.CARGO_HOME ?? join(homedir(), ".cargo"),
  RUSTUP_HOME: process.env.RUSTUP_HOME ?? join(homedir(), ".rustup"),
};
const profile = Object.fromEntries(["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"].map((name) => [name, join(taskRoot, name)]));
const testEnvironment = { ...safeEnvironment, ...profile, ...toolchain, HOI4_TEST_RUST_THREADS: "1" };
function runFilter(filter, fixture) {
  console.log(`Offline acceptance: ${filter}`);
  const result = spawnSync(process.execPath, [join(repo, "scripts/run_rust_tests.mjs"), "--offline", filter, "--", "--ignored", "--nocapture"], {
    cwd: repo, stdio: "inherit", windowsHide: true,
    env: { ...testEnvironment, ...(fixture ? { HOI4_MOD_SETUP_SCAN_FIXTURE: fixture } : {}) },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Offline acceptance failed: ${filter}`);
}
try {
  for (const directory of Object.values(profile)) await mkdir(directory);
  // Fail before spending time on the large scanner fixture when source or
  // lifecycle prerequisites are not satisfied.
  runFilter("local_source_serves_every_declared_file");
  runFilter("offline_lifecycle::offline_");
  const suppliedFixture = process.env.HOI4_MOD_SETUP_SCAN_FIXTURE;
  const fixture = suppliedFixture ?? join(taskRoot, "large-mod");
  if (suppliedFixture) {
    if (!isAbsolute(suppliedFixture) || (await lstat(suppliedFixture)).isSymbolicLink()
        || await readFile(join(fixture, "AGENTS.md"), "utf8") !== "# Synthetic project\nKeep all gameplay files unchanged.\n") {
      throw new Error("Reuse only an absolute synthetic scan fixture created by this runner.");
    }
  } else {
    await mkdir(fixture);
    await writeFile(join(fixture, "descriptor.mod"), 'name="Synthetic large mod"\nversion="0.1.0"\nsupported_version="1.19.*"\n');
    await writeFile(join(fixture, "AGENTS.md"), "# Synthetic project\nKeep all gameplay files unchanged.\n");
    // Ordinary gameplay must not inflate the bounded setup inventory.
    console.log("Preparing a temporary 50,000-file gameplay fixture.");
    for (let folder = 0; folder < 100; folder += 1) {
      const directory = join(fixture, "events", String(folder));
      await mkdir(directory, { recursive: true });
      const seed = join(directory, "0.txt");
      await writeFile(seed, "# synthetic gameplay\n");
      // Real regular-file entries, with 500 links per seed (below NTFS's
      // 1,024-link limit). No write occurs after links are created.
      for (let first = 1; first < 500; first += 32) {
        await Promise.all(Array.from({ length: Math.min(32, 500 - first) }, (_, offset) =>
          link(seed, join(directory, `${first + offset}.txt`))));
      }
      if ((folder + 1) % 25 === 0) console.log(`Prepared ${(folder + 1) * 500} gameplay entries.`);
    }
  }
  const sentinel = await readFile(join(fixture, "events", "99", "499.txt"));
  runFilter("external_very_large_mod_fixture_completes_without_mutation", fixture);
  if (!sentinel.equals(await readFile(join(fixture, "events", "99", "499.txt")))) {
    throw new Error("The read-only scanner changed the gameplay sentinel.");
  }
} finally {
  // Cleanup is confined to the exact temporary namespace minted above.
  const suffix = relative(tmpdir(), taskRoot);
  if (suffix.startsWith("hoi4-setup-offline-") && !suffix.includes("..") && !isAbsolute(suffix)) {
    await rm(taskRoot, { recursive: true, force: true });
  }
}
