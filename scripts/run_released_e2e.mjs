import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { createHash } from "node:crypto";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import { chromium, expect } from "@playwright/test";
import { assertWindowsInstallerRegistryIsClean, cleanupWindowsInstallerRegistry, resolveNativeSystemExecutable, resolveSystemPowerShellPath, uninstallKey, windowsRegistry } from "./windows_installer_registry.mjs";
import { assertOriginals, cleanAppEnvironment, cleanProcessEnvironment, downloadPublic, fileHash, githubJson, inside, inventoryDigest, requireHostedWindows, reviewedApplyLabel, sanitizeReport, setupPythonPath, snapshot, startPlannerFixture, stopOwnedSession, verifiedCandidate } from "./released_e2e_support.mjs";
import { configureHostedWebView } from "./released_e2e_windows.mjs";

requireHostedWindows();
const metadataToken = process.env.GITHUB_TOKEN;
for (const name of Object.keys(process.env)) {
  if (/TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTHORIZATION|API.?KEY|^ACTIONS_|^ANTHROPIC_|^OPENAI_|^MESHY_|^CODEX_HOME$/i.test(name)) delete process.env[name];
}
const exec = promisify(execFile);
const repository = resolve(import.meta.dirname, "..");
const pins = JSON.parse(await readFile(join(repository, "tests/remote/released-app-pins.json"), "utf8"));
const candidateDirectory = process.env.HOI4_E2E_CANDIDATE_DIR;
const applicationKind = candidateDirectory ? "candidate" : "published";
if (candidateDirectory) {
  inside(repository, candidateDirectory);
  pins.app.version = JSON.parse(await readFile(join(repository, "package.json"), "utf8")).version;
  pins.app.commit = process.env.GITHUB_SHA;
}
let expectedSourceRevision = candidateDirectory ? pins.candidate.sourceRevision : pins.app.sourceRevision;
const scenario = process.argv[2];
assert.ok(["new", "kaiserreich", "recovery", "clients"].includes(scenario), "Unknown remote scenario.");
// The clients scenario creates a new mod like "new", then moves it to the
// pinned source revision that publishes the additional coding clients.
const fresh = scenario === "new" || scenario === "clients";
const clientSourceRevision = pins.clientPackages?.sourceRevision;
if (scenario === "clients") {
  assert.ok(candidateDirectory, "The clients scenario exercises a same-workflow candidate.");
  assert.match(clientSourceRevision ?? "", /^[0-9a-f]{40}$/, "Pin the source revision that publishes the client packages.");
}
const workRoot = await mkdtemp(join(process.env.RUNNER_TEMP, "hoi4-mod-setup-installer-e2e-"));
const installRoot = join(workRoot, "installed");
const profile = join(workRoot, "profile");
const mods = join(workRoot, "mods");
const output = resolve(repository, "test-results", `released-${scenario}`);
for (const path of [output, mods, ...["home", "config", "data", "temp"].map((part) => join(profile, part))]) await mkdir(path, { recursive: true });
const journalRoot = join(profile, "data", "HOI4 Mod Setup", "transactions");
const project = join(mods, fresh ? "remote_acceptance_mod" : "kaiserreich");
const result = { scenario, applicationKind, applicationVersion: pins.app.version, releaseSourceCommitFromPin: pins.app.commit, runner: process.env.RUNNER_OS,
  ai: "deterministic loopback HTTP fixture; real provider authentication and semantic quality are not tested",
  nativeIpc: `unmodified ${applicationKind} binary; no IPC mocks or application test hooks`, stages: [], screenshots: [], success: false };
let app;
let browser;
let page;
let planner;
let executable;
let ownsRegistry = false;
let original = new Map();
let gitignoreText;
let failure;
let clearWebViewPolicy;

