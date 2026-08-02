import type { AiAccountStatus, AiAnalysisRequest, AiProviderProfile, AppUpdateStatus, CodexAccountStatus, CodexAnalysisRecord, CodexAnalysisRequest, CodexAnalysisResult, CodexLoginStart, ConflictPreview, CredentialReference, FolderSelection, GeneratedArtifactPreview, GitOnlineAction, GitOnlinePlan, GitOnlineResult, InstallationPlan, OpenInCodexResult, ReadinessReport, ScanProgress, ScanSnapshot, SourceManifestPreview, SuggestedProjectPaths, TransactionJournal, WizardState, WorkflowHealthResult } from "../types";

interface RawScanFinding {
  id: string;
  key: string;
  value: unknown;
  status: string;
  evidence?: Array<{ path: string; confidence: number; note?: string }>;
}

interface RawScanResult {
  scan_id: string;
  project_root: string;
  completed_at?: string | null;
  partial?: boolean;
  cancelled?: boolean;
  limits_hit?: string[];
  files_scanned?: number;
  directories_scanned?: number;
  bytes_read?: number;
  findings?: RawScanFinding[];
}

interface RawScanProgress {
  request_id: string;
  stage: string;
  current_path: string;
  files_scanned: number;
  directories_scanned: number;
  bytes_read: number;
}

interface RawReadinessReport {
  checks: Array<{ id: string; label: string; status: string; blocking: boolean; message?: string }>;
  open_in_codex: { enabled: boolean; blocking_check_ids: string[] };
  core_ready: boolean;
  codex?: { authenticated_during_setup: boolean; analysis_status: string; confirmed_field_count: number };
}

interface ReadinessInput {
  project_id: string;
  project_root: string;
  workflow_3d_state: string;
}

interface TauriCommandMap {
  app_update_check: { args: Record<string, never>; result: AppUpdateStatus };
  app_update_install: { args: Record<string, never>; result: void };
  ai_provider_profiles: { args: Record<string, never>; result: AiProviderProfile[] };
  ai_account_read: { args: { provider: string; model: string; endpoint: string }; result: AiAccountStatus };
  store_ai_provider_credential: { args: { provider: string; value: string }; result: boolean };
  remove_ai_provider_credential: { args: { provider: string }; result: boolean };
  ai_analyze: { args: { request: AiAnalysisRequest }; result: CodexAnalysisResult };
  approve_scan_evidence: { args: { projectRoot: string; scanId: string; evidence: Array<{ reference: string; path: string; excerpt: string; excerpt_sha256: string; confidence?: number | null }> }; result: void };
  codex_account_read: { args: Record<string, never>; result: CodexAccountStatus };
  codex_login_start: { args: { mode: "browser" | "device" }; result: CodexLoginStart };
  codex_login_wait: { args: { loginId: string }; result: CodexAccountStatus };
  codex_login_cancel: { args: { loginId: string }; result: void };
  open_codex_login_url: { args: { url: string }; result: void };
  open_external_url: { args: { url: string }; result: void };
  codex_logout: { args: Record<string, never>; result: void };
  codex_analyze: { args: { request: CodexAnalysisRequest }; result: CodexAnalysisResult };
  confirm_codex_analysis: { args: { record: CodexAnalysisRecord; confirmedFields: string[]; confirmedValues: unknown }; result: CodexAnalysisRecord };
  pick_project_folder: { args: Record<string, never>; result: FolderSelection };
  pick_launcher_folder: { args: Record<string, never>; result: FolderSelection };
  suggest_project_paths: { args: { projectId: string }; result: SuggestedProjectPaths };
  preview_source_manifest: { args: { sourceMode: WizardState["sourceMode"]; pinnedRef: string }; result: SourceManifestPreview };
  store_meshy_credential: { args: { value: string }; result: CredentialReference };
  meshy_credential_status: { args: Record<string, never>; result: CredentialReference | null };
  remove_meshy_credential: { args: { reference: CredentialReference }; result: void };
  scan_project: { args: { root: string; requestId: string; launcherDescriptorPath?: string | null }; result: RawScanResult };
  cancel_scan: { args: { requestId: string }; result: void };
  evaluate_readiness: { args: { input: ReadinessInput }; result: RawReadinessReport };
  run_3d_health_check: { args: { projectRoot: string }; result: WorkflowHealthResult };
  run_mcp_health_check: { args: { projectRoot: string }; result: WorkflowHealthResult };
  preview_descriptors: { args: { state: WizardState }; result: GeneratedArtifactPreview[] };
  preview_installation_conflict: { args: { planId: string; path: string }; result: ConflictPreview };
  build_installation_plan: { args: { state: WizardState }; result: InstallationPlan };
  build_maintenance_plan: { args: { mode: "update" | "repair" | "reinstall" | "remove"; projectRoot: string; codexAnalysis?: CodexAnalysisRecord | null; addOptionalComponents?: string[] }; result: InstallationPlan };
  approve_installation: { args: { planId: string }; result: void };
  resolve_installation_conflict: { args: { planId: string; path: string; choice: string }; result: InstallationPlan };
  apply_installation: { args: { planId: string; projectRoot: string }; result: TransactionJournal };
  rollback_installation: { args: { projectRoot: string; transactionId: string }; result: TransactionJournal };
  read_transaction_journal: { args: { projectRoot: string; transactionId: string }; result: TransactionJournal };
  find_interrupted_transaction: { args: { projectRoot: string }; result: TransactionJournal | null };
  resume_installation: { args: { projectRoot: string; transactionId: string }; result: TransactionJournal };
  discard_installation_staging: { args: { projectRoot: string; transactionId: string }; result: TransactionJournal };
  open_in_codex: { args: { projectRoot: string }; result: OpenInCodexResult };
  git_online_prepare: { args: { projectRoot: string; action: Exclude<GitOnlineAction, "none">; remoteName: string; repository: string; branch: string }; result: GitOnlinePlan };
  git_online_action: { args: { projectRoot: string; planId: string; confirmed: boolean; transactionId?: string | null }; result: GitOnlineResult };
}

