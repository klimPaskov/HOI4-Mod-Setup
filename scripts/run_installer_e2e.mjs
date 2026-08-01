import { existsSync, readdirSync, statSync } from "node:fs";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

const root = resolve(import.meta.dirname, "..");
const packagesRoot = resolve(root, "dist", "release", "packages");
const execFileAsync = promisify(execFile);
const startupObservationMs = 2_000;
const uninstallObservationMs = 30_000;

function walk(directory) {
  if (!existsSync(directory)) return [];
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const absolute = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(absolute));
    else if (entry.isFile()) files.push(absolute);
  }
  return files;
}

function requireOne(suffix) {
  const matches = walk(packagesRoot).filter((path) => path.toLowerCase().endsWith(suffix));
  if (matches.length !== 1) {
    throw new Error(`expected exactly one ${suffix} package, found ${matches.length}`);
  }
  return matches[0];
}

async function waitUntilMissing(path, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (existsSync(path) && Date.now() < deadline) {
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 250));
  }
  return !existsSync(path);
}

async function launchAndStop(executable) {
  if (!statSync(executable).isFile()) throw new Error(`installed executable is missing: ${executable}`);
  const child = spawn(executable, [], {
    cwd: root,
    env: { ...process.env, HOI4_MOD_SETUP_E2E: "1" },
    stdio: "ignore",
    windowsHide: true,
  });
  await new Promise((resolvePromise, reject) => {
    child.once("error", reject);
    setTimeout(resolvePromise, startupObservationMs);
  });
  if (child.exitCode !== null) throw new Error(`installed application exited during startup with code ${child.exitCode}`);
  const exited = new Promise((resolvePromise) => child.once("exit", resolvePromise));
  if (process.platform === "win32") {
    await execFileAsync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], { windowsHide: true });
  } else {
    child.kill("SIGTERM");
  }
  await exited;
}

async function testWindowsInstaller(workRoot) {
  const installer = requireOne(".exe");
  const installRoot = resolve(workRoot, "installed");
  await execFileAsync(installer, ["/S", `/D=${installRoot}`], { windowsHide: true, timeout: 120_000 });
  const executable = walk(installRoot).find((path) => path.toLowerCase().endsWith("hoi4-mod-setup.exe"));
  if (!executable) throw new Error("NSIS installation did not produce the application executable");
  await launchAndStop(executable);
  const uninstaller = walk(installRoot).find((path) => /uninstall.*\.exe$/i.test(path));
  if (!uninstaller) throw new Error("NSIS installation did not produce an uninstaller");
  await execFileAsync(uninstaller, ["/S"], { windowsHide: true, timeout: 120_000 });
  if (!await waitUntilMissing(executable, uninstallObservationMs)) {
    throw new Error("NSIS uninstall left the application executable behind");
  }
}

async function testMacInstaller(workRoot) {
  const packagePath = requireOne(".dmg");
  const mountRoot = resolve(workRoot, "mount");
  const installedRoot = resolve(workRoot, "Applications");
  await mkdir(mountRoot, { recursive: true });
  await mkdir(installedRoot, { recursive: true });
  await execFileAsync("hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", mountRoot, packagePath], { timeout: 120_000 });
  try {
    const app = readdirSync(mountRoot, { withFileTypes: true })
      .find((entry) => entry.isDirectory() && entry.name.endsWith(".app"));
    if (!app) throw new Error("DMG did not contain an application bundle");
    const installedApp = resolve(installedRoot, app.name);
    await execFileAsync("ditto", [resolve(mountRoot, app.name), installedApp]);
    const executable = walk(resolve(installedApp, "Contents", "MacOS"))
      .find((path) => /hoi4[- ]mod[- ]setup$/i.test(path));
    if (!executable) throw new Error("installed macOS application executable was not found");
    await launchAndStop(executable);
    await rm(installedApp, { recursive: true, force: true });
    if (existsSync(installedApp)) throw new Error("macOS application removal did not complete");
  } finally {
    await execFileAsync("hdiutil", ["detach", mountRoot], { timeout: 120_000 });
  }
}

if (!["win32", "darwin"].includes(process.platform)) {
  throw new Error(`installer smoke testing is unsupported on ${process.platform}`);
}

const workRoot = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-installer-e2e-"));
try {
  if (process.platform === "win32") await testWindowsInstaller(workRoot);
  else await testMacInstaller(workRoot);
  console.log(`Installer install, launch, and removal smoke passed for ${process.platform}`);
} finally {
  await rm(workRoot, { recursive: true, force: true });
}
