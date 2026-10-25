// The free setup assistant relay (Cloudflare Worker).
//
// HOI4 Mod Setup's default assistant needs no account: the app sends one
// structured chat request here, and the relay forwards it to a single free
// OpenRouter model with the maintainer's key. The key is a Worker secret; it
// never reaches the app, the page, or the repository.
//
// The relay accepts only the shape the app's core builds (model, messages,
// response_format), only the configured model, bounded sizes, and a few
// requests per minute from each address. Everything else is refused.

const MAX_BODY_BYTES = 256 * 1024;
const MAX_MESSAGE_CHARS = 160_000;
const MAX_SCHEMA_BYTES = 48 * 1024;
const MAX_REPLY_BYTES = 4 * 1024 * 1024;
const MAX_TOKENS = 8192;
const UPSTREAM = "https://openrouter.ai/api/v1/chat/completions";
const ROUTE = "/v1/chat/completions";
const APP_URL = "https://klimpaskov.github.io/HOI4-Mod-Setup/";

export default {
  async fetch(request, env) {
    return handle(request, env, fetch);
  },
};

/** The relay with an injectable upstream `fetch`, for tests. */
export async function handle(request, env, upstreamFetch) {
  const origin = request.headers.get("origin");
  const allowed = allowedOrigins(env);
  // Desktop requests carry no Origin; a browser page must be the app's.
  if (origin !== null && !allowed.includes(origin)) return reply(403, "This page may not use the free assistant.", null);
  const cors = origin === null ? null : origin;
  const url = new URL(request.url);
  if (request.method === "OPTIONS") {
    if (url.pathname !== ROUTE || cors === null) return reply(404, "Not found.", cors);
    return new Response(null, { status: 204, headers: corsHeaders(cors, true) });
  }
  if (url.pathname !== ROUTE) return reply(404, "Not found.", cors);
  if (request.method !== "POST") return reply(405, "Use POST.", cors);
  if (!env.OPENROUTER_API_KEY || !env.MODEL) return reply(503, "The free assistant is not configured.", cors);

  if (env.RATE_LIMITER) {
    const key = request.headers.get("cf-connecting-ip") ?? "unknown";
    const { success } = await env.RATE_LIMITER.limit({ key });
    if (!success) return reply(429, "The free assistant is busy. Wait a minute and try again.", cors);
  }

  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_BODY_BYTES) return reply(413, "The request is too large.", cors);
  const text = await request.text();
  if (new TextEncoder().encode(text).length > MAX_BODY_BYTES) return reply(413, "The request is too large.", cors);

  let body;
  try {
    body = JSON.parse(text);
  } catch {
    return reply(400, "The request is not JSON.", cors);
  }
  const problem = checkBody(body, env.MODEL);
  if (problem) return reply(400, problem, cors);

  const upstreamBody = {
    model: env.MODEL,
    messages: body.messages.map(({ role, content }) => ({ role, content })),
    max_tokens: MAX_TOKENS,
    provider: { require_parameters: true },
  };
  if (body.response_format) upstreamBody.response_format = body.response_format;

  let upstream;
  try {
    upstream = await upstreamFetch(UPSTREAM, {
      method: "POST",
      headers: {
        authorization: `Bearer ${env.OPENROUTER_API_KEY}`,
        "content-type": "application/json",
        accept: "application/json",
        "http-referer": APP_URL,
        "x-title": "HOI4 Mod Setup",
      },
      body: JSON.stringify(upstreamBody),
      // Workers accept only "follow" or "manual" here ("error" throws), so a
      // redirect is held back and refused below instead of followed.
      redirect: "manual",
    });
  } catch {
    return reply(502, "The free assistant could not reach its model.", cors);
  }
  if (upstream.status >= 300 && upstream.status < 400) {
    return reply(502, "The free assistant could not reach its model.", cors);
  }
  if (upstream.status === 429 || upstream.status === 402) {
    return reply(429, "The free assistant has reached its limit for now. Try again later, or choose another assistant.", cors);
  }
  if (upstream.status === 401 || upstream.status === 403) {
    // The maintainer's key, not the user's: never report it as theirs.
    return reply(503, "The free assistant is not available right now. Choose another assistant.", cors);
  }
  const replyText = await upstream.text();
  if (new TextEncoder().encode(replyText).length > MAX_REPLY_BYTES) return reply(502, "The model's reply was too large.", cors);
  if (!upstream.ok) return reply(502, `The model returned HTTP ${upstream.status}${upstreamReason(replyText)}.`, cors);
  return new Response(replyText, {
    status: 200,
    headers: { "content-type": "application/json", "cache-control": "no-store", ...corsHeaders(cors, false) },
  });
}

/** The reason a request body is refused, or null. */
export function checkBody(body, model) {
  if (!body || typeof body !== "object" || Array.isArray(body)) return "The request must be a JSON object.";
  const extra = Object.keys(body).filter((key) => !["model", "messages", "response_format"].includes(key));
  if (extra.length > 0) return `Unsupported fields: ${extra.slice(0, 5).join(", ")}.`;
  if (body.model !== model) return `The free assistant only runs ${model}.`;
  const { messages } = body;
  if (!Array.isArray(messages) || messages.length < 1 || messages.length > 4) return "Send one to four messages.";
  for (const message of messages) {
    if (!message || typeof message !== "object" || Array.isArray(message)) return "Each message must be an object.";
    if (Object.keys(message).some((key) => key !== "role" && key !== "content")) return "Messages carry only a role and text.";
    if (message.role !== "system" && message.role !== "user") return "Only system and user messages are accepted.";
    if (typeof message.content !== "string" || message.content.length === 0 || message.content.length > MAX_MESSAGE_CHARS) return "Each message must be bounded text.";
  }
  if (body.response_format !== undefined) {
    const format = body.response_format;
    if (!format || format.type !== "json_schema" || typeof format.json_schema !== "object" || format.json_schema === null) return "Only a JSON Schema response format is accepted.";
    if (JSON.stringify(format).length > MAX_SCHEMA_BYTES) return "The response schema is too large.";
  }
  return null;
}

/** OpenRouter's own short reason for a refusal (never the request or any key), so a setup problem is diagnosable. */
export function upstreamReason(text) {
  try {
    const message = JSON.parse(text)?.error?.message;
    if (typeof message === "string" && message.trim()) return `: ${message.replace(/\s+/g, " ").trim().slice(0, 240)}`;
  } catch {
    // not JSON
  }
  return "";
}

function allowedOrigins(env) {
  return String(env.ALLOWED_ORIGINS ?? "")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
}

function corsHeaders(origin, preflight) {
  if (origin === null) return {};
  const headers = { "access-control-allow-origin": origin, vary: "Origin" };
  if (preflight) {
    headers["access-control-allow-methods"] = "POST, OPTIONS";
    headers["access-control-allow-headers"] = "content-type, accept";
    headers["access-control-max-age"] = "600";
  }
  return headers;
}

function reply(status, message, origin) {
  return new Response(JSON.stringify({ error: { message } }), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store", ...corsHeaders(origin, false) },
  });
}