export async function checkForAppUpdate(): Promise<CommandResult<AppUpdateStatus>> {
  return invokeCommandResult("app_update_check", {});
}

export async function installAppUpdate(): Promise<CommandResult<void>> {
  return invokeCommandResult("app_update_install", {});
}

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export interface CommandResult<T> {
  value: T | null;
  error?: string;
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined"
    && "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>);
}

async function resolveInvoke(): Promise<Invoke | null> {
  try {
    const module = await import("@tauri-apps/api/core");
    return module.invoke as Invoke;
  } catch {
    return null;
  }
}

export async function invokeCommand<Command extends keyof TauriCommandMap>(
  command: Command,
  args: TauriCommandMap[Command]["args"],
): Promise<TauriCommandMap[Command]["result"] | null> {
  return (await invokeCommandResult(command, args)).value;
}

export async function invokeCommandResult<Command extends keyof TauriCommandMap>(
  command: Command,
  args: TauriCommandMap[Command]["args"],
): Promise<CommandResult<TauriCommandMap[Command]["result"]>> {
  const invoke = await resolveInvoke();
  if (!invoke) return { value: null, error: "The packaged desktop bridge is unavailable." };
  try {
    return { value: await invoke<TauriCommandMap[Command]["result"]>(command, args as Record<string, unknown>) };
  } catch (error) {
    return {
      value: null,
      error: typeof error === "string" ? error : error instanceof Error ? error.message : "The desktop command failed.",
    };
  }
}

export async function storeMeshyCredential(value: string): Promise<CredentialReference | null> {
  if (!value.trim()) return null;
  return invokeCommand("store_meshy_credential", { value });
}

export async function readMeshyCredential(): Promise<CredentialReference | null> {
  return invokeCommand("meshy_credential_status", {});
}

export async function removeMeshyCredential(reference: CredentialReference): Promise<boolean> {
  return (await invokeCommandResult("remove_meshy_credential", { reference })).error === undefined;
}

