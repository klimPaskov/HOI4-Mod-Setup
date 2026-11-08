import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { appendFile, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { withLinuxDebInstallation } from "./linux_installer_registry.mjs";
import { actionTimings, createPhaseClock, fileHash, githubJson, inside, requireDisposableLinux, reviewedApplyLabel, sanitizeReport, stageTimings, startPlannerFixture, timingSummary, verifiedCandidate } from "./released_e2e_support.mjs";
import { WebDriverSession } from "./linux_webdriver.mjs";

// Fresh install on a disposable Linux machine: install the exact CI-built Debian package, drive the production
// WebKitGTK UI through WebKitWebDriver (real IPC, real source download, real transactions and readiness), and
// record how long each phase takes. No game or launcher is started. Planning uses the same loopback model fixture
// as the Windows acceptance, so provider sign-in is not covered.
requireDisposableLinux();
const metadataToken = process.env.GITHUB_TOKEN;
for (const name of Object.keys(process.env)) {
  if (/TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTHORIZATION|API.?KEY|^ACTIONS_|^ANTHROPIC_|^OPENAI_|^MESHY_|^CODEX_HOME$/i.test(name)) delete process.env[name];
}
const repository = resolve(import.meta.dirname, "..");
const candidateDirectory = process.env.HOI4_E2E_CANDIDATE_DIR;
assert.ok(candidateDirectory, "The Linux acceptance installs a same-workflow candidate package (set HOI4_E2E_CANDIDATE_DIR).");
inside(repository, candidateDirectory);
const version = JSON.parse(await readFile(join(repository, "package.json"), "utf8")).version;
const revision = process.env.HOI4_E2E_CANDIDATE_REVISION ?? process.env.GITHUB_SHA;
const scenario = "new";
const workRoot = await mkdtemp(join(process.env.RUNNER_TEMP ?? "/tmp", "hoi4-mod-setup-linux-e2e-"));
const profile = join(workRoot, "profile");
const mods = join(workRoot, "mods");
const output = resolve(repository, "test-results", `linux-${scenario}`);
for (const path of [output, mods, ...["home", "config", "data", "cache", "state"].map((part) => join(profile, part))]) await mkdir(path, { recursive: true });
const project = join(mods, "linux_acceptance_mod");
const launcherPath = join(mods, "linux_acceptance_mod.mod");
const result = { scenario, applicationKind: "candidate", applicationVersion: version, runner: "Linux",
  ai: "deterministic loopback HTTP fixture; real provider authentication and semantic quality are not tested",
  nativeIpc: "unmodified candidate .deb installed with apt; no IPC mocks or application test hooks", stages: [], screenshots: [], phases: [], success: false };
const clock = createPhaseClock();
result.phases = clock.phases;
let session;
let planner;
let failure;

const heading = () => session.heading();
async function record(name) {
  result.stages.push({ name, at: new Date().toISOString(), heading: await heading() });
  const file = `${String(result.stages.length).padStart(2, "0")}-${name.replace(/[^a-z0-9-]/gi, "-")}.png`;
  if (await session.screenshot(join(output, file))) result.screenshots.push(file);
  else (result.missingScreenshots ??= []).push(file);
  console.log(`E2E linux ${scenario}: ${name}`);
}
async function next(name = "Next") { await session.click("button", name, { timeout: 180_000 }); }
async function at(title) { await session.waitUntil(async () => (await heading()) === title, 180_000, `screen "${title}"`); }

// The app's data folder follows XDG_DATA_HOME, so find the journal folder instead of hard-coding the product name.
async function journalRoot() {
  async function search(directory, depth) {
    if (depth > 3 || !existsSync(directory)) return undefined;
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const path = join(directory, entry.name);
      if (entry.name === "transactions") return path;
      const deeper = await search(path, depth + 1);
      if (deeper) return deeper;
    }
    return undefined;
  }
  return search(join(profile, "data"), 0);
}
async function journals() {
  const root = await journalRoot();
  if (!root) return [];
  const values = [];
  for (const name of await readdir(root)) {
    if (!/^[a-f0-9-]{36}$/.test(name)) continue;
    try { values.push(JSON.parse(await readFile(join(root, name, "journal.json"), "utf8"))); } catch { /* A checkpoint may be replacing the JSON. */ }
  }
  return values.sort((a, b) => a.created_at.localeCompare(b.created_at));
}
async function completedTransaction(previousIds = []) {
  return session.waitUntil(async () => {
    const errors = await session.alerts();
    if (errors.some((text) => /could not|unavailable|failed|error|recovering the previous/i.test(text))) throw new Error(errors.join(" ").slice(0, 1500));
    const journal = (await journals()).findLast((item) => !previousIds.includes(item.transaction_id) && item.transaction_kind === "installation");
    if (journal?.state === "interrupted") throw new Error(`Installation interrupted: ${journal.error?.message ?? journal.last_checkpoint}`);
    return journal?.state === "completed" && journal;
  }, 20 * 60_000, "real installation completion");
}
async function approveSuggestions() {
  await session.click("button", "Confirm Local model suggestions");
  await session.textVisible("Local model suggestions confirmed");
}
async function waitForReview(maintenance) {
  await clock.measure(maintenance ? "maintenance: resolve, download, verify and review" : "resolve, download, verify and review", () => session.waitUntil(async () => {
    const title = await heading();
    assert.notEqual(title, "Resolve file conflicts", "A fresh or repaired install of an untouched project must not report file conflicts.");
    const label = reviewedApplyLabel(title, maintenance);
    if (label && await session.isEnabled("button", label)) return true;
    const errors = await session.alerts();
    if (errors.some((text) => /could not|unavailable|failed|error/i.test(text))) throw new Error(errors.join(" ").slice(0, 1500));
    return false;
  }, 15 * 60_000, "source preparation and reviewed plan"));
  await record(maintenance ? "maintenance-review" : "dry-run");
}
async function applyAndReady(maintenance = false) {
  const previousIds = (await journals()).map((item) => item.transaction_id);
  const label = reviewedApplyLabel(await heading(), maintenance);
  assert.ok(label, "Installation requires a recognized reviewed-plan screen.");
  await next(label);
  const journal = await clock.measure(maintenance ? "maintenance: apply (journal completed)" : "install: apply (journal completed)", () => completedTransaction(previousIds));
  assert.equal(journal.stages.length, 12);
  assert.ok(journal.stages.every((stage) => stage.status === "complete"));
  await clock.measure(maintenance ? "maintenance: ready screen" : "install: ready screen", async () => {
    if (!maintenance) { await session.waitUntil(() => session.isEnabled("button", "Continue"), 60_000, "Continue enabled"); await next("Continue"); }
    await at("Project ready");
    await session.waitUntil(async () => /\bis ready\b/.test(await session.text()), 90_000, "ready heading");
  });
  const readiness = JSON.parse(await readFile(join(await journalRoot(), journal.transaction_id, "readiness-report.json"), "utf8"));
  result.transactions ??= [];
  result.transactions.push({ id: journal.transaction_id, state: journal.state, stages: journal.stages.map(({ id, status }) => ({ id, status })), stageTimings: stageTimings(journal), actionTimings: actionTimings(journal), coreReady: readiness.core_ready });
  result.readinessChecks = readiness.checks.map((check) => ({ id: check.id, status: check.status, blocking: check.blocking }));
  assert.equal(readiness.core_ready, true, `core readiness: ${JSON.stringify(result.readinessChecks.filter((check) => check.status !== "pass"))}`);
  assert.ok(!readiness.checks.some((check) => check.blocking && check.status === "block"));
  await record(maintenance ? "repaired-ready" : "installed-ready");
  return { journal, readiness };
}
// The kit's default branch can move during a run, so a "latest" install must land on a commit that was its head at some
// time since the run started: the start commit or one of its descendants.
async function assertLatestSourceRevision(actual, startRevision, branch) {
  if (!metadataToken) return;
  const repo = "/repos/klimPaskov/Agentic-HOI4-Modding";
  const seen = new Set([startRevision]);
  let sha = (await githubJson(`${repo}/commits/${encodeURIComponent(branch)}`, metadataToken)).sha;
  for (let hop = 0; hop < 60 && sha && !seen.has(sha); hop += 1) {
    seen.add(sha);
    sha = (await githubJson(`${repo}/commits/${sha}`, metadataToken)).parents?.[0]?.sha;
  }
  assert.ok(seen.has(actual), `${actual} is not the head of the kit's default branch at any point since the run started at ${startRevision}.`);
}
// Best-effort diagnostics, taken while the app and driver are still alive.
async function diagnose(error) {
  result.error = String(error.message).replace(/(?:sk-|gh[pousr]_)[A-Za-z0-9_-]+/g, "[REDACTED]").slice(0, 2000);
  if (!session) return;
    // Keep the primary failure: each diagnostic is best effort and reports why it is missing.
    try { await record("failure"); } catch (cause) { result.failureScreenshot = `unavailable: ${String(cause.message).slice(0, 200)}`; }
    result.driverReport = session.driverReport();
    try { result.failureText = (await session.text()).slice(0, 1500); } catch (cause) { result.failureText = `unavailable: ${String(cause.message).slice(0, 200)}`; }
    try { result.failureControls = await session.controls(); } catch (cause) { result.failureControls = `unavailable: ${String(cause.message).slice(0, 200)}`; }
}
function pngSize(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "thumbnail.png is not a PNG");
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
}
function appEnvironment() {
  const keep = ["PATH", "DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR", "LANG", "LC_ALL", "WEBKIT_DISABLE_DMABUF_RENDERER", "WEBKIT_DISABLE_COMPOSITING_MODE", "GDK_BACKEND", "LIBGL_ALWAYS_SOFTWARE", "NO_AT_BRIDGE", "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS"];
  const env = Object.fromEntries(keep.filter((name) => process.env[name] !== undefined).map((name) => [name, process.env[name]]));
  // The system Node (Debian's /usr/bin/node) is the only runtime the app trusts, so it must come first on PATH.
  env.PATH = `/usr/bin:${env.PATH ?? ""}`;
  return { ...env, HOME: join(profile, "home"), XDG_CONFIG_HOME: join(profile, "config"), XDG_DATA_HOME: join(profile, "data"), XDG_CACHE_HOME: join(profile, "cache"),
    XDG_STATE_HOME: join(profile, "state"), TMPDIR: join(profile, "home"), HOI4_MOD_SETUP_E2E: "1",
    // The Tauri runtime reads this to let the distro WebKitWebDriver automate its WebView; the binary is unmodified.
    TAURI_WEBVIEW_AUTOMATION: "true" };
}

