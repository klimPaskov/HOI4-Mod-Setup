import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { cleanAppEnvironment, inside, inventoryDigest, requireHostedWindows, reviewedApplyLabel, sanitizeReport, setupPythonPath, startPlannerFixture, stopOwnedSession, verifiedCandidate } from "./released_e2e_support.mjs";

test("remote-only guard refuses a local or self-hosted session", () => {
  const previous = process.env.RUNNER_ENVIRONMENT;
  try {
    process.env.RUNNER_ENVIRONMENT = "self-hosted";
    assert.throws(requireHostedWindows, /disposable GitHub-hosted Windows/);
  } finally {
    if (previous === undefined) delete process.env.RUNNER_ENVIRONMENT;
    else process.env.RUNNER_ENVIRONMENT = previous;
  }
});

test("the application environment is allowlisted and profile isolated", () => {
  const previous = process.env.GITHUB_TOKEN;
  try {
    process.env.GITHUB_TOKEN = "do-not-forward";
    const profile = resolve("owned-profile");
    const env = cleanAppEnvironment(profile, 9222, join(profile, "webview"));
    assert.equal(env.GITHUB_TOKEN, undefined);
    assert.equal(env.APPDATA, join(profile, "config"));
    assert.equal(env.USERPROFILE, join(profile, "home"));
    assert.equal(env.HOI4_MOD_SETUP_SOURCE_REPOSITORY, undefined);
    assert.equal(env.PYTHONPATH, undefined);
    assert.match(env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, /--remote-debugging-address=127\.0\.0\.1/);
  } finally {
    if (previous === undefined) delete process.env.GITHUB_TOKEN;
    else process.env.GITHUB_TOKEN = previous;
  }
});

test("owned-path checks reject root and sibling escapes", () => {
  const root = resolve("owned");
  assert.throws(() => inside(root, root));
  assert.throws(() => inside(root, resolve("owned-other", "file")));
  assert.throws(() => inside(root, join(root, "..", "file")));
  assert.equal(inside(root, join(root, "file")), join(root, "file"));
});

test("apply controls follow the actual reviewed screen for installation and maintenance", () => {
  for (const maintenance of [false, true]) {
    assert.equal(reviewedApplyLabel("Review changes", maintenance), "Start installation");
    assert.equal(reviewedApplyLabel("Start a mod project", maintenance), undefined);
    assert.equal(reviewedApplyLabel("Project ready", maintenance), undefined);
  }
  assert.equal(reviewedApplyLabel("Resolve file conflicts", true), "Apply");
  assert.equal(reviewedApplyLabel("Resolve file conflicts", false), undefined);
  assert.equal(reviewedApplyLabel("Update and repair", true), "Apply reviewed plan");
  assert.equal(reviewedApplyLabel("Update and repair", false), undefined);
});

test("the extraction runtime requires absolute setup-python identity", async () => {
  await assert.rejects(setupPythonPath("python", resolve("cache")), /runtime identity/);
  await assert.rejects(setupPythonPath(resolve("python"), undefined), /runtime identity/);
});

test("report metadata removes known and credential-shaped values", () => {
  const report = { error: "private-value Bearer abc.def sk-secret123", pageErrors: ["ghp_123456 AIza12345 github_pat_12345"] };
  const text = JSON.stringify(sanitizeReport(report, ["private-value"]));
  for (const secret of ["private-value", "abc.def", "sk-secret123", "ghp_123456", "AIza12345", "github_pat_12345"]) assert.ok(!text.includes(secret));
});

test("owned policy cleanup runs even when process or browser shutdown fails", async () => {
  for (const failing of [0, 1, 2]) {
    const called = [];
    const operations = [0, 1, 2].map((index) => async () => { called.push(index); if (index === failing) throw new Error("injected cleanup failure"); });
    await assert.rejects(stopOwnedSession(...operations), /Owned session cleanup failed/);
    assert.deepEqual(called, [0, 1, 2]);
  }
});

