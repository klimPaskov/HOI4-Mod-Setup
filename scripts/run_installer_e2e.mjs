import { existsSync, readdirSync, statSync } from "node:fs";
import { chmod, copyFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  assertWindowsInstallerRegistryIsClean,
  cleanupWindowsInstallerRegistry,
} from "./windows_installer_registry.mjs";

import { launchAndStop } from "./native_smoke.mjs";
import { withLinuxDebInstallation } from "./linux_installer_registry.mjs";
import { verifyLinuxPackage } from "./linux_packages.mjs";

const root = resolve(import.meta.dirname, "..");
const packagesRoot = resolve(root, "dist", "release", "packages");
const execFileAsync = promisify(execFile);
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

let smokeProfile;
async function launchInstalled(executable) {
  if (!statSync(executable).isFile()) throw new Error(`installed executable is missing: ${executable}`);
  await launchAndStop(executable, { cwd: smokeProfile, env: { ...process.env,
    XDG_CONFIG_HOME: join(smokeProfile, "config"), XDG_DATA_HOME: join(smokeProfile, "data"), XDG_CACHE_HOME: join(smokeProfile, "cache"),
    APPDATA: join(smokeProfile, "config"), LOCALAPPDATA: join(smokeProfile, "data"),
  } });
}

async function testWindowsInstaller(workRoot, installRoot) {
  const installer = requireOne(".exe");
  await execFileAsync(installer, ["/S", `/D=${installRoot}`], { windowsHide: true, timeout: 120_000 });
  const executable = walk(installRoot).find((path) => path.toLowerCase().endsWith("hoi4-mod-setup.exe"));
  if (!executable) throw new Error("NSIS installation did not produce the application executable");
  await launchInstalled(executable);
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
  await execFileAsync("/usr/bin/hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", mountRoot, packagePath], { timeout: 120_000 });
  try {
    const app = readdirSync(mountRoot, { withFileTypes: true })
      .find((entry) => entry.isDirectory() && entry.name.endsWith(".app"));
    if (!app) throw new Error("DMG did not contain an application bundle");
    const installedApp = resolve(installedRoot, app.name);
    await execFileAsync("/usr/bin/ditto", [resolve(mountRoot, app.name), installedApp], { timeout: 120_000 });
    const executable = walk(resolve(installedApp, "Contents", "MacOS"))
      .find((path) => /hoi4[- ]mod[- ]setup$/i.test(path));
    if (!executable) throw new Error("installed macOS application executable was not found");
    await launchInstalled(executable);
    await rm(installedApp, { recursive: true, force: true });
    if (existsSync(installedApp)) throw new Error("macOS application removal did not complete");
  } finally {
    await execFileAsync("/usr/bin/hdiutil", ["detach", mountRoot], { timeout: 120_000 });
  }
}

async function testLinuxInstaller(workRoot) {
  const image = requireOne(".appimage");
  const deb = requireOne(".deb");
  const metadata = JSON.parse(await readFile(resolve(root, "dist", "release", "BUILD_METADATA.json"), "utf8"));
  verifyLinuxPackage(image, metadata.version, "X64");
  verifyLinuxPackage(deb, metadata.version, "X64");
  const imageRoot = join(workRoot, "appimage");
  await mkdir(imageRoot);
  const installedImage = join(imageRoot, "HOI4-Mod-Setup.AppImage");
  await copyFile(image, installedImage);
  await chmod(installedImage, 0o700);
  await execFileAsync(installedImage, ["--appimage-extract"], { cwd: imageRoot, timeout: 120_000, maxBuffer: 16 * 1024 * 1024 });
  await launchInstalled(join(imageRoot, "squashfs-root", "AppRun"));
  await rm(imageRoot, { recursive: true });
  if (existsSync(imageRoot)) throw new Error("AppImage removal did not complete");

  if (process.env.HOI4_MOD_SETUP_DEDICATED_LINUX !== "1") {
    const debRoot = join(workRoot, "deb");
    await execFileAsync("/usr/bin/dpkg-deb", ["--extract", deb, debRoot], { timeout: 120_000 });
    await launchInstalled(join(debRoot, "usr", "bin", "hoi4-mod-setup"));
    await rm(debRoot, { recursive: true });
    if (existsSync(debRoot)) throw new Error("extracted Debian application removal did not complete");
    console.log("Linux extraction smoke passed; actual dpkg lifecycle requires HOI4_MOD_SETUP_DEDICATED_LINUX=1 on a disposable runner.");
    return;
  }
  await withLinuxDebInstallation(deb, launchInstalled);
}

if (!["win32", "darwin", "linux"].includes(process.platform)) {
  throw new Error(`installer smoke testing is unsupported on ${process.platform}`);
}

const workRoot = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-installer-e2e-"));
const installRoot = resolve(workRoot, "installed");
smokeProfile = join(workRoot, "profile");
await mkdir(smokeProfile);
let ownsWindowsRegistry = false;
let testError = null;
try {
  if (process.platform === "win32") {
    await assertWindowsInstallerRegistryIsClean();
    ownsWindowsRegistry = true;
    await testWindowsInstaller(workRoot, installRoot);
  }
  else if (process.platform === "darwin") await testMacInstaller(workRoot);
  else await testLinuxInstaller(workRoot);
  console.log(`Installer install, launch, and removal smoke passed for ${process.platform}`);
} catch (error) {
  testError = error;
} finally {
  try {
    if (ownsWindowsRegistry) await cleanupWindowsInstallerRegistry({ workRoot, installRoot });
  } catch (cleanupError) {
    testError = testError
      ? new AggregateError([testError, cleanupError], "installer E2E and registry cleanup both failed")
      : cleanupError;
  }
  try {
    await rm(workRoot, { recursive: true, force: true });
    if (existsSync(workRoot)) throw new Error("installer E2E temporary directory cleanup did not complete");
  } catch (cleanupError) {
    testError = testError
      ? new AggregateError([testError, cleanupError], "installer E2E and temporary-directory cleanup both failed")
      : cleanupError;
  }
}

if (testError) throw testError;
