import assert from "node:assert/strict";
import { join, resolve } from "node:path";
import test from "node:test";
import { cleanAppEnvironment, inside, inventoryDigest, requireHostedWindows, startPlannerFixture } from "./released_e2e_support.mjs";

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

test("inventory identity is stable regardless of enumeration order", () => {
  const one = new Map([["b", { sha256: "two", size: 2 }], ["a", { sha256: "one", size: 1 }]]);
  assert.equal(inventoryDigest(one), inventoryDigest(new Map([...one].reverse())));
});

test("loopback planner binds approved input and returns one correction without credentials", async () => {
  const planner = await startPlannerFixture({ malformedFirst: true });
  try {
    const input = { mode: "new_project_identity", constraints: { requested_mod_name: "Remote Acceptance Mod" }, evidence: [] };
    const prompt = `Instructions\ninput_sha256=${"a".repeat(64)}\ninput=${JSON.stringify(input)}`;
    async function request(content) {
      const response = await fetch(planner.endpoint, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ model: "e2e-planner", messages: [{ role: "user", content }] }) });
      assert.equal(response.status, 200);
      return JSON.parse((await response.json()).choices[0].message.content);
    }
    assert.deepEqual(await request(prompt), {});
    const analysis = await request(`${prompt}\n\nThe previous response was rejected by deterministic validation.`);
    assert.equal(analysis.input_sha256, "a".repeat(64));
    assert.equal(analysis.proposals.length, 10);
    assert.equal(analysis.proposals.find(({ key }) => key === "display_name").value, "Remote Acceptance Mod");
    assert.ok(analysis.proposals.every(({ evidence_refs }) => evidence_refs.length === 0));
    assert.equal(planner.requests[1].correction, true);
    const denied = await fetch(planner.endpoint, { method: "POST", headers: { Authorization: "Bearer do-not-forward" }, body: "{}" });
    assert.equal(denied.status, 400);
  } finally { await planner.close(); }
});
