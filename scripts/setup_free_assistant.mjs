#!/usr/bin/env node
// One command that turns on the free setup assistant.
//
//   node scripts/setup_free_assistant.mjs          paste two keys, done
//   node scripts/setup_free_assistant.mjs --check  only check this computer is ready (no keys, no changes)
//
// It validates an OpenRouter key and a Cloudflare API token, finds the Cloudflare
// account, stores the three secrets in the repository's `free-assistant`
// environment through the GitHub CLI (on stdin, never on a command line), runs the
// "Deploy free assistant relay" workflow, tests the deployed relay with one real
// request, and opens a pull request that sets `endpoint` in src/free-assistant.json.
// Keys are never printed, written to disk, or put in the repository.
import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CONFIG_PATH = resolve(root, "src/free-assistant.json");
const WORKFLOW = "relay.yml";
const ENVIRONMENT = "free-assistant";
const WORKER_NAME = "hoi4-mod-setup-free";
const BRANCH = "chore/enable-free-assistant";
const CF = "https://api.cloudflare.com/client/v4";

// ---- pure helpers (tested in setup_free_assistant.node-test.mjs) -------------------

export function extractWorkerUrl(text) {
  const matches = String(text).match(/https:\/\/[a-z0-9-]+\.[a-z0-9-]+\.workers\.dev/gi);
  return matches ? matches[matches.length - 1].toLowerCase() : null;
}

export function workerUrlFor(subdomain) {
  return `https://${WORKER_NAME}.${subdomain}.workers.dev`;
}

export function chatEndpoint(workerUrl) {
  return `${workerUrl.replace(/\/+$/, "")}/v1/chat/completions`;
}

export function withEndpoint(config, endpoint) {
  return { ...config, endpoint };
}

export function suggestSubdomain(accountName, random = Math.random) {
  const base = String(accountName ?? "")
    .toLowerCase()
    .replace(/@.*$/, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 20);
  const suffix = Math.floor(random() * 9000 + 1000);
  return `${base || "hoi4-mod-setup"}-${suffix}`;
}

export function describeOpenRouterKey(body) {
  const data = body?.data;
  if (!data) return { ok: false, note: "OpenRouter did not return key details." };
  // is_free_tier is true until the account has bought credits.
  return {
    ok: true,
    freeTier: data.is_free_tier === true,
    note: data.is_free_tier === true
      ? "This account has not bought credits, so OpenRouter allows only 50 free-model requests a day for all users together. Adding $10 of credits raises that to 1,000 a day: https://openrouter.ai/settings/credits"
      : "Credits found: the higher free-model daily limit applies.",
  };
}

// ---- small tools -------------------------------------------------------------------

