import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { copyFile, lstat, mkdir, mkdtemp, readFile, readdir, realpath, rm, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { createInterface } from "node:readline";

const root = resolve(import.meta.dirname, "..");
const manifestPath = resolve(root, "docs", "source-manifest", "hoi4-mod-setup.manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const component = manifest.components.find((candidate) => candidate.id === "mcp.hoi4_agent_tools");
const health = component?.validation?.find((rule) => rule.id === "mcp.hoi4.health")?.parameters;
const maxOutputBytes = 2 * 1024 * 1024;
const maxPackageFileBytes = 32 * 1024 * 1024;
const maxPackageTreeBytes = 256 * 1024 * 1024;
const maxPackageFiles = 10_000;

function requireEvidence() {
  if (process.platform !== "win32") {
    throw new Error("The current HOI4 Agent Tools MCP route is supported only on Windows.");
  }
  if (!health || health.package_name !== "hoi4-agent-tools" || health.package_version !== "3.6.0") {
    throw new Error("The bundled manifest does not declare the reviewed MCP 3.6.0 package.");
  }
  if (!Array.isArray(health.required_tools) || health.required_tools.length !== 34) {
    throw new Error("The bundled MCP tool list does not match the current source contract.");
  }
  if (!/^[a-f0-9]{64}$/.test(health.package_tree_sha256 ?? "")
      || !/^[a-f0-9]{64}$/.test(health.runtime_entry_sha256 ?? "")
      || !Number.isSafeInteger(health.package_file_count)
      || !Number.isSafeInteger(health.runtime_entry_size)) {
    throw new Error("The bundled MCP integrity evidence is incomplete.");
  }
}

function runNode(args, options, stage, timeoutMs = 120_000) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(process.execPath, args, {
      ...options,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout = [];
    const stderr = [];
    let outputBytes = 0;
    let oversized = false;
    const collect = (target) => (chunk) => {
      outputBytes += chunk.length;
      if (outputBytes > maxOutputBytes) {
        oversized = true;
        return;
      }
      target.push(chunk);
    };
    child.stdout.on("data", collect(stdout));
    child.stderr.on("data", collect(stderr));
    const timer = setTimeout(() => {
      if (child.exitCode === null) child.kill();
      reject(new Error("The bounded npm operation timed out."));
    }, timeoutMs);
    child.once("error", () => {
      clearTimeout(timer);
      reject(new Error("The pinned npm runtime could not start."));
    });
    child.once("close", (code) => {
      clearTimeout(timer);
      if (oversized) {
        reject(new Error("The bounded npm operation exceeded its output limit."));
      } else if (code !== 0) {
        const errorText = Buffer.concat([...stdout, ...stderr]).toString("utf8");
        const npmErrorCode = errorText.match(/npm error code ([A-Z0-9_]+)/i)?.[1]
          ?? errorText.match(/\b(EAI_AGAIN|ECONNRESET|ETIMEDOUT|ENOTFOUND|ECONNREFUSED|EACCES|EPERM|ENOENT|E404|E403|EINTEGRITY|EAUTH|E401)\b/)?.[1];
        let safeError = errorText;
        for (const key of ["APPDATA", "NPM_CONFIG_USERCONFIG", "NPM_CONFIG_GLOBALCONFIG", "NPM_CONFIG_CACHE", "TEMP", "TMP", "USERPROFILE"]) {
          const value = options.env?.[key];
          if (value) safeError = safeError.replaceAll(value, "<temp>");
        }
        safeError = safeError
          .replace(/\b(?:sk-[A-Za-z0-9_-]{8,}|npm_[A-Za-z0-9]{8,})\b/g, "<redacted>")
          .replace(/(token|password|authorization|_auth)\s*[:=]\s*[^\s]+/gi, "$1=<redacted>")
          .replace(/https?:\/\/[^\s]+/gi, "<url>")
          .replace(/\s+/g, " ")
          .trim();
        reject(new Error("The isolated npm " + stage + " failed with exit code " + code
          + (npmErrorCode ? " (" + npmErrorCode + ")" : "")
          + "; " + (safeError ? safeError.slice(0, 180) : "no diagnostic text") + "."));
      } else {
        resolvePromise(Buffer.concat(stdout).toString("utf8").trim());
      }
    });
  });
}

async function packageFiles(packageRoot) {
  const files = [];
  let totalBytes = 0;
  const pending = [packageRoot];
  while (pending.length) {
    const directory = pending.pop();
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const absolute = join(directory, entry.name);
      const metadata = await lstat(absolute);
      if (metadata.isSymbolicLink()) throw new Error("The MCP package contains a link.");
      if (metadata.isDirectory()) {
        pending.push(absolute);
      } else if (metadata.isFile()) {
        if (metadata.size > maxPackageFileBytes) throw new Error("An MCP package file exceeds its size bound.");
        totalBytes += metadata.size;
        if (totalBytes > maxPackageTreeBytes) throw new Error("The MCP package exceeds its total size bound.");
        const bytes = await readFile(absolute);
        files.push({ path: relative(packageRoot, absolute).replaceAll("\\", "/"), bytes });
        if (files.length > maxPackageFiles) throw new Error("The MCP package exceeds its file-count bound.");
      } else {
        throw new Error("The MCP package contains a special file.");
      }
    }
  }
  files.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
  const digest = createHash("sha256");
  for (const file of files) {
    digest.update(file.path, "utf8");
    digest.update(Buffer.from([0]));
    digest.update(String(file.bytes.length), "ascii");
    digest.update(Buffer.from([0]));
    digest.update(file.bytes);
  }
  return { files, sha256: digest.digest("hex") };
}

