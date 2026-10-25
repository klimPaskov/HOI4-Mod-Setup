// The browser setup assistant. Every web setup uses one: it sends a
// structured-output request to a reviewed hosted route that allows browser
// requests, with the API key the user typed or got from OpenRouter sign-in
// for this tab only. The free route needs no key: the maintainer's relay holds
// it. The WebAssembly core builds the request body
// and validates the reply with the same rules as the desktop app; provider
// suggestions never become detected facts or files without user review.

import claudeModels from "../claude-models.json";
import freeAssistantConfig from "../free-assistant.json";
import { recordRequest } from "./github";
import type { CoreClient } from "./core-client";
import type { Analysis, AnalysisRequest, AssistantProvenance, AssistantReply, AssistantRoute, AssistantTurn } from "./protocol";

export interface BrowserProvider {
  id: string;
  label: string;
  protocol: AssistantRoute["protocol"];
  endpoint: string;
  defaultModel: string;
  efforts: string[];
  defaultEffort: string;
  optimizationProfile: string;
  keyUrl: string;
  /** A model server on this computer: an editable loopback address, no key. */
  local?: boolean;
  /** The maintainer's relay to a free model: no account and no key. */
  free?: boolean;
  /** OpenRouter offers sign-in that returns a key for the user's account. */
  signIn?: boolean;
}

const FREE_ASSISTANT = freeAssistantConfig as { endpoint: string | null; model: string };
export const OPENROUTER_FREE_MODEL = "nvidia/nemotron-3.5-super-vl:free";

/** The free route, once its relay is deployed. */
export const FREE_PROVIDER: BrowserProvider | null = FREE_ASSISTANT.endpoint
  ? { id: "free", label: "Free model (no account)", protocol: "openai_compatible", endpoint: FREE_ASSISTANT.endpoint, defaultModel: FREE_ASSISTANT.model, efforts: [], defaultEffort: "none", optimizationProfile: "Free setup analysis", keyUrl: "", free: true }
  : null;

/** Providers whose APIs accept requests from a web page. Routes match the
 * desktop app's checked-in defaults. */
