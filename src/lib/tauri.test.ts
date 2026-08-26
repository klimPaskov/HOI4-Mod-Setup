import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyInstallationResult, approveInstallation, approveScanEvidence, buildInstallationPlan, buildMaintenancePlan, cancelCodexLogin, cancelScan, confirmCodexAnalysis, evaluateReadiness, inspectLocalPortraitProvider, installLocalPortraitWorkflows, openExternalUrlResult, packageChatSources, pickChatSourcesFolder, previewChatSources, previewDescriptors, readMeshyCredential, runAiAnalysis, runCodexAnalysis, scanProject } from "./tauri";
import type { AiAnalysisRequest, CodexAnalysisRequest, ScanProgress, WizardState } from "../types";

const { invoke, listen, unlisten } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

describe("typed scanner bridge", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
    unlisten.mockReset();
  });

  it("filters progress events by the opaque request ID and cleans up the listener", async () => {
    let handler: ((event: { payload: { request_id: string; stage: string; current_path: string; files_scanned: number; directories_scanned: number; bytes_read: number } }) => void) | undefined;
    listen.mockImplementation(async (_name: string, next: typeof handler) => {
      handler = next;
      return unlisten;
    });
    invoke.mockImplementation(async (_command: string, args: { requestId: string }) => {
      handler?.({ payload: { request_id: "different-request", stage: "wrong", current_path: "wrong", files_scanned: 99, directories_scanned: 99, bytes_read: 99 } });
      handler?.({ payload: { request_id: args.requestId, stage: "detecting_git", current_path: ".git", files_scanned: 4, directories_scanned: 2, bytes_read: 2048 } });
      return { scan_id: "scan-1", project_root: "C:/mods/example", completed_at: null, partial: false, cancelled: false, limits_hit: [], files_scanned: 4, directories_scanned: 2, bytes_read: 2048, findings: [] };
    });
    const progress: ScanProgress[] = [];
    let requestId = "";

    const result = await scanProject("C:/mods/example", (update) => progress.push(update), (id) => { requestId = id; });

    expect(requestId).toMatch(/^[0-9a-f-]{36}$/i);
    expect(progress).toEqual([{ stage: "detecting_git", currentPath: ".git", filesScanned: 4, directoriesScanned: 2, bytesRead: 2048 }]);
    expect(result?.filesScanned).toBe(4);
    expect(invoke).toHaveBeenCalledWith("scan_project", expect.objectContaining({ root: "C:/mods/example", requestId }));
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("uses the typed cancellation command for the active request", async () => {
    invoke.mockResolvedValue(undefined);

    const result = await cancelScan("scan-request");

    expect(result.value).toBeUndefined();
    expect(result.error).toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("cancel_scan", { requestId: "scan-request" });
  });

  it("preserves finding origins and exposes core scan conflicts for review", async () => {
    invoke.mockResolvedValue({
      scan_id: "scan-1",
      project_root: "C:/mods/example",
      partial: false,
      cancelled: false,
      findings: [{
        id: "agents.present",
        category: "documentation",
        key: "project_instructions",
        value: "AGENTS.md",
        status: "accepted",
        origin: "deterministic",
        recommendation: "Keep project instructions.",
        evidence: [{ path: "AGENTS.md", confidence: 1, note: "Detected instructions" }],
      }],
      conflicts: [{
        id: "conflict.git.inspection",
        path: ".git",
        kind: "unsafe_git_configuration",
        severity: "block",
        details: "Git configuration changed during inspection.",
      }],
    });

    const result = await scanProject("C:/mods/example");

    expect(result?.findings).toEqual(expect.arrayContaining([
      expect.objectContaining({ id: "agents.present", label: "Detected · project_instructions", origin: "deterministic" }),
      expect.objectContaining({ id: "conflict.git.inspection", label: "Blocking conflict · unsafe git configuration", status: "blocking" }),
    ]));
  });

  it("uses the typed external-link command for reviewed URLs", async () => {
    invoke.mockResolvedValue(undefined);

    const result = await openExternalUrlResult("https://github.com/klimPaskov/Agentic-HOI4-Modding");

    expect(result.error).toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("open_external_url", {
      url: "https://github.com/klimPaskov/Agentic-HOI4-Modding",
    });
  });

  it("uses typed commands for ChatGPT source preview and packaging", async () => {
    invoke.mockResolvedValueOnce({ path: "C:/Users/Player/Downloads" }).mockResolvedValueOnce({ eligible: true, files: [] }).mockResolvedValueOnce({ archive_path: "C:/Users/Player/Downloads/example.zip", included_files: [], bytes: 0, sha256: "a".repeat(64) });

    await pickChatSourcesFolder();
    await previewChatSources("C:/mods/example");
    await packageChatSources("C:/mods/example", "C:/Users/Player/Downloads", ["AGENTS.md"]);

    expect(invoke).toHaveBeenNthCalledWith(1, "pick_chat_sources_folder", {});
    expect(invoke).toHaveBeenNthCalledWith(2, "preview_chat_sources", { projectRoot: "C:/mods/example" });
    expect(invoke).toHaveBeenNthCalledWith(3, "package_chat_sources", {
      projectRoot: "C:/mods/example",
      destinationDirectory: "C:/Users/Player/Downloads",
      selectedFileIds: ["AGENTS.md"],
    });
  });

  it("binds Codex cancellation to the active App Server login ID", async () => {
    invoke.mockResolvedValue(null);

    const cancelled = await cancelCodexLogin("login-1");

    expect(cancelled).toBe(true);
    expect(invoke).toHaveBeenCalledWith("codex_login_cancel", { loginId: "login-1" });
  });

  it("treats a null unit-command payload as successful evidence approval", async () => {
    invoke.mockResolvedValue(null);
    const evidence = [{
      reference: "finding-1",
      path: "descriptor.mod",
      excerpt: "name=\"Example\"",
      excerpt_sha256: "a".repeat(64),
      confidence: 1,
    }];

    const approved = await approveScanEvidence("C:/mods/example", "scan-1", evidence);

    expect(approved).toBe(true);
    expect(invoke).toHaveBeenCalledWith("approve_scan_evidence", {
      projectRoot: "C:/mods/example",
      scanId: "scan-1",
      evidence,
    });
  });

  it("treats a null unit-command payload as successful installation approval", async () => {
    invoke.mockResolvedValue(null);

    const approved = await approveInstallation("plan-1");

    expect(approved).toBe(true);
    expect(invoke).toHaveBeenCalledWith("approve_installation", { planId: "plan-1" });
  });

  it("starts installation with only the core-owned plan ID", async () => {
    invoke.mockResolvedValue(null);

    await applyInstallationResult("plan-1", "C:/mods/example");

    expect(invoke).toHaveBeenCalledWith("apply_installation", {
      planId: "plan-1",
      projectRoot: "C:/mods/example",
    });
    expect(JSON.stringify(invoke.mock.calls)).not.toContain("codex_analysis");
  });

  it("reads an existing Meshy vault reference without sending a key value", async () => {
    invoke.mockResolvedValue({
      name: "MESHY_API_KEY",
      provider: "windows_credential_manager",
      reference: "credential://meshy_api_key/default",
    });

    const reference = await readMeshyCredential();

    expect(reference?.reference).toBe("credential://meshy_api_key/default");
    expect(invoke).toHaveBeenCalledWith("meshy_credential_status", {});
    expect(JSON.stringify(invoke.mock.calls)).not.toMatch(/msy_|secret|key[_-]?value/i);
  });

  it("passes existing-project optional workflow choices to the core", async () => {
    invoke.mockResolvedValue(undefined);

    await buildMaintenancePlan("repair", "C:/mods/example", undefined, [
      "workflow.3d",
      "workflow.super_events",
    ]);

    expect(invoke).toHaveBeenCalledWith("build_maintenance_plan", {
      mode: "repair",
      projectRoot: "C:/mods/example",
      codexAnalysis: null,
      addOptionalComponents: ["workflow.3d", "workflow.super_events"],
    });
  });

  it("persists the selected portrait provider through readiness and maintenance commands", async () => {
    invoke.mockResolvedValue({
      checks: [],
      open_in_codex: { enabled: true, blocking_check_ids: [] },
      core_ready: true,
    });
    const portrait = {
      enabled: true,
      provider: "cloud",
      providerStatus: "needs_authorization",
      workflowRepository: "https://github.com/klimPaskov/comfyui-hoi4-portraits",
      workflowBranch: "codex/portrait-pipeline",
      workflowCommit: "b47222a77f2f6454704530865aa1441fad48bdd3",
      preferredWorkflow: "source",
      localComfyuiRoot: "",
      localServerUrl: "http://127.0.0.1:8188",
      runpodUrl: "",
      runpodWorkspace: "/workspace/comfyui-hoi4-portraits",
      mcpRegistered: true,
    } as NonNullable<WizardState["portraitPipeline"]>;

    await evaluateReadiness("C:/mods/example", "example", "not_selected", portrait);
    await buildMaintenancePlan("repair", "C:/mods/example", undefined, [], portrait);

    expect(invoke).toHaveBeenNthCalledWith(1, "evaluate_readiness", {
      input: expect.objectContaining({
        portrait_provider: "cloud",
        portrait_provider_status: "needs_authorization",
      }),
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "build_maintenance_plan", {
      mode: "repair",
      projectRoot: "C:/mods/example",
      codexAnalysis: null,
      addOptionalComponents: [],
      portraitPipeline: portrait,
    });
  });

  it("uses typed local portrait discovery and workflow installation commands", async () => {
    invoke.mockResolvedValue({
      status: "needs_workflow_install",
      configuredRoot: "C:/ComfyUI",
      detectedRoot: "C:/ComfyUI",
      serverUrl: "http://127.0.0.1:8188",
      serverStatus: "ready",
      hardwareStatus: "ready",
      gpuName: "NVIDIA test",
      vramGb: 24,
      workflowStatus: "missing",
      modelStatus: "ready",
      huggingfaceAccessHint: true,
      message: "workflow missing",
      canonicalRepository: "https://github.com/klimPaskov/comfyui-hoi4-portraits",
      canonicalCommit: "b47222a77f2f6454704530865aa1441fad48bdd3",
      installCommand: "python scripts/install_workflows.py --comfyui-root <COMFYUI_ROOT>",
    });

    await inspectLocalPortraitProvider("C:/ComfyUI", "http://127.0.0.1:8188");
    await installLocalPortraitWorkflows("C:/ComfyUI");

    expect(invoke).toHaveBeenNthCalledWith(1, "inspect_local_portrait_provider", {
      configuredRoot: "C:/ComfyUI",
      serverUrl: "http://127.0.0.1:8188",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "install_local_portrait_workflows", {
      comfyuiRoot: "C:/ComfyUI",
    });
  });

  it("sends readiness without portrait-workflow setup state", async () => {
    invoke.mockResolvedValue({
      checks: [],
      open_in_codex: { enabled: true, blocking_check_ids: [] },
      core_ready: true,
    });

    await evaluateReadiness("C:/mods/example", "example", "not_selected");

    expect(invoke).toHaveBeenCalledWith("evaluate_readiness", {
      input: {
        project_id: "example",
        project_root: "C:/mods/example",
        workflow_3d_state: "not_selected",
      },
    });
    expect(JSON.stringify(invoke.mock.calls)).not.toMatch(/lora|comfyui|portrait/i);
  });

  it("wraps semantic analysis requests under the Rust command argument name", async () => {
    invoke.mockResolvedValue(null);
    const codexRequest = { mode: "new_project_identity" } as unknown as CodexAnalysisRequest;
    const providerRequest = { mode: "new_project_identity", provider: "claude" } as unknown as AiAnalysisRequest;

    await runCodexAnalysis(codexRequest);
    await runAiAnalysis(providerRequest);

    expect(invoke).toHaveBeenNthCalledWith(1, "codex_analyze", {
      request: codexRequest,
      model: "gpt-5.6-luna",
      reasoningEffort: "xhigh",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "ai_analyze", { request: providerRequest });
  });

  it("sends the reviewed render values with semantic confirmation", async () => {
    invoke.mockResolvedValue(null);
    const record = { analysis_id: "analysis-1" } as never;
    const values = { description: "description", folderProfile: ["common"], identity: {} };

    await confirmCodexAnalysis(record, ["project_id"], values);

    expect(invoke).toHaveBeenCalledWith("confirm_codex_analysis", {
      record,
      confirmedFields: ["project_id"],
      confirmedValues: values,
    });
  });

  it("never serializes the Meshy password draft in state-bearing core commands", async () => {
    invoke.mockResolvedValue([]);
    const state = { meshKeyDraft: "test-meshy-draft-not-for-transport" } as unknown as WizardState;

    await previewDescriptors(state);
    await buildInstallationPlan(state);

    expect(invoke).toHaveBeenNthCalledWith(1, "preview_descriptors", { state: { meshKeyDraft: "" } });
    expect(invoke).toHaveBeenNthCalledWith(2, "build_installation_plan", { state: { meshKeyDraft: "" } });
    expect(JSON.stringify(invoke.mock.calls)).not.toContain("test-meshy-draft-not-for-transport");
  });
});
