import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { resolveNativeSystemExecutable } from "./windows_installer_registry.mjs";

const execFileAsync = promisify(execFile);
const delay = (milliseconds) => new Promise((resolvePromise) => setTimeout(resolvePromise, milliseconds));

function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((resolvePromise, reject) => {
    const timer = setTimeout(() => {
      child.removeListener("exit", exited);
      reject(new Error("native application did not exit within the bounded timeout"));
    }, timeoutMs);
    function exited() { clearTimeout(timer); resolvePromise(); }
    child.once("exit", exited);
  });
}

async function terminate(child, shutdownMs) {
  if (!child.pid) return;
  if (process.platform === "win32") {
    if (child.exitCode !== null || child.signalCode !== null) return;
    await execFileAsync(resolveNativeSystemExecutable("taskkill.exe"), ["/PID", String(child.pid), "/T", "/F"], { windowsHide: true, timeout: shutdownMs });
  } else {
    // The child owns a fresh process group; include WebKit helper processes.
    try { process.kill(-child.pid, "SIGTERM"); } catch (error) { if (error.code !== "ESRCH") throw error; }
  }
  try { await waitForExit(child, shutdownMs); }
  catch (error) {
    if (process.platform !== "win32") {
      try { process.kill(-child.pid, "SIGKILL"); } catch (killError) { if (killError.code !== "ESRCH") throw killError; }
      await waitForExit(child, shutdownMs);
    }
    throw error;
  }
}

export async function launchAndStop(executable, { args = [], cwd, env = process.env, observationMs = 2_000, shutdownMs = 10_000 } = {}) {
  const output = [];
  let outputLength = 0;
  const child = spawn(executable, args, {
    cwd, env: { ...env, HOI4_MOD_SETUP_E2E: "1" },
    stdio: ["ignore", "pipe", "pipe"], windowsHide: true, detached: process.platform !== "win32",
  });
  const capture = (chunk) => {
    const text = String(chunk).slice(0, Math.max(0, 4096 - outputLength));
    outputLength += text.length;
    if (text) output.push(text);
  };
  child.stdout?.on("data", capture);
  child.stderr?.on("data", capture);
  let failure;
  try {
    await new Promise((resolvePromise, reject) => { child.once("spawn", resolvePromise); child.once("error", reject); });
    await delay(observationMs);
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`native application exited during startup with code ${child.exitCode}, signal ${child.signalCode}`);
    await terminate(child, shutdownMs);
  } catch (error) {
    failure = error;
    try { await terminate(child, shutdownMs); } catch (cleanupError) { failure = new AggregateError([error, cleanupError], "native launch and cleanup failed"); }
  }
  if (failure) throw new Error(`${failure.message}${output.length ? `\n${output.join("").trim()}` : ""}`, { cause: failure });
}
