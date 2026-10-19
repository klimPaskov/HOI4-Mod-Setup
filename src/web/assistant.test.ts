import { afterEach, describe, expect, it, vi } from "vitest";
import { AssistantError, BROWSER_PROVIDERS, draftDescription, FREE_PROVIDER, needsKey, OPENROUTER_FREE_MODEL, provenanceFor, routeFor, runAssistant } from "./assistant";
import type { CoreClient } from "./core-client";
import type { Analysis, AnalysisRequest, CoreOperations, Operation } from "./protocol";

const request: AnalysisRequest = { mode: "new_project_identity", brief: "A Balkan mod", evidence: [], constraints: {} };
const analysis: Analysis = { analysis_id: "id", project_summary: "A Balkan mod.", proposals: [], component_recommendations: [], warnings: [] };

function core(replies: Array<CoreOperations["assistantReply"]["result"]>) {
  const calls: Array<{ op: Operation; args: unknown }> = [];
  const client: CoreClient = {
    call: vi.fn(async (op: Operation, args: unknown) => {
      calls.push({ op, args });
      if (op === "assistantTurn") return { inputSha256: "a".repeat(64), headers: [["accept", "application/json"], ["content-type", "application/json"]], body: { model: "m" } };
      if (op === "assistantReply") return replies.shift();
      throw new Error(`unexpected ${op}`);
    }) as unknown as CoreClient["call"],
    onProgress: () => () => undefined,
    terminate: () => undefined,
  };
  return { client, calls };
}

afterEach(() => vi.unstubAllGlobals());

const CLAUDE = BROWSER_PROVIDERS.find((item) => item.id === "claude")!;
const OPENROUTER = BROWSER_PROVIDERS.find((item) => item.id === "openrouter")!;

