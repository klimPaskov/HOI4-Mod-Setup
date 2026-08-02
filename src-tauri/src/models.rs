use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

pub const SUPPORTED_MANIFEST_MAJOR: u64 = 1;

pub const TRANSACTION_STAGES: [&str; 12] = [
    "preflight",
    "repository source resolution",
    "selective download",
    "checksum verification",
    "dry-run review",
    "backup",
    "staging",
    "validation",
    "apply",
    "post-install checks",
    "readiness report",
    "rollback record",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Macos,
    Unsupported,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Unsupported
        }
    }

    pub fn manifest_name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestPlatform {
    Windows,
    Macos,
    All,
}

impl ManifestPlatform {
    pub fn supports(self, platform: Platform) -> bool {
        matches!(
            (self, platform),
            (Self::All, Platform::Windows | Platform::Macos)
                | (Self::Windows, Platform::Windows)
                | (Self::Macos, Platform::Macos)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    Latest,
    PinnedCommit,
    PinnedRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Managed,
    Merged,
    Generated,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentCategory {
    Core,
    Skill,
    Subagent,
    Codex,
    Mcp,
    Script,
    Validator,
    Template,
    Documentation,
    Wiki,
    Workflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    File,
    Tree,
    Generated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Fail,
    Prompt,
    ThreeWayMerge,
    TomlMerge,
    KeepLocal,
    ReplaceIfUnmodified,
    GeneratedReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStrategy {
    ReplaceIfUnmodified,
    ThreeWayMerge,
    TomlMerge,
    Recreate,
    RepositoryScript,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryDescriptor {
    pub provider: String,
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub api_base: Option<String>,
    #[serde(default)]
    pub license_evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDefinition {
    pub kind: SourceKind,
    pub path: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub template_engine: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinationDefinition {
    pub path: String,
    pub ownership: Ownership,
    #[serde(default)]
    pub outside_project: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRequirement {
    pub id: String,
    pub required: bool,
    #[serde(default)]
    pub version_policy: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub health_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentRequirement {
    pub name: String,
    pub secret: bool,
    pub required: bool,
    #[serde(default)]
    pub storage: Option<String>,
    #[serde(default)]
    pub non_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationRule {
    pub id: String,
    pub severity: String,
    pub kind: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFile {
    pub path: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePolicy {
    pub strategy: UpdateStrategy,
    pub remove_obsolete: bool,
    #[serde(default)]
    pub preserve_local_additions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDefinition {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub category: ComponentCategory,
    pub optional: bool,
    pub platforms: Vec<ManifestPlatform>,
    pub source: SourceDefinition,
    pub destination: DestinationDefinition,
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub required_tools: Vec<ToolRequirement>,
    #[serde(default)]
    pub environment: Vec<EnvironmentRequirement>,
    #[serde(default)]
    pub expected_files: Vec<ExpectedFile>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    pub conflict_policy: ConflictPolicy,
    pub validation: Vec<ValidationRule>,
    pub update: UpdatePolicy,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub display_name: String,
    pub components: Vec<String>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiProvenance {
    pub source_status: String,
    pub license_status: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiDefinition {
    pub component_id: String,
    pub destination: String,
    #[serde(default)]
    pub snapshot_marker: Option<String>,
    pub required_pages: Vec<String>,
    #[serde(default = "default_media_policy")]
    pub required_media_policy: String,
    pub provenance: WikiProvenance,
}

fn default_media_policy() -> String {
    "all_declared".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LatestPolicy {
    pub resolve_default_branch: bool,
    pub record_commit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedPolicy {
    pub allow_commit: bool,
    pub allow_release: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestUpdatePolicy {
    pub latest: LatestPolicy,
    pub pinned: PinnedPolicy,
    pub rollback_retention: u32,
    #[serde(default)]
    pub manifest_cache_ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SigningPolicy {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub algorithm: Option<String>,
    #[serde(default)]
    pub public_key_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteManifest {
    pub schema_version: String,
    pub manifest_id: String,
    #[serde(default)]
    pub generated_for_revision: Option<String>,
    pub repository: RepositoryDescriptor,
    pub components: Vec<ComponentDefinition>,
    pub profiles: Vec<Profile>,
    pub wiki: WikiDefinition,
    pub update_policy: ManifestUpdatePolicy,
    #[serde(default)]
    pub signing: Option<SigningPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub repository: String,
    pub mode: SourceMode,
    pub resolved_revision: String,
    #[serde(default)]
    pub requested_ref: Option<String>,
    #[serde(default)]
    pub release: Option<String>,
    pub manifest_sha256: String,
    #[serde(default = "default_manifest_origin")]
    pub manifest_origin: String,
}

/// Wiki provenance copied from the exact resolved manifest. Readiness and
/// maintenance must not substitute values from a newer bundled manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiInstallMetadata {
    #[serde(default)]
    pub snapshot_marker: Option<String>,
    pub required_media_policy: String,
    pub source_status: String,
    pub license_status: String,
    /// Repository license evidence copied from the exact manifest revision.
    /// `unknown` is used for legacy locks that predate this field.
    #[serde(default = "default_unknown_status")]
    pub repository_license_status: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

pub(crate) fn default_unknown_status() -> String {
    "unknown".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockSourceIdentity {
    pub repository: String,
    pub mode: SourceMode,
    pub revision: String,
    #[serde(default)]
    pub requested_ref: Option<String>,
    #[serde(default)]
    pub release: Option<String>,
    pub manifest_sha256: String,
    #[serde(default = "default_manifest_origin")]
    pub manifest_origin: String,
}

fn default_manifest_origin() -> String {
    "remote".into()
}

pub(crate) fn default_ai_provider() -> String {
    "codex".into()
}

pub(crate) fn default_ai_model() -> String {
    "default".into()
}

pub(crate) fn default_ai_optimization_profile() -> String {
    "Codex project and ChatGPT Chat".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    pub kind: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedFile {
    /// Core-owned operation binding added after manifest selection. The source
    /// adapter leaves it empty until a reviewed plan operation is created.
    #[serde(default)]
    pub operation_id: String,
    pub source_path: String,
    pub destination: String,
    pub source_revision: String,
    /// SHA-256 of the exact remote manifest that authorized this file.
    #[serde(default)]
    pub manifest_sha256: String,
    pub sha256: String,
    pub size: u64,
    pub component_id: String,
    pub ownership: Ownership,
    pub platform: ManifestPlatform,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanEvidence {
    pub detector: String,
    pub path: String,
    #[serde(default)]
    pub line_start: Option<u32>,
    #[serde(default)]
    pub line_end: Option<u32>,
    #[serde(default)]
    pub excerpt_sha256: Option<String>,
    pub confidence: f32,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanFinding {
    pub id: String,
    pub category: String,
    pub key: String,
    pub value: Value,
    pub status: String,
    #[serde(default = "default_scan_origin")]
    pub origin: String,
    #[serde(default)]
    pub user_value: Option<Value>,
    pub evidence: Vec<ScanEvidence>,
    #[serde(default)]
    pub recommendation: Option<String>,
}

fn default_scan_origin() -> String {
    "deterministic".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConflict {
    pub id: String,
    pub path: String,
    pub kind: String,
    pub severity: String,
    #[serde(default)]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanSummary {
    pub accepted: u32,
    pub needs_review: u32,
    pub blocking: u32,
    pub warnings: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub schema_version: String,
    pub scan_id: Uuid,
    pub project_root: String,
    pub mode: String,
    #[serde(default)]
    pub platform: Option<Platform>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub read_only: bool,
    #[serde(default)]
    pub scanner_version: Option<String>,
    #[serde(default)]
    pub partial: bool,
    #[serde(default)]
    pub cancelled: bool,
    pub semantic_analysis: SemanticAnalysisSummary,
    #[serde(default)]
    pub limits_hit: Vec<String>,
    #[serde(default)]
    pub files_scanned: u32,
    #[serde(default)]
    pub directories_scanned: u32,
    #[serde(default)]
    pub bytes_read: u64,
    pub findings: Vec<ScanFinding>,
    #[serde(default)]
    pub conflicts: Vec<ScanConflict>,
    pub summary: ScanSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAnalysisSummary {
    pub requested: bool,
    pub required: bool,
    pub status: String,
    pub engine: String,
    pub auth_mode: String,
    pub transport: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub input_manifest: Vec<Value>,
    pub output_schema_id: String,
    #[serde(default)]
    pub analysis_id: Option<Uuid>,
    #[serde(default)]
    pub response_sha256: Option<String>,
    #[serde(default)]
    pub suggestions: Vec<ScanFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationAction {
    Create,
    Replace,
    Merge,
    Rename,
    Skip,
    DeleteManaged,
    Generate,
    Chmod,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalState {
    Absent,
    Unmodified,
    Modified,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackAction {
    RemoveCreated,
    RestoreBackup,
    ReverseMerge,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanOperation {
    pub id: String,
    pub component_id: String,
    /// Immutable ownership evidence carried from the manifest, generated
    /// artifact, or predecessor lock. Maintenance must not infer ownership
    /// from the action chosen for this particular transaction.
    #[serde(default)]
    pub ownership: Option<Ownership>,
    #[serde(default)]
    pub location_scope: Option<String>,
    pub action: OperationAction,
    #[serde(default)]
    pub source_path: Option<String>,
    pub destination: String,
    #[serde(default)]
    pub source_sha256: Option<String>,
    /// Size of the bytes authenticated by `source_sha256` at the resolved
    /// source revision. Generated operations use the reviewed output size.
    #[serde(default)]
    pub source_size: Option<u64>,
    /// Platform evidence carried from the remote manifest into the plan.
    #[serde(default)]
    pub platform: Option<ManifestPlatform>,
    /// Expected executable state declared by the verified source or inherited
    /// from the predecessor lock. On Unix this is applied and verified as
    /// filesystem mode evidence; Windows records the declaration only.
    #[serde(default)]
    pub executable: bool,
    #[serde(default)]
    pub result_sha256: Option<String>,
    #[serde(default)]
    pub base_sha256: Option<String>,
    #[serde(default)]
    pub local_sha256: Option<String>,
    pub local_state: LocalState,
    #[serde(default)]
    pub resolution: Option<String>,
    /// When true, `destination` is an explicitly user-approved external path
    /// such as a HOI4 launcher descriptor. It is never inferred from a source
    /// path and is validated separately from the project root.
    #[serde(default)]
    pub external: bool,
    pub rollback: RollbackAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanConflict {
    pub id: String,
    pub path: String,
    pub options: Vec<String>,
    pub selected: Option<String>,
    #[serde(default)]
    pub apply_to_identical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAction {
    pub id: String,
    pub component_id: String,
    pub platform: Platform,
    pub command_source: String,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub environment_names: Vec<String>,
    #[serde(default)]
    pub network_access: String,
    #[serde(default)]
    pub expected_writes: Vec<String>,
    #[serde(default)]
    pub privilege: String,
    #[serde(default)]
    pub rollback_boundary: String,
    #[serde(default)]
    pub display_command: Option<String>,
    pub risk: String,
    pub requires_approval: bool,
    #[serde(default)]
    pub contains_secret: bool,
    /// Immutable identity evidence for a command that the core may execute.
    /// Missing evidence keeps the action reviewable but unavailable.
    #[serde(default)]
    pub verified_executable_sha256: Option<String>,
    #[serde(default)]
    pub verified_executable_size: Option<u64>,
    /// Immutable identity evidence for the reviewed command interpreter used
    /// by a wrapper route. Missing evidence keeps the route unavailable.
    #[serde(default)]
    pub verified_interpreter_sha256: Option<String>,
    #[serde(default)]
    pub verified_interpreter_size: Option<u64>,
    /// Immutable identity evidence for a runtime dependency resolved by the
    /// reviewed wrapper, such as Node for an MCP command.
    #[serde(default)]
    pub verified_runtime_sha256: Option<String>,
    #[serde(default)]
    pub verified_runtime_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionPlanInfo {
    pub stages: Vec<String>,
    pub backup_root: String,
    pub staging_root: String,
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default)]
    pub atomic_apply_expected: bool,
    #[serde(default)]
    pub project_root_mode: ProjectRootMode,
    #[serde(default)]
    pub project_root_parent: Option<String>,
    #[serde(default)]
    pub project_root_leaf: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRootMode {
    #[default]
    Existing,
    CreateLeaf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanApprovals {
    pub dry_run_reviewed: bool,
    pub external_actions_reviewed: bool,
    pub git_remote_approved: bool,
    pub push_approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationPlan {
    pub schema_version: String,
    pub plan_id: Uuid,
    pub project_id: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub maintenance_mode: Option<String>,
    pub source: SourceIdentity,
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    #[serde(default)]
    pub ai_endpoint: Option<String>,
    #[serde(default = "default_ai_optimization_profile")]
    pub ai_optimization_profile: String,
    #[serde(default)]
    pub flatten_chat_sources: bool,
    #[serde(default)]
    pub codex_analysis: Option<CodexAnalysisRecord>,
    pub selected_components: Vec<String>,
    /// Required wiki pages copied from the exact manifest used for this plan.
    /// Keeping this beside the source identity prevents readiness from using
    /// a newer bundled manifest when a pinned install uses an older revision.
    #[serde(default)]
    pub wiki_required_pages: Vec<String>,
    /// Provenance and media policy from the exact manifest used for this plan.
    /// `None` is retained for legacy plans and is treated as incomplete when
    /// the offline wiki is selected.
    #[serde(default)]
    pub wiki_metadata: Option<WikiInstallMetadata>,
    #[serde(default)]
    pub generated_artifacts: Vec<GeneratedArtifact>,
    /// Revision-bound source verification records created by the source
    /// adapter. Transactions require one exact record for every remote
    /// operation whose incoming bytes are staged.
    #[serde(default)]
    pub download_ledger: Vec<DownloadedFile>,
    #[serde(default)]
    pub git_setup: Option<crate::git::GitSetup>,
    #[serde(default)]
    pub credential_references: Vec<CredentialReference>,
    #[serde(default)]
    pub optional_workflows: BTreeMap<String, String>,
    pub operations: Vec<PlanOperation>,
    pub conflicts: Vec<PlanConflict>,
    #[serde(default)]
    pub external_actions: Vec<ExternalAction>,
    pub transaction: TransactionPlanInfo,
    pub approvals: PlanApprovals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedFile {
    pub path: String,
    #[serde(default)]
    pub location_scope: Option<String>,
    pub component_id: String,
    pub source_path: String,
    pub source_revision: String,
    pub source_sha256: String,
    #[serde(default)]
    pub source_size: Option<u64>,
    #[serde(default)]
    pub base_sha256: Option<String>,
    pub installed_sha256: String,
    #[serde(default)]
    pub installed_size: Option<u64>,
    pub ownership: Ownership,
    /// True when a reviewed first install kept pre-existing local bytes as
    /// the accepted baseline. Readiness hashes those bytes normally, while
    /// maintenance never treats them as removable managed output.
    #[serde(default)]
    pub preserved_local: bool,
    #[serde(default)]
    pub external: bool,
    #[serde(default)]
    pub generated_content: Option<String>,
    #[serde(default)]
    pub generated_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub executable: bool,
    #[serde(default)]
    pub platform: Option<ManifestPlatform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockComponent {
    pub id: String,
    pub version: Option<String>,
    pub state: String,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub validation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeChoice {
    pub path: String,
    pub choice: String,
    #[serde(default)]
    pub result_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalWorkflowLock {
    pub state: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub credential_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModification {
    pub path: String,
    pub installed_sha256: String,
    pub current_sha256: String,
    pub detected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationLock {
    pub schema_version: String,
    pub project_id: String,
    pub installed_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub source: LockSourceIdentity,
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    #[serde(default)]
    pub ai_endpoint: Option<String>,
    #[serde(default = "default_ai_optimization_profile")]
    pub ai_optimization_profile: String,
    #[serde(default)]
    pub flatten_chat_sources: bool,
    #[serde(default)]
    pub codex_analysis: Option<CodexAnalysisRecord>,
    /// Required wiki pages copied from the exact manifest used for this lock.
    /// An empty value on a legacy lock is treated as incomplete by readiness.
    #[serde(default)]
    pub wiki_required_pages: Vec<String>,
    /// Provenance and media policy from the exact manifest used for this lock.
    /// Legacy locks may omit it and remain readable but not fully ready.
    #[serde(default)]
    pub wiki_metadata: Option<WikiInstallMetadata>,
    pub components: Vec<LockComponent>,
    pub files: Vec<LockedFile>,
    pub merge_choices: Vec<MergeChoice>,
    pub optional_workflows: BTreeMap<String, OptionalWorkflowLock>,
    pub local_modifications: Vec<LocalModification>,
    #[serde(default)]
    pub rollback_records: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageCheckpoint {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalOperation {
    pub id: String,
    /// `rollback_applying` is a durable per-operation checkpoint. A retry
    /// verifies the restored state before doing any further destructive work.
    pub status: String,
    pub destination: String,
    /// The ownership captured in the approved plan. Keeping it in the
    /// journal prevents rollback/removal from treating a recreated merged or
    /// generated file as managed merely because its action was `replace`.
    #[serde(default)]
    pub ownership: Option<Ownership>,
    #[serde(default)]
    pub component_id: Option<String>,
    /// Manifest or generated source identity copied from the approved plan.
    /// Rollback journals may carry the predecessor operation's source for an
    /// auditable link back to the installation record.
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_size: Option<u64>,
    #[serde(default)]
    pub action: Option<OperationAction>,
    #[serde(default)]
    pub location_scope: Option<String>,
    #[serde(default)]
    pub external: bool,
    #[serde(default)]
    pub backup_path: Option<String>,
    #[serde(default)]
    pub before_sha256: Option<String>,
    #[serde(default)]
    pub before_executable: Option<bool>,
    #[serde(default)]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub source_sha256: Option<String>,
    #[serde(default)]
    pub result_sha256: Option<String>,
    #[serde(default)]
    pub expected_executable: Option<bool>,
    #[serde(default)]
    pub rollback: Option<RollbackAction>,
    /// For a rollback transaction, the verified backup from the parent
    /// transaction that supplies the restored bytes. The rollback
    /// transaction's own `backup_path` remains the bytes needed to undo the
    /// rollback itself.
    #[serde(default)]
    pub rollback_source_path: Option<String>,
    /// The explicit conflict/removal choice that made the operation safe to
    /// apply. It is kept separate from action because `skip` can mean keep,
    /// review, or an intentional managed removal decision.
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub backup_sha256: Option<String>,
    /// Hash of the staged bytes before live apply. This is intentionally
    /// separate from `after_sha256`, which is reserved for observed live
    /// post-apply bytes.
    #[serde(default)]
    pub staged_sha256: Option<String>,
    #[serde(default)]
    pub after_sha256: Option<String>,
    #[serde(default)]
    pub after_exists: Option<bool>,
    #[serde(default)]
    pub after_executable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub resume_allowed: bool,
    pub rollback_allowed: bool,
    pub discard_staging_allowed: bool,
    pub project_apply_started: bool,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRootLifecycle {
    #[serde(default)]
    pub mode: ProjectRootMode,
    #[serde(default)]
    pub canonical_parent: Option<String>,
    #[serde(default)]
    pub leaf: Option<String>,
    #[serde(default = "default_root_checkpoint")]
    pub checkpoint: String,
    #[serde(default)]
    pub created_by_transaction: bool,
    #[serde(default)]
    pub observed_exists: bool,
    #[serde(default)]
    pub cleanup_result: Option<String>,
}

impl Default for ProjectRootLifecycle {
    fn default() -> Self {
        Self {
            mode: ProjectRootMode::Existing,
            canonical_parent: None,
            leaf: None,
            checkpoint: default_root_checkpoint(),
            created_by_transaction: false,
            observed_exists: true,
            cleanup_result: None,
        }
    }
}

fn default_root_checkpoint() -> String {
    "not_required".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalError {
    pub code: String,
    pub message: String,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionJournal {
    pub schema_version: String,
    pub transaction_id: Uuid,
    #[serde(default = "default_transaction_kind")]
    pub transaction_kind: String,
    #[serde(default)]
    pub parent_transaction_id: Option<Uuid>,
    #[serde(default)]
    pub rollback_transaction_id: Option<Uuid>,
    #[serde(default)]
    pub result_lock_sha256: Option<String>,
    #[serde(default)]
    pub result_lock_exists: Option<bool>,
    #[serde(default)]
    pub rollback_record_sha256: Option<String>,
    pub project_id: String,
    /// Canonical project root bound to this journal. Older journals may omit
    /// it, but recovery must refuse to resume them without a root binding.
    #[serde(default)]
    pub project_root: String,
    #[serde(default)]
    pub project_root_lifecycle: ProjectRootLifecycle,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_checkpoint: String,
    #[serde(default)]
    pub plan_sha256: Option<String>,
    pub stages: Vec<StageCheckpoint>,
    pub operations: Vec<JournalOperation>,
    #[serde(default)]
    pub created_directories: Vec<String>,
    pub recovery: RecoveryState,
    #[serde(default)]
    pub git_initialized: bool,
    #[serde(default)]
    pub git_remote_added_name: Option<String>,
    #[serde(default)]
    pub git_remote_added_url: Option<String>,
    /// A verified copy of the lock that existed before this transaction.
    /// Keeping this outside the project makes rollback able to restore a
    /// predecessor lock without guessing whether the current lock is ours.
    #[serde(default)]
    pub previous_lock_backup_path: Option<String>,
    #[serde(default)]
    pub previous_lock_sha256: Option<String>,
    #[serde(default)]
    pub error: Option<JournalError>,
}

fn default_transaction_kind() -> String {
    "installation".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessEvidence {
    pub kind: String,
    pub value: Value,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessCheck {
    pub id: String,
    pub category: String,
    pub label: String,
    pub status: String,
    pub blocking: bool,
    #[serde(default)]
    pub message: Option<String>,
    pub evidence: Vec<ReadinessEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReadinessSummary {
    pub pass: u32,
    pub warn: u32,
    pub block: u32,
    pub not_selected: u32,
    pub planned_unavailable: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenInCodex {
    pub enabled: bool,
    pub blocking_check_ids: Vec<String>,
    #[serde(default)]
    pub command_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenInCodexResult {
    pub opened: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub schema_version: String,
    pub report_id: Uuid,
    pub project_id: String,
    pub generated_at: String,
    pub codex: ReadinessCodexSummary,
    pub checks: Vec<ReadinessCheck>,
    pub summary: ReadinessSummary,
    #[serde(default)]
    pub core_ready: bool,
    pub open_in_codex: OpenInCodex,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessCodexSummary {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    pub integration: String,
    pub auth_mode: String,
    pub authenticated_during_setup: bool,
    pub analysis_status: String,
    pub confirmed_field_count: u32,
    pub no_account_metadata_persisted: bool,
    pub blocking_check_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialReference {
    pub name: String,
    pub provider: String,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderProfile {
    pub id: String,
    pub display_name: String,
    pub protocol: String,
    pub requires_credential: bool,
    pub optimization_profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub display_name: String,
    pub project_id: String,
    pub author: String,
    pub version: String,
    pub supported_game_version: String,
    pub project_root: PathBuf,
    pub default_branch: String,
    #[serde(default)]
    pub script_prefix: Option<String>,
    #[serde(default)]
    pub primary_namespace: Option<String>,
    #[serde(default)]
    pub descriptor_tags: Vec<String>,
    #[serde(default)]
    pub launcher_descriptor_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedFile {
    pub operation_id: String,
    pub destination: String,
    pub bytes: Vec<u8>,
    pub expected_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedArtifact {
    pub component_id: String,
    pub destination: String,
    pub content: String,
    pub expected_sha256: String,
    #[serde(default)]
    pub external: bool,
    /// Binary generated content is kept separate from the human-readable
    /// preview. It is never rendered as text and is hash-checked before apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexAnalysisRecord {
    pub engine: String,
    #[serde(default)]
    pub auth_mode: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub optimization_profile: Option<String>,
    pub analysis_id: Uuid,
    pub schema_version: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub confirmed_fields: Vec<String>,
    pub confirmed_at: String,
    #[serde(default)]
    pub account_identity_persisted: bool,
    /// Optional core-owned purpose. The project-root and scan binding remain
    /// in the in-memory pending-analysis session and are deliberately not
    /// serialized into the project, plan, lock, or renderer payload.
    #[serde(default)]
    pub analysis_purpose: Option<String>,
    #[serde(skip)]
    pub project_root: Option<String>,
    #[serde(skip)]
    pub scan_id: Option<Uuid>,
    #[serde(default)]
    pub evidence_sha256: Option<String>,
}
