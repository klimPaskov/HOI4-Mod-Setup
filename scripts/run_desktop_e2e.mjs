import { existsSync, readdirSync, statSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { launchAndStop } from "./native_smoke.mjs";

const root = resolve(import.meta.dirname, "..");
const releaseRoots = [
  resolve(root, "target", "release"),
  resolve(root, "src-tauri", "target", "release"),
];

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

function findExecutable() {
  const files = releaseRoots.flatMap((releaseRoot) => walk(releaseRoot));
  const candidates = process.platform === "win32"
    ? files.filter((file) => file.toLowerCase().endsWith("hoi4-mod-setup.exe"))
    : ["darwin", "linux"].includes(process.platform)
      ? files.filter((file) => (file.includes(".app/Contents/MacOS/") && /hoi4[- ]mod[- ]setup$/i.test(file)) || /[\\/]hoi4-mod-setup$/i.test(file))
      : [];
  const executable = candidates.sort((left, right) => left.length - right.length)[0];
  if (!executable || !statSync(executable).isFile()) {
    throw new Error(`native application executable was not found under ${releaseRoots.join(", ")}`);
  }
  return executable;
}

const executable = findExecutable();
const profile = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-desktop-e2e-"));
try {
  await launchAndStop(executable, { cwd: profile, env: { ...process.env,
    XDG_CONFIG_HOME: join(profile, "config"), XDG_DATA_HOME: join(profile, "data"), XDG_CACHE_HOME: join(profile, "cache"),
    APPDATA: join(profile, "config"), LOCALAPPDATA: join(profile, "data"),
  } });
  console.log(`Native desktop launch smoke passed for ${process.platform}`);
} finally {
  await rm(profile, { recursive: true, force: true });
}
