import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App, { Components, DryRun, Git, Identity, Mcp, Ready, Scan, Update, Welcome, estimateRemainingTime, estimateSemanticPlanningProgress, recoveryProgress } from "./App";
import { applyInstallationResult, approveInstallation, buildInstallationPlanResult, cancelCodexLogin, checkForAppUpdate, findInterruptedTransaction, installAppUpdate, logoutCodexResult, openCodexLoginUrlResult, openExternalUrlResult, openInCodex, pickProjectFolder, previewDescriptorsResult, previewSourceManifestResult, readCodexAccount, readTransactionJournal, rollbackInstallationResult, runCodexAnalysisResult, suggestProjectPaths } from "./lib/tauri";
import type { CodexAnalysisResult, FolderSelection, ScanProgress, SourceManifestPreview, WizardState } from "./types";

vi.mock("./lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("./lib/tauri")>("./lib/tauri");
  return { ...actual, applyInstallationResult: vi.fn(), approveInstallation: vi.fn(), buildInstallationPlanResult: vi.fn(), cancelCodexLogin: vi.fn(), checkForAppUpdate: vi.fn(), findInterruptedTransaction: vi.fn(), installAppUpdate: vi.fn(), logoutCodexResult: vi.fn(), openCodexLoginUrlResult: vi.fn(), openExternalUrlResult: vi.fn(), openInCodex: vi.fn(), pickProjectFolder: vi.fn(), previewDescriptorsResult: vi.fn(), previewSourceManifestResult: vi.fn(), readCodexAccount: vi.fn(), readTransactionJournal: vi.fn(), rollbackInstallationResult: vi.fn(), runCodexAnalysisResult: vi.fn(), suggestProjectPaths: vi.fn() };
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

beforeEach(() => {
  vi.mocked(readCodexAccount).mockResolvedValue({ available: false, authenticated: false, auth_mode: "none", usage_limited: false });
  vi.mocked(readTransactionJournal).mockResolvedValue(null);
  vi.mocked(rollbackInstallationResult).mockResolvedValue({ value: null, error: "Undo is unavailable." });
  vi.mocked(logoutCodexResult).mockResolvedValue({ value: null });
  vi.mocked(openCodexLoginUrlResult).mockResolvedValue({ value: undefined });
  vi.mocked(openExternalUrlResult).mockResolvedValue({ value: undefined });
  vi.mocked(cancelCodexLogin).mockResolvedValue(true);
  vi.mocked(pickProjectFolder).mockResolvedValue(null);
  vi.mocked(previewDescriptorsResult).mockResolvedValue({ value: null });
  vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: null });
  vi.mocked(runCodexAnalysisResult).mockResolvedValue({ value: null });
  vi.mocked(suggestProjectPaths).mockResolvedValue(null);
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
    aiModel: "default",
    readiness: { openInCodex: true, coreReady: true, blockingCheckIds: [], checks: [] },
    selectedComponents: [],
    meshSelected: false,
  } as unknown as WizardState;
}

function welcomeState(account: WizardState["codexAccount"], codexLogin?: WizardState["codexLogin"]): WizardState {
  return { mode: "new", aiProvider: "codex", aiModel: "default", aiEndpoint: "", selectedComponents: ["codex.config"], flattenForChat: false, codexAccount: account, codexLogin } as unknown as WizardState;
}