export async function readCodexAccount(): Promise<CodexAccountStatus | null> {
  return invokeCommand("codex_account_read", {});
}

export async function readAiProviderProfiles(): Promise<AiProviderProfile[]> {
  return (await invokeCommand("ai_provider_profiles", {})) ?? [];
}

export async function readAiAccount(
  provider: string,
  model: string,
  endpoint: string,
): Promise<AiAccountStatus | null> {
  return invokeCommand("ai_account_read", {
    provider,
    model,
    endpoint,
  });
}

export async function storeAiProviderCredential(provider: string, value: string): Promise<boolean> {
  if (!value.trim()) return false;
  return (await invokeCommand("store_ai_provider_credential", { provider, value })) === true;
}

export async function removeAiProviderCredential(provider: string): Promise<boolean> {
  return (await invokeCommand("remove_ai_provider_credential", { provider })) === true;
}

export async function startCodexLogin(mode: "browser" | "device"): Promise<CodexLoginStart | null> {
  return invokeCommand("codex_login_start", { mode });
}

export async function waitForCodexLogin(loginId: string): Promise<CodexAccountStatus | null> {
  return invokeCommand("codex_login_wait", { loginId });
}

export async function waitForCodexLoginResult(loginId: string): Promise<CommandResult<CodexAccountStatus>> {
  return invokeCommandResult("codex_login_wait", { loginId });
}

export async function cancelCodexLogin(loginId: string): Promise<boolean> {
  if (!loginId.trim()) return false;
  return (await invokeCommandResult("codex_login_cancel", { loginId })).error === undefined;
}

export async function openCodexLoginUrlResult(url: string): Promise<CommandResult<void>> {
  return invokeCommandResult("open_codex_login_url", { url });
}

export async function openExternalUrlResult(url: string): Promise<CommandResult<void>> {
  return invokeCommandResult("open_external_url", { url });
}

export async function logoutCodex(): Promise<boolean> {
  return (await logoutCodexResult()).error === undefined;
}

export async function logoutCodexResult(): Promise<CommandResult<void>> {
  return invokeCommandResult("codex_logout", {});
}

export async function runCodexAnalysis(request: CodexAnalysisRequest): Promise<CodexAnalysisResult | null> {
  return invokeCommand("codex_analyze", { request });
}

export async function runCodexAnalysisResult(
  request: CodexAnalysisRequest,
): Promise<CommandResult<CodexAnalysisResult>> {
  return invokeCommandResult("codex_analyze", { request });
}

export async function runAiAnalysis(request: AiAnalysisRequest): Promise<CodexAnalysisResult | null> {
  return invokeCommand("ai_analyze", { request });
}

export async function runAiAnalysisResult(
  request: AiAnalysisRequest,
): Promise<CommandResult<CodexAnalysisResult>> {
  return invokeCommandResult("ai_analyze", { request });
}

export async function approveScanEvidence(
  projectRoot: string,
  scanId: string,
  evidence: Array<{ reference: string; path: string; excerpt: string; excerpt_sha256: string; confidence?: number | null }>,
): Promise<boolean> {
  return (await invokeCommandResult("approve_scan_evidence", { projectRoot, scanId, evidence })).error === undefined;
}

export async function confirmCodexAnalysis(record: CodexAnalysisRecord, confirmedFields: string[], confirmedValues: unknown): Promise<CodexAnalysisRecord | null> {
  return invokeCommand("confirm_codex_analysis", { record, confirmedFields, confirmedValues });
}

export async function pickProjectFolder(): Promise<FolderSelection | null> {
  return invokeCommand("pick_project_folder", {});
}

export async function pickLauncherFolder(): Promise<FolderSelection | null> {
  return invokeCommand("pick_launcher_folder", {});
}

export async function suggestProjectPaths(projectId: string): Promise<SuggestedProjectPaths | null> {
  return invokeCommand("suggest_project_paths", { projectId });
}

