import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App, { analysisBrief, CodingEnvironments, Components, ContextAttachments, contextEvidence, Conflict, DryRun, Findings, Identity, ImageGenerationSetup, LiveTestingSetup, liveTestingSelectionValid, Mcp, Mesh, Ready, Scan, Update, UpdateSourceOptions, Welcome, Workflows, buildAnalysisConfirmationValues, dynamicMaintenanceOptionalComponentIds, estimatePlanPreparationProgress, estimateRemainingTime, estimateSemanticPlanningProgress, imageGenerationComponentIds, initialState, maintenanceReviewScreen, manifestComponentSupportsPlatform, normalizeCodingEnvironmentSelection, codingEnvironmentOptions, recoveryProgress } from "./App";
import { applyInstallationResult, approveInstallation, approveScanEvidence, buildInstallationPlanResult, buildMaintenancePlanResult, cancelClaudeLogin, cancelCodexLogin, checkForAppUpdate, confirmCodexAnalysis, findInterruptedTransaction, forgetContextAttachment, installAppUpdate, logoutClaudeResult, logoutCodexResult, openCodexLoginUrlResult, openExternalUrlResult, openInCodingEnvironment, pickContextDocuments, pickContextImages, pickProjectFolder, previewDescriptorsResult, previewInstallationConflict, previewSourceManifestResult, readAiAccount, readAiModels, readCodexAccount, readTransactionJournal, removeImageGenerationCredential, rollbackInstallationResult, runCodexAnalysisResult, scanProject, startClaudeLogin, startCodexLogin, storeImageGenerationCredential, suggestProjectPaths, waitForClaudeLoginResult, waitForCodexLoginResult } from "./lib/tauri";
import type { AiAccountStatus, AiProviderId, CodexAnalysisResult, ContextDocument, ContextImage, FolderSelection, InstallationPlan, ScanFinding, ScanProgress, SourceManifestPreview, WizardState } from "./types";
import { renderCoverPng, renderReferenceJpeg } from "./lib/images";
import { documentationFixture, isDocumentationScreenshot } from "./documentation-fixtures";

vi.mock("./lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("./lib/tauri")>("./lib/tauri");
  return { ...actual, applyInstallationResult: vi.fn(), approveInstallation: vi.fn(), approveScanEvidence: vi.fn(), buildInstallationPlanResult: vi.fn(), buildMaintenancePlanResult: vi.fn(), cancelClaudeLogin: vi.fn(), cancelCodexLogin: vi.fn(), checkForAppUpdate: vi.fn(), confirmCodexAnalysis: vi.fn(), findInterruptedTransaction: vi.fn(), forgetContextAttachment: vi.fn(), installAppUpdate: vi.fn(), logoutClaudeResult: vi.fn(), logoutCodexResult: vi.fn(), openCodexLoginUrlResult: vi.fn(), openExternalUrlResult: vi.fn(), openInCodex: vi.fn(), openInCodingEnvironment: vi.fn(), pickContextDocuments: vi.fn(), pickContextImages: vi.fn(), pickProjectFolder: vi.fn(), previewDescriptorsResult: vi.fn(), previewInstallationConflict: vi.fn(), previewSourceManifestResult: vi.fn(), readAiAccount: vi.fn(), readAiModels: vi.fn(), readCodexAccount: vi.fn(), readTransactionJournal: vi.fn(), removeImageGenerationCredential: vi.fn(), rollbackInstallationResult: vi.fn(), runCodexAnalysisResult: vi.fn(), scanProject: vi.fn(), startClaudeLogin: vi.fn(), startCodexLogin: vi.fn(), storeImageGenerationCredential: vi.fn(), suggestProjectPaths: vi.fn(), waitForClaudeLoginResult: vi.fn(), waitForCodexLoginResult: vi.fn() };
});

// jsdom has no canvas; the context image rendering is exercised in images.test.ts.
vi.mock("./lib/images", async () => {
  const actual = await vi.importActual<typeof import("./lib/images")>("./lib/images");
  return { ...actual, renderCoverPng: vi.fn(), renderReferenceJpeg: vi.fn() };
});

afterEach(() => {
  window.history.replaceState({}, "", "/");
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  delete window.__HOI4_DOCUMENTATION_STATE__;
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

beforeEach(() => {
  vi.mocked(readCodexAccount).mockResolvedValue({ available: false, authenticated: false, auth_mode: "none", usage_limited: false });
  vi.mocked(readAiModels).mockResolvedValue([]);
  vi.mocked(readAiAccount).mockImplementation(async (provider, model) => ({ available: true, authenticated: false, provider: provider as AiProviderId, model, auth_mode: provider === "claude_account" ? "claude_account" : "api_key", usage_limited: false, error: provider === "claude_account" ? "Sign in to Claude to continue." : "the selected provider credential is not available" }));
  vi.mocked(readTransactionJournal).mockResolvedValue(null);
  vi.mocked(rollbackInstallationResult).mockResolvedValue({ value: null, error: "Undo is unavailable." });
  vi.mocked(logoutCodexResult).mockResolvedValue({ value: null });
  vi.mocked(openCodexLoginUrlResult).mockResolvedValue({ value: undefined });
  vi.mocked(openExternalUrlResult).mockResolvedValue({ value: undefined });
  vi.mocked(cancelCodexLogin).mockResolvedValue(true);
  vi.mocked(cancelClaudeLogin).mockResolvedValue(true);
  vi.mocked(startClaudeLogin).mockResolvedValue({ value: "claude-login" });
  vi.mocked(waitForClaudeLoginResult).mockResolvedValue({ value: null, error: "Claude sign-in did not finish." });
  vi.mocked(logoutClaudeResult).mockResolvedValue({ value: undefined });
  vi.mocked(pickProjectFolder).mockResolvedValue(null);
  vi.mocked(previewDescriptorsResult).mockResolvedValue({ value: null });
  vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: null });
  vi.mocked(runCodexAnalysisResult).mockResolvedValue({ value: null });
  vi.mocked(previewInstallationConflict).mockResolvedValue(null);
  vi.mocked(approveScanEvidence).mockResolvedValue(true);
  vi.mocked(scanProject).mockResolvedValue(null);
  vi.mocked(buildMaintenancePlanResult).mockResolvedValue({ value: null, error: "Plan fixture stopped." });
  vi.mocked(startCodexLogin).mockResolvedValue(null);
  vi.mocked(suggestProjectPaths).mockResolvedValue(null);
  vi.mocked(waitForCodexLoginResult).mockResolvedValue({ value: null });
  vi.mocked(checkForAppUpdate).mockResolvedValue({ value: null });
  vi.mocked(installAppUpdate).mockResolvedValue({ value: undefined });
  vi.mocked(findInterruptedTransaction).mockResolvedValue(null);
  vi.mocked(approveInstallation).mockResolvedValue(true);
  vi.mocked(applyInstallationResult).mockResolvedValue({ value: null });
});

function readyState(): WizardState {
  return {
    identity: { projectRoot: "C:\\mods\\cold-war-curtain", displayName: "Cold War Curtain" },
    aiProvider: "codex",
    aiModel: "gpt-5.6-luna",
    aiReasoningEffort: "xhigh",
    readiness: { openInCodex: true, coreReady: true, blockingCheckIds: [], checks: [] },
    selectedComponents: [],
    meshSelected: false,
    primaryCodingEnvironment: "codex",
    additionalCodingEnvironments: [],
  } as unknown as WizardState;
}

function welcomeState(account: WizardState["codexAccount"], codexLogin?: WizardState["codexLogin"]): WizardState {
  return { mode: "new", aiProvider: "codex", aiModel: "gpt-5.6-luna", aiReasoningEffort: "xhigh", aiEndpoint: "", selectedComponents: ["codex.config", "mcp.hoi4_agent_tools", "workflow.3d"], meshSelected: true, codexAccount: account, codexLogin } as unknown as WizardState;
}

function selectCodexProvider() {
  fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "codex" } });
}

// Most wizard-flow tests exercise the Codex App Server path so its coverage is
// retained now that the Claude account route is the default.
async function renderAuthenticatedApp() {
  vi.mocked(readCodexAccount).mockResolvedValue({
    available: true,
    authenticated: true,
    auth_mode: "chatgpt",
    usage_limited: false,
  });
  render(<App />);
  selectCodexProvider();
  await waitFor(() => expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled());
}

function enableTauriRuntime() {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
}

function semanticAnalysisFixture() {
  return {
    schema_version: "1.0.0",
    analysis_id: "semantic-analysis",
    mode: "new_project_identity" as const,
    input_sha256: "a".repeat(64),
    project_summary: "A focused project summary.",
    proposals: [
      ["display_name", "Iron Dawn"],
      ["project_id", "iron_dawn"],
      ["script_prefix", "id"],
      ["primary_namespace", "id"],
      ["project_description", "A focused alternate-history project."],
      ["descriptor_tags", ["Alternative History", "Events"]],
      ["folder_profile", ["common", "events"]],
      ["agents_profile", "default"],
      ["localisation_convention", "english"],
      ["documentation_convention", "markdown"],
    ].map(([key, value]) => ({ key, value, confidence: 0.9, reason: `Reason for ${String(key)}.`, evidence_refs: ["brief:1"] })),
    component_recommendations: [{ component_id: "core.skills", recommendation: "recommended", reason: "The brief calls for repeated HOI4 workflow support." }],
    warnings: [],
  };
}

