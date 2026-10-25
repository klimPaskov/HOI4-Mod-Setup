import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { checkBody, handle } from "./worker.mjs";

const MODEL = "nvidia/nemotron-3.5-super-vl:free";
const ORIGIN = "https://klimpaskov.github.io";
const KEY = ["sk", "or", "v1", "relay-test"].join("-");
const env = (extra = {}) => ({ OPENROUTER_API_KEY: KEY, MODEL, ALLOWED_ORIGINS: ORIGIN, ...extra });
const body = (extra = {}) => ({
  model: MODEL,
  messages: [{ role: "system", content: "Return JSON." }, { role: "user", content: "A mod." }],
  response_format: { type: "json_schema", json_schema: { name: "mod_description", strict: true, schema: { type: "object" } } },
  ...extra,
});

function post(payload, headers = {}) {
  const text = typeof payload === "string" ? payload : JSON.stringify(payload);
  return new Request("https://relay.example/v1/chat/completions", { method: "POST", headers: { "content-type": "application/json", ...headers }, body: text });
}

function upstreamReturning(status, text, calls = []) {
  return async (url, init) => {
    calls.push({ url, init });
    return new Response(text, { status, headers: { "content-type": "application/json" } });
  };
}

test("forwards the reviewed shape with the secret key and the configured model only", async () => {
  const calls = [];
  const response = await handle(post(body(), { origin: ORIGIN }), env(), upstreamReturning(200, '{"choices":[]}', calls));
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("access-control-allow-origin"), ORIGIN);
  assert.equal(await response.text(), '{"choices":[]}');
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, "https://openrouter.ai/api/v1/chat/completions");
  assert.equal(calls[0].init.headers.authorization, `Bearer ${KEY}`);
  assert.equal(calls[0].init.redirect, "manual");
  const sent = JSON.parse(calls[0].init.body);
  assert.deepEqual(Object.keys(sent).sort(), ["max_tokens", "messages", "model", "provider", "response_format"]);
  assert.equal(sent.model, MODEL);
  assert.equal(sent.max_tokens, 8192);
});

test("the upstream call uses only fetch options the Workers runtime accepts", async () => {
  const calls = [];
  await handle(post(body(), { origin: ORIGIN }), env(), upstreamReturning(200, '{"choices":[]}', calls));
  // Workers throw on any other redirect mode ("error"), which showed up only once deployed.
  assert.ok(["follow", "manual"].includes(calls[0].init.redirect));
});

test("a refusal from the model provider carries its short reason", async () => {
  const response = await handle(post(body(), { origin: ORIGIN }), env(), upstreamReturning(404, '{"error":{"message":"No endpoints found matching your data policy"}}', []));
  assert.equal(response.status, 502);
  assert.match(await response.text(), /HTTP 404: No endpoints found matching your data policy/);
  const plain = await handle(post(body(), { origin: ORIGIN }), env(), upstreamReturning(500, "not json", []));
  assert.match(await plain.text(), /HTTP 500\./);
});

test("a redirect from the model provider is refused, not followed", async () => {
  const response = await handle(post(body(), { origin: ORIGIN }), env(), upstreamReturning(302, "", []));
  assert.equal(response.status, 502);
  assert.match(await response.text(), /could not reach its model/);
});

test("desktop requests without an Origin are served without CORS headers", async () => {
  const response = await handle(post(body()), env(), upstreamReturning(200, "{}"));
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("access-control-allow-origin"), null);
});

test("other pages, routes, and methods are refused before the upstream is called", async () => {
  const calls = [];
  const upstream = upstreamReturning(200, "{}", calls);
  assert.equal((await handle(post(body(), { origin: "https://evil.example" }), env(), upstream)).status, 403);
  assert.equal((await handle(new Request("https://relay.example/v1/models"), env(), upstream)).status, 404);
  assert.equal((await handle(new Request("https://relay.example/v1/chat/completions"), env(), upstream)).status, 405);
  assert.equal((await handle(post(body()), env({ OPENROUTER_API_KEY: "" }), upstream)).status, 503);
  assert.equal(calls.length, 0);
});

