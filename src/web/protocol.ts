// Messages between the web edition's page and its core worker.

export type Platform = "windows" | "macos" | "linux";

export interface DetectedValue {
  key: string;
  value: unknown;
  reason: string;
}

export interface ScanFinding {
  id: string;
  category?: string;
  key?: string;
  value: unknown;
  status: string;
  confidence?: number;
  path?: string | null;
  note?: string | null;
}

/** An existing setup read from the folder's installation lock. */
export interface ExistingSetup {
  valid: boolean;
  reason?: string;
  project_id?: string;
  script_prefix?: string | null;
  primary_namespace?: string | null;
  installed_at?: string;
  updated_at?: string | null;
  source?: { mode: string; revision: string; requested_ref: string | null };
  ai_provider?: string;
  ai_model?: string;
  primary_coding_environment?: string;
  additional_coding_environments?: string[];
  components?: string[];
  portrait_pipeline?: { enabled: boolean; provider: string; preferred_workflow?: string; local_comfyui_root?: string; local_server_url?: string; runpod_url?: string; runpod_workspace?: string } | null;
  image_generation?: ImageChoice | null;
  live_testing?: LiveTestingChoice | null;
  file_count?: number;
  /** Why only the desktop app can update, repair, or change this setup. */
  desktop_only?: string | null;
}

export interface ScanSummary {
  findings: ScanFinding[];
  detected: DetectedValue[];
  descriptor: Record<string, string> | null;
  stats: { files_indexed: number; directories_indexed: number; samples_read: number; bytes_read: number; excluded_entries: number; coverage: string } | null;
  issues: string[];
  existing_setup?: ExistingSetup | null;
}

export interface ManifestComponent {
  id: string;
  display_name: string;
  description: string | null;
  category: string;
  optional: boolean;
  coding_environment: string | null;
  platforms: string[];
  dependencies: string[];
  file_count: number;
  total_size: number;
}

export interface CodingEnvironment {
  id: string;
  display_name: string;
  description: string | null;
  default_primary: boolean;
  mcp_support?: "project" | "manual" | "none";
  documentation_url?: string;
}

export interface ImageProvider {
  id: string;
  display_name: string;
  description?: string | null;
  availability?: string | null;
  requirements?: string[] | null;
  good_for?: string[] | string | null;
  cost_summary?: string | null;
  documentation_url?: string | null;
  supported_coding_environments?: string[] | null;
  route_kind?: string | null;
  auth_method?: string | null;
  api_protocol?: string | null;
  supported_models?: string[] | null;
  requires_primary_coding_environment?: string | null;
}

export interface LiveTestingProvider {
  id: string;
  display_name: string;
  description?: string;
  requirements?: string[];
  documentation_url?: string;
  supported_platforms?: string[];
  supported_coding_environments?: string[];
  experimental?: boolean;
}

export interface SourcePreview {
  schema_version: string;
  revision: string;
  components: ManifestComponent[];
  profiles: Array<{ id: string; display_name: string; default: boolean; components: string[] }>;
  coding_environments: CodingEnvironment[];
  unavailable_coding_environments: Array<{ id: string; display_name: string; reason: string }>;
  image_generation?: { default_provider: string | null; providers: ImageProvider[] } | null;
  live_testing?: { default_provider?: string; providers: LiveTestingProvider[] } | null;
}

/** Image providers; keys never leave the operating-system credential store. */
export interface ImageChoice {
  primary_provider: string;
  additional_providers: string[];
  route_settings: Record<string, Record<string, string>>;
}

export interface LiveTestingChoice {
  provider: "none" | "clausewitz_mcp" | "hoi4_live_tools";
  linux_ready: boolean;
  wsl_distribution: string;
  ssh_host: string;
}

export interface PortraitChoice {
  provider: "disabled" | "cloud" | "local" | "runpod";
  preferredWorkflow?: string;
  localComfyuiRoot?: string;
  localServerUrl?: string;
  runpodUrl?: string;
  runpodWorkspace?: string;
}

export interface SelectionRequest {
  selectedComponents: string[];
  primaryCodingEnvironment: string;
  additionalCodingEnvironments: string[];
  platform: Platform;
  imageGeneration?: ImageChoice;
  liveTesting?: LiveTestingChoice;
  portrait?: PortraitChoice;
}

export interface SelectionResult {
  components: string[];
  unsupported: Array<{ component_id: string; state: string; reason: string | null }>;
  files: Array<{ component_id: string; source_path: string; destination: string; expected_sha256: string | null; expected_size: number | null }>;
}

export type MaintenanceMode = "update" | "repair" | "change";

export interface ExportRequest {
  /** Absent for maintenance, which keeps the lock's identity. */
  identity?: {
    displayName: string;
    projectId: string;
    author?: string;
    version?: string;
    supportedGameVersion?: string;
    scriptPrefix?: string | null;
    primaryNamespace?: string | null;
    descriptorTags: string[];
  };
  maintenance?: MaintenanceMode;
  description?: string;
  conventions?: { agentsProfile?: string; localisationConvention?: string; documentationConvention?: string };
  layout?: "overlay" | "full_copy";
  installedAt: string;
  thumbnailPng?: string | null;
  assistant?: AssistantProvenance | null;
  contextDocuments?: Array<{ name: string; data: string }>;
  contextImages?: Array<{ name: string; data: string }>;
}