async function renderAuthenticatedApp() {
  vi.mocked(readCodexAccount).mockResolvedValue({
    available: true,
    authenticated: true,
    auth_mode: "chatgpt",
    usage_limited: false,
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled());
}

function enableTauriRuntime() {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
}

describe("HOI4 Mod Setup wizard", () => {
  it("keeps semantic planning estimates bounded within each real stage", () => {
    const startedAt = Date.parse("2026-08-02T12:00:00Z");
    expect(estimateSemanticPlanningProgress("preparing", startedAt, startedAt)).toEqual({ percent: 5, remaining: "About 2 minutes remaining" });
    expect(estimateSemanticPlanningProgress("analyzing", startedAt, startedAt + 45_000)).toEqual({ percent: 54, remaining: "About 50 seconds remaining" });
    expect(estimateSemanticPlanningProgress("validating", startedAt, startedAt + 120_000)).toEqual({ percent: 98, remaining: "Less than 10 seconds remaining" });
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
  it("checks quietly on launch and installs an available signed update only after approval", async () => {
    enableTauriRuntime();
    vi.mocked(checkForAppUpdate).mockResolvedValue({
      value: { currentVersion: "0.1.1", availableVersion: "0.2.0", available: true },
    });

    render(<App />);

    expect(await screen.findByText("Version 0.2.0 is available")).toBeInTheDocument();
    expect(installAppUpdate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Update now" }));
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
    fireEvent.click(await screen.findByRole("button", { name: "Update now" }));

    expect(await screen.findByText("Update failed. Try again.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update now" })).toBeEnabled();
    expect(screen.getByRole("button", { name: /create new mod/i })).toBeInTheDocument();
  });

  it("starts in the Project phase and exposes the two supported entry routes", () => {
    render(<App />);
    expect(screen.getByText("HOI4 Mod Setup")).toBeInTheDocument();
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

  it("selects a non-Codex provider at the start and keeps the Codex-only option out of that route", () => {
    function ControlledWelcome() {
      const [state, setState] = useState(welcomeState({ available: false, authenticated: false, auth_mode: "none", usage_limited: false }));
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });

    expect(screen.getByLabelText("Model")).toBeInTheDocument();
    expect(screen.getByLabelText("Provider address")).toBeInTheDocument();
    expect(screen.getByLabelText("Model")).toHaveValue("claude-sonnet-5");
    expect(screen.getByLabelText("Provider address")).toHaveValue("https://api.anthropic.com/v1/messages");
    expect(screen.getByLabelText("Claude API key")).toHaveAttribute("type", "password");
    expect(screen.getByText("Advanced")).toBeInTheDocument();
    expect(screen.queryByText(/flattened ChatGPT project-sources/i)).not.toBeInTheDocument();
  });

  it("opens only the fixed provider account page from the simple connection flow", async () => {
    enableTauriRuntime();
    function ControlledWelcome() {
      const [state, setState] = useState(welcomeState({ available: false, authenticated: false, auth_mode: "none", usage_limited: false }));
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });
    fireEvent.click(screen.getByRole("button", { name: "Get Claude API key" }));

    await waitFor(() => expect(openExternalUrlResult).toHaveBeenCalledWith("https://platform.claude.com/settings/keys"));
  });

  it("shows the flattened ChatGPT sources checkbox only in Codex Components", async () => {
    const manifest = {
      schema_version: "1.0.0",
      manifest_id: "example",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [],
    } as unknown as SourceManifestPreview;
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: manifest });

    function ControlledComponents({ provider }: { provider: "codex" | "claude" }) {
      const [state, setState] = useState({
        aiProvider: provider,
        flattenForChat: false,
        sourceMode: "latest",
        pinnedRef: "",
        selectedComponents: [],
      } as unknown as WizardState);
      return <Components state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<Git state={{ aiProvider: "codex", gitMode: "skip", gitBranch: "main", gitRemoteName: "origin", gitRemoteUrl: "", initialCommit: false } as unknown as WizardState} update={vi.fn()} />);
    expect(screen.queryByRole("checkbox", { name: /Prepare a flattened ChatGPT project-sources folder/i })).not.toBeInTheDocument();
    cleanup();

    const { unmount } = render(<ControlledComponents provider="codex" />);
    const checkbox = await screen.findByRole("checkbox", { name: /Prepare a flattened ChatGPT project-sources folder/i });
    fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();
    expect(screen.queryByLabelText(/Additional project files/i)).not.toBeInTheDocument();
    unmount();

    render(<ControlledComponents provider="claude" />);
    await screen.findByText("Components loaded.");
    expect(screen.queryByRole("checkbox", { name: /Prepare a flattened ChatGPT project-sources folder/i })).not.toBeInTheDocument();
  });

  it("shows flattened ChatGPT sources as a sized file package in Components after planning", async () => {
    const manifest = {
      schema_version: "1.0.0",
      manifest_id: "example",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [],
    } as unknown as SourceManifestPreview;
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: manifest });

    render(<Components state={{
      aiProvider: "codex",
      flattenForChat: true,
      sourceMode: "latest",
      pinnedRef: "",
      selectedComponents: [],
      plan: {
        operations: [],
        conflicts: [],
        generated_artifacts: [
          { destination: "chatgpt_project_sources/AGENTS.md", content: "abc" },
          { destination: "chatgpt_project_sources/README.md", content: "", bytes: [1, 2, 3, 4] },
        ],
      },
    } as unknown as WizardState} update={vi.fn()} />);

    expect(await screen.findByRole("checkbox", { name: /Prepare a flattened ChatGPT project-sources folder/i })).toBeChecked();
    expect(screen.getByText("2 files · 7 B")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Files in the ChatGPT folder"));
    expect(screen.getByText("AGENTS.md")).toBeInTheDocument();
    expect(screen.getByText("README.md")).toBeInTheDocument();
  });

  it("previews flattened skill and subagent filenames and source sizes in Components", async () => {
    const manifest = {
      schema_version: "1.0.0",
      manifest_id: "example",
      source: { repository: "klimPaskov/Agentic-HOI4-Modding", mode: "latest", resolved_revision: "a".repeat(40), manifest_sha256: "b".repeat(64), manifest_origin: "remote" },
      repository: { provider: "github", owner: "klimPaskov", name: "Agentic-HOI4-Modding", default_branch: "main" },
      components: [
        { id: "core.skills", display_name: "Skills", category: "skill", optional: false, platforms: ["all"], source: { kind: "tree", path: ".agents/skills" }, destination: { path: ".agents/skills/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [{ path: ".agents/skills/hoi4-events/SKILL.md", size: 120 }], capabilities: [], validation: [], update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true } },
        { id: "core.subagents", display_name: "Subagents", category: "subagent", optional: false, platforms: ["all"], source: { kind: "tree", path: ".codex/agents" }, destination: { path: ".codex/agents/", ownership: "managed" }, dependencies: [], required_tools: [], environment: [], expected_files: [{ path: ".codex/agents/hoi4_researcher.toml", size: 80 }], capabilities: [], validation: [], update: { strategy: "replace_if_unmodified", remove_obsolete: true, preserve_local_additions: true } },
      ],
    } as unknown as SourceManifestPreview;
    vi.mocked(previewSourceManifestResult).mockResolvedValue({ value: manifest });

    render(<Components state={{ aiProvider: "codex", flattenForChat: true, sourceMode: "latest", pinnedRef: "", selectedComponents: ["core.skills", "core.subagents"] } as unknown as WizardState} update={vi.fn()} />);

    await screen.findByText("Components loaded.");
    fireEvent.click(screen.getByText("Files in the ChatGPT folder"));
    expect(screen.getByText("hoi4-events.md")).toBeInTheDocument();
    expect(screen.getByText("hoi4_researcher.toml")).toBeInTheDocument();
    expect(screen.getByText("120 B")).toBeInTheDocument();
    expect(screen.getByText("80 B")).toBeInTheDocument();
    expect(screen.getAllByText("Size calculated during review")).toHaveLength(2);
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

    expect(screen.getByText("Do you want to set up the 3D models workflow?")).toBeInTheDocument();
    expect(screen.getByText("Do you want to set up the Super Events workflow?")).toBeInTheDocument();
    expect(screen.getAllByText("Add it during the next update or repair")).toHaveLength(2);
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

  it("keeps the 3D question exact and places Super Events immediately after it", async () => {
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    const modelsWorkflow = screen.getByRole("switch", { name: /Do you want to set up the 3D models workflow\?/ });
    const superEventsWorkflow = screen.getByRole("switch", { name: /Do you want to set up the Super Events workflow\?/ });
    expect(modelsWorkflow.compareDocumentPosition(superEventsWorkflow) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.click(superEventsWorkflow);
    expect(superEventsWorkflow).toHaveAttribute("aria-checked", "true");
    expect(screen.queryByText(/LoRA|ComfyUI/i)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("heading", { name: "MCP and credentials" })).toBeInTheDocument();
  });

  it("links to the separate portrait workflow only after core setup succeeds", () => {
    const { rerender } = render(<Ready state={{ ...readyState(), readiness: null }} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.queryByRole("link", { name: /portrait workflow/i })).not.toBeInTheDocument();

    rerender(<Ready state={{ ...readyState(), readiness: { openInCodex: false, coreReady: false, blockingCheckIds: ["descriptor.project"], checks: [] } }} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.queryByRole("link", { name: /portrait workflow/i })).not.toBeInTheDocument();

    rerender(<Ready state={readyState()} update={vi.fn()} onMaintenance={vi.fn()} />);
    const links = screen.getAllByRole("link", { name: "Optional portrait workflow: Explore ComfyUI HOI4 Portraits on GitHub" });
    expect(links).toHaveLength(1);
    expect(links[0]).toHaveAttribute("href", "https://github.com/klimPaskov/comfyui-hoi4-portraits");
    expect(screen.queryByRole("switch", { name: /LoRA|ComfyUI|portrait/i })).not.toBeInTheDocument();
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
    expect(screen.getByLabelText("Descriptor tags")).toHaveValue("Total Conversion, Alternative History, Events");
    expect(screen.getByLabelText("Initial folders")).toHaveValue("common, events, localisation/english, gfx, interface, docs, history");

    fireEvent.change(screen.getByLabelText("Script prefix"), { target: { value: "iron" } });
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    fireEvent.change(screen.getByLabelText("Mod description"), { target: { value: "A focused portrait workflow." } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByLabelText("Script prefix")).toHaveValue("iron");
    expect(screen.getByLabelText("Descriptor tags")).toHaveValue("Portraits");
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

  it("shows correlated scan stage, path, counters, and cancellation", () => {
    const progress: ScanProgress = { stage: "detecting_git", currentPath: ".git", filesScanned: 12, directoriesScanned: 4, bytesRead: 4096 };
    const onCancel = vi.fn().mockResolvedValue(undefined);
    render(<Scan state={{ mode: "existing" } as unknown as WizardState} complete={false} progress={progress} partial={false} limitsHit={[]} canCancel cancellationRequested={false} onCancel={onCancel} />);

    expect(screen.getByText("Checking Git state")).toBeInTheDocument();
    expect(screen.getByText(".git")).toBeInTheDocument();
    expect(screen.getByText(/12 files/)).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Project scan progress" })).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByRole("progressbar", { name: "Project scan progress" })).toHaveAttribute("aria-valuetext", "Scan in progress");
    fireEvent.click(screen.getByRole("button", { name: "Cancel scan" }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("keeps a truncated scan visibly partial and does not call it a full scan", () => {
    render(<Scan state={{ mode: "existing" } as unknown as WizardState} complete progress={{ stage: "complete", currentPath: ".", filesScanned: 20, directoriesScanned: 8, bytesRead: 8192 }} partial limitsHit={["scan.file_limit"]} canCancel={false} cancellationRequested={false} onCancel={vi.fn().mockResolvedValue(undefined)} />);

    expect(screen.getByText("Read-only scan reached a safety limit")).toBeInTheDocument();
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
      project_root: "C:\\mods\\cold_war_curtain",
      launcher_descriptor_path: "C:\\mods\\cold_war_curtain.mod",
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
    await waitFor(() => expect(suggestProjectPaths).toHaveBeenCalledWith("cold_war_curtain"));
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() => expect(screen.getByLabelText("Project folder")).toHaveValue("C:\\mods\\cold_war_curtain"));
    fireEvent.click(screen.getByText("Launcher file location"));
    expect(screen.getByLabelText("Launcher descriptor path")).toHaveValue("C:\\mods\\cold_war_curtain.mod");
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

  it("confirms the existing launcher descriptor returned with the selected project", async () => {
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
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\other" } });
    expect(screen.queryByText("C:\\mods\\existing.mod")).not.toBeInTheDocument();
  });

  it("opens the source repository through the desktop browser bridge", async () => {
    enableTauriRuntime();
    render(<App />);

    fireEvent.click(screen.getByRole("link", { name: /Agentic-HOI4-Modding/i }));

    await waitFor(() => expect(openExternalUrlResult).toHaveBeenCalledWith("https://github.com/klimPaskov/Agentic-HOI4-Modding"));
  });

  it("keeps signed-out local recovery reachable from the welcome screen", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    expect(screen.getByText("Choose an installed project")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\installed" } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("heading", { name: "Installation was interrupted" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /remove managed components/i })).toBeEnabled();
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
    for (let index = 0; index < 6; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
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
      flattenForChat: false,
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
      flattenForChat: false,
      plan: {
        operations: [
          { id: "agents", action: "create", destination: "AGENTS.md" },
          { id: "skill", action: "replace", destination: ".agents/skills/example/SKILL.md" },
        ],
        conflicts: [],
        external_actions: [],
      },
    } as unknown as WizardState} update={vi.fn()} />);

    const files = screen.getByText("Files to install · 2", { selector: "summary" });
    expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument();
    const details = files.closest("details")!;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(screen.getByText("AGENTS.md")).toBeInTheDocument();
    expect(screen.getByText(".agents/skills/example/SKILL.md")).toBeInTheDocument();
    expect(screen.queryByText("Open full file plan")).not.toBeInTheDocument();
  });

  it("opens integration requirements by default", () => {
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

    expect(screen.getByText("Requirements", { selector: "summary" }).closest("details")).toHaveAttribute("open");
  });

  it("shows dry-run preparation as busy and enables installation only after a plan succeeds", async () => {
    let finishPlan: ((result: { value: unknown }) => void) | undefined;
    vi.mocked(buildInstallationPlanResult).mockImplementation(() => new Promise((resolve) => { finishPlan = resolve as (result: { value: unknown }) => void; }));
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    for (let index = 0; index < 6; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));

    fireEvent.click(screen.getByRole("button", { name: "Prepare changes" }));
    expect(screen.getByRole("button", { name: "Preparing changes…" })).toBeDisabled();
    expect(screen.getByRole("status", { name: "Plan preparation status" })).toHaveTextContent("Preparing changes");
    expect(screen.getByRole("progressbar", { name: "Preparing changes" })).toHaveAttribute("aria-valuetext", "Preparing the installation review");
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
    for (let index = 0; index < 5; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
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
    for (let index = 0; index < 5; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: "Prepare changes" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /start installation/i })).toBeEnabled());

    fireEvent.click(screen.getByRole("button", { name: /start installation/i }));

    expect(await screen.findByRole("heading", { name: "Installing components" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Installation was interrupted" })).not.toBeInTheDocument();
    expect(await screen.findByText("Preparing file 2 of 4")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Installation progress" }).getAttribute("aria-valuetext")).toContain("Preparing file 2 of 4");
    const prepareRow = screen.getByText("Prepare the setup").closest(".timeline-row") as HTMLElement;
    expect(within(prepareRow).getByText("50%")).toBeInTheDocument();
    expect(within(prepareRow).getByRole("progressbar", { name: "Prepare the setup progress" })).toHaveAttribute("aria-valuenow", "50");
    expect(within(screen.getByText("Check the project").closest(".timeline-row") as HTMLElement).getByText("100%")).toBeInTheDocument();
    expect(within(screen.getByText("Validate the project").closest(".timeline-row") as HTMLElement).getByText("0%")).toBeInTheDocument();

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

  it("announces a manual project-opening path when Codex has no verified opener", async () => {
    vi.mocked(openInCodex).mockResolvedValue({
      opened: false,
      message: "No verified Codex opener was found. Open this folder manually: C:\\mods\\cold-war-curtain",
    });
    render(<Ready state={readyState()} update={vi.fn()} onMaintenance={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /open in codex/i }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/open this folder manually/i));
    expect(screen.getByRole("status")).toHaveTextContent(/cold-war-curtain/i);
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
    expect(screen.getByRole("status")).toHaveTextContent(/optional integration is unavailable/i);
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
    expect(screen.getByText("Requires core.agents · all")).toBeInTheDocument();
    expect(screen.getByText("1 file · destination: .agents/skills/")).toBeInTheDocument();
  });
});