test("the CORS preflight names only what the app sends", async () => {
  const response = await handle(new Request("https://relay.example/v1/chat/completions", { method: "OPTIONS", headers: { origin: ORIGIN } }), env(), upstreamReturning(200, "{}"));
  assert.equal(response.status, 204);
  assert.equal(response.headers.get("access-control-allow-methods"), "POST, OPTIONS");
  assert.equal(response.headers.get("access-control-allow-headers"), "content-type, accept");
});

test("bodies outside the reviewed shape are refused", () => {
  assert.equal(checkBody(body(), MODEL), null);
  assert.equal(checkBody({ model: MODEL, messages: [{ role: "user", content: "x" }] }, MODEL), null);
  assert.match(checkBody(body({ model: "anthropic/claude-opus" }), MODEL), /only runs/);
  assert.match(checkBody(body({ tools: [] }), MODEL), /Unsupported fields: tools/);
  assert.match(checkBody(body({ stream: true }), MODEL), /Unsupported fields: stream/);
  assert.match(checkBody(body({ messages: [] }), MODEL), /one to four/);
  assert.match(checkBody(body({ messages: Array(5).fill({ role: "user", content: "x" }) }), MODEL), /one to four/);
  assert.match(checkBody(body({ messages: [{ role: "assistant", content: "x" }] }), MODEL), /system and user/);
  assert.match(checkBody(body({ messages: [{ role: "user", content: "x", name: "y" }] }), MODEL), /only a role/);
  assert.match(checkBody(body({ messages: [{ role: "user", content: "x".repeat(160_001) }] }), MODEL), /bounded/);
  assert.match(checkBody(body({ response_format: { type: "text" } }), MODEL), /JSON Schema/);
  assert.match(checkBody(body({ response_format: { type: "json_schema", json_schema: { schema: "x".repeat(50_000) } } }), MODEL), /too large/);
  assert.match(checkBody([], MODEL), /object/);
});

test("oversized and malformed requests are refused", async () => {
  const upstream = upstreamReturning(200, "{}");
  assert.equal((await handle(post("x".repeat(256 * 1024 + 1)), env(), upstream)).status, 413);
  assert.equal((await handle(post("{not json"), env(), upstream)).status, 400);
});

test("rate limits apply per address before any upstream call", async () => {
  const calls = [];
  const keys = [];
  const limiter = { limit: async ({ key }) => { keys.push(key); return { success: false }; } };
  const response = await handle(post(body(), { "cf-connecting-ip": "203.0.113.9" }), env({ RATE_LIMITER: limiter }), upstreamReturning(200, "{}", calls));
  assert.equal(response.status, 429);
  assert.deepEqual(keys, ["203.0.113.9"]);
  assert.equal(calls.length, 0);
});

test("upstream failures never blame the user's credentials", async () => {
  for (const [status, expected] of [[401, 503], [403, 503], [402, 429], [429, 429], [500, 502]]) {
    const response = await handle(post(body()), env(), upstreamReturning(status, "{}"));
    assert.equal(response.status, expected, String(status));
    const text = await response.text();
    assert.ok(!text.includes(KEY));
  }
  const unreachable = await handle(post(body()), env(), async () => { throw new Error("down"); });
  assert.equal(unreachable.status, 502);
});

test("the relay's model matches the app's checked-in free model", () => {
  const config = JSON.parse(readFileSync(new URL("../src/free-assistant.json", import.meta.url), "utf8"));
  const toml = readFileSync(new URL("./wrangler.toml", import.meta.url), "utf8");
  assert.match(toml, new RegExp(`^MODEL = "${config.model.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"$`, "m"));
});

test("fallback models are sent as an ordered list behind the configured one", async () => {
  const calls = [];
  await handle(post(body(), { origin: ORIGIN }), env({ FALLBACK_MODELS: "a/b:free, c/d:free" }), upstreamReturning(200, '{"choices":[]}', calls));
  const sent = JSON.parse(calls[0].init.body);
  assert.deepEqual(sent.models, [MODEL, "a/b:free", "c/d:free"]);
  assert.equal(sent.model, undefined);
});