function run(command, args, { input, cwd = root } = {}) {
  return new Promise((resolvePromise) => {
    let child;
    try {
      child = spawn(command, args, { cwd, stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
    } catch (error) {
      resolvePromise({ code: 127, stdout: "", stderr: String(error.message) });
      return;
    }
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => resolvePromise({ code: 127, stdout, stderr: String(error.message) }));
    child.on("close", (code) => resolvePromise({ code: code ?? 1, stdout, stderr }));
    if (input !== undefined) child.stdin.end(input);
    else child.stdin.end();
  });
}

const gh = (args, options) => run("gh", args, options);
const git = (args, options) => run("git", args, options);

function readHidden(prompt) {
  return new Promise((resolvePromise, reject) => {
    process.stdout.write(prompt);
    const stdin = process.stdin;
    if (!stdin.isTTY) {
      // Piped input (for example a password manager): read one line.
      let data = "";
      stdin.setEncoding("utf8");
      stdin.on("data", (chunk) => { data += chunk; if (data.includes("\n")) { stdin.pause(); process.stdout.write("\n"); resolvePromise(data.split(/\r?\n/)[0].trim()); } });
      stdin.on("end", () => resolvePromise(data.trim()));
      return;
    }
    let value = "";
    stdin.setRawMode(true);
    stdin.resume();
    stdin.setEncoding("utf8");
    const onData = (chunk) => {
      for (const char of chunk) {
        if (char === "\r" || char === "\n") {
          stdin.setRawMode(false);
          stdin.pause();
          stdin.off("data", onData);
          process.stdout.write("\n");
          resolvePromise(value.trim());
          return;
        }
        if (char === "\u0003") { stdin.setRawMode(false); reject(new Error("Cancelled.")); return; }
        if (char === "\u007f" || char === "\b") { value = value.slice(0, -1); continue; }
        value += char;
      }
    };
    stdin.on("data", onData);
  });
}

async function readLine(prompt) {
  process.stdout.write(prompt);
  return new Promise((resolvePromise) => {
    process.stdin.resume();
    process.stdin.setEncoding("utf8");
    process.stdin.once("data", (chunk) => { process.stdin.pause(); resolvePromise(String(chunk).trim()); });
  });
}

async function cf(path, token, { method = "GET", body } = {}) {
  const response = await fetch(`${CF}${path}`, {
    method,
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  let json = null;
  try { json = await response.json(); } catch { /* not JSON */ }
  return { status: response.status, json };
}

const step = (n, total, text) => console.log(`\n[${n}/${total}] ${text}`);
const ok = (text) => console.log(`  ok  ${text}`);
class Stop extends Error {}
const stop = (text) => { throw new Stop(text); };

// ---- the flow ----------------------------------------------------------------------

async function preflight() {
  const version = await gh(["--version"]);
  if (version.code !== 0) stop("The GitHub CLI (gh) is not installed. Install it from https://cli.github.com and run `gh auth login`, then run this again.");
  const auth = await gh(["auth", "status"]);
  if (auth.code !== 0) stop("gh is not signed in. Run `gh auth login`, then run this again.");
  const repo = await gh(["repo", "view", "--json", "nameWithOwner,viewerPermission", "-q", ".nameWithOwner + \" \" + .viewerPermission"]);
  if (repo.code !== 0) stop(`Run this from a clone of the repository.\n${repo.stderr.trim()}`);
  const [nameWithOwner, permission] = repo.stdout.trim().split(" ");
  if (permission !== "ADMIN") stop(`You need admin rights on ${nameWithOwner} (you have ${permission}) to create the environment and secrets.`);
  const status = await git(["status", "--porcelain"]);
  if (status.stdout.trim()) stop("You have uncommitted changes. Commit or stash them first, then run this again (the script opens a pull request from a clean copy of main).");
  const branch = (await git(["branch", "--show-current"])).stdout.trim();
  ok(`GitHub CLI signed in, admin on ${nameWithOwner}, working tree clean`);
  return { repo: nameWithOwner, branch };
}

async function collectKeys() {
  console.log("\nPaste your keys (nothing is shown or saved on this computer).");
  console.log("  OpenRouter key:   https://openrouter.ai/settings/keys   (Create Key)");
  console.log("  Cloudflare token: https://dash.cloudflare.com/profile/api-tokens   (Create Token > 'Edit Cloudflare Workers' template)\n");
  const openrouter = await readHidden("OpenRouter API key: ");
  if (!openrouter) stop("No OpenRouter key entered.");
  const cloudflare = await readHidden("Cloudflare API token: ");
  if (!cloudflare) stop("No Cloudflare token entered.");
  return { openrouter, cloudflare };
}

async function validateOpenRouter(key) {
  const response = await fetch("https://openrouter.ai/api/v1/auth/key", { headers: { authorization: `Bearer ${key}` } });
  if (response.status === 401 || response.status === 403) stop("OpenRouter rejected that key. Create a new one at https://openrouter.ai/settings/keys and run this again.");
  if (!response.ok) stop(`OpenRouter returned HTTP ${response.status}. Try again in a minute.`);
  const details = describeOpenRouterKey(await response.json());
  ok("OpenRouter key accepted");
  if (details.ok && details.freeTier) console.log(`  note ${details.note}`);
}

async function findAccount(token) {
  const result = await cf("/accounts", token);
  if (result.status === 401 || result.status === 403 || !result.json?.success) {
    stop("Cloudflare rejected that token. Create one with the 'Edit Cloudflare Workers' template at https://dash.cloudflare.com/profile/api-tokens and run this again.");
  }
  const accounts = result.json.result ?? [];
  if (accounts.length === 0) stop("That token can see no Cloudflare account. When creating it, choose your account under Account Resources.");
  let account = accounts[0];
  if (accounts.length > 1) {
    accounts.forEach((candidate, index) => console.log(`  ${index + 1}. ${candidate.name}`));
    const choice = Number(await readLine("Which account? Enter a number: "));
    account = accounts[choice - 1];
    if (!account) stop("That is not one of the accounts.");
  }
  ok(`Cloudflare account: ${account.name}`);
  return account;
}

async function ensureSubdomain(token, account) {
  const current = await cf(`/accounts/${account.id}/workers/subdomain`, token);
  const existing = current.json?.result?.subdomain;
  if (existing) { ok(`workers.dev address: ${existing}.workers.dev`); return existing; }
  const wanted = suggestSubdomain(account.name);
  const created = await cf(`/accounts/${account.id}/workers/subdomain`, token, { method: "PUT", body: { subdomain: wanted } });
  if (!created.json?.success) {
    stop(`Your Cloudflare account has no workers.dev address yet and it could not be created here. Open https://dash.cloudflare.com/?to=/:account/workers-and-pages once to set it up, then run this again.`);
  }
  ok(`Created the workers.dev address ${wanted}.workers.dev`);
  return wanted;
}

async function storeSecrets(repo, values) {
  const env = await gh(["api", "-X", "PUT", `repos/${repo}/environments/${ENVIRONMENT}`]);
  if (env.code !== 0) stop(`Could not create the ${ENVIRONMENT} environment:\n${env.stderr.trim()}`);
  for (const [name, value] of Object.entries(values)) {
    const result = await gh(["secret", "set", name, "--env", ENVIRONMENT, "--repo", repo], { input: value });
    if (result.code !== 0) stop(`Could not store ${name}:\n${result.stderr.trim()}`);
  }
  ok(`Stored OPENROUTER_API_KEY, CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID in the ${ENVIRONMENT} environment`);
}

async function deployRelay(repo) {
  const startedAt = Date.now() - 5000;
  const dispatch = await gh(["workflow", "run", WORKFLOW, "--repo", repo, "--ref", "main"]);
  if (dispatch.code !== 0) stop(`Could not start the deploy workflow. Is relay.yml on the main branch?\n${dispatch.stderr.trim()}`);
  let runId = null;
  for (let attempt = 0; attempt < 30 && !runId; attempt += 1) {
    await new Promise((r) => setTimeout(r, 4000));
    const list = await gh(["run", "list", "--workflow", WORKFLOW, "--repo", repo, "--limit", "5", "--json", "databaseId,createdAt"]);
    const runs = JSON.parse(list.stdout || "[]").filter((entry) => Date.parse(entry.createdAt) >= startedAt);
    runId = runs[0]?.databaseId ?? null;
  }
  if (!runId) stop("The deploy run did not start. Open the Actions tab and run 'Deploy free assistant relay' by hand.");
  process.stdout.write("  deploying");
  for (let attempt = 0; attempt < 120; attempt += 1) {
    const view = await gh(["run", "view", String(runId), "--repo", repo, "--json", "status,conclusion"]);
    const state = JSON.parse(view.stdout || "{}");
    if (state.status === "completed") {
      process.stdout.write("\n");
      if (state.conclusion !== "success") {
        const log = await gh(["run", "view", String(runId), "--repo", repo, "--log-failed"]);
        console.log(log.stdout.split("\n").slice(-25).join("\n"));
        stop(`The deploy workflow ${state.conclusion}. The last lines of its log are above; the full run is at https://github.com/${repo}/actions/runs/${runId}`);
      }
      ok("Relay deployed");
      const log = await gh(["run", "view", String(runId), "--repo", repo, "--log"]);
      return extractWorkerUrl(log.stdout);
    }
    process.stdout.write(".");
    await new Promise((r) => setTimeout(r, 5000));
  }
  stop("The deploy took longer than 10 minutes. Check the Actions tab.");
}

async function testRelay(workerUrl) {
  const config = JSON.parse(await readFile(CONFIG_PATH, "utf8"));
  const endpoint = chatEndpoint(workerUrl);
  let last = "";
  for (let attempt = 0; attempt < 4; attempt += 1) {
    if (attempt > 0) await new Promise((r) => setTimeout(r, 8000));
    try {
      const response = await fetch(endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ model: config.model, messages: [{ role: "user", content: "Reply with the single word OK." }] }),
      });
      const text = await response.text();
      if (response.ok && /"choices"/.test(text)) { ok(`Relay answered a real request through OpenRouter (${config.model})`); return endpoint; }
      last = `HTTP ${response.status}: ${text.slice(0, 200)}`;
    } catch (error) {
      last = String(error.message);
    }
  }
  console.log(`  warn the relay did not answer a test request yet (${last}).`);
  console.log("       A free model can be busy or rate-limited for a few minutes. The pull request is still opened; do not merge it until a test request works.");
  return endpoint;
}

