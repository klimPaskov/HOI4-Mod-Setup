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
assert.ok(["new", "kaiserreich", "recovery"].includes(scenario), "Unknown remote scenario.");
const workRoot = await mkdtemp(join(process.env.RUNNER_TEMP, "hoi4-mod-setup-installer-e2e-"));
const installRoot = join(workRoot, "installed");
const profile = join(workRoot, "profile");
const mods = join(workRoot, "mods");
const output = resolve(repository, "test-results", `released-${scenario}`);
for (const path of [output, mods, ...["home", "config", "data", "temp"].map((part) => join(profile, part))]) await mkdir(path, { recursive: true });
const journalRoot = join(profile, "data", "HOI4 Mod Setup", "transactions");
const project = join(mods, scenario === "new" ? "remote_acceptance_mod" : "kaiserreich");
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
  if (scenario === "new") {
    await expect(page.getByRole("radio", { name: /^ChatGPT subscription \(GPTImage\)/ })).toBeChecked({ timeout: 180_000 });
  } else {
    await page.getByRole("radio", { name: /^fal\.ai / }).check();
    await page.getByRole("combobox", { name: "Model", exact: true }).selectOption({ label: "Ideogram V3 Transparent" });
  }
  await record("image-provider"); await next();
  await at("Choose what to install");
  await expect(page.getByRole("button", { name: /Image provider MCP adapter/ })).toHaveAttribute("aria-pressed", scenario === "new" ? "false" : "true");
  await expect(page.getByRole("button", { name: /GPTImage MCP source/ })).toHaveAttribute("aria-pressed", scenario === "new" ? "true" : "false");
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
async function waitForReview(maintenance) {
  await waitUntil(async () => {
    const title = await heading().innerText();
    if (title === "Resolve file conflicts") {
      const done = page.getByRole("button", { name: maintenance ? "Apply" : "Review changes", exact: true });
      if (await done.isEnabled()) {
        if (!maintenance) await done.click();
        return maintenance;
      }
      const message = await page.locator(".conflict-actions p").innerText();
      await page.getByRole("button", { name: "Keep current", exact: true }).click();
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

  if (scenario !== "new") {
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

  planner = await startPlannerFixture({ malformedFirst: scenario === "new", projectName: scenario === "new" ? "Remote Acceptance Mod" : "Kaiserreich" });
  await launch("webview-initial"); await record("welcome");
  await page.getByRole("button", { name: scenario === "new" ? /^Create new mod/ : /^Import existing mod/ }).click();
  await page.getByRole("combobox", { name: "AI provider", exact: true }).selectOption("local");
  await page.getByLabel("Model", { exact: true }).fill("e2e-planner");
  await page.getByLabel("Local model address", { exact: true }).fill(planner.endpoint);
  await page.getByRole("button", { name: /^(Connect|Check) Local model$/ }).click();
  await next("Continue");
  if (scenario === "new") {
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
    assert.equal(planner.requests.length, 2, "One malformed proposal should receive exactly one corrective request.");
    assert.equal(planner.requests[1].correction, true);
    await record("confirmed-identity"); await next();
  } else {
    await at("Project identity"); await page.getByLabel("Project folder", { exact: true }).fill(project);
    await next(); await at("Scanning project"); await next(); await at("Confirm scan findings");
    await assertOriginals(project, original, { exact: true });
    await next("Review with Local model");
    await approveSuggestions();
    await assertOriginals(project, original, { exact: true });
    await record("confirmed-read-only-scan"); await next();
  }
  await commonSetup();
  if (scenario === "new") assert.ok(!existsSync(project));
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
    if (scenario === "new") {
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
    const repairFile = lock.files.find((file) => !file.external && !file.preserved_local && file.path.startsWith(".agents/skills/") && file.path.endsWith("/SKILL.md"));
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
