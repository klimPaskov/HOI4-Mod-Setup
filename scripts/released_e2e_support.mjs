import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { lstat, mkdir, readdir, readFile, realpath } from "node:fs/promises";
import { createServer } from "node:http";
import { isAbsolute, join, relative, resolve } from "node:path";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";

export function requireHostedWindows() {
  if (process.platform !== "win32" || process.env.GITHUB_ACTIONS !== "true"
    || process.env.RUNNER_ENVIRONMENT !== "github-hosted" || !isAbsolute(process.env.RUNNER_TEMP ?? "")) {
    throw new Error("Published-app E2E runs only on a disposable GitHub-hosted Windows runner, never a local desktop.");
  }
}

export function inside(root, path) {
  const value = relative(resolve(root), resolve(path));
  if (!value || value.startsWith("..") || isAbsolute(value)) throw new Error("Path is outside its owned test root.");
  return resolve(path);
}

export function reviewedApplyLabel(title, maintenance) {
  if (title === "Review changes") return "Start installation";
  if (maintenance && title === "Resolve file conflicts") return "Apply";
  if (maintenance && title === "Update and repair") return "Apply reviewed plan";
  return undefined;
}

export async function fileHash(path) {
  const hash = createHash("sha256");
  for await (const bytes of createReadStream(path)) hash.update(bytes);
  return hash.digest("hex");
}

export async function verifiedCandidate(root, version, revision) {
  const rootMetadata = await lstat(root);
  assert.ok(rootMetadata.isDirectory() && !rootMetadata.isSymbolicLink());
  async function regularArtifact(name) {
    const path = inside(root, join(root, name));
    const parts = relative(root, path).split(/[\\/]/);
    let current = root;
    for (const [index, part] of parts.entries()) {
      current = join(current, part);
      const metadata = await lstat(current);
      assert.ok(!metadata.isSymbolicLink());
      assert.ok(index === parts.length - 1 ? metadata.isFile() : metadata.isDirectory());
    }
    return path;
  }
  const metadata = JSON.parse(await readFile(await regularArtifact("BUILD_METADATA.json"), "utf8"));
  assert.equal(metadata.product, "HOI4 Mod Setup");
  assert.equal(metadata.version, version);
  assert.equal(metadata.sourceRevision, revision);
  assert.match(revision, /^[a-f0-9]{40}$/);
  assert.equal(metadata.frontendOnly, false);
  assert.equal(metadata.platform, "Windows");
  assert.equal(metadata.architecture, "X64");
  assert.equal(metadata.bundle, "nsis");
  assert.equal(metadata.artifact_manifest, "ARTIFACTS.sha256");
  const entries = JSON.parse(await readFile(await regularArtifact("ARTIFACTS.sha256"), "utf8"));
  assert.ok(Array.isArray(entries) && entries.length < 10_000);
  const names = new Set();
  for (const entry of entries) {
    assert.equal(typeof entry.path, "string");
    assert.ok(!names.has(entry.path) && !entry.path.includes("\\") && !entry.path.split("/").includes(".."));
    assert.match(entry.sha256, /^[a-f0-9]{64}$/);
    names.add(entry.path);
    const path = await regularArtifact(entry.path);
    assert.equal(await fileHash(path), entry.sha256);
  }
  assert.ok(names.has("BUILD_METADATA.json"));
  const installers = entries.filter((entry) => /^packages\/nsis\/[^/]+\.exe$/i.test(entry.path));
  assert.equal(installers.length, 1);
  const installer = inside(root, join(root, installers[0].path));
  return { installer, sha256: installers[0].sha256, size: (await lstat(installer)).size, sourceRevision: metadata.sourceRevision };
}

export function cleanProcessEnvironment(profile) {
  const keep = new Set(["PATH", "PATHEXT", "SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC", "PROGRAMFILES", "PROGRAMFILES(X86)", "PROGRAMW6432", "PROGRAMDATA", "ALLUSERSPROFILE", "PROCESSOR_ARCHITECTURE", "PROCESSOR_ARCHITEW6432", "NUMBER_OF_PROCESSORS"]);
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => keep.has(name.toUpperCase())));
  return {
    ...env, HOME: join(profile, "home"), USERPROFILE: join(profile, "home"),
    APPDATA: join(profile, "config"), LOCALAPPDATA: join(profile, "data"),
    TEMP: join(profile, "temp"), TMP: join(profile, "temp"),
  };
}

