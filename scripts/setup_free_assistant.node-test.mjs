import assert from "node:assert/strict";
import test from "node:test";
import { chatEndpoint, describeOpenRouterKey, extractWorkerUrl, suggestSubdomain, withEndpoint, workerUrlFor } from "./setup_free_assistant.mjs";

test("the worker address is read from the deploy log, last one wins", () => {
  const log = "Uploaded hoi4-mod-setup-free\nDeployed hoi4-mod-setup-free triggers\n  https://hoi4-mod-setup-free.some-account.workers.dev\nCurrent Version ID: abc";
  assert.equal(extractWorkerUrl(log), "https://hoi4-mod-setup-free.some-account.workers.dev");
  assert.equal(extractWorkerUrl("no address here"), null);
});

test("the address can be built from the account's workers.dev subdomain", () => {
  assert.equal(workerUrlFor("klim-1234"), "https://hoi4-mod-setup-free.klim-1234.workers.dev");
});

test("the chat endpoint is the worker address plus the OpenAI-compatible path", () => {
  assert.equal(chatEndpoint("https://x.y.workers.dev/"), "https://x.y.workers.dev/v1/chat/completions");
  assert.equal(chatEndpoint("https://x.y.workers.dev"), "https://x.y.workers.dev/v1/chat/completions");
});

test("only the endpoint changes in the app's free-assistant config", () => {
  const next = withEndpoint({ endpoint: null, model: "m:free" }, "https://e/v1/chat/completions");
  assert.deepEqual(next, { endpoint: "https://e/v1/chat/completions", model: "m:free" });
});

test("a suggested subdomain is a safe DNS label", () => {
  const value = suggestSubdomain("Klim Pashkov's Account!", () => 0.5);
  assert.match(value, /^[a-z0-9]+(-[a-z0-9]+)*$/);
  assert.ok(value.length <= 63);
  assert.match(suggestSubdomain("", () => 0.1), /^hoi4-mod-setup-\d{4}$/);
});

test("an OpenRouter key without credits is flagged with the daily limit", () => {
  const free = describeOpenRouterKey({ data: { is_free_tier: true } });
  assert.equal(free.freeTier, true);
  assert.match(free.note, /50 free-model requests a day/);
  assert.equal(describeOpenRouterKey({ data: { is_free_tier: false } }).freeTier, false);
  assert.equal(describeOpenRouterKey({}).ok, false);
});
