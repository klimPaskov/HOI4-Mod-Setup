import { stageVerifiedLocalTarball } from "./mcp_local_tarball.mjs";
import { isContained, packageFiles } from "./mcp_package_integrity.mjs";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { copyFile, lstat, mkdir, mkdtemp, readFile, readlink, realpath, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { createInterface } from "node:readline";

const root = resolve(import.meta.dirname, "..");
const clientVersion = JSON.parse(await readFile(resolve(root, "package.json"), "utf8")).version;
const manifestPath = resolve(root, "docs", "source-manifest", "hoi4-mod-setup.manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const component = manifest.components.find((candidate) => candidate.id === "mcp.hoi4_agent_tools");
const health = component?.validation?.find((rule) => rule.id === "mcp.hoi4.health")?.parameters;
const platform = process.platform === "win32" ? "windows" : process.platform;
const pin = health?.platform_pins?.[platform] ?? (platform === "windows" ? health : undefined);
const maxOutputBytes = 2 * 1024 * 1024;

function requireEvidence() {
  if (!["windows", "linux"].includes(platform)) {
    throw new Error("The reviewed HOI4 Agent Tools MCP route supports Windows and native Linux.");
  }
  if (!health || health.package_name !== "hoi4-agent-tools" || !/^\d+\.\d+\.\d+$/.test(health.package_version ?? "")) {
    throw new Error("The bundled manifest does not declare an exact reviewed MCP package version.");
  }
  if (!Array.isArray(health.required_tools) || health.required_tools.length !== 28) {
    throw new Error("The bundled MCP tool list does not match the current source contract.");
  }
  if (!/^sha512-[A-Za-z0-9+/]{86}==$/.test(health.package_integrity ?? "")
      || !/^[a-f0-9]{64}$/.test(pin?.package_tree_sha256 ?? "")
      || !/^[a-f0-9]{64}$/.test(pin?.runtime_entry_sha256 ?? "")
      || !Number.isSafeInteger(pin?.package_file_count)
      || !Number.isSafeInteger(pin?.runtime_entry_size)) {
    throw new Error("The bundled MCP integrity evidence is incomplete.");
  }
  if (platform === "linux" && health.platform_targets?.linux !== "hoi4-agent-tools") {
    throw new Error("The manifest does not declare a verified native Linux target.");
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
      reject(new Error(`${stage} exceeded the bounded ${Math.ceil(timeoutMs / 1000)} second limit.`));
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

async function verifyInstalledPackage(prefix) {
  const modules = resolve(prefix, ...(platform === "linux" ? ["lib", "node_modules"] : ["node_modules"]));
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
  const tree = await packageFiles(packageRoot, pin.package_symlinks ?? {});
  if (process.env.HOI4_MCP_DIAGNOSTICS === "1") {
    if (process.env.GITHUB_ACTIONS !== "true" || process.env.RUNNER_ENVIRONMENT !== "github-hosted") throw new Error("MCP diagnostic inventory is hosted-runner-only.");
    const output = resolve(root, "test-results", "mcp-runtime");
    await mkdir(output, { recursive: true });
    await writeFile(join(output, `tree-${platform}-node${process.versions.node}.json`), JSON.stringify({
      package: health.package_name, version: health.package_version, integrity: health.package_integrity,
      node: process.versions.node, npm: await runNode([npmCli, "--version"], { cwd: root, env: environment }, "npm version"),
      expected: { sha256: pin.package_tree_sha256, count: pin.package_file_count },
      actual: { sha256: tree.sha256, count: tree.files.length },
      files: tree.files.map(({ path, bytes }) => ({ path, size: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") })),
    }, null, 2) + "\n");
  }
  if (tree.sha256 !== pin.package_tree_sha256 || tree.files.length !== pin.package_file_count) {
    throw new Error(`Installed MCP package tree does not match the manifest evidence: expected ${pin.package_tree_sha256}/${pin.package_file_count}, received ${tree.sha256}/${tree.files.length}.`);
  }
  const entry = resolve(packageRoot, health.runtime_entry);
  const entryBytes = await readFile(entry);
  if (entryBytes.length !== pin.runtime_entry_size
      || createHash("sha256").update(entryBytes).digest("hex") !== pin.runtime_entry_sha256) {
    throw new Error("Installed MCP runtime entry does not match the manifest evidence.");
  }
  if (platform === "linux") {
    const locator = resolve(prefix, "bin", "hoi4-agent-tools");
    if (!(await lstat(locator)).isSymbolicLink()
        || await readlink(locator) !== `../lib/node_modules/${health.package_name}/${health.runtime_entry}`
        || await realpath(locator) !== await realpath(entry)) {
      throw new Error("The native MCP locator does not match the reviewed entry.");
    }
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
      // Settle only after the server exits, so its working directory is
      // released before the temporary tree is removed.
      const settle = () => (error ? reject(error) : resolvePromise(tools));
      if (child.exitCode !== null || child.signalCode !== null) {
        settle();
        return;
      }
      const exitTimer = setTimeout(settle, 10_000);
      child.once("exit", () => {
        clearTimeout(exitTimer);
        settle();
      });
      child.kill();
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
    timer = setTimeout(() => finish(new Error("MCP initialize/tools/list exceeded its 120-second bound.")), 120_000);
    child.stdin.write(JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-11-25",
        capabilities: {},
        clientInfo: { name: "hoi4-mod-setup-readiness", version: clientVersion },
      },
    }) + "\n");
  });
}

requireEvidence();
const nodeDirectory = dirname(process.execPath);
const npmCli = platform === "linux" ? await realpath(join(nodeDirectory, "npm")) : resolve(nodeDirectory, "node_modules", "npm", "bin", "npm-cli.js");
if (!(await lstat(npmCli)).isFile() || (platform === "linux" && !npmCli.endsWith("/npm-cli.js"))) {
  throw new Error("The native npm JavaScript entry is unavailable.");
}
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
if (!isContained(await realpath(tempBase), resolvedRoot)) {
  throw new Error("Refusing to use a temporary MCP directory outside the system temp root.");
}
const environment = {
  PATH: platform === "linux" ? "/usr/bin:/bin" : nodeDirectory,
  XDG_DATA_HOME: resolve(tempRoot, "xdg-data"),
  XDG_CONFIG_HOME: resolve(tempRoot, "xdg-config"),
  XDG_CACHE_HOME: resolve(tempRoot, "xdg-cache"),
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
  const localTarball = process.env.HOI4_MCP_LOCAL_TARBALL;
  let installSource = health.package_name + "@" + health.package_version;
  if (localTarball !== undefined) {
    installSource = await stageVerifiedLocalTarball(
      localTarball, health.package_integrity, resolve(tempRoot, "verified-mcp.tgz"),
    );
  } else {
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
    installSource,
  ], { cwd: root, env: environment }, "package install", 600_000);
  const entry = await verifyInstalledPackage(tempPrefix);
  const tools = await probeTools(entry, environment, testProject);
  process.stdout.write("MCP live check passed: " + health.package_name + "@" + health.package_version
    + "; " + tools.length + " tools advertised; all " + health.required_tools.length + " manifest routes present.\n");
} finally {
  await rm(resolvedRoot, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
}
