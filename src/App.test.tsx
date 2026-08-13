import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App, { ChatSources, Components, DryRun, Findings, Git, Identity, Mcp, Mesh, Ready, Scan, Update, Welcome, Workflows, detectedChatSourcesAvailable, estimatePlanPreparationProgress, estimateRemainingTime, estimateSemanticPlanningProgress, initialState, maintenanceReviewScreen, recoveryProgress } from "./App";
import { applyInstallationResult, approveInstallation, buildInstallationPlanResult, cancelCodexLogin, checkForAppUpdate, findInterruptedTransaction, installAppUpdate, logoutCodexResult, openCodexLoginUrlResult, openExternalUrlResult, openInCodex, pickProjectFolder, previewDescriptorsResult, previewSourceManifestResult, readCodexAccount, readTransactionJournal, rollbackInstallationResult, runCodexAnalysisResult, suggestProjectPaths } from "./lib/tauri";
import type { ChatSourcesPreview, CodexAnalysisResult, FolderSelection, ScanFinding, ScanProgress, SourceManifestPreview, WizardState } from "./types";
import { documentationFixture, isDocumentationScreenshot } from "./documentation-fixtures";

vi.mock("./lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("./lib/tauri")>("./lib/tauri");
  return { ...actual, applyInstallationResult: vi.fn(), approveInstallation: vi.fn(), buildInstallationPlanResult: vi.fn(), cancelCodexLogin: vi.fn(), checkForAppUpdate: vi.fn(), findInterruptedTransaction: vi.fn(), installAppUpdate: vi.fn(), logoutCodexResult: vi.fn(), openCodexLoginUrlResult: vi.fn(), openExternalUrlResult: vi.fn(), openInCodex: vi.fn(), pickProjectFolder: vi.fn(), previewDescriptorsResult: vi.fn(), previewSourceManifestResult: vi.fn(), readCodexAccount: vi.fn(), readTransactionJournal: vi.fn(), rollbackInstallationResult: vi.fn(), runCodexAnalysisResult: vi.fn(), suggestProjectPaths: vi.fn() };
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
  it("starts new projects with the Atlantis Rising example", () => {
    expect(initialState.identity.displayName).toBe("Atlantis Rising");
    expect(initialState.description).toBe("An Atlantis total conversion with a new Atlantic island, naval expansion, custom units, national focuses, and original mechanics.");
    expect(initialState.identity.projectId).toBe("atlantis_rising");
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
  });

  it("provides a sanitized ChatGPT source-package screenshot state", () => {
    window.history.replaceState({}, "", "/?screenshot=chat-sources");
    const fixture = documentationFixture(readyState());

    expect(fixture.screen).toBe("chat-sources");
    expect(fixture.chatSourcesAvailable).toBe(true);
    expect(fixture.chatSourcesPreview?.destinationDirectory).toBe("C:\\Users\\Player\\Downloads");
    expect(fixture.chatSourcesPreview?.files.some((file) => file.category === "root_markdown" && !file.selectedByDefault)).toBe(true);
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

  it("detects ChatGPT source packaging inputs without an installation lock", () => {
    expect(detectedChatSourcesAvailable([
      { id: "skill.inventory", value: JSON.stringify({ count: 1 }) } as ScanFinding,
    ])).toBe(true);
    expect(detectedChatSourcesAvailable([
      { id: "subagent.inventory", value: JSON.stringify({ count: 1 }) } as ScanFinding,
    ])).toBe(true);
    expect(detectedChatSourcesAvailable([])).toBe(false);
  });

  it("offers source packaging from findings for an unmanaged project", () => {
    const onPackageChatSources = vi.fn().mockResolvedValue(undefined);
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
      onPackageChatSources={onPackageChatSources}
    />);

    fireEvent.click(screen.getByRole("button", { name: /Package ChatGPT project sources/ }));
    expect(onPackageChatSources).toHaveBeenCalledTimes(1);
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
      onPackageChatSources={vi.fn().mockResolvedValue(undefined)}
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

  it("opens every prepared maintenance plan in a visible review step", () => {
    expect(maintenanceReviewScreen({ conflicts: [] })).toBe("dry-run");
    expect(maintenanceReviewScreen({ conflicts: [{ selected: undefined }] as never })).toBe("conflict");
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

  it("offers ChatGPT source packaging when the scan finds source files", () => {
    const onPackageChatSources = vi.fn().mockResolvedValue(undefined);
    render(<Update
      state={{ ...readyState(), aiProvider: "codex", chatSourcesAvailable: true } as unknown as WizardState}
      update={vi.fn()}
      findings={[]}
      setFindings={vi.fn()}
      onMaintenance={vi.fn()}
      onStartMaintenance={vi.fn()}
      onReanalyze={vi.fn().mockResolvedValue(true)}
      onPackageChatSources={onPackageChatSources}
    />);

    fireEvent.click(screen.getByRole("button", { name: /Package ChatGPT project sources/ }));
    expect(onPackageChatSources).toHaveBeenCalledTimes(1);
  });

  it("keeps required ChatGPT source files selected and root Markdown opt-in", () => {
    const preview: ChatSourcesPreview = {
      eligible: true,
      projectRoot: "C:\\mods\\atlantis_rising",
      destinationDirectory: "C:\\Users\\Player\\Downloads",
      archiveName: "atlantis_rising-chatgpt-project-sources.zip",
      files: [
        { id: "instructions:AGENTS.md", sourcePath: "AGENTS.md", archivePath: "AGENTS.md", category: "instructions", size: 1024, required: true, selectedByDefault: true },
        { id: "readme:README.md", sourcePath: "README.md", archivePath: "README.md", category: "readme", size: 512, required: true, selectedByDefault: true },
        { id: "skill:hoi4-events", sourcePath: ".agents/skills/hoi4-events/SKILL.md", archivePath: "hoi4-events.md", category: "skill", size: 2048, required: true, selectedByDefault: true },
        { id: "subagent:hoi4_researcher.toml", sourcePath: ".codex/agents/hoi4_researcher.toml", archivePath: "hoi4_researcher.toml", category: "subagent", size: 768, required: true, selectedByDefault: true },
        { id: "root-markdown:NOTES.md", sourcePath: "NOTES.md", archivePath: "NOTES.md", category: "root_markdown", size: 256, required: false, selectedByDefault: false },
      ],
    };
    const update = vi.fn();
    const state = { ...readyState(), chatSourcesPreview: preview, chatSourcesDestination: preview.destinationDirectory, chatSourcesSelectedIds: preview.files.filter((file) => file.selectedByDefault).map((file) => file.id) } as unknown as WizardState;

    render(<ChatSources state={state} update={update} onPickFolder={vi.fn().mockResolvedValue(undefined)} />);

    expect(screen.getByLabelText("Download folder")).toHaveValue("C:\\Users\\Player\\Downloads");
    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes).toHaveLength(5);
    expect(checkboxes.slice(0, 4).every((checkbox) => (checkbox as HTMLInputElement).checked && (checkbox as HTMLInputElement).disabled)).toBe(true);
    const optional = screen.getByText("NOTES.md").closest("label");
    expect(optional).not.toBeNull();
    const optionalCheckbox = optional?.querySelector("input");
    expect(optionalCheckbox).not.toBeNull();
    expect(optionalCheckbox).not.toBeChecked();
    fireEvent.click(optionalCheckbox as HTMLInputElement);
    expect(update).toHaveBeenCalledWith(expect.objectContaining({ chatSourcesSelectedIds: expect.arrayContaining(["root-markdown:NOTES.md"]) }));
  });

  it("uses declarative workflow titles and places Super Events immediately after 3D", async () => {
    await renderAuthenticatedApp();
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
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

  it("keeps 3D readiness in the report without a redundant Ready-screen action", () => {
    render(<Ready state={{ ...readyState(), meshSelected: true } as unknown as WizardState} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.getByText("3D model workflow")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /check 3d setup/i })).not.toBeInTheDocument();
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

  it("links prepared ChatGPT sources directly to ChatGPT Chat", () => {
    render(<Ready state={{ ...readyState(), flattenForChat: true }} update={vi.fn()} onMaintenance={vi.fn()} />);
    expect(screen.getByRole("link", { name: /ChatGPT Chat sources prepared/i })).toHaveAttribute("href", "https://chatgpt.com");
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
    expect(screen.getByText(/Backup verified.*Project apply had not started.*3 files staged/)).toBeInTheDocument();
    expect(screen.getByText("validation", { selector: "code" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /continue setup/i }).find((button) => button.classList.contains("recovery-card"))).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /discard prepared files/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /undo changes/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /manage installed components/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("Details", { selector: "summary" }));
    expect(screen.getByText("Validation")).toBeInTheDocument();
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
    expect(await screen.findByText("2 of 4 files")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Installation progress" }).getAttribute("aria-valuetext")).toContain("2 of 4 files");
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
    for (let index = 0; index < 5; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: "Prepare changes" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /start installation/i })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: /start installation/i }));

    const error = await screen.findByText(/Installation could not start:/);
    expect(error).toHaveTextContent("[REDACTED]");
    expect(screen.queryByText(secretValue)).not.toBeInTheDocument();
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

  it("shows immediate and completed feedback while opening Codex", async () => {
    let finishOpen: ((result: { opened: boolean; message: string }) => void) | undefined;
    vi.mocked(openInCodex).mockReturnValue(new Promise((resolve) => { finishOpen = resolve; }));
    render(<Ready state={readyState()} update={vi.fn()} onMaintenance={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /open in codex/i }));
    expect(screen.getByRole("button", { name: /opening codex/i })).toBeDisabled();

    await act(async () => finishOpen?.({ opened: true, message: "Codex was launched." }));
    expect(screen.getByRole("button", { name: /codex opened/i })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("Codex opened successfully.");
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
  });
});
