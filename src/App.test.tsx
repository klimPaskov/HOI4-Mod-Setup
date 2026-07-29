import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App, { Components, Git, Ready, Scan, Update, Welcome } from "./App";
import { cancelCodexLogin, logoutCodexResult, openCodexLoginUrlResult, openInCodex, previewSourceManifest, readCodexAccount } from "./lib/tauri";
import type { ScanProgress, SourceManifestPreview, WizardState } from "./types";

vi.mock("./lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("./lib/tauri")>("./lib/tauri");
  return { ...actual, cancelCodexLogin: vi.fn(), logoutCodexResult: vi.fn(), openCodexLoginUrlResult: vi.fn(), openInCodex: vi.fn(), previewSourceManifest: vi.fn(), readCodexAccount: vi.fn() };
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

beforeEach(() => {
  vi.mocked(readCodexAccount).mockResolvedValue({ available: false, authenticated: false, auth_mode: "none", usage_limited: false });
  vi.mocked(logoutCodexResult).mockResolvedValue({ value: null });
  vi.mocked(openCodexLoginUrlResult).mockResolvedValue({ value: undefined });
  vi.mocked(cancelCodexLogin).mockResolvedValue(true);
  vi.mocked(previewSourceManifest).mockResolvedValue(null);
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
  return { mode: "new", aiProvider: "codex", aiModel: "default", aiEndpoint: "", selectedComponents: ["codex.config"], flattenForChat: false, flattenAdditionalFiles: [], codexAccount: account, codexLogin } as unknown as WizardState;
}

describe("HOI4 Mod Setup wizard", () => {
  it("starts in the Project phase and exposes the two supported entry routes", () => {
    render(<App />);
    expect(screen.getByText("HOI4 Mod Setup")).toBeInTheDocument();
    expect(screen.getByText("Project")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create new mod/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /import existing mod/i })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Setup phases" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create new mod/i })).toHaveAttribute("aria-pressed", "true");
  });

  it("selects a non-Codex provider at the start and keeps the Codex-only option out of that route", () => {
    function ControlledWelcome() {
      const [state, setState] = useState(welcomeState({ available: false, authenticated: false, auth_mode: "none", usage_limited: false }));
      return <Welcome state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    render(<ControlledWelcome />);
    fireEvent.change(screen.getByLabelText("AI provider"), { target: { value: "claude" } });

    expect(screen.getByLabelText("Model")).toBeInTheDocument();
    expect(screen.getByLabelText("Endpoint")).toBeInTheDocument();
    expect(screen.getByLabelText("Model")).toHaveValue("");
    expect(screen.getByLabelText("Endpoint")).toHaveValue("");
    expect(screen.getByLabelText("Provider API key")).toHaveAttribute("type", "password");
    expect(screen.queryByText(/flattened ChatGPT project-sources/i)).not.toBeInTheDocument();
  });

  it("shows the flattened ChatGPT sources checkbox only for Codex and records its choice", () => {
    function ControlledGit({ provider }: { provider: "codex" | "claude" }) {
      const [state, setState] = useState({
        aiProvider: provider,
        flattenForChat: false,
        flattenAdditionalFiles: [],
        gitMode: "skip",
        gitBranch: "main",
        gitRemoteName: "origin",
        gitRemoteUrl: "",
        initialCommit: false,
      } as unknown as WizardState);
      return <Git state={state} update={(patch) => setState((current) => ({ ...current, ...patch }))} />;
    }

    const { unmount } = render(<ControlledGit provider="codex" />);
    const checkbox = screen.getByRole("checkbox", { name: /Prepare a flattened ChatGPT project-sources folder/i });
    const gitSetup = screen.getByText("Skip Git setup");
    expect(gitSetup.compareDocumentPosition(checkbox) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.click(checkbox);
    expect(screen.getByText('After setup, start planning using ChatGPT "Chat". The folder is prepared locally; no upload or planning action starts automatically.')).toBeInTheDocument();
    unmount();

    render(<ControlledGit provider="claude" />);
    expect(screen.queryByRole("checkbox", { name: /Prepare a flattened ChatGPT project-sources folder/i })).not.toBeInTheDocument();
  });

  it("offers the exact 3D question again for an existing managed setup", () => {
    render(<Update
      state={{
        ...readyState(),
        aiProvider: "codex",
        existingInstallationDetected: true,
        installedWorkflow3dState: "not_selected",
        meshSelected: false,
        meshKeyDraft: "",
        meshKeyStatus: "missing",
        loraInterest: false,
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

    expect(screen.getByText("Do you want to set up the 3D models workflow?")).toBeInTheDocument();
    expect(screen.getByText("Add it during the next repair")).toBeInTheDocument();
  });

  it("keeps the optional workflow questions exact and records portrait interest without actions", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByText("Do you want to set up the 3D models workflow?")).toBeInTheDocument();
    expect(screen.getByText("Do you want to set up LoRAs and ComfyUI for portrait generation?")).toBeInTheDocument();
    const loraSwitch = screen.getByRole("switch", { name: /Do you want to set up LoRAs/i });
    expect(loraSwitch).toHaveAttribute("aria-checked", "false");
    fireEvent.click(loraSwitch);
    expect(loraSwitch).toHaveAttribute("aria-checked", "true");
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByText(/no ComfyUI, model, LoRA, Python, or GPU changes/i)).toBeInTheDocument();
  });

  it("moves focus to the new screen heading and names scan progress", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    expect(screen.getByRole("heading", { name: "Describe the mod" })).toHaveFocus();

    cleanup();
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /import existing mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    expect(screen.getByRole("heading", { name: "Project identity" })).toHaveFocus();
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\temp\\hoi4-mod" } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("progressbar", { name: "Project scan progress" })).toHaveAttribute("aria-valuetext", "Scan in progress");
  });

  it("fills generated identity fields from the mod name and description", () => {
    render(<App />);
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

  it("exposes the native project-folder picker with an accessible name", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("button", { name: "Browse project folder" })).toBeInTheDocument();
  });

  it("keeps signed-out local recovery reachable from the welcome screen", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /manage an existing project/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    expect(screen.getByText("Choose an installed project")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "C:\\mods\\installed" } });
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("heading", { name: "Installation was interrupted" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /remove managed components/i })).toBeEnabled();
  });

  it("shows a distinct usage-limited state and preserves remote logout errors", async () => {
    const update = vi.fn();
    vi.mocked(logoutCodexResult).mockResolvedValue({ value: null, error: "remote logout failed; retry in Codex" });
    vi.mocked(readCodexAccount).mockResolvedValue({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: true, email: "user@example.com" });
    render(<Welcome state={welcomeState({ available: true, authenticated: true, auth_mode: "chatgpt", usage_limited: true, email: "user@example.com" })} update={update} />);

    expect(screen.getByRole("status")).toHaveTextContent(/usage is currently limited/i);
    fireEvent.click(screen.getByRole("button", { name: /sign out/i }));

    await waitFor(() => expect(update).toHaveBeenCalledWith(expect.objectContaining({ transactionError: "remote logout failed; retry in Codex" })));
  });

  it("opens the returned login URL through the typed system-browser bridge", async () => {
    const update = vi.fn();
    const url = "https://auth.example/login?state=opaque";
    render(<Welcome state={welcomeState({ available: true, authenticated: false, auth_mode: "chatgpt", usage_limited: false, error: "signed out" }, { available: true, auth_url: url, device_code: false })} update={update} />);
    fireEvent.click(screen.getByRole("button", { name: /open the chatgpt sign-in page/i }));

    await waitFor(() => expect(openCodexLoginUrlResult).toHaveBeenCalledWith(url));
    expect(update).toHaveBeenCalledWith({ transactionError: undefined });
  });

  it("exposes a cancellable pending sign-in state", async () => {
    const update = vi.fn();
    render(<Welcome state={{ ...welcomeState({ available: true, authenticated: false, auth_mode: "chatgpt", usage_limited: false }), codexLoginPending: true, codexLogin: { available: true, login_id: "login-1", device_code: false } }} update={update} />);
    fireEvent.click(screen.getByRole("button", { name: /cancel sign-in/i }));

    await waitFor(() => expect(cancelCodexLogin).toHaveBeenCalledOnce());
    expect(update).toHaveBeenCalledWith(expect.objectContaining({ codexLoginPending: false, transactionError: expect.stringMatching(/cancelled/i) }));
  });

  it("keeps unresolved dry-run conflicts blocking installation", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /create new mod/i }));
    fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    for (let index = 0; index < 6; index += 1) fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByText("Unresolved conflicts")).toBeInTheDocument();
    expect(screen.getByText("Plan unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /start installation/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /resolve conflicts/i })).toBeInTheDocument();
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
    vi.mocked(previewSourceManifest).mockResolvedValue(manifest);

    render(<Components state={{ sourceMode: "latest", pinnedRef: "", selectedComponents: [] } as unknown as WizardState} update={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("1 declared files · destination: .agents/skills/")).toBeInTheDocument());
    expect(screen.getByText("Dependencies and file list")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Dependencies and file list"));
    expect(screen.getByText("Requires core.agents · all")).toBeInTheDocument();
    expect(screen.getByText("1 declared files · destination: .agents/skills/")).toBeInTheDocument();
  });
});
