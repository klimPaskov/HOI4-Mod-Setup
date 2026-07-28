export type ScreenId =
  | "welcome"
  | "description"
  | "identity"
  | "scan"
  | "findings"
  | "components"
  | "workflows"
  | "mesh"
  | "lora"
  | "mcp"
  | "git"
  | "dry-run"
  | "install"
  | "ready"
  | "update"
  | "conflict"
  | "recovery";

export type PhaseId = "project" | "review" | "components" | "integrations" | "git" | "install" | "ready";

export type SourceMode = "latest" | "pinned_commit" | "pinned_release";

export type WorkflowState = "not_selected" | "selected_pending" | "ready" | "incomplete" | "interest_recorded" | "planned_unavailable" | "unsupported_platform";

export type StatusTone = "pass" | "review" | "block" | "info" | "muted";

export type ConflictChoice = "keep" | "replace" | "merge" | "rename" | "skip";

export type RecoveryChoice = "resume" | "rollback" | "discard";

export type InstallationAction = "create" | "replace" | "merge" | "rename" | "skip" | "delete_managed" | "generate" | "chmod" | "external";

export type InstallationLocalState = "absent" | "unmodified" | "modified" | "unknown";

export type InstallationRollbackAction = "remove_created" | "restore_backup" | "reverse_merge" | "none";

export interface ProjectIdentity {
  displayName: string;
  projectId: string;
  author: string;
  version: string;
  supportedGameVersion: string;
  projectRoot: string;
  defaultBranch: string;
  scriptPrefix?: string;
  primaryNamespace?: string;
  descriptorTags?: string[];
  launcherDescriptorPath?: string;
}

export interface CodexAccountStatus {
  available: boolean;
  authenticated: boolean;
  auth_mode: string;
  email?: string | null;
  plan_type?: string | null;
  usage_limited: boolean;
  app_server_version?: string | null;
  error?: string | null;
}

export interface CodexLoginStart {
    available: boolean;
    login_id?: string | null;
    auth_url?: string | null;
  verification_url?: string | null;
  user_code?: string | null;
  device_code: boolean;
  error?: string | null;
}

export interface CodexProposal {
  key: string;
  value: unknown;
  confidence: number;
  reason: string;
  evidence_refs: string[];
}

export interface CodexAnalysis {
  schema_version: string;
  analysis_id: string;
  mode: "new_project_identity" | "existing_project_semantics";
  input_sha256: string;
  project_summary: string;
  proposals: CodexProposal[];
  component_recommendations: Array<{ component_id: string; recommendation: string; reason: string }>;
  warnings: string[];
}

export interface CodexAnalysisRecord {
  engine: string;
  auth_mode: string;
  analysis_id: string;
  schema_version: string;
  input_sha256: string;
  output_sha256: string;
  confirmed_fields: string[];
  confirmed_at: string;
  account_identity_persisted?: boolean;
  analysis_purpose?: "existing_project_import" | "maintenance_reanalysis";
  evidence_sha256?: string;
}

export interface CodexAnalysisRequest {
  mode: "new_project_identity" | "existing_project_semantics";
  brief: string;
  evidence: Array<{ reference: string; path: string; excerpt: string; excerpt_sha256: string; confidence?: number | null }>;
  constraints: unknown;
  analysis_purpose?: "existing_project_import" | "maintenance_reanalysis";
  project_root?: string;
  scan_id?: string;
}

export interface CodexAnalysisResult {
  analysis: CodexAnalysis;
  record: CodexAnalysisRecord;
}

export interface ScanFinding {
  id: string;
  label: string;
  value: string;
  /** Immutable value returned by the completed core scan; edits stay review-only. */
  evidenceExcerpt?: string;
  confidence: number;
  status: "accepted" | "needs_review" | "edited" | "rejected" | "blocking";
  evidence: string;
  evidencePath?: string;
}

export interface ScanProgress {
  stage: string;
  currentPath: string;
  filesScanned: number;
  directoriesScanned: number;
  bytesRead: number;
}

