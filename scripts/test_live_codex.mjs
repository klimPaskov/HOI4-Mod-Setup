import { createHash } from "node:crypto";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, realpath, rm } from "node:fs/promises";
import { dirname, join, parse, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { createInterface } from "node:readline";

const execFileAsync = promisify(execFile);
const timeoutMs = 20_000;
const maximumLineBytes = 1024 * 1024;
const executable = process.env.HOI4_CODEX_EXECUTABLE;
const appMetadata = JSON.parse(await (await import("node:fs/promises")).readFile(
  resolve(import.meta.dirname, "..", "package.json"),
  "utf8",
));

function requireExecutable() {
  if (process.platform !== "win32") {
    throw new Error("This live Codex smoke is currently wired for Windows.");
  }
  if (!executable || !/^[A-Za-z]:[\\/]/.test(executable) || !existsSync(executable)) {
    throw new Error("Set HOI4_CODEX_EXECUTABLE to the absolute installed Codex executable path.");
  }
}

async function verifyPublisherAndHash() {
  const systemRoot = process.env.SystemRoot ?? process.env.SYSTEMROOT;
  if (!systemRoot) throw new Error("Windows system directory is unavailable.");
  const powershell = resolve(systemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
  const before = createHash("sha256").update(await import("node:fs/promises").then(({ readFile }) => readFile(executable))).digest("hex");
  const verifyScript = "$sig = Get-AuthenticodeSignature -LiteralPath $env:HOI4_CODEX_EXECUTABLE; if ($sig.Status -ne 'Valid' -or $sig.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false) -ne 'OpenAI OpCo, LLC') { exit 1 }";
  try {
    await execFileAsync(powershell, ["-NoProfile", "-NonInteractive", "-Command", verifyScript], {
      windowsHide: true,
      timeout: 10_000,
      env: { SystemRoot: systemRoot, SYSTEMROOT: systemRoot, HOI4_CODEX_EXECUTABLE: executable },
    });
  } catch {
    throw new Error("The selected Codex executable does not have the reviewed OpenAI publisher signature.");
  }
  const after = createHash("sha256").update(await import("node:fs/promises").then(({ readFile }) => readFile(executable))).digest("hex");
  if (before !== after) throw new Error("The selected Codex executable changed during signature verification.");
}

async function readExecutableVersion() {
  const systemRoot = process.env.SystemRoot ?? process.env.SYSTEMROOT;
  try {
    const { stdout } = await execFileAsync(executable, ["--version"], {
      windowsHide: true,
      timeout: 10_000,
      env: {
        PATH: dirname(executable),
        SystemRoot: systemRoot,
        SYSTEMROOT: systemRoot,
        SystemDrive: process.env.SystemDrive,
        HOMEDRIVE: process.env.HOMEDRIVE,
        HOMEPATH: process.env.HOMEPATH,
        USERPROFILE: process.env.USERPROFILE,
        APPDATA: process.env.APPDATA,
        LOCALAPPDATA: process.env.LOCALAPPDATA,
        CODEX_HOME: process.env.CODEX_HOME,
        TEMP: process.env.TEMP,
        TMP: process.env.TMP,
      },
    });
    const version = stdout.match(/(?:^|\s)([0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?)(?:\s|$)/)?.[1];
    if (!version) throw new Error("invalid version output");
    return version;
  } catch {
    throw new Error("The signed Codex executable did not return a bounded semantic version.");
  }
}

function codexEnvironment({ home, userProfile, appData, localAppData }) {
  const systemRoot = process.env.SystemRoot ?? process.env.SYSTEMROOT;
  const root = parse(userProfile).root;
  return {
    PATH: dirname(executable),
    SystemRoot: systemRoot,
    SYSTEMROOT: systemRoot,
    TEMP: home,
    TMP: home,
    USERPROFILE: userProfile,
    HOMEDRIVE: root.replace(/[\\/]$/, ""),
    HOMEPATH: userProfile.slice(root.length - 1),
    APPDATA: appData,
    LOCALAPPDATA: localAppData,
    CODEX_HOME: home,
  };
}

function startServer(environment, cwd) {
  const child = spawn(executable, ["app-server", "--stdio"], {
    cwd,
    env: environment,
    stdio: ["pipe", "pipe", "ignore"],
    windowsHide: true,
  });
  let nextId = 1;
  let settled = false;
  const pending = new Map();
  const lines = createInterface({ input: child.stdout });
  lines.on("line", (line) => {
    if (Buffer.byteLength(line, "utf8") > maximumLineBytes) {
      failAll(new Error("Codex App Server exceeded the bounded JSONL response size."));
      return;
    }
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      failAll(new Error("Codex App Server returned malformed JSONL."));
      return;
    }
    if (!Number.isSafeInteger(message.id)) return;
    const resolvePending = pending.get(message.id);
    if (!resolvePending) return;
    pending.delete(message.id);
    if (message.error) resolvePending({ error: true });
    else resolvePending({ value: message.result });
  });
  child.once("error", () => failAll(new Error("Codex App Server could not start.")));
  child.once("close", () => failAll(new Error("Codex App Server stopped unexpectedly.")));

  function failAll(error) {
    if (settled) return;
    settled = true;
    lines.close();
    for (const resolvePending of pending.values()) resolvePending({ error: true, failure: error });
    pending.clear();
  }

  function request(method, params) {
    if (settled || child.exitCode !== null) return Promise.reject(new Error("Codex App Server is not running."));
    const id = nextId++;
    return new Promise((resolvePromise, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error("Codex App Server request exceeded the bounded timeout."));
      }, timeoutMs);
      pending.set(id, (response) => {
        clearTimeout(timer);
        if (response.failure) reject(response.failure);
        else if (response.error) reject(new Error("Codex App Server rejected a reviewed protocol request."));
        else resolvePromise(response.value);
      });
      child.stdin.write(JSON.stringify({ id, method, params }) + "\n", (error) => {
        if (!error) return;
        pending.delete(id);
        clearTimeout(timer);
        reject(new Error("Codex App Server request could not be sent."));
      });
    });
  }

  function notify(method, params = {}) {
    child.stdin.write(JSON.stringify({ method, params }) + "\n");
  }

  async function close() {
    settled = true;
    lines.close();
    if (child.exitCode !== null) return;
    const taskkill = resolve(process.env.SystemRoot ?? process.env.SYSTEMROOT, "System32", "taskkill.exe");
    try {
      await execFileAsync(taskkill, ["/PID", String(child.pid), "/T", "/F"], { windowsHide: true, timeout: 10_000 });
    } catch {
      if (child.exitCode === null) child.kill();
    }
    if (child.exitCode === null) {
      await new Promise((resolvePromise) => {
        const timer = setTimeout(resolvePromise, 5_000);
        child.once("close", () => {
          clearTimeout(timer);
          resolvePromise();
        });
      });
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 150));
  }

  return { child, request, notify, close };
}