/** Non-secret record of the browser assistant whose suggestions were used. */
export interface AssistantProvenance {
  provider: string;
  model: string;
  reasoningEffort: string;
  endpoint: string;
  optimizationProfile: string;
}

export interface ApprovedEvidence {
  reference: string;
  path: string;
  excerpt: string;
  excerpt_sha256: string;
  confidence: number | null;
}

/** A document the user added, as the core excerpted it. */
export interface ContextDocument {
  id: string;
  name: string;
  size: number;
  reference: string;
  evidence_path: string;
  excerpt: string;
  excerpt_sha256: string;
  truncated: boolean;
  redacted: boolean;
  copyable: boolean;
}

export interface AnalysisRequest {
  mode: "new_project_identity" | "existing_project_semantics";
  brief: string;
  evidence: ApprovedEvidence[];
  constraints: Record<string, unknown>;
}

export interface AssistantRoute {
  provider: string;
  protocol: "anthropic_messages" | "openai_compatible" | "openai_responses";
  endpoint: string;
  model: string;
  reasoningEffort: string;
  supportsEffort: boolean;
  optimizationProfile: string;
}

export interface AssistantTurn {
  inputSha256: string;
  headers: Array<[string, string]>;
  body: unknown;
}

export interface AnalysisProposal {
  key: string;
  value: unknown;
  confidence: number;
  reason: string;
  evidence_refs: string[];
}

export interface Analysis {
  analysis_id: string;
  project_summary: string;
  proposals: AnalysisProposal[];
  component_recommendations: Array<{ component_id: string; recommendation: "required" | "recommended" | "not_recommended"; reason: string }>;
  warnings: string[];
}

export type AssistantReply = { analysis: Analysis } | { retry: string } | { error: string };

export type ExportAction = "create" | "unchanged" | "conflict" | "update" | "restore" | "remove" | "release" | "modified";

export interface ExportEntry {
  path: string;
  component_id: string;
  action: ExportAction;
  incoming_sha256: string;
  local_sha256: string | null;
  size: number;
  mergeable?: boolean;
}

export interface Comparison {
  path: string;
  incoming: string | null;
  local: string | null;
  base?: string | null;
  merged?: string | null;
  binary: boolean;
}

export type Decision = "keep" | "replace" | "merge" | "rename" | "skip";

export interface ReadinessCheck {
  id: string;
  label: string;
  state: "pass" | "action" | "info";
  detail: string;
}

export interface CoreOperations {
  reset: { args: Record<string, never>; result: null };
  addFiles: { args: { files: Array<{ path: string; buffer: ArrayBuffer }> }; result: number };
  scan: { args: Record<string, never>; result: ScanSummary };
  openSource: { args: { manifest: ArrayBuffer; revision: string; mode: string; requestedRef: string | null }; result: null };
  preview: { args: { platform: Platform }; result: SourcePreview };
  select: { args: { request: SelectionRequest }; result: SelectionResult };
  download: { args: { revision: string; expected: Record<string, string> }; result: number };
  export: { args: { request: ExportRequest; existing: boolean }; result: ExportEntry[] };
  exportRemoval: { args: { layout: "overlay" | "full_copy" }; result: ExportEntry[] };
  compare: { args: { path: string }; result: Comparison };
  fetchBases: { args: Record<string, never>; result: { loaded: string[]; failed: Array<{ path: string; reason: string }> } };
  merge: { args: { path: string }; result: string };
  readiness: { args: Record<string, never>; result: ReadinessCheck[] };
  launcher: { args: { folder: string }; result: string };
  zip: { args: { decisions: Record<string, Decision>; layout?: "overlay" | "full_copy" }; result: ArrayBuffer };
  evidence: { args: { excluded?: string[] }; result: ApprovedEvidence[] };
  registry: { args: Record<string, never>; result: Record<string, unknown> };
  contextDocument: { args: { number: number; name: string; buffer: ArrayBuffer }; result: ContextDocument };
  assistantTurn: { args: { request: AnalysisRequest; route: AssistantRoute; correction: string | null }; result: AssistantTurn };
  assistantReply: { args: { envelope: string; request: AnalysisRequest; inputSha256: string; analysisId: string }; result: AssistantReply };
  descriptionTurn: { args: { route: AssistantRoute; note: string }; result: { headers: Array<[string, string]>; body: unknown } };
  descriptionReply: { args: { envelope: string }; result: { description: string } | { error: string } };
  openRouterStart: { args: { callback: string }; result: { url: string; verifier: string; state: string } };
  openRouterKeyRequest: { args: { code: string; verifier: string }; result: string };
  openRouterKey: { args: { response: string }; result: string };
}

export type Operation = keyof CoreOperations;

export interface Request<Op extends Operation = Operation> {
  id: number;
  op: Op;
  args: CoreOperations[Op]["args"];
}

export type WorkerMessage =
  | { kind: "result"; id: number; ok: true; result: unknown }
  | { kind: "result"; id: number; ok: false; error: string }
  | { kind: "progress"; done: number; total: number; path: string }
  | { kind: "network"; url: string; status: number | "failed"; at: string };