describe("browser assistant", () => {
  it("adds the key only as a header and opts in to Anthropic browser access", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const route = routeFor(CLAUDE, CLAUDE.defaultModel, "medium");
    const { client, calls } = core([{ analysis }]);
    await expect(runAssistant(client, route, " sk-test-key ", request)).resolves.toEqual(analysis);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("https://api.anthropic.com/v1/messages");
    const headers = init.headers as Headers;
    expect(headers.get("x-api-key")).toBe("sk-test-key");
    expect(headers.get("anthropic-dangerous-direct-browser-access")).toBe("true");
    expect(init.credentials).toBe("omit");
    expect(init.redirect).toBe("error");
    expect(JSON.stringify(calls)).not.toContain("sk-test-key");
  });

  it("uses a bearer token on OpenAI-style routes and retries once with the validator's reason", async () => {
    const fetchMock = vi.fn().mockImplementation(async () => new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const gemini = BROWSER_PROVIDERS.find((item) => item.id === "gemini")!;
    const route = routeFor(gemini, gemini.defaultModel, "high");
    const { client, calls } = core([{ retry: "project_id is invalid" }, { analysis }]);
    await runAssistant(client, route, "key", request);
    expect((fetchMock.mock.calls[0][1].headers as Headers).get("authorization")).toBe("Bearer key");
    const turns = calls.filter((call) => call.op === "assistantTurn").map((call) => (call.args as { correction: string | null }).correction);
    expect(turns).toEqual([null, "project_id is invalid"]);
  });

  it("stops after a second rejected reply and never retries a provider error", async () => {
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => new Response("{}", { status: 200 })));
    const route = routeFor(CLAUDE, CLAUDE.defaultModel, "medium");
    await expect(runAssistant(core([{ retry: "a" }, { retry: "b" }]).client, route, "key", request)).rejects.toThrow(/twice/);
    const truncated = core([{ error: "provider output was truncated" }]);
    await expect(runAssistant(truncated.client, route, "key", request)).rejects.toThrow(/truncated/);
    expect(truncated.calls.filter((call) => call.op === "assistantTurn")).toHaveLength(1);
  });

  it("explains rejected keys and usage limits", async () => {
    const route = routeFor(CLAUDE, CLAUDE.defaultModel, "medium");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 401 })));
    await expect(runAssistant(core([]).client, route, "key", request)).rejects.toThrow(/did not accept the API key/);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 429 })));
    await expect(runAssistant(core([]).client, route, "key", request)).rejects.toThrow(/usage is limited/);
    await expect(runAssistant(core([]).client, route, "  ", request)).rejects.toBeInstanceOf(AssistantError);
  });

  it("records only non-secret provenance and rejects odd model IDs", () => {
    const glm = BROWSER_PROVIDERS.find((item) => item.id === "glm")!;
    expect(provenanceFor(routeFor(glm, glm.defaultModel, "high"))).toEqual({ provider: "glm", model: glm.defaultModel, reasoningEffort: "none", endpoint: glm.endpoint, optimizationProfile: "GLM setup analysis" });
    expect(() => routeFor(glm, "model with spaces", "high")).toThrow(/model ID/);
  });

  it("talks to a local model on this computer without a key", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const local = BROWSER_PROVIDERS.find((item) => item.id === "local")!;
    expect(() => routeFor(local, "qwen3", "none", "http://192.168.1.10:11434/v1/chat/completions")).toThrow(/address on this computer/);
    expect(() => routeFor(local, "qwen3", "none", "https://ollama.example/v1")).toThrow(/address on this computer/);
    // A hosted route cannot be pointed anywhere else.
    expect(() => routeFor(CLAUDE, "claude-haiku-5-5", "medium", "https://evil.example/v1")).toThrow();
    const route = routeFor(local, "qwen3", "none", "http://127.0.0.1:11434/v1/chat/completions");
    const { client } = core([{ analysis }]);
    await runAssistant(client, route, "", request);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("http://127.0.0.1:11434/v1/chat/completions");
    expect((init.headers as Headers).has("authorization")).toBe(false);
    expect(provenanceFor(route)).toMatchObject({ provider: "local", endpoint: "http://127.0.0.1:11434/v1/chat/completions" });
  });

  it("leads with the free route when its relay is deployed, and OpenRouter otherwise", () => {
    expect(BROWSER_PROVIDERS[0].id).toBe(FREE_PROVIDER ? "free" : "openrouter");
    expect(OPENROUTER.signIn).toBe(true);
    expect(OPENROUTER.defaultModel).toBe(OPENROUTER_FREE_MODEL);
    expect(OPENROUTER_FREE_MODEL.endsWith(":free")).toBe(true);
    expect(needsKey(OPENROUTER)).toBe(true);
    expect(needsKey({ free: true })).toBe(false);
    expect(needsKey({ local: true })).toBe(false);
    // An OpenRouter key goes only to OpenRouter.
    expect(() => routeFor(OPENROUTER, OPENROUTER.defaultModel, "none", "https://evil.example/api/v1/chat/completions")).toThrow();
    expect(provenanceFor(routeFor(OPENROUTER, OPENROUTER.defaultModel, "none"))).toMatchObject({ provider: "openrouter", endpoint: "https://openrouter.ai/api/v1/chat/completions" });
  });

  it("sends free requests without any key and explains relay limits", async () => {
    const free = FREE_PROVIDER ?? { ...OPENROUTER, id: "free", label: "Free model", endpoint: "https://relay.example/v1/chat/completions", free: true, signIn: false };
    expect(() => routeFor(free, "anthropic/claude-opus", "none")).toThrow(/only its own model/);
    const route = routeFor(free, free.defaultModel, "none");
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    await runAssistant(core([{ analysis }]).client, route, "typed-by-mistake", request);
    expect((fetchMock.mock.calls[0][1].headers as Headers).has("authorization")).toBe(false);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 429 })));
    await expect(runAssistant(core([]).client, route, "", request)).rejects.toThrow(/free assistant is busy/);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 503 })));
    await expect(runAssistant(core([]).client, route, "", request)).rejects.toThrow(/not available right now/);
  });

  it("asks OpenRouter users to sign in or paste a key", async () => {
    const route = routeFor(OPENROUTER, OPENROUTER.defaultModel, "none");
    await expect(runAssistant(core([]).client, route, "", request)).rejects.toThrow(/Sign in with OpenRouter/);
  });

  it("drafts a fuller description with the chosen route and never retries", async () => {
    const fetchMock = vi.fn().mockImplementation(async () => new Response('{"choices":[]}', { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const calls: Array<{ op: Operation; args: unknown }> = [];
    const replies: Array<CoreOperations["descriptionReply"]["result"]> = [{ description: "A fuller Balkans mod." }, { error: "the draft did not match its schema" }];
    const client: CoreClient = {
      call: vi.fn(async (op: Operation, args: unknown) => {
        calls.push({ op, args });
        if (op === "descriptionTurn") return { headers: [["content-type", "application/json"]], body: { model: "m" } };
        if (op === "descriptionReply") return replies.shift();
        throw new Error(`unexpected ${op}`);
      }) as unknown as CoreClient["call"],
      onProgress: () => () => undefined,
      terminate: () => undefined,
    };
    const route = routeFor(OPENROUTER, OPENROUTER.defaultModel, "none");
    await expect(draftDescription(client, route, "or-key", "balkans")).resolves.toBe("A fuller Balkans mod.");
    expect((fetchMock.mock.calls[0][1].headers as Headers).get("authorization")).toBe("Bearer or-key");
    expect(calls[0]).toEqual({ op: "descriptionTurn", args: { route, note: "balkans" } });
    expect(JSON.stringify(calls)).not.toContain("or-key");
    await expect(draftDescription(client, route, "or-key", "balkans")).rejects.toThrow(/not usable/);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    await expect(draftDescription(client, route, "", "balkans")).rejects.toThrow(/Sign in with OpenRouter/);
  });
});
