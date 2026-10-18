// Messages between the web edition's page and its core worker.

export type Platform = "windows" | "macos" | "linux";

export interface DetectedValue {
  key: string;
  value: unknown;
  reason: string;
}

export interface ScanSummary {
  findings: Array<{ id: string; value: unknown; status: string }>;
  detected: DetectedValue[];
  descriptor: Record<string, string> | null;
  stats: { files_indexed: number; directories_indexed: number; samples_read: number; bytes_read: number; excluded_entries: number; coverage: string } | null;
  issues: string[];
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

export interface SourcePreview {
  schema_version: string;
  revision: string;
  components: ManifestComponent[];
  profiles: Array<{ id: string; display_name: string; default: boolean; components: string[] }>;
  coding_environments: CodingEnvironment[];
  unavailable_coding_environments: Array<{ id: string; display_name: string; reason: string }>;
}

export interface SelectionRequest {
  selectedComponents: string[];
  primaryCodingEnvironment: string;
  additionalCodingEnvironments: string[];
  platform: Platform;
}

export interface SelectionResult {
  components: string[];
  unsupported: Array<{ component_id: string; state: string; reason: string | null }>;
  files: Array<{ component_id: string; source_path: string; destination: string; expected_sha256: string | null; expected_size: number | null }>;
}

export interface ExportRequest {
  identity: {
    displayName: string;
    projectId: string;
    author?: string;
    version?: string;
    supportedGameVersion?: string;
    scriptPrefix?: string | null;
    primaryNamespace?: string | null;
    descriptorTags: string[];
  };
  description: string;
  conventions: { agentsProfile?: string; localisationConvention?: string; documentationConvention?: string };
  layout?: "overlay" | "full_copy";
  installedAt: string;
  thumbnailPng?: string | null;
  assistant?: AssistantProvenance | null;
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

export interface ExportEntry {
  path: string;
  component_id: string;
  action: "create" | "unchanged" | "conflict";
  incoming_sha256: string;
  local_sha256: string | null;
  size: number;
}

export interface Comparison {
  path: string;
  incoming: string | null;
  local: string | null;
  binary: boolean;
}

export type Decision = "keep" | "replace";

export interface CoreOperations {
  reset: { args: Record<string, never>; result: null };
  addFiles: { args: { files: Array<{ path: string; buffer: ArrayBuffer }> }; result: number };
  scan: { args: Record<string, never>; result: ScanSummary };
  openSource: { args: { manifest: ArrayBuffer; revision: string; mode: string; requestedRef: string | null }; result: null };
  preview: { args: { platform: Platform }; result: SourcePreview };
  select: { args: { request: SelectionRequest }; result: SelectionResult };
  download: { args: { revision: string }; result: number };
  export: { args: { request: ExportRequest; existing: boolean }; result: ExportEntry[] };
  compare: { args: { path: string }; result: Comparison };
  zip: { args: { decisions: Record<string, Decision> }; result: ArrayBuffer };
  evidence: { args: Record<string, never>; result: ApprovedEvidence[] };
  registry: { args: Record<string, never>; result: Record<string, unknown> };
  assistantTurn: { args: { request: AnalysisRequest; route: AssistantRoute; correction: string | null }; result: AssistantTurn };
  assistantReply: { args: { envelope: string; request: AnalysisRequest; inputSha256: string; analysisId: string }; result: AssistantReply };
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