async function verifyInstalledPackage(prefix) {
  const modules = resolve(prefix, "node_modules");
  const packageRoot = resolve(modules, health.package_name);
  const packageInfo = JSON.parse(await readFile(resolve(packageRoot, "package.json"), "utf8"));
  if (packageInfo.name !== health.package_name || packageInfo.version !== health.package_version) {
    throw new Error("Installed MCP package name or version does not match the manifest.");
  }
  const npmLockPath = resolve(modules, ".package-lock.json");
  try {
    const lock = JSON.parse(await readFile(npmLockPath, "utf8"));
    const integrity = lock.packages?.[`node_modules/${health.package_name}`]?.integrity;
    if (integrity !== health.package_integrity) throw new Error("Installed npm lock integrity does not match the manifest.");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  const tree = await packageFiles(packageRoot);
  if (tree.sha256 !== health.package_tree_sha256 || tree.files.length !== health.package_file_count) {
    throw new Error("Installed MCP package tree does not match the manifest evidence.");
  }
  const entry = resolve(packageRoot, health.runtime_entry);
  const entryBytes = await readFile(entry);
  if (entryBytes.length !== health.runtime_entry_size
      || createHash("sha256").update(entryBytes).digest("hex") !== health.runtime_entry_sha256) {
    throw new Error("Installed MCP runtime entry does not match the manifest evidence.");
  }
  return entry;
}

function probeTools(entry, childEnvironment, projectRoot) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(process.execPath, [entry], {
      cwd: projectRoot,
      env: childEnvironment,
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    const lines = createInterface({ input: child.stdout });
    const stderr = [];
    let stderrBytes = 0;
    child.stderr.on("data", (chunk) => {
      stderrBytes += chunk.length;
      if (stderrBytes <= 4096) stderr.push(chunk);
    });
    let settled = false;
    let timer;
    const finish = (error, tools) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      lines.close();
      child.kill();
      if (error) reject(error);
      else resolvePromise(tools);
    };
    lines.on("line", (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        finish(new Error("MCP emitted a non-JSONL stdout record."));
        return;
      }
      if (message.id === 1) {
        if (message.error) {
          finish(new Error("MCP initialize failed."));
          return;
        }
        if (message.result?.protocolVersion !== "2025-11-25"
            || message.result?.serverInfo?.version !== health.package_version
            || message.result?.capabilities?.tools === undefined) {
          finish(new Error("MCP initialize response did not match the reviewed protocol and package."));
          return;
        }
        child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n");
        child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }) + "\n");
      } else if (message.id === 2) {
        if (message.error || !Array.isArray(message.result?.tools)) {
          finish(new Error("MCP tools/list failed."));
          return;
        }
        const names = message.result.tools.map((tool) => tool.name).filter((name) => typeof name === "string");
        const missing = health.required_tools.filter((name) => !names.includes(name));
        if (missing.length) {
          finish(new Error("MCP tools/list omitted " + missing.length + " manifest-required routes."));
          return;
        }
        finish(null, names);
      }
    });
    child.once("error", () => finish(new Error("MCP runtime could not start.")));
    child.once("close", (code) => {
      if (!settled) {
        let diagnostic = Buffer.concat(stderr).toString("utf8");
        for (const value of Object.values(childEnvironment)) {
          if (value && typeof value === "string") diagnostic = diagnostic.replaceAll(value, "<temp>");
        }
        diagnostic = diagnostic
          .replace(/\b(?:sk-[A-Za-z0-9_-]{8,}|npm_[A-Za-z0-9]{8,})\b/g, "<redacted>")
          .replace(/(token|password|authorization|_auth)\s*[:=]\s*[^\s]+/gi, "$1=<redacted>")
          .replace(/https?:\/\/[^\s]+/gi, "<url>")
          .replace(/\s+/g, " ")
          .trim();
        finish(new Error("MCP exited before tools/list completed (" + code + ")"
          + (diagnostic ? ": " + diagnostic.slice(0, 180) : ".")));
      }
    });
    timer = setTimeout(() => finish(new Error("MCP initialize/tools/list exceeded its 15-second bound.")), 15_000);
    child.stdin.write(JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-11-25",
        capabilities: {},
        clientInfo: { name: "hoi4-mod-setup-readiness", version: "0.3.5" },
      },
    }) + "\n");
  });
}