export interface ScanSnapshot {
  findings: ScanFinding[];
  scanId: string;
  projectRoot: string;
  completedAt?: string | null;
  partial: boolean;
  cancelled: boolean;
  limitsHit: string[];
  filesScanned: number;
  directoriesScanned: number;
  bytesRead: number;
}

export interface ComponentRow {
  id: string;
  title: string;
  detail: string;
  size: string;
  selected: boolean;
  required?: boolean;
  platform?: "all" | "windows" | "macos";
  state?: "supported" | "unsupported_platform" | "blocked";
}

export interface ReadinessReport {
  openInCodex: boolean;
  blockingCheckIds: string[];
  checks: Array<{ id: string; label: string; status: string; blocking: boolean; message: string }>;
  codex?: { authenticated_during_setup: boolean; analysis_status: string; confirmed_field_count: number };
}

export interface OpenInCodexResult {
  opened: boolean;
  message: string;
}

export interface WorkflowHealthResult {
  status: "ready" | "incomplete" | string;
  exit_code?: number | null;
  timed_out: boolean;
  stdout: string;
  stderr: string;
}

export interface GeneratedArtifactPreview {
  component_id: string;
  destination: string;
  content: string;
  expected_sha256: string;
  external?: boolean;
  bytes?: number[];
}

export interface ConflictPreview {
  path: string;
  kind: string;
  base?: string | null;
  local?: string | null;
  incoming?: string | null;
  base_sha256?: string | null;
  local_sha256?: string | null;
  incoming_sha256?: string | null;
  truncated: boolean;
  redacted: boolean;
}

export interface ManifestComponentPreview {
  id: string;
  display_name: string;
  description?: string | null;
  category: string;
  optional: boolean;
  platforms: string[];
  source: { kind: string; path: string };
  destination: { path: string; ownership: string; outside_project?: boolean };
  dependencies: string[];
  required_tools: Array<{ id: string; required: boolean; version_policy?: string | null; version?: string | null; commands: string[]; health_checks: string[] }>;
  environment: Array<{ name: string; secret: boolean; required: boolean; storage?: string | null; non_empty: boolean }>;
  expected_files: Array<{ path: string; sha256?: string | null; size?: number | null }>;
  capabilities: string[];
  validation: Array<{ id: string; severity: string; kind: string; target?: string | null }>;
  update: { strategy: string; remove_obsolete: boolean; preserve_local_additions: boolean };
}

export interface SourceManifestPreview {
  schema_version: string;
  manifest_id: string;
  source: InstallationPlan["source"];
  repository: { provider: string; owner: string; name: string; default_branch: string; web_url?: string | null; license_evidence?: string | null };
  components: ManifestComponentPreview[];
}

export interface FolderSelection {
  path?: string | null;
  error?: string | null;
  cancelled: boolean;
}

export interface InstallationPlanOperation {
  id: string;
  component_id: string;
  action: InstallationAction;
  source_path?: string | null;
  destination: string;
  source_sha256?: string | null;
  source_size?: number | null;
  platform?: "windows" | "macos" | "all" | null;
  result_sha256?: string | null;
  base_sha256?: string | null;
  local_sha256?: string | null;
  local_state: InstallationLocalState;
  resolution?: string | null;
  external?: boolean;
  rollback: InstallationRollbackAction;
}

export interface InstallationPlanConflict {
  id: string;
  path: string;
  options: string[];
  selected?: string | null;
  apply_to_identical?: boolean;
}

export interface InstallationExternalAction {
  id: string;
  component_id: string;
  platform: "windows" | "macos";
  command_source: string;
  executable?: string | null;
  arguments?: string[];
  working_directory?: string | null;
  environment_names?: string[];
  network_access?: string;
  expected_writes?: string[];
  privilege?: string;
  rollback_boundary?: string;
  display_command?: string | null;
  risk: string;
  requires_approval: boolean;
  contains_secret?: boolean;
}

