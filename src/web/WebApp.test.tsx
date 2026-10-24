import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import WebApp from "./WebApp";
import { BROWSER_PROVIDERS, FREE_PROVIDER } from "./assistant";
import { createCoreClient } from "./core-client";
import type { CoreClient } from "./core-client";
import type { CoreOperations, ExistingSetup, Operation, SourcePreview, WorkerMessage } from "./protocol";

const COMMIT = "0b124ad9d3121fda5eb3cc5ff365f74a28db0f8f";

const preview: CoreOperations["preview"]["result"] = {
  schema_version: "6.0.0",
  revision: COMMIT,
  components: [
    { id: "workflow.super_events", display_name: "Super Events workflow", description: null, category: "workflow", optional: true, coding_environment: null, platforms: ["all"], dependencies: [], file_count: 3, total_size: 10 },
    { id: "workflow.3d", display_name: "3D models workflow", description: null, category: "workflow", optional: true, coding_environment: null, platforms: ["windows"], dependencies: [], file_count: 3, total_size: 10 },
  ],
  profiles: [{ id: "core", display_name: "Core", default: true, components: ["core"] }],
  coding_environments: [
    { id: "claude_code", display_name: "Claude Code", description: null, default_primary: true },
    { id: "codex", display_name: "Codex", description: null, default_primary: false },
  ],
  unavailable_coding_environments: [],
};

/** A preview with the Integrations catalogs of manifest format 6. */
const integrationsPreview: SourcePreview = {
  ...preview,
  components: [
    ...preview.components,
    { id: "workflow.portraits.core", display_name: "Portrait contract", description: null, category: "workflow", optional: true, coding_environment: null, platforms: ["all"], dependencies: [], file_count: 1, total_size: 1 },
  ],
  image_generation: {
    default_provider: "none",
    providers: [
      { id: "none", display_name: "No image provider", route_kind: "manual", supported_coding_environments: ["claude_code", "codex"] },
      { id: "openai_api", display_name: "OpenAI Images", route_kind: "stdio_adapter", auth_method: "api_key_vault", supported_models: ["gpt-image-2"], supported_coding_environments: ["claude_code", "codex"], requirements: ["An OpenAI API key saved in the operating-system credential store."] },
      { id: "codex_native", display_name: "Codex image generation", route_kind: "native", requires_primary_coding_environment: "codex", supported_coding_environments: ["codex"] },
    ],
  },
  live_testing: { providers: [{ id: "clausewitz_mcp", display_name: "Clausewitz MCP", description: "Direct state reads from native Linux HOI4.", requirements: ["Linux native HOI4 1.19.2 only"] }] },
};

const existingSetup: ExistingSetup = {
  valid: true,
  project_id: "demo_mod",
  installed_at: "2026-10-01T00:00:00Z",
  updated_at: null,
  source: { mode: "pinned_commit", revision: COMMIT, requested_ref: COMMIT },
  primary_coding_environment: "codex",
  additional_coding_environments: ["claude_code"],
  components: ["core", "workflow.super_events"],
  image_generation: null,
  live_testing: null,
  portrait_pipeline: null,
  file_count: 42,
};

type Answers = { [Op in Operation]: (args: CoreOperations[Op]["args"]) => CoreOperations[Op]["result"] };

