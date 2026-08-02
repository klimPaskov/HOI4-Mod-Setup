import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyInstallationResult, approveInstallation, approveScanEvidence, buildInstallationPlan, buildMaintenancePlan, cancelCodexLogin, cancelScan, confirmCodexAnalysis, evaluateReadiness, openExternalUrlResult, previewDescriptors, readMeshyCredential, runAiAnalysis, runCodexAnalysis, scanProject } from "./tauri";
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

  it("uses the typed external-link command for reviewed URLs", async () => {
    invoke.mockResolvedValue(undefined);

    const result = await openExternalUrlResult("https://github.com/klimPaskov/Agentic-HOI4-Modding");

    expect(result.error).toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("open_external_url", {
      url: "https://github.com/klimPaskov/Agentic-HOI4-Modding",
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

    expect(invoke).toHaveBeenNthCalledWith(1, "codex_analyze", { request: codexRequest });
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