try {
  const sourceRepository = await githubJson("/repos/klimPaskov/Agentic-HOI4-Modding", metadataToken);
  const startRevision = (await githubJson(`/repos/klimPaskov/Agentic-HOI4-Modding/commits/${encodeURIComponent(sourceRepository.default_branch)}`, metadataToken)).sha;
  result.latestSourceAtStart = startRevision;
  const candidate = await verifiedCandidate(candidateDirectory, version, revision, { platform: "Linux", bundle: "appimage,deb", installer: /^packages\/deb\/[^/]+\.deb$/i });
  result.installer = { origin: "same-workflow native build", sha256: candidate.sha256, size: candidate.size, sourceRevision: candidate.sourceRevision };
  result.signing = "Candidate artifact; not signed release evidence.";

  planner = await startPlannerFixture({ malformedFirst: true, projectName: "Linux Acceptance Mod" });
  // Install the way docs/LINUX.md tells users to (sudo apt install ./file.deb), so declared dependencies resolve.
  await withLinuxDebInstallation(candidate.installer, async (executable) => {
    try {
    result.executableSha256 = await fileHash(executable);
    session = await clock.measure("launch to first screen", async () => {
      const started = await WebDriverSession.start({ binary: executable, env: appEnvironment(), cwd: workRoot });
      await started.waitUntil(async () => (await started.heading()) === "Start a mod project", 90_000, "first screen");
      return started;
    });
    await record("welcome");
    await session.click("button", /^Create new mod/);
    await session.selectOption("AI provider", "local");
    await session.fill("Model", "e2e-planner");
    await session.fill("Local model address", planner.endpoint);
    await session.click("button", /^(Connect|Check) Local model$/);
    await next("Continue");
    await at("Describe the mod");
    await session.fill("Mod name", "Linux Acceptance Mod");
    await session.fill("Mod description", "An alternative-history mod focused on diplomacy in Europe.");
    await clock.measure("planning with the loopback model", async () => { await next(); await at("Project identity"); });
    await session.fill("Project folder", project);
    await session.clickText("Launcher file location");
    await session.fill("Launcher descriptor path", launcherPath);
    await approveSuggestions();
    assert.ok(!existsSync(project) && !existsSync(launcherPath), "Nothing may be written before the reviewed install.");
    assert.equal(planner.requests.length, 2, "One malformed proposal should receive exactly one corrective request.");
    assert.equal(planner.requests[1].correction, true);
    await record("confirmed-identity"); await next();

    await at("Coding environments");
    await session.check("radio", /^Claude Code/);
    await session.check("checkbox", /^Codex /);
    await record("coding-environments"); await next();
    await at("Image generation");
    assert.equal(await session.isChecked("checkbox", /Install hoi4-imagegen/, { timeout: 180_000 }), true);
    await record("image-provider"); await next();
    await at("Choose what to install");
    assert.equal(await session.attribute("button", /Image provider MCP adapter/, "aria-pressed"), "false");
    assert.equal(await session.attribute("button", /hoi4-imagegen image server/, "aria-pressed"), "true");
    await record("selected-components"); await next();
    await at("Optional workflows");
    await session.element("switch", /^HOI4 Live Tools/);
    await next();
    await session.waitUntil(async () => /^(Live testing|MCP and credentials|Choose Git setup)$/.test(await heading() ?? ""), 60_000, "post-workflow screen");
    if (await heading() === "Live testing") { await session.check("radio", /^None /); await next(); }
    await session.waitUntil(async () => /^(MCP and credentials|Choose Git setup)$/.test(await heading() ?? ""), 60_000, "integrations or Git screen");
    if (await heading() === "MCP and credentials") { await record("integrations"); await next(); }
    await at("Choose Git setup");
    await session.check("radio", /^Skip Git setup/);
    await next();
    assert.ok(!existsSync(project));
    await waitForReview(false);
    const { readiness } = await applyAndReady();

    const lock = JSON.parse(await readFile(join(project, ".hoi4-mod-setup/install.lock.json"), "utf8"));
    assert.equal(lock.ai_provider, "local");
    assert.equal(lock.source.mode, "latest");
    await assertLatestSourceRevision(lock.source.revision, startRevision, sourceRepository.default_branch);
    assert.equal(lock.codex_analysis.account_identity_persisted, false);
    assert.equal(lock.primary_coding_environment, "claude_code");
    assert.ok(lock.additional_coding_environments.includes("codex"));
    for (const path of ["CLAUDE.md", "AGENTS.md", ".codex/config.toml", ".mcp.json"]) assert.ok(existsSync(join(project, path)), `missing ${path}`);
    result.installed = { files: lock.files.length, source: lock.source, primary: lock.primary_coding_environment, additional: lock.additional_coding_environments, image: lock.image_generation.primary_provider };
    const descriptor = await readFile(join(project, "descriptor.mod"), "utf8");
    const launcher = await readFile(launcherPath, "utf8");
    assert.match(descriptor, /picture="thumbnail\.png"/);
    assert.doesNotMatch(descriptor + launcher, /^(script_prefix|namespace)\s*=/m);
    const encodedPath = launcher.match(/^path=(".*")$/m)?.[1];
    assert.ok(encodedPath);
    assert.equal(resolve(JSON.parse(encodedPath)), resolve(project));
    assert.deepEqual(pngSize(await readFile(join(project, "thumbnail.png"))), [600, 600]);
    assert.equal(readiness.checks.find((check) => check.id === "launcher.registration").status, "pass");
    assert.equal(lock.image_generation.primary_provider, "hoi4_imagegen");
    assert.ok(lock.image_generation.installed_release_tree_sha256);

    // Repair: remove one owned skill through the filesystem, then restore it through the UI at its original hash.
    const repairFile = lock.files.find((file) => !file.external && !file.preserved_local && file.path.startsWith(".agents/skills/") && file.path.endsWith("/SKILL.md"));
    assert.ok(repairFile);
    const repairPath = inside(project, join(project, repairFile.path));
    await rm(repairPath);
    await session.click("button", "Manage installation");
    await at("Update and repair");
    await session.click("button", /^Repair installation/);
    await waitForReview(true);
    await applyAndReady(true);
    assert.equal(await fileHash(repairPath), repairFile.installed_sha256);
    await next("Finish"); await at("Congratulations, you are all set!"); await record("finished");
    assert.equal(await fileHash(executable), result.executableSha256, "The app changed during the test.");
    } catch (error) {
      await diagnose(error);
      throw error;
    } finally {
      // The app must have exited before dpkg purges the package.
      await session?.close().catch(() => {});
    }
  }, { useApt: true });
  result.plannerRequests = planner.requests;
  result.success = true;
} catch (error) {
  failure = error;
  result.error = String(error.message).replace(/(?:sk-|gh[pousr]_)[A-Za-z0-9_-]+/g, "[REDACTED]").slice(0, 2000);
} finally {
  await session?.close().catch(() => {});
  await planner?.close().catch(() => {});
  result.success &&= !failure;
  Object.assign(result, sanitizeReport(result, [metadataToken]));
  await writeFile(join(output, "result.json"), JSON.stringify(result, null, 2) + "\n");
  if (process.env.GITHUB_STEP_SUMMARY) await appendFile(process.env.GITHUB_STEP_SUMMARY, timingSummary(result)).catch(() => {});
  await rm(workRoot, { recursive: true, force: true });
}
if (failure) throw new Error(result.error ?? "Linux E2E failed.");
console.log(`candidate ${version} linux ${scenario} E2E passed.`);