export const BROWSER_PROVIDERS: BrowserProvider[] = [
  ...(FREE_PROVIDER ? [FREE_PROVIDER] : []),
  { id: "openrouter", label: "OpenRouter", protocol: "openai_compatible", endpoint: "https://openrouter.ai/api/v1/chat/completions", defaultModel: OPENROUTER_FREE_MODEL, efforts: [], defaultEffort: "none", optimizationProfile: "OpenRouter setup analysis", keyUrl: "https://openrouter.ai/settings/keys", signIn: true },
  { id: "claude", label: "Claude API key", protocol: "anthropic_messages", endpoint: "https://api.anthropic.com/v1/messages", defaultModel: claudeModels[0]?.id ?? "claude-haiku-5-5", efforts: claudeModels[0]?.supported_reasoning_efforts ?? ["medium"], defaultEffort: claudeModels[0]?.default_reasoning_effort ?? "medium", optimizationProfile: "Claude setup analysis", keyUrl: "https://platform.claude.com/settings/keys" },
  { id: "gemini", label: "Gemini API key", protocol: "openai_compatible", endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions", defaultModel: "gemini-3.8-flash", efforts: ["low", "medium", "high"], defaultEffort: "high", optimizationProfile: "Gemini setup analysis", keyUrl: "https://aistudio.google.com/api-keys" },
  { id: "grok", label: "Grok API key", protocol: "openai_responses", endpoint: "https://api.x.ai/v1/responses", defaultModel: "grok-4.7", efforts: ["low", "medium", "high"], defaultEffort: "high", optimizationProfile: "Grok setup analysis", keyUrl: "https://console.x.ai" },
  { id: "deepseek", label: "DeepSeek", protocol: "openai_compatible", endpoint: "https://api.deepseek.com/chat/completions", defaultModel: "deepseek-flash", efforts: ["low", "high", "max"], defaultEffort: "high", optimizationProfile: "DeepSeek setup analysis", keyUrl: "https://platform.deepseek.com/api_keys" },
  { id: "glm", label: "GLM", protocol: "openai_compatible", endpoint: "https://open.bigmodel.cn/api/paas/v4/chat/completions", defaultModel: "glm-5.2", efforts: [], defaultEffort: "none", optimizationProfile: "GLM setup analysis", keyUrl: "https://bigmodel.cn/usercenter/proj-mgmt/apikeys" },
  { id: "local", label: "Local model", protocol: "openai_compatible", endpoint: "http://127.0.0.1:11434/v1/chat/completions", defaultModel: "", efforts: [], defaultEffort: "none", optimizationProfile: "Local setup analysis", keyUrl: "https://docs.ollama.com/api/openai-compatibility", local: true },
];

/** An OpenAI-compatible endpoint on this computer, without credentials. */
export function isLoopbackEndpoint(value: string): boolean {
  try {
    const url = new URL(value);
    // Only the origins the page's Content Security Policy allows.
    return url.protocol === "http:" && ["127.0.0.1", "localhost"].includes(url.hostname) && url.port !== "" && !url.username && !url.password && !url.search && !url.hash;
  } catch {
    return false;
  }
}

const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
const REQUEST_TIMEOUT_MS = 120_000;
export const MODEL_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;

/** Model IDs to suggest for a provider; any other exact ID may be typed. */
export function modelChoices(provider: BrowserProvider): string[] {
  return provider.id === "claude" ? claudeModels.map((item) => item.id) : [provider.defaultModel];
}

/** Whether a route needs a key from the user. */
export function needsKey(provider: Pick<BrowserProvider, "local" | "free">): boolean {
  return !provider.local && !provider.free;
}

export class AssistantError extends Error {}

function supportsEffort(provider: BrowserProvider, model: string): boolean {
  if (provider.id === "claude") {
    const entry = claudeModels.find((item) => item.id === model);
    return (entry?.supported_reasoning_efforts.length ?? 0) > 1;
  }
  return provider.efforts.length > 0;
}

export function routeFor(provider: BrowserProvider, model: string, effort: string, endpoint: string = provider.endpoint): AssistantRoute {
  if (!MODEL_PATTERN.test(model)) throw new AssistantError("Enter a model ID without spaces.");
  if (provider.free && model !== provider.defaultModel) throw new AssistantError("The free assistant runs only its own model.");
  if (provider.local ? !isLoopbackEndpoint(endpoint) : endpoint !== provider.endpoint) throw new AssistantError("A local model must use an address on this computer, such as http://127.0.0.1:11434/v1/chat/completions.");
  return {
    provider: provider.id,
    protocol: provider.protocol,
    endpoint,
    model,
    reasoningEffort: effort,
    supportsEffort: supportsEffort(provider, model),
    optimizationProfile: provider.optimizationProfile,
  };
}

export function provenanceFor(route: AssistantRoute): AssistantProvenance {
  return {
    provider: route.provider,
    model: route.model,
    reasoningEffort: route.supportsEffort ? route.reasoningEffort : "none",
    endpoint: route.endpoint,
    optimizationProfile: route.optimizationProfile,
  };
}

async function send(route: AssistantRoute, apiKey: string, headers: Array<[string, string]>, body: unknown): Promise<string> {
  const requestHeaders = new Headers(headers);
  if (route.protocol === "anthropic_messages") {
    requestHeaders.set("x-api-key", apiKey);
    // Anthropic serves browser requests only when the page opts in.
    requestHeaders.set("anthropic-dangerous-direct-browser-access", "true");
  } else if (apiKey) {
    requestHeaders.set("authorization", `Bearer ${apiKey}`);
  }
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  let response: Response;
  try {
    response = await fetch(route.endpoint, {
      method: "POST",
      headers: requestHeaders,
      body: JSON.stringify(body),
      redirect: "error",
      credentials: "omit",
      referrerPolicy: "no-referrer",
      signal: controller.signal,
    });
  } catch {
    recordRequest(route.endpoint, "failed");
    throw new AssistantError(route.provider === "local"
      ? "The local model could not be reached. Start its server and allow this page's address (for Ollama, set OLLAMA_ORIGINS); your browser may also ask to allow local network access."
      : route.provider === "free"
        ? "The free assistant could not be reached. Try again later, or choose another assistant."
        : "The provider could not be reached. Check your connection and try again.");
  } finally {
    clearTimeout(timer);
  }
  recordRequest(route.endpoint, response.status);
  const text = await response.text();
  if (text.length > MAX_RESPONSE_BYTES) throw new AssistantError("The provider's reply was too large.");
  if (route.provider === "free" && !response.ok) throw new AssistantError(response.status === 429
    ? "The free assistant is busy or has reached its limit for now. Try again later, or choose another assistant."
    : "The free assistant is not available right now. Try again later, or choose another assistant.");
  if (response.status === 429) throw new AssistantError("Provider usage is limited right now. Try again later.");
  if (response.status === 401 || response.status === 403) throw new AssistantError("The provider did not accept the API key.");
  if (!response.ok) throw new AssistantError(`The provider returned HTTP ${response.status}.`);
  return text;
}

/** One analysis with at most one corrective turn, like the desktop app. */
export async function runAssistant(core: CoreClient, route: AssistantRoute, apiKey: string, request: AnalysisRequest): Promise<Analysis> {
  const key = checkedKey(route, apiKey);
  let correction: string | null = null;
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    const turn: AssistantTurn = await core.call("assistantTurn", { request, route, correction });
    const envelope = await send(route, key, turn.headers, turn.body);
    const reply: AssistantReply = await core.call("assistantReply", { envelope, request, inputSha256: turn.inputSha256, analysisId: crypto.randomUUID() });
    if ("analysis" in reply) return reply.analysis;
    if ("error" in reply) throw new AssistantError(`The provider's reply was not usable: ${reply.error}`);
    correction = reply.retry;
  }
  throw new AssistantError("The provider's reply did not match the required format twice. Try again, or choose another model.");
}

/** The key for a route: none for a local model or the free relay. */
function checkedKey(route: AssistantRoute, apiKey: string): string {
  if (route.provider === "free") return "";
  const key = apiKey.trim();
  if ((!key && route.provider !== "local") || /\s/.test(key)) throw new AssistantError(route.provider === "openrouter" ? "Sign in with OpenRouter or enter an API key." : "Enter the API key.");
  return key;
}

/** The magic wand: a fuller version of the user's note from the chosen
 * assistant. Nothing is retried; the user can ask again. */
export async function draftDescription(core: CoreClient, route: AssistantRoute, apiKey: string, note: string): Promise<string> {
  const key = checkedKey(route, apiKey);
  const turn = await core.call("descriptionTurn", { route, note });
  const envelope = await send(route, key, turn.headers, turn.body);
  const reply = await core.call("descriptionReply", { envelope });
  if ("description" in reply) return reply.description;
  throw new AssistantError(`The draft was not usable: ${reply.error}`);
}

/** The value of one proposal, when the reply has it. */
export function proposal(analysis: Analysis, key: string): unknown {
  return analysis.proposals.find((item) => item.key === key)?.value;
}