describe("HOI4 Mod Setup wizard", () => {
  it("starts new projects with the Atlantis Rising example", () => {
    expect(initialState.sourceMode).toBe("latest");
    expect(initialState.pinnedRef).toBe("");
    expect(initialState.identity.displayName).toBe("Atlantis Rising");
    expect(initialState.description).toBe("An Atlantis total conversion with a new Atlantic island, naval expansion, custom units, national focuses, and original mechanics.");
    expect(initialState.aiProvider).toBe("claude_account");
    expect(initialState.aiModel).toBe("claude-haiku-5-5");
    expect(initialState.aiReasoningEffort).toBe("medium");
    expect(initialState.identity.projectId).toBe("atlantis_rising");
    expect(initialState.primaryCodingEnvironment).toBe("claude_code");
    expect(initialState.additionalCodingEnvironments).toEqual([]);
  });

  it("never sends the Create example description as an imported project's brief", () => {
    const brief = analysisBrief("existing_project_semantics", initialState.description);
    expect(brief).not.toContain("Atlantis");
    expect(brief).toMatch(/approved scan evidence/);
    expect(analysisBrief("new_project_identity", "My own mod idea.")).toBe("My own mod idea.");
  });

  it("normalizes coding-environment choices to one primary and unique additional clients", () => {
    expect(normalizeCodingEnvironmentSelection("cursor", ["cursor", "codex", "cursor", "unknown"])).toEqual({
      primary: "cursor",
      additional: ["codex"],
    });
    expect(normalizeCodingEnvironmentSelection(undefined, undefined)).toEqual({ primary: "claude_code", additional: [] });
    const declared = codingEnvironmentOptions({ coding_environments: [{ id: "windsurf", display_name: "Windsurf" }, { id: "codex", display_name: "Codex", default_primary: true }] } as never);
    expect(declared.map((environment) => environment.id)).toEqual(["windsurf", "codex"]);
    expect(normalizeCodingEnvironmentSelection(undefined, ["windsurf"], declared)).toEqual({ primary: "codex", additional: ["windsurf"] });
  });

  it("lets every supported environment be primary and keeps the primary out of additional choices", () => {
    for (const environment of ["codex", "claude_code", "cursor", "qoder", "opencode"] as const) {
      const update = vi.fn();
      const state = { ...initialState, primaryCodingEnvironment: environment, additionalCodingEnvironments: [] };
      render(<CodingEnvironments state={state} update={update} />);
      expect(screen.getByRole("radio", { name: new RegExp(environment === "claude_code" ? "Claude Code" : environment, "i") })).toBeChecked();
      expect(screen.queryByRole("checkbox", { name: new RegExp(environment === "claude_code" ? "Claude Code" : environment, "i") })).not.toBeInTheDocument();
      cleanup();
    }
  });

  it("updates a controlled selection when the primary changes and removes it from additions", () => {
    function ControlledEnvironmentStep() {
      const [state, setState] = useState<WizardState>({ ...initialState, primaryCodingEnvironment: "codex", additionalCodingEnvironments: ["cursor", "qoder"] });
      return <CodingEnvironments state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledEnvironmentStep />);
    fireEvent.click(screen.getByRole("radio", { name: /^Cursor / }));
    expect(screen.getByRole("radio", { name: /^Cursor / })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Codex/ })).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /Cursor/ })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /Qoder/ })).toBeChecked();
  });

  it("keeps environment names, descriptions, and primary state in bounded layout cells", () => {
    const { container } = render(<CodingEnvironments state={initialState} update={vi.fn()} />);
    const group = screen.getByRole("radiogroup", { name: /primary coding environment/i });

    expect(group).toHaveClass("coding-environment-options");
    expect(container.querySelectorAll(".coding-environment-option")).toHaveLength(5);
    expect(container.querySelectorAll(".coding-environment-copy")).toHaveLength(5);
    expect(within(group).getByText("Codex").closest(".coding-environment-copy")).toHaveTextContent(/project instructions, skills, agents, and MCP/i);
    expect(container.querySelector(".coding-environment-option")).toHaveTextContent(/Claude Code/);
    expect(screen.getByRole("group", { name: /additional coding environments/i })).toContainElement(screen.getByRole("checkbox", { name: /Cursor/ }));
  });

  const formatSixPreview = {
    schema_version: "6.0.0",
    coding_environments: [
      { id: "claude_code", display_name: "Claude Code", default_primary: true, mcp_support: "project" },
      { id: "gemini_cli", display_name: "Gemini CLI", description: "GEMINI.md, .gemini agents, shared .agents skills, and MCP", mcp_support: "project", documentation_url: "https://geminicli.com/docs/tools/mcp-server/" },
      { id: "cline", display_name: "Cline", description: "AGENTS.md, a Cline rule, and Claude-format skills", mcp_support: "manual", documentation_url: "https://docs.cline.bot/mcp/configuring-mcp-servers" },
      { id: "aider", display_name: "Aider", description: "An .aider.conf.yml that reads AGENTS.md", mcp_support: "none" },
    ],
    unavailable_coding_environments: [{ id: "augment", display_name: "Augment (Auggie CLI)", reason: "This client is not supported on Windows." }],
  } as unknown as SourceManifestPreview;

  it("labels manual and absent MCP routes and lists clients this version cannot install", () => {
    const state = { ...initialState, manifestPreview: formatSixPreview, primaryCodingEnvironment: "claude_code", additionalCodingEnvironments: [] } as WizardState;
    render(<CodingEnvironments state={state} update={vi.fn()} />);
    const primary = screen.getByRole("radiogroup", { name: /primary coding environment/i });
    expect(within(primary).getAllByRole("radio")).toHaveLength(4);
    expect(within(primary).getByText("Gemini CLI").closest(".coding-environment-copy")).toHaveTextContent(/\.gemini agents/);
    expect(within(primary).getByText("Gemini CLI").closest(".coding-environment-copy")).not.toHaveTextContent(/set up in the client/);
    expect(within(primary).getByText("Cline").closest(".coding-environment-copy")).toHaveTextContent("MCP is set up in the client");
    expect(within(primary).getByText("Aider").closest(".coding-environment-copy")).toHaveTextContent("No MCP support");
    // Unavailable clients are visible behind disclosure but never selectable.
    expect(screen.queryByRole("radio", { name: /Augment/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /Augment/ })).not.toBeInTheDocument();
    const unavailable = screen.getByText("Not available (1)").closest("details")!;
    expect(unavailable).toHaveTextContent(/Augment \(Auggie CLI\).*not supported on Windows/);
  });

  it("keeps the development-client selection independent from the setup assistant", () => {
    for (const aiProvider of ["gemini_account", "grok_account", "claude_account", "codex"]) {
      const update = vi.fn();
      const state = { ...initialState, aiProvider, manifestPreview: formatSixPreview, primaryCodingEnvironment: "claude_code", additionalCodingEnvironments: [] } as unknown as WizardState;
      render(<CodingEnvironments state={state} update={update} />);
      expect(screen.getByRole("radio", { name: /^Claude Code/ })).toBeChecked();
      expect(screen.getByRole("radio", { name: /^Gemini CLI/ })).not.toBeChecked();
      expect(update).not.toHaveBeenCalled();
      fireEvent.click(screen.getByRole("radio", { name: /^Gemini CLI/ }));
      expect(update).toHaveBeenCalledWith(expect.objectContaining({ primaryCodingEnvironment: "gemini_cli", additionalCodingEnvironments: [] }));
      expect(update.mock.calls[0][0]).not.toHaveProperty("aiProvider");
      cleanup();
    }
  });

  it("shows a manual MCP to-do with the reviewed guide only for selected manual clients", () => {
    const state = { ...readyState(), manifestPreview: formatSixPreview, selectedComponents: ["mcp.hoi4_agent_tools"], primaryCodingEnvironment: "gemini_cli", additionalCodingEnvironments: ["cline", "aider"] } as WizardState;
    render(<Ready state={state} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.getByText(/To-do: add HOI4 Agent Tools in Cline's own MCP settings/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Cline MCP guide" })).toHaveAttribute("href", "https://docs.cline.bot/mcp/configuring-mcp-servers");
    expect(screen.queryByText(/in Gemini CLI's own MCP settings/)).not.toBeInTheDocument();
    expect(screen.queryByText(/in Aider's own MCP settings/)).not.toBeInTheDocument();
    cleanup();
    render(<Ready state={{ ...state, selectedComponents: [] }} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.queryByText(/To-do: add HOI4 Agent Tools/)).not.toBeInTheDocument();
  });

  it("shows the default image route and keeps the primary separate from optional routes", () => {
    window.history.replaceState({}, "", "/?screenshot=image-generation");
    const fixture = documentationFixture(readyState());
    function ControlledImageStep() {
      const [state, setState] = useState(fixture);
      return <ImageGenerationSetup state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }
    render(<ControlledImageStep />);
    expect(screen.getByRole("radio", { name: /ChatGPT subscription \(GPTImage\)/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /OpenAI Images API/ })).not.toBeChecked();
    expect(screen.getAllByLabelText("Add as an optional provider", { selector: "input" })[0]).toBeChecked();
    expect(screen.getByText(/No separate image API bill/)).toBeInTheDocument();
    expect(screen.getByLabelText("Model")).toHaveValue("gpt-image-2.5-sunburst");
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "gpt-image-2" } });
    expect(screen.getByLabelText("Model")).toHaveValue("gpt-image-2");
    fireEvent.click(screen.getByRole("radio", { name: /OpenAI Images API/ }));
    expect(screen.getByRole("radio", { name: /OpenAI Images API/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /ChatGPT subscription \(GPTImage\)/ })).not.toBeChecked();
  });

  it("keeps manual image providers usable with clients the provider table does not list", () => {
    window.history.replaceState({}, "", "/?screenshot=image-generation");
    const fixture = { ...documentationFixture(readyState()), primaryCodingEnvironment: "aider", additionalCodingEnvironments: [] as string[] };
    render(<ImageGenerationSetup state={fixture} update={vi.fn()} />);
    expect(screen.getByRole("radio", { name: /^None — I will make art myself/ })).toBeEnabled();
    expect(screen.getByRole("radio", { name: /ChatGPT subscription \(GPTImage\)/ })).toBeDisabled();
  });

  it("selects the manifest image components when the provider route changes", () => {
    window.history.replaceState({}, "", "/?screenshot=image-generation");
    const fixture = documentationFixture(readyState());
    function ControlledImageStep() {
      const [state, setState] = useState({ ...fixture, selectedComponents: [] as string[] });
      return <><ImageGenerationSetup state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} /><output aria-label="selected image components">{state.selectedComponents.join(",")}</output></>;
    }

    render(<ControlledImageStep />);
    expect(imageGenerationComponentIds(fixture.imageGeneration, fixture.manifestPreview)).toContain("imagegen.gptimage");
    expect(screen.getByLabelText("selected image components")).toHaveTextContent("imagegen.gptimage");
    fireEvent.click(screen.getByRole("radio", { name: /OpenAI Images API/ }));
    expect(screen.getByLabelText("selected image components")).toHaveTextContent("imagegen.runtime");
    expect(screen.getByLabelText("selected image components")).not.toHaveTextContent("imagegen.gptimage");
  });

  it("sends image API keys to the OS vault bridge and clears the password draft", async () => {
    window.history.replaceState({}, "", "/?screenshot=image-generation");
    const fixture = documentationFixture(readyState());
    vi.mocked(storeImageGenerationCredential).mockResolvedValue({ name: "IMAGE_GENERATION_API_KEY", provider: "windows_credential_manager", reference: "credential://image_generation_api_key/openai_api_api_key", provider_id: "openai_api_api_key" });
    function ControlledImageStep() {
      const [state, setState] = useState({ ...fixture, imageGenerationCredentialDrafts: {} });
      return <ImageGenerationSetup state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }
    render(<ControlledImageStep />);
    const input = screen.getByLabelText("OpenAI API key");
    fireEvent.change(input, { target: { value: "sk-test-image-key" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Store" })[0]);
    await waitFor(() => expect(storeImageGenerationCredential).toHaveBeenCalledWith("openai_api", "api_key", "sk-test-image-key"));
    expect(await screen.findByText(/credential stored on this computer/i)).toBeInTheDocument();
    expect(input).toHaveValue("");
  });

  it("provides sanitized development-only states for public screenshots", () => {
    const base = readyState();
    window.history.replaceState({}, "", "/?screenshot=ready");
    const fixture = documentationFixture(base);

    expect(isDocumentationScreenshot()).toBe(true);
    expect(fixture.screen).toBe("ready");
    expect(fixture.identity.displayName).toBe("Atlantis Rising");
    expect(fixture.codexAccount?.email).toBeUndefined();
    expect(fixture.meshKeyDraft).toBeFalsy();
    expect(fixture.manifestPreview?.components.find((item) => item.id === "mcp.hoi4_agent_tools")?.platforms).toEqual(["windows"]);
    expect(fixture.manifestPreview?.components.find((item) => item.id === "workflow.3d")?.platforms).toEqual(["windows"]);
  });

  it("provides a sanitized coding-environment visual regression state", () => {
    window.history.replaceState({}, "", "/?screenshot=environments");
    const fixture = documentationFixture(readyState());

    expect(fixture.screen).toBe("environments");
    expect(fixture.primaryCodingEnvironment).toBe("codex");
    expect(fixture.additionalCodingEnvironments).toEqual(["claude_code"]);
    expect(fixture.identity.projectRoot).toContain("C:\\Users\\Player\\");
  });

  it("provides a sanitized interrupted-installation screenshot state", () => {
    window.history.replaceState({}, "", "/?screenshot=recovery");
    const fixture = documentationFixture(readyState());

    expect(fixture.screen).toBe("recovery");
    expect(fixture.identity.projectRoot).toContain("C:\\Users\\Player\\");
    expect(fixture.transaction?.last_checkpoint).toBe("validation");
    expect(fixture.transaction?.operations).toHaveLength(1146);
    expect(fixture.transaction?.error?.message).not.toMatch(/klimp|token|secret/i);
  });

  it("ignores unknown documentation screenshot routes", () => {
    const base = readyState();
    window.history.replaceState({}, "", "/?screenshot=unknown");
    expect(isDocumentationScreenshot()).toBe(false);
    expect(documentationFixture(base)).toBe(base);
  });

  it("does not offer ChatGPT source packaging from findings or update", () => {
    const finding = {
      id: "skill.inventory",
      label: "Skills",
      value: JSON.stringify({ count: 1 }),
      confidence: 1,
      status: "accepted",
      evidence: "A direct project skill was detected.",
    } as ScanFinding;
    render(<Findings
      state={readyState()}
      findings={[finding]}
      selected={finding.id}
      setSelected={vi.fn()}
      setFindings={vi.fn()}
      onConfirmAnalysis={vi.fn().mockResolvedValue(undefined)}
      onManageExisting={vi.fn()}
    />);

    expect(screen.queryByRole("button", { name: /ChatGPT/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/ChatGPT/)).not.toBeInTheDocument();
    cleanup();

    render(<Update
      state={readyState()}
      update={vi.fn()}
      findings={[]}
      setFindings={vi.fn()}
      onMaintenance={vi.fn()}
      onStartMaintenance={vi.fn()}
      onReanalyze={vi.fn().mockResolvedValue(true)}
    />);

    expect(screen.queryByRole("button", { name: /ChatGPT/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/ChatGPT/)).not.toBeInTheDocument();
  });

  it("shows blocking core scan conflicts in the review surface", () => {
    const conflict = {
      id: "conflict.git.inspection",
      category: "conflict",
      label: "Blocking conflict · unsafe git configuration",
      value: "Git configuration changed during inspection.",
      evidenceExcerpt: "Git configuration changed during inspection.",
      confidence: 1,
      status: "blocking",
      evidence: ".git - Git configuration changed during inspection.",
      evidencePath: ".git",
      origin: "deterministic",
      recommendation: "Resolve this blocking conflict before planning.",
    } as ScanFinding;
    render(<Findings
      state={readyState()}
      findings={[conflict]}
      selected={conflict.id}
      setSelected={vi.fn()}
      setFindings={vi.fn()}
      onConfirmAnalysis={vi.fn().mockResolvedValue(undefined)}
      onManageExisting={vi.fn()}
    />);

    expect(screen.getByText("Blocking conflict · unsafe git configuration")).toBeInTheDocument();
    expect(screen.getAllByText("Git configuration changed during inspection.").length).toBeGreaterThan(0);
  });

  it("keeps semantic planning estimates bounded within each real stage", () => {
    const startedAt = Date.parse("2026-08-02T12:00:00Z");
    expect(estimateSemanticPlanningProgress("preparing", startedAt, startedAt)).toEqual({ percent: 5, remaining: "About 2 minutes remaining" });
    expect(estimateSemanticPlanningProgress("analyzing", startedAt, startedAt + 45_000)).toEqual({ percent: 54, remaining: "About 50 seconds remaining" });
    expect(estimateSemanticPlanningProgress("validating", startedAt, startedAt + 120_000)).toEqual({ percent: 98, remaining: "Less than 10 seconds remaining" });
  });

  it("shows a bounded live estimate while preparing the installation review", () => {
    const startedAt = Date.parse("2026-08-02T12:00:00Z");
    expect(estimatePlanPreparationProgress(startedAt, startedAt)).toEqual({ percent: 4, remaining: "About 30 seconds remaining" });
    expect(estimatePlanPreparationProgress(startedAt, startedAt + 15_000)).toEqual({ percent: 50, remaining: "About 15 seconds remaining" });
    expect(estimatePlanPreparationProgress(startedAt, startedAt + 60_000).percent).toBe(96);
  });

  it("lets the returned update reanalysis be confirmed from the Update screen", async () => {
    const record = { analysis_id: "maintenance", confirmed_fields: [] } as unknown as WizardState["codexAnalysisRecord"];
    const onConfirmAnalysis = vi.fn().mockResolvedValue(undefined);
    render(<Update
      state={{
        ...initialState,
        aiProvider: "claude_account",
        maintenanceEvidenceReady: true,
        codexAnalysis: semanticAnalysisFixture() as unknown as WizardState["codexAnalysis"],
        codexAnalysisRecord: record,
        maintenanceCodexAnalysisRecord: record,
      } as WizardState}
      update={vi.fn()}
      findings={[]}
      setFindings={vi.fn()}
      onMaintenance={vi.fn()}
      onStartMaintenance={vi.fn()}
      onReanalyze={vi.fn().mockResolvedValue(true)}
      onConfirmAnalysis={onConfirmAnalysis}
    />);

    const review = screen.getByRole("region", { name: /proposal review/i });
    fireEvent.click(within(review).getByRole("button", { name: /^Confirm/ }));
    await waitFor(() => expect(onConfirmAnalysis).toHaveBeenCalledTimes(1));
  });

  it("opens every prepared maintenance plan in a visible review step", () => {
    expect(maintenanceReviewScreen({ conflicts: [] })).toBe("dry-run");
    expect(maintenanceReviewScreen({ conflicts: [{ selected: undefined }] as never })).toBe("conflict");
  });

  it("prevents duplicate maintenance analysis and discards a late response after retargeting", async () => {
    enableTauriRuntime();
    vi.mocked(readCodexAccount).mockResolvedValue({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false });
    window.__HOI4_DOCUMENTATION_STATE__ = {
      ...initialState, ...readyState(), identity: { ...initialState.identity, ...readyState().identity },
      screen: "update", projectPathsOverridden: true,
    };
    vi.mocked(scanProject).mockResolvedValue({
      findings: [{ id: "descriptor.name", category: "descriptor", label: "Name", value: "Example", status: "accepted", confidence: 1, evidencePath: "descriptor.mod", evidenceExcerpt: "name=\"Example\"" }],
      scanId: "update-scan", projectRoot: "C:\\mods\\cold-war-curtain", partial: false, cancelled: false, limitsHit: [], completedAt: "2026-10-15T00:00:00Z",
    } as never);
    let finish: ((response: Awaited<ReturnType<typeof runCodexAnalysisResult>>) => void) | undefined;
    vi.mocked(runCodexAnalysisResult).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Scan the project" }));
    const run = await screen.findByRole("button", { name: "Run Codex reanalysis" });
    fireEvent.click(run);
    await waitFor(() => expect(runCodexAnalysisResult).toHaveBeenCalledTimes(1));
    expect(screen.getByRole("combobox", { name: "Target version" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Review in progress..." })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Review in progress..." }));
    expect(runCodexAnalysisResult).toHaveBeenCalledTimes(1);
    expect(vi.mocked(runCodexAnalysisResult).mock.calls[0][0].constraints).toMatchObject({ update_source: { mode: "latest" } });
    // Programmatic state change models navigation or a late external update.
    fireEvent.change(screen.getByRole("combobox", { name: "Target version" }), { target: { value: "pinned_release" } });
    await act(async () => finish?.({ value: { analysis: semanticAnalysisFixture(), record: { analysis_id: "stale", confirmed_fields: [] } } as unknown as CodexAnalysisResult }));
    expect(screen.queryByRole("button", { name: "Confirm Codex suggestions" })).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Target version" })).toHaveValue("pinned_release");
  });

  it("estimates remaining installation time from measured elapsed progress", () => {
    const now = Date.parse("2026-08-02T12:01:00Z");
    expect(estimateRemainingTime(50, "2026-08-02T12:00:00Z", now)).toBe("About 1 minute remaining");
    expect(estimateRemainingTime(0, "2026-08-02T12:00:00Z", now)).toBe("Calculating time remaining");
    expect(estimateRemainingTime(100, "2026-08-02T12:00:00Z", now)).toBe("Complete");
  });
  it("reports truthful rollback backup progress from the child journal", () => {
    expect(recoveryProgress({
      transaction_kind: "rollback",
      state: "preflight",
      operations: [
        { id: "rollback-1", action: "replace", status: "pending", destination: "AGENTS.md", backup_path: "backup/1.bak" },
        { id: "rollback-2", action: "replace", status: "pending", destination: "README.md", backup_path: null, after_exists: null },
        { id: "rollback-3", action: "skip", status: "rolled_back", destination: "kept.txt" },
      ],
    } as never)).toMatchObject({
      complete: 1,
      total: 2,
      percent: 50,
      current: "README.md",
      label: "Saving file 1 of 2",
    });
  });
  it("shows and automatically installs an available signed update on startup", async () => {
    enableTauriRuntime();
    vi.mocked(checkForAppUpdate).mockResolvedValue({
      value: { currentVersion: "0.1.1", availableVersion: "0.2.0", available: true },
    });

    render(<App />);

    expect(await screen.findByText("Installing version 0.2.0...")).toBeInTheDocument();
    await waitFor(() => expect(installAppUpdate).toHaveBeenCalledTimes(1));
  });

  it("does not start duplicate startup update installs", async () => {
    enableTauriRuntime();
    vi.mocked(checkForAppUpdate).mockResolvedValue({
      value: { currentVersion: "0.1.1", availableVersion: "0.2.0", available: true },
    });

    render(<StrictMode><App /></StrictMode>);

    await waitFor(() => expect(installAppUpdate).toHaveBeenCalledTimes(1));
  });

  it("does not interrupt setup when the background update check is offline", async () => {
    enableTauriRuntime();
    vi.mocked(checkForAppUpdate).mockResolvedValue({ value: null, error: "offline" });

    render(<App />);

    await waitFor(() => expect(checkForAppUpdate).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("button", { name: "Update now" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create new mod/i })).toBeInTheDocument();
  });

  it("does not queue duplicate Codex account checks during a pending startup", async () => {
    enableTauriRuntime();
    let resolveAccount: ((value: Awaited<ReturnType<typeof readCodexAccount>>) => void) | undefined;
    vi.mocked(readCodexAccount).mockReturnValue(new Promise((resolve) => { resolveAccount = resolve; }));

    render(<StrictMode><App /></StrictMode>);
    selectCodexProvider();

    await waitFor(() => expect(readCodexAccount).toHaveBeenCalledTimes(1));
    await act(async () => resolveAccount?.({ available: false, authenticated: false, auth_mode: "none", usage_limited: false }));
    expect(await screen.findByRole("button", { name: "Sign in with ChatGPT" })).toBeInTheDocument();
  });

  it("shows a retryable Codex usage-check failure and keeps Continue disabled", async () => {
    enableTauriRuntime();
    vi.mocked(readCodexAccount).mockResolvedValue({
      available: false,
      authenticated: false,
      auth_mode: "",
      usage_limited: false,
      error: "Codex usage could not be checked. Choose Check again to retry.",
    });

    render(<App />);
    selectCodexProvider();

    expect(await screen.findByText("Codex usage could not be checked. Choose Check again to retry.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check again" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("keeps the wizard usable and offers retry when update installation fails", async () => {
    enableTauriRuntime();
    vi.mocked(checkForAppUpdate).mockResolvedValue({
      value: { currentVersion: "0.1.1", availableVersion: "0.2.0", available: true },
    });
    vi.mocked(installAppUpdate).mockResolvedValue({ value: null, error: "signature rejected" });

    render(<App />);
    expect(await screen.findByText("Update failed. Try again.")).toBeInTheDocument();
    expect(installAppUpdate).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Retry update" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Retry update" }));
    await waitFor(() => expect(installAppUpdate).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("button", { name: /create new mod/i })).toBeInTheDocument();
  });

  it("starts in the Project phase and exposes the two supported entry routes", () => {
    render(<App />);
    expect(screen.getAllByText("HOI4 Mod Setup").length).toBeGreaterThan(0);
    expect(screen.getByText("Project")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create new mod/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /import existing mod/i })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Setup phases" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create new mod/i })).toHaveAttribute("aria-pressed", "true");
  });

  it("keeps Continue disabled until the selected provider is ready", () => {
    render(<App />);

    const continueButton = screen.getByRole("button", { name: "Continue" });
    expect(continueButton).toBeDisabled();
    fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });
    expect(continueButton).toBeDisabled();
  });

  describe("Claude account setup assistant", () => {
    const signedInClaude: AiAccountStatus = { available: true, authenticated: true, provider: "claude_account", model: "claude-haiku-5-5", auth_mode: "claude_account", usage_limited: false };

    it("defaults to Claude with Haiku 5.5 and asks the user to sign in through Claude Code", async () => {
      enableTauriRuntime();
      render(<App />);

      expect(screen.getByLabelText("AI provider")).toHaveValue("claude_account");
      expect(screen.getByLabelText("Model")).toHaveValue("claude-haiku-5-5");
      expect(screen.getByLabelText("Claude model ID")).toHaveValue("claude-haiku-5-5");
      expect(screen.getByLabelText("Reasoning effort")).toHaveValue("medium");
      expect(await screen.findByRole("button", { name: "Sign in to Claude" })).toBeEnabled();
      expect(screen.getByText(/never sees your Claude credentials/i)).toBeInTheDocument();
      expect(screen.queryByLabelText(/API key/i)).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
      expect(readAiAccount).toHaveBeenCalledWith("claude_account", "claude-haiku-5-5", "medium", "");
      expect(readCodexAccount).not.toHaveBeenCalled();
    });

    it("allows a Claude Code model ID under Advanced without a live-catalog block", () => {
      function ControlledWelcome() {
        const [state, setState] = useState<WizardState>({
          ...initialState,
          aiModel: "claude-haiku-5-5",
          aiReasoningEffort: "medium",
          aiModelCatalogStatus: "live-missing",
          aiAccount: { ...signedInClaude },
        });
        return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
      }

      render(<ControlledWelcome />);
      fireEvent.click(screen.getByText("Advanced", { selector: "summary" }));
      const model = screen.getByLabelText("Claude model ID");
      fireEvent.change(model, { target: { value: "claude-experimental-6" } });

      expect(model).toHaveValue("claude-experimental-6");
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      expect(screen.queryByRole("combobox", { name: "Reasoning effort" })).not.toBeInTheDocument();
    });

    it("offers the official installer page when Claude Code is missing", async () => {
      enableTauriRuntime();
      vi.mocked(readAiAccount).mockResolvedValue({ available: false, authenticated: false, provider: "claude_account", model: "claude-haiku-5-5", auth_mode: "claude_account", usage_limited: false, error: "Claude Code is not installed. Install it, then choose Check again." });
      render(<App />);

      expect(await screen.findByText("Claude Code is not installed. Install it, then choose Check again.")).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: /Install Claude Code/ }));
      await waitFor(() => expect(openExternalUrlResult).toHaveBeenCalledWith("https://code.claude.com/docs/en/setup"));
      expect(screen.queryByRole("button", { name: "Sign in to Claude" })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    });

    it("runs Claude Code sign-in and enables Continue once the account is signed in", async () => {
      enableTauriRuntime();
      vi.mocked(waitForClaudeLoginResult).mockResolvedValue({ value: signedInClaude });
      render(<App />);

      fireEvent.click(await screen.findByRole("button", { name: "Sign in to Claude" }));

      expect(await screen.findByText("Signed in to Claude")).toBeInTheDocument();
      expect(startClaudeLogin).toHaveBeenCalledTimes(1);
      expect(waitForClaudeLoginResult).toHaveBeenCalledWith("claude-login", "claude-haiku-5-5");
      await waitFor(() => expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled());
    });

    it("shows one busy sign-in state and cancels the pending Claude Code login", async () => {
      enableTauriRuntime();
      vi.mocked(waitForClaudeLoginResult).mockReturnValue(new Promise(() => undefined));
      render(<App />);

      fireEvent.click(await screen.findByRole("button", { name: "Sign in to Claude" }));
      const pending = await screen.findByRole("button", { name: "Waiting for browser sign-in…" });
      expect(pending).toBeDisabled();
      expect(pending).toHaveAttribute("aria-busy", "true");
      expect(screen.getByText(/claude auth login/)).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Cancel sign-in" }));

      await waitFor(() => expect(cancelClaudeLogin).toHaveBeenCalledWith("claude-login"));
      expect(await screen.findByRole("button", { name: "Sign in to Claude" })).toBeEnabled();
      expect(screen.getAllByText("Claude sign-in cancelled. You can try again.").length).toBeGreaterThan(0);
    });

    it("cancels a pending Claude sign-in when another provider is selected", async () => {
      enableTauriRuntime();
      let finishLogin: ((result: { value: AiAccountStatus }) => void) | undefined;
      vi.mocked(waitForClaudeLoginResult).mockReturnValue(new Promise((resolve) => { finishLogin = resolve; }));
      render(<App />);

      fireEvent.click(await screen.findByRole("button", { name: "Sign in to Claude" }));
      await screen.findByRole("button", { name: "Waiting for browser sign-in…" });
      fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "kimi" } });

      await waitFor(() => expect(cancelClaudeLogin).toHaveBeenCalledWith("claude-login"));
      await act(async () => finishLogin?.({ value: signedInClaude }));
      expect(screen.getByLabelText("AI provider")).toHaveValue("kimi");
      expect(screen.queryByText("Signed in to Claude")).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    });

    it("keeps the draft usable when Claude sign-in does not finish", async () => {
      enableTauriRuntime();
      render(<App />);

      fireEvent.click(await screen.findByRole("button", { name: "Sign in to Claude" }));

      expect((await screen.findAllByText("Claude sign-in did not finish.")).length).toBeGreaterThan(0);
      expect(screen.getByRole("button", { name: "Sign in to Claude" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    });

    it("signs out through Claude Code and clears provider-bound analysis", async () => {
      enableTauriRuntime();
      const update = vi.fn();
      vi.mocked(readAiAccount).mockResolvedValue({ ...signedInClaude, authenticated: false, error: "Sign in to Claude to continue." });
      render(<Welcome state={{ ...initialState, aiAccount: signedInClaude, codexAnalysisRecord: { analysis_id: "a" } as never }} update={update} />);

      fireEvent.click(screen.getByRole("button", { name: "Sign out" }));

      await waitFor(() => expect(logoutClaudeResult).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(update).toHaveBeenCalledWith(expect.objectContaining({ codexAnalysisRecord: undefined, codexAnalysis: undefined, aiAccount: expect.objectContaining({ authenticated: false }) })));
    });

    it("lets the user switch to an Anthropic API key without losing the Haiku default", () => {
      function ControlledWelcome() {
        const [state, setState] = useState<WizardState>({ ...initialState, aiAccount: { available: true, authenticated: false, provider: "claude_account", model: "claude-haiku-5-5", auth_mode: "claude_account", usage_limited: false, error: "Sign in to Claude to continue." } });
        return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
      }
      render(<ControlledWelcome />);

      fireEvent.click(screen.getByRole("button", { name: "Use an Anthropic API key instead" }));

      expect(screen.getByLabelText("AI provider")).toHaveValue("claude");
      expect(screen.getByLabelText("Model")).toHaveValue("claude-haiku-5-5");
      expect(screen.getByLabelText("Anthropic API key")).toHaveAttribute("type", "password");
    });
  });

  it("selects a setup assistant without presenting it as the future development client", () => {
    function ControlledWelcome() {
      const [state, setState] = useState(welcomeState({ available: false, authenticated: false, auth_mode: "none", usage_limited: false }));
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });

    expect(screen.getByLabelText("Model")).toBeInTheDocument();
    expect(screen.getByLabelText("Provider address")).toBeInTheDocument();
    expect(screen.getByLabelText("Model")).toHaveValue("claude-haiku-5-5");
    expect(screen.getByLabelText("Reasoning effort")).toHaveValue("medium");
    expect(screen.getByLabelText("Provider address")).toHaveValue("https://api.anthropic.com/v1/messages");
    expect(screen.getByLabelText("Anthropic API key")).toHaveAttribute("type", "password");
    expect(screen.getByText("Advanced")).toBeInTheDocument();
    expect(screen.getByText(/Used only to analyze and prepare the mod/i)).toBeInTheDocument();
    expect(screen.queryByText(/project follows the provider/i)).not.toBeInTheDocument();
    expect(screen.getByLabelText("AI provider")).toHaveValue("claude");
  });

  it("uses the live model catalog and only offers efforts supported by the selected model", async () => {
    enableTauriRuntime();
    vi.mocked(readAiModels).mockResolvedValue([
      { id: "gpt-5.6-luna", display_name: "GPT-5.6 Luna", default_reasoning_effort: "xhigh", supported_reasoning_efforts: ["low", "high", "xhigh", "max"] },
      { id: "deliberate-model", display_name: "Deliberate model", default_reasoning_effort: "max", supported_reasoning_efforts: ["low", "max"] },
    ]);
    function ControlledWelcome() {
      const [state, setState] = useState(welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false }));
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    await waitFor(() => expect(within(screen.getByLabelText("Reasoning effort")).getAllByRole("option").map((option) => option.textContent)).toEqual(["Light", "High", "Extra high", "Max"]));
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "deliberate-model" } });

    expect(screen.getByLabelText("Reasoning effort")).toHaveValue("max");
    expect(within(screen.getByLabelText("Reasoning effort")).getAllByRole("option").map((option) => option.textContent)).toEqual(["Light", "Max"]);
  });

  it.each([false, true])("uses Codex's advertised default only for an untouched model choice (explicit=%s)", async (explicit) => {
    enableTauriRuntime();
    vi.mocked(readAiModels).mockResolvedValue([
      { id: "another-model", display_name: "Another model", default_reasoning_effort: "high", supported_reasoning_efforts: ["high"] },
      { id: "server-default", display_name: "Account default", default_reasoning_effort: "medium", supported_reasoning_efforts: ["low", "medium", "high"], is_default: true },
    ]);
    function ControlledWelcome() {
      const [state, setState] = useState({ ...welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false }), aiModelChoiceExplicit: explicit });
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }
    render(<ControlledWelcome />);
    if (explicit) {
      await screen.findByRole("alert");
      expect(screen.getByLabelText("Model")).toHaveValue("gpt-5.6-luna");
    } else {
      await waitFor(() => expect(screen.getByLabelText("Model")).toHaveValue("server-default"));
      expect(screen.getByLabelText("Reasoning effort")).toHaveValue("medium");
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    }
  });

  it.each([
    ["codex", "gpt-5.6-luna", "xhigh", ""],
    ["claude", "claude-haiku-4-5-20251001", "high", "https://api.anthropic.com/v1/messages"],
    ["kimi", "kimi-k2.6", "high", "https://api.moonshot.ai/v1/chat/completions"],
    ["glm", "glm-5.2", "high", "https://open.bigmodel.cn/api/paas/v4/chat/completions"],
    ["deepseek", "deepseek-flash", "high", "https://api.deepseek.com/chat/completions"],
    ["local", "local-model", "high", "http://127.0.0.1:11434/v1/chat/completions"],
    ["custom", "custom-model", "high", "https://models.example.test/v1/chat/completions"],
  ] as const)("keeps the %s model control usable when its live catalog fails", async (provider, model, effort, endpoint) => {
    enableTauriRuntime();
    vi.mocked(readAiModels).mockRejectedValue(new Error("catalog offline"));
    function ControlledWelcome() {
      const [state, setState] = useState({
        ...welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false }),
        aiProvider: provider,
        aiModel: model,
        aiReasoningEffort: effort,
        aiEndpoint: endpoint,
        aiAccount: provider === "codex" ? null : { available: true, authenticated: true, provider, model, auth_mode: provider === "local" ? "local_or_user_configured" : "api_key", usage_limited: false },
      } as WizardState);
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    await waitFor(() => expect(readAiModels).toHaveBeenCalledWith(provider, endpoint));
    expect(screen.getByLabelText("Model")).toHaveValue(model);
    if (provider === "local" || provider === "custom") {
      expect(screen.getByLabelText("Model")).toHaveAttribute("list", "provider-model-options");
      expect(await screen.findByText(/Live model suggestions could not be refreshed/i)).toBeInTheDocument();
    } else {
      expect(within(screen.getByLabelText("Model")).getByRole("option", { name: model === "claude-haiku-4-5-20251001" ? "Claude Haiku 4.5" : model })).toBeInTheDocument();
      if (provider === "codex") expect(within(screen.getByLabelText("Reasoning effort")).getAllByRole("option")).toHaveLength(1);
      else expect(screen.queryByLabelText("Reasoning effort")).not.toBeInTheDocument();
      expect(await screen.findByText(/Using the built-in model/i)).toBeInTheDocument();
    }
  });

  it.each([
    ["codex", "gpt-5.6-luna", "", false],
    ["claude", "claude-haiku-4-5-20251001", "https://api.anthropic.com/v1/messages", false],
    ["kimi", "kimi-k2.6", "https://api.moonshot.ai/v1/chat/completions", false],
    ["glm", "glm-5.2", "https://open.bigmodel.cn/api/paas/v4/chat/completions", false],
    ["deepseek", "deepseek-flash", "https://api.deepseek.com/chat/completions", false],
    ["local", "local-model", "http://127.0.0.1:11434/v1/chat/completions", true],
    ["custom", "custom-model", "https://models.example.test/v1/chat/completions", true],
  ] as const)("distinguishes an empty %s catalog from a live result", async (provider, model, endpoint, manual) => {
    enableTauriRuntime();
    vi.mocked(readAiModels).mockResolvedValue([]);
    function ControlledWelcome() {
      const [state, setState] = useState({
        ...welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false }),
        aiProvider: provider,
        aiModel: model,
        aiEndpoint: endpoint,
        aiAccount: provider === "codex" ? null : { available: true, authenticated: true, provider, model, auth_mode: provider === "local" ? "local_or_user_configured" : "api_key", usage_limited: false },
      } as WizardState);
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    expect(await screen.findByText(manual ? /No live model suggestions were returned/i : /No live models were returned/i)).toBeInTheDocument();
    expect(screen.getByLabelText("Model")).toHaveValue(model);
  });

  it.each(["local", "custom"] as const)("keeps the %s model field editable while showing live suggestions", async (provider) => {
    enableTauriRuntime();
    vi.mocked(readAiModels).mockResolvedValue([{ id: "suggested-model", display_name: "Suggested model", default_reasoning_effort: "high", supported_reasoning_efforts: ["high"] }]);
    function ControlledWelcome() {
      const [state, setState] = useState({
        ...welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false }),
        aiProvider: provider,
        aiModel: "entered-model",
        aiEndpoint: provider === "local" ? "http://127.0.0.1:11434/v1/chat/completions" : "https://models.example.test/v1/chat/completions",
        aiAccount: { available: true, authenticated: true, provider, model: "entered-model", auth_mode: provider === "local" ? "local_or_user_configured" : "api_key", usage_limited: false },
      } as WizardState);
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    await waitFor(() => expect(document.querySelector('datalist option[value="suggested-model"]')).toBeInTheDocument());
    const modelInput = screen.getByRole("combobox", { name: /^Model/ });
    fireEvent.change(modelInput, { target: { value: "another-model" } });
    expect(modelInput).toHaveValue("another-model");
  });

  it("opens only the fixed provider account page from the simple connection flow", async () => {
    enableTauriRuntime();
    function ControlledWelcome() {
      const [state, setState] = useState(welcomeState({ available: false, authenticated: false, auth_mode: "none", usage_limited: false }));
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });
    fireEvent.click(screen.getByRole("button", { name: "Get Anthropic API key" }));

    await waitFor(() => expect(openExternalUrlResult).toHaveBeenCalledWith("https://platform.claude.com/settings/keys"));
  });

  it("invalidates confirmed analysis when the source version changes", async () => {
    const update = vi.fn();
    render(<Components state={{
      aiProvider: "codex",
      sourceMode: "latest",
      pinnedRef: "",
      selectedComponents: ["core.skills"],
      codexAnalysis: { analysis_id: "analysis" },
      codexAnalysisRecord: { analysis_id: "analysis", confirmed_fields: ["display_name"] },
    } as unknown as WizardState} update={update} />);

    fireEvent.click(screen.getByText("Choose source version"));
    fireEvent.change(screen.getByLabelText("Version"), { target: { value: "pinned_commit" } });

    expect(update).toHaveBeenCalledWith(expect.objectContaining({
      sourceMode: "pinned_commit",
      pinnedRef: "",
      codexAnalysis: undefined,
      codexAnalysisRecord: undefined,
      plan: undefined,
    }));
  });

  it("returns a new project to analysis review after a pinned source edit", async () => {
    enableTauriRuntime();
    vi.mocked(suggestProjectPaths).mockResolvedValue({
      mod_directory: "C:\\mods",
      project_root: "C:\\mods\\atlantis_rising",
      launcher_descriptor_path: "C:\\mods\\atlantis_rising.mod",
      project_exists: false,
      launcher_descriptor_exists: false,
    });
    vi.mocked(runCodexAnalysisResult).mockResolvedValue({ value: {
      analysis: {
        schema_version: "1.0.0", analysis_id: "source-bound-analysis", mode: "new_project_identity", input_sha256: "a".repeat(64),
        project_summary: "A source-bound project", proposals: [], component_recommendations: [], warnings: [],
      },
      record: {
        engine: "codex_app_server", auth_mode: "chatgpt", provider: "codex", model: "default", analysis_id: "source-bound-analysis",
        schema_version: "1.0.0", input_sha256: "a".repeat(64), output_sha256: "b".repeat(64),
        confirmed_fields: ["display_name", "project_id", "script_prefix", "primary_namespace", "project_description", "descriptor_tags", "folder_profile", "agents_profile", "localisation_convention", "documentation_convention"],
        confirmed_at: "2026-08-21T00:00:00Z", source_revision: "c".repeat(40), source_manifest_sha256: "d".repeat(64),
      },
    } as CodexAnalysisResult });
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: {
      schema_version: "1.0.0", manifest_id: "source-bound", source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "c".repeat(40), manifest_sha256: "d".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [{ id: "core.skills", display_name: "Skills", category: "skill", optional: false, platforms: ["all"], source: { kind: "tree", path: ".agents/skills" }, destination: { path: ".agents/skills/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [], update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true } }],
      profiles: [{ id: "core", display_name: "Core", components: ["core.skills"], default: true }],
    } as unknown as SourceManifestPreview });

    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Project identity" });
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Coding environments" });
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Image generation" });
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Choose what to install" });
    fireEvent.click(screen.getByText("Choose source version"));
    fireEvent.change(screen.getByLabelText("Version"), { target: { value: "pinned_commit" } });
    fireEvent.change(await screen.findByLabelText("Commit"), { target: { value: "e".repeat(40) } });
    await screen.findByText("Components loaded.");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));

    expect(await screen.findByRole("heading", { name: "Describe the mod" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Optional workflows" })).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(/exact source/i);
  });

  it("returns an existing project to Findings after its pinned source is edited", async () => {
    enableTauriRuntime();
    const manifest = {
      schema_version: "1.0.0", manifest_id: "existing-source", source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [{ id: "core.skills", display_name: "Skills", category: "skill", optional: false, platforms: ["all"], source: { kind: "tree", path: ".agents/skills" }, destination: { path: ".agents/skills/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [], update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true } }],
      profiles: [{ id: "core", display_name: "Core", components: ["core.skills"], default: true }],
    } as unknown as SourceManifestPreview;
    window.__HOI4_DOCUMENTATION_STATE__ = {
      ...initialState,
      screen: "components",
      mode: "existing",
      aiProvider: "codex",
      aiModel: "gpt-5.6-luna",
      aiReasoningEffort: "xhigh",
      codexAccount: { available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false },
      codexAnalysis: { analysis_id: "existing-analysis" } as never,
      codexAnalysisRecord: { analysis_id: "existing-analysis", confirmed_fields: ["display_name"] } as never,
      manifestPreview: manifest,
      selectedComponents: ["core.skills"],
    };
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: manifest });

    render(<App />);
    await screen.findByRole("heading", { name: "Choose what to install" });
    fireEvent.click(screen.getByText("Choose source version"));
    fireEvent.change(screen.getByLabelText("Version"), { target: { value: "pinned_release" } });
    fireEvent.change(await screen.findByLabelText("Release"), { target: { value: "v0.2.12" } });
    await screen.findByText("Components loaded.");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));

    expect(await screen.findByRole("heading", { name: "Confirm scan findings" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Optional workflows" })).not.toBeInTheDocument();
  });

  it("classifies manifest platform support without enabling Windows workflows on macOS", () => {
    const component = { platforms: ["windows"] } as unknown as SourceManifestPreview["components"][number];
    expect(manifestComponentSupportsPlatform(component, "windows")).toBe(true);
    expect(manifestComponentSupportsPlatform(component, "macos")).toBe(false);
  });

  it("keeps a missing 3D route visibly unavailable instead of opening Mesh setup", () => {
    render(<Workflows state={{ aiProvider: "codex", meshSelected: true, superEventsSelected: false, selectedComponents: [], manifestPreview: { components: [] } } as unknown as WizardState} update={vi.fn()} />);

    const mesh = screen.getByRole("switch", { name: /3D models workflow/ });
    expect(mesh).toBeDisabled();
    expect(mesh).toHaveAttribute("aria-checked", "false");
  });

  it("labels an unsupported manifest workflow on macOS without offering selection", () => {
    const originalUserAgent = navigator.userAgent;
    Object.defineProperty(navigator, "userAgent", { configurable: true, value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)" });
    try {
      const workflow = {
        id: "workflow.future_windows",
        display_name: "Future Windows workflow",
        description: "A source-declared optional workflow",
        category: "workflow",
        optional: true,
        platforms: ["windows"],
        source: { kind: "tree", path: ".tools/future" },
        destination: { path: ".tools/future/", ownership: "managed" },
        dependencies: [],
        required_tools: [],
        environment: [],
        expected_files: [],
        capabilities: [],
        validation: [],
        update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
      };
      render(<Workflows state={{ aiProvider: "codex", meshSelected: false, superEventsSelected: false, selectedComponents: [], manifestPreview: { components: [workflow] } } as unknown as WizardState} update={vi.fn()} />);

      const future = screen.getByRole("switch", { name: /Future Windows workflow/ });
      expect(future).toBeDisabled();
      expect(screen.getByText("Not available on this computer")).toBeInTheDocument();
    } finally {
      Object.defineProperty(navigator, "userAgent", { configurable: true, value: originalUserAgent });
    }
  });

  it("does not surface a stale Windows-only MCP route on macOS", () => {
    const originalUserAgent = navigator.userAgent;
    Object.defineProperty(navigator, "userAgent", { configurable: true, value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)" });
    try {
      const component = {
        id: "mcp.windows_tools",
        display_name: "Windows tools",
        category: "mcp",
        optional: true,
        platforms: ["windows"],
        source: { kind: "tree", path: ".tools/windows" },
        destination: { path: ".tools/windows/", ownership: "managed" },
        dependencies: [],
        required_tools: [],
        environment: [],
        expected_files: [],
        capabilities: [],
        validation: [],
        update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
      };
      render(<Mcp state={{ selectedComponents: [component.id], manifestPreview: { components: [component] } } as unknown as WizardState} />);
      expect(screen.getByText("No integration is selected.")).toBeInTheDocument();
    } finally {
      Object.defineProperty(navigator, "userAgent", { configurable: true, value: originalUserAgent });
    }
  });

  it("offers a direct retry after manifest loading fails", async () => {
    const manifest = {
      schema_version: "1.0.0",
      manifest_id: "retry",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [],
      profiles: [],
    } as unknown as SourceManifestPreview;
    vi.mocked(previewSourceManifestResult)
      .mockResolvedValueOnce({ value: null, error: "temporary source failure" })
      .mockResolvedValueOnce({ value: manifest });

    render(<Components state={{ aiProvider: "codex", sourceMode: "latest", pinnedRef: "", selectedComponents: [] } as unknown as WizardState} update={vi.fn()} />);

    fireEvent.click(await screen.findByRole("button", { name: "Retry loading components" }));
    expect(await screen.findByText("Components loaded.")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("status")).toHaveFocus());
    expect(previewSourceManifestResult).toHaveBeenCalledTimes(2);
  });

  it("selects newly published default-profile components without an app allowlist", async () => {
    const future = {
      id: "core.future_skills",
      display_name: "Future skills",
      description: "New repository-declared skills",
      category: "skill",
      optional: false,
      platforms: ["all"],
      source: { kind: "tree", path: ".agents/skills/future" },
      destination: { path: ".agents/skills/future/", ownership: "managed" },
      dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [],
      update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: {
      schema_version: "1.0.0",
      manifest_id: "dynamic",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [future],
      profiles: [{ id: "core", display_name: "Core", components: [future.id], default: true }],
    } as unknown as SourceManifestPreview });

    function ControlledComponents() {
      const [state, setState] = useState({ aiProvider: "codex", sourceMode: "latest", pinnedRef: "", selectedComponents: [] } as unknown as WizardState);
      return <><Components state={state} update={(patch) => setState((value) => ({ ...value, ...patch }))} /><output aria-label="selected components">{state.selectedComponents.join(",")}</output></>;
    }

    render(<ControlledComponents />);
    expect(await screen.findAllByText("Future skills")).not.toHaveLength(0);
    expect(screen.getByLabelText("selected components")).toHaveTextContent("core.future_skills");
  });

  it("does not silently select a newly defaulted optional for an existing installation", async () => {
    const future = {
      id: "workflow.future_tools", display_name: "Future tools", category: "workflow", optional: true, platforms: ["all"],
      source: { kind: "tree", path: ".tools/future" }, destination: { path: ".tools/future/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [],
      update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: {
      schema_version: "1.0.0", manifest_id: "dynamic",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [future], profiles: [{ id: "core", display_name: "Core", components: [future.id], default: true }],
    } as unknown as SourceManifestPreview });

    function ControlledComponents() {
      const [state, setState] = useState({ aiProvider: "codex", sourceMode: "latest", pinnedRef: "", selectedComponents: [], existingInstallationDetected: true, installedComponentIds: [], maintenanceOptionalSelections: [] } as unknown as WizardState);
      return <><Components state={state} update={(patch) => setState((value) => ({ ...value, ...patch }))} /><output aria-label="selected existing components">{state.selectedComponents.join(",")}</output></>;
    }

    render(<ControlledComponents />);
    await screen.findByText("Components loaded.");
    expect(screen.getByLabelText("selected existing components")).not.toHaveTextContent("workflow.future_tools");
  });

  it("adds only explicitly selected top-level optional components during maintenance", () => {
    const dependency = {
      id: "workflow.future_tools.runtime", display_name: "Future runtime", category: "workflow", optional: true, platforms: ["all"],
      source: { kind: "tree", path: ".tools/future/runtime" }, destination: { path: ".tools/future/runtime/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [], update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    const parent = { ...dependency, id: "workflow.future_tools", display_name: "Future tools", dependencies: [dependency.id] };
    const state = {
      ...readyState(), existingInstallationDetected: true, installedComponentIds: [], maintenanceOptionalSelections: [parent.id, dependency.id],
      manifestPreview: { components: [parent, dependency], profiles: [] },
    } as unknown as WizardState;

    expect(dynamicMaintenanceOptionalComponentIds(state)).toEqual(["workflow.future_tools"]);
    expect(dynamicMaintenanceOptionalComponentIds({ ...state, installedComponentIds: [parent.id] })).toEqual([]);
  });

  it("renders and selects a future optional workflow from the manifest", () => {
    const future = {
      id: "workflow.future_tools", display_name: "Future tools workflow", description: "New source-declared helper workflow", category: "workflow", optional: true, platforms: ["all"],
      source: { kind: "tree", path: ".tools/future" }, destination: { path: ".tools/future/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [],
      update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    const state = {
      ...readyState(),
      manifestPreview: { components: [future], profiles: [] },
      selectedComponents: [],
      meshSelected: false,
      superEventsSelected: false,
    } as unknown as WizardState;
    function ControlledWorkflows() {
      const [current, setCurrent] = useState(state);
      return <><Workflows state={current} update={(patch) => setCurrent((value) => ({ ...value, ...patch }))} /><output aria-label="selected workflows">{current.selectedComponents.join(",")}</output></>;
    }

    render(<ControlledWorkflows />);
    fireEvent.click(screen.getByRole("switch", { name: /Future tools workflow/ }));
    expect(screen.getByLabelText("selected workflows")).toHaveTextContent("workflow.future_tools");
  });

  it("restores the optional HOI4 Live Tools workflow and its live-testing route", () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    const liveTesting = {
      id: "workflow.live_testing", display_name: "Live testing", category: "workflow", optional: true, platforms: ["all"],
      source: { kind: "tree", path: ".claude/skills/hoi4-live-playtest" }, destination: { path: ".claude/skills/hoi4-live-playtest/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [],
      update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    const liveTools = {
      id: "workflow.live_tools", display_name: "HOI4 Live Tools", description: "Install the experimental Windows HOI4 Live Tools runtime.", category: "workflow", optional: true, platforms: ["windows"],
      source: { kind: "tree", path: ".claude/skills" }, destination: { path: ".claude/skills/", ownership: "managed" }, dependencies: ["workflow.live_testing"], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [],
      update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    const state = { ...readyState(), liveTesting: { provider: "none", linux_ready: false, wsl_distribution: "", ssh_host: "" }, manifestPreview: { components: [liveTesting, liveTools], profiles: [] }, selectedComponents: [], meshSelected: false, superEventsSelected: false } as unknown as WizardState;
    function ControlledWorkflows() {
      const [current, setCurrent] = useState(state);
      return <><Workflows state={current} update={(patch) => setCurrent((value) => ({ ...value, ...patch }))} /><output aria-label="selected workflows">{current.selectedComponents.join(",")}</output><output aria-label="live testing provider">{current.liveTesting.provider}</output></>;
    }

    render(<ControlledWorkflows />);
    const liveToolsSwitch = screen.getByRole("switch", { name: /HOI4 Live Tools/ });
    expect(liveToolsSwitch).toBeEnabled();
    fireEvent.click(liveToolsSwitch);
    expect(screen.getByLabelText("selected workflows")).toHaveTextContent("workflow.live_tools");
    expect(screen.getByLabelText("selected workflows")).toHaveTextContent("workflow.live_testing");
    expect(screen.getByLabelText("live testing provider")).toHaveTextContent("hoi4_live_tools");
  });

  it("shows one parent switch for a manifest workflow dependency closure", () => {
    const dependency = {
      id: "workflow.future_tools.runtime", display_name: "Future runtime internals", category: "workflow", optional: true, platforms: ["all"],
      source: { kind: "tree", path: ".tools/future/runtime" }, destination: { path: ".tools/future/runtime/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [],
      update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    const parent = {
      id: "workflow.future_tools", display_name: "Future tools workflow", category: "workflow", optional: true, platforms: ["all"],
      source: { kind: "tree", path: ".tools/future" }, destination: { path: ".tools/future/", ownership: "managed" }, dependencies: [dependency.id], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [],
      update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true },
    };
    const state = {
      ...readyState(),
      manifestPreview: { components: [parent, dependency], profiles: [] },
      selectedComponents: [],
      meshSelected: false,
      superEventsSelected: false,
    } as unknown as WizardState;

    render(<Workflows state={state} update={vi.fn()} />);

    expect(screen.getByRole("switch", { name: /Future tools workflow/ })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: /Future runtime internals/ })).not.toBeInTheDocument();
  });

  it.each(["repair", "remove"] as const)("scans a newly installed project before %s planning", async (mode) => {
    enableTauriRuntime();
    window.__HOI4_DOCUMENTATION_STATE__ = { ...initialState, ...readyState(), identity: { ...initialState.identity, ...readyState().identity }, projectPathsOverridden: true, screen: "update", scanContext: undefined };
    let finishScan: ((value: Awaited<ReturnType<typeof scanProject>>) => void) | undefined;
    vi.mocked(scanProject).mockReturnValue(new Promise((resolve) => { finishScan = resolve; }));
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: mode === "repair" ? /Repair installation/ : /Remove components/ }));
    await waitFor(() => expect(scanProject).toHaveBeenCalled());
    expect(buildMaintenancePlanResult).not.toHaveBeenCalled();
    await act(async () => finishScan?.({
      findings: [], scanId: "fresh-maintenance", projectRoot: "C:\\mods\\cold-war-curtain", partial: false, cancelled: false,
      limitsHit: [], filesScanned: 1, directoriesScanned: 1, bytesRead: 1, completedAt: "2026-10-14T00:00:00Z",
    } as never));
    await waitFor(() => expect(buildMaintenancePlanResult).toHaveBeenCalled());
    expect(vi.mocked(scanProject).mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(buildMaintenancePlanResult).mock.invocationCallOrder[0]);
  });

  it("uses the fresh installed snapshot without overwriting the requested workflow", async () => {
    enableTauriRuntime();
    window.__HOI4_DOCUMENTATION_STATE__ = {
      ...initialState, ...readyState(), identity: { ...initialState.identity, ...readyState().identity },
      projectPathsOverridden: true, screen: "update", scanContext: undefined,
      existingInstallationDetected: false, meshSelected: true, installedWorkflow3dState: "ready",
    };
    vi.mocked(scanProject).mockResolvedValue({
      findings: [{ id: "installation.managed", value: JSON.stringify({ present: true, valid: true, component_ids: ["core.agents"], workflow_3d_state: "not_selected" }) }],
      scanId: "fresh", projectRoot: "C:\\mods\\cold-war-curtain", partial: false, cancelled: false, limitsHit: [], completedAt: "2026-10-14T00:00:00Z",
    } as never);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Repair installation/ }));
    await waitFor(() => expect(buildMaintenancePlanResult).toHaveBeenCalled());
    expect(vi.mocked(buildMaintenancePlanResult).mock.calls[0][3]).toContain("workflow.3d");
  });

  it("keeps confirmed Update reanalysis bound to its original scan", async () => {
    enableTauriRuntime();
    const record = { analysis_id: "confirmed-update", confirmed_fields: ["display_name"], scan_id: "confirmed-scan" } as unknown as WizardState["maintenanceCodexAnalysisRecord"];
    window.__HOI4_DOCUMENTATION_STATE__ = {
      ...initialState, ...readyState(), identity: { ...initialState.identity, ...readyState().identity },
      projectPathsOverridden: true, screen: "update", maintenanceEvidenceReady: true,
      maintenanceCodexAnalysisRecord: record,
    };
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Check for updates/ }));
    await waitFor(() => expect(buildMaintenancePlanResult).toHaveBeenCalled());
    expect(scanProject).not.toHaveBeenCalled();
    expect(vi.mocked(buildMaintenancePlanResult).mock.calls[0][2]).toBe(record);
    expect(vi.mocked(buildMaintenancePlanResult).mock.calls[0][10]).toEqual({ mode: "latest", selected_ref: undefined });
  });

  it("defaults to Latest and invalidates review when the update target changes", () => {
    expect(initialState.sourceMode).toBe("latest");
    expect(initialState.pinnedRef).toBe("");
    const update = vi.fn();
    render(<UpdateSourceOptions state={initialState} update={update} />);
    expect(screen.getByRole("combobox", { name: "Target version" })).toHaveValue("latest");
    fireEvent.change(screen.getByRole("combobox", { name: "Target version" }), { target: { value: "pinned_release" } });
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({
      sourceMode: "pinned_release", pinnedRef: "", plan: undefined, manifestPreview: undefined,
      maintenanceEvidenceReady: false, maintenanceCodexAnalysisRecord: undefined, codexAnalysisRecord: undefined,
    }));
    cleanup();
    render(<UpdateSourceOptions state={{ ...initialState, sourceMode: "pinned_release", pinnedRef: "old" }} update={update} />);
    fireEvent.change(screen.getByLabelText("Release", { exact: true }), { target: { value: "new" } });
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ sourceMode: "pinned_release", pinnedRef: "new", maintenanceCodexAnalysisRecord: undefined }));
    fireEvent.change(screen.getByRole("combobox", { name: "Target version" }), { target: { value: "latest" } });
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ sourceMode: "latest", pinnedRef: "" }));
  });

  it.each([null, { partial: true, cancelled: false }, { partial: false, cancelled: true }])("does not plan repair after an incomplete scan: %j", async (scan) => {
    enableTauriRuntime();
    window.__HOI4_DOCUMENTATION_STATE__ = { ...initialState, ...readyState(), identity: { ...initialState.identity, ...readyState().identity }, projectPathsOverridden: true, screen: "update", scanContext: undefined };
    vi.mocked(scanProject).mockResolvedValue(scan as never);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Repair installation/ }));
    await screen.findByText("The project could not be fully scanned. Nothing was changed; try the review again.");
    expect(buildMaintenancePlanResult).not.toHaveBeenCalled();
  });

  it("offers both optional workflows again for an existing managed setup", () => {
    const onStartMaintenance = vi.fn();
    render(<Update
      state={{
        ...readyState(),
        aiProvider: "codex",
        existingInstallationDetected: true,
        installedWorkflow3dState: "not_selected",
        installedSuperEventsState: "not_selected",
        meshSelected: false,
        superEventsSelected: false,
        meshKeyDraft: "",
        meshKeyStatus: "missing",
        maintenanceEvidenceReady: false,
        findings: [],
      } as unknown as WizardState}
      update={vi.fn()}
      findings={[]}
      setFindings={vi.fn()}
      onMaintenance={vi.fn()}
      onStartMaintenance={onStartMaintenance}
      onReanalyze={vi.fn().mockResolvedValue(true)}
    />);

    expect(screen.getByText("3D models workflow")).toBeInTheDocument();
    expect(screen.getAllByText("Super Events workflow")).toHaveLength(2);
    expect(screen.getAllByText("Add it during the next update or repair")).toHaveLength(3);
    fireEvent.click(screen.getByRole("button", { name: /Repair installation/ }));
    expect(onStartMaintenance).toHaveBeenCalledWith("repair");

    cleanup();
    render(<Update
      state={{
        ...readyState(),
        aiProvider: "codex",
        existingInstallationDetected: true,
        installedWorkflow3dState: "not_selected",
        meshSelected: true,
        meshKeyDraft: "",
        meshKeyStatus: "missing",
        meshCredentialReference: undefined,
        maintenanceEvidenceReady: false,
        findings: [],
      } as unknown as WizardState}
      update={vi.fn()}
      findings={[]}
      setFindings={vi.fn()}
      onMaintenance={vi.fn()}
      onStartMaintenance={vi.fn()}
      onReanalyze={vi.fn().mockResolvedValue(true)}
    />);
    expect(screen.getByLabelText("Meshy API key")).toBeInTheDocument();
  });

  it("keeps an installed client that the selected source version does not offer and says why", () => {
    const update = vi.fn();
    const preview = {
      schema_version: "5.0.0", components: [], profiles: [],
      coding_environments: [{ id: "claude_code", display_name: "Claude Code", default_primary: true }, { id: "codex", display_name: "Codex" }, { id: "cursor", display_name: "Cursor" }],
    } as unknown as SourceManifestPreview;
    render(<Update
      state={{
        ...readyState(),
        aiProvider: "codex",
        existingInstallationDetected: true,
        installedWorkflow3dState: "not_selected",
        installedSuperEventsState: "not_selected",
        meshSelected: false,
        superEventsSelected: false,
        meshKeyDraft: "",
        meshKeyStatus: "missing",
        maintenanceEvidenceReady: false,
        findings: [],
        manifestPreview: preview,
        primaryCodingEnvironment: "claude_code",
        additionalCodingEnvironments: ["gemini_cli", "codex"],
      } as unknown as WizardState}
      update={update}
      findings={[]}
      setFindings={vi.fn()}
      onMaintenance={vi.fn()}
      onStartMaintenance={vi.fn()}
      onReanalyze={vi.fn().mockResolvedValue(true)}
    />);
    expect(screen.getByText(/Installed but not in this source version: gemini_cli\..*will not remove its files implicitly/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: /^Cursor/, hidden: true }));
    expect(update).toHaveBeenCalledWith(expect.objectContaining({ primaryCodingEnvironment: "claude_code", additionalCodingEnvironments: ["codex", "cursor", "gemini_cli"] }));
  });

  it("uses declarative workflow titles and places Super Events immediately after 3D", async () => {
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    const modelsWorkflow = screen.getByRole("switch", { name: /3D models workflow/ });
    const superEventsWorkflow = screen.getByRole("switch", { name: /Super Events workflow/ });
    expect(modelsWorkflow.compareDocumentPosition(superEventsWorkflow) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.click(superEventsWorkflow);
    expect(superEventsWorkflow).toHaveAttribute("aria-checked", "true");
    expect(screen.queryByText(/LoRA/i)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("heading", { name: "MCP and credentials" })).toBeInTheDocument();
  });

  it("reveals the portrait workflow resource minimum when enabled", () => {
    const portraitPipeline = {
      enabled: false,
      provider: "disabled",
      providerStatus: "not_selected",
      workflowRepository: "https://github.com/klimPaskov/comfyui-hoi4-portraits",
      workflowBranch: "codex/portrait-pipeline",
      workflowCommit: "a".repeat(40),
      preferredWorkflow: "source",
      localComfyuiRoot: "",
      localServerUrl: "http://127.0.0.1:8188",
      runpodUrl: "",
      runpodWorkspace: "/workspace/comfyui-hoi4-portraits",
      mcpRegistered: false,
    } as const;
    const state = {
      ...readyState(),
      manifestPreview: {
        components: [
          { id: "workflow.3d" },
          { id: "workflow.super_events" },
          { id: "workflow.portraits.cloud" },
          { id: "workflow.portraits.local" },
          { id: "workflow.portraits.runpod" },
        ],
      },
      portraitPipeline,
      meshSelected: false,
      superEventsSelected: false,
      selectedComponents: [],
    } as unknown as WizardState;

    function ControlledWorkflows() {
      const [current, setCurrent] = useState(state);
      return <Workflows state={current} update={(patch) => setCurrent((value) => ({ ...value, ...patch }))} />;
    }

    render(<ControlledWorkflows />);
    expect(screen.queryByText(/Minimum recommended to run this workflow/i)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("switch", { name: /ComfyUI portrait production/ }));
    expect(screen.getByText("Minimum recommended to run this workflow: 16 GB VRAM and 25 GB storage.")).toBeInTheDocument();

    cleanup();
    render(<Update
      state={{
        ...state,
        existingInstallationDetected: true,
        portraitPipeline: { ...portraitPipeline, enabled: true, provider: "runpod", providerStatus: "needs_runpod" },
      } as unknown as WizardState}
      update={vi.fn()}
      findings={[]}
      setFindings={vi.fn()}
      onMaintenance={vi.fn()}
      onStartMaintenance={vi.fn()}
      onReanalyze={vi.fn().mockResolvedValue(true)}
    />);
    expect(screen.getByText("Minimum recommended to run this workflow: 16 GB VRAM and 25 GB storage.")).toBeInTheDocument();
  });

  it("advances from Meshy configuration without changing an existing stored key", () => {
    const update = vi.fn();
    render(<Mesh state={{
      meshKeyDraft: "",
      meshKeyStatus: "present",
      meshCredentialReference: "credential://existing",
      selectedComponents: ["workflow.3d"],
    } as unknown as WizardState} update={update} />);

    fireEvent.click(screen.getByRole("button", { name: "Configure later" }));

    expect(update).toHaveBeenCalledWith({ meshKeyDraft: "", meshKeyStatus: "present", screen: "mcp" });
    expect(update.mock.calls[0][0]).not.toHaveProperty("meshCredentialReference");
  });

  it("ends a managed removal with a removal summary instead of readiness checks", () => {
    render(<Ready state={{ ...readyState(), readiness: null, removalSummary: { removed: 1239, kept: 5 } }} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Setup removed" })).toBeInTheDocument();
    expect(screen.getByText("Removed 1,239 unchanged setup files and the folders they emptied.")).toBeInTheDocument();
    expect(screen.getByText("Kept 5 files you changed. Review them before deleting.")).toBeInTheDocument();
    expect(screen.queryByText("Project and descriptors")).not.toBeInTheDocument();
  });

  it("keeps 3D readiness in the report without a redundant Ready-screen action", () => {
    render(<Ready state={{ ...readyState(), meshSelected: true } as unknown as WizardState} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.getByText("3D model workflow")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /check 3d setup/i })).not.toBeInTheDocument();
  });

  it("explains when core readiness passes without an installed Codex integration", () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    render(<Ready state={{
      ...readyState(),
      readiness: { openInCodex: false, coreReady: true, blockingCheckIds: ["codex.config"], checks: [] },
    }} update={vi.fn()} onMaintenance={vi.fn()} />);

    expect(screen.getByText("Install the Codex configuration component to enable this action.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open in codex/i })).toBeDisabled();
  });

  it("shows selected portrait provider guidance only after core setup succeeds", () => {
    const { rerender } = render(<Ready state={{ ...readyState(), readiness: null }} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.queryByRole("link", { name: /portrait workflow/i })).not.toBeInTheDocument();

    rerender(<Ready state={{ ...readyState(), readiness: { openInCodex: false, coreReady: false, blockingCheckIds: ["descriptor.project"], checks: [] } }} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.queryByRole("link", { name: /portrait workflow/i })).not.toBeInTheDocument();

    rerender(<Ready state={{ ...readyState(), portraitPipeline: { enabled: true, provider: "cloud" } } as unknown as WizardState} update={vi.fn()} onMaintenance={vi.fn()} />);
    const links = screen.getAllByRole("link", { name: "Portrait production source and setup guidance" });
    expect(links).toHaveLength(1);
    expect(links[0]).toHaveAttribute("href", "https://github.com/klimPaskov/comfyui-hoi4-portraits");
    expect(screen.getByText(/Comfy Cloud: Not reported/i)).toBeInTheDocument();
  });

  it("keeps optional ChatGPT source status out of Ready and its details", () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    const state = readyState();
    render(<Ready state={{ ...state, readiness: { ...state.readiness!, checks: [...state.readiness!.checks, { id: "codex.chat_flatten", label: "ChatGPT Chat sources", status: "incomplete", blocking: false, message: "Optional source folder needs review" }] } }} update={vi.fn()} onMaintenance={vi.fn()} />);
    fireEvent.click(screen.getByText("Readiness details"));
    expect(screen.queryByText(/ChatGPT.*sources/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /ChatGPT/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open in Codex/i })).toBeEnabled();
  });

  it("moves focus to the new screen heading and names scan progress", async () => {
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    expect(screen.getByRole("heading", { name: "Describe the mod" })).toHaveFocus();

    cleanup();
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /import existing mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    expect(screen.getByRole("heading", { name: "Project identity" })).toHaveFocus();
    expect(screen.queryByText("Descriptors valid")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\temp\\hoi4-mod" } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("progressbar", { name: "Project scan progress" })).toHaveAttribute("aria-valuetext", "Scan in progress");
  });

  it("fills generated identity fields from the mod name and description", async () => {
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.change(screen.getByLabelText("Mod name"), { target: { value: "Iron Dawn" } });
    fireEvent.change(screen.getByLabelText("Mod description"), { target: { value: "An alternate-history event and decisions mod with new countries." } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));

    expect(screen.getByLabelText("Project ID")).toHaveValue("iron_dawn");
    expect(screen.getByLabelText("Script prefix")).toHaveValue("id");
    expect(screen.getByLabelText("Primary namespace")).toHaveValue("id");
    expect(screen.getByRole("group", { name: "Descriptor tags" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Alternative History" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Events" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Gameplay" })).toBeChecked();
    expect(screen.getByLabelText("Initial folders")).toHaveValue("common, events, localisation/english, gfx, interface, docs, history");

    fireEvent.change(screen.getByLabelText("Script prefix"), { target: { value: "iron" } });
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    fireEvent.change(screen.getByLabelText("Mod description"), { target: { value: "A focused portrait workflow." } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByLabelText("Script prefix")).toHaveValue("iron");
    expect(screen.getByRole("checkbox", { name: "Graphics" })).toBeChecked();
  });

  it("shows progress and an actionable error when Description planning does not complete", async () => {
    enableTauriRuntime();
    let finishAnalysis: ((value: { value: CodexAnalysisResult | null; error?: string }) => void) | undefined;
    vi.mocked(runCodexAnalysisResult).mockReturnValue(new Promise<{ value: CodexAnalysisResult | null; error?: string }>((resolve) => {
      finishAnalysis = resolve;
    }));

    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.change(screen.getByLabelText("Mod name"), { target: { value: "Iron Dawn" } });
    fireEvent.change(screen.getByLabelText("Mod description"), { target: { value: "An alternate-history event mod." } });
    fireEvent.click(screen.getByRole("button", { name: "Next" }));

    const pending = await screen.findByRole("button", { name: "Preparing details…" });
    expect(pending).toBeDisabled();
    expect(pending).toHaveAttribute("aria-busy", "true");
    const planningProgress = screen.getByRole("progressbar", { name: "Mod detail preparation progress" });
    expect(planningProgress).toHaveAttribute("aria-valuenow", "20");
    expect(planningProgress).toHaveAttribute("aria-valuetext", "20% complete. Generating project details. Estimated time: About 2 minutes remaining");
    expect(screen.getByText("Preparing your mod details")).toBeInTheDocument();
    expect(screen.getByText("20%")).toBeInTheDocument();
    expect(screen.getByText("Estimated time: About 2 minutes remaining")).toBeInTheDocument();

    finishAnalysis?.({ value: null, error: "The installed Codex version rejected the planning request." });
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("The installed Codex version rejected the planning request."));
    expect(screen.queryByRole("progressbar", { name: "Mod detail preparation progress" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next" })).toBeEnabled();
    expect(screen.getByLabelText("Mod description")).toHaveValue("An alternate-history event mod.");
  });

  it("shows an immediate pending state while reviewing an existing project", async () => {
    enableTauriRuntime();
    let finishAnalysis: ((value: { value: CodexAnalysisResult | null; error?: string }) => void) | undefined;
    vi.mocked(scanProject).mockResolvedValue({
      findings: [{ id: "descriptor.name", label: "Mod name", value: "Existing mod", evidence: "name=Existing mod", evidencePath: "descriptor.mod", evidenceExcerpt: "name=Existing mod", confidence: 1, status: "accepted" }],
      scanId: "scan-1", projectRoot: "C:\\mods\\existing", partial: false, cancelled: false, limitsHit: [], filesScanned: 1, directoriesScanned: 1, bytesRead: 24, completedAt: "2026-10-13T00:00:00Z",
    } as never);
    vi.mocked(runCodexAnalysisResult).mockReturnValue(new Promise<{ value: CodexAnalysisResult | null; error?: string }>((resolve) => {
      finishAnalysis = resolve;
    }));

    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /import existing mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\existing" } });
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Scanning project" });
    await screen.findByText("Scan evidence saved for review");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Confirm scan findings" });

    fireEvent.click(screen.getByRole("button", { name: "Review with Codex" }));
    const pending = await screen.findByRole("button", { name: "Reviewing with Codex…" });
    expect(pending).toBeDisabled();
    expect(pending).toHaveAttribute("aria-busy", "true");

    await act(async () => finishAnalysis?.({ value: null, error: "temporary provider failure" }));
    expect(await screen.findByRole("button", { name: "Review with Codex" })).toBeEnabled();
  });

  it("distinguishes unavailable binary previews from missing conflict content", async () => {
    vi.mocked(previewInstallationConflict).mockResolvedValue({
      path: "descriptor.mod",
      kind: "binary",
      base: null,
      local: null,
      incoming: null,
      base_sha256: null,
      local_sha256: "local-hash",
      incoming_sha256: "incoming-hash",
      truncated: false,
      redacted: false,
    });
    const state = {
      ...initialState,
      plan: {
        plan_id: "conflict-plan",
        conflicts: [{ id: "descriptor", path: "descriptor.mod", options: ["keep", "replace"], selected: null }],
        operations: [{ id: "descriptor", destination: "descriptor.mod", action: "replace", local_state: "modified", local_sha256: "local-hash" }],
      },
    } as unknown as WizardState;

    render(<Conflict state={state} update={vi.fn()} onChoice={vi.fn()} />);

    expect(await screen.findByText(/Binary\/non-text content preview unavailable/)).toBeInTheDocument();
    expect(screen.getByText(/SHA-256: local-hash/)).toBeInTheDocument();
    expect(screen.getByText(/SHA-256: incoming-hash/)).toBeInTheDocument();
    expect(screen.getByText(/No original content is available/)).toBeInTheDocument();
    expect(screen.queryByText(/No current content is present/)).not.toBeInTheDocument();
    expect(screen.queryByText(/No new text preview is available/)).not.toBeInTheDocument();
  });

  it("keeps imported conflicts in the setup rail and labels the non-maintenance action", () => {
    const plan = {
      plan_id: "conflict-plan",
      conflicts: [{ id: "descriptor", path: "descriptor.mod", options: ["keep", "replace"], selected: "keep" }],
      operations: [],
    };
    window.__HOI4_DOCUMENTATION_STATE__ = { ...initialState, screen: "conflict", plan: plan as never };
    render(<App />);

    expect(screen.getByRole("navigation", { name: "Setup phases" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Maintenance phases" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Review changes" })).toBeEnabled();

    cleanup();
    window.__HOI4_DOCUMENTATION_STATE__ = { ...initialState, screen: "conflict", maintenanceMode: "repair", plan: plan as never };
    render(<App />);

    expect(screen.getByRole("navigation", { name: "Maintenance phases" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply" })).toBeEnabled();
  });

  it("shows a concise all-resolved conflict status while retaining prior choices", () => {
    const state = {
      ...initialState,
      conflictChoice: "replace",
      plan: {
        plan_id: "resolved-conflict-plan",
        conflicts: [{ id: "descriptor", path: "descriptor.mod", options: ["keep", "replace"], selected: "keep" }],
        operations: [],
      },
    } as unknown as WizardState;

    render(<Conflict state={state} update={vi.fn()} onChoice={vi.fn()} />);

    expect(screen.getByText(/All conflicts are resolved\. You can review or change an earlier choice below\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep current" })).toHaveClass("primary");
    expect(screen.getByRole("button", { name: "Use new" })).toHaveClass("secondary");
  });

  it("shows correlated scan stage, path, counters, and cancellation", () => {
    const progress: ScanProgress = { stage: "detecting_git", currentPath: ".git", filesScanned: 12, directoriesScanned: 4, bytesRead: 4096 };
    const onCancel = vi.fn().mockResolvedValue(undefined);
    render(<Scan state={{ mode: "existing" } as unknown as WizardState} complete={false} progress={progress} partial={false} limitsHit={[]} canCancel cancellationRequested={false} onCancel={onCancel} />);

    expect(screen.getByText("Checking Git state")).toBeInTheDocument();
    expect(screen.getByText(".git")).toBeInTheDocument();
    expect(screen.getByText(/12 setup files, 4 setup directories/)).toBeInTheDocument();
    expect(screen.queryByText(/files indexed/)).not.toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Project scan progress" })).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByRole("progressbar", { name: "Project scan progress" })).toHaveAttribute("aria-valuetext", "Scan in progress");
    fireEvent.click(screen.getByRole("button", { name: "Cancel scan" }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("separates indexed project files and content samples from setup files during profiling", () => {
    const progress: ScanProgress = {
      stage: "profiling_project",
      currentPath: "common/national_focus/germany.txt",
      filesScanned: 12,
      directoriesScanned: 4,
      bytesRead: 3 * 1024 * 1024,
      profileStats: { files_indexed: 842, directories_indexed: 97, samples_read: 31, bytes_read: 3 * 1024 * 1024 - 4096, excluded_entries: 5, coverage: "complete" },
    };
    render(<Scan state={{ mode: "existing" } as unknown as WizardState} complete={false} progress={progress} partial={false} limitsHit={[]} canCancel cancellationRequested={false} onCancel={vi.fn().mockResolvedValue(undefined)} />);

    expect(screen.getByText("common/national_focus/germany.txt")).toBeInTheDocument();
    expect(screen.getByText(/842 files indexed, 31 content samples, 12 setup files/)).toBeInTheDocument();
    expect(screen.queryByText(/setup directories/)).not.toBeInTheDocument();
    expect(screen.getByText(/3\.0 MB read/)).toBeInTheDocument();
    expect(screen.getByText("Profiling the mod")).toBeInTheDocument();
  });

  it("keeps a truncated scan visibly partial and does not call it a full scan", () => {
    render(<Scan state={{ mode: "existing" } as unknown as WizardState} complete progress={{ stage: "complete", currentPath: ".", filesScanned: 20, directoriesScanned: 8, bytesRead: 8192 }} partial limitsHit={["scan.file_limit"]} canCancel={false} cancellationRequested={false} onCancel={vi.fn().mockResolvedValue(undefined)} />);

    expect(screen.getByText("Scan incomplete")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(/partial result/i);
    expect(screen.getByText(/Partial scan evidence saved for review/)).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Project scan progress" })).toHaveAttribute("aria-valuetext", "Partial scan complete");
  });

  it("exposes the native project-folder picker with an accessible name", async () => {
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.change(screen.getByLabelText("Mod name"), { target: { value: "Iron Dawn" } });
    fireEvent.change(screen.getByLabelText("Mod description"), { target: { value: "An alternate-history mod." } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("button", { name: "Change location project folder" })).toBeInTheDocument();
  });

  it("previews each generated text or image file after identity confirmation", async () => {
    enableTauriRuntime();
    vi.mocked(previewDescriptorsResult).mockResolvedValue({
      value: [
        { component_id: "project.descriptor", destination: "descriptor.mod", content: "name=\"Atlantis\"", expected_sha256: "a".repeat(64) },
        { component_id: "project.launcher_descriptor", destination: "C:\\mods\\atlantis.mod", content: "path=\"C:/mods/atlantis\"", expected_sha256: "b".repeat(64), external: true },
        { component_id: "project.thumbnail", destination: "thumbnail.png", content: "Generated placeholder", expected_sha256: "c".repeat(64), bytes: [137, 80, 78, 71] },
      ],
    });
    const state = {
      mode: "new",
      identity: {
        displayName: "Atlantis",
        projectId: "atlantis",
        author: "",
        version: "0.1.0",
        supportedGameVersion: "1.17.*",
        projectRoot: "C:\\mods\\atlantis",
        launcherDescriptorPath: "C:\\mods\\atlantis.mod",
        defaultBranch: "main",
        scriptPrefix: "atlantis",
        primaryNamespace: "atlantis",
        descriptorTags: ["Alternative History"],
      },
      folderProfile: ["common"],
      codexAnalysisRecord: { confirmed_fields: ["project_id"] },
    } as unknown as WizardState;

    render(<Identity state={state} update={vi.fn()} updateIdentity={vi.fn()} onPickProjectFolder={vi.fn().mockResolvedValue(null)} onPickLauncherFolder={vi.fn().mockResolvedValue(null)} onConfirmAnalysis={vi.fn().mockResolvedValue(undefined)} />);

    fireEvent.click(screen.getAllByRole("button", { name: "Preview" })[0]);
    expect(await screen.findByText('name="Atlantis"')).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "Preview" })[2]);
    const image = await screen.findByRole("img", { name: "Generated thumbnail placeholder preview" });
    expect(image).toHaveAttribute("src", expect.stringMatching(/^data:image\/png;base64,/));
  });

  it("uses automated new-project paths and exposes the matching launcher descriptor", async () => {
    enableTauriRuntime();
    vi.mocked(suggestProjectPaths).mockResolvedValue({
      mod_directory: "C:\\mods",
      project_root: "C:\\mods\\atlantis_rising",
      launcher_descriptor_path: "C:\\mods\\atlantis_rising.mod",
      project_exists: false,
      launcher_descriptor_exists: false,
    });
    vi.mocked(runCodexAnalysisResult).mockResolvedValue({
      value: {
        analysis: {
          schema_version: "1.0.0",
          analysis_id: "analysis-1",
          mode: "new_project_identity",
          input_sha256: "a".repeat(64),
          project_summary: "A test project",
          proposals: [],
          component_recommendations: [],
          warnings: [],
        },
        record: {
          engine: "codex",
          auth_mode: "chatgpt",
          provider: "codex",
          model: "default",
          analysis_id: "analysis-1",
          schema_version: "1.0.0",
          input_sha256: "a".repeat(64),
          output_sha256: "b".repeat(64),
          confirmed_fields: ["display_name"],
          confirmed_at: "2026-07-29T00:00:00Z",
        },
      } as CodexAnalysisResult,
    });

    await renderAuthenticatedApp();
    await waitFor(() => expect(suggestProjectPaths).toHaveBeenCalledWith("atlantis_rising"));
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() => expect(screen.getByLabelText("Project folder")).toHaveValue("C:\\mods\\atlantis_rising"));
    fireEvent.click(screen.getByText("Launcher file location"));
    expect(screen.getByLabelText("Launcher descriptor path")).toHaveValue("C:\\mods\\atlantis_rising.mod");
  });

  it("does not open recovery for an automatically suggested new-project destination", async () => {
    enableTauriRuntime();
    vi.mocked(suggestProjectPaths).mockResolvedValue({
      mod_directory: "C:\\mods",
      project_root: "C:\\mods\\new_mod",
      launcher_descriptor_path: "C:\\mods\\new_mod.mod",
      project_exists: false,
      launcher_descriptor_exists: false,
    });
    vi.mocked(findInterruptedTransaction).mockResolvedValue({
      transaction_id: "stale-transaction",
      state: "staging",
      last_checkpoint: "stage-file-op-1",
      recovery: {
        resume_allowed: true,
        rollback_allowed: false,
        discard_staging_allowed: true,
        project_apply_started: false,
        recommended_action: "resume",
      },
    } as never);
    vi.mocked(runCodexAnalysisResult).mockResolvedValue({
      value: {
        analysis: {
          schema_version: "1.0.0",
          analysis_id: "analysis-new-project-recovery",
          mode: "new_project_identity",
          input_sha256: "a".repeat(64),
          project_summary: "A test project",
          proposals: [],
          component_recommendations: [],
          warnings: [],
        },
        record: {
          engine: "codex",
          auth_mode: "chatgpt",
          provider: "codex",
          model: "default",
          analysis_id: "analysis-new-project-recovery",
          schema_version: "1.0.0",
          input_sha256: "a".repeat(64),
          output_sha256: "b".repeat(64),
          confirmed_fields: ["display_name"],
          confirmed_at: "2026-07-29T00:00:00Z",
        },
      } as CodexAnalysisResult,
    });

    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() => expect(screen.getByLabelText("Project folder")).toHaveValue("C:\\mods\\new_mod"));

    expect(findInterruptedTransaction).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { name: "Installation was interrupted" })).not.toBeInTheDocument();
  });

  it("presents provider proposals with readable labels and separated values", async () => {
    enableTauriRuntime();
    vi.mocked(runCodexAnalysisResult).mockResolvedValue({
      value: {
        analysis: {
          schema_version: "1.0.0",
          analysis_id: "analysis-1",
          mode: "new_project_identity",
          input_sha256: "a".repeat(64),
          project_summary: "A focused alternate-history project.",
          proposals: [
            {
              key: "display_name",
              value: "Iron Dawn",
              confidence: 0.96,
              reason: "Uses the supplied mod name.",
              evidence_refs: [],
            },
            {
              key: "descriptor_tags",
              value: ["Alternative History", "Events"],
              confidence: 0.88,
              reason: "Matches the requested focus.",
              evidence_refs: [],
            },
          ],
          component_recommendations: [],
          warnings: [],
        },
        record: {
          engine: "codex",
          auth_mode: "chatgpt",
          provider: "codex",
          model: "default",
          analysis_id: "analysis-1",
          schema_version: "1.0.0",
          input_sha256: "a".repeat(64),
          output_sha256: "b".repeat(64),
          confirmed_fields: [],
          confirmed_at: "2026-07-29T00:00:00Z",
        },
      } as CodexAnalysisResult,
    });

    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByLabelText("Mod name"), { target: { value: "Iron Dawn" } });
    fireEvent.change(screen.getByLabelText("Mod description"), { target: { value: "An alternate-history event mod." } });
    fireEvent.click(screen.getByRole("button", { name: "Next" }));

    expect(await screen.findByText("2 suggested values")).toBeInTheDocument();
    const review = screen.getByRole("region", { name: "Codex proposal review" });
    expect(within(review).getByText("Mod name")).toBeInTheDocument();
    expect(within(review).getByText("Descriptor tags")).toBeInTheDocument();
    expect(within(review).getByText("Iron Dawn")).toBeInTheDocument();
    expect(within(review).getByText("Alternative History, Events")).toBeInTheDocument();
    expect(within(review).queryByText("display_name")).not.toBeInTheDocument();
    expect(within(review).queryByText("descriptor_tags")).not.toBeInTheDocument();
  });

  it("keeps edited suggestions and paths confirmable without rerunning analysis", async () => {
    enableTauriRuntime();
    const analysis = {
      ...semanticAnalysisFixture(),
      analysis_id: "analysis-edit",
      proposals: semanticAnalysisFixture().proposals.map((proposal) => proposal.key === "descriptor_tags"
        ? { ...proposal, value: ["Alternative History", "Military"] }
        : proposal),
      component_recommendations: [],
    };
    const record = { engine: "codex_app_server", auth_mode: "chatgpt", provider: "codex", model: "gpt-5.6-luna", analysis_id: "analysis-edit", schema_version: "1.0.0", input_sha256: "a".repeat(64), output_sha256: "b".repeat(64), confirmed_fields: [], confirmed_at: "" };
    vi.mocked(runCodexAnalysisResult).mockResolvedValue({ value: { analysis, record } as unknown as CodexAnalysisResult });
    vi.mocked(confirmCodexAnalysis).mockResolvedValue({ ...record, confirmed_fields: analysis.proposals.map((proposal) => proposal.key), confirmed_at: "2026-10-03T00:00:00Z" } as never);

    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.change(screen.getByLabelText("Mod name"), { target: { value: "Iron Dawn" } });
    fireEvent.change(screen.getByLabelText("Mod description"), { target: { value: "An alternate-history military mod." } });
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByText("10 suggested values");
    expect(vi.mocked(runCodexAnalysisResult).mock.calls[0][0].constraints).toMatchObject({ requested_mod_name: "Iron Dawn" });

    const review = screen.getByRole("region", { name: "Codex proposal review" });
    fireEvent.change(within(review).getAllByRole("textbox")[0], { target: { value: "Iron Dawn Reforged" } });
    expect(within(review).getByDisplayValue("Alternative History, Military")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm Codex suggestions" }));

    await waitFor(() => expect(confirmCodexAnalysis).toHaveBeenCalledTimes(1));
    expect(vi.mocked(confirmCodexAnalysis).mock.calls[0][0]).toMatchObject({ analysis_id: "analysis-edit" });
    expect(vi.mocked(confirmCodexAnalysis).mock.calls[0][2]).toMatchObject({ identity: expect.objectContaining({ displayName: "Iron Dawn Reforged" }) });

    const edits = [
      { label: "Project folder", field: "projectRoot", value: "C:/mods/iron_dawn_reforged" },
      { label: "Launcher descriptor path", field: "launcherDescriptorPath", value: "C:/mods/iron_dawn_reforged.mod", disclosure: "Launcher file location" },
      { label: "Author", field: "author", value: "Mod team", disclosure: "Advanced project metadata" },
    ];
    for (const [index, edit] of edits.entries()) {
      await waitFor(() => expect(screen.queryByRole("button", { name: "Confirm Codex suggestions" })).not.toBeInTheDocument());
      if (edit.disclosure) fireEvent.click(screen.getByText(edit.disclosure));
      fireEvent.change(screen.getByLabelText(edit.label), { target: { value: edit.value } });
      const confirm = screen.getByRole("button", { name: "Confirm Codex suggestions" });
      expect(confirm).toBeEnabled();
      for (const preview of screen.getAllByRole("button", { name: "Preview" })) expect(preview).toBeDisabled();
      fireEvent.click(screen.getByRole("button", { name: "Next" }));
      expect(screen.getByRole("heading", { name: "Project identity" })).toBeInTheDocument();
      fireEvent.click(confirm);
      await waitFor(() => expect(confirmCodexAnalysis).toHaveBeenCalledTimes(index + 2));
      expect(vi.mocked(confirmCodexAnalysis).mock.calls[index + 1][0]).toMatchObject({ analysis_id: "analysis-edit" });
      expect(vi.mocked(confirmCodexAnalysis).mock.calls[index + 1][2]).toMatchObject({ identity: expect.objectContaining({ [edit.field]: edit.value }) });
    }
    expect(runCodexAnalysisResult).toHaveBeenCalledTimes(1);
  });

  it("renders all ten semantic proposals as editable accessible review fields", () => {
    const analysis = semanticAnalysisFixture();
    function ControlledIdentity() {
      const [state, setState] = useState({
        ...initialState,
        screen: "identity" as const,
        mode: "new" as const,
        projectPathStatus: "manual" as const,
        identity: { ...initialState.identity, projectRoot: "C:\\mods\\iron_dawn", launcherDescriptorPath: "C:\\mods\\iron_dawn.mod" },
        codexAnalysis: analysis as unknown as WizardState["codexAnalysis"],
        codexAnalysisRecord: { engine: "codex_app_server", auth_mode: "chatgpt", analysis_id: "semantic-analysis", schema_version: "1.0.0", input_sha256: "a".repeat(64), output_sha256: "b".repeat(64), confirmed_fields: [], confirmed_at: "pending" },
        conventions: { agents_profile: "default", localisation_convention: "english", documentation_convention: "markdown" },
      } as WizardState);
      return <Identity state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} updateIdentity={(patch) => setState((current) => ({ ...current, identity: { ...current.identity, ...patch } }))} updateDescription={(description) => setState((current) => ({ ...current, description }))} onPickProjectFolder={vi.fn().mockResolvedValue(null)} onPickLauncherFolder={vi.fn().mockResolvedValue(null)} onConfirmAnalysis={vi.fn().mockResolvedValue(undefined)} />;
    }

    render(<ControlledIdentity />);

    expect(screen.getAllByText("Suggested", { exact: true })).toHaveLength(10);
    expect(screen.getAllByText("Why this was suggested", { selector: "summary" })).toHaveLength(10);
    expect(screen.getByLabelText("Project guidance")).toHaveValue("default");
    expect(screen.getByLabelText("Localisation style")).toHaveValue("english");
    expect(screen.getByLabelText("Documentation style")).toHaveValue("markdown");

    fireEvent.change(screen.getByLabelText("Project guidance"), { target: { value: "events-first" } });
    expect(screen.getByLabelText("Project guidance")).toHaveValue("events-first");
    fireEvent.click(screen.getAllByText("Why this was suggested", { selector: "summary" })[0]);
    expect(screen.getByText("Reason for display_name.")).toBeInTheDocument();
  });

  it("builds the confirmation payload with conventions and component recommendations", () => {
    const analysis = semanticAnalysisFixture() as unknown as WizardState["codexAnalysis"];
    const state = {
      ...initialState,
      description: "Edited description",
      folderProfile: ["common", "events"],
      codexAnalysis: analysis,
      conventions: { agents_profile: "events-first", localisation_convention: "english", documentation_convention: "markdown" },
      semanticComponentRecommendations: [{ component_id: "core.skills", recommendation: "recommended", reason: "Keep the workflow skills." }],
    } as WizardState;

    expect(buildAnalysisConfirmationValues(state)).toMatchObject({
      description: "Edited description",
      folderProfile: ["common", "events"],
      conventions: { agents_profile: "events-first", localisation_convention: "english", documentation_convention: "markdown" },
      componentRecommendations: [{ component_id: "core.skills", recommendation: "recommended", reason: "The brief calls for repeated HOI4 workflow support." }],
    });
    expect(Object.keys(buildAnalysisConfirmationValues(state))).toEqual(["description", "folderProfile", "identity", "conventions", "componentRecommendations"]);
  });

  it("shows component recommendations by manifest display name without selecting them", async () => {
    const manifest = {
      schema_version: "1.0.0",
      manifest_id: "recommendation-fixture",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [{ id: "core.skills", display_name: "HOI4 Skills", description: "Workflow skills", category: "skill", optional: true, platforms: ["all"], source: { kind: "tree", path: ".agents/skills" }, destination: { path: ".agents/skills/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [], capabilities: [], validation: [], update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true } }],
      profiles: [],
    } as unknown as SourceManifestPreview;
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: manifest });
    function ControlledComponents() {
      const [state, setState] = useState({
        ...initialState,
        sourceMode: "latest" as const,
        pinnedRef: "",
        selectedComponents: [],
        codexAnalysis: { ...semanticAnalysisFixture(), component_recommendations: [{ component_id: "core.skills", recommendation: "recommended", reason: "Keep the workflow skills." }] } as unknown as WizardState["codexAnalysis"],
        semanticComponentRecommendations: [{ component_id: "core.skills", recommendation: "recommended", reason: "Keep the workflow skills." }],
      } as WizardState);
      return <><Components state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} /><output aria-label="selected components">{state.selectedComponents.join(",")}</output></>;
    }

    render(<ControlledComponents />);
    const summary = await screen.findByRole("region", { name: "Setup assistant recommendations" });
    expect(within(summary).getByText("HOI4 Skills")).toBeInTheDocument();
    expect(within(summary).getByText("Keep the workflow skills.")).toBeInTheDocument();
    expect(screen.getByLabelText("selected components")).toHaveTextContent("");
  });

  it("announces a live catalog that omits the selected model and recovers when a listed model is chosen", async () => {
    enableTauriRuntime();
    vi.mocked(readAiModels).mockResolvedValue([{ id: "listed-model", display_name: "Listed model", default_reasoning_effort: "high", supported_reasoning_efforts: ["high"] }]);
    function ControlledWelcome() {
      const [state, setState] = useState({ ...welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: false }), aiModelChoiceExplicit: true });
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    expect(await screen.findByRole("alert")).toHaveTextContent(/not in the live catalog/i);
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "listed-model" } });
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(screen.getByLabelText("Reasoning effort")).toHaveValue("high");
  });

  it("shows the bounded launcher candidate and allows scanning without it", async () => {
    vi.mocked(pickProjectFolder).mockResolvedValue({
      path: "C:\\mods\\existing",
      launcher_descriptor_path: "C:\\mods\\existing.mod",
      cancelled: false,
    } as FolderSelection);
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /import existing mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Browse project folder" }));
    await waitFor(() => expect(screen.getByText("C:\\mods\\existing.mod")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Scan without launcher file" }));
    expect(screen.queryByText("C:\\mods\\existing.mod")).not.toBeInTheDocument();
    expect(screen.getByText("The scan will continue without an external launcher file.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Browse project folder" }));
    await waitFor(() => expect(screen.getByText("C:\\mods\\existing.mod")).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\other" } });
    expect(screen.queryByText("C:\\mods\\existing.mod")).not.toBeInTheDocument();
  });

  it("reports an ambiguous launcher registration while keeping internal-only scan available", async () => {
    vi.mocked(pickProjectFolder).mockResolvedValue({
      path: "C:\\mods\\existing",
      launcher_descriptor_path: null,
      error: "multiple launcher descriptors register this project: C:\\mods\\existing.mod, C:\\mods\\legacy.mod",
      cancelled: false,
    });
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /import existing mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Browse project folder" }));

    expect(await screen.findByText(/Launcher discovery needs review: multiple launcher descriptors/)).toBeInTheDocument();
    expect(screen.getByLabelText("Project folder")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "Scan without launcher file" }));
    expect(screen.getByLabelText("Project folder")).toHaveValue("C:\\mods\\existing");
  });

  it("opens the source repository through the desktop browser bridge", async () => {
    enableTauriRuntime();
    render(<App />);

    fireEvent.click(screen.getByRole("link", { name: /HOI4 Mod Setup/i }));

    await waitFor(() => expect(openExternalUrlResult).toHaveBeenCalledWith("https://github.com/klimPaskov/HOI4-Mod-Setup"));
  });

  it("keeps signed-out project management reachable from the welcome screen", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    expect(screen.getByRole("heading", { name: "Project identity" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\installed" } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("heading", { name: "Scanning project" })).toBeInTheDocument();
  });

  it("allows project-management scans to exclude a discovered launcher candidate", async () => {
    vi.mocked(pickProjectFolder).mockResolvedValue({
      path: "C:\\mods\\installed",
      launcher_descriptor_path: "C:\\mods\\installed.mod",
      cancelled: false,
    } as FolderSelection);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    fireEvent.click(screen.getByRole("button", { name: "Browse project folder" }));

    expect(await screen.findByText("C:\\mods\\installed.mod")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Scan without launcher file" }));
    expect(screen.queryByText("C:\\mods\\installed.mod")).not.toBeInTheDocument();
    expect(screen.getByText("The scan will continue without an external launcher file.")).toBeInTheDocument();
  });

  it("shows only Undo after project apply has started", async () => {
    vi.mocked(findInterruptedTransaction).mockResolvedValue({
      transaction_id: "6a26d121-5bdc-4d0a-9d2f-6e3168ca7528",
      state: "applying",
      last_checkpoint: "apply-intent-op-00007",
      recovery: {
        resume_allowed: false,
        rollback_allowed: true,
        discard_staging_allowed: false,
        project_apply_started: true,
        recommended_action: "rollback",
      },
    } as never);

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\partial" } });

    expect(await screen.findByText("Some project files were already changed, so continuing automatically is unavailable.")).toBeInTheDocument();
    expect(screen.getByText("Undo the partial setup before installing again.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /continue setup/i })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /undo changes/i }).find((button) => button.classList.contains("recovery-card"))).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: /discard prepared files/i })).not.toBeInTheDocument();
  });

  it("shows live work and blocks duplicate clicks while undo is running", async () => {
    let finishRollback: ((result: Awaited<ReturnType<typeof rollbackInstallationResult>>) => void) | undefined;
    vi.mocked(findInterruptedTransaction).mockResolvedValue({
      transaction_id: "partial-transaction",
      rollback_transaction_id: "rollback-child",
      state: "rolling_back",
      last_checkpoint: "apply-op-00007",
      recovery: {
        resume_allowed: false,
        rollback_allowed: true,
        discard_staging_allowed: false,
        project_apply_started: true,
        recommended_action: "rollback",
      },
    } as never);
    vi.mocked(rollbackInstallationResult).mockImplementation(() => new Promise((resolve) => {
      finishRollback = resolve;
    }));
    vi.mocked(readTransactionJournal).mockImplementation(async (_root, transactionId) => transactionId === "rollback-child" ? ({
      transaction_id: "rollback-child",
      transaction_kind: "rollback",
      state: "preflight",
      operations: [
        { id: "rollback-1", action: "replace", status: "pending", destination: "AGENTS.md", backup_path: "backup/1.bak" },
        { id: "rollback-2", action: "replace", status: "pending", destination: "README.md", backup_path: null, after_exists: null },
      ],
    } as never) : ({ rollback_transaction_id: "rollback-child" } as never));

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\partial" } });
    const primary = (await screen.findAllByRole("button", { name: "Undo changes" })).find((button) => button.classList.contains("primary"));
    expect(primary).toBeDefined();
    fireEvent.click(primary!);

    expect(await screen.findByRole("button", { name: "Undoing changes…" })).toBeDisabled();
    expect(await screen.findByText("Saving file 1 of 2")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Recovery progress" })).toHaveAttribute("aria-valuenow", "50");

    vi.mocked(findInterruptedTransaction).mockResolvedValue(null);
    await act(async () => finishRollback?.({ value: { transaction_id: "partial-transaction", state: "rolled_back" } as never }));
    expect(await screen.findByRole("heading", { name: "Start a mod project" })).toBeInTheDocument();
    expect(screen.getByText("The partial setup was undone. You can start again.")).toBeInTheDocument();
  });

  it("redacts raw rollback command errors before displaying them", async () => {
    const secretName = ["private", "key"].join("_");
    const secretValue = ["synthetic", "rollback", "credential"].join("-");
    vi.mocked(findInterruptedTransaction).mockResolvedValue({
      transaction_id: "partial-transaction",
      state: "applying",
      last_checkpoint: "apply-op-00007",
      recovery: {
        resume_allowed: false,
        rollback_allowed: true,
        discard_staging_allowed: false,
        project_apply_started: true,
        recommended_action: "rollback",
      },
    } as never);
    vi.mocked(rollbackInstallationResult).mockResolvedValue({
      value: null,
      error: `provider failed; {"${secretName}":"${secretValue}"}`,
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\partial" } });
    const primary = (await screen.findAllByRole("button", { name: "Undo changes" })).find((button) => button.classList.contains("primary"));
    fireEvent.click(primary!);

    const error = await screen.findByText(/Undo could not continue:/);
    expect(error).toHaveTextContent("[REDACTED]");
    expect(screen.queryByText(secretValue)).not.toBeInTheDocument();
  });

  it("shows only Discard when setup stopped before changing project files", async () => {
    vi.mocked(findInterruptedTransaction).mockResolvedValue({
      transaction_id: "pre-apply-transaction",
      state: "staging",
      last_checkpoint: "stage-file-op-00007",
      recovery: {
        resume_allowed: false,
        rollback_allowed: false,
        discard_staging_allowed: true,
        project_apply_started: false,
        recommended_action: "discard_staging",
      },
    } as never);

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\prepared-only" } });

    expect(await screen.findByText("Setup stopped before any project files were changed.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /continue setup/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /undo changes/i })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /discard prepared files/i }).find((button) => button.classList.contains("recovery-card"))).toHaveAttribute("aria-pressed", "true");
  });

  it("shows the exact validation checkpoint and safe pre-apply recovery choices", async () => {
    const recoverySecret = ["msy", "secretRecoveryValue123456789"].join("_");
    const secondarySecret = ["synthetic", "client", "credential"].join("-");
    const secondaryName = ["client", "secret"].join("_");
    const quotedSecret = ["synthetic", "private", "credential"].join("-");
    const quotedName = ["private", "key"].join("_");
    const stageIds = [
      "preflight", "repository source resolution", "selective download", "checksum verification", "dry-run review", "backup", "staging", "validation", "apply", "post-install checks", "readiness report", "rollback record",
    ];
    vi.mocked(findInterruptedTransaction).mockResolvedValue({
      transaction_id: "validation-transaction",
      state: "validating",
      last_checkpoint: "validation",
      stages: stageIds.map((id, index) => ({ id, status: index < 7 ? "complete" : index === 7 ? "active" : "pending" })),
      operations: [
        { id: "op-1", destination: "AGENTS.md", status: "staged" },
        { id: "op-2", destination: "descriptor.mod", status: "staged" },
        { id: "op-3", destination: "C:\\mods\\atlantis_rising.mod", status: "staged", external: true },
      ],
      recovery: {
        resume_allowed: true,
        rollback_allowed: false,
        discard_staging_allowed: true,
        project_apply_started: false,
        recommended_action: "resume",
      },
      error: {
        code: "transaction_error",
        message: `Launcher descriptor path did not match the selected project root. MESHY_API_KEY=${recoverySecret}; ${secondaryName}=${secondarySecret}; {"${quotedName}":"${quotedSecret}"}; ${"🧪".repeat(2048)}`,
        stage: "validation",
      },
    } as never);

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\atlantis_rising" } });

    expect(await screen.findByText("Stage 8 of 12")).toBeInTheDocument();
    expect(screen.getByText(/Backup checked.*Project changes had not started.*3 files prepared/)).toBeInTheDocument();
    expect(screen.getByText("Check the prepared files", { selector: "code" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /continue setup/i }).find((button) => button.classList.contains("recovery-card"))).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /discard prepared files/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /undo changes/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /manage installed components/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("Details", { selector: "summary" }));
    expect(screen.getByText("Check the prepared files", { selector: "strong" })).toBeInTheDocument();
    const failureMessage = screen.getByText(/Launcher descriptor path did not match the selected project root/);
    expect(failureMessage).toHaveTextContent("[REDACTED]");
    expect(new TextEncoder().encode(failureMessage.textContent ?? "").length).toBeLessThanOrEqual(2048);
    expect(screen.queryByText(recoverySecret)).not.toBeInTheDocument();
    expect(screen.queryByText(secondarySecret)).not.toBeInTheDocument();
    expect(screen.queryByText(quotedSecret)).not.toBeInTheDocument();
  });

  it("shows a distinct usage-limited state and preserves remote logout errors", async () => {
    enableTauriRuntime();
    const update = vi.fn();
    vi.mocked(logoutCodexResult).mockResolvedValue({ value: null, error: "remote logout failed; retry in Codex" });
    vi.mocked(readCodexAccount).mockResolvedValue({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: true, email: "user@example.com" });
    render(<Welcome state={welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: true, email: "user@example.com" })} update={update} />);

    expect(screen.getByRole("status")).toHaveTextContent(/usage is currently limited/i);
    fireEvent.click(screen.getByRole("button", { name: /sign out/i }));

    await waitFor(() => expect(update).toHaveBeenCalledWith(expect.objectContaining({ transactionError: "remote logout failed; retry in Codex" })));
  });

  it("opens the returned login URL through the typed system-browser bridge", async () => {
    enableTauriRuntime();
    const update = vi.fn();
    const url = "https://auth.example/login?state=opaque";
    render(<Welcome state={welcomeState({ available: true, authenticated: false, auth_mode: "chatgpt", usage_limited: false, error: "signed out" }, { available: true, auth_url: url, device_code: false })} update={update} />);
    fireEvent.click(screen.getByRole("button", { name: /open the chatgpt sign-in page/i }));

    await waitFor(() => expect(openCodexLoginUrlResult).toHaveBeenCalledWith(url));
    expect(update).toHaveBeenCalledWith({ transactionError: undefined });
  });

  it("clears a stale availability error as soon as a new sign-in attempt starts", async () => {
    enableTauriRuntime();
    const update = vi.fn();
    vi.mocked(startCodexLogin).mockResolvedValue({ available: true, device_code: false });
    render(<Welcome state={welcomeState({ available: false, authenticated: false, auth_mode: "none", usage_limited: false, error: "Codex is temporarily unavailable." })} update={update} />);

    fireEvent.click(screen.getByRole("button", { name: /sign in with chatgpt/i }));

    await waitFor(() => expect(startCodexLogin).toHaveBeenCalledWith("browser"));
    expect(update).toHaveBeenCalledWith(expect.objectContaining({
      codexLoginPending: true,
      codexAccount: expect.objectContaining({ error: undefined }),
      transactionError: undefined,
    }));
  });

  it("exposes a cancellable pending sign-in state", async () => {
    enableTauriRuntime();
    const update = vi.fn();
    render(<Welcome state={{ ...welcomeState({ available: true, authenticated: false, auth_mode: "chatgpt", usage_limited: false }), codexLoginPending: true, codexLogin: { available: true, login_id: "login-1", device_code: false } }} update={update} />);
    fireEvent.click(screen.getByRole("button", { name: /cancel sign-in/i }));

    await waitFor(() => expect(cancelCodexLogin).toHaveBeenCalledWith("login-1"));
    expect(update).toHaveBeenCalledWith(expect.objectContaining({ codexLoginPending: false, transactionError: expect.stringMatching(/cancelled/i) }));
  });

  it("keeps an unprepared dry run from starting installation", async () => {
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    for (let index = 0; index < 8; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByText("Unresolved conflicts")).toBeInTheDocument();
    expect(screen.getByText("Plan unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /start installation/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /prepare changes/i })).toBeInTheDocument();
  });

  it("presents manifest-declared verification tools as simple setup checks", () => {
    render(<DryRun state={{
      aiProvider: "codex",
      gitBranch: "main",
      gitOnlineAction: "none",
      plan: {
        operations: [],
        conflicts: [],
        external_actions: [
          { id: "mcp", component_id: "mcp.hoi4_agent_tools", risk: "high", requires_approval: true },
          { id: "3d", component_id: "workflow.3d", risk: "high", requires_approval: true },
        ],
      },
    } as unknown as WizardState} update={vi.fn()} />);

    expect(screen.getByText("Setup checks", { selector: "summary" })).toBeInTheDocument();
    expect(screen.getByText("2 included")).toBeInTheDocument();
    expect(screen.queryByText(/high risk|approval required|external tools/i)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Setup checks", { selector: "summary" }));
    expect(screen.getByText("Check the installed AI tools")).toBeInTheDocument();
    expect(screen.getByText("Check the 3D workflow")).toBeInTheDocument();
  });

  it("shows the real file plan only when the user opens it", () => {
    render(<DryRun state={{
      aiProvider: "codex",
      gitBranch: "main",
      gitOnlineAction: "none",
      plan: {
        operations: [
          { id: "agents", action: "create", destination: "AGENTS.md" },
          { id: "skill", action: "replace", destination: ".agents/skills/example/SKILL.md" },
        ],
        transaction: { directories: ["events"] },
        conflicts: [],
        external_actions: [],
      },
    } as unknown as WizardState} update={vi.fn()} />);

    const files = screen.getByText("Files and folders to install · 3", { selector: "summary" });
    expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument();
    const details = files.closest("details")!;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(screen.getByText("AGENTS.md")).toBeInTheDocument();
    expect(screen.getByText(".agents/skills/example/SKILL.md")).toBeInTheDocument();
    expect(screen.getByText("events/")).toBeInTheDocument();
    expect(screen.getByText("Create folder")).toBeInTheDocument();
    expect(screen.queryByText("Open full file plan")).not.toBeInTheDocument();
  });

  it("keeps integration technical details collapsed by default", () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    render(<Mcp state={{
      selectedComponents: ["mcp.hoi4_agent_tools"],
      manifestPreview: {
        components: [{
          id: "mcp.hoi4_agent_tools",
          category: "mcp",
          display_name: "HOI4 Agent Tools",
          description: "Project tools",
          platforms: ["windows"],
          required_tools: [],
          environment: [],
          capabilities: [],
          validation: [],
        }],
      },
      meshSelected: false,
      meshKeyStatus: "missing",
    } as unknown as WizardState} />);

    // Technical requirements stay behind progressive disclosure.
    expect(screen.getByText("Technical details", { selector: "summary" }).closest("details")).not.toHaveAttribute("open");
  });

  it("shows dry-run preparation as busy and enables installation only after a plan succeeds", async () => {
    let finishPlan: ((result: { value: unknown }) => void) | undefined;
    vi.mocked(buildInstallationPlanResult).mockImplementation(() => new Promise((resolve) => { finishPlan = resolve as (result: { value: unknown }) => void; }));
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    for (let index = 0; index < 8; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));

    fireEvent.click(screen.getByRole("button", { name: "Prepare changes" }));
    expect(screen.getByRole("button", { name: "Preparing changes…" })).toBeDisabled();
    expect(screen.getByRole("status", { name: "Plan preparation status" })).toHaveTextContent("Preparing changes");
    expect(screen.getByRole("progressbar", { name: "Preparing changes" })).toHaveAttribute("aria-valuenow");
    expect(screen.getByRole("progressbar", { name: "Preparing changes" }).getAttribute("aria-valuetext")).toMatch(/% complete.*estimated time/i);
    expect(screen.getByRole("button", { name: /start installation/i })).toBeDisabled();

    await act(async () => finishPlan?.({ value: { operations: [], conflicts: [], generated_artifacts: [], external_actions: [] } }));
    await waitFor(() => expect(screen.getByRole("button", { name: /start installation/i })).toBeEnabled());
  });

  it("opens recovery instead of starting a second transaction for the same project", async () => {
    vi.mocked(buildInstallationPlanResult).mockResolvedValue({
      value: { plan_id: "new-plan", operations: [], conflicts: [], generated_artifacts: [], external_actions: [] } as never,
    });
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\cold_war_curtain" } });
    for (let index = 0; index < 7; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: "Prepare changes" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /start installation/i })).toBeEnabled());

    vi.mocked(findInterruptedTransaction).mockResolvedValue({
      transaction_id: "6a26d121-5bdc-4d0a-9d2f-6e3168ca7528",
      state: "staging",
      last_checkpoint: "stage-file-op-00068",
      recovery: {
        resume_allowed: true,
        rollback_allowed: true,
        discard_staging_allowed: true,
        project_apply_started: false,
        recommended_action: "resume",
      },
    } as never);
    vi.mocked(approveInstallation).mockClear();
    vi.mocked(applyInstallationResult).mockClear();
    fireEvent.click(screen.getByRole("button", { name: /start installation/i }));

    expect(await screen.findByRole("heading", { name: "Installation was interrupted" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /continue setup/i }).find((button) => button.classList.contains("primary"))).toBeEnabled();
    expect(approveInstallation).not.toHaveBeenCalled();
    expect(applyInstallationResult).not.toHaveBeenCalled();
  });

  it("stays on installation progress while the reviewed transaction is running", async () => {
    let finishInstallation: ((result: { value: unknown }) => void) | undefined;
    vi.mocked(buildInstallationPlanResult).mockResolvedValue({
      value: { plan_id: "active-plan", operations: [], conflicts: [], generated_artifacts: [], external_actions: [] } as never,
    });
    vi.mocked(applyInstallationResult).mockImplementation(() => new Promise((resolve) => {
      finishInstallation = resolve as (result: { value: unknown }) => void;
    }));
    vi.mocked(readTransactionJournal).mockResolvedValue({
      transaction_id: "active-plan",
      state: "staging",
      last_checkpoint: "stage-file-op-2",
      stages: [
        "preflight", "repository source resolution", "selective download", "checksum verification", "dry-run review", "backup",
      ].map((id) => ({ id, status: "complete" })).concat([
        { id: "staging", status: "active" },
        { id: "validation", status: "pending" },
        { id: "apply", status: "pending" },
        { id: "post-install checks", status: "pending" },
        { id: "readiness report", status: "pending" },
        { id: "rollback record", status: "pending" },
      ]),
      operations: [
        { id: "op-1", status: "staged" },
        { id: "op-2", status: "staged" },
        { id: "op-3", status: "pending" },
        { id: "op-4", status: "pending" },
      ],
      recovery: { resume_allowed: true, rollback_allowed: false, discard_staging_allowed: true, project_apply_started: false, recommended_action: "resume" },
    } as never);
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\active" } });
    for (let index = 0; index < 7; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: "Prepare changes" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /start installation/i })).toBeEnabled());

    fireEvent.click(screen.getByRole("button", { name: /start installation/i }));

    expect(await screen.findByRole("heading", { name: "Installing components" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Installation was interrupted" })).not.toBeInTheDocument();
    expect(await screen.findByText("2 of 4 files")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Installation progress" }).getAttribute("aria-valuetext")).toContain("2 of 4 files");
    const prepareRow = screen.getByText("Prepare the setup").closest(".timeline-row") as HTMLElement;
    expect(within(prepareRow).getByText("50%")).toBeInTheDocument();
    expect(within(prepareRow).getByRole("progressbar", { name: "Prepare the setup progress" })).toHaveAttribute("aria-valuenow", "50");
    expect(within(screen.getByText("Check the project").closest(".timeline-row") as HTMLElement).getByText("100%")).toBeInTheDocument();
    expect(within(screen.getByText("Check the prepared files").closest(".timeline-row") as HTMLElement).getByText("0%")).toBeInTheDocument();

    await act(async () => finishInstallation?.({
      value: {
        transaction_id: "active-plan",
        state: "completed",
        last_checkpoint: "completed",
        stages: [],
        recovery: { resume_allowed: false, rollback_allowed: true, discard_staging_allowed: false, project_apply_started: true, recommended_action: "none" },
      },
    }));
  });

  it("redacts raw installation command errors before displaying them", async () => {
    const secretName = ["client", "secret"].join("_");
    const secretValue = ["synthetic", "installation", "credential"].join("-");
    vi.mocked(buildInstallationPlanResult).mockResolvedValue({
      value: { plan_id: "failing-plan", operations: [], conflicts: [], generated_artifacts: [], external_actions: [] } as never,
    });
    vi.mocked(applyInstallationResult).mockResolvedValue({
      value: null,
      error: `provider failed; {"${secretName}":"${secretValue}"}`,
    });
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\failing" } });
    for (let index = 0; index < 7; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: "Prepare changes" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /start installation/i })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: /start installation/i }));

    const error = await screen.findByText(/Installation could not start:/);
    expect(error).toHaveTextContent("[REDACTED]");
    expect(screen.queryByText(secretValue)).not.toBeInTheDocument();
  });

  it("opens a native Linux project terminal without claiming the client started", async () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("X11; Linux x86_64");
    vi.mocked(openInCodingEnvironment).mockResolvedValue({ opened: false, message: "Project terminal opened. Run codex to start the client." });
    render(<Ready state={readyState()} update={vi.fn()} onMaintenance={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Open project terminal/i }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Run codex to start the client."));
    expect(screen.queryByText("Codex opened successfully.")).not.toBeInTheDocument();
  });

  it("announces a manual project-opening path when Codex has no verified opener", async () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    vi.mocked(openInCodingEnvironment).mockResolvedValue({
      opened: false,
      message: "No verified Codex opener was found. Open this folder manually: C:\\mods\\cold-war-curtain",
    });
    render(<Ready state={readyState()} update={vi.fn()} onMaintenance={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /open in codex/i }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/open this folder manually/i));
    expect(screen.getByRole("status")).toHaveTextContent(/cold-war-curtain/i);
  });

  it("shows immediate and completed feedback while opening Codex", async () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    let finishOpen: ((result: { opened: boolean; message: string }) => void) | undefined;
    vi.mocked(openInCodingEnvironment).mockReturnValue(new Promise((resolve) => { finishOpen = resolve; }));
    render(<Ready state={readyState()} update={vi.fn()} onMaintenance={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /open in codex/i }));
    expect(screen.getByRole("button", { name: /opening codex/i })).toBeDisabled();

    await act(async () => finishOpen?.({ opened: true, message: "Codex was launched." }));
    expect(screen.getByRole("button", { name: /codex opened/i })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("Codex opened successfully.");
  });

  it("opens the project in Claude Code when it is the primary environment", async () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    vi.mocked(openInCodingEnvironment).mockResolvedValue({ opened: true, message: "Claude Code started." });
    render(<Ready state={{ ...readyState(), primaryCodingEnvironment: "claude_code", readiness: { openInCodex: false, coreReady: true, blockingCheckIds: [], checks: [] } } as unknown as WizardState} update={vi.fn()} onMaintenance={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /open in claude code/i }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Claude Code opened successfully."));
    expect(vi.mocked(openInCodingEnvironment)).toHaveBeenCalledWith("C:\\mods\\cold-war-curtain", "claude_code");
    expect(screen.queryByText("Install the Codex configuration component to enable this action.")).not.toBeInTheDocument();
  });

  it("opens a completion page in the existing Ready phase when Finish is pressed", () => {
    window.__HOI4_DOCUMENTATION_STATE__ = {
      ...initialState,
      ...readyState(),
      screen: "ready",
    };
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Finish" }));

    expect(screen.getByRole("heading", { name: "Congratulations, you are all set!" })).toBeInTheDocument();
    expect(screen.getByText("Setup complete")).toBeInTheDocument();
    expect(screen.getByLabelText("Setup phases").querySelector('[aria-current="step"]')).toHaveTextContent("Ready");
    expect(screen.queryByRole("button", { name: "Finish" })).not.toBeInTheDocument();
  });

  it("does not offer an MCP health action when the verified route is unavailable", () => {
    render(<Ready state={{
      ...readyState(),
      selectedComponents: ["mcp.hoi4_agent_tools"],
      readiness: {
        openInCodex: false,
        coreReady: true,
        blockingCheckIds: [],
        checks: [{ id: "mcp.hoi4", status: "planned_unavailable", blocking: false }],
      },
    } as unknown as WizardState} update={vi.fn()} onMaintenance={vi.fn()} />);

    expect(screen.queryByRole("button", { name: /run source-declared MCP check/i })).not.toBeInTheDocument();
    expect(screen.getByText("Optional")).toBeInTheDocument();
  });

  it("keeps recovery actions out of the completed readiness screen", () => {
    const state = {
      ...readyState(),
      readiness: { openInCodex: false, blockingCheckIds: ["installation.rollback"], checks: [] },
      transaction: { transaction_kind: "installation", state: "rolled_back", rollback_transaction_id: "rollback-1" },
    } as unknown as WizardState;

    render(<Ready state={state} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.queryByRole("button", { name: /restore rolled-back state/i })).not.toBeInTheDocument();
  });

  it("keeps manifest dependencies and file evidence behind the component disclosure", async () => {
    const manifest = {
      schema_version: "1.0.0",
      manifest_id: "example",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [{ id: "core.skills", display_name: "Skills", description: "Workflow skills", category: "skill", optional: false, platforms: ["all"], source: { kind: "tree", path: ".agents/skills" }, destination: { path: ".agents/skills/", ownership: "managed" }, dependencies: ["core.agents"], required_tools: [], environment: [], expected_files: [{ path: ".agents/skills/example/SKILL.md", sha256: "c".repeat(64), size: 1 }], capabilities: [], validation: [], update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true } }],
    } as unknown as SourceManifestPreview;
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: manifest });

    render(<Components state={{ sourceMode: "latest", pinnedRef: "", selectedComponents: [] } as unknown as WizardState} update={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("1 file · destination: .agents/skills/")).toBeInTheDocument());
    expect(screen.getByText("Dependencies and file list")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Dependencies and file list"));
    expect(screen.getByText("Requires core.agents")).toBeInTheDocument();
    expect(screen.getByText("1 file · destination: .agents/skills/")).toBeInTheDocument();
    expect(screen.getByText(".agents/skills/example/SKILL.md")).toBeInTheDocument();
  });
});


describe("live testing selection", () => {
  it("uses native Linux without WSL or SSH fields and blocks Live Tools", () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (X11; Linux x86_64)");
    window.history.replaceState({}, "", "/?screenshot=live-testing");
    const state = documentationFixture(initialState)!;
    render(<LiveTestingSetup state={state} update={vi.fn()} />);
    expect(screen.getByText(/Clausewitz MCP runs directly on Linux/)).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /WSL distribution|SSH host/ })).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /^Clausewitz MCP/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /^HOI4 Live/ })).toBeDisabled();
    expect(liveTestingSelectionValid({ ...state, liveTesting: { ...state.liveTesting, linux_ready: true } })).toBe(true);
  });
  it("defaults to deferred Clausewitz and clears the workflow when disabled", () => {
    window.history.replaceState({}, "", "/?screenshot=live-testing");
    const state = documentationFixture(initialState)!;
    const update = vi.fn();
    render(<LiveTestingSetup state={state} update={update} />);
    expect(screen.getByRole("radio", { name: /^Clausewitz MCP/ })).toBeChecked();
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: /^None/ }));
    expect(update.mock.calls[0][0].liveTesting.provider).toBe("none");
    expect(update.mock.calls[0][0].selectedComponents).not.toContain("workflow.live_testing");
  });
  it("requires an SSH alias only when the Linux route is confirmed on macOS", () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Macintosh Mac OS X");
    expect(liveTestingSelectionValid(initialState)).toBe(true);
    const ready = { ...initialState, liveTesting: { ...initialState.liveTesting, linux_ready: true } };
    expect(liveTestingSelectionValid(ready)).toBe(false);
    expect(liveTestingSelectionValid({ ...ready, liveTesting: { ...ready.liveTesting, ssh_host: "linux-game" } })).toBe(true);
    expect(liveTestingSelectionValid({ ...ready, liveTesting: { ...ready.liveTesting, ssh_host: "user@host" } })).toBe(false);
  });
  it("rejects unsafe WSL distribution names", () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Windows NT 10.0");
    expect(liveTestingSelectionValid({ ...initialState, liveTesting: { ...initialState.liveTesting, wsl_distribution: "Ubuntu; command" } })).toBe(false);
    expect(liveTestingSelectionValid(initialState)).toBe(true);
  });
  it("keeps a failed live runtime installation as a to-do", () => {
    const state: WizardState = {
      ...initialState,
      selectedComponents: ["workflow.live_testing", "workflow.live_tools"],
      liveTesting: { provider: "hoi4_live_tools", linux_ready: false, wsl_distribution: "", ssh_host: "" },
      readiness: {
        coreReady: true,
        openInCodex: false,
        blockingCheckIds: [],
        checks: [{ id: "workflow.live_testing", label: "Live testing", status: "warn", blocking: false, message: "Runtime installation failed." }],
      } as NonNullable<WizardState["readiness"]>,
    };
    render(<Ready state={state} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.getByText("To-do", { exact: true })).toBeInTheDocument();
    expect(screen.queryByText("Configured", { exact: true })).not.toBeInTheDocument();
  });
});