requireEvidence();
const nodeDirectory = dirname(process.execPath);
const npmCli = resolve(nodeDirectory, "node_modules", "npm", "bin", "npm-cli.js");
const tempRoot = await mkdtemp(join(tmpdir(), "hoi4-mod-setup-mcp-live-"));
const tempBase = resolve(tmpdir());
const tempPrefix = resolve(tempRoot, "appdata", "npm");
const testProject = resolve(tempRoot, "project");
const isolatedHome = resolve(tempRoot, "home");
const isolatedCodexHome = resolve(tempRoot, "codex");
const emptyUserNpmConfig = resolve(tempRoot, "user.npmrc");
const emptyGlobalNpmConfig = resolve(tempRoot, "global.npmrc");
const npmCache = resolve(tempRoot, "npm-cache");
await Promise.all([
  mkdir(tempPrefix, { recursive: true }),
  mkdir(testProject, { recursive: true }),
  mkdir(isolatedHome, { recursive: true }),
  mkdir(isolatedCodexHome, { recursive: true }),
  writeFile(emptyUserNpmConfig, "", { encoding: "utf8", flag: "wx" }),
  writeFile(emptyGlobalNpmConfig, "", { encoding: "utf8", flag: "wx" }),
]);
await Promise.all([
  mkdir(resolve(testProject, "common"), { recursive: true }),
  mkdir(resolve(testProject, "events"), { recursive: true }),
  mkdir(resolve(testProject, "localisation", "english"), { recursive: true }),
  writeFile(resolve(testProject, "descriptor.mod"), 'name = "HOI4 Mod Setup MCP smoke"\nsupported_version = "1.17.*"\npicture = "thumbnail.png"\n', { encoding: "utf8", flag: "wx" }),
  writeFile(resolve(testProject, "README.md"), "# HOI4 Mod Setup MCP smoke\n", { encoding: "utf8", flag: "wx" }),
  copyFile(resolve(root, "src-tauri", "icons", "icon.png"), resolve(testProject, "thumbnail.png")),
]);
const resolvedRoot = await realpath(tempRoot);
if (!resolvedRoot.toLowerCase().startsWith(tempBase.toLowerCase() + "\\")) {
  throw new Error("Refusing to use a temporary MCP directory outside the system temp root.");
}
const environment = {
  PATH: nodeDirectory,
  APPDATA: resolve(tempRoot, "appdata"),
  LOCALAPPDATA: resolve(tempRoot, "localappdata"),
  HOME: isolatedHome,
  CODEX_HOME: isolatedCodexHome,
  TEMP: tempRoot,
  TMP: tempRoot,
  SystemRoot: process.env.SystemRoot,
  USERPROFILE: tempRoot,
  NPM_CONFIG_USERCONFIG: emptyUserNpmConfig,
  NPM_CONFIG_GLOBALCONFIG: emptyGlobalNpmConfig,
  NPM_CONFIG_CACHE: npmCache,
  NPM_CONFIG_AUDIT: "false",
  NPM_CONFIG_FUND: "false",
  CI: "true",
};

try {
  const integrityText = await runNode([
    npmCli,
    "view",
    health.package_name + "@" + health.package_version,
    "dist.integrity",
    "--json",
    "--registry=https://registry.npmjs.org",
  ], { cwd: root, env: environment }, "integrity request");
  if (JSON.parse(integrityText) !== health.package_integrity) {
    throw new Error("npm registry integrity does not match the bundled manifest.");
  }
  await runNode([
    npmCli,
    "install",
    "--global",
    "--prefix",
    tempPrefix,
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    "--registry=https://registry.npmjs.org",
    health.package_name + "@" + health.package_version,
  ], { cwd: root, env: environment }, "package install", 180_000);
  const entry = await verifyInstalledPackage(tempPrefix);
  const tools = await probeTools(entry, environment, testProject);
  process.stdout.write("MCP live check passed: " + health.package_name + "@" + health.package_version
    + "; " + tools.length + " tools advertised; all " + health.required_tools.length + " manifest routes present.\n");
} finally {
  await rm(resolvedRoot, { recursive: true, force: true });
}