/** A fake core that answers like the WebAssembly worker. */
function fakeCore(overrides: Partial<Answers> = {}) {
  const calls: Array<{ op: Operation; args: unknown }> = [];
  const answers: Answers = {
    reset: () => null,
    addFiles: (args: CoreOperations["addFiles"]["args"]) => args.files.length,
    scan: () => ({ findings: [], detected: [{ key: "primary_namespace", value: "demo", reason: "events" }], descriptor: { name: "Demo Mod" }, stats: { files_indexed: 2, directories_indexed: 1, samples_read: 2, bytes_read: 10, excluded_entries: 0, coverage: "complete" }, issues: [] }),
    openSource: () => null,
    preview: () => preview,
    select: () => ({ components: ["core"], unsupported: [], files: [{ component_id: "core", source_path: "AGENTS.md", destination: "AGENTS.md", expected_sha256: "a".repeat(64), expected_size: 1 }] }),
    download: () => 1,
    export: () => [
      { path: "AGENTS.md", component_id: "core", action: "create", incoming_sha256: "a".repeat(64), local_sha256: null, size: 1 },
      { path: "README.md", component_id: "project.readme", action: "conflict", incoming_sha256: "b".repeat(64), local_sha256: "c".repeat(64), size: 2 },
    ],
    exportRemoval: () => [
      { path: ".claude/skills/demo/SKILL.md", component_id: "core.skills", action: "remove", incoming_sha256: "a".repeat(64), local_sha256: "a".repeat(64), size: 1 },
      { path: "AGENTS.md", component_id: "core.agents", action: "release", incoming_sha256: "a".repeat(64), local_sha256: "b".repeat(64), size: 1 },
    ],
    compare: (args: CoreOperations["compare"]["args"]) => ({ path: args.path, incoming: "incoming text", local: "my text", binary: false }),
    fetchBases: () => ({ loaded: [], failed: [] }),
    merge: () => "merged text",
    readiness: () => [
      { id: "descriptor", label: "Descriptor", state: "pass", detail: "descriptor.mod names the mod for the launcher." },
      { id: "mcp.hoi4_agent_tools", label: "HOI4 Agent Tools", state: "action", detail: "Install the tools once." },
    ],
    launcher: () => "name=\"Demo Mod\"\npath=\"mod/demo_mod\"\n",
    zip: () => new ArrayBuffer(4),
    evidence: () => [],
    registry: () => ({ source_revision: COMMIT, manifest_sha256: "d".repeat(64), component_ids: ["workflow.super_events"] }),
    contextDocument: (args: CoreOperations["contextDocument"]["args"]) => ({ id: `doc-${args.number}`, name: args.name, size: 12, reference: `context-${args.number}`, evidence_path: `@context/${args.number}-${args.name.toLowerCase()}`, excerpt: "Lore notes.", excerpt_sha256: "f".repeat(64), truncated: false, redacted: false, copyable: true }),
    assistantTurn: () => ({ inputSha256: "e".repeat(64), headers: [["content-type", "application/json"]], body: { model: "claude-haiku-5-5" } }),
    assistantReply: () => ({
      analysis: {
        analysis_id: "f2a1c1de-0000-4000-8000-000000000000",
        project_summary: "A grand strategy overhaul of the Balkans.",
        proposals: [
          { key: "display_name", value: "Iron Dawn", confidence: 0.9, reason: "", evidence_refs: [] },
          { key: "project_id", value: "suggested_id", confidence: 0.9, reason: "", evidence_refs: [] },
          { key: "primary_namespace", value: "suggested_ns", confidence: 0.9, reason: "", evidence_refs: [] },
          { key: "localisation_convention", value: "English and German YAML with BOM.", confidence: 0.9, reason: "", evidence_refs: [] },
          { key: "project_description", value: "Balkan nations rise in 1936.", confidence: 0.8, reason: "", evidence_refs: [] },
          { key: "descriptor_tags", value: ["Events", "Not a tag"], confidence: 0.8, reason: "", evidence_refs: [] },
        ],
        component_recommendations: [{ component_id: "workflow.super_events", recommendation: "recommended", reason: "Story events." }],
        warnings: [],
      },
    }),
    ...overrides,
  } as never;
  const worker = {
    onmessage: null as ((event: MessageEvent<WorkerMessage>) => void) | null,
    postMessage(request: { id: number; op: Operation; args: never }) {
      calls.push({ op: request.op, args: request.args });
      queueMicrotask(() => {
        try {
          const result = (answers[request.op] as (args: never) => unknown)(request.args);
          worker.onmessage?.({ data: { kind: "result", id: request.id, ok: true, result } } as MessageEvent<WorkerMessage>);
        } catch (error) {
          worker.onmessage?.({ data: { kind: "result", id: request.id, ok: false, error: (error as Error).message } } as MessageEvent<WorkerMessage>);
        }
      });
    },
    terminate: vi.fn(),
  };
  return { client: createCoreClient(worker) as CoreClient, calls };
}

function stubGitHub(provider: (init: RequestInit) => Response = () => new Response("{}", { status: 200 })) {
  const providerCalls: RequestInit[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
    if (url.startsWith("https://api.anthropic.com/")) {
      providerCalls.push(init!);
      return provider(init!);
    }
    if (url.endsWith("/Agentic-HOI4-Modding")) return new Response(JSON.stringify({ default_branch: "main" }), { status: 200 });
    if (url.includes("/commits/")) return new Response(JSON.stringify({ sha: COMMIT }), { status: 200 });
    if (url.includes("raw.githubusercontent.com")) return new Response("{}", { status: 200 });
    return new Response("not found", { status: 404 });
  }));
  return providerCalls;
}

function stubDownloads() {
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:zip"), revokeObjectURL: vi.fn() }));
  return vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
}

/** Fill in the setup assistant on the Project step. */
function chooseAssistant(key = "sk-ant-test") {
  fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });
  fireEvent.change(screen.getByLabelText("API key"), { target: { value: key } });
}