function contextDocument(id: string, name: string, overrides: Partial<ContextDocument> = {}): ContextDocument {
  const index = id.replace(/\D/g, "");
  return { id, name, size: 2048, reference: `context-${index}`, evidence_path: `@context/${index}-${name}`, excerpt: `Notes from ${name}`, excerpt_sha256: index.repeat(64).slice(0, 64), truncated: false, redacted: false, copyable: true, ...overrides };
}

function contextImage(id: string, name: string): ContextImage {
  const mediaType = name.endsWith(".png") ? "image/png" : "image/jpeg";
  return { id, name, size: 4096, media_type: mediaType, data_url: `data:${mediaType};base64,${id}` };
}

let observedContextState: WizardState | undefined;

function ControlledContext({ initial, resetAnalysisOnChange = false }: { initial: WizardState; resetAnalysisOnChange?: boolean }) {
  const [state, setState] = useState<WizardState>(initial);
  observedContextState = state;
  return <ContextAttachments state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} resetAnalysisOnChange={resetAnalysisOnChange} />;
}

function openContextDisclosure() {
  const summary = screen.getByText("Add context (optional)");
  const details = summary.closest("details") as HTMLDetailsElement;
  if (!details.open) fireEvent.click(summary);
  if (!details.open) details.open = true;
}