test("candidate artifacts require exact build identity and complete file hashes", async () => {
  const root = await mkdtemp(join(tmpdir(), "hoi4-candidate-evidence-"));
  try {
    const revision = "a".repeat(40);
    const metadata = JSON.stringify({ product: "HOI4 Mod Setup", version: "1.2.3", sourceRevision: revision, frontendOnly: false, platform: "Windows", architecture: "X64", bundle: "nsis", artifact_manifest: "ARTIFACTS.sha256" });
    const installerPath = "packages/nsis/example.exe";
    const installer = Buffer.from("synthetic installer bytes; never executed");
    const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
    await mkdir(join(root, "packages/nsis"), { recursive: true });
    await writeFile(join(root, "BUILD_METADATA.json"), metadata);
    await writeFile(join(root, installerPath), installer);
    const entries = [{ path: "BUILD_METADATA.json", sha256: digest(metadata) }, { path: installerPath, sha256: digest(installer) }];
    const manifest = join(root, "ARTIFACTS.sha256");
    await writeFile(manifest, JSON.stringify(entries));
    assert.equal((await verifiedCandidate(root, "1.2.3", revision)).sha256, digest(installer));
    await assert.rejects(verifiedCandidate(root, "1.2.4", revision));
    await assert.rejects(verifiedCandidate(root, "1.2.3", "b".repeat(40)));
    await writeFile(join(root, installerPath), "changed");
    await assert.rejects(verifiedCandidate(root, "1.2.3", revision));
    await writeFile(join(root, installerPath), installer);
    await writeFile(manifest, JSON.stringify([...entries, entries[1]]));
    await assert.rejects(verifiedCandidate(root, "1.2.3", revision));
    await writeFile(manifest, JSON.stringify([{ path: "../outside", sha256: "b".repeat(64) }]));
    await assert.rejects(verifiedCandidate(root, "1.2.3", revision));
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("inventory identity is stable regardless of enumeration order", () => {
  const one = new Map([["b", { sha256: "two", size: 2 }], ["a", { sha256: "one", size: 1 }]]);
  assert.equal(inventoryDigest(one), inventoryDigest(new Map([...one].reverse())));
});

test("loopback planner binds approved input and returns one correction without credentials", async () => {
  const planner = await startPlannerFixture({ malformedFirst: true });
  try {
    const input = { mode: "new_project_identity", constraints: { requested_mod_name: "Remote Acceptance Mod" }, evidence: [] };
    const inputJson = JSON.stringify(input);
    const digest = createHash("sha256").update(inputJson).digest("hex");
    const prompt = `Instructions\ninput_sha256=${digest}\ninput=${inputJson}`;
    async function request(content) {
      const response = await fetch(planner.endpoint, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ model: "e2e-planner", messages: [{ role: "user", content }] }) });
      assert.equal(response.status, 200);
      return JSON.parse((await response.json()).choices[0].message.content);
    }
    assert.deepEqual(await request(prompt), {});
    const analysis = await request(`${prompt}\n\nThe previous response was rejected by deterministic validation.`);
    assert.equal(analysis.input_sha256, digest);
    assert.equal(analysis.proposals.length, 10);
    assert.equal(analysis.proposals.find(({ key }) => key === "display_name").value, "Remote Acceptance Mod");
    assert.ok(analysis.proposals.every(({ evidence_refs }) => evidence_refs.length === 0));
    assert.equal(planner.requests[1].correction, true);
    const denied = await fetch(planner.endpoint, { method: "POST", headers: { Authorization: "Bearer do-not-forward" }, body: "{}" });
    assert.equal(denied.status, 400);
    const unbound = await fetch(planner.endpoint, { method: "POST", body: JSON.stringify({ model: "e2e-planner", messages: [{ role: "user", content: prompt.replace(digest, "a".repeat(64)) }] }) });
    assert.equal(unbound.status, 400);
  } finally { await planner.close(); }
});
