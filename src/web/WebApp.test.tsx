import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import WebApp from "./WebApp";
import { createCoreClient } from "./core-client";
import type { CoreClient } from "./core-client";
import type { CoreOperations, Operation, WorkerMessage } from "./protocol";

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

/** A fake core that answers like the WebAssembly worker. */
function fakeCore(overrides: Partial<{ [Op in Operation]: (args: CoreOperations[Op]["args"]) => CoreOperations[Op]["result"] }> = {}) {
  const calls: Array<{ op: Operation; args: unknown }> = [];
  const answers: { [Op in Operation]: (args: CoreOperations[Op]["args"]) => CoreOperations[Op]["result"] } = {
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
    compare: (args: CoreOperations["compare"]["args"]) => ({ path: args.path, incoming: "incoming text", local: "my text", binary: false }),
    zip: () => new ArrayBuffer(4),
    evidence: () => [],
    registry: () => ({ source_revision: COMMIT, manifest_sha256: "d".repeat(64), component_ids: ["workflow.super_events"] }),
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

/** Fill in the setup assistant on the Project step. */
function chooseAssistant(key = "sk-ant-test") {
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
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Continue" })).toHaveAttribute("title", "Enter your Claude API key.");
    expect(screen.getByLabelText("AI provider")).toHaveValue("claude");
    expect(screen.getByLabelText("Model")).toHaveValue("claude-haiku-5-5");
    chooseAssistant();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "bad model" } });
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("prepares a new mod with the assistant and downloads a ZIP without uploading files", async () => {
    const providerCalls = stubGitHub();
    const createObjectURL = vi.fn(() => "blob:zip");
    vi.stubGlobal("URL", Object.assign(URL, { createObjectURL, revokeObjectURL: vi.fn() }));
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
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
    fireEvent.click(screen.getByRole("radio", { name: "Use incoming" }));
    fireEvent.click(screen.getByRole("radio", { name: /A full copy/ }));

    vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:zip"), revokeObjectURL: vi.fn() }));
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    fireEvent.click(screen.getByRole("button", { name: "Download ZIP" }));
    await screen.findByRole("heading", { name: "Your ZIP is ready" });
    expect(calls.find((call) => call.op === "export")!.args).toMatchObject({ existing: true, request: { identity: { projectId: "demo_mod" } } });
    expect(calls.find((call) => call.op === "zip")!.args).toEqual({ decisions: { "README.md": "replace" } });
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