describe("optional setup context", () => {
  beforeEach(() => {
    observedContextState = undefined;
    vi.mocked(forgetContextAttachment).mockResolvedValue(true);
    vi.mocked(renderReferenceJpeg).mockResolvedValue("UkVGRVJFTkNF");
    vi.mocked(renderCoverPng).mockResolvedValue("VEhVTUI=");
  });

  it("adds documents with context on and copies off, explains uncopyable files, and removes them", async () => {
    vi.mocked(pickContextDocuments).mockResolvedValue({ value: { added: [contextDocument("doc-1", "pitch.md"), contextDocument("doc-2", "notes.md", { copyable: false, truncated: true, redacted: true })], refused: [], cancelled: false } });
    render(<ControlledContext initial={{ ...initialState, plan: { plan_id: "stale-plan" } as unknown as InstallationPlan }} />);
    openContextDisclosure();

    fireEvent.click(screen.getByRole("button", { name: "Add documents…" }));
    const pitch = (await screen.findByText("pitch.md")).closest("li") as HTMLElement;
    expect(within(pitch).getByRole("checkbox", { name: "Use as context" })).toBeChecked();
    expect(within(pitch).getByRole("checkbox", { name: "Keep a copy in docs/reference" })).not.toBeChecked();
    expect(within(pitch).getByRole("checkbox", { name: "Keep a copy in docs/reference" })).toBeEnabled();
    const notes = screen.getByText("notes.md").closest("li") as HTMLElement;
    expect(within(notes).getByRole("checkbox", { name: "Keep a copy in docs/reference" })).toBeDisabled();
    expect(within(notes).getByText(/Shortened/)).toBeInTheDocument();
    expect(within(notes).getByText(/Some lines hidden/)).toBeInTheDocument();
    expect(within(notes).getByText("Not copied because some lines were hidden.")).toBeInTheDocument();
    expect(screen.getByText("· 2 documents")).toBeInTheDocument();
    expect(observedContextState?.plan).toBeUndefined();
    expect(observedContextState?.contextDocuments?.map((document) => [document.id, document.includeInAnalysis, document.keepCopy])).toEqual([["doc-1", true, false], ["doc-2", true, false]]);

    fireEvent.click(within(pitch).getByRole("checkbox", { name: "Keep a copy in docs/reference" }));
    expect(observedContextState?.contextDocuments?.[0].keepCopy).toBe(true);

    fireEvent.click(within(pitch).getByRole("button", { name: "Remove pitch.md" }));
    await waitFor(() => expect(screen.queryByText("pitch.md")).not.toBeInTheDocument());
    expect(forgetContextAttachment).toHaveBeenCalledWith("doc-1");
    expect(observedContextState?.contextDocuments?.map((document) => document.id)).toEqual(["doc-2"]);
    expect(screen.getByRole("button", { name: "Add documents…" })).toHaveFocus();
  });

  it("renders the chosen thumbnail and toggles visual references", async () => {
    vi.mocked(pickContextImages).mockResolvedValue({ value: { added: [contextImage("img-1", "cover.png"), contextImage("img-2", "flag.jpg")], refused: [], cancelled: false } });
    render(<ControlledContext initial={{ ...initialState }} />);
    openContextDisclosure();

    fireEvent.click(screen.getByRole("button", { name: "Add images…" }));
    const cover = (await screen.findByRole("img", { name: "cover.png" })).closest("li") as HTMLElement;
    const flag = screen.getByRole("img", { name: "flag.jpg" }).closest("li") as HTMLElement;
    await waitFor(() => expect(observedContextState?.contextImages?.map((image) => image.referenceData)).toEqual(["UkVGRVJFTkNF", "UkVGRVJFTkNF"]));
    expect(renderReferenceJpeg).toHaveBeenCalledWith("data:image/png;base64,img-1", 1600, 1024 * 1024);
    expect(within(cover).getByRole("checkbox", { name: "Save as visual reference" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "No thumbnail from images" })).toBeChecked();

    fireEvent.click(within(cover).getByRole("radio", { name: "Use as thumbnail" }));
    await waitFor(() => expect(observedContextState?.contextThumbnail).toEqual({ attachmentId: "img-1", png: "VEhVTUI=" }));
    expect(renderCoverPng).toHaveBeenCalledWith("data:image/png;base64,img-1", 600);
    expect(within(cover).getByRole("radio", { name: "Use as thumbnail" })).toBeChecked();
    expect(screen.getByRole("img", { name: "Thumbnail preview from cover.png" })).toHaveAttribute("src", "data:image/png;base64,VEhVTUI=");

    fireEvent.click(within(cover).getByRole("checkbox", { name: "Save as visual reference" }));
    await waitFor(() => expect(observedContextState?.contextImages?.[0].saveReference).toBe(false));
    expect(observedContextState?.contextImages?.[0].referenceData).toBeUndefined();

    vi.mocked(renderReferenceJpeg).mockResolvedValueOnce("U0VDT05E");
    fireEvent.click(within(cover).getByRole("checkbox", { name: "Save as visual reference" }));
    await waitFor(() => expect(observedContextState?.contextImages?.[0]).toMatchObject({ saveReference: true, referenceData: "U0VDT05E" }));

    fireEvent.click(screen.getByRole("radio", { name: "No thumbnail from images" }));
    expect(observedContextState?.contextThumbnail).toBeUndefined();

    vi.mocked(renderCoverPng).mockRejectedValueOnce(new Error("no canvas"));
    fireEvent.click(within(flag).getByRole("radio", { name: "Use as thumbnail" }));
    expect(await screen.findByText("flag.jpg could not be used as the thumbnail.")).toHaveAttribute("role", "status");
    expect(observedContextState?.contextThumbnail).toBeUndefined();
  });

  it("drops the thumbnail when its image is removed", async () => {
    const image = { ...contextImage("img-4", "cover.png"), saveReference: true, referenceData: "UkVG" };
    render(<ControlledContext initial={{ ...initialState, contextImages: [image], contextThumbnail: { attachmentId: "img-4", png: "VEhVTUI=" } }} />);

    fireEvent.click(screen.getByRole("button", { name: "Remove cover.png" }));
    await waitFor(() => expect(observedContextState?.contextImages).toEqual([]));
    expect(forgetContextAttachment).toHaveBeenCalledWith("img-4");
    expect(observedContextState?.contextThumbnail).toBeUndefined();
  });

  it("sends only the documents kept as context, after any scan evidence", () => {
    const documents = [
      { ...contextDocument("doc-1", "pitch.md"), includeInAnalysis: true, keepCopy: false },
      { ...contextDocument("doc-2", "old-notes.md"), includeInAnalysis: false, keepCopy: true },
    ];

    expect(contextEvidence({ contextDocuments: documents })).toEqual([{ reference: "context-1", path: "@context/1-pitch.md", excerpt: "Notes from pitch.md", excerpt_sha256: "1".repeat(64), confidence: null }]);
    expect(contextEvidence({})).toEqual([]);
  });

  it("includes the kept documents in the new-mod analysis request", async () => {
    enableTauriRuntime();
    vi.mocked(pickContextDocuments).mockResolvedValue({ value: { added: [contextDocument("doc-1", "pitch.md"), contextDocument("doc-2", "old-notes.md")], refused: [], cancelled: false } });

    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    openContextDisclosure();
    fireEvent.click(screen.getByRole("button", { name: "Add documents…" }));
    const oldNotes = (await screen.findByText("old-notes.md")).closest("li") as HTMLElement;
    fireEvent.click(within(oldNotes).getByRole("checkbox", { name: "Use as context" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));

    await waitFor(() => expect(runCodexAnalysisResult).toHaveBeenCalledTimes(1));
    expect(vi.mocked(runCodexAnalysisResult).mock.calls[0][0].evidence).toEqual([{ reference: "context-1", path: "@context/1-pitch.md", excerpt: "Notes from pitch.md", excerpt_sha256: "1".repeat(64), confidence: null }]);
    expect(approveScanEvidence).not.toHaveBeenCalled();
  });

  it("announces refused files, failed picks, and keeps cancelled picks silent", async () => {
    vi.mocked(pickContextDocuments)
      .mockResolvedValueOnce({ value: { added: [contextDocument("doc-3", "brief.md")], refused: ["tool.exe: this file type cannot be added as a document"], cancelled: false } })
      .mockResolvedValueOnce({ value: { added: [], refused: [], cancelled: true } })
      .mockResolvedValueOnce({ value: null, error: "The packaged desktop bridge is unavailable." });
    render(<ControlledContext initial={{ ...initialState }} />);
    openContextDisclosure();

    fireEvent.click(screen.getByRole("button", { name: "Add documents…" }));
    const status = await screen.findByText(/Not added: tool\.exe: this file type cannot be added as a document/);
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveTextContent("Added 1 document.");

    fireEvent.click(screen.getByRole("button", { name: "Add documents…" }));
    await waitFor(() => expect(pickContextDocuments).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByRole("button", { name: "Add documents…" })).not.toHaveAttribute("aria-busy"));
    expect(observedContextState?.contextDocuments?.map((document) => document.id)).toEqual(["doc-3"]);

    fireEvent.click(screen.getByRole("button", { name: "Add documents…" }));
    expect(await screen.findByText("Documents could not be added: The packaged desktop bridge is unavailable.")).toHaveAttribute("role", "status");
  });

  it("discards a description analysis only when the documents sent as context change", () => {
    const record = { analysis_id: "semantic-analysis", confirmed_fields: [] } as unknown as WizardState["codexAnalysisRecord"];
    render(<ControlledContext resetAnalysisOnChange initial={{ ...initialState, codexAnalysis: semanticAnalysisFixture() as unknown as WizardState["codexAnalysis"], codexAnalysisRecord: record, contextDocuments: [{ ...contextDocument("doc-1", "pitch.md"), includeInAnalysis: true, keepCopy: false }] }} />);

    fireEvent.click(screen.getByRole("checkbox", { name: "Keep a copy in docs/reference" }));
    expect(observedContextState?.codexAnalysis).toBeDefined();
    fireEvent.click(screen.getByRole("checkbox", { name: "Use as context" }));
    expect(observedContextState?.codexAnalysis).toBeUndefined();
    expect(observedContextState?.codexAnalysisRecord).toBeUndefined();
  });

  it("offers a fresh review when the documents change after an existing-mod review", () => {
    const update = vi.fn();
    const state = { ...initialState, mode: "existing", codexAnalysis: semanticAnalysisFixture() as unknown as WizardState["codexAnalysis"], contextAnalysisDocumentIds: [], contextDocuments: [{ ...contextDocument("doc-1", "pitch.md"), includeInAnalysis: true, keepCopy: false }] } as WizardState;
    render(<ContextAttachments state={state} update={update} />);

    expect(screen.getByText("Context changed — run the review again to use it.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Review again" }));
    expect(update).toHaveBeenCalledWith(expect.objectContaining({ codexAnalysis: undefined, codexAnalysisRecord: undefined, plan: undefined }));
  });
});