/** A new mod through the assistant up to the reviewed details. */
async function describeNewMod() {
  fireEvent.click(screen.getByRole("button", { name: /New mod/ }));
  chooseAssistant();
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  expect(screen.getByRole("heading", { name: "Describe the mod" })).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Mod name"), { target: { value: "Iron Dawn" } });
  fireEvent.change(screen.getByLabelText("What the mod is about"), { target: { value: "Balkan nations rise in 1936 through events and decisions." } });
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  expect(await screen.findByRole("heading", { name: "Review the details" })).toBeInTheDocument();
}

function pickFolder(container: HTMLElement) {
  const file = new File(["name=\"Demo Mod\""], "descriptor.mod");
  Object.defineProperty(file, "webkitRelativePath", { value: "demo_mod/descriptor.mod" });
  fireEvent.change(container.querySelector<HTMLInputElement>("input[type=file]")!, { target: { files: [file] } });
}

function scanWithSetup(): CoreOperations["scan"]["result"] {
  return { findings: [], detected: [], descriptor: { name: "Demo Mod" }, stats: null, issues: [], existing_setup: existingSetup };
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("web edition", () => {
  it("needs a setup assistant before the setup starts", () => {
    stubGitHub();
    const { client } = fakeCore();
    render(<WebApp client={client} />);
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: /New mod/ }));
    expect(screen.getByRole("button", { name: /New mod/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByLabelText("AI provider")).toHaveValue(BROWSER_PROVIDERS[0].id);
    if (FREE_PROVIDER) {
      // The free route needs no account or key.
      expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
      expect(screen.queryByLabelText("API key")).toBeNull();
    } else {
      expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "Continue" })).toHaveAttribute("title", "Sign in with OpenRouter or enter an API key.");
      expect(screen.getByRole("button", { name: "Sign in with OpenRouter" })).toBeInTheDocument();
    }
    fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Continue" })).toHaveAttribute("title", "Enter your Claude API key.");
    expect(screen.getByLabelText("Model")).toHaveValue("claude-haiku-5-5");
    chooseAssistant();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "bad model" } });
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("expands a rough brief with the magic wand and can undo it", async () => {
    const providerCalls = stubGitHub(() => new Response('{"content":[]}', { status: 200 }));
    const { client, calls } = fakeCore({
      descriptionTurn: () => ({ headers: [["content-type", "application/json"]], body: { model: "claude-haiku-5-5" } }),
      descriptionReply: () => ({ description: "A thorough Balkans mod set in 1936." }),
    });
    render(<WebApp client={client} />);
    fireEvent.click(screen.getByRole("button", { name: /New mod/ }));
    chooseAssistant();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Describe the mod" })).toBeInTheDocument();
    const brief = screen.getByLabelText("What the mod is about");
    expect(screen.getByRole("button", { name: "Expand description" })).toBeDisabled();
    fireEvent.change(brief, { target: { value: "balkans 1936" } });
    fireEvent.click(screen.getByRole("button", { name: "Expand description" }));
    await waitFor(() => expect(brief).toHaveValue("A thorough Balkans mod set in 1936."));
    expect(screen.getByText(/Suggested by Claude/)).toBeInTheDocument();
    expect(providerCalls).toHaveLength(1);
    expect(calls.find((call) => call.op === "descriptionTurn")?.args).toMatchObject({ note: "balkans 1936" });
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(brief).toHaveValue("balkans 1936");
  });

  it("keeps the brief when the wand's draft is refused", async () => {
    stubGitHub(() => new Response('{"content":[]}', { status: 200 }));
    const { client } = fakeCore({
      descriptionTurn: () => ({ headers: [], body: {} }),
      descriptionReply: () => ({ error: "The draft attributes text to an AI tool" }),
    });
    render(<WebApp client={client} />);
    fireEvent.click(screen.getByRole("button", { name: /New mod/ }));
    chooseAssistant();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const brief = screen.getByLabelText("What the mod is about");
    fireEvent.change(brief, { target: { value: "balkans 1936" } });
    fireEvent.click(screen.getByRole("button", { name: "Expand description" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("not usable");
    expect(brief).toHaveValue("balkans 1936");
  });

  it("prepares a new mod with the assistant and downloads a ZIP without uploading files", async () => {
    const providerCalls = stubGitHub();
    const click = stubDownloads();
    const { client, calls } = fakeCore();
    render(<WebApp client={client} />);
    await describeNewMod();

    // Every value comes from the assistant; the user only reviews.
    expect(screen.getByLabelText(/Mod name/)).toHaveValue("Iron Dawn");
    expect(screen.getByLabelText(/Mod ID/)).toHaveValue("suggested_id");
    expect(screen.getByLabelText(/Event namespace/)).toHaveValue("suggested_ns");
    expect(screen.getByLabelText(/^Description/)).toHaveValue("Balkan nations rise in 1936.");
    expect(screen.getByRole("checkbox", { name: "Events" })).toBeChecked();
    expect(screen.getByText("A grand strategy overhaul of the Balkans.")).toBeInTheDocument();
    expect(providerCalls).toHaveLength(1);
    expect((providerCalls[0].headers as Headers).get("x-api-key")).toBe("sk-ant-test");
    const turn = calls.find((call) => call.op === "assistantTurn")!.args as CoreOperations["assistantTurn"]["args"];
    expect(turn.request.brief).toBe("Balkan nations rise in 1936 through events and decisions.");
    expect(turn.request.constraints).toMatchObject({ requested_mod_name: "Iron Dawn", component_registry: { component_ids: ["workflow.super_events"] } });

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Choose components" })).toBeInTheDocument();
    expect(screen.getByLabelText("Primary")).toHaveValue("claude_code");
    // The assistant's recommendation is preselected.
    expect(screen.getByRole("checkbox", { name: /Super Events workflow/ })).toBeChecked();
    expect(screen.getByText(/Recommended by Claude/)).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /3D models workflow/ })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Download and review" }));

    expect(await screen.findByRole("heading", { name: "Review the changes" })).toBeInTheDocument();
    const exported = calls.find((call) => call.op === "export")!.args as CoreOperations["export"]["args"];
    expect(exported.existing).toBe(false);
    expect(exported.request.identity).toMatchObject({ displayName: "Iron Dawn", projectId: "suggested_id" });
    expect(exported.request.assistant).toMatchObject({ provider: "claude", endpoint: "https://api.anthropic.com/v1/messages" });
    const select = calls.find((call) => call.op === "select")!.args as CoreOperations["select"]["args"];
    expect(select.request.selectedComponents).toContain("workflow.super_events");
    expect(select.request.selectedComponents).not.toContain("workflow.3d");

    fireEvent.click(screen.getByRole("button", { name: "Download ZIP" }));
    expect(await screen.findByRole("heading", { name: "Your ZIP is ready" })).toBeInTheDocument();
    expect(click).toHaveBeenCalled();
    const zip = calls.find((call) => call.op === "zip")!.args as CoreOperations["zip"]["args"];
    expect(zip.decisions).toEqual({ "README.md": "keep" });
    // The checks the desktop's Ready screen would show.
    expect(screen.getByText("HOI4 Agent Tools")).toBeInTheDocument();
    expect(screen.getAllByText("To do").length).toBeGreaterThan(0);
    const hosts = (vi.mocked(fetch).mock.calls as unknown as Array<[string]>).map(([url]) => new URL(url).host);
    expect(new Set(hosts)).toEqual(new Set(["api.github.com", "raw.githubusercontent.com", "api.anthropic.com"]));
    expect(JSON.stringify(calls)).not.toContain("sk-ant-test");
  });

  it("imports a folder, keeps what the folder decides, and lets the user choose per conflict", async () => {
    stubGitHub();
    const { client, calls } = fakeCore({
      scan: () => ({ findings: [], detected: [{ key: "display_name", value: "Demo Mod", reason: "descriptor" }, { key: "primary_namespace", value: "demo", reason: "events" }], descriptor: { name: "Demo Mod" }, stats: { files_indexed: 2, directories_indexed: 1, samples_read: 2, bytes_read: 10, excluded_entries: 0, coverage: "complete" }, issues: [] }),
      evidence: () => [{ reference: "profile.detected", path: "@scan/profile/detected", excerpt: "[]", excerpt_sha256: "0".repeat(64), confidence: 1 }],
    });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    expect(await screen.findByText("demo_mod")).toBeInTheDocument();
    expect(screen.getByText(/primary namespace/)).toBeInTheDocument();
    chooseAssistant();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("heading", { name: "Review the details" })).toBeInTheDocument();
    // An existing mod is described by its folder: the rail has no Describe step.
    expect(within(screen.getByRole("navigation", { name: "Setup steps" })).queryByText("Describe")).toBeNull();
    expect(screen.getByLabelText(/Mod name/)).toHaveValue("Demo Mod");
    expect(screen.getByLabelText(/Event namespace/)).toHaveValue("demo");
    expect(screen.getByLabelText(/Mod ID/)).toHaveValue("demo_mod");
    expect(screen.getByLabelText(/^Description/)).toHaveValue("Balkan nations rise in 1936.");
    expect(screen.getByText("English and German YAML with BOM.")).toBeInTheDocument();
    expect(screen.getByText(/What Claude received \(1 scan findings\)/)).toBeInTheDocument();
    const turn = calls.find((call) => call.op === "assistantTurn")!.args as CoreOperations["assistantTurn"]["args"];
    expect(turn.request.mode).toBe("existing_project_semantics");
    expect(turn.request.evidence.map((item) => item.reference)).toEqual(["profile.detected"]);

    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("heading", { name: "Prepare a mod in your browser" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Review the details" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Download and review" }));
    expect(await screen.findByRole("button", { name: "Compare" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    expect(await screen.findByText("my text")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Use the setup's version" }));
    fireEvent.click(screen.getByRole("radio", { name: /A full copy/ }));

    stubDownloads();
    fireEvent.click(screen.getByRole("button", { name: "Download ZIP" }));
    await screen.findByRole("heading", { name: "Your ZIP is ready" });
    expect(calls.find((call) => call.op === "export")!.args).toMatchObject({ existing: true, request: { identity: { projectId: "demo_mod" } } });
    expect(calls.find((call) => call.op === "zip")!.args).toEqual({ decisions: { "README.md": "replace" }, layout: "full_copy" });
    // An existing mod can get its launcher file.
    fireEvent.click(screen.getByRole("button", { name: "Download launcher file" }));
    await waitFor(() => expect(calls.some((call) => call.op === "launcher")).toBe(true));
    expect(calls.find((call) => call.op === "launcher")!.args).toEqual({ folder: "demo_mod" });
  });

  it("leaves out excluded findings and sends included documents as context", async () => {
    stubGitHub();
    const { client, calls } = fakeCore({
      scan: () => ({ findings: [{ id: "profile.structure", key: "project_structure", value: "Events and decisions", status: "accepted" }, { id: "descriptor.name", key: "mod_name", value: "Demo Mod", status: "accepted" }], detected: [], descriptor: { name: "Demo Mod" }, stats: null, issues: [] }),
      evidence: (args) => [{ reference: "descriptor.name", path: "descriptor.mod", excerpt: "Demo Mod", excerpt_sha256: "0".repeat(64), confidence: 1 }].filter((item) => !(args.excluded ?? []).includes(item.reference)),
    });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    await screen.findByText("demo_mod");
    fireEvent.click(screen.getByRole("checkbox", { name: /Mod name/ }));
    const documents = container.querySelector<HTMLInputElement>("input[accept*='.md']")!;
    fireEvent.change(documents, { target: { files: [new File(["Lore notes."], "Lore.md")] } });
    expect(await screen.findByText("Lore.md")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: /Keep a copy/ }));
    chooseAssistant();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Review the details" });
    const evidence = calls.find((call) => call.op === "evidence")!.args as CoreOperations["evidence"]["args"];
    expect(evidence.excluded).toEqual(["descriptor.name"]);
    const turn = calls.find((call) => call.op === "assistantTurn")!.args as CoreOperations["assistantTurn"]["args"];
    expect(turn.request.evidence.map((item) => item.reference)).toEqual(["context-1"]);
    expect(turn.request.evidence[0].path).toBe("@context/1-lore.md");

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Download and review" }));
    await screen.findByRole("heading", { name: "Review the changes" });
    const exported = calls.find((call) => call.op === "export")!.args as CoreOperations["export"]["args"];
    expect(exported.request.contextDocuments).toEqual([{ name: "Lore.md", data: btoa("Lore notes.") }]);
  });

  it("repairs an existing setup without the assistant, using the recorded choices", async () => {
    stubGitHub();
    const { client, calls } = fakeCore({ scan: scanWithSetup });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    expect(await screen.findByRole("heading", { name: "Existing setup found" })).toBeInTheDocument();
    // Maintenance keeps the recorded identity and needs no API key.
    expect(screen.queryByLabelText("API key")).toBeNull();
    expect(screen.getByRole("radio", { name: /^Update/ })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: /^Repair/ }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Review the changes" })).toBeInTheDocument();
    const select = calls.find((call) => call.op === "select")!.args as CoreOperations["select"]["args"];
    expect(select.request).toMatchObject({ primaryCodingEnvironment: "codex", additionalCodingEnvironments: ["claude_code"] });
    expect(select.request.selectedComponents).toContain("workflow.super_events");
    const exported = calls.find((call) => call.op === "export")!.args as CoreOperations["export"]["args"];
    expect(exported.request.maintenance).toBe("repair");
    expect(exported.request.identity).toBeUndefined();
    const opened = calls.find((call) => call.op === "openSource")!.args as CoreOperations["openSource"]["args"];
    expect(opened).toMatchObject({ revision: COMMIT, mode: "pinned_commit" });
    expect(calls.some((call) => call.op === "assistantTurn")).toBe(false);
  });

  it("removes a setup by listing untouched files for deletion", async () => {
    stubGitHub();
    const { client, calls } = fakeCore({ scan: scanWithSetup });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    await screen.findByRole("heading", { name: "Existing setup found" });
    fireEvent.click(screen.getByRole("radio", { name: /Remove the setup/ }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Review the changes" })).toBeInTheDocument();
    expect(screen.getByText(/To delete \(1\)/)).toBeInTheDocument();
    expect(screen.getByText(/Left alone \(1\)/)).toBeInTheDocument();
    stubDownloads();
    fireEvent.click(screen.getByRole("button", { name: "Download ZIP" }));
    await screen.findByRole("heading", { name: "Your ZIP is ready" });
    expect(calls.some((call) => call.op === "exportRemoval")).toBe(true);
    expect(calls.some((call) => call.op === "select")).toBe(false);
  });

  it("leaves setups with desktop-only workflows to the desktop app, except removal", async () => {
    stubGitHub();
    const { client } = fakeCore({ scan: () => ({ ...scanWithSetup(), existing_setup: { ...existingSetup, desktop_only: "This setup uses the 3D models workflow, which only the desktop app can maintain." } }) });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    await screen.findByRole("heading", { name: "Existing setup found" });
    expect(screen.getByText(/only the desktop app can maintain/)).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /^Update/ })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /^Set up again/ })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /Remove the setup/ })).toBeChecked();
  });

  it("previews a three-way merge before it can be chosen", async () => {
    stubGitHub();
    const { client, calls } = fakeCore({
      scan: scanWithSetup,
      export: () => [{ path: "CLAUDE.md", component_id: "core.claude.instructions", action: "conflict", incoming_sha256: "b".repeat(64), local_sha256: "c".repeat(64), size: 2, mergeable: true }],
      compare: (args) => ({ path: args.path, incoming: "incoming text", local: "my text", base: "base text", merged: "merged text", binary: false }),
    });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    await screen.findByRole("heading", { name: "Existing setup found" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Choose components" })).toBeInTheDocument();
    expect(screen.getByLabelText("Primary")).toHaveValue("codex");
    fireEvent.click(screen.getByRole("button", { name: "Download and review" }));
    await screen.findByRole("heading", { name: "Review the changes" });
    expect(calls.some((call) => call.op === "fetchBases")).toBe(true);
    expect(screen.getByRole("radio", { name: "Merge" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Preview merge" }));
    expect(await screen.findByText("merged text")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Merge" }));
    stubDownloads();
    fireEvent.click(screen.getByRole("button", { name: "Download ZIP" }));
    await screen.findByRole("heading", { name: "Your ZIP is ready" });
    expect(calls.find((call) => call.op === "zip")!.args).toMatchObject({ decisions: { "CLAUDE.md": "merge" } });
    const exported = calls.find((call) => call.op === "export")!.args as CoreOperations["export"]["args"];
    expect(exported.request.maintenance).toBe("update");
  });

  /** An existing mod through the assistant to the change review. */
  async function existingToReview(overrides: Parameters<typeof fakeCore>[0]) {
    stubGitHub();
    const core = fakeCore({ evidence: () => [{ reference: "profile.detected", path: "@scan/profile/detected", excerpt: "[]", excerpt_sha256: "0".repeat(64), confidence: 1 }], ...overrides });
    const rendered = render(<WebApp client={core.client} />);
    pickFolder(rendered.container);
    await screen.findByText("demo_mod");
    chooseAssistant();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Review the details" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Download and review" }));
    await screen.findByRole("heading", { name: "Review the changes" });
    return core;
  }

  const countItem = (text: string) => screen.getByText((_, element) => element?.tagName === "LI" && element.textContent === text);
  const entry = (path: string, component_id: string, action: CoreOperations["export"]["result"][number]["action"]) => ({ path, component_id, action, incoming_sha256: "b".repeat(64), local_sha256: action === "create" ? null : "c".repeat(64), size: 2 });

  it("keeps the mod's own descriptor and README without asking, and still asks about anything else", async () => {
    const { calls } = await existingToReview({
      export: () => [
        entry("AGENTS.md", "core.agents", "create"),
        entry("descriptor.mod", "project.descriptor", "conflict"),
        entry("README.md", "project.readme", "conflict"),
        entry("thumbnail.png", "project.thumbnail", "conflict"),
      ],
    });
    // Only the thumbnail the user chose needs a decision.
    expect(countItem("1 need a decision")).toBeInTheDocument();
    expect(countItem("2 of your own files kept")).toBeInTheDocument();
    expect(screen.getByText("Your own files, kept as they are (2)")).toBeInTheDocument();
    expect(screen.getByText(/The setup's version is a new descriptor/)).toBeInTheDocument();
    // Every file defaults to keeping the user's version.
    expect(screen.getAllByRole("radio", { name: "Keep mine" }).every((radio) => (radio as HTMLInputElement).checked)).toBe(true);
    stubDownloads();
    fireEvent.click(screen.getByRole("button", { name: "Download ZIP" }));
    await screen.findByRole("heading", { name: "Your ZIP is ready" });
    expect(calls.find((call) => call.op === "zip")!.args).toMatchObject({ decisions: { "descriptor.mod": "keep", "README.md": "keep", "thumbnail.png": "keep" } });
  });

  it("asks about an unreadable descriptor and carries the mod's own version into the one it offers", async () => {
    const { calls } = await existingToReview({
      scan: () => ({ findings: [], detected: [{ key: "primary_namespace", value: "demo", reason: "events" }], descriptor: null, stats: null, issues: [] }),
      export: () => [entry("descriptor.mod", "project.descriptor", "conflict")],
    });
    expect(countItem("1 need a decision")).toBeInTheDocument();
    expect(screen.queryByText(/of your own files kept/)).toBeNull();
    expect((calls.find((call) => call.op === "export")!.args as CoreOperations["export"]["args"]).request.identity).not.toHaveProperty("version");
  });

  it("offers the descriptor's version and game version, not new defaults", async () => {
    const { calls } = await existingToReview({
      scan: () => ({ findings: [], detected: [{ key: "primary_namespace", value: "demo", reason: "events" }], descriptor: { name: "Demo Mod", version: "1.2", supported_version: "1.12.*" }, stats: null, issues: [] }),
      export: () => [entry("AGENTS.md", "core.agents", "create")],
    });
    const request = (calls.find((call) => call.op === "export")!.args as CoreOperations["export"]["args"]).request;
    expect(request.identity).toMatchObject({ version: "1.2", supportedGameVersion: "1.12.*" });
  });

  it("explains a merge that cannot be done in plain words", async () => {
    stubGitHub();
    const { client } = fakeCore({
      scan: scanWithSetup,
      export: () => [{ ...entry("CLAUDE.md", "core.claude.instructions", "conflict"), mergeable: true }],
      merge: () => { throw new Error("merge error: non-overlapping line merge could not be proven safe"); },
    });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    await screen.findByRole("heading", { name: "Existing setup found" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Choose components" });
    fireEvent.click(screen.getByRole("button", { name: "Download and review" }));
    await screen.findByRole("heading", { name: "Review the changes" });
    fireEvent.click(screen.getByRole("button", { name: "Preview merge" }));
    expect(await screen.findByText(/too close together to combine safely/)).toBeInTheDocument();
    expect(screen.queryByText(/proven safe/)).toBeNull();
  });

  it("names the file that could not be read when a folder is imported", async () => {
    stubGitHub();
    const { client } = fakeCore();
    const { container } = render(<WebApp client={client} />);
    const file = new File(["x"], "locked.txt");
    Object.defineProperty(file, "webkitRelativePath", { value: "demo_mod/common/locked.txt" });
    Object.defineProperty(file, "arrayBuffer", { value: () => Promise.reject(new Error("The requested file could not be read.")) });
    fireEvent.change(container.querySelector<HTMLInputElement>("input[type=file]")!, { target: { files: [file] } });
    expect(await screen.findByRole("alert")).toHaveTextContent("common/locked.txt could not be read");
  });

  it("says when the dropped folder has no descriptor.mod", async () => {
    stubGitHub();
    const { client } = fakeCore({ scan: () => ({ findings: [], detected: [], descriptor: null, stats: null, issues: [] }) });
    const { container } = render(<WebApp client={client} />);
    pickFolder(container);
    expect(await screen.findByText(/could not be read. The setup offers a new one/)).toBeInTheDocument();
    cleanup();
    const other = render(<WebApp client={fakeCore({ scan: () => ({ findings: [], detected: [], descriptor: null, stats: null, issues: [] }) }).client} />);
    const file = new File(["x"], "events.txt");
    Object.defineProperty(file, "webkitRelativePath", { value: "mods/events/events.txt" });
    fireEvent.change(other.container.querySelector<HTMLInputElement>("input[type=file]")!, { target: { files: [file] } });
    expect(await screen.findByText(/no descriptor.mod at its top level/)).toBeInTheDocument();
  });

  it("warns before a reload discards work in progress, and not otherwise", async () => {
    stubGitHub();
    const { client } = fakeCore();
    const { container } = render(<WebApp client={client} />);
    const leave = () => {
      const event = new Event("beforeunload", { cancelable: true });
      window.dispatchEvent(event);
      return event.defaultPrevented;
    };
    expect(leave()).toBe(false);
    pickFolder(container);
    await screen.findByText("demo_mod");
    expect(leave()).toBe(true);
  });

  it("starts the next mod from a clean page, keeping only the assistant choice", async () => {
    await existingToReview({ export: () => [entry("AGENTS.md", "core.agents", "create")] });
    stubDownloads();
    fireEvent.click(screen.getByRole("button", { name: "Download ZIP" }));
    await screen.findByRole("heading", { name: "Your ZIP is ready" });
    fireEvent.click(screen.getByRole("button", { name: "Prepare another mod" }));
    expect(screen.getByRole("heading", { name: "Prepare a mod in your browser" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Scan result" })).toBeNull();
    expect(screen.queryByText("demo_mod")).toBeNull();
    expect(screen.getByRole("button", { name: /New mod/ })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByLabelText("AI provider")).toHaveValue("claude");
  });

  it("chooses image, portrait, and live-testing integrations", async () => {
    stubGitHub();
    const { client, calls } = fakeCore({ preview: () => integrationsPreview });
    render(<WebApp client={client} />);
    await describeNewMod();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Choose components" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Choose integrations" })).toBeInTheDocument();
    // A provider for another primary client is not offered.
    expect(within(screen.getByLabelText("Main image provider")).queryByText("Codex image generation")).toBeNull();
    fireEvent.change(screen.getByLabelText("Main image provider"), { target: { value: "openai_api" } });
    expect(screen.getByText(/A browser cannot save it there/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "gpt-image-2" } });
    fireEvent.click(screen.getByRole("radio", { name: "Comfy Cloud" }));
    fireEvent.click(screen.getByRole("radio", { name: "Clausewitz MCP" }));
    expect(screen.getByRole("radio", { name: /HOI4 Live Tools/ })).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox", { name: /compatible Linux game/ }));
    fireEvent.click(screen.getByRole("button", { name: "Download and review" }));
    await screen.findByRole("heading", { name: "Review the changes" });
    const select = calls.find((call) => call.op === "select")!.args as CoreOperations["select"]["args"];
    expect(select.request.imageGeneration).toEqual({ primary_provider: "openai_api", additional_providers: [], route_settings: { openai_api: { model: "gpt-image-2" } } });
    expect(select.request.portrait).toMatchObject({ provider: "cloud" });
    expect(select.request.liveTesting).toMatchObject({ provider: "clausewitz_mcp", linux_ready: true });
  });

  it("refuses an unsafe local image server address before downloading", async () => {
    stubGitHub();
    const comfy: SourcePreview = { ...integrationsPreview, image_generation: { default_provider: "none", providers: [{ id: "none", display_name: "No image provider", route_kind: "manual" }, { id: "comfyui_local", display_name: "ComfyUI", route_kind: "stdio_adapter", api_protocol: "comfyui", auth_method: "local", supported_coding_environments: ["claude_code"] }] } };
    const { client, calls } = fakeCore({ preview: () => comfy });
    render(<WebApp client={client} />);
    await describeNewMod();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Choose components" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Choose integrations" });
    fireEvent.change(screen.getByLabelText("Main image provider"), { target: { value: "comfyui_local" } });
    fireEvent.change(screen.getByLabelText("Local server address"), { target: { value: "http://192.168.1.4:8188" } });
    expect(screen.getByRole("button", { name: "Download and review" })).toBeDisabled();
    expect(calls.some((call) => call.op === "select")).toBe(false);
  });

  it("asks again without overriding what the user changed", async () => {
    const providerCalls = stubGitHub();
    const { client } = fakeCore();
    render(<WebApp client={client} />);
    await describeNewMod();
    fireEvent.change(screen.getByLabelText(/Mod ID/), { target: { value: "my_id" } });
    fireEvent.click(screen.getByRole("button", { name: "Ask Claude again" }));
    await waitFor(() => expect(providerCalls).toHaveLength(2));
    await screen.findByRole("heading", { name: "Review the details" });
    expect(screen.getByLabelText(/Mod ID/)).toHaveValue("my_id");
    expect(screen.getByLabelText(/Event namespace/)).toHaveValue("suggested_ns");
  });

  it("keeps the user on the step when the assistant refuses the key", async () => {
    stubGitHub(() => new Response("{}", { status: 401 }));
    const { client } = fakeCore();
    render(<WebApp client={client} />);
    fireEvent.click(screen.getByRole("button", { name: /New mod/ }));
    chooseAssistant("wrong-key");
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByLabelText("Mod name"), { target: { value: "Iron Dawn" } });
    fireEvent.change(screen.getByLabelText("What the mod is about"), { target: { value: "Balkan nations rise." } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("did not accept the API key");
    expect(screen.getByRole("heading", { name: "Describe the mod" })).toBeInTheDocument();
    expect(screen.getByLabelText("What the mod is about")).toHaveValue("Balkan nations rise.");
  });

  it("shows a core error and keeps the user on the same step", async () => {
    stubGitHub();
    const { client } = fakeCore({ download: () => { throw new Error("SHA-256 mismatch for AGENTS.md"); } });
    render(<WebApp client={client} />);
    await describeNewMod();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Download and review" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("SHA-256 mismatch");
    await waitFor(() => expect(screen.getByRole("heading", { name: "Choose components" })).toBeInTheDocument());
  });
});