const pause = (ms) => new Promise((done) => setTimeout(done, ms));
const heading = () => page.locator("#screen-title");
async function waitUntil(check, milliseconds, label) {
  const deadline = Date.now() + milliseconds;
  while (Date.now() < deadline) {
    const value = await check();
    if (value) return value;
    await pause(500);
  }
  throw new Error(`Timed out: ${label}`);
}
async function record(name) {
  result.stages.push({ name, at: new Date().toISOString(), heading: await heading().innerText() });
  await heading().scrollIntoViewIfNeeded();
  const file = `${String(result.stages.length).padStart(2, "0")}-${name.replace(/[^a-z0-9-]/gi, "-")}.png`;
  await page.screenshot({ path: join(output, file) });
  result.screenshots.push(file);
  console.log(`E2E ${scenario}: ${name}`);
}
async function next(name = "Next") { await page.getByRole("button", { name, exact: true }).click({ timeout: 180_000 }); }
async function at(title) { await expect(heading()).toHaveText(title, { timeout: 180_000 }); }
async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const port = server.address().port;
  await new Promise((done) => server.close(done));
  return port;
}
async function launch(viewName) {
  const port = await freePort();
  const env = cleanAppEnvironment(profile, port, join(profile, viewName));
  assert.ok(!Object.keys(env).some((key) => /TOKEN|SECRET|PASSWORD|CREDENTIAL|^GH_|^GITHUB_|^ANTHROPIC_|^OPENAI_|^CODEX_HOME$/.test(key)));
  clearWebViewPolicy = await configureHostedWebView(env);
  app = spawn(executable, [], { cwd: workRoot, env, stdio: "ignore", windowsHide: true });
  await new Promise((done, reject) => { app.once("spawn", done); app.once("error", reject); });
  await waitUntil(async () => {
    if (app.exitCode !== null) throw new Error("Installed application exited before WebView2 was ready.");
    try { return (await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1000) })).ok; } catch { return false; }
  }, 90_000, "published WebView2 CDP readiness");
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  page = await waitUntil(async () => browser.contexts().flatMap((context) => context.pages()).find((candidate) => /^https?:\/\/tauri\.localhost\/?/.test(candidate.url())), 30_000, "production Tauri page");
  page.setDefaultTimeout(30_000);
  page.on("pageerror", (error) => { result.pageErrors ??= []; if (result.pageErrors.length < 20) result.pageErrors.push(error.message.slice(0, 500)); });
  await at("Start a mod project");
}
async function stop() {
  await stopOwnedSession(async () => {
    if (app?.pid && app.exitCode === null && app.signalCode === null) {
      await exec(resolveNativeSystemExecutable("taskkill.exe"), ["/PID", String(app.pid), "/T", "/F"], { windowsHide: true, timeout: 20_000 });
      await waitUntil(() => app.exitCode !== null || app.signalCode !== null, 20_000, "owned app exit");
    }
  }, async () => { await browser?.close(); browser = undefined; page = undefined; },
  async () => { await clearWebViewPolicy?.(); clearWebViewPolicy = undefined; });
  app = undefined;
}
async function journals() {
  if (!existsSync(journalRoot)) return [];
  const values = [];
  for (const name of await readdir(journalRoot)) {
    if (!/^[a-f0-9-]{36}$/.test(name)) continue;
    try { values.push(JSON.parse(await readFile(join(journalRoot, name, "journal.json"), "utf8"))); } catch { /* A checkpoint may be replacing the JSON. */ }
  }
  return values.sort((a, b) => a.created_at.localeCompare(b.created_at));
}
async function completedTransaction(previousIds = []) {
  return waitUntil(async () => {
    const errors = await page.getByRole("alert").allTextContents();
    if (errors.some((text) => /could not|unavailable|failed|error|recovering the previous/i.test(text))) throw new Error(errors.join(" ").slice(0, 1500));
    const journal = (await journals()).findLast((item) => !previousIds.includes(item.transaction_id) && item.transaction_kind === "installation");
    if (journal?.state === "interrupted") throw new Error(`Installation interrupted: ${journal.error?.message ?? journal.last_checkpoint}`);
    return journal?.state === "completed" && journal;
  }, 20 * 60_000, "real installation completion");
}
async function approveSuggestions() {
  await page.getByRole("button", { name: "Confirm Local model suggestions", exact: true }).click();
  await expect(page.getByText("Local model suggestions confirmed", { exact: true })).toBeVisible();
}
async function commonSetup() {
  await at("Coding environments");
  await page.getByRole("radiogroup", { name: "Primary coding environment" }).getByRole("radio", { name: /^Claude Code/ }).check();
  await page.getByRole("checkbox", { name: /^Codex / }).check();
  await record("coding-environments"); await next();
  await at("Image generation");
  if (fresh) {
    await expect(page.getByRole("radio", { name: /^ChatGPT subscription \(GPTImage\)/ })).toBeChecked({ timeout: 180_000 });
  } else {
    await page.getByRole("radio", { name: /^fal\.ai / }).check();
    await page.getByRole("combobox", { name: "Model", exact: true }).selectOption({ label: "Ideogram V3 Transparent" });
  }
  await record("image-provider"); await next();
  await at("Choose what to install");
  await expect(page.getByRole("button", { name: /Image provider MCP adapter/ })).toHaveAttribute("aria-pressed", fresh ? "false" : "true");
  await expect(page.getByRole("button", { name: /GPTImage MCP source/ })).toHaveAttribute("aria-pressed", fresh ? "true" : "false");
  await record("selected-components"); await next();
  await at("Optional workflows");
  await expect(page.getByRole("switch", { name: /^HOI4 Live Tools/ })).toBeVisible();
  await next();
  await expect(heading()).toHaveText(/^(Live testing|MCP and credentials)$/);
  if (await heading().innerText() === "Live testing") { await page.getByRole("radio", { name: /^None / }).check(); await next(); }
  await at("MCP and credentials"); await record("integrations"); await next();
  await at("Choose Git setup");
  await page.getByRole("radio", { name: /^Skip Git setup/ }).check();
  await next();
  await waitForReview(false);
}
// `choices` maps a conflict path to the button that resolves it; any other
// conflict keeps the current file.
async function waitForReview(maintenance, choices = {}) {
  await waitUntil(async () => {
    const title = await heading().innerText();
    if (title === "Resolve file conflicts") {
      const done = page.getByRole("button", { name: maintenance ? "Apply" : "Review changes", exact: true });
      if (await done.isEnabled()) {
        if (!maintenance) await done.click();
        return maintenance;
      }
      const message = await page.locator(".conflict-actions p").innerText();
      const shown = await page.locator(".diff-grid").innerText();
      const choice = Object.entries(choices).find(([path]) => shown.includes(path))?.[1] ?? "Keep current";
      await page.getByRole("button", { name: choice, exact: true }).click();
      await waitUntil(async () => await done.isEnabled() || await page.locator(".conflict-actions p").innerText() !== message, 30_000, "conflict decision acknowledgement");
      return false;
    }
    const label = reviewedApplyLabel(title, maintenance);
    if (label) {
      const apply = page.getByRole("button", { name: label, exact: true });
      if (await apply.count() && await apply.isEnabled()) return true;
    }
    const errors = await page.getByRole("alert").allTextContents();
    if (errors.some((text) => /could not|unavailable|failed|error/i.test(text))) throw new Error(errors.join(" ").slice(0, 1500));
    return false;
  }, 15 * 60_000, "source preparation and reviewed conflicts");
  await record(maintenance ? "maintenance-review" : "dry-run");
}
async function applyAndReady(maintenance = false) {
  const previousIds = (await journals()).map((item) => item.transaction_id);
  const title = await heading().innerText();
  const label = reviewedApplyLabel(title, maintenance);
  assert.ok(label, "Installation requires a recognized reviewed-plan screen.");
  await next(label);
  const journal = await completedTransaction(previousIds);
  assert.equal(journal.stages.length, 12);
  assert.ok(journal.stages.every((stage) => stage.status === "complete"));
  if (!maintenance) {
    await expect(page.getByRole("button", { name: "Continue", exact: true })).toBeEnabled({ timeout: 60_000 });
    await next("Continue");
  }
  await at("Project ready");
  await expect(page.getByRole("heading", { name: / is ready$/ })).toBeVisible({ timeout: 90_000 });
  const readiness = JSON.parse(await readFile(join(journalRoot, journal.transaction_id, "readiness-report.json"), "utf8"));
  assert.equal(readiness.core_ready, true);
  assert.ok(!readiness.checks.some((check) => check.blocking && check.status === "block"));
  result.transactions ??= [];
  result.transactions.push({ id: journal.transaction_id, state: journal.state, stages: journal.stages.map(({ id, status }) => ({ id, status })), coreReady: readiness.core_ready });
  await record(maintenance ? "repaired-ready" : "installed-ready");
  return { journal, readiness };
}