export interface InstallationPlan {
  schema_version: string;
  plan_id: string;
  project_id: string;
  created_at?: string | null;
  maintenance_mode?: "update" | "repair" | "reinstall" | "remove" | null;
  source: {
    repository: string;
    mode: SourceMode;
    resolved_revision: string;
    requested_ref?: string | null;
    release?: string | null;
    manifest_sha256: string;
    manifest_origin?: "remote" | "bundled_revision_bootstrap" | string;
  };
  codex_analysis?: CodexAnalysisRecord | null;
  selected_components: string[];
  wiki_required_pages: string[];
  generated_artifacts?: Array<{ component_id: string; destination: string; content: string; expected_sha256: string; external?: boolean; bytes?: number[] }>;
  git_setup?: { mode: "initialize" | "preserve" | "skip"; branch: string; initial_commit: boolean; remote_name?: string | null; remote_url?: string | null; push_approved: boolean } | null;
  credential_references?: CredentialReference[];
  optional_workflows?: Record<string, string>;
  operations: InstallationPlanOperation[];
  conflicts: InstallationPlanConflict[];
  external_actions?: InstallationExternalAction[];
  transaction: {
    stages: string[];
    backup_root: string;
    staging_root: string;
    atomic_apply_expected?: boolean;
  };
  approvals: {
    dry_run_reviewed: boolean;
    external_actions_reviewed: boolean;
    git_remote_approved: boolean;
    push_approved: boolean;
  };
}

export interface CredentialReference {
  name: string;
  provider: "windows_credential_manager" | "macos_keychain";
  reference: string;
}

export interface TransactionJournal {
  schema_version: string;
  transaction_id: string;
  transaction_kind?: "installation" | "rollback";
  parent_transaction_id?: string | null;
  rollback_transaction_id?: string | null;
  result_lock_sha256?: string | null;
  result_lock_exists?: boolean | null;
  project_id: string;
  project_root?: string;
  state: string;
  created_at: string;
  updated_at: string;
  last_checkpoint: string;
  plan_sha256?: string | null;
  stages: Array<{ id: string; status: string; started_at?: string | null; completed_at?: string | null; evidence?: string[] }>;
  operations: Array<{ id: string; status: string; destination: string; external?: boolean; backup_path?: string | null; rollback_source_path?: string | null; before_sha256?: string | null; expected_sha256?: string | null; after_sha256?: string | null; after_exists?: boolean | null }>;
  recovery: { resume_allowed: boolean; rollback_allowed: boolean; discard_staging_allowed: boolean; project_apply_started: boolean; recommended_action: string };
  error?: { code: string; message: string; stage: string } | null;
}

export interface WizardState {
  screen: ScreenId;
  mode: "new" | "existing";
  recoveryEntry?: boolean;
  identity: ProjectIdentity;
  description: string;
  folderProfile?: string[];
  sourceMode: SourceMode;
  pinnedRef: string;
  selectedComponents: string[];
  components: ComponentRow[];
  meshSelected: boolean;
  meshKeyDraft: string;
  meshKeyStatus: "missing" | "present" | "verified" | "unsupported";
  meshCredentialReference?: CredentialReference;
  loraInterest: boolean;
  gitMode: "initialize" | "preserve" | "skip";
  gitBranch: string;
  initialCommit: boolean;
  gitRemoteName: string;
  gitRemoteUrl: string;
  installProgress: number;
  installStage: string;
  transaction?: TransactionJournal;
  plan?: InstallationPlan;
  maintenanceMode?: "update" | "repair" | "reinstall" | "remove";
  maintenanceCodexAnalysisRecord?: CodexAnalysisRecord;
  maintenanceEvidenceReady?: boolean;
  scanContext?: { scanId: string; projectRoot: string; completedAt?: string | null; partial?: boolean; limitsHit?: string[] };
  conflictChoice?: ConflictChoice;
  recoveryChoice: RecoveryChoice;
  transactionError?: string;
  readiness: ReadinessReport | null;
  codexAccount: CodexAccountStatus | null;
  codexLogin?: CodexLoginStart;
  codexLoginPending?: boolean;
  codexAnalysis?: CodexAnalysis;
  codexAnalysisRecord?: CodexAnalysisRecord;
  manifestPreview?: SourceManifestPreview;
  sourceStatus: string;
  draftSaved: boolean;
}