export function cleanAppEnvironment(profile, cdpPort, browserProfile) {
  return {
    ...cleanProcessEnvironment(profile),
    WEBVIEW2_USER_DATA_FOLDER: browserProfile,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-address=127.0.0.1 --remote-debugging-port=${cdpPort}`,
  };
}

export async function setupPythonPath(location, cacheRoot) {
  if (!isAbsolute(location ?? "") || !isAbsolute(cacheRoot ?? "")) throw new Error("Missing setup-python runtime identity.");
  const path = await realpath(join(location, "python.exe"));
  inside(cacheRoot, path);
  if (!/[\\/]Python[\\/]3\.13\.\d+[\\/]x64[\\/]python\.exe$/i.test(path)) throw new Error("Unexpected setup-python runtime path.");
  return path;
}

export function sanitizeReport(value, knownSecrets = []) {
  let json = JSON.stringify(value);
  for (const secret of knownSecrets.filter((item) => typeof item === "string" && item.length > 3)) json = json.replaceAll(secret, "[REDACTED]");
  json = json.replace(/(?:sk-|gh[pousr]_|github_pat_|AIza)[A-Za-z0-9_-]+/g, "[REDACTED]")
    .replace(/Bearer\s+[A-Za-z0-9_.+\/-]+/gi, "Bearer [REDACTED]");
  return JSON.parse(json);
}

export async function stopOwnedSession(stopProcess, closeBrowser, clearPolicy) {
  const errors = [];
  for (const operation of [stopProcess, closeBrowser, clearPolicy]) {
    try { await operation(); } catch (error) { errors.push(error); }
  }
  if (errors.length) throw new AggregateError(errors, "Owned session cleanup failed.");
}

export async function downloadPublic(url, destination, maximumBytes, expectedHash) {
  const allowed = new Set(["github.com", "release-assets.githubusercontent.com", "codeload.github.com"]);
  let current = url;
  for (let redirect = 0; redirect < 5; redirect++) {
    const parsed = new URL(current);
    if (parsed.protocol !== "https:" || !allowed.has(parsed.hostname) || parsed.username || parsed.password || parsed.port) throw new Error("Unreviewed download origin.");
    const response = await fetch(current, { redirect: "manual", signal: AbortSignal.timeout(600_000) });
    if ([301, 302, 303, 307, 308].includes(response.status)) {
      current = new URL(response.headers.get("location"), current).href;
      await response.body?.cancel();
      continue;
    }
    if (!response.ok || !response.body) throw new Error(`Public download failed with HTTP ${response.status}.`);
    let size = 0;
    const hash = createHash("sha256");
    await mkdir(resolve(destination, ".."), { recursive: true });
    await pipeline(Readable.fromWeb(response.body), new Transform({ transform(chunk, _encoding, callback) {
      size += chunk.length;
      if (size > maximumBytes) { callback(new Error("Download exceeded its byte limit.")); return; }
      hash.update(chunk); callback(null, chunk);
    } }), createWriteStream(destination, { flags: "wx" }));
    const sha256 = hash.digest("hex");
    if (expectedHash) assert.equal(sha256, expectedHash, "Published artifact digest changed.");
    return { url, size, sha256 };
  }
  throw new Error("Download exceeded its redirect limit.");
}

export async function githubJson(path, token) {
  if (!path.startsWith("/repos/Kaiserreich/Kaiserreich-HOI4/git/")) throw new Error("Unreviewed fixture metadata route.");
  const response = await fetch(`https://api.github.com${path}`, {
    redirect: "error", signal: AbortSignal.timeout(60_000),
    headers: { Accept: "application/vnd.github+json", "User-Agent": "hoi4-mod-setup-remote-e2e",
      ...(token ? { Authorization: `Bearer ${token}` } : {}) },
  });
  if (!response.ok) throw new Error(`Fixture metadata failed with HTTP ${response.status}.`);
  const chunks = []; let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > 24 * 1024 * 1024) throw new Error("Fixture metadata is too large.");
    chunks.push(Buffer.from(chunk));
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

export async function snapshot(root) {
  const files = new Map();
  async function walk(directory, depth) {
    if (depth > 40 || files.size > 100_000) throw new Error("Test project inventory exceeded its bound.");
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = inside(root, join(directory, entry.name));
      const meta = await lstat(path);
      if (meta.isSymbolicLink()) throw new Error("Test project contains a link.");
      if (meta.isDirectory()) await walk(path, depth + 1);
      else if (meta.isFile()) files.set(relative(root, path).replaceAll("\\", "/"), { sha256: await fileHash(path), size: meta.size });
      else throw new Error("Test project contains a special file.");
    }
  }
  await walk(root, 0);
  return files;
}