function validateInitialize(result, expectedServerVersion) {
  const userAgent = typeof result?.userAgent === "string" ? result.userAgent : "";
  const userAgentToken = userAgent.split(/\s+/)[0] ?? "";
  const serverVersion = userAgentToken.startsWith("hoi4-mod-setup/")
    ? userAgentToken.slice("hoi4-mod-setup/".length)
    : "";
  if (serverVersion !== expectedServerVersion
      || typeof result?.platformFamily !== "string"
      || typeof result?.platformOs !== "string"
      || result.platformFamily !== "windows"
      || result.platformOs !== "windows") {
    throw new Error("Codex initialize metadata mismatch: server_version=" + (serverVersion || "missing")
      + ", expected=" + expectedServerVersion
      + ", family=" + (result?.platformFamily ?? "missing")
      + ", os=" + (result?.platformOs ?? "missing") + ".");
  }
}

async function initialize(server, expectedServerVersion) {
  const result = await server.request("initialize", {
    clientInfo: { name: "hoi4-mod-setup", title: "HOI4 Mod Setup", version: appMetadata.version },
  });
  validateInitialize(result, expectedServerVersion);
  server.notify("initialized");
}

async function loginStartAndCancel(server, type) {
  const result = await server.request("account/login/start", { type });
  const loginId = result?.loginId ?? result?.login_id;
  if (typeof loginId !== "string" || !loginId.trim()) {
    throw new Error("Codex App Server did not return a login attempt ID.");
  }
  await server.request("account/login/cancel", { loginId });
  return true;
}