// Coding clients published by the pinned source revision. Augment runs only on
// macOS and Linux, so this Windows run must not offer it.
const ADDED_CLIENTS = [
  ["gemini_cli", "Gemini CLI"], ["github_copilot", "GitHub Copilot (VS Code)"], ["antigravity", "Antigravity"],
  ["grok_build", "Grok Build"], ["kilo_code", "Kilo Code"], ["kiro", "Kiro"], ["qwen_code", "Qwen Code"],
  ["cline", "Cline"], ["devin_desktop", "Devin Desktop"], ["continue", "Continue"], ["aider", "Aider"],
];
const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
async function readProjectJson(path) { return JSON.parse(await readFile(inside(project, join(project, path)), "utf8")); }
async function readLock() { return readProjectJson(".hoi4-mod-setup/install.lock.json"); }
async function openMaintenance(target, ref) {
  await page.getByRole("button", { name: "Manage installation", exact: true }).click();
  await at("Update and repair");
  const version = page.getByRole("combobox", { name: "Target version", exact: true });
  await expect(version).toHaveValue("latest");
  if (target !== "latest") {
    await version.selectOption(target);
    await page.getByLabel("Commit", { exact: true }).fill(ref);
  }
}
async function reviewProject() {
  await page.getByRole("button", { name: "Scan the project", exact: true }).click();
  await page.getByRole("button", { name: "Run Local model reanalysis", exact: true }).click({ timeout: 180_000 });
  await page.getByRole("button", { name: "Confirm Local model suggestions", exact: true }).waitFor({ timeout: 180_000 });
  await approveSuggestions();
}
function clientCheckbox(label) {
  return page.locator(".maintenance-environment-panel").getByRole("checkbox", { name: new RegExp(`^${escapeRegExp(label)}(?:\\s|$)`) });
}
async function openAdditionalClients() {
  const details = page.locator(".maintenance-environment-panel details");
  if (!await details.evaluate((element) => element.open)) await details.locator("summary").click();
}
async function clientPackagesFlow() {
  // 1. Move the Latest (format 5) install to the pinned format 6 source and
  // add every newly published client. A Kiro MCP file the user already had is
  // merged with the shared route instead of being replaced.
  const kiroMcp = ".kiro/settings/mcp.json";
  const kiroUser = { kept_setting: { value: 1 }, mcpServers: { user_server: { command: "user-tool" } } };
  await mkdir(join(project, ".kiro/settings"), { recursive: true });
  await writeFile(join(project, kiroMcp), JSON.stringify(kiroUser, null, 2));
  await openMaintenance("pinned_commit", clientSourceRevision);
  await reviewProject();
  await openAdditionalClients();
  await expect(clientCheckbox("Gemini CLI")).toBeVisible({ timeout: 180_000 });
  await expect(clientCheckbox("Augment")).toHaveCount(0);
  for (const [, label] of ADDED_CLIENTS) await clientCheckbox(label).check();
  await record("clients-selected");
  await page.getByRole("button", { name: /^Check for updates/ }).click();
  await waitForReview(true, { [kiroMcp]: "Use new" });
  const { readiness: addedReadiness } = await applyAndReady(true);
  const added = await readLock();
  assert.equal(added.source.mode, "pinned_commit");
  assert.equal(added.source.revision, clientSourceRevision);
  assert.equal(added.primary_coding_environment, "claude_code");
  for (const [id] of ADDED_CLIENTS) assert.ok(added.additional_coding_environments.includes(id), `missing ${id}`);
  assert.ok(added.additional_coding_environments.includes("codex"));
  assert.equal(addedReadiness.checks.find((check) => check.id === "coding.environments").status, "pass");
  assert.equal(new Set(added.files.map((file) => `${file.external}:${file.path.toLowerCase()}`)).size, added.files.length);
  for (const path of [
    "GEMINI.md", ".gemini/agents/hoi4-repo-explorer.md", ".agents/agents/hoi4-repo-explorer.md", ".kilo/agents/hoi4-repo-explorer.md",
    ".kiro/agents/hoi4-repo-explorer.md", ".qwen/agents/hoi4-repo-explorer.md", ".devin/agents/hoi4-repo-explorer.md",
    ".grok/skills/hoi4-ai-strategies/SKILL.md", ".kiro/skills/hoi4-ai-strategies/SKILL.md", ".qwen/skills/hoi4-ai-strategies/SKILL.md",
    ".github/instructions/hoi4-mod-setup.instructions.md", ".clinerules/hoi4-mod-setup.md", ".continue/rules/hoi4-mod-setup.md", ".aider.conf.yml",
  ]) assert.ok(existsSync(join(project, path)), `missing ${path}`);
  assert.match(await readFile(join(project, "GEMINI.md"), "utf8"), /@\.\/AGENTS\.md/);
  // One shared .agents/skills copy serves Codex, Gemini CLI, Antigravity, Kilo Code, and Devin.
  const sharedSkill = added.files.filter((file) => !file.external && file.path === ".agents/skills/hoi4-ai-strategies/SKILL.md");
  assert.equal(sharedSkill.length, 1);
  // Every project MCP file registers the same verified, retargeted launch.
  const claudeRoute = (await readProjectJson(".mcp.json")).mcpServers.hoi4_agent_tools;
  assert.notEqual(claudeRoute.command, "hoi4-agent-tools.cmd", "Windows registrations must point at the reviewed user prefix.");
  for (const path of [".gemini/settings.json", ".agents/mcp_config.json", ".kiro/settings/mcp.json", ".qwen/settings.json"]) {
    const route = (await readProjectJson(path)).mcpServers.hoi4_agent_tools;
    assert.equal(route.command, claudeRoute.command, path);
    assert.deepEqual(route.args ?? [], claudeRoute.args ?? [], path);
  }
  const kiroMerged = await readProjectJson(kiroMcp);
  assert.deepEqual(kiroMerged.mcpServers.user_server, kiroUser.mcpServers.user_server, "the merge keeps the user's Kiro server");
  assert.deepEqual(kiroMerged.kept_setting, kiroUser.kept_setting);
  const kiroLocked = added.files.find((file) => !file.external && file.path === kiroMcp);
  assert.equal(kiroLocked?.ownership, "merged");
  assert.ok(!kiroLocked.installer_created, "a file the user had before setup is not installer-created");
  const vscode = (await readProjectJson(".vscode/mcp.json")).servers.hoi4_agent_tools;
  assert.equal(vscode.type, "stdio");
  assert.equal(vscode.command, claudeRoute.command);
  const kilo = (await readProjectJson(".kilo/kilo.json")).mcp.hoi4_agent_tools;
  assert.equal(kilo.type, "local");
  assert.deepEqual(kilo.command, [claudeRoute.command, ...(claudeRoute.args ?? [])]);
  const grok = await readFile(join(project, ".grok/config.toml"), "utf8");
  assert.match(grok, /\[mcp_servers\.hoi4_agent_tools\]/);
  assert.match(grok, /command = ["'][A-Za-z]:[\\/]/);
  for (const path of [".clinerules", ".continue/rules", ".aider.conf.yml"]) assert.ok(!(await readFile(join(project, path.endsWith(".yml") ? path : `${path}/hoi4-mod-setup.md`), "utf8")).includes("mcpServers"), path);
  result.clientPackages = { source: added.source, additional: added.additional_coding_environments, files: added.files.length };

  // 2. Deselect Kiro and GitHub Copilot after local edits: unchanged files are
  // removed, a modified agent and a user-extended MCP file are kept.
  const keptAgent = ".kiro/agents/hoi4-repo-explorer.md";
  const keptAgentText = `${await readFile(join(project, keptAgent), "utf8")}\nLocal note that must be kept.\n`;
  await writeFile(join(project, keptAgent), keptAgentText);
  const vscodeConfig = await readProjectJson(".vscode/mcp.json");
  vscodeConfig.servers.user_server = { type: "stdio", command: "user-tool" };
  await writeFile(join(project, ".vscode/mcp.json"), JSON.stringify(vscodeConfig, null, 2));
  await openMaintenance("pinned_commit", clientSourceRevision);
  await reviewProject();
  await openAdditionalClients();
  await clientCheckbox("Kiro").uncheck();
  await clientCheckbox("GitHub Copilot (VS Code)").uncheck();
  await record("clients-deselected");
  await page.getByRole("button", { name: /^Check for updates/ }).click();
  await waitForReview(true);
  await applyAndReady(true);
  const reduced = await readLock();
  assert.ok(!reduced.additional_coding_environments.includes("kiro"));
  assert.ok(!reduced.additional_coding_environments.includes("github_copilot"));
  assert.equal(await readFile(join(project, keptAgent), "utf8"), keptAgentText);
  assert.ok(!existsSync(join(project, ".kiro/agents/hoi4-skill-maintainer.md")), "unchanged Kiro agents are removed");
  assert.ok(!existsSync(join(project, ".kiro/skills/hoi4-ai-strategies/SKILL.md")), "the unchanged Kiro skill copy is removed");
  assert.ok(!existsSync(join(project, ".github/instructions/hoi4-mod-setup.instructions.md")));
  assert.equal((await readProjectJson(".vscode/mcp.json")).servers.user_server.command, "user-tool");
  assert.ok(!reduced.files.some((file) => !file.external && file.path.startsWith(".kiro/skills/")));
  // The unchanged, merged Kiro MCP file loses only the shared route, leaves the
  // lock, and its component is retired.
  assert.deepEqual(await readProjectJson(kiroMcp), kiroUser, "the deselected Kiro MCP file loses only the shared route");
  assert.ok(!reduced.files.some((file) => !file.external && file.path === kiroMcp), "the Kiro MCP file stays managed");
  assert.ok(!reduced.components.some((component) => component.id === kiroLocked.component_id), "the Kiro MCP component stays locked");
  // Kept edits of a deselected client leave management instead of holding
  // readiness in review.
  for (const path of [keptAgent, ".vscode/mcp.json"]) {
    assert.ok(!reduced.files.some((file) => !file.external && file.path === path), `${path} stays managed`);
    assert.ok(!(reduced.local_modifications ?? []).some((item) => item.path === path), `${path} stays a local modification`);
  }
  assert.ok(existsSync(join(project, ".qwen/skills/hoi4-ai-strategies/SKILL.md")), "other clients keep their packages");
  result.clientDeselection = { removed: ["kiro", "github_copilot"], keptModified: [keptAgent, ".vscode/mcp.json"], routeRemoved: [kiroMcp] };

  // 3. Repair restores a deleted new-client projection at the pinned revision.
  const repaired = reduced.files.find((file) => !file.external && file.path === ".gemini/agents/hoi4-repo-explorer.md");
  assert.ok(repaired);
  await rm(join(project, repaired.path));
  await page.getByRole("button", { name: "Manage installation", exact: true }).click();
  await at("Update and repair");
  await page.getByRole("button", { name: /^Repair installation/ }).click();
  await waitForReview(true); await applyAndReady(true);
  assert.equal(await fileHash(join(project, repaired.path)), repaired.installed_sha256);
  assert.deepEqual((await readLock()).source, reduced.source);

  // 4. Latest (format 5) cannot keep the new clients, so it refuses before
  // planning any removal and the project stays unchanged.
  const before = await readLock();
  await openMaintenance("latest");
  await reviewProject();
  await expect(page.getByText(/Installed but not in this source version:/)).toBeVisible({ timeout: 180_000 });
  await record("clients-unavailable-in-latest");
  await page.getByRole("button", { name: /^Check for updates/ }).click();
  // The update screen shows a planning refusal as its supporting text.
  await expect(page.getByText(/not included in this setup version|cannot be installed from this setup version/i).first()).toBeVisible({ timeout: 300_000 });
  await record("clients-refused-on-latest");
  assert.deepEqual(await readLock(), before);
  result.clientsRefusedOnOlderSource = true;
  await next("Back");
  await at("Project ready");
}

try {
  if (candidateDirectory) {
    const sourceRepository = await githubJson("/repos/klimPaskov/Agentic-HOI4-Modding", metadataToken);
    const sourceCommit = await githubJson(`/repos/klimPaskov/Agentic-HOI4-Modding/commits/${encodeURIComponent(sourceRepository.default_branch)}`, metadataToken);
    expectedSourceRevision = sourceCommit.sha;
    result.latestSourceAtStart = expectedSourceRevision;
  }
  const installer = join(workRoot, "installer.exe");
  if (candidateDirectory) {
    const candidate = await verifiedCandidate(candidateDirectory, pins.app.version, pins.app.commit);
    await copyFile(candidate.installer, installer);
    assert.equal(await fileHash(installer), candidate.sha256);
    result.installer = { origin: "same-workflow native build", sha256: candidate.sha256, size: candidate.size, sourceRevision: candidate.sourceRevision };
    result.signing = "Candidate artifact; not signed release evidence.";
  } else {
  result.installer = await downloadPublic(pins.app.url, installer, 32 * 1024 * 1024, pins.app.sha256);
  assert.equal(result.installer.size, pins.app.size);
  await exec(resolveSystemPowerShellPath(), ["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; Import-Module ($PSHOME + '\\Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1'); $s=Get-AuthenticodeSignature -LiteralPath $env:HOI4_E2E_INSTALLER; if (-not $s.SignerCertificate -or $s.SignerCertificate.GetNameInfo([Security.Cryptography.X509Certificates.X509NameType]::SimpleName,$false) -ne 'ChaosX' -or $s.Status -eq 'HashMismatch') { throw 'Published installer signature mismatch' }"], { env: { ...process.env, HOI4_E2E_INSTALLER: installer }, windowsHide: true, timeout: 30_000 });
  }
  await assertWindowsInstallerRegistryIsClean(); ownsRegistry = true;
  await exec(installer, ["/S", `/D=${installRoot}`], { windowsHide: true, timeout: 120_000 });
  executable = join(installRoot, "hoi4-mod-setup.exe");
  assert.ok(existsSync(executable));
  result.executableSha256 = await fileHash(executable);

  if (!fresh) {
    const version = scenario === "recovery" ? "1.6.4" : "1.7";
    const pin = pins.kaiserreich.versions[version];
    const commit = await githubJson(`/repos/${pins.kaiserreich.repository}/git/commits/${pin.commit}`, metadataToken);
    assert.equal(commit.sha, pin.commit);
    const tree = await githubJson(`/repos/${pins.kaiserreich.repository}/git/trees/${commit.tree.sha}?recursive=1`, metadataToken);
    assert.equal(tree.sha, commit.tree.sha); assert.equal(tree.truncated, false);
    const treePath = join(workRoot, "tree.json"); await writeFile(treePath, JSON.stringify(tree));
    const archive = join(workRoot, "kaiserreich.zip");
    const downloaded = await downloadPublic(`https://codeload.github.com/${pins.kaiserreich.repository}/zip/${pin.commit}`, archive, 1536 * 1024 * 1024);
    const python = await setupPythonPath(process.env.pythonLocation, process.env.RUNNER_TOOL_CACHE);
    const extracted = await exec(python, ["-I", "-S", join(repository, "scripts/extract_mod_fixture.py"), archive, project, treePath], {
      cwd: workRoot, env: { ...cleanProcessEnvironment(profile), GITHUB_ACTIONS: "true", RUNNER_ENVIRONMENT: "github-hosted" },
      windowsHide: true, timeout: 15 * 60_000, maxBuffer: 64 * 1024,
    });
    const descriptor = (await readFile(join(project, "descriptor.mod"), "utf8")).replaceAll("\r\n", "\n");
    assert.equal(createHash("sha256").update(descriptor).digest("hex"), pin.descriptor_sha256);
    assert.equal(await fileHash(join(project, "README.md")), pin.readme_sha256);
    result.mod = { version, commit: pin.commit, archive: downloaded, inventory: JSON.parse(extracted.stdout) };
    original = await snapshot(project);
    gitignoreText = existsSync(join(project, ".gitignore")) ? await readFile(join(project, ".gitignore"), "utf8") : undefined;
    result.originalDigest = inventoryDigest(original);
  }

  planner = await startPlannerFixture({ malformedFirst: scenario === "new", projectName: fresh ? "Remote Acceptance Mod" : "Kaiserreich" });
  await launch("webview-initial"); await record("welcome");
  await page.getByRole("button", { name: fresh ? /^Create new mod/ : /^Import existing mod/ }).click();
  await page.getByRole("combobox", { name: "AI provider", exact: true }).selectOption("local");
  await page.getByLabel("Model", { exact: true }).fill("e2e-planner");
  await page.getByLabel("Local model address", { exact: true }).fill(planner.endpoint);
  await page.getByRole("button", { name: /^(Connect|Check) Local model$/ }).click();
  await next("Continue");
  if (fresh) {
    await at("Describe the mod");
    await page.getByLabel("Mod name", { exact: true }).fill("Remote Acceptance Mod");
    await page.getByLabel("Mod description", { exact: true }).fill("An alternative-history mod focused on diplomacy in Europe.");
    await next(); await at("Project identity");
    await page.getByLabel("Project folder", { exact: true }).fill(project);
    await page.getByText("Launcher file location", { exact: true }).click();
    await page.getByLabel("Launcher descriptor path", { exact: true }).fill(join(mods, "remote_acceptance_mod.mod"));
    await approveSuggestions();
    assert.ok(!existsSync(project));
    assert.ok(!existsSync(join(mods, "remote_acceptance_mod.mod")));
    if (scenario === "new") {
      assert.equal(planner.requests.length, 2, "One malformed proposal should receive exactly one corrective request.");
      assert.equal(planner.requests[1].correction, true);
    }
    await record("confirmed-identity"); await next();
  } else {
    await at("Project identity"); await page.getByLabel("Project folder", { exact: true }).fill(project);
    await next(); await at("Scanning project"); await next(); await at("Confirm scan findings");
    await assertOriginals(project, original, { exact: true });
    await next("Review with Local model");
    await approveSuggestions();
    if (candidateDirectory) {
      const profile = planner.requests.at(-1)?.projectProfile;
      assert.ok(profile?.filesIndexed > 1000, "Kaiserreich must have a real project inventory, not just setup files.");
      assert.ok(profile.samplesRead > 10 && profile.samplesRead <= 256);
      assert.ok(profile.categories.includes("common/national_focus"));
      assert.ok(profile.categories.includes("events/root"));
      assert.ok(profile.namespaceCount > 0, "Approved provider input must contain sampled namespaces.");
      assert.ok(profile.languageCount > 0, "Approved provider input must contain actual localisation headers.");
      assert.ok(profile.documentationCount > 0);
      result.projectProfile = profile;
    }
    await assertOriginals(project, original, { exact: true });
    await record("confirmed-read-only-scan"); await next();
  }
  await commonSetup();
  if (fresh) assert.ok(!existsSync(project));
  else await assertOriginals(project, original, { exact: true });

  if (scenario === "recovery") {
    await next("Start installation");
    const interrupted = await waitUntil(async () => {
      const journal = (await journals()).findLast((item) => item.recovery?.project_apply_started && item.state !== "completed");
      return journal?.operations?.some((operation) => operation.status === "verified" && operation.action !== "skip") && journal;
    }, 15 * 60_000, "an observed live apply checkpoint");
    await stop();
    result.interruption = { transactionId: interrupted.transaction_id, observedState: interrupted.state };
    assert.ok(!existsSync(join(project, ".hoi4-mod-setup/install.lock.json")));
    await planner.close(); planner = undefined;
    await launch("webview-recovery");
    await page.getByRole("button", { name: "Manage an existing project", exact: true }).click();
    await page.getByLabel("Project folder", { exact: true }).fill(project);
    await at("Installation was interrupted"); await record("recovery-without-provider");
    await page.getByRole("button", { name: "Undo changes", exact: true }).click();
    await waitUntil(async () => (await journals()).some((journal) => journal.transaction_id === interrupted.transaction_id && journal.state === "rolled_back"), 10 * 60_000, "real rollback completion");
    result.preservation = await assertOriginals(project, original, { exact: true });
    await record("rollback-complete");
  } else {
    const { readiness } = await applyAndReady();
    const lock = JSON.parse(await readFile(join(project, ".hoi4-mod-setup/install.lock.json"), "utf8"));
    assert.equal(lock.ai_provider, "local");
    assert.equal(lock.source.revision, expectedSourceRevision);
    if (candidateDirectory) assert.equal(lock.source.mode, "latest");
    assert.equal(lock.codex_analysis.account_identity_persisted, false);
    assert.equal(lock.primary_coding_environment, "claude_code");
    assert.ok(lock.additional_coding_environments.includes("codex"));
    assert.ok(existsSync(join(project, "CLAUDE.md")) && existsSync(join(project, "AGENTS.md")));
    assert.ok(existsSync(join(project, ".codex/config.toml")) && existsSync(join(project, ".mcp.json")));
    result.installed = { files: lock.files.length, source: lock.source, primary: lock.primary_coding_environment, additional: lock.additional_coding_environments, image: lock.image_generation.primary_provider };
    if (fresh) {
      const descriptor = await readFile(join(project, "descriptor.mod"), "utf8");
      const launcher = await readFile(join(mods, "remote_acceptance_mod.mod"), "utf8");
      assert.match(descriptor, /picture="thumbnail\.png"/);
      assert.doesNotMatch(descriptor + launcher, /^(script_prefix|namespace)\s*=/m);
      const encodedPath = launcher.match(/^path=(".*")$/m)?.[1];
      assert.ok(encodedPath);
      assert.equal(resolve(JSON.parse(encodedPath)).toLowerCase(), resolve(project).toLowerCase());
      const png = await readFile(join(project, "thumbnail.png"));
      const dimensions = await page.evaluate(async (base64) => { const image = new Image(); image.src = `data:image/png;base64,${base64}`; await image.decode(); return [image.naturalWidth, image.naturalHeight]; }, png.toString("base64"));
      assert.deepEqual(dimensions, [600, 600]);
      assert.equal(readiness.checks.find((check) => check.id === "launcher.registration").status, "pass");
      assert.equal(lock.image_generation.primary_provider, "gptimage");
      assert.ok(lock.image_generation.installed_package_tree_sha256);
    } else {
      result.preservation = await assertOriginals(project, original, { gitignoreText });
      assert.equal(readiness.checks.find((check) => check.id === "launcher.registration").status, "not_selected");
      assert.equal(lock.image_generation.primary_provider, "fal_ai");
      assert.match(lock.image_generation.route_settings.fal_ai.model, /ideogram.*transparent/);
    }
    if (scenario === "clients") await clientPackagesFlow();
    const repairFile = scenario === "clients" ? undefined : lock.files.find((file) => !file.external && !file.preserved_local && file.path.startsWith(".agents/skills/") && file.path.endsWith("/SKILL.md"));
    if (scenario !== "clients") {
    assert.ok(repairFile);
    const repairPath = inside(project, join(project, repairFile.path));
    await rm(repairPath);
    await page.getByRole("button", { name: "Manage installation", exact: true }).click();
    await at("Update and repair");
    await page.getByRole("button", { name: /^Repair installation/ }).click();
    await waitForReview(true); await applyAndReady(true);
    assert.equal(await fileHash(repairPath), repairFile.installed_sha256);
    const repairedLock = JSON.parse(await readFile(join(project, ".hoi4-mod-setup/install.lock.json"), "utf8"));
    assert.deepEqual(repairedLock.source, lock.source);
    const repairedFile = repairedLock.files.find((file) => !file.external && file.path === repairFile.path);
    assert.equal(repairedFile.source_revision, repairFile.source_revision);
    assert.equal(repairedFile.source_sha256, repairFile.source_sha256);
    assert.equal(new Set(repairedLock.files.map((file) => `${file.external}:${file.path.toLowerCase()}`)).size, repairedLock.files.length);
    result.repaired = { files: repairedLock.files.length, instructionRecords: {} };
    for (const path of ["CLAUDE.md", "AGENTS.md"]) {
      const records = repairedLock.files.filter((file) => !file.external && file.path === path);
      assert.equal(records.length, 1);
      assert.equal(records[0].ownership, lock.files.find((file) => !file.external && file.path === path).ownership);
      assert.equal(await fileHash(inside(project, join(project, path))), records[0].installed_sha256);
      result.repaired.instructionRecords[path] = records.length;
    }
    if (scenario !== "new") await assertOriginals(project, original, { gitignoreText });
    if (candidateDirectory) {
      // Exercise a real pin and then explicitly leave it through the native UI.
      for (const target of scenario === "new" ? ["pinned_commit", "latest"] : ["latest"]) {
        await page.getByRole("button", { name: "Manage installation", exact: true }).click();
        await at("Update and repair");
        const version = page.getByRole("combobox", { name: "Target version", exact: true });
        await expect(version).toHaveValue("latest");
        await version.selectOption(target);
        if (target === "pinned_commit") await page.getByLabel("Commit", { exact: true }).fill(expectedSourceRevision);
        await record(`update-${target}-target`);
        await page.getByRole("button", { name: "Scan the project", exact: true }).click();
        await page.getByRole("button", { name: "Run Local model reanalysis", exact: true }).click({ timeout: 180_000 });
        await page.getByRole("button", { name: "Confirm Local model suggestions", exact: true }).waitFor({ timeout: 180_000 });
        await approveSuggestions();
        await page.getByRole("button", { name: /^Check for updates/ }).click();
        await waitForReview(true);
        const { journal: updateJournal, readiness: updateReadiness } = await applyAndReady(true);
        const updated = JSON.parse(await readFile(join(project, ".hoi4-mod-setup/install.lock.json"), "utf8"));
        assert.equal(updated.source.mode, target);
        assert.equal(updated.source.revision, expectedSourceRevision);
        assert.equal(updated.source.requested_ref ?? null, target === "pinned_commit" ? expectedSourceRevision : null);
        assert.equal(updated.source.release ?? null, null);
        assert.equal(updated.source.repository, "klimPaskov/Agentic-HOI4-Modding");
        assert.equal(updated.source.manifest_origin, "remote");
        assert.equal(updated.codex_analysis.source_revision, updated.source.revision);
        assert.equal(updated.codex_analysis.source_manifest_sha256, updated.source.manifest_sha256);
        const sourcePath = join(workRoot, `verified-${target}.json`);
        await downloadPublic(`https://raw.githubusercontent.com/klimPaskov/Agentic-HOI4-Modding/${updated.source.revision}/hoi4-mod-setup.manifest.json`, sourcePath, 4 * 1024 * 1024, updated.source.manifest_sha256);
        const manifest = JSON.parse(await readFile(sourcePath, "utf8"));
        const health = manifest.components.find((component) => component.id === "mcp.hoi4_agent_tools").validation.find((rule) => rule.parameters?.package_version);
        const mcp = updated.components.find((component) => component.id === "mcp.hoi4_agent_tools");
        assert.equal(mcp.version, health.parameters.package_version);
        assert.equal(mcp.state, "installed");
        assert.equal(mcp.validation, "pass");
        assert.equal(updateReadiness.checks.find((check) => check.id === "mcp.hoi4").status, "pass");
        assert.equal(new Set(updated.files.map((file) => `${file.external}:${file.path.toLowerCase()}`)).size, updated.files.length);
        if (scenario !== "new") await assertOriginals(project, original, { gitignoreText });
        result.updates ??= [];
        result.updates.push({ target, source: updated.source, mcpVersion: mcp.version, mcpHealth: "pass", transactionId: updateJournal.transaction_id });
        if (target === "pinned_commit") {
          const pinnedFile = updated.files.find((file) => !file.external && file.path === repairFile.path);
          await rm(repairPath);
          await page.getByRole("button", { name: "Manage installation", exact: true }).click();
          await at("Update and repair");
          await page.getByRole("button", { name: /^Repair installation/ }).click();
          await waitForReview(true); await applyAndReady(true);
          const pinnedRepair = JSON.parse(await readFile(join(project, ".hoi4-mod-setup/install.lock.json"), "utf8"));
          assert.deepEqual(pinnedRepair.source, updated.source);
          assert.equal(await fileHash(repairPath), pinnedFile.installed_sha256);
          result.pinnedRepair = { source: pinnedRepair.source, restoredFile: pinnedFile.path };
        }
      }
    }
    }
    await next("Finish"); await at("Congratulations, you are all set!"); await record("finished");
  }
  assert.ok(!result.pageErrors?.length, "The native renderer reported an uncaught error.");
  result.plannerRequests = planner?.requests ?? [];
  assert.equal(await fileHash(executable), result.executableSha256, "The app changed during version-specific testing.");
  result.success = true;
} catch (error) {
  failure = error;
  result.error = String(error.message).replace(/(?:sk-|gh[pousr]_)[A-Za-z0-9_-]+/g, "[REDACTED]").slice(0, 2000);
  if (page) { try { await record("failure"); } catch { /* Keep the primary failure. */ } }
} finally {
  try { await stop(); } catch (error) { failure ??= error; result.cleanupError = "Owned app shutdown failed."; }
  await planner?.close().catch(() => {});
  if (ownsRegistry) {
    try {
      const uninstaller = (await readdir(installRoot)).find((name) => /^uninstall.*\.exe$/i.test(name));
      if (uninstaller) {
        await exec(join(installRoot, uninstaller), ["/S"], { windowsHide: true, timeout: 120_000 });
        await waitUntil(() => !existsSync(executable), 60_000, "NSIS application removal");
        await waitUntil(async () => !await windowsRegistry.keyExists(uninstallKey), 60_000, "NSIS uninstall registration removal");
      }
      await cleanupWindowsInstallerRegistry({ workRoot, installRoot, tempRoot: process.env.RUNNER_TEMP });
    } catch (error) { failure ??= error; result.cleanupError = `Owned installer cleanup failed: ${String(error.message).slice(0, 1500)}`; }
  }
  result.success &&= !failure;
  Object.assign(result, sanitizeReport(result, [metadataToken]));
  await writeFile(join(output, "result.json"), JSON.stringify(result, null, 2) + "\n");
  inside(process.env.RUNNER_TEMP, workRoot);
  await rm(workRoot, { recursive: true, force: true });
}
if (failure) throw new Error(result.error ?? result.cleanupError ?? "Remote E2E failed.");
console.log(`${applicationKind} ${pins.app.version} ${scenario} E2E passed.`);