export function inventoryDigest(files) {
  return createHash("sha256").update(JSON.stringify([...files].sort(([a], [b]) => a.localeCompare(b)))).digest("hex");
}

export async function assertOriginals(root, original, { exact = false, gitignoreText } = {}) {
  const current = await snapshot(root);
  for (const [path, evidence] of original) {
    const actual = current.get(path);
    assert.ok(actual, `Original file disappeared: ${path}`);
    if (!exact && path === ".gitignore" && actual.sha256 !== evidence.sha256 && gitignoreText !== undefined) {
      const lines = (await readFile(join(root, path), "utf8")).replaceAll("\r\n", "\n").split("\n");
      let cursor = 0;
      for (const line of gitignoreText.replaceAll("\r\n", "\n").split("\n").filter(Boolean)) {
        const next = lines.indexOf(line, cursor);
        assert.ok(next >= cursor, "The managed ignore merge removed or reordered original rules.");
        cursor = next + 1;
      }
    } else assert.equal(actual.sha256, evidence.sha256, `Original file changed: ${path}`);
  }
  if (exact) assert.equal(inventoryDigest(current), inventoryDigest(original), "The project was not restored exactly.");
  return { originalFiles: original.size, currentFiles: current.size, originalDigest: inventoryDigest(original) };
}

export async function startPlannerFixture({ malformedFirst = false } = {}) {
  const requests = [];
  const server = createServer(async (request, response) => {
    const send = (status, value) => { response.writeHead(status, { "Content-Type": "application/json" }); response.end(JSON.stringify(value)); };
    try {
      if (request.headers.authorization) throw new Error("A local fixture must not receive credentials.");
      if (request.method === "GET" && request.url === "/v1/models") {
        send(200, { object: "list", data: [{ id: "e2e-planner", object: "model", supported_reasoning_efforts: ["high"] }] }); return;
      }
      if (request.method !== "POST" || request.url !== "/v1/chat/completions") { send(404, { error: "unknown fixture route" }); return; }
      const chunks = []; let size = 0;
      for await (const chunk of request) { size += chunk.length; if (size > 1024 * 1024) throw new Error("Fixture input is too large."); chunks.push(chunk); }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      assert.equal(body.model, "e2e-planner");
      assert.ok(!body.tools?.length);
      const prompt = body.messages.find((message) => message.role === "user")?.content;
      const match = prompt?.match(/\ninput_sha256=([a-f0-9]{64})\ninput=([\s\S]*)/);
      assert.ok(match, "Missing approved input binding.");
      const inputJson = match[2].split("\n\nThe previous response was rejected")[0];
      assert.equal(createHash("sha256").update(inputJson).digest("hex"), match[1], "Approved input digest mismatch.");
      const input = JSON.parse(inputJson);
      const existing = input.mode === "existing_project_semantics";
      const name = input.constraints?.requested_mod_name || (existing ? "Kaiserreich" : "Remote Acceptance Mod");
      const id = name.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");
      const description = existing ? "An existing alternative-history mod for Hearts of Iron IV." : "An alternative-history mod focused on diplomacy in Europe.";
      const values = { display_name: name, project_id: id, script_prefix: id, primary_namespace: id, descriptor_tags: ["Alternative History"], folder_profile: ["common", "events", "gfx", "localisation/english", "docs"], project_description: description, agents_profile: "default", localisation_convention: "english", documentation_convention: "markdown" };
      const result = { schema_version: "1.0.0", analysis_id: "remote-e2e", mode: input.mode, input_sha256: match[1], project_summary: description,
        proposals: Object.entries(values).map(([key, value]) => ({ key, value, confidence: 0.8, reason: "The mod name and brief provide a consistent starting point.", evidence_refs: [] })), component_recommendations: [], warnings: [] };
      requests.push({ mode: input.mode, inputSha256: match[1], correction: prompt.includes("The previous response was rejected") });
      const content = malformedFirst && requests.length === 1 ? "{}" : JSON.stringify(result);
      send(200, { choices: [{ message: { role: "assistant", content }, finish_reason: "stop" }] });
    } catch { send(400, { error: "invalid bounded fixture request" }); }
  });
  await new Promise((resolvePromise) => server.listen(0, "127.0.0.1", resolvePromise));
  return { endpoint: `http://127.0.0.1:${server.address().port}/v1/chat/completions`, requests,
    close: () => new Promise((resolvePromise, reject) => server.close((error) => error ? reject(error) : resolvePromise())) };
}