requireExecutable();
await verifyPublisherAndHash();
const expectedServerVersion = await readExecutableVersion();
const base = await realpath(tmpdir());
const rawTempRoot = await mkdtemp(join(base, "hoi4-mod-setup-codex-live-"));
const tempRoot = await realpath(rawTempRoot);
if (!tempRoot.startsWith(base + sep)) throw new Error("The temporary Codex home escaped the system temp directory.");
const currentHome = process.env.CODEX_HOME || resolve(process.env.USERPROFILE ?? "", ".codex");
const existingEnvironment = codexEnvironment({
  home: currentHome,
  userProfile: process.env.USERPROFILE ?? "",
  appData: process.env.APPDATA ?? "",
  localAppData: process.env.LOCALAPPDATA ?? "",
});
const isolatedEnvironment = codexEnvironment({
  home: resolve(tempRoot, "codex-home"),
  userProfile: resolve(tempRoot, "user"),
  appData: resolve(tempRoot, "appdata"),
  localAppData: resolve(tempRoot, "localappdata"),
});
for (const folder of [isolatedEnvironment.CODEX_HOME, isolatedEnvironment.USERPROFILE, isolatedEnvironment.APPDATA, isolatedEnvironment.LOCALAPPDATA]) {
  await mkdir(folder, { recursive: true });
}
await mkdir(tempRoot, { recursive: true });

try {
  let liveAccountType = "unavailable";
  let usageCheck = "not_applicable";
  const current = startServer(existingEnvironment, tempRoot);
  try {
    await initialize(current, expectedServerVersion);
    const accountResult = await current.request("account/read", { refreshToken: false });
    const account = accountResult?.account ?? accountResult;
    const rawType = account?.type ?? account?.accountType ?? account?.authMode;
    liveAccountType = rawType === "chatgpt" ? "chatgpt" : rawType === "apiKey" ? "api_key" : "signed_out";
    if (liveAccountType === "chatgpt") {
      try {
        await current.request("account/rateLimits/read", {});
        usageCheck = "available";
      } catch {
        usageCheck = "unavailable";
      }
    }
  } finally {
    await current.close();
  }

  const isolated = startServer(isolatedEnvironment, tempRoot);
  try {
    await initialize(isolated, expectedServerVersion);
    const accountResult = await isolated.request("account/read", { refreshToken: false });
    const account = accountResult?.account ?? accountResult;
    const rawType = account?.type ?? account?.accountType ?? account?.authMode;
    if (rawType === "chatgpt" || rawType === "apiKey") {
      throw new Error("The isolated Codex home unexpectedly contained an account.");
    }
    await loginStartAndCancel(isolated, "chatgpt");
    await loginStartAndCancel(isolated, "chatgptDeviceCode");
  } finally {
    await isolated.close();
  }
  process.stdout.write("Codex live check passed: initialize compatible; account_type=" + liveAccountType
    + "; server_version=" + expectedServerVersion + "; usage_check=" + usageCheck
    + "; browser_login=start/cancel; device_code_login=start/cancel. No login URL, code, email, account ID, token, or rate-limit values were printed.\n");
} finally {
  await rm(tempRoot, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