async function openPullRequest(repo, endpoint, originalBranch) {
  await git(["fetch", "origin", "main"]);
  const switched = await git(["switch", "-C", BRANCH, "origin/main"]);
  if (switched.code !== 0) stop(`Could not create the ${BRANCH} branch:\n${switched.stderr.trim()}`);
  try {
    const config = JSON.parse(await readFile(CONFIG_PATH, "utf8"));
    await writeFile(CONFIG_PATH, `${JSON.stringify(withEndpoint(config, endpoint), null, 2)}\n`, "utf8");
    await git(["add", "src/free-assistant.json"]);
    const commit = await git(["commit", "-m", "feat: turn on the free setup assistant\n\nSet the deployed relay address so the free model is offered and becomes the default.\n\nCo-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"]);
    if (commit.code !== 0) stop(`Nothing to commit (is the endpoint already set?):\n${commit.stdout.trim()}`);
    const push = await git(["push", "-u", "--force-with-lease", "origin", BRANCH]);
    if (push.code !== 0) stop(`Could not push the branch:\n${push.stderr.trim()}`);
    const existing = await gh(["pr", "list", "--repo", repo, "--head", BRANCH, "--json", "url", "-q", ".[0].url"]);
    if (existing.stdout.trim()) return existing.stdout.trim();
    const pr = await gh(["pr", "create", "--repo", repo, "--base", "main", "--head", BRANCH, "--title", "feat: turn on the free setup assistant", "--body",
      `The relay is deployed, so \`src/free-assistant.json\` now names it. The free model is offered on desktop and web and becomes the default setup assistant.\n\nThe maintainer's OpenRouter key stays a Cloudflare Worker secret; nothing secret is in this change.\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)`]);
    if (pr.code !== 0) stop(`Could not open the pull request:\n${pr.stderr.trim()}`);
    return pr.stdout.trim().split("\n").pop();
  } finally {
    if (originalBranch) await git(["switch", originalBranch]);
  }
}