export async function previewSourceManifest(sourceMode: WizardState["sourceMode"], pinnedRef: string): Promise<SourceManifestPreview | null> {
  return (await previewSourceManifestResult(sourceMode, pinnedRef)).value;
}

export async function previewSourceManifestResult(sourceMode: WizardState["sourceMode"], pinnedRef: string): Promise<CommandResult<SourceManifestPreview>> {
  return invokeCommandResult("preview_source_manifest", { sourceMode, pinnedRef });
}

function newScanRequestId(): string {
  if (typeof globalThis.crypto?.randomUUID === "function") return globalThis.crypto.randomUUID();
  return `scan-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export async function scanProject(
  root: string,
  onProgress?: (progress: ScanProgress) => void,
  onRequestId?: (requestId: string) => void,
  launcherDescriptorPath?: string,
): Promise<ScanSnapshot | null> {
  const requestId = newScanRequestId();
  onRequestId?.(requestId);
  let unlisten: (() => void) | undefined;
  try {
    if (onProgress) {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<RawScanProgress>("scan-progress", (event) => {
          if (event.payload.request_id !== requestId) return;
          onProgress({
            stage: event.payload.stage,
            currentPath: event.payload.current_path,
            filesScanned: event.payload.files_scanned,
            directoriesScanned: event.payload.directories_scanned,
            bytesRead: event.payload.bytes_read,
          });
        });
      } catch {
        // The browser preview has no Tauri event bus; the final command result remains authoritative.
      }
    }
    const result = await invokeCommand("scan_project", {
      root,
      requestId,
      launcherDescriptorPath: launcherDescriptorPath || null,
    });
    if (!result) return null;
    return {
      findings: (result.findings ?? []).map((finding) => ({
        id: finding.id,
        label: finding.key,
        value: typeof finding.value === "string" ? finding.value : JSON.stringify(finding.value) ?? "",
        evidenceExcerpt: typeof finding.value === "string" ? finding.value : JSON.stringify(finding.value) ?? "",
        confidence: Math.max(0, ...(finding.evidence ?? []).map((evidence) => evidence.confidence)),
        evidencePath: finding.evidence?.[0]?.path,
        status: finding.status === "blocking" ? "blocking" : finding.status === "needs_review" ? "needs_review" : "accepted",
        evidence: (finding.evidence ?? []).map((evidence) => `${evidence.path}${evidence.note ? ` - ${evidence.note}` : ""}`).join("; "),
      })),
      scanId: result.scan_id,
      projectRoot: result.project_root,
      completedAt: result.completed_at,
      partial: result.partial ?? false,
      cancelled: result.cancelled ?? false,
      limitsHit: result.limits_hit ?? [],
      filesScanned: result.files_scanned ?? 0,
      directoriesScanned: result.directories_scanned ?? 0,
      bytesRead: result.bytes_read ?? 0,
    };
  } finally {
    unlisten?.();
  }
}

export async function cancelScan(requestId: string): Promise<CommandResult<void>> {
  return invokeCommandResult("cancel_scan", { requestId });
}

export async function evaluateReadiness(projectRoot: string, projectId: string, optional3d: string): Promise<ReadinessReport | null> {
  const raw = await invokeCommand("evaluate_readiness", {
    input: {
      project_id: projectId,
      project_root: projectRoot,
      workflow_3d_state: optional3d,
    },
  });
  if (!raw) return null;
  return {
    openInCodex: raw.open_in_codex.enabled,
    coreReady: raw.core_ready,
    blockingCheckIds: raw.open_in_codex.blocking_check_ids,
    codex: raw.codex,
    checks: raw.checks.map((check) => ({
      id: check.id,
      label: check.label,
      status: check.status,
      blocking: check.blocking,
      message: check.message ?? "",
    })),
  };
}

export async function run3DHealthCheck(projectRoot: string): Promise<WorkflowHealthResult | null> {
  return invokeCommand("run_3d_health_check", { projectRoot });
}

export async function runMcpHealthCheck(projectRoot: string): Promise<WorkflowHealthResult | null> {
  return invokeCommand("run_mcp_health_check", { projectRoot });
}

export async function previewDescriptors(state: WizardState): Promise<GeneratedArtifactPreview[]> {
  return (await invokeCommand("preview_descriptors", { state: stateForCore(state) })) ?? [];
}

export async function previewDescriptorsResult(state: WizardState): Promise<CommandResult<GeneratedArtifactPreview[]>> {
  return invokeCommandResult("preview_descriptors", { state: stateForCore(state) });
}

export async function previewInstallationConflict(planId: string, path: string): Promise<ConflictPreview | null> {
  return invokeCommand("preview_installation_conflict", { planId, path });
}

export async function buildInstallationPlan(state: WizardState): Promise<InstallationPlan | null> {
  return invokeCommand("build_installation_plan", { state: stateForCore(state) });
}

export async function buildInstallationPlanResult(state: WizardState): Promise<CommandResult<InstallationPlan>> {
  return invokeCommandResult("build_installation_plan", { state: stateForCore(state) });
}

function stateForCore(state: WizardState): WizardState {
  // Meshy input is a password draft. Keep the field for the typed command
  // shape, but never serialize its value across the renderer/core boundary.
  return { ...state, meshKeyDraft: "" };
}

export async function approveInstallation(planId: string): Promise<boolean> {
  return (await invokeCommandResult("approve_installation", { planId })).error === undefined;
}

export async function resolveInstallationConflict(planId: string, path: string, choice: string): Promise<InstallationPlan | null> {
  return invokeCommand("resolve_installation_conflict", { planId, path, choice });
}

export async function buildMaintenancePlan(mode: "update" | "repair" | "reinstall" | "remove", projectRoot: string, codexAnalysis?: CodexAnalysisRecord, addOptionalComponents: string[] = []): Promise<InstallationPlan | null> {
  return invokeCommand("build_maintenance_plan", { mode, projectRoot, codexAnalysis: codexAnalysis ?? null, addOptionalComponents });
}

export async function rollbackInstallation(projectRoot: string, transactionId: string): Promise<TransactionJournal | null> {
  return invokeCommand("rollback_installation", { projectRoot, transactionId });
}

export async function rollbackInstallationResult(projectRoot: string, transactionId: string): Promise<CommandResult<TransactionJournal>> {
  return invokeCommandResult("rollback_installation", { projectRoot, transactionId });
}

export async function readTransactionJournal(projectRoot: string, transactionId: string): Promise<TransactionJournal | null> {
  return invokeCommand("read_transaction_journal", { projectRoot, transactionId });
}

export async function findInterruptedTransaction(projectRoot: string): Promise<TransactionJournal | null> {
  return invokeCommand("find_interrupted_transaction", { projectRoot });
}

export async function resumeInstallation(projectRoot: string, transactionId: string): Promise<TransactionJournal | null> {
  return invokeCommand("resume_installation", { projectRoot, transactionId });
}

export async function discardInstallationStaging(projectRoot: string, transactionId: string): Promise<TransactionJournal | null> {
  return invokeCommand("discard_installation_staging", { projectRoot, transactionId });
}

export async function applyInstallation(planId: string, projectRoot: string): Promise<TransactionJournal | null> {
  return invokeCommand("apply_installation", { planId, projectRoot });
}

export async function applyInstallationResult(planId: string, projectRoot: string): Promise<CommandResult<TransactionJournal>> {
  return invokeCommandResult("apply_installation", { planId, projectRoot });
}

export async function openInCodex(projectRoot: string): Promise<OpenInCodexResult | null> {
  return invokeCommand("open_in_codex", { projectRoot });
}

export async function prepareGitOnlineAction(args: TauriCommandMap["git_online_prepare"]["args"]): Promise<CommandResult<GitOnlinePlan>> {
  return invokeCommandResult("git_online_prepare", args);
}

export async function runGitOnlineAction(args: TauriCommandMap["git_online_action"]["args"]): Promise<CommandResult<GitOnlineResult>> {
  return invokeCommandResult("git_online_action", args);
}
