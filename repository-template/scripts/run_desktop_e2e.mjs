import { existsSync, readdirSync, statSync } from "node:fs";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const releaseRoots = [
  resolve(root, "target", "release"),
  resolve(root, "src-tauri", "target", "release"),
];
const startupTimeoutMs = 30_000;
const observationMs = 2_000;
const shutdownTimeoutMs = 10_000;
const cleanupObservationMs = 2_000;
const execFileAsync = promisify(execFile);

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
    : process.platform === "darwin"
      ? files.filter((file) => (file.includes(".app/Contents/MacOS/") && /hoi4[- ]mod[- ]setup$/i.test(file)) || /[\\/]hoi4-mod-setup$/i.test(file))
      : [];
  const executable = candidates.sort((left, right) => left.length - right.length)[0];
  if (!executable || !statSync(executable).isFile()) {
    throw new Error(`native application executable was not found under ${releaseRoots.join(", ")}`);
  }
  return executable;
}

function waitFor(event, child, timeoutMs) {
  return new Promise((resolvePromise, reject) => {
    const timer = setTimeout(() => reject(new Error(`${event} timed out after ${timeoutMs} ms`)), timeoutMs);
    child.once(event, (...args) => {
      clearTimeout(timer);
      resolvePromise(args);
    });
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
  });
}

async function terminateChild(child) {
  if (child.exitCode !== null) return;
  if (process.platform === "win32" && child.pid) {
    try {
      await execFileAsync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
        windowsHide: true,
      });
    } catch (error) {
      if (child.exitCode === null) {
        await new Promise((resolvePromise) => {
          const timer = setTimeout(resolvePromise, cleanupObservationMs);
          child.once("exit", () => {
            clearTimeout(timer);
            resolvePromise();
          });
        });
      }
      if (child.exitCode === null) {
        throw error;
      }
    }
    return;
  }
  child.kill("SIGTERM");
}

const executable = findExecutable();
const output = [];
const child = spawn(executable, [], {
  cwd: root,
  env: { ...process.env, HOI4_MOD_SETUP_E2E: "1" },
  stdio: ["ignore", "pipe", "pipe"],
  windowsHide: true,
});
const capture = (chunk) => {
  if (output.join("").length < 4096) output.push(String(chunk).slice(0, 4096));
};
child.stdout?.on("data", capture);
child.stderr?.on("data", capture);

try {
  await waitFor("spawn", child, startupTimeoutMs);
  await new Promise((resolvePromise) => setTimeout(resolvePromise, observationMs));
  if (child.exitCode !== null) {
    throw new Error(`native application exited during startup with code ${child.exitCode}`);
  }
  const exit = waitFor("exit", child, shutdownTimeoutMs);
  await terminateChild(child);
  await exit;
  console.log(`Native desktop launch smoke passed for ${process.platform}: ${executable}`);
} catch (error) {
  if (child.exitCode === null) {
    try {
      await terminateChild(child);
    } catch {
      // Preserve the original smoke-test failure; cleanup is best-effort.
    }
  }
  const diagnostic = output.join("").trim();
  throw new Error(`${error instanceof Error ? error.message : String(error)}${diagnostic ? `\n${diagnostic}` : ""}`);
}