async function main() {
  const checkOnly = process.argv.includes("--check");
  const total = 7;
  try {
    step(1, total, "Checking this computer");
    const { repo, branch } = await preflight();
    if (checkOnly) { console.log("\nReady. Run without --check to continue."); return; }

    step(2, total, "Keys");
    const keys = await collectKeys();

    step(3, total, "Checking the keys");
    await validateOpenRouter(keys.openrouter);
    const account = await findAccount(keys.cloudflare);
    const subdomain = await ensureSubdomain(keys.cloudflare, account);

    step(4, total, "Storing the secrets on GitHub");
    await storeSecrets(repo, { OPENROUTER_API_KEY: keys.openrouter, CLOUDFLARE_API_TOKEN: keys.cloudflare, CLOUDFLARE_ACCOUNT_ID: account.id });

    step(5, total, "Deploying the relay (about two minutes)");
    const workerUrl = (await deployRelay(repo)) ?? workerUrlFor(subdomain);
    ok(`Relay address: ${workerUrl}`);

    step(6, total, "Testing the relay with one real request");
    const endpoint = await testRelay(workerUrl);

    step(7, total, "Opening the pull request that turns it on");
    const pr = await openPullRequest(repo, endpoint, branch);
    console.log(`\nDone.\n  Pull request: ${pr}\n  Relay:        ${endpoint}\n\nTell Claude "free assistant deployed" and it will finish the testing and merge.`);
  } catch (error) {
    if (error instanceof Stop) { console.error(`\nStopped: ${error.message}`); process.exitCode = 1; return; }
    console.error(`\nUnexpected error: ${error.message}`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await main();
