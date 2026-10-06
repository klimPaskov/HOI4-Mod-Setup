use crate::models::*;
use crate::paths::{
    application_data_root, validate_project_root, validate_project_root_or_destination,
};
use crate::safe_fs::RootedDir;
use crate::security::{
    canonical_relative_key, is_link_metadata, normalize_relative_path, path_has_link_component,
    persistable_json_bytes, redact_secrets, safe_join, sha256_bytes, sha256_file,
    validate_external_destination,
};
use crate::AppError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
#[cfg(test)]
use std::fs::OpenOptions;
#[cfg(test)]
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

const OPERATION_INTENT_BATCH: usize = 64;
const OPERATION_CHECKPOINT_BATCH: usize = 1_024;
const OPERATION_CHECKPOINT_MAX_RECORDS: usize = 2_048;
const OPERATION_CHECKPOINT_MAX_BYTES: usize = 32 * 1024 * 1024;
const JOURNAL_ERROR_MESSAGE_MAX_BYTES: usize = 2 * 1024;
const STAGE_EVIDENCE_MAX_CHARS: usize = 1_024;
/// Operation checkpoint record format. `1.1.0` adds the optional quarantine
/// fields on the embedded operation and the monotonic `sequence`; replay still
/// accepts `1.0.0` records written before them.
const OPERATION_CHECKPOINT_SCHEMA: &str = "1.1.0";
const LEGACY_OPERATION_CHECKPOINT_SCHEMA: &str = "1.0.0";
/// Bounds for the recovery sweep of leftover destination quarantines.
const QUARANTINE_SWEEP_MAX_DIRECTORIES: usize = 4_096;
const QUARANTINE_SWEEP_MAX_ENTRIES: usize = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OperationCheckpoint {
    schema_version: String,
    transaction_id: Uuid,
    operation_index: usize,
    operation: JournalOperation,
    journal_state: String,
    last_checkpoint: String,
    recovery: RecoveryState,
    updated_at: String,
    /// Monotonic per-journal position. Replay orders records by this value,
    /// never by wall-clock time; records written before it existed omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sequence: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct TransactionOptions {
    pub app_data_root: Option<PathBuf>,
    /// Only the recovery path may set this. It lets a verified replay reuse
    /// its own non-terminal journal without weakening the new-mutation gate.
    pub resume_transaction_id: Option<Uuid>,
    pub fail_before_stage: Option<usize>,
    pub fail_after_stage: Option<usize>,
    pub fail_before_operation: Option<usize>,
    /// Inject failure after a live destination changes but before the
    /// operation result is observed and journaled.
    pub fail_after_live_mutation: Option<usize>,
    pub fail_after_operation: Option<usize>,
    /// Inject failure at one same-directory quarantine boundary of the
    /// operation with this index. Placement itself is covered by
    /// `fail_after_live_mutation`.
    pub fail_at_quarantine: Option<(usize, QuarantineBoundary)>,
    /// Inject failure at one quarantine boundary of the success-lock commit.
    pub fail_at_lock_quarantine: Option<QuarantineBoundary>,
    /// Test barrier that models a concurrent local change inside the window
    /// between the precondition check and the live namespace change.
    pub live_mutation_barrier: Option<LiveMutationHook>,
    pub fail_before_git: bool,
    pub fail_after_git: bool,
    /// Deterministic fault boundaries around reviewed external actions. The
    /// after-action hook models a child that returned after external effects
    /// but before the completion checkpoint was persisted.
    pub fail_before_post_install_action: bool,
    pub fail_after_post_install_action: bool,
    pub fail_before_post_install_action_index: Option<usize>,
    pub fail_after_post_install_action_index: Option<usize>,
    /// Runs only manifest-declared, user-reviewed post-install actions after
    /// managed-file verification. Production supplies the allowlisted runner;
    /// tests inject a deterministic fake.
    pub post_install_action_runner: Option<PostInstallActionRunner>,
}

/// A deterministic crash boundary around the quarantine of an existing live
/// destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineBoundary {
    /// The quarantine intent is durable and the destination has not moved.
    BeforeRename,
    /// The destination moved to its quarantine name and is not yet verified.
    AfterRename,
    /// The quarantined bytes differed from the precondition, that observed
    /// hash is durable, and the changed bytes have not been moved back.
    BeforeMoveBack,
    /// The quarantined bytes matched the precondition and nothing is placed.
    AfterVerification,
    /// The operation result is durable and the quarantine still exists.
    BeforeRelease,
}

/// Points where a test barrier may change the live destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveMutationBarrier {
    /// The precondition hash matched and no namespace change happened yet.
    AfterPrecondition,
    /// The displaced bytes were verified and the new bytes are not placed.
    AfterQuarantineVerified,
    /// The new bytes were placed and the operation result is not yet read
    /// back or journaled.
    AfterPlacement,
}

/// Receives the absolute live destination and the operation index.
pub type LiveMutationHook = fn(&Path, usize, LiveMutationBarrier);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostInstallActionOutcome {
    pub component_id: String,
    pub state: String,
    pub evidence: String,
}

pub type PostInstallActionRunner =
    fn(&Path, &InstallationPlan, &str) -> Result<PostInstallActionOutcome, AppError>;

fn reviewed_external_action_evidence(
    plan: &InstallationPlan,
    component_id: &str,
) -> Result<Vec<String>, AppError> {
    const MAX_ACTION_EVIDENCE_BYTES: usize = 32 * 1024;
    plan.external_actions
        .iter()
        .filter(|action| {
            action.component_id == component_id
                || (component_id == crate::mcp::COMPONENT_ID
                    && action.component_id == "mcp.hoi4_agent_tools.bootstrap")
        })
        .map(|action| {
            let serialized = serde_json::to_string(action)?;
            if serialized.len() > MAX_ACTION_EVIDENCE_BYTES {
                return Err(AppError::Transaction(
                    "reviewed external-action evidence is too large to journal".into(),
                ));
            }
            let summary = serde_json::to_string(&serde_json::json!({
                "id": action.id,
                "component_id": action.component_id,
                "command_source": action.command_source,
                "network_access": action.network_access,
                "expected_writes": action.expected_writes,
                "rollback_boundary": action.rollback_boundary,
            }))?;
            Ok(redact_secrets(
                &format!(
                    "external-action-reviewed:sha256={}:{}",
                    sha256_bytes(serialized.as_bytes()),
                    summary
                ),
                &[],
            )
            .chars()
            .take(STAGE_EVIDENCE_MAX_CHARS)
            .collect())
        })
        .collect()
}

pub fn validate_plan(plan: &InstallationPlan) -> Result<(), AppError> {
    if plan.transaction.stages
        != TRANSACTION_STAGES
            .iter()
            .map(|stage| stage.to_string())
            .collect::<Vec<_>>()
    {
        return Err(AppError::Transaction(
            "plan does not contain the required twelve ordered stages".into(),
        ));
    }
    let mut profile_directories = std::collections::HashSet::new();
    for directory in &plan.transaction.directories {
        let normalized = normalize_relative_path(directory)?;
        if &normalized != directory || normalized.rsplit('/').next() == Some(".gitkeep") {
            return Err(AppError::PathSecurity(format!(
                "invalid profile directory: {directory}"
            )));
        }
        let key = canonical_relative_key(&normalized)?;
        if !profile_directories.insert(key) {
            return Err(AppError::PathSecurity(format!(
                "duplicate profile directory: {directory}"
            )));
        }
    }
    crate::source::validate_commit(&plan.source.resolved_revision)?;
    crate::source::validate_sha256(&plan.source.manifest_sha256)?;
    let ai_profile = crate::ai::profile(&plan.ai_provider).ok_or_else(|| {
        AppError::Credential(format!(
            "plan uses an unsupported AI provider: {}",
            plan.ai_provider
        ))
    })?;
    if plan.ai_model.trim().is_empty() || plan.ai_model.len() > 256 {
        return Err(AppError::Credential(
            "plan AI model must be non-empty and bounded".into(),
        ));
    }
    crate::ai::validate_reasoning_effort(&plan.ai_reasoning_effort)?;
    if plan.ai_optimization_profile != ai_profile.optimization_profile {
        return Err(AppError::Credential(
            "plan AI optimization profile does not match the selected provider".into(),
        ));
    }
    crate::ai::validate_endpoint_for_provider(
        plan.ai_provider.as_str(),
        plan.ai_endpoint.as_deref(),
    )?;
    crate::coding_environment::validate_selection(&CodingEnvironmentSelection {
        primary: plan.primary_coding_environment.clone(),
        additional: plan.additional_coding_environments.clone(),
    })?;
    if !matches!(
        plan.source.manifest_origin.as_str(),
        "remote" | "bundled_revision_bootstrap"
    ) {
        return Err(AppError::Source(
            "plan has an unsupported manifest origin".into(),
        ));
    }
    let removing = plan.maintenance_mode.as_deref() == Some("remove")
        && !plan.operations.is_empty()
        && plan.operations.iter().all(|operation| {
            matches!(
                operation.action,
                OperationAction::Skip | OperationAction::DeleteManaged
            )
        })
        && plan.generated_artifacts.is_empty()
        && plan.external_actions.is_empty()
        && plan.git_setup.is_none();
    if let Some(codex_analysis) = plan.codex_analysis.as_ref() {
        crate::codex::validate_confirmed_record(codex_analysis)?;
        if codex_analysis.provider.as_deref() != Some(plan.ai_provider.as_str())
            || codex_analysis.model.as_deref() != Some(plan.ai_model.as_str())
            || codex_analysis.reasoning_effort.as_deref() != Some(plan.ai_reasoning_effort.as_str())
            || codex_analysis.optimization_profile.as_deref()
                != Some(plan.ai_optimization_profile.as_str())
        {
            return Err(AppError::Credential(
                "the confirmed semantic analysis does not match the selected provider, model, reasoning effort, or profile".into(),
            ));
        }
        if codex_analysis.source_revision.as_deref() != Some(plan.source.resolved_revision.as_str())
            || codex_analysis.source_manifest_sha256.as_deref()
                != Some(plan.source.manifest_sha256.as_str())
        {
            return Err(AppError::Source(
                "the confirmed semantic analysis is bound to a different source manifest".into(),
            ));
        }
    } else if !removing {
        return Err(AppError::Credential(
            "a confirmed selected-provider analysis is required before apply".into(),
        ));
    }
    if plan.source.repository
        != format!(
            "{}/{}",
            crate::source::SOURCE_OWNER,
            crate::source::SOURCE_NAME
        )
    {
        return Err(AppError::Source(
            "plan source repository is not the approved workflow repository".into(),
        ));
    }
    if !plan.approvals.dry_run_reviewed {
        return Err(AppError::Transaction(
            "dry-run approval is required before mutation".into(),
        ));
    }
    if !plan.approvals.external_actions_reviewed {
        return Err(AppError::Transaction(
            "external-action review is required before mutation".into(),
        ));
    }
    if plan
        .git_setup
        .as_ref()
        .is_some_and(|setup| setup.remote_url.is_some())
        && !plan.approvals.git_remote_approved
    {
        return Err(AppError::Transaction(
            "configured Git remote requires explicit approval before apply".into(),
        ));
    }
    if plan
        .external_actions
        .iter()
        .any(|action| action.contains_secret)
    {
        return Err(AppError::Credential(
            "external action contains a serialized secret".into(),
        ));
    }
    for artifact in &plan.generated_artifacts {
        crate::source::validate_sha256(&artifact.expected_sha256)?;
        let artifact_bytes = artifact
            .bytes
            .as_deref()
            .unwrap_or(artifact.content.as_bytes());
        if sha256_bytes(artifact_bytes) != artifact.expected_sha256 {
            return Err(AppError::Transaction(format!(
                "generated artifact checksum mismatch: {}",
                artifact.destination
            )));
        }
        if artifact.external {
            validate_external_destination(&artifact.destination)?;
        } else {
            normalize_relative_path(&artifact.destination)
                .map_err(|error| AppError::Transaction(error.to_string()))?;
        }
    }
    let mut destinations = std::collections::HashSet::new();
    let mut operation_ids = std::collections::HashSet::new();
    let mut component_ids = std::collections::HashSet::new();
    for component in &plan.selected_components {
        if !component_ids.insert(component) {
            return Err(AppError::Transaction(format!(
                "duplicate selected component: {component}"
            )));
        }
    }
    for operation in &plan.operations {
        if operation.ownership.is_none() {
            return Err(AppError::Transaction(format!(
                "operation ownership is required: {}",
                operation.destination
            )));
        }
        if operation.id.trim().is_empty() || !operation_ids.insert(operation.id.clone()) {
            return Err(AppError::Transaction(format!(
                "operation IDs must be non-empty and unique: {}",
                operation.id
            )));
        }
        if matches!(
            operation.action,
            OperationAction::External | OperationAction::Chmod
        ) {
            return Err(AppError::Transaction(format!(
                "unsupported external operation action: {:?}",
                operation.action
            )));
        }
        if let Some(scope) = operation.location_scope.as_deref() {
            if !matches!(scope, "project" | "external_launcher" | "application_data")
                || (operation.external && scope != "external_launcher")
                || (!operation.external && scope == "external_launcher")
            {
                return Err(AppError::Transaction(format!(
                    "operation location scope does not match destination: {}",
                    operation.destination
                )));
            }
        }
        if !operation.external && operation.external_parent_identity.is_some() {
            return Err(AppError::Transaction(format!(
                "only an external destination can carry a parent identity: {}",
                operation.destination
            )));
        }
        if let Some(result_sha256) = &operation.result_sha256 {
            crate::source::validate_sha256(result_sha256)?;
        }
        for hash in [
            operation.source_sha256.as_deref(),
            operation.base_sha256.as_deref(),
            operation.local_sha256.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            crate::source::validate_sha256(hash)?;
        }
        let destination = if operation.external {
            validate_external_destination(&operation.destination)
                .map_err(|error| AppError::Transaction(error.to_string()))?
                .display()
                .to_string()
                .to_lowercase()
        } else {
            canonical_relative_key(&operation.destination)
                .map_err(|error| AppError::Transaction(error.to_string()))?
        };
        if !destinations.insert(destination) {
            return Err(AppError::Transaction(format!(
                "duplicate plan destination: {}",
                operation.destination
            )));
        }
        if operation.action == OperationAction::External
            && operation.rollback != RollbackAction::None
        {
            return Err(AppError::Transaction(
                "external operation must declare rollback none".into(),
            ));
        }
        if operation.action != OperationAction::Skip && operation.rollback == RollbackAction::None {
            return Err(AppError::Transaction(format!(
                "mutating operation must declare a rollback action: {}",
                operation.destination
            )));
        }
        if operation
            .source_path
            .as_deref()
            .is_some_and(|path| path.starts_with("generated:"))
        {
            let expected = if operation.action == OperationAction::Rename {
                RollbackAction::RemoveCreated
            } else if operation.action == OperationAction::Generate
                && operation.local_sha256.is_some()
            {
                RollbackAction::RestoreBackup
            } else if operation.action == OperationAction::Generate {
                RollbackAction::RemoveCreated
            } else {
                operation.rollback
            };
            if operation.action != OperationAction::Skip && operation.rollback != expected {
                return Err(AppError::Transaction(format!(
                    "generated operation has inconsistent rollback metadata: {}",
                    operation.destination
                )));
            }
        }
    }
    for conflict in &plan.conflicts {
        if conflict.selected.is_none() {
            return Err(AppError::Transaction(format!(
                "unresolved conflict: {}",
                conflict.path
            )));
        }
        if let Some(selected) = &conflict.selected {
            if !conflict.options.iter().any(|option| option == selected) {
                return Err(AppError::Transaction(format!(
                    "invalid conflict choice for {}",
                    conflict.path
                )));
            }
            let matching = plan.operations.iter().find(|operation| {
                operation.resolution.as_deref() == Some(selected.as_str())
                    && (operation.destination == conflict.path
                        || selected == "rename"
                        || operation.component_id == conflict.id.trim_start_matches("conflict."))
            });
            let action_matches = matching.is_some_and(|operation| match selected.as_str() {
                "keep" | "skip" => operation.action == OperationAction::Skip,
                "replace" => matches!(
                    operation.action,
                    OperationAction::Replace | OperationAction::Generate
                ),
                "merge" => operation.action == OperationAction::Merge,
                "rename" => operation.action == OperationAction::Rename,
                _ => false,
            });
            if !action_matches {
                return Err(AppError::Transaction(format!(
                    "conflict decision is not bound to an operation: {}",
                    conflict.path
                )));
            }
        }
    }
    if plan.approvals.push_approved {
        return Err(AppError::Transaction(
            "push approval is outside the setup transaction".into(),
        ));
    }
    Ok(())
}

fn validate_plan_project_root(
    plan: &InstallationPlan,
    project_root: &Path,
    root_exists: bool,
) -> Result<(), AppError> {
    match plan.transaction.project_root_mode {
        ProjectRootMode::Existing => {
            if !root_exists {
                return Err(AppError::Transaction(
                    "an existing-project plan cannot create a missing project root".into(),
                ));
            }
        }
        ProjectRootMode::CreateLeaf => {
            if plan.maintenance_mode.is_some() || root_exists {
                return Err(AppError::Transaction(
                    "only a first installation may create an absent project root".into(),
                ));
            }
            let parent = plan
                .transaction
                .project_root_parent
                .as_deref()
                .ok_or_else(|| AppError::Transaction("new-project plan has no parent".into()))?;
            let leaf = plan
                .transaction
                .project_root_leaf
                .as_deref()
                .ok_or_else(|| AppError::Transaction("new-project plan has no leaf".into()))?;
            crate::security::normalize_relative_path(leaf)?;
            if leaf != plan.project_id {
                return Err(AppError::Transaction(
                    "new-project root leaf must match the reviewed project ID".into(),
                ));
            }
            let parent = validate_project_root(Path::new(parent))?;
            let expected = parent.join(leaf);
            if !same_root_path(&expected, project_root) {
                return Err(AppError::PathSecurity(
                    "new-project destination changed after review".into(),
                ));
            }
        }
    }
    Ok(())
}

pub fn new_journal(
    plan: &InstallationPlan,
    project_id: &str,
    project_root: &Path,
) -> TransactionJournal {
    let now = Utc::now().to_rfc3339();
    let root_identity = match plan.transaction.project_root_mode {
        ProjectRootMode::Existing => plan.transaction.project_root_identity.clone().or_else(|| {
            RootedDir::open_read(project_root)
                .ok()?
                .identity_token()
                .ok()
        }),
        ProjectRootMode::CreateLeaf => {
            if project_root.exists() {
                RootedDir::open_read(project_root)
                    .ok()
                    .and_then(|directory| directory.identity_token().ok())
            } else {
                None
            }
        }
    };
    let parent_identity = (plan.transaction.project_root_mode == ProjectRootMode::CreateLeaf)
        .then(|| plan.transaction.project_root_identity.clone())
        .flatten()
        .or_else(|| {
            (plan.transaction.project_root_mode == ProjectRootMode::CreateLeaf)
                .then(|| {
                    plan.transaction
                        .project_root_parent
                        .as_deref()
                        .map(Path::new)
                        .or_else(|| project_root.parent())
                        .and_then(|path| RootedDir::open_read(path).ok())
                        .and_then(|directory| directory.identity_token().ok())
                })
                .flatten()
        });
    TransactionJournal {
        schema_version: crate::migrations::CURRENT_JOURNAL_SCHEMA.into(),
        transaction_id: plan.plan_id,
        transaction_kind: "installation".into(),
        parent_transaction_id: None,
        rollback_transaction_id: None,
        result_lock_sha256: None,
        result_lock_exists: None,
        rollback_record_sha256: None,
        project_id: project_id.into(),
        project_root: project_root.display().to_string(),
        project_root_lifecycle: ProjectRootLifecycle {
            mode: plan.transaction.project_root_mode,
            canonical_parent: plan.transaction.project_root_parent.clone(),
            leaf: plan.transaction.project_root_leaf.clone(),
            root_identity,
            parent_identity,
            checkpoint: if plan.transaction.project_root_mode == ProjectRootMode::CreateLeaf {
                "pending".into()
            } else {
                "not_required".into()
            },
            created_by_transaction: false,
            observed_exists: plan.transaction.project_root_mode == ProjectRootMode::Existing,
            cleanup_result: None,
        },
        primary_coding_environment: plan.primary_coding_environment.clone(),
        additional_coding_environments: plan.additional_coding_environments.clone(),
        state: "preflight".into(),
        created_at: now.clone(),
        updated_at: now,
        last_checkpoint: "preflight".into(),
        plan_sha256: serde_json::to_vec(plan)
            .ok()
            .map(|bytes| sha256_bytes(&bytes)),
        stages: TRANSACTION_STAGES
            .iter()
            .map(|id| StageCheckpoint {
                id: (*id).into(),
                status: "pending".into(),
                started_at: None,
                completed_at: None,
                evidence: vec![],
            })
            .collect(),
        operations: plan
            .operations
            .iter()
            .map(|operation| JournalOperation {
                id: operation.id.clone(),
                status: "pending".into(),
                destination: operation.destination.clone(),
                ownership: operation.ownership,
                component_id: Some(operation.component_id.clone()),
                source_path: operation.source_path.clone(),
                source_size: operation.source_size,
                action: Some(operation.action),
                location_scope: operation.location_scope.clone(),
                external: operation.external,
                backup_path: None,
                before_sha256: operation.local_sha256.clone(),
                before_executable: None,
                expected_sha256: operation
                    .result_sha256
                    .clone()
                    .or_else(|| operation.source_sha256.clone()),
                source_sha256: operation.source_sha256.clone(),
                result_sha256: operation.result_sha256.clone(),
                expected_executable: (!matches!(
                    operation.action,
                    OperationAction::DeleteManaged
                        | OperationAction::Skip
                        | OperationAction::External
                ))
                .then_some(operation.executable),
                rollback: Some(operation.rollback),
                rollback_source_path: None,
                resolution: operation.resolution.clone(),
                backup_sha256: None,
                staged_sha256: None,
                after_sha256: None,
                after_exists: None,
                after_executable: None,
                quarantine_leaf: None,
                quarantine_sha256: None,
                // The journal starts from the parent reviewed in the plan, so
                // the backup stage verifies that directory instead of binding
                // whatever occupies the path when the transaction starts.
                external_parent_identity: operation
                    .external
                    .then(|| operation.external_parent_identity.clone())
                    .flatten(),
            })
            .collect(),
        created_directories: Vec::new(),
        recovery: RecoveryState {
            resume_allowed: true,
            rollback_allowed: true,
            discard_staging_allowed: true,
            project_apply_started: false,
            recommended_action: "resume".into(),
        },
        git_initialized: false,
        git_remote_added_name: None,
        git_remote_added_url: None,
        previous_lock_backup_path: None,
        previous_lock_sha256: None,
        checkpoint_sequence: None,
        // Bound by the caller once the transaction's storage is open.
        app_data_identity: None,
        error: None,
    }
}

fn validate_journal_project_root(
    project_root: &Path,
    journal: &TransactionJournal,
    journal_path: &Path,
) -> Result<PathBuf, AppError> {
    if journal.project_root.trim().is_empty() {
        return Err(AppError::PathSecurity(
            "transaction journal has no project-root binding".into(),
        ));
    }
    let requested_root =
        validated_root_for_lifecycle(project_root, &journal.project_root_lifecycle)?;
    let bound_root = validated_root_for_lifecycle(
        Path::new(&journal.project_root),
        &journal.project_root_lifecycle,
    )?;
    let roots_match = same_root_path(&bound_root, &requested_root);
    if !roots_match {
        return Err(AppError::PathSecurity(
            "requested project root does not match the journal binding".into(),
        ));
    }
    validate_project_root_lifecycle_identity(&requested_root, &journal.project_root_lifecycle)?;
    let journal_file = journal_path.file_name().and_then(|name| name.to_str());
    let transaction_directory = journal_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str());
    let expected_transaction_directory = journal.transaction_id.to_string();
    if journal_file != Some("journal.json")
        || transaction_directory != Some(expected_transaction_directory.as_str())
    {
        return Err(AppError::PathSecurity(
            "transaction journal path is not bound to its transaction ID".into(),
        ));
    }
    Ok(requested_root)
}

fn validate_project_root_lifecycle_identity(
    project_root: &Path,
    lifecycle: &ProjectRootLifecycle,
) -> Result<(), AppError> {
    match lifecycle.mode {
        ProjectRootMode::Existing => {
            let expected = lifecycle.root_identity.as_deref().ok_or_else(|| {
                AppError::PathSecurity(
                    "transaction journal has no identity binding for the existing project root"
                        .into(),
                )
            })?;
            let observed = RootedDir::open_read(project_root)?.identity_token()?;
            if observed != expected {
                return Err(AppError::PathSecurity(
                    "project root directory identity changed after review".into(),
                ));
            }
        }
        ProjectRootMode::CreateLeaf => {
            let parent = lifecycle.canonical_parent.as_deref().ok_or_else(|| {
                AppError::PathSecurity("create-root journal has no canonical parent".into())
            })?;
            let expected_parent = lifecycle.parent_identity.as_deref().ok_or_else(|| {
                AppError::PathSecurity(
                    "create-root journal has no identity binding for its parent".into(),
                )
            })?;
            let observed_parent = RootedDir::open_read(Path::new(parent))?.identity_token()?;
            if observed_parent != expected_parent {
                return Err(AppError::PathSecurity(
                    "project root parent directory identity changed after review".into(),
                ));
            }

            match fs::symlink_metadata(project_root) {
                Ok(_) => {
                    let root = RootedDir::open_read(project_root)?;
                    let Some(expected_root) = lifecycle.root_identity.as_deref() else {
                        // A process can stop after mkdir succeeds and before its
                        // identity reaches the journal. Recovery may inspect and
                        // preserve this root, but must never claim it as managed.
                        if lifecycle.checkpoint == "applying" && !lifecycle.created_by_transaction {
                            return Ok(());
                        }
                        return Err(AppError::PathSecurity(
                            "project root appeared before its identity was journaled; manual inspection is required".into(),
                        ));
                    };
                    if root.identity_token()? != expected_root {
                        return Err(AppError::PathSecurity(
                            "created project root directory identity changed".into(),
                        ));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if lifecycle.observed_exists
                        && !matches!(lifecycle.checkpoint.as_str(), "removing" | "removed")
                    {
                        return Err(AppError::PathSecurity(
                            "journaled project root directory is missing".into(),
                        ));
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

fn open_bound_project_root(
    project_root: &Path,
    lifecycle: &ProjectRootLifecycle,
) -> Result<RootedDir, AppError> {
    let root = match lifecycle.mode {
        ProjectRootMode::Existing => RootedDir::open(project_root)?,
        ProjectRootMode::CreateLeaf => {
            let parent_path = lifecycle.canonical_parent.as_deref().ok_or_else(|| {
                AppError::PathSecurity("create-root journal has no canonical parent".into())
            })?;
            let leaf = lifecycle.leaf.as_deref().ok_or_else(|| {
                AppError::PathSecurity("create-root journal has no validated leaf".into())
            })?;
            crate::security::normalize_relative_path(leaf)?;
            let parent = RootedDir::open(Path::new(parent_path))?;
            if Some(parent.identity_token()?.as_str()) != lifecycle.parent_identity.as_deref() {
                return Err(AppError::PathSecurity(
                    "project root parent directory identity changed".into(),
                ));
            }
            parent.open_dir(leaf)?
        }
    };
    let expected_root = lifecycle.root_identity.as_deref().ok_or_else(|| {
        AppError::PathSecurity("transaction has no identity binding for the project root".into())
    })?;
    if root.identity_token()? != expected_root {
        return Err(AppError::PathSecurity(
            "project root directory identity changed before the rooted operation".into(),
        ));
    }
    Ok(root)
}

fn reviewed_project_root_identity(
    plan: &InstallationPlan,
    project_root: &Path,
) -> Result<String, AppError> {
    let anchor = match plan.transaction.project_root_mode {
        ProjectRootMode::Existing => project_root.to_path_buf(),
        ProjectRootMode::CreateLeaf => plan
            .transaction
            .project_root_parent
            .as_deref()
            .map(PathBuf::from)
            .or_else(|| project_root.parent().map(Path::to_path_buf))
            .ok_or_else(|| {
                AppError::PathSecurity("new project root has no identity-bound parent".into())
            })?,
    };
    RootedDir::open_read(&anchor)?.identity_token()
}

fn validate_plan_project_root_identity(
    plan: &InstallationPlan,
    project_root: &Path,
) -> Result<(), AppError> {
    let expected = plan
        .transaction
        .project_root_identity
        .as_deref()
        .ok_or_else(|| {
            AppError::PathSecurity("installation plan has no project-root identity binding".into())
        })?;
    let observed = reviewed_project_root_identity(plan, project_root)?;
    if observed != expected {
        return Err(AppError::PathSecurity(
            "project root or its reviewed parent changed after planning".into(),
        ));
    }
    Ok(())
}

/// Observe an external destination through one retained handle on its
/// parent: the parent's identity and the hash of a regular-file leaf come
/// from the same opened directory. A parent that does not exist yields
/// neither; a link or other non-directory at the parent path is refused by
/// `RootedDir`.
fn observe_external_destination(
    destination: &str,
) -> Result<(Option<String>, Option<String>), AppError> {
    let absolute = validate_external_destination(destination)?;
    let parent = absolute.parent().ok_or_else(|| {
        AppError::PathSecurity("external destination has no parent directory".into())
    })?;
    let leaf = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::PathSecurity("external destination name is invalid".into()))?;
    match fs::symlink_metadata(parent) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((None, None)),
        Err(error) => return Err(error.into()),
    }
    let directory = RootedDir::open_read(parent)?;
    let hash = if directory.is_regular_file(leaf)? {
        Some(directory.hash_file(leaf)?)
    } else {
        None
    };
    Ok((Some(directory.identity_token()?), hash))
}

/// Bind every external destination of a plan under review to the parent
/// directory whose contents the plan reviewed. The identity and a fresh hash
/// of the destination are read through one handle, and the hash must still
/// equal the reviewed `local_sha256`, so the bound directory is the one that
/// was reviewed rather than one swapped in after the review hash was taken.
/// A parent that does not exist at review stays unbound; the backup stage
/// then binds the directory it first opens, as for plans from before
/// plan-time binding.
pub fn bind_plan_external_parents(plan: &mut InstallationPlan) -> Result<(), AppError> {
    for operation in plan
        .operations
        .iter_mut()
        .filter(|operation| operation.external)
    {
        let (identity, observed) = observe_external_destination(&operation.destination)?;
        if identity.is_some() && observed != operation.local_sha256 {
            return Err(AppError::Transaction(format!(
                "external destination changed while the plan was built; review it again: {}",
                operation.destination
            )));
        }
        operation.external_parent_identity = identity;
    }
    Ok(())
}

/// Refuse, before any transaction storage is written, an external
/// destination parent that is no longer the directory bound in the reviewed
/// plan. A mutating operation also needs its reviewed parent to exist; a
/// skipped operation never changes its destination, so a missing parent is
/// left to the later checks that read it.
fn validate_plan_external_parent_identities(plan: &InstallationPlan) -> Result<(), AppError> {
    for operation in plan
        .operations
        .iter()
        .filter(|operation| operation.external)
    {
        let Some(expected) = operation.external_parent_identity.as_deref() else {
            continue;
        };
        let absolute = validate_external_destination(&operation.destination)?;
        let parent = absolute.parent().ok_or_else(|| {
            AppError::PathSecurity("external destination has no parent directory".into())
        })?;
        if !external_parent_present(parent, Some(expected), &operation.destination)? {
            if matches!(
                operation.action,
                OperationAction::Skip | OperationAction::External
            ) {
                continue;
            }
            return Err(AppError::PathSecurity(format!(
                "the folder reviewed for {} is missing; move it back to {} or review the plan again",
                operation.destination,
                parent.display()
            )));
        }
        let directory = RootedDir::open_read(parent)?;
        verify_external_parent_identity(&directory, Some(expected), &operation.destination)?;
    }
    Ok(())
}

fn open_plan_bound_project_root(
    plan: &InstallationPlan,
    project_root: &Path,
) -> Result<RootedDir, AppError> {
    if plan.transaction.project_root_mode != ProjectRootMode::Existing {
        return Err(AppError::PathSecurity(
            "an absent project root cannot be opened before apply".into(),
        ));
    }
    let expected = plan
        .transaction
        .project_root_identity
        .as_deref()
        .ok_or_else(|| {
            AppError::PathSecurity("installation plan has no project-root identity binding".into())
        })?;
    let root = RootedDir::open(project_root)?;
    if root.identity_token()? != expected {
        return Err(AppError::PathSecurity(
            "project root changed while acquiring the transaction handle".into(),
        ));
    }
    Ok(root)
}

fn same_root_path(left: &Path, right: &Path) -> bool {
    if cfg!(target_os = "windows") {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

fn validated_root_for_lifecycle(
    path: &Path,
    lifecycle: &ProjectRootLifecycle,
) -> Result<PathBuf, AppError> {
    match lifecycle.mode {
        ProjectRootMode::Existing => validate_project_root(path),
        ProjectRootMode::CreateLeaf => {
            let parent = lifecycle.canonical_parent.as_deref().ok_or_else(|| {
                AppError::PathSecurity("create-root journal has no canonical parent".into())
            })?;
            let leaf = lifecycle.leaf.as_deref().ok_or_else(|| {
                AppError::PathSecurity("create-root journal has no validated leaf".into())
            })?;
            crate::security::normalize_relative_path(leaf)?;
            let parent = validate_project_root(Path::new(parent))?;
            let expected = parent.join(leaf);
            let (validated, _) = validate_project_root_or_destination(path)?;
            if !same_root_path(&validated, &expected) {
                return Err(AppError::PathSecurity(
                    "project root does not match the journaled parent and leaf".into(),
                ));
            }
            Ok(validated)
        }
    }
}

fn journal_app_root(journal_path: &Path) -> Result<PathBuf, AppError> {
    journal_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| AppError::PathSecurity("journal has no application root".into()))
}

/// Transaction states are terminal only after the journal no longer needs a
/// recovery decision. Every other state is treated as an incomplete journal;
/// this includes ordinary active checkpoints because a process can stop
/// without running the error handler that normally changes the state to
/// `interrupted`.
pub fn transaction_state_is_terminal(state: &str) -> bool {
    matches!(state, "completed" | "rolled_back" | "staging_discarded")
}

fn normalize_incomplete_recovery(journal: &mut TransactionJournal) {
    if transaction_state_is_terminal(&journal.state) {
        return;
    }
    let identity_binding_present = match journal.project_root_lifecycle.mode {
        ProjectRootMode::Existing => journal.project_root_lifecycle.root_identity.is_some(),
        ProjectRootMode::CreateLeaf => {
            let lifecycle = &journal.project_root_lifecycle;
            lifecycle.parent_identity.is_some()
                && (!lifecycle.observed_exists
                    || lifecycle.root_identity.is_some()
                    || (lifecycle.checkpoint == "applying" && !lifecycle.created_by_transaction)
                    || matches!(lifecycle.checkpoint.as_str(), "removing" | "removed"))
        }
    };
    if !identity_binding_present {
        journal.recovery.resume_allowed = false;
        journal.recovery.rollback_allowed = false;
        journal.recovery.discard_staging_allowed = false;
        journal.recovery.recommended_action = "inspect".into();
        return;
    }
    if journal.state == "finalizing" {
        return;
    }
    if journal.state == "rolling_back" {
        journal.recovery.resume_allowed = false;
        journal.recovery.rollback_allowed = true;
        journal.recovery.discard_staging_allowed = false;
        journal.recovery.project_apply_started = true;
        journal.recovery.recommended_action = "rollback".into();
        return;
    }
    if journal.transaction_kind == "rollback" {
        journal.recovery.resume_allowed = false;
        journal.recovery.rollback_allowed = false;
        journal.recovery.discard_staging_allowed = false;
        journal.recovery.project_apply_started = true;
        journal.recovery.recommended_action = "inspect".into();
        return;
    }
    let apply_started = journal.recovery.project_apply_started
        || journal.operations.iter().any(|operation| {
            matches!(
                operation.status.as_str(),
                "applying" | "applied" | "verified" | "rollback_applying" | "rolled_back"
            )
        });
    let staging_complete = journal
        .stages
        .iter()
        .find(|stage| stage.id == "staging")
        .is_some_and(|stage| stage.status == "complete");
    journal.recovery.project_apply_started = apply_started;
    journal.recovery.resume_allowed = !apply_started && staging_complete;
    journal.recovery.rollback_allowed = apply_started;
    journal.recovery.discard_staging_allowed = !apply_started;
    journal.recovery.recommended_action = if apply_started {
        "rollback"
    } else if staging_complete {
        "resume"
    } else {
        "discard_staging"
    }
    .into();
}

fn mark_project_apply_started(journal: &mut TransactionJournal) {
    journal.recovery = RecoveryState {
        resume_allowed: false,
        rollback_allowed: true,
        discard_staging_allowed: false,
        project_apply_started: true,
        recommended_action: "rollback".into(),
    };
}

fn roots_match_for_transaction(bound: &str, requested: &Path) -> bool {
    let Ok((bound_root, _)) = validate_project_root_or_destination(Path::new(bound)) else {
        return false;
    };
    let Ok((requested_root, _)) = validate_project_root_or_destination(requested) else {
        return false;
    };
    same_root_path(&bound_root, &requested_root)
}

/// Find a non-terminal journal bound to a project before starting a new
/// mutation. Startup discovery is useful for the UI, but this core-owned
/// check is the final exclusivity boundary and also protects callers that do
/// not go through the wizard.
pub fn find_incomplete_transaction(
    app_root: &Path,
    project_root: &Path,
) -> Result<Option<TransactionJournal>, AppError> {
    if path_has_link_component(app_root) {
        return Err(AppError::PathSecurity(
            "transaction application root contains a symlink or junction".into(),
        ));
    }
    let transactions_root = app_root.join("transactions");
    if path_has_link_component(&transactions_root) {
        return Err(AppError::PathSecurity(
            "transaction storage contains a symlink or junction".into(),
        ));
    }
    match fs::symlink_metadata(&transactions_root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(metadata) if is_link_metadata(&metadata) || !metadata.is_dir() => {
            return Err(AppError::PathSecurity(
                "transaction storage is not a regular directory".into(),
            ));
        }
        Ok(_) => {}
        Err(error) => return Err(error.into()),
    }
    // Every journal is read through handles retained from the application
    // data root, so a journal that recorded its storage identities can be
    // compared with the directories that actually hold it.
    let app = RootedDir::open_read(app_root)?;
    let transactions = app.open_dir(TRANSACTIONS_AREA)?;
    let entries = transactions.read_dir_names()?;
    let mut candidates = Vec::new();
    for entry in entries {
        let Some(entry_name) = entry.to_str() else {
            continue;
        };
        if !transactions.is_directory(entry_name)? {
            continue;
        }
        let path = transactions_root.join(entry_name);
        let directory = transactions.open_dir(entry_name)?;
        if !directory.exists(JOURNAL_FILE)? {
            continue;
        }
        if !directory.is_regular_file(JOURNAL_FILE)? {
            return Err(AppError::PathSecurity(
                "transaction journal is not a regular file".into(),
            ));
        }
        let journal_path = path.join(JOURNAL_FILE);
        match load_journal_in(&directory, JOURNAL_FILE) {
            Ok(mut journal) => {
                if !transaction_state_is_terminal(&journal.state)
                    && roots_match_for_transaction(&journal.project_root, project_root)
                {
                    // Storage swapped away from an incomplete journal of this
                    // project blocks a new transaction instead of being
                    // ignored or followed.
                    verify_app_data_binding(&journal, Some(&app), &directory)?;
                    normalize_incomplete_recovery(&mut journal);
                    candidates.push(journal);
                }
            }
            Err(error) => {
                // A corrupt journal cannot be safely recovered, but if its
                // bounded root field identifies this project it must still
                // block a second transaction instead of being ignored.
                let bytes = directory.read_file(JOURNAL_FILE)?;
                if bytes.len() <= 1024 * 1024 {
                    let root_matches = serde_json::from_slice::<serde_json::Value>(&bytes)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("project_root")
                                .and_then(serde_json::Value::as_str)
                                .map(ToOwned::to_owned)
                        })
                        .is_some_and(|bound| roots_match_for_transaction(&bound, project_root));
                    if root_matches {
                        return Err(AppError::Transaction(format!(
                            "an unreadable transaction journal requires recovery before a new transaction: {}",
                            journal_path.display()
                        )));
                    }
                }
                // Journals for another project remain outside this request's
                // scope. Preserve the original parse failure only when the
                // file could plausibly belong to the selected project.
                let _ = error;
            }
        }
    }
    let referenced_parents = candidates
        .iter()
        .filter_map(|journal| journal.parent_transaction_id)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let priority = |journal: &TransactionJournal| {
            if referenced_parents.contains(&journal.transaction_id) {
                0
            } else if journal.transaction_kind == "rollback" {
                2
            } else {
                1
            }
        };
        priority(left)
            .cmp(&priority(right))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    Ok(candidates.into_iter().next())
}

fn read_existing_lock_from_root(project: &RootedDir) -> Result<Option<InstallationLock>, AppError> {
    let lock_relative = ".hoi4-mod-setup/install.lock.json";
    if !project.exists(lock_relative)? {
        return Ok(None);
    }
    if !project.is_regular_file(lock_relative)? {
        return Err(AppError::PathSecurity(
            "installation lock is not a regular file".into(),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&project.read_file(lock_relative)?)
        .map_err(|error| {
            AppError::Transaction(format!("invalid existing installation lock: {error}"))
        })?;
    crate::migrations::migrate_lock(value).map(Some)
}

fn capture_previous_lock(
    project_directory: Option<&RootedDir>,
    backup_root: &Path,
    backup_directory: &RootedDir,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    let Some(project_directory) = project_directory else {
        return Ok(());
    };
    let lock_relative = ".hoi4-mod-setup/install.lock.json";
    if !project_directory.exists(lock_relative)? {
        return Ok(());
    }
    if !project_directory.is_regular_file(lock_relative)? {
        return Err(AppError::PathSecurity(
            "installation lock is not a regular file".into(),
        ));
    }
    let bytes = project_directory.read_file(lock_relative)?;
    let backup_leaf = "install.lock.json.bak";
    let backup = backup_root.join(backup_leaf);
    let digest = sha256_bytes(&bytes);
    if backup_directory.exists(backup_leaf)? {
        if !backup_directory.is_regular_file(backup_leaf)?
            || backup_directory.hash_file(backup_leaf)? != digest
        {
            return Err(AppError::Transaction(
                "existing predecessor-lock backup does not match the reviewed lock".into(),
            ));
        }
    } else {
        backup_directory.write_atomic(backup_leaf, &bytes)?;
    }
    if backup_directory.hash_file(backup_leaf)? != digest {
        return Err(AppError::Transaction(
            "installation lock backup verification failed".into(),
        ));
    }
    journal.previous_lock_backup_path = Some(backup.display().to_string());
    journal.previous_lock_sha256 = Some(digest);
    persist_journal(store, journal)
}

pub fn run_transaction(
    project_root: &Path,
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    options: &TransactionOptions,
) -> Result<(TransactionJournal, InstallationLock), AppError> {
    let (project_root, root_exists) = validate_project_root_or_destination(project_root)?;
    validate_plan(plan)?;
    validate_plan_project_root(plan, &project_root, root_exists)?;
    validate_plan_project_root_identity(plan, &project_root)?;
    validate_plan_external_parent_identities(plan)?;
    validate_flatten_transaction_inputs(plan, prepared_files, &project_root)?;
    let mut effective_plan = plan.clone();
    let mut project_directory = if plan.transaction.project_root_mode == ProjectRootMode::Existing {
        Some(open_plan_bound_project_root(plan, &project_root)?)
    } else {
        None
    };
    let app_root = match options.app_data_root.clone() {
        Some(root) => root,
        None => application_data_root()?,
    };
    if crate::security::path_has_link_component(&app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    let mut interrupted_journal = None;
    if let Some(journal) = find_incomplete_transaction(&app_root, &project_root)? {
        if options.resume_transaction_id != Some(journal.transaction_id) {
            return Err(AppError::Transaction(format!(
                "an incomplete transaction must be recovered before starting another: {}",
                journal.transaction_id
            )));
        }
        interrupted_journal = Some(journal);
    }
    let previous_lock = if let Some(project) = project_directory.as_ref() {
        read_existing_lock_from_root(project)?
    } else {
        None
    };
    // The application-data root and this transaction's directories are held
    // for the whole call and bound into the journal, so later stages and
    // recovery calls refuse a directory swapped in at the same path.
    let app = AppDataRoot::open_or_create(&app_root)?;
    let store = app.transaction_store(plan.plan_id, true)?;
    let mut app_data_identity = app.bind_new_storage(&store)?;
    if let Some(interrupted) = interrupted_journal.as_ref() {
        // A replay writes a fresh journal into the interrupted run's storage,
        // which must still be the storage that run recorded. Its backup and
        // staging bindings carry over so those stages verify instead of
        // binding whatever now occupies the path.
        if let Some(bound) = interrupted.app_data_identity.as_ref() {
            if bound.root != app_data_identity.root {
                return Err(app_data_drift(&app.path));
            }
            if bound.transaction != app_data_identity.transaction {
                return Err(app_data_drift(store.journal_path()));
            }
            app_data_identity.backup = bound.backup.clone();
            app_data_identity.staging = bound.staging.clone();
        }
    }
    let backup_root = app.area_path(BACKUPS_AREA, plan.plan_id);
    store.write_json(PLAN_FILE, plan)?;
    let mut journal = new_journal(plan, &plan.project_id, &project_root);
    journal.app_data_identity = Some(app_data_identity);
    // A replay writes a fresh journal, but an external parent bound by the
    // interrupted run stays bound: the backup stage must find the same
    // directory instead of binding whatever now occupies the path. The
    // fresh journal already carries the plan's identity; an interrupted run
    // can only have bound that same directory.
    if let Some(interrupted) = interrupted_journal.as_ref() {
        for operation in journal
            .operations
            .iter_mut()
            .filter(|operation| operation.external)
        {
            let carried = interrupted
                .operations
                .iter()
                .find(|previous| {
                    previous.id == operation.id
                        && previous.external
                        && previous.destination == operation.destination
                })
                .and_then(|previous| previous.external_parent_identity.clone());
            match (operation.external_parent_identity.as_deref(), carried) {
                (Some(planned), Some(carried)) if planned != carried => {
                    return Err(AppError::PathSecurity(format!(
                        "the interrupted transaction bound a different folder than the reviewed plan for {}; manual review is required",
                        operation.destination
                    )));
                }
                (_, Some(carried)) => operation.external_parent_identity = Some(carried),
                (_, None) => {}
            }
        }
    }
    persist_journal(&store, &mut journal)?;

    let result: Result<InstallationLock, AppError> = (|| {
        stage_start(
            &mut journal,
            0,
            "preflight",
            &store,
            options.fail_before_stage,
        )?;
        stage_complete(
            &mut journal,
            0,
            "preflight",
            &store,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            1,
            "repository source resolution",
            &store,
            options.fail_before_stage,
        )?;
        add_stage_evidence(
            &mut journal,
            1,
            vec![
                format!("repository={}", plan.source.repository),
                format!("revision={}", plan.source.resolved_revision),
                format!("manifest_sha256={}", plan.source.manifest_sha256),
                format!("manifest_origin={}", plan.source.manifest_origin),
            ],
            &store,
        )?;
        stage_complete(
            &mut journal,
            1,
            "repository source resolution",
            &store,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            2,
            "selective download",
            &store,
            options.fail_before_stage,
        )?;
        let selected_evidence = validate_prepared_files(plan, prepared_files)?;
        add_stage_evidence(&mut journal, 2, selected_evidence, &store)?;
        stage_complete(
            &mut journal,
            2,
            "selective download",
            &store,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            3,
            "checksum verification",
            &store,
            options.fail_before_stage,
        )?;
        let verified_evidence = validate_prepared_files(plan, prepared_files)?;
        add_stage_evidence(&mut journal, 3, verified_evidence, &store)?;
        stage_complete(
            &mut journal,
            3,
            "checksum verification",
            &store,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            4,
            "dry-run review",
            &store,
            options.fail_before_stage,
        )?;
        stage_complete(
            &mut journal,
            4,
            "dry-run review",
            &store,
            options.fail_after_stage,
        )?;

        stage_start(&mut journal, 5, "backup", &store, options.fail_before_stage)?;
        let backup_directory = open_journal_area(&app, &mut journal, BACKUPS_AREA, plan.plan_id)?;
        persist_journal(&store, &mut journal)?;
        capture_previous_lock(
            project_directory.as_ref(),
            &backup_root,
            &backup_directory,
            &mut journal,
            &store,
        )?;
        backup_existing(
            &project_root,
            plan,
            &backup_root,
            &backup_directory,
            project_directory.as_ref(),
            &mut journal,
            &store,
        )?;
        compact_operation_checkpoints(&store, &mut journal)?;
        stage_complete(&mut journal, 5, "backup", &store, options.fail_after_stage)?;
        stage_start(
            &mut journal,
            6,
            "staging",
            &store,
            options.fail_before_stage,
        )?;
        let staging_directory = open_journal_area(&app, &mut journal, STAGING_AREA, plan.plan_id)?;
        persist_journal(&store, &mut journal)?;
        stage_files(
            plan,
            prepared_files,
            &staging_directory,
            &mut journal,
            &store,
        )?;
        stage_profile_directories(plan, &staging_directory)?;
        compact_operation_checkpoints(&store, &mut journal)?;
        stage_complete(&mut journal, 6, "staging", &store, options.fail_after_stage)?;
        stage_start(
            &mut journal,
            7,
            "validation",
            &store,
            options.fail_before_stage,
        )?;
        validate_staging(&project_root, plan, prepared_files, &staging_directory)?;
        stage_complete(
            &mut journal,
            7,
            "validation",
            &store,
            options.fail_after_stage,
        )?;
        stage_start(&mut journal, 8, "apply", &store, options.fail_before_stage)?;
        ensure_project_root_for_apply(&project_root, plan, &mut journal, &store)?;
        if project_directory.is_none() {
            project_directory = Some(open_bound_project_root(
                &project_root,
                &journal.project_root_lifecycle,
            )?);
        }
        let project_directory = project_directory.as_ref().ok_or_else(|| {
            AppError::PathSecurity("transaction has no retained project-root handle".into())
        })?;
        apply_profile_directories_rooted(project_directory, plan, &mut journal, &store)?;
        apply_operations(
            &project_root,
            plan,
            &staging_directory,
            project_directory,
            &mut journal,
            &store,
            options,
        )?;
        compact_operation_checkpoints(&store, &mut journal)?;
        project_directory.verify_bound_to_path()?;
        // Every operation verified, so a quarantine of this transaction that
        // still exists is a leftover; settle it by hash or stop here.
        sweep_transaction_quarantines(
            Some(project_directory),
            &journal,
            None,
            QuarantineSweep::Result,
        )?;
        if let Some(setup) = &plan.git_setup {
            journal.git_initialized = setup.mode == crate::git::GitMode::Initialize;
            if setup.mode == crate::git::GitMode::Preserve && setup.remote_url.is_some() {
                // Record the expected remote identity before invoking Git.
                // If the process stops after Git changes the repository but
                // before the result checkpoint, rollback can still compare
                // the live remote with the approved value and remove it only
                // when it is unchanged.
                journal.git_remote_added_name = setup.remote_name.clone();
                journal.git_remote_added_url = setup.remote_url.clone();
            }
            journal.last_checkpoint = "git-intent".into();
            persist_journal(&store, &mut journal)?;
            if options.fail_before_git {
                return Err(AppError::Transaction(
                    "fault injected before Git setup".into(),
                ));
            }
            let managed_paths = plan
                .operations
                .iter()
                .filter(|operation| {
                    !operation.external
                        && !matches!(
                            operation.action,
                            OperationAction::Skip
                                | OperationAction::External
                                | OperationAction::DeleteManaged
                        )
                })
                .map(|operation| {
                    let expected_sha256 = operation
                        .result_sha256
                        .clone()
                        .or_else(|| operation.source_sha256.clone())
                        .ok_or_else(|| {
                            AppError::Transaction(format!(
                                "managed Git path is missing expected hash evidence: {}",
                                operation.destination
                            ))
                        })?;
                    let destination = safe_join(&project_root, &operation.destination)?;
                    let expected_size = fs::symlink_metadata(&destination)
                        .map_err(|error| {
                            AppError::Transaction(format!(
                                "managed Git path metadata is unavailable: {}: {error}",
                                operation.destination
                            ))
                        })?
                        .len();
                    Ok(crate::git::ManagedGitPath {
                        relative: operation.destination.clone(),
                        expected_sha256,
                        expected_size,
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            project_directory.verify_bound_to_path()?;
            let git_result =
                crate::git::apply_git_setup(project_root.as_path(), setup, &managed_paths)?;
            project_directory.verify_bound_to_path()?;
            if options.fail_after_git {
                return Err(AppError::Transaction(
                    "fault injected after Git setup".into(),
                ));
            }
            journal.git_initialized = git_result.initialized;
            if git_result.remote_configured && setup.mode == crate::git::GitMode::Preserve {
                journal.git_remote_added_name = setup.remote_name.clone();
                journal.git_remote_added_url = setup.remote_url.clone();
            }
            journal.last_checkpoint = "git-verified".into();
            persist_journal(&store, &mut journal)?;
        }
        stage_complete(&mut journal, 8, "apply", &store, options.fail_after_stage)?;
        stage_start(
            &mut journal,
            9,
            "post-install checks",
            &store,
            options.fail_before_stage,
        )?;
        project_directory.verify_bound_to_path()?;
        post_install_checks(&project_root, project_directory, plan, &mut journal, &store)?;
        project_directory.verify_bound_to_path()?;
        if let Some(runner) = options.post_install_action_runner {
            let components = [crate::mcp::COMPONENT_ID, "workflow.3d"]
                .into_iter()
                .filter(|component_id| {
                    effective_plan
                        .optional_workflows
                        .get(*component_id)
                        .is_some_and(|state| {
                            matches!(state.as_str(), "selected_pending" | "incomplete" | "ready")
                        })
                })
                .collect::<Vec<_>>();
            for (action_index, component_id) in components.into_iter().enumerate() {
                let reviewed_actions = reviewed_external_action_evidence(plan, component_id)?;
                if reviewed_actions.is_empty() {
                    return Err(AppError::Transaction(format!(
                        "the reviewed post-install component has no action evidence: {component_id}"
                    )));
                }
                if let Some(stage) = journal.stages.get_mut(9) {
                    stage.evidence.extend(reviewed_actions);
                }
                journal.last_checkpoint = format!("post-install-action-intent:{component_id}");
                persist_journal(&store, &mut journal)?;
                if (options.fail_before_post_install_action && action_index == 0)
                    || options.fail_before_post_install_action_index == Some(action_index)
                {
                    return Err(AppError::Transaction(format!(
                        "fault injected before reviewed post-install action {component_id}"
                    )));
                }
                project_directory.verify_bound_to_path()?;
                let outcome = runner(&project_root, plan, component_id)?;
                project_directory.verify_bound_to_path()?;
                if outcome.component_id != component_id {
                    return Err(AppError::Transaction(format!(
                        "post-install action for {component_id} returned result for {}",
                        outcome.component_id
                    )));
                }
                if !matches!(
                    outcome.state.as_str(),
                    "ready" | "incomplete" | "unsupported_platform"
                ) {
                    return Err(AppError::Transaction(format!(
                        "post-install action returned an invalid state for {}",
                        outcome.component_id
                    )));
                }
                if !effective_plan
                    .optional_workflows
                    .contains_key(&outcome.component_id)
                {
                    return Err(AppError::Transaction(format!(
                        "post-install action returned an unselected component: {}",
                        outcome.component_id
                    )));
                }
                effective_plan
                    .optional_workflows
                    .insert(outcome.component_id.clone(), outcome.state.clone());
                let evidence = redact_secrets(&outcome.evidence, &[])
                    .chars()
                    .take(768)
                    .collect::<String>();
                if let Some(stage) = journal.stages.get_mut(9) {
                    stage.evidence.push(format!(
                        "external-action:{}:{}:{}",
                        outcome.component_id, outcome.state, evidence
                    ));
                }
                if (options.fail_after_post_install_action && action_index == 0)
                    || options.fail_after_post_install_action_index == Some(action_index)
                {
                    return Err(AppError::Transaction(format!(
                        "fault injected after reviewed post-install action {component_id}"
                    )));
                }
                journal.last_checkpoint = format!("post-install-action-complete:{component_id}");
                persist_journal(&store, &mut journal)?;
            }
        }
        stage_complete(
            &mut journal,
            9,
            "post-install checks",
            &store,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            10,
            "readiness report",
            &store,
            options.fail_before_stage,
        )?;
        project_directory.verify_bound_to_path()?;
        let readiness = build_transaction_readiness(&project_root, &effective_plan, &journal)?;
        project_directory.verify_bound_to_path()?;
        let readiness_path = store.file_path(READINESS_FILE);
        store.write_json(READINESS_FILE, &readiness)?;
        if let Some(stage) = journal.stages.get_mut(10) {
            stage.evidence.push(readiness_path.display().to_string());
        }
        stage_complete(
            &mut journal,
            10,
            "readiness report",
            &store,
            options.fail_after_stage,
        )?;
        let blocking_checks = readiness
            .checks
            .iter()
            .filter(|check| check.blocking && check.status == "block")
            .map(|check| match check.message.as_deref() {
                Some(message) => format!("{}: {message}", check.id),
                None => check.id.clone(),
            })
            .collect::<Vec<_>>();
        if !blocking_checks.is_empty() {
            return Err(AppError::Transaction(format!(
                "readiness is blocked; success lock was not written: {}",
                blocking_checks.join(", ")
            )));
        }
        final_live_verification(project_directory, plan, &journal)?;
        project_directory.verify_bound_to_path()?;
        let lock = build_lock(
            &effective_plan,
            prepared_files,
            &journal,
            previous_lock.as_ref(),
            &project_root,
            project_directory,
        )?;
        let lock_bytes = serialized_json_bytes(&lock)?;
        journal.result_lock_exists = Some(true);
        journal.result_lock_sha256 = Some(sha256_bytes(&lock_bytes));
        stage_start(
            &mut journal,
            11,
            "rollback record",
            &store,
            options.fail_before_stage,
        )?;
        // Keep a durable finalization state across the lock write. If the
        // process stops after the lock is committed but before the final
        // journal update, startup can verify the rollback record and finish
        // the journal without replaying file operations.
        journal.state = "finalizing".into();
        journal.recovery = RecoveryState {
            resume_allowed: true,
            rollback_allowed: true,
            discard_staging_allowed: false,
            project_apply_started: true,
            recommended_action: "resume".into(),
        };
        persist_journal(&store, &mut journal)?;
        store.write_json(ROLLBACK_RECORD_FILE, &journal)?;
        journal.rollback_record_sha256 = Some(store.directory.hash_file(ROLLBACK_RECORD_FILE)?);
        persist_journal(&store, &mut journal)?;
        maybe_abort_for_test("after_rollback_record");
        if options.fail_after_stage == Some(11) {
            return Err(AppError::Transaction(
                "fault injected after stage rollback record".into(),
            ));
        }
        project_directory.ensure_dir(".hoi4-mod-setup")?;
        // The lock is the final success artifact. Journal finalization after
        // this point is best-effort: a stale `finalizing` journal is safely
        // reconciled by resume only after the lock and rollback record verify.
        commit_success_lock(project_directory, &journal, &lock_bytes, options)?;
        maybe_abort_for_test("after_lock_write");
        stage_complete(&mut journal, 11, "rollback record", &store, None)?;
        journal.state = "completed".into();
        journal.recovery = RecoveryState {
            resume_allowed: false,
            rollback_allowed: true,
            discard_staging_allowed: false,
            project_apply_started: true,
            recommended_action: "none".into(),
        };
        let _ = persist_journal(&store, &mut journal);
        Ok(lock)
    })();

    match result {
        Ok(lock) => Ok((journal, lock)),
        Err(error) => {
            // Once the success lock has been written, finalization must stay
            // recoverable even if the best-effort closing journal write
            // fails. Downgrading this state to generic `interrupted` would
            // leave a durable success lock that the recovery path refuses to
            // reconcile.
            let lock_committed = journal.state == "finalizing"
                && journal.result_lock_exists == Some(true)
                && journal.result_lock_sha256.is_some()
                && journal.rollback_record_sha256.is_some();
            if !lock_committed {
                journal.state = "interrupted".into();
            }
            journal.updated_at = Utc::now().to_rfc3339();
            normalize_incomplete_recovery(&mut journal);
            let staging_complete = journal
                .stages
                .get(6)
                .is_some_and(|stage| stage.status == "complete");
            if !lock_committed && !journal.recovery.project_apply_started && !staging_complete {
                // Resume replays verified staged bytes. Before staging has
                // completed there is nothing safe to replay, so expose only
                // staging cleanup and require a fresh reviewed transaction.
                journal.recovery.resume_allowed = false;
                journal.recovery.recommended_action = "discard_staging".into();
            } else {
                journal.recovery.recommended_action = if lock_committed {
                    "resume".into()
                } else if journal.recovery.project_apply_started {
                    "rollback".into()
                } else {
                    "resume".into()
                };
            }
            journal.error = Some(JournalError {
                code: "TRANSACTION_FAILED".into(),
                message: error.to_string(),
                stage: journal.last_checkpoint.clone(),
            });
            let _ = persist_journal(&store, &mut journal);
            Err(error)
        }
    }
}

/// Write the success lock without replacing a lock that changed after the
/// predecessor backup. An existing predecessor is quarantined under a name
/// derived from the transaction, verified against the journaled predecessor
/// hash, and released only after the new lock verifies. Finalization and
/// rollback settle that quarantine after an interruption.
fn commit_success_lock(
    project: &RootedDir,
    journal: &TransactionJournal,
    lock_bytes: &[u8],
    options: &TransactionOptions,
) -> Result<(), AppError> {
    let current = live_leaf_hash(project, LOCK_RELATIVE_PATH)?;
    if current.as_deref() != journal.previous_lock_sha256.as_deref() {
        return Err(AppError::Transaction(
            "installation lock changed after its backup; the success lock was not written".into(),
        ));
    }
    let fault = |boundary: QuarantineBoundary| -> Result<(), AppError> {
        if options.fail_at_lock_quarantine == Some(boundary) {
            Err(AppError::Transaction(format!(
                "fault injected at lock quarantine boundary {boundary:?}"
            )))
        } else {
            Ok(())
        }
    };
    let held = mutate_live_leaf(
        project,
        LOCK_RELATIVE_PATH,
        current.as_deref(),
        LiveChange::Bytes(lock_bytes),
        &lock_quarantine_leaf(journal.transaction_id, "commit"),
        None,
        "lock-commit",
        &fault,
        &no_live_barrier,
    )?;
    if project.hash_file(LOCK_RELATIVE_PATH)?
        != journal.result_lock_sha256.as_deref().unwrap_or_default()
    {
        return Err(AppError::Transaction(
            "success lock verification failed after rooted write".into(),
        ));
    }
    if let (Some(quarantine), Some(previous)) = (held, current) {
        fault(QuarantineBoundary::BeforeRelease)?;
        release_quarantine(project, &quarantine, &previous)?;
    }
    Ok(())
}

/// Settle both lock quarantines that this journal may own: the success-lock
/// commit and the rollback restore of the predecessor lock.
fn settle_journal_lock_quarantines(
    project: &RootedDir,
    journal: &TransactionJournal,
) -> Result<(), AppError> {
    let result = match (
        journal.result_lock_exists,
        journal.result_lock_sha256.as_deref(),
    ) {
        (Some(true), Some(hash)) => Some(hash),
        _ => None,
    };
    let previous = journal.previous_lock_sha256.as_deref();
    if previous.is_some() && result.is_some() {
        settle_lock_quarantine(
            project,
            &lock_quarantine_leaf(journal.transaction_id, "commit"),
            previous,
            result,
        )?;
    }
    if result.is_some() {
        settle_lock_quarantine(
            project,
            &lock_quarantine_leaf(journal.transaction_id, "restore"),
            result,
            previous,
        )?;
    }
    Ok(())
}

const JOURNAL_FILE: &str = "journal.json";
const PLAN_FILE: &str = "plan.json";
const READINESS_FILE: &str = "readiness-report.json";
const ROLLBACK_RECORD_FILE: &str = "rollback-record.json";
const CHECKPOINT_LOG_FILE: &str = "operation-checkpoints.jsonl";
const TRANSACTIONS_AREA: &str = "transactions";
const BACKUPS_AREA: &str = "backups";
const STAGING_AREA: &str = "staging";

/// The application-data root retained for one transaction call. The
/// per-transaction directories under it (`transactions/<id>`,
/// `backups/<id>`, and `staging/<id>`) are opened through this handle, and a
/// journal that carries `app_data_identity` requires every one of them to be
/// the directory it recorded, so a directory swapped away between calls is
/// refused instead of followed. Within a call the handles are retained; on
/// Windows they also deny the rename of the directory or any ancestor.
struct AppDataRoot {
    directory: RootedDir,
    /// The path as configured. Journaled backup paths are compared with
    /// paths derived from it, so it is never canonicalized here.
    path: PathBuf,
}

impl AppDataRoot {
    fn open_or_create(path: &Path) -> Result<Self, AppError> {
        Ok(Self {
            directory: RootedDir::open_or_create(path)?,
            path: path.to_path_buf(),
        })
    }

    fn open(path: &Path) -> Result<Self, AppError> {
        if path_has_link_component(path) {
            return Err(AppError::PathSecurity(
                "application data root contains a symlink or junction".into(),
            ));
        }
        Ok(Self {
            directory: RootedDir::open(path)?,
            path: path.to_path_buf(),
        })
    }

    /// Open the root of an existing journal. The journal must sit at
    /// `<root>/transactions/<transaction id>/journal.json`, so the store opened
    /// through the root is the journal the caller read.
    fn for_journal(journal_path: &Path, journal: &TransactionJournal) -> Result<Self, AppError> {
        let transaction_directory = journal_path.parent();
        let area = transaction_directory.and_then(Path::parent);
        let expected_id = journal.transaction_id.to_string();
        if journal_path.file_name().and_then(|name| name.to_str()) != Some(JOURNAL_FILE)
            || transaction_directory
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                != Some(expected_id.as_str())
            || area
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                != Some(TRANSACTIONS_AREA)
        {
            return Err(AppError::PathSecurity(
                "transaction journal path is not bound to its transaction ID".into(),
            ));
        }
        Self::open(&journal_app_root(journal_path)?)
    }

    fn identity(&self) -> Result<String, AppError> {
        self.directory.identity_token()
    }

    fn area_path(&self, area: &str, transaction_id: Uuid) -> PathBuf {
        self.path.join(area).join(transaction_id.to_string())
    }

    /// Open `<area>/<id>`, creating it only when `create` is set and the
    /// journal has not bound it yet. A bound directory is never recreated: a
    /// new directory at the same path would be a different directory.
    fn open_area(
        &self,
        area: &str,
        transaction_id: Uuid,
        create: bool,
        expected: Option<&str>,
    ) -> Result<Option<RootedDir>, AppError> {
        let relative = format!("{area}/{transaction_id}");
        let present = self.directory.is_directory(area)? && self.directory.exists(&relative)?;
        let directory = if present {
            if !self.directory.is_directory(&relative)? {
                return Err(AppError::PathSecurity(format!(
                    "transaction {area} storage is not a directory: {}",
                    self.area_path(area, transaction_id).display()
                )));
            }
            self.directory.open_dir(&relative)?
        } else if expected.is_some() {
            return Err(AppError::PathSecurity(format!(
                "the application data directory bound to this transaction is missing; move it back to {} or review the transaction manually",
                self.area_path(area, transaction_id).display()
            )));
        } else if create {
            self.directory.ensure_dir(&relative)?
        } else {
            return Ok(None);
        };
        verify_app_data_directory(&directory, expected, &self.area_path(area, transaction_id))?;
        Ok(Some(directory))
    }

    fn open_or_create_area(
        &self,
        area: &str,
        transaction_id: Uuid,
        expected: Option<&str>,
    ) -> Result<RootedDir, AppError> {
        self.open_area(area, transaction_id, true, expected)?
            .ok_or_else(|| AppError::PathSecurity("transaction storage was not created".into()))
    }

    /// Open or create the transaction directory. The caller compares it with
    /// the journal it reads through the returned store.
    fn transaction_store(
        &self,
        transaction_id: Uuid,
        create: bool,
    ) -> Result<TransactionStore, AppError> {
        let directory = self
            .open_area(TRANSACTIONS_AREA, transaction_id, create, None)?
            .ok_or_else(|| {
                AppError::Transaction(format!(
                    "transaction storage is missing: {}",
                    self.area_path(TRANSACTIONS_AREA, transaction_id).display()
                ))
            })?;
        Ok(TransactionStore {
            journal_path: self
                .area_path(TRANSACTIONS_AREA, transaction_id)
                .join(JOURNAL_FILE),
            directory,
        })
    }

    /// Read a journal through `store` and require the storage it recorded.
    fn read_bound_journal(&self, store: &TransactionStore) -> Result<TransactionJournal, AppError> {
        let journal = load_journal_in(&store.directory, JOURNAL_FILE)?;
        verify_app_data_binding(&journal, Some(&self.directory), &store.directory)?;
        Ok(journal)
    }

    /// Identity evidence for a transaction whose storage this call creates.
    fn bind_new_storage(&self, store: &TransactionStore) -> Result<AppDataIdentity, AppError> {
        Ok(AppDataIdentity {
            root: self.identity()?,
            transaction: store.directory.identity_token()?,
            backup: None,
            staging: None,
        })
    }
}

/// The retained transaction directory (`transactions/<id>`): the journal,
/// the reviewed plan, the checkpoint log, the readiness report, and the
/// rollback record are read and written only through this handle.
struct TransactionStore {
    directory: RootedDir,
    journal_path: PathBuf,
}

impl TransactionStore {
    fn journal_path(&self) -> &Path {
        &self.journal_path
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.journal_path.with_file_name(name)
    }

    fn write_json<T: Serialize>(&self, name: &str, value: &T) -> Result<(), AppError> {
        self.directory
            .write_atomic(name, &persistable_json_bytes(value)?)
    }

    /// A store over the directory of an arbitrary journal path, for unit
    /// tests of the journal helpers outside the application-data layout.
    #[cfg(test)]
    fn open_journal_directory(journal_path: &Path) -> Result<Self, AppError> {
        assert_eq!(
            journal_path.file_name().and_then(|name| name.to_str()),
            Some(JOURNAL_FILE)
        );
        Ok(Self {
            directory: RootedDir::open(journal_path.parent().unwrap())?,
            journal_path: journal_path.to_path_buf(),
        })
    }
}

fn app_data_drift(path: &Path) -> AppError {
    AppError::PathSecurity(format!(
        "application data directory is no longer the directory bound to this transaction: {}; move the original folder back or review the transaction manually",
        path.display()
    ))
}

fn verify_app_data_directory(
    directory: &RootedDir,
    expected: Option<&str>,
    path: &Path,
) -> Result<(), AppError> {
    match expected {
        Some(expected) if directory.identity_token()? != expected => Err(app_data_drift(path)),
        _ => Ok(()),
    }
}

/// Compare retained application-data handles with the identities the
/// journal recorded when its transaction created them. A journal from before
/// the binding carries none and keeps path-based access.
fn verify_app_data_binding(
    journal: &TransactionJournal,
    root: Option<&RootedDir>,
    transaction: &RootedDir,
) -> Result<(), AppError> {
    let Some(identity) = journal.app_data_identity.as_ref() else {
        return Ok(());
    };
    if let Some(root) = root {
        if root.identity_token()? != identity.root {
            return Err(app_data_drift(Path::new("application data root")));
        }
    }
    if transaction.identity_token()? != identity.transaction {
        return Err(app_data_drift(Path::new(&format!(
            "transactions/{}",
            journal.transaction_id
        ))));
    }
    Ok(())
}

/// The retained backup directory of one journal and the path under which
/// the journal recorded its backups. Recorded paths are still compared with
/// the derived path, and the bytes are read only through the handle.
struct JournalBackups {
    root: PathBuf,
    directory: Option<RootedDir>,
    /// The journal bound a backup directory that is no longer present. Only
    /// a step that needs a backup is refused, so a rollback that restores
    /// nothing from it is not blocked.
    bound_missing: bool,
}

impl JournalBackups {
    fn open(app: &AppDataRoot, journal: &TransactionJournal) -> Result<Self, AppError> {
        let expected = journal
            .app_data_identity
            .as_ref()
            .and_then(|identity| identity.backup.as_deref());
        let relative = format!("{BACKUPS_AREA}/{}", journal.transaction_id);
        let present =
            app.directory.is_directory(BACKUPS_AREA)? && app.directory.exists(&relative)?;
        Ok(Self {
            root: app.area_path(BACKUPS_AREA, journal.transaction_id),
            directory: if present {
                app.open_area(BACKUPS_AREA, journal.transaction_id, false, expected)?
            } else {
                None
            },
            bound_missing: !present && expected.is_some(),
        })
    }

    /// Validate a journaled backup path and return the retained directory
    /// with the leaf to read from it.
    fn resolve<'a>(
        &self,
        recorded: &str,
        leaf: &'a str,
    ) -> Result<(&RootedDir, &'a str), AppError> {
        let expected = self.root.join(leaf);
        let supplied = PathBuf::from(recorded);
        let matches = if cfg!(target_os = "windows") {
            supplied
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected.to_string_lossy())
        } else {
            supplied == expected
        };
        if !matches {
            return Err(AppError::PathSecurity(
                "journal backup path is outside the transaction backup root".into(),
            ));
        }
        let directory = self.directory.as_ref().ok_or_else(|| {
            if self.bound_missing {
                AppError::PathSecurity(format!(
                    "the application data directory bound to this transaction is missing; move it back to {} or review the transaction manually",
                    self.root.display()
                ))
            } else {
                AppError::Transaction(format!(
                    "transaction backup directory is missing: {}",
                    self.root.display()
                ))
            }
        })?;
        Ok((directory, leaf))
    }
}

/// Open a journal's `backups/<id>` or `staging/<id>` directory through the
/// retained application-data root, creating it the first time. A journal
/// that already bound the directory requires that same directory; otherwise
/// the directory opened here is bound now and the caller persists the
/// journal before using it. Journals from before the binding stay unbound.
fn open_journal_area(
    app: &AppDataRoot,
    journal: &mut TransactionJournal,
    area: &str,
    transaction_id: Uuid,
) -> Result<RootedDir, AppError> {
    let expected = journal.app_data_identity.as_ref().and_then(|identity| {
        if area == BACKUPS_AREA {
            identity.backup.clone()
        } else {
            identity.staging.clone()
        }
    });
    let directory = app.open_or_create_area(area, transaction_id, expected.as_deref())?;
    if let Some(identity) = journal.app_data_identity.as_mut() {
        let bound = if area == BACKUPS_AREA {
            &mut identity.backup
        } else {
            &mut identity.staging
        };
        if bound.is_none() {
            *bound = Some(directory.identity_token()?);
        }
    }
    Ok(directory)
}

fn persist_journal(
    store: &TransactionStore,
    journal: &mut TransactionJournal,
) -> Result<(), AppError> {
    journal.updated_at = Utc::now().to_rfc3339();
    // A snapshot covers every checkpoint appended before it. Recording the
    // current sequence (zero before the first checkpoint) lets replay skip
    // exactly those records without comparing wall-clock times.
    journal.checkpoint_sequence.get_or_insert(0);
    sanitize_journal_error(journal);
    store.write_json(JOURNAL_FILE, journal)
}

fn sanitize_journal_error(journal: &mut TransactionJournal) {
    let Some(error) = journal.error.as_mut() else {
        return;
    };
    let redacted = redact_secrets(&error.message, &[]);
    if redacted.len() <= JOURNAL_ERROR_MESSAGE_MAX_BYTES {
        error.message = redacted;
        return;
    }
    let mut end = JOURNAL_ERROR_MESSAGE_MAX_BYTES.saturating_sub(3);
    while end > 0 && !redacted.is_char_boundary(end) {
        end -= 1;
    }
    error.message = format!("{}...", &redacted[..end]);
}

#[cfg(test)]
fn operation_checkpoint_root(journal_path: &Path) -> Result<PathBuf, AppError> {
    let parent = journal_path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("transaction journal has no parent".into()))?;
    Ok(parent.join(CHECKPOINT_LOG_FILE))
}

#[cfg(test)]
fn persist_operation_checkpoint(
    store: &TransactionStore,
    journal: &mut TransactionJournal,
    operation_index: usize,
) -> Result<(), AppError> {
    append_operation_checkpoints(store, journal, &[operation_index], true)
}

fn append_operation_checkpoint(
    store: &TransactionStore,
    journal: &mut TransactionJournal,
    operation_index: usize,
) -> Result<(), AppError> {
    append_operation_checkpoints(store, journal, &[operation_index], false)
}

fn persist_operation_checkpoint_batch(
    store: &TransactionStore,
    journal: &mut TransactionJournal,
    operation_indices: &[usize],
) -> Result<(), AppError> {
    append_operation_checkpoints(store, journal, operation_indices, true)
}

fn append_operation_checkpoints(
    store: &TransactionStore,
    journal: &mut TransactionJournal,
    operation_indices: &[usize],
    sync: bool,
) -> Result<(), AppError> {
    if operation_indices.is_empty() {
        return Ok(());
    }
    let checkpoint_directory = &store.directory;
    if !checkpoint_directory.exists(CHECKPOINT_LOG_FILE)? {
        // Atomically create the append log once so its directory entry is
        // durable before an apply-intent checkpoint can guard a live change.
        checkpoint_directory.write_atomic(CHECKPOINT_LOG_FILE, b"")?;
    }
    journal.updated_at = Utc::now().to_rfc3339();
    let mut bytes = Vec::new();
    for operation_index in operation_indices {
        let operation = journal
            .operations
            .get(*operation_index)
            .cloned()
            .ok_or_else(|| AppError::Transaction("operation checkpoint index is invalid".into()))?;
        let sequence = journal.checkpoint_sequence.unwrap_or(0) + 1;
        journal.checkpoint_sequence = Some(sequence);
        let checkpoint = OperationCheckpoint {
            schema_version: OPERATION_CHECKPOINT_SCHEMA.into(),
            transaction_id: journal.transaction_id,
            operation_index: *operation_index,
            operation,
            journal_state: journal.state.clone(),
            last_checkpoint: journal.last_checkpoint.clone(),
            recovery: journal.recovery.clone(),
            updated_at: journal.updated_at.clone(),
            sequence: Some(sequence),
        };
        let value = serde_json::to_value(&checkpoint)?;
        crate::security::reject_secret_like_keys(&value)?;
        let mut record = serde_json::to_vec(&value)?;
        if record.len() > 64 * 1024 {
            return Err(AppError::Transaction(
                "operation checkpoint exceeds its bounded size".into(),
            ));
        }
        record.push(b'\n');
        bytes.extend(record);
    }

    checkpoint_directory.append_file(CHECKPOINT_LOG_FILE, &bytes, sync)
}

fn clear_operation_checkpoints(store: &TransactionStore) -> Result<(), AppError> {
    let directory = &store.directory;
    if !directory.exists(CHECKPOINT_LOG_FILE)? {
        return Ok(());
    }
    if !directory.is_regular_file(CHECKPOINT_LOG_FILE)? {
        return Err(AppError::PathSecurity(
            "operation checkpoint storage is not a regular file".into(),
        ));
    }
    directory.remove_file(CHECKPOINT_LOG_FILE)
}

fn compact_operation_checkpoints(
    store: &TransactionStore,
    journal: &mut TransactionJournal,
) -> Result<(), AppError> {
    persist_journal(store, journal)?;
    clear_operation_checkpoints(store)
}

fn replay_operation_checkpoints(
    directory: &RootedDir,
    journal: &mut TransactionJournal,
) -> Result<(), AppError> {
    let name = CHECKPOINT_LOG_FILE;
    if !directory.exists(name)? {
        return Ok(());
    }
    if !directory.is_regular_file(name)? {
        return Err(AppError::PathSecurity(
            "operation checkpoint storage is not a regular file".into(),
        ));
    }
    let snapshot_updated_at = journal.updated_at.clone();
    let snapshot_sequence = journal.checkpoint_sequence;
    let bytes = directory.read_file(name)?;
    if bytes.len() > OPERATION_CHECKPOINT_MAX_BYTES {
        return Err(AppError::Transaction(
            "operation checkpoint log exceeds its bounded size".into(),
        ));
    }
    let has_complete_tail = bytes.is_empty() || bytes.ends_with(b"\n");
    let mut records = 0usize;
    for (line_index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        if line.len() > 64 * 1024 {
            return Err(AppError::Transaction(
                "operation checkpoint exceeds its bounded size".into(),
            ));
        }
        records += 1;
        if records > OPERATION_CHECKPOINT_MAX_RECORDS {
            return Err(AppError::Transaction(
                "operation checkpoint log exceeds its bounded record count".into(),
            ));
        }
        let checkpoint: OperationCheckpoint = match serde_json::from_slice(line) {
            Ok(checkpoint) => checkpoint,
            Err(_)
                if !has_complete_tail
                    && line_index == bytes.split(|byte| *byte == b'\n').count() - 1 =>
            {
                break;
            }
            Err(error) => {
                return Err(AppError::Transaction(format!(
                    "invalid operation checkpoint: {error}"
                )))
            }
        };
        if !matches!(
            checkpoint.schema_version.as_str(),
            OPERATION_CHECKPOINT_SCHEMA | LEGACY_OPERATION_CHECKPOINT_SCHEMA
        ) || checkpoint.transaction_id != journal.transaction_id
            || checkpoint.operation_index >= journal.operations.len()
            || checkpoint.operation.id != journal.operations[checkpoint.operation_index].id
        {
            return Err(AppError::Transaction(
                "operation checkpoint does not match its transaction".into(),
            ));
        }
        // Sequenced records are ordered by their monotonic position, so a
        // wall-clock step backwards cannot hide a durable intent. A snapshot
        // always carries a sequence once this code wrote it, so an unsequenced
        // record beside it is older. Only a journal and log written entirely
        // before sequences existed fall back to the timestamp comparison.
        let newer = match (checkpoint.sequence, snapshot_sequence) {
            (Some(record), Some(snapshot)) => record > snapshot,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => checkpoint.updated_at > snapshot_updated_at,
        };
        if !newer {
            continue;
        }
        journal.operations[checkpoint.operation_index] = checkpoint.operation;
        let advances = match checkpoint.sequence {
            Some(record) => journal
                .checkpoint_sequence
                .is_none_or(|current| record > current),
            None => checkpoint.updated_at > journal.updated_at,
        };
        if advances {
            journal.state = checkpoint.journal_state;
            journal.last_checkpoint = checkpoint.last_checkpoint;
            journal.recovery = checkpoint.recovery;
            journal.updated_at = checkpoint.updated_at;
            if checkpoint.sequence.is_some() {
                journal.checkpoint_sequence = checkpoint.sequence;
            }
        }
    }
    Ok(())
}

/// Return the exact bytes written by `atomic_write_json`. Keeping the lock
/// hash tied to this representation lets finalization recovery reject a
/// substituted but otherwise parseable success lock.
fn serialized_json_bytes<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, AppError> {
    let json = serde_json::to_value(value)?;
    crate::security::reject_secret_like_keys(&json)?;
    Ok(serde_json::to_vec_pretty(&json)?)
}

fn operation_destination(
    project_root: &Path,
    operation: &PlanOperation,
) -> Result<PathBuf, AppError> {
    if operation.external {
        validate_external_destination(&operation.destination)
    } else {
        safe_join(project_root, &operation.destination)
    }
}

fn locked_file_destination(project_root: &Path, file: &LockedFile) -> Result<PathBuf, AppError> {
    if file.external {
        validate_external_destination(&file.path)
    } else {
        safe_join(project_root, &file.path)
    }
}

fn location_scope_for_file(file: &LockedFile) -> String {
    file.location_scope.clone().unwrap_or_else(|| {
        if file.external {
            "external_launcher".into()
        } else if file.path.starts_with(".hoi4-mod-setup/") {
            "application_data".into()
        } else {
            "project".into()
        }
    })
}

/// Path of an operation's staged bytes relative to `staging/<id>`.
fn staging_relative(operation: &PlanOperation) -> Result<String, AppError> {
    if operation.external {
        normalize_relative_path(&format!("external/{}", operation.id))
    } else {
        normalize_relative_path(&operation.destination)
    }
}

fn stage_start(
    journal: &mut TransactionJournal,
    index: usize,
    id: &str,
    store: &TransactionStore,
    fail_before: Option<usize>,
) -> Result<(), AppError> {
    if fail_before == Some(index) {
        return Err(AppError::Transaction(format!(
            "fault injected before stage {id}"
        )));
    }
    journal.state = match index {
        0 => "preflight",
        1 => "resolving",
        2 => "downloading",
        3 => "verifying",
        4 => "reviewed",
        5 => "backing_up",
        6 => "staging",
        7 => "validating",
        8 => "applying",
        9 => "post_check",
        10 => "reporting",
        11 => "reporting",
        _ => "failed",
    }
    .into();
    if let Some(stage) = journal.stages.get_mut(index) {
        stage.status = "active".into();
        stage.started_at = Some(Utc::now().to_rfc3339());
    }
    journal.last_checkpoint = id.into();
    persist_journal(store, journal)
}

fn stage_complete(
    journal: &mut TransactionJournal,
    index: usize,
    id: &str,
    store: &TransactionStore,
    fail_after: Option<usize>,
) -> Result<(), AppError> {
    if fail_after == Some(index) {
        return Err(AppError::Transaction(format!(
            "fault injected after stage {id}"
        )));
    }
    if let Some(stage) = journal.stages.get_mut(index) {
        stage.status = "complete".into();
        stage.completed_at = Some(Utc::now().to_rfc3339());
    }
    persist_journal(store, journal)
}

fn add_stage_evidence(
    journal: &mut TransactionJournal,
    index: usize,
    evidence: Vec<String>,
    store: &TransactionStore,
) -> Result<(), AppError> {
    if evidence.len() > 4096 || evidence.iter().any(|item| item.len() > 1024) {
        return Err(AppError::Transaction(
            "transaction stage evidence exceeds the bounded limit".into(),
        ));
    }
    let stage = journal
        .stages
        .get_mut(index)
        .ok_or_else(|| AppError::Transaction("transaction stage index is invalid".into()))?;
    stage.evidence = evidence;
    persist_journal(store, journal)
}

/// Revalidate the exact bytes handed from the read-only plan builder to the
/// mutation transaction. Planning may have fetched remote content before the
/// user reviewed the dry run; the journaled transaction must still bind every
/// selected payload to its operation and checksum before it creates backups.
fn validate_prepared_files(
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
) -> Result<Vec<String>, AppError> {
    if prepared_files.len() > 4096 {
        return Err(AppError::Transaction(
            "prepared transaction payload exceeds the bounded file limit".into(),
        ));
    }
    let operations: HashMap<&str, &PlanOperation> = plan
        .operations
        .iter()
        .map(|operation| (operation.id.as_str(), operation))
        .collect();
    let mut ledger = HashMap::new();
    for entry in &plan.download_ledger {
        if entry.operation_id.is_empty()
            || ledger.insert(entry.operation_id.as_str(), entry).is_some()
        {
            return Err(AppError::Transaction(
                "source download ledger has an empty or duplicate operation binding".into(),
            ));
        }
        let operation = operations.get(entry.operation_id.as_str()).ok_or_else(|| {
            AppError::Transaction(format!(
                "source download ledger references an unknown operation: {}",
                entry.operation_id
            ))
        })?;
        let source_path = operation.source_path.as_deref().ok_or_else(|| {
            AppError::Transaction(format!(
                "source download ledger operation has no source path: {}",
                entry.operation_id
            ))
        })?;
        if source_path.starts_with("generated:")
            || entry.component_id != operation.component_id
            || entry.source_path != source_path
            || entry.destination != operation.destination
            || entry.source_revision != plan.source.resolved_revision
            || entry.manifest_sha256 != plan.source.manifest_sha256
            || operation.source_sha256.as_deref() != Some(entry.sha256.as_str())
            || operation.source_size != Some(entry.size)
            || operation.ownership != Some(entry.ownership)
            || operation.platform != Some(entry.platform)
            || operation.executable != entry.executable
        {
            return Err(AppError::Transaction(format!(
                "source download ledger does not match reviewed operation {}",
                entry.operation_id
            )));
        }
        crate::source::validate_sha256(&entry.sha256)?;
        crate::source::validate_sha256(&entry.manifest_sha256)?;
    }
    let mut seen = std::collections::HashSet::new();
    let mut evidence = Vec::with_capacity(prepared_files.len());
    for prepared in prepared_files {
        if !seen.insert(prepared.operation_id.as_str()) {
            return Err(AppError::Transaction(format!(
                "prepared transaction payload has a duplicate operation: {}",
                prepared.operation_id
            )));
        }
        let operation = operations
            .get(prepared.operation_id.as_str())
            .ok_or_else(|| {
                AppError::Transaction(format!(
                    "prepared transaction payload is not bound to an operation: {}",
                    prepared.operation_id
                ))
            })?;
        let destinations_match = if operation.external {
            validate_external_destination(&prepared.destination)
                .map_err(|error| AppError::Transaction(error.to_string()))?
                == validate_external_destination(&operation.destination)
                    .map_err(|error| AppError::Transaction(error.to_string()))?
        } else {
            canonical_relative_key(&prepared.destination)
                .map_err(|error| AppError::Transaction(error.to_string()))?
                == canonical_relative_key(&operation.destination)
                    .map_err(|error| AppError::Transaction(error.to_string()))?
        };
        if !destinations_match {
            return Err(AppError::Transaction(format!(
                "prepared destination is not bound to operation {}",
                operation.destination
            )));
        }
        crate::source::validate_sha256(&prepared.expected_sha256)?;
        let actual = sha256_bytes(&prepared.bytes);
        if actual != prepared.expected_sha256 {
            return Err(AppError::Transaction(format!(
                "prepared checksum mismatch: {}",
                operation.destination
            )));
        }
        if matches!(operation.action, OperationAction::DeleteManaged) {
            return Err(AppError::Transaction(format!(
                "delete operation has prepared content: {}",
                operation.destination
            )));
        }
        if operation.action != OperationAction::Skip {
            let expected = operation
                .result_sha256
                .as_deref()
                .or(operation.source_sha256.as_deref())
                .ok_or_else(|| {
                    AppError::Transaction(format!(
                        "mutating operation has no checksum expectation: {}",
                        operation.destination
                    ))
                })?;
            if expected != actual {
                return Err(AppError::Transaction(format!(
                    "prepared content is not the reviewed operation result: {}",
                    operation.destination
                )));
            }
        } else if operation.source_sha256.is_some()
            && !matches!(operation.resolution.as_deref(), Some("keep" | "skip"))
        {
            let expected = operation
                .result_sha256
                .as_deref()
                .or(operation.source_sha256.as_deref())
                .ok_or_else(|| {
                    AppError::Transaction(format!(
                        "skipped operation has no checksum expectation: {}",
                        operation.destination
                    ))
                })?;
            if expected != actual {
                return Err(AppError::Transaction(format!(
                    "prepared content is not the reviewed skipped baseline: {}",
                    operation.destination
                )));
            }
        }
        let requires_source_ledger = operation
            .source_path
            .as_deref()
            .is_some_and(|path| !path.starts_with("generated:"))
            && operation.source_sha256.is_some()
            && operation.action != OperationAction::DeleteManaged
            && !(operation.action == OperationAction::Skip
                && matches!(operation.resolution.as_deref(), Some("keep" | "skip")));
        if requires_source_ledger && !ledger.contains_key(operation.id.as_str()) {
            return Err(AppError::Transaction(format!(
                "remote operation has no revision-bound download evidence: {}",
                operation.destination
            )));
        }
        if operation
            .source_sha256
            .as_deref()
            .is_some_and(|expected| expected == actual)
            && operation
                .source_size
                .is_some_and(|expected| expected != prepared.bytes.len() as u64)
        {
            return Err(AppError::Transaction(format!(
                "prepared source size mismatch: {}",
                operation.destination
            )));
        }
        evidence.push(if let Some(entry) = ledger.get(operation.id.as_str()) {
            format!(
                "{}={actual};source={}:{}:{}",
                operation.destination, entry.source_revision, entry.source_path, entry.sha256
            )
        } else {
            format!("{}={actual}", operation.destination)
        });
    }
    for operation in &plan.operations {
        if operation.action != OperationAction::Skip
            && operation.action != OperationAction::DeleteManaged
            && !seen.contains(operation.id.as_str())
        {
            return Err(AppError::Transaction(format!(
                "mutating operation has no prepared content: {}",
                operation.destination
            )));
        }
        if operation.action == OperationAction::Skip
            && operation.source_sha256.is_some()
            && !matches!(operation.resolution.as_deref(), Some("keep" | "skip"))
            && !seen.contains(operation.id.as_str())
        {
            return Err(AppError::Transaction(format!(
                "skipped operation has no verified incoming content: {}",
                operation.destination
            )));
        }
        let requires_source_ledger = operation
            .source_path
            .as_deref()
            .is_some_and(|path| !path.starts_with("generated:"))
            && operation.source_sha256.is_some()
            && operation.action != OperationAction::DeleteManaged
            && !(operation.action == OperationAction::Skip
                && matches!(operation.resolution.as_deref(), Some("keep" | "skip")));
        if requires_source_ledger && !ledger.contains_key(operation.id.as_str()) {
            return Err(AppError::Transaction(format!(
                "remote operation has no revision-bound download evidence: {}",
                operation.destination
            )));
        }
    }
    evidence.sort();
    Ok(evidence)
}

fn flatten_destination(path: &str) -> bool {
    path.replace('\\', "/")
        .starts_with(&format!("{}/", crate::flatten::FLAT_DESTINATION_ROOT))
}

fn flatten_input_uses_incoming(operation: &PlanOperation) -> bool {
    if matches!(
        operation.resolution.as_deref(),
        Some(
            "review_required"
                | "reverse_merge_required"
                | "user_owned_review"
                | "keep_user_modification"
                | "obsolete_review"
        )
    ) {
        return false;
    }
    !(matches!(
        (operation.action, operation.resolution.as_deref()),
        (OperationAction::Skip, Some("keep" | "skip"))
    ) || (operation.local_state == LocalState::Modified && operation.resolution.is_none()))
}

/// Rebuild the optional flat Chat view from the reviewed, non-flat inputs at
/// the mutation boundary. This prevents a tampered plan or changed user extra
/// from bypassing the flattener's link, secret, collision, and size checks.
pub(crate) fn validate_flatten_transaction_inputs(
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    project_root: &Path,
) -> Result<(), AppError> {
    if !plan.flatten_chat_sources {
        return Ok(());
    }
    let operations = plan
        .operations
        .iter()
        .map(|operation| (operation.id.as_str(), operation))
        .collect::<HashMap<_, _>>();
    let mut accepted_prepared = Vec::new();
    for file in prepared_files
        .iter()
        .filter(|file| !flatten_destination(&file.destination))
    {
        let operation = operations.get(file.operation_id.as_str()).copied();
        if operation.is_none_or(flatten_input_uses_incoming) {
            accepted_prepared.push(file.clone());
            continue;
        }
        let Some(operation) = operation else {
            continue;
        };
        let kept_local = matches!(
            operation.resolution.as_deref(),
            Some("keep" | "keep_user_modification")
        );
        let normalized = file.destination.replace('\\', "/");
        let eligible =
            normalized.starts_with(".agents/skills/") || normalized.starts_with(".codex/agents/");
        if kept_local && eligible {
            let bytes = crate::flatten::read_regular_file_no_follow_under_root(
                project_root,
                &file.destination,
            )?;
            accepted_prepared.push(PreparedFile {
                operation_id: file.operation_id.clone(),
                destination: file.destination.clone(),
                expected_sha256: sha256_bytes(&bytes),
                bytes,
            });
        }
    }
    let accepted_generated = plan
        .generated_artifacts
        .iter()
        .filter(|artifact| !flatten_destination(&artifact.destination))
        .filter(|artifact| {
            let source_path = format!("generated:{}", artifact.destination);
            plan.operations
                .iter()
                .find(|operation| operation.source_path.as_deref() == Some(source_path.as_str()))
                .is_none_or(flatten_input_uses_incoming)
        })
        .cloned()
        .collect::<Vec<_>>();
    let rebuilt =
        crate::flatten::build_artifacts(&accepted_prepared, &accepted_generated, project_root)?;
    let expected = plan
        .generated_artifacts
        .iter()
        .filter(|artifact| flatten_destination(&artifact.destination))
        .filter(|artifact| {
            let source_path = format!("generated:{}", artifact.destination);
            plan.operations
                .iter()
                .find(|operation| operation.source_path.as_deref() == Some(source_path.as_str()))
                .is_none_or(flatten_input_uses_incoming)
        })
        .map(|artifact| {
            (
                artifact.destination.clone(),
                (artifact.expected_sha256.clone(), artifact.content.clone()),
            )
        })
        .collect::<HashMap<_, _>>();
    // A reviewed keep/skip decision for an already-existing flat output is a
    // deliberate no-op. The flattener can still derive the incoming version
    // of that same destination from the accepted source files, but that
    // derived value is not going to be applied. Compare only outputs whose
    // generated operation still consumes incoming bytes; otherwise a valid
    // keep decision would look like a tampered plan at the mutation boundary.
    let incoming_flat_destinations = expected
        .keys()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let rebuilt_map = rebuilt
        .iter()
        .filter(|artifact| incoming_flat_destinations.contains(&artifact.destination))
        .map(|artifact| {
            (
                artifact.destination.clone(),
                (artifact.expected_sha256.clone(), artifact.content.clone()),
            )
        })
        .collect::<HashMap<_, _>>();
    if expected != rebuilt_map {
        return Err(AppError::Transaction(
            "flattened Chat source inputs changed after review; rebuild the plan".into(),
        ));
    }
    Ok(())
}

fn copy_backup_from_root(
    source_root: &RootedDir,
    source_relative: &str,
    backup_root: &RootedDir,
    backup_relative: &str,
    expected_sha256: &str,
) -> Result<Option<bool>, AppError> {
    if backup_root.exists(backup_relative)? {
        if !backup_root.is_regular_file(backup_relative)?
            || backup_root.hash_file(backup_relative)? != expected_sha256
        {
            return Err(AppError::Transaction(
                "existing backup does not match the reviewed source file".into(),
            ));
        }
    } else if !source_root.copy_file_atomic_noreplace_to(
        source_relative,
        backup_root,
        backup_relative,
    )? {
        return Err(AppError::Transaction(
            "backup name was taken during backup; refusing to replace it".into(),
        ));
    }
    if backup_root.hash_file(backup_relative)? != expected_sha256 {
        return Err(AppError::Transaction(
            "backup verification failed after rooted copy".into(),
        ));
    }
    #[cfg(unix)]
    {
        let source_executable = source_root.observed_executable(source_relative)?;
        if backup_root.observed_executable(backup_relative)? != source_executable {
            return Err(AppError::Transaction(
                "backup executable metadata verification failed".into(),
            ));
        }
        Ok(source_executable)
    }
    #[cfg(not(unix))]
    {
        Ok(None)
    }
}

fn backup_existing(
    project_root: &Path,
    plan: &InstallationPlan,
    backup_root: &Path,
    backup_directory: &RootedDir,
    project_directory: Option<&RootedDir>,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    if path_has_link_component(backup_root) {
        return Err(AppError::PathSecurity(
            "backup root contains a symlink or junction".into(),
        ));
    }
    backup_directory.verify_bound_to_path()?;
    let mut checkpointed = 0usize;
    for (operation_index, operation) in plan.operations.iter().enumerate() {
        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) {
            continue;
        }
        let backup_leaf = format!("{}.bak", operation.id);
        let backup = backup_root.join(&backup_leaf);
        let (source_hash, before_executable) = if operation.external {
            let destination = operation_destination(project_root, operation)?;
            let parent = destination.parent().ok_or_else(|| {
                AppError::PathSecurity("external backup destination has no parent".into())
            })?;
            let source_leaf = destination
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    AppError::PathSecurity("external backup destination name is invalid".into())
                })?;
            let source_root = RootedDir::open(parent)?;
            // A reviewed plan carries the parent's identity into the journal,
            // and a resumed replay carries the identity of the interrupted
            // run; either way this must still be the same directory. Only a
            // plan without one binds the parent here, the first time the
            // transaction opens it.
            let record_index = journal
                .operations
                .iter()
                .position(|record| record.id == operation.id)
                .ok_or_else(|| AppError::Transaction("journal operation missing".into()))?;
            let bound = journal.operations[record_index]
                .external_parent_identity
                .clone();
            verify_external_parent_identity(
                &source_root,
                bound.as_deref(),
                &operation.destination,
            )?;
            if bound.is_none() {
                journal.operations[record_index].external_parent_identity =
                    Some(source_root.identity_token()?);
                journal.last_checkpoint = format!("bind-external-parent-{}", operation.id);
                append_operation_checkpoint(store, journal, record_index)?;
            }
            let exists = source_root.exists(source_leaf)?;
            let is_file = source_root.is_regular_file(source_leaf)?;
            if exists && !is_file {
                return Err(AppError::PathSecurity(format!(
                    "external backup source is not a regular file: {}",
                    operation.destination
                )));
            }
            if !is_file {
                continue;
            }
            let hash = source_root.hash_file(source_leaf)?;
            if operation.local_sha256.as_deref() != Some(hash.as_str()) {
                return Err(AppError::Transaction(format!(
                    "live precondition changed before backup: {}",
                    operation.destination
                )));
            }
            let executable = copy_backup_from_root(
                &source_root,
                source_leaf,
                backup_directory,
                &backup_leaf,
                &hash,
            )?;
            (hash, executable)
        } else {
            let Some(source_root) = project_directory.as_ref() else {
                continue;
            };
            let exists = source_root.exists(&operation.destination)?;
            let is_file = source_root.is_regular_file(&operation.destination)?;
            if exists && !is_file {
                return Err(AppError::Transaction(format!(
                    "directory or special-file replacement requires an explicit plan: {}",
                    operation.destination
                )));
            }
            if !is_file {
                continue;
            }
            let hash = source_root.hash_file(&operation.destination)?;
            if operation.local_sha256.as_deref() != Some(hash.as_str()) {
                return Err(AppError::Transaction(format!(
                    "live precondition changed before backup: {}",
                    operation.destination
                )));
            }
            let executable = copy_backup_from_root(
                source_root,
                &operation.destination,
                backup_directory,
                &backup_leaf,
                &hash,
            )?;
            (hash, executable)
        };
        if path_has_link_component(&backup) {
            return Err(AppError::PathSecurity(format!(
                "backup path contains a symlink or junction: {}",
                backup.display()
            )));
        }
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.before_executable = before_executable;
            record.backup_path = Some(backup.display().to_string());
            record.backup_sha256 = Some(source_hash);
            journal.last_checkpoint = format!("backup-file-{}", operation.id);
            append_operation_checkpoint(store, journal, operation_index)?;
            checkpointed += 1;
            if checkpointed % OPERATION_CHECKPOINT_BATCH == 0 {
                compact_operation_checkpoints(store, journal)?;
            }
        }
    }
    Ok(())
}

fn stage_files(
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    staging: &RootedDir,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    let prepared: HashMap<&str, &PreparedFile> = prepared_files
        .iter()
        .map(|file| (file.operation_id.as_str(), file))
        .collect();
    let mut checkpointed = 0usize;
    for (operation_index, operation) in plan.operations.iter().enumerate() {
        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External | OperationAction::DeleteManaged
        ) {
            continue;
        }
        let file = prepared.get(operation.id.as_str()).ok_or_else(|| {
            AppError::Transaction(format!("prepared bytes missing for {}", operation.id))
        })?;
        let hash = sha256_bytes(&file.bytes);
        let expected = operation
            .result_sha256
            .as_ref()
            .or(operation.source_sha256.as_ref());
        if hash != file.expected_sha256 || expected.is_some_and(|value| value != &hash) {
            return Err(AppError::Source(format!(
                "prepared checksum mismatch for {}",
                operation.destination
            )));
        }
        // The staged bytes are written through the retained, identity-bound
        // staging handle; no component of the staging path is resolved again.
        let staged = staging_relative(operation)?;
        if staging.exists(&staged)? && !staging.is_regular_file(&staged)? {
            return Err(AppError::PathSecurity(format!(
                "staging destination is not a regular file: {staged}"
            )));
        }
        staging.write_atomic(&staged, &file.bytes)?;
        #[cfg(unix)]
        {
            staging.set_executable(&staged, operation.executable)?;
            if staging.observed_executable(&staged)? != Some(operation.executable) {
                return Err(AppError::Transaction(format!(
                    "staging executable metadata mismatch for {}",
                    operation.destination
                )));
            }
        }
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "staged".into();
            record.staged_sha256 = Some(hash);
        }
        journal.last_checkpoint = format!("stage-file-{}", operation.id);
        append_operation_checkpoint(store, journal, operation_index)?;
        checkpointed += 1;
        if checkpointed % OPERATION_CHECKPOINT_BATCH == 0 {
            compact_operation_checkpoints(store, journal)?;
        }
    }
    Ok(())
}

fn stage_profile_directories(plan: &InstallationPlan, staging: &RootedDir) -> Result<(), AppError> {
    for directory in &plan.transaction.directories {
        staging.ensure_dir(&normalize_relative_path(directory)?)?;
    }
    Ok(())
}

/// Validate staged output read through the retained staging handle. Each
/// file is read once, and the bytes that are hashed are the bytes that are
/// validated.
fn validate_staging(
    project_root: &Path,
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    staging: &RootedDir,
) -> Result<(), AppError> {
    for directory in &plan.transaction.directories {
        if !staging.is_directory(&normalize_relative_path(directory)?)? {
            return Err(AppError::PathSecurity(format!(
                "staged profile path is not a regular directory: {directory}"
            )));
        }
    }
    let prepared: HashMap<&str, &PreparedFile> = prepared_files
        .iter()
        .map(|file| (file.operation_id.as_str(), file))
        .collect();
    for operation in &plan.operations {
        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External | OperationAction::DeleteManaged
        ) {
            continue;
        }
        let staged = staging_relative(operation)?;
        if !staging.is_regular_file(&staged)? {
            return Err(AppError::PathSecurity(format!(
                "staging destination is not a regular file: {staged}"
            )));
        }
        let bytes = staging.read_file(&staged)?;
        let expected = prepared
            .get(operation.id.as_str())
            .map(|file| file.expected_sha256.as_str())
            .unwrap_or("");
        if sha256_bytes(&bytes) != expected {
            return Err(AppError::Transaction(format!(
                "staging hash mismatch for {}",
                operation.destination
            )));
        }
        #[cfg(unix)]
        if staging.observed_executable(&staged)? != Some(operation.executable) {
            return Err(AppError::Transaction(format!(
                "staging executable metadata changed for {}",
                operation.destination
            )));
        }
        validate_managed_bytes(project_root, operation, &bytes)?;
    }
    Ok(())
}

fn validate_managed_bytes(
    project_root: &Path,
    operation: &PlanOperation,
    bytes: &[u8],
) -> Result<(), AppError> {
    let destination = operation.destination.as_str();
    let lower = destination.to_ascii_lowercase().replace('\\', "/");
    if operation.component_id == "project.descriptor" || lower == "descriptor.mod" {
        let descriptor = crate::descriptors::parse_descriptor(bytes).map_err(|error| {
            AppError::Transaction(format!("descriptor validation failed: {error}"))
        })?;
        if !descriptor.fields.contains_key("name")
            || !descriptor.fields.contains_key("supported_version")
        {
            return Err(AppError::Transaction(
                "descriptor validation failed: name or supported_version is missing".into(),
            ));
        }
    } else if operation.component_id == "project.launcher_descriptor"
        || operation.location_scope.as_deref() == Some("external_launcher")
    {
        let descriptor = crate::descriptors::parse_descriptor(bytes).map_err(|error| {
            AppError::Transaction(format!("launcher descriptor validation failed: {error}"))
        })?;
        if !descriptor.fields.contains_key("name") {
            return Err(AppError::Transaction(
                "launcher descriptor validation failed: name is missing".into(),
            ));
        }
        let declared_path = descriptor.fields.get("path").ok_or_else(|| {
            AppError::Transaction("launcher descriptor validation failed: path is missing".into())
        })?;
        if !crate::descriptors::launcher_path_matches_project_root(declared_path, project_root)? {
            return Err(AppError::Transaction(
                "launcher descriptor path does not match the selected project root".into(),
            ));
        }
    } else if lower == "thumbnail.png" {
        crate::descriptors::validate_thumbnail_png(bytes).map_err(|error| {
            AppError::Transaction(format!("thumbnail validation failed: {error}"))
        })?;
    } else if lower.ends_with(".toml") {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| AppError::Transaction(format!("TOML validation failed: {destination}")))?;
        text.parse::<toml::Value>().map_err(|error| {
            AppError::Transaction(format!("TOML validation failed for {destination}: {error}"))
        })?;
    } else if lower.ends_with(".json") {
        let value = serde_json::from_slice::<serde_json::Value>(bytes).map_err(|error| {
            AppError::Transaction(format!("JSON validation failed for {destination}: {error}"))
        })?;
        let schema = if lower.ends_with("/.hoi4-mod-setup/state.json")
            || lower == ".hoi4-mod-setup/state.json"
            || lower.ends_with("/.hoi4-mod-setup/project-state.json")
            || lower == ".hoi4-mod-setup/project-state.json"
        {
            Some(include_str!("../../docs/schemas/project-state.schema.json"))
        } else if lower.ends_with("/.hoi4-mod-setup/install.lock.json")
            || lower == ".hoi4-mod-setup/install.lock.json"
        {
            Some(include_str!(
                "../../docs/schemas/installation-lock.schema.json"
            ))
        } else if lower.ends_with("/.hoi4-mod-setup/readiness-report.json")
            || lower == ".hoi4-mod-setup/readiness-report.json"
        {
            Some(include_str!(
                "../../docs/schemas/readiness-report.schema.json"
            ))
        } else {
            None
        };
        if let Some(schema) = schema {
            let schema_value =
                serde_json::from_str::<serde_json::Value>(schema).map_err(|error| {
                    AppError::Transaction(format!(
                        "checked-in JSON Schema is invalid for {destination}: {error}"
                    ))
                })?;
            let validator = jsonschema::draft202012::new(&schema_value).map_err(|error| {
                AppError::Transaction(format!(
                    "checked-in JSON Schema cannot be compiled for {destination}: {error}"
                ))
            })?;
            validator.validate(&value).map_err(|error| {
                AppError::Transaction(format!(
                    "schema validation failed for {destination} at {}: {error}",
                    error.instance_path()
                ))
            })?;
        }
    } else if lower == "agents.md" {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| AppError::Transaction("AGENTS.md is not valid UTF-8".into()))?;
        if text.trim().is_empty() || text.contains("{{") {
            return Err(AppError::Transaction(
                "AGENTS.md contains no usable rendered instructions".into(),
            ));
        }
    } else if lower.starts_with("paradox_wiki/") {
        if lower.ends_with(".md") || lower.ends_with(".svg") {
            let text = std::str::from_utf8(bytes).map_err(|_| {
                AppError::Transaction(format!("offline wiki text is not UTF-8: {destination}"))
            })?;
            if text.trim().is_empty() {
                return Err(AppError::Transaction(format!(
                    "offline wiki text is empty: {destination}"
                )));
            }
        } else if lower.ends_with(".png") {
            if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(AppError::Transaction(format!(
                    "offline wiki PNG is invalid: {destination}"
                )));
            }
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            if bytes.len() < 4
                || !bytes.starts_with(&[0xff, 0xd8, 0xff])
                || !bytes.ends_with(&[0xff, 0xd9])
            {
                return Err(AppError::Transaction(format!(
                    "offline wiki JPEG is invalid: {destination}"
                )));
            }
        } else {
            return Err(AppError::Transaction(format!(
                "offline wiki file type is not supported: {destination}"
            )));
        }
    }
    Ok(())
}

fn ensure_project_root_for_apply(
    project_root: &Path,
    plan: &InstallationPlan,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    match plan.transaction.project_root_mode {
        ProjectRootMode::Existing => {
            validate_project_root(project_root)?;
            validate_project_root_lifecycle_identity(
                project_root,
                &journal.project_root_lifecycle,
            )?;
            Ok(())
        }
        ProjectRootMode::CreateLeaf => {
            if journal.project_root_lifecycle.mode != ProjectRootMode::CreateLeaf {
                return Err(AppError::Transaction(
                    "journal root lifecycle does not match the reviewed plan".into(),
                ));
            }
            validate_project_root_lifecycle_identity(
                project_root,
                &journal.project_root_lifecycle,
            )?;
            let (validated, exists) = validate_project_root_or_destination(project_root)?;
            if exists {
                return Err(AppError::Transaction(
                    "new project destination appeared after review".into(),
                ));
            }
            if !same_root_path(&validated, project_root) {
                return Err(AppError::PathSecurity(
                    "new project destination changed before apply".into(),
                ));
            }
            mark_project_apply_started(journal);
            journal.project_root_lifecycle.checkpoint = "applying".into();
            journal.project_root_lifecycle.observed_exists = false;
            journal.last_checkpoint = "apply-project-root-intent".into();
            persist_journal(store, journal)?;
            maybe_abort_for_test("before_project_root_create");
            let parent_path = journal
                .project_root_lifecycle
                .canonical_parent
                .as_deref()
                .ok_or_else(|| AppError::PathSecurity("new project root has no parent".into()))?;
            let leaf = journal
                .project_root_lifecycle
                .leaf
                .as_deref()
                .ok_or_else(|| AppError::PathSecurity("new project root has no leaf".into()))?;
            let parent = RootedDir::open(Path::new(parent_path))?;
            if Some(parent.identity_token()?.as_str())
                != journal.project_root_lifecycle.parent_identity.as_deref()
            {
                return Err(AppError::PathSecurity(
                    "project root parent changed before directory creation".into(),
                ));
            }
            let created = parent.create_dir(leaf)?;
            maybe_abort_for_test("after_project_root_create");
            journal.project_root_lifecycle.root_identity = Some(created.identity_token()?);
            journal.project_root_lifecycle.checkpoint = "created".into();
            journal.project_root_lifecycle.created_by_transaction = true;
            journal.project_root_lifecycle.observed_exists = true;
            journal.last_checkpoint = "apply-project-root-created".into();
            persist_journal(store, journal)
        }
    }
}

#[cfg(test)]
fn apply_profile_directories(
    project_root: &Path,
    plan: &InstallationPlan,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    let root = open_bound_project_root(project_root, &journal.project_root_lifecycle)?;
    apply_profile_directories_rooted(&root, plan, journal, store)
}

fn apply_profile_directories_rooted(
    root: &RootedDir,
    plan: &InstallationPlan,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    let mut missing = std::collections::BTreeSet::new();
    for directory in &plan.transaction.directories {
        if crate::security::normalize_relative_path(directory)?.is_empty() {
            return Err(AppError::PathSecurity(
                "profile directory cannot be the project root".into(),
            ));
        }
        let mut current = String::new();
        for component in Path::new(directory).components() {
            let Component::Normal(component) = component else {
                return Err(AppError::PathSecurity(
                    "profile directory contains an invalid path component".into(),
                ));
            };
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(&component.to_string_lossy());
            if root.is_directory(&current)? {
                continue;
            }
            if root.exists(&current)? {
                return Err(AppError::PathSecurity(format!(
                    "profile destination is not a regular directory: {directory}"
                )));
            }
            missing.insert(current.clone());
        }
    }
    journal.created_directories = missing.into_iter().collect();
    journal.last_checkpoint = "apply-profile-directories-intent".into();
    persist_journal(store, journal)?;
    for directory in &plan.transaction.directories {
        root.ensure_dir(directory)?;
        root.open_dir(directory)?;
    }
    journal.last_checkpoint = "apply-profile-directories-created".into();
    persist_journal(store, journal)
}

fn apply_operations(
    project_root: &Path,
    plan: &InstallationPlan,
    staging_directory: &RootedDir,
    project_directory: &RootedDir,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
    options: &TransactionOptions,
) -> Result<(), AppError> {
    mark_project_apply_started(journal);
    persist_journal(store, journal)?;
    staging_directory.verify_bound_to_path()?;
    for (index, operation) in plan.operations.iter().enumerate() {
        if index % OPERATION_INTENT_BATCH == 0 {
            let batch_end = (index + OPERATION_INTENT_BATCH).min(plan.operations.len());
            let intent_indices = (index..batch_end)
                .filter(|candidate| {
                    !matches!(
                        plan.operations[*candidate].action,
                        OperationAction::Skip | OperationAction::External
                    )
                })
                .collect::<Vec<_>>();
            for candidate in &intent_indices {
                if let Some(record) = journal.operations.get_mut(*candidate) {
                    record.status = "applying".into();
                    record.after_sha256 = None;
                    record.after_exists = None;
                }
            }
            journal.last_checkpoint = format!("apply-batch-intent-{index:05}-{batch_end:05}");
            persist_operation_checkpoint_batch(store, journal, &intent_indices)?;
        }
        if options.fail_before_operation == Some(index) {
            return Err(AppError::Transaction(format!(
                "fault injected before operation {}",
                operation.id
            )));
        }
        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) {
            if let Some(record) = journal
                .operations
                .iter_mut()
                .find(|record| record.id == operation.id)
            {
                record.status = "verified".into();
            }
            journal.last_checkpoint = format!("apply-noop-{}", operation.id);
            append_operation_checkpoint(store, journal, index)?;
            if options.fail_after_operation == Some(index) {
                return Err(AppError::Transaction(format!(
                    "fault injected after no-op operation {}",
                    operation.id
                )));
            }
            if (index + 1) % OPERATION_CHECKPOINT_BATCH == 0 || index + 1 == plan.operations.len() {
                compact_operation_checkpoints(store, journal)?;
            }
            continue;
        }
        let destination = if operation.external {
            operation_destination(project_root, operation)?
        } else {
            project_root.join(&operation.destination)
        };
        let deleting = operation.action == OperationAction::DeleteManaged;
        // The precondition, the live mutation, and the post-apply readback
        // all use this one retained target. An external parent must still be
        // the directory bound in the plan or at backup, which always runs
        // earlier in the same run, so an unbound external parent is never
        // opened here.
        let bound_identity = journal
            .operations
            .get(index)
            .filter(|record| record.id == operation.id)
            .and_then(|record| record.external_parent_identity.clone());
        if operation.external && bound_identity.is_none() {
            return Err(AppError::Transaction(format!(
                "external destination parent was not bound before apply: {}",
                operation.destination
            )));
        }
        let target = if operation.external && deleting {
            // A managed delete whose external parent no longer exists
            // changes nothing.
            existing_live_target(
                None,
                true,
                &operation.destination,
                bound_identity.as_deref(),
            )?
        } else {
            Some(live_target(
                (!operation.external).then_some(project_directory),
                operation.external,
                &operation.destination,
                true,
                bound_identity.as_deref(),
            )?)
        };
        let current_hash = match target.as_ref() {
            Some(target) => {
                let exists = target.dir().exists(&target.relative)?;
                let is_file = target.dir().is_regular_file(&target.relative)?;
                if exists && !is_file {
                    return Err(AppError::Transaction(format!(
                        "destination is not a regular file: {}",
                        operation.destination
                    )));
                }
                is_file
                    .then(|| target.dir().hash_file(&target.relative))
                    .transpose()?
            }
            None => None,
        };
        if let Some(expected) = &operation.local_sha256 {
            if current_hash.as_deref() != Some(expected.as_str()) {
                return Err(AppError::Transaction(format!(
                    "local precondition changed for {}",
                    operation.destination
                )));
            }
        } else if current_hash.is_some() {
            return Err(AppError::Transaction(format!(
                "live destination changed after review and has no hash precondition: {}",
                operation.destination
            )));
        }
        let staged_relative = staging_relative(operation)?;
        let staged_hash = if operation.action != OperationAction::DeleteManaged {
            if !staging_directory.is_regular_file(&staged_relative)? {
                return Err(AppError::PathSecurity(format!(
                    "staging destination is not a regular file: {staged_relative}"
                )));
            }
            let staged_hash = staging_directory.hash_file(&staged_relative)?;
            let expected = operation
                .result_sha256
                .as_deref()
                .or(operation.source_sha256.as_deref());
            if expected != Some(staged_hash.as_str()) {
                return Err(AppError::Transaction(format!(
                    "staged content changed before apply: {}",
                    operation.destination
                )));
            }
            Some(staged_hash)
        } else {
            None
        };
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "applying".into();
            record.after_sha256 = None;
            record.after_exists = None;
        }
        journal.last_checkpoint = format!("apply-intent-{}", operation.id);
        // A managed delete whose destination is already absent changes
        // nothing.
        let mutation_target = if deleting && current_hash.is_none() {
            None
        } else {
            target.as_ref()
        };
        let quarantine_fault_at = options
            .fail_at_quarantine
            .and_then(|(fault_index, boundary)| (fault_index == index).then_some(boundary));
        let quarantine_fault = |boundary: QuarantineBoundary| -> Result<(), AppError> {
            if quarantine_fault_at == Some(boundary) {
                Err(AppError::Transaction(format!(
                    "fault injected at quarantine boundary {boundary:?} for operation {}",
                    operation.id
                )))
            } else {
                Ok(())
            }
        };
        let barrier = |point: LiveMutationBarrier| {
            if let Some(hook) = options.live_mutation_barrier {
                hook(&destination, index, point);
            }
        };
        barrier(LiveMutationBarrier::AfterPrecondition);
        let change = if deleting {
            LiveChange::Delete
        } else {
            LiveChange::Copy {
                source: staging_directory,
                source_relative: &staged_relative,
            }
        };
        let quarantine_leaf = quarantine_leaf_name(journal.transaction_id, &operation.id);
        let held_quarantine = match mutation_target {
            Some(target) => mutate_live_leaf(
                target.dir(),
                &target.relative,
                current_hash.as_deref(),
                change,
                &quarantine_leaf,
                Some(QuarantineJournal {
                    journal: &mut *journal,
                    store,
                    index,
                }),
                "apply",
                &quarantine_fault,
                &barrier,
            )?,
            None => None,
        };
        #[cfg(unix)]
        if let (false, Some(target)) = (deleting, mutation_target) {
            target
                .dir()
                .set_executable(&target.relative, operation.executable)?;
        }
        barrier(LiveMutationBarrier::AfterPlacement);
        if options.fail_after_live_mutation == Some(index) {
            return Err(AppError::Transaction(format!(
                "fault injected after live mutation {}",
                operation.id
            )));
        }
        let (after_hash, after_executable, after_exists) = match target.as_ref() {
            Some(target) => {
                let exists = target.dir().exists(&target.relative)?;
                let is_file = target.dir().is_regular_file(&target.relative)?;
                if exists && !is_file {
                    return Err(AppError::PathSecurity(format!(
                        "destination is not a regular file after apply: {}",
                        operation.destination
                    )));
                }
                let (after_hash, after_executable) = target_hash_and_executable(Some(target))?;
                (after_hash, after_executable, exists)
            }
            None => (None, None, false),
        };
        if operation.action != OperationAction::DeleteManaged && after_hash.is_none() {
            return Err(AppError::Transaction(format!(
                "destination missing after apply: {}",
                operation.destination
            )));
        }
        if operation.action != OperationAction::DeleteManaged
            && after_executable.is_some_and(|value| value != operation.executable)
        {
            return Err(AppError::Transaction(format!(
                "destination executable metadata mismatch after apply: {}",
                operation.destination
            )));
        }
        // Only the staged bytes, or an absent destination for a delete, may
        // be recorded as the installed result. Anything else was written by
        // someone else after placement. The operation keeps its `applying`
        // intent without `after_sha256`, so rollback never treats those bytes
        // as installed, and the reviewed original stays in its quarantine.
        let placed_as_reviewed = if deleting {
            !after_exists
        } else {
            after_hash.is_some() && after_hash == staged_hash
        };
        if !placed_as_reviewed {
            let kept = held_quarantine
                .as_deref()
                .map(|quarantine| format!("; the reviewed original is kept at {quarantine}"))
                .unwrap_or_default();
            return Err(AppError::Transaction(format!(
                "local precondition changed during apply: {} changed after the reviewed bytes were placed and the local bytes were kept{kept}",
                operation.destination
            )));
        }
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "verified".into();
            record.staged_sha256 = staged_hash;
            record.after_sha256 = after_hash;
            record.after_exists = Some(after_exists);
            record.after_executable = after_executable;
        }
        journal.last_checkpoint = format!("apply-{}", operation.id);
        if let Some(quarantine) = held_quarantine.as_deref() {
            // The displaced bytes are released only after this result is
            // durable, so every interruption leaves either the quarantine or
            // a synced result checkpoint that explains its absence.
            persist_operation_checkpoint_batch(store, journal, &[index])?;
            quarantine_fault(QuarantineBoundary::BeforeRelease)?;
            let verified = current_hash.as_deref().ok_or_else(|| {
                AppError::Transaction("held quarantine has no verified precondition".into())
            })?;
            let target = target.as_ref().ok_or_else(|| {
                AppError::Transaction("held quarantine has no retained destination".into())
            })?;
            release_quarantine(target.dir(), quarantine, verified)?;
        } else {
            append_operation_checkpoint(store, journal, index)?;
        }
        if options.fail_after_operation == Some(index) {
            return Err(AppError::Transaction(format!(
                "fault injected after operation {}",
                operation.id
            )));
        }
        if (index + 1) % OPERATION_CHECKPOINT_BATCH == 0 || index + 1 == plan.operations.len() {
            compact_operation_checkpoints(store, journal)?;
        }
    }
    Ok(())
}

const QUARANTINE_PREFIX: &str = ".hoi4ms-quarantine-";
const LOCK_RELATIVE_PATH: &str = ".hoi4-mod-setup/install.lock.json";

/// Deterministic same-directory quarantine name for one operation. The
/// transaction ID keeps names from different transactions apart, so an older
/// retained quarantine is never reused or replaced.
fn quarantine_leaf_name(transaction_id: Uuid, operation_id: &str) -> String {
    let readable = !operation_id.is_empty()
        && operation_id.len() <= 64
        && operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    let suffix = if readable {
        operation_id.to_string()
    } else {
        format!("op-{}", &sha256_bytes(operation_id.as_bytes())[..32])
    };
    format!(
        "{QUARANTINE_PREFIX}{}-{suffix}.tmp",
        transaction_id.simple()
    )
}

/// Quarantine names for the success-lock commit and the rollback lock restore.
fn lock_quarantine_leaf(transaction_id: Uuid, purpose: &str) -> String {
    quarantine_leaf_name(transaction_id, &format!("install-lock-{purpose}"))
}

fn quarantine_relative(destination: &str, leaf: &str) -> Result<String, AppError> {
    let normalized = normalize_relative_path(destination)?;
    let relative = match normalized.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{leaf}"),
        None => leaf.to_string(),
    };
    normalize_relative_path(&relative)
}

/// Return the journaled quarantine name only when it is exactly the name this
/// transaction derives for the operation. A journal cannot point recovery at
/// an arbitrary sibling file.
fn journaled_quarantine_leaf(
    transaction_id: Uuid,
    operation: &JournalOperation,
) -> Result<Option<String>, AppError> {
    let Some(leaf) = operation.quarantine_leaf.as_deref() else {
        return Ok(None);
    };
    let expected = quarantine_leaf_name(transaction_id, &operation.id);
    if leaf != expected {
        return Err(AppError::PathSecurity(format!(
            "journal quarantine name is not bound to operation {}",
            operation.id
        )));
    }
    if operation
        .quarantine_sha256
        .as_deref()
        .is_some_and(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(AppError::Transaction(format!(
            "journal quarantine checksum is invalid for operation {}",
            operation.id
        )));
    }
    Ok(Some(expected))
}

/// The forward quarantine name of an operation. A journaled name must be the
/// derived one; when the journal lacks it, for example because the intent
/// checkpoint was not replayed, recovery still probes the derived name, which
/// no other transaction or operation can use.
fn forward_quarantine_leaf(
    transaction_id: Uuid,
    operation: &JournalOperation,
) -> Result<String, AppError> {
    Ok(journaled_quarantine_leaf(transaction_id, operation)?
        .unwrap_or_else(|| quarantine_leaf_name(transaction_id, &operation.id)))
}

enum LiveRoot<'a> {
    Borrowed(&'a RootedDir),
    Owned(RootedDir),
}

/// The retained directory and leaf-relative path of one live destination.
struct LiveTarget<'a> {
    root: LiveRoot<'a>,
    relative: String,
}

impl LiveTarget<'_> {
    fn dir(&self) -> &RootedDir {
        match &self.root {
            LiveRoot::Borrowed(directory) => directory,
            LiveRoot::Owned(directory) => directory,
        }
    }

    fn hash(&self) -> Result<Option<String>, AppError> {
        live_leaf_hash(self.dir(), &self.relative)
    }
}

fn live_leaf_hash(directory: &RootedDir, relative: &str) -> Result<Option<String>, AppError> {
    if !directory.exists(relative)? {
        return Ok(None);
    }
    if !directory.is_regular_file(relative)? {
        return Err(AppError::PathSecurity(format!(
            "live destination is not a regular file: {relative}"
        )));
    }
    directory.hash_file(relative).map(Some)
}

/// Refuse an external destination parent whose identity differs from the
/// one bound into the journal. Path-based checks before and after an
/// operation cannot detect a directory swapped away and back in between; the
/// identity of the retained handle can. A journal operation without a bound
/// identity predates the binding and keeps its earlier path-only behavior.
fn verify_external_parent_identity(
    directory: &RootedDir,
    expected: Option<&str>,
    destination: &str,
) -> Result<(), AppError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if directory.identity_token()? != expected {
        return Err(AppError::PathSecurity(format!(
            "external destination parent is no longer the directory bound to this transaction: {destination}"
        )));
    }
    Ok(())
}

/// Open the parent of a project or external destination. External parents
/// are retained for the duration of the operation, like the project root,
/// and must still have `external_identity` when the journal bound one. A
/// bound parent is never recreated: a new directory at the same path would be
/// a different directory.
fn live_target<'a>(
    project_directory: Option<&'a RootedDir>,
    external: bool,
    destination: &str,
    create_parent: bool,
    external_identity: Option<&str>,
) -> Result<LiveTarget<'a>, AppError> {
    if external {
        let absolute = validate_external_destination(destination)?;
        let parent = absolute.parent().ok_or_else(|| {
            AppError::PathSecurity("external destination has no parent directory".into())
        })?;
        let leaf = absolute
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::PathSecurity("external destination name is invalid".into()))?
            .to_string();
        let directory = if create_parent && external_identity.is_none() {
            RootedDir::open_or_create(parent)?
        } else {
            RootedDir::open(parent)?
        };
        verify_external_parent_identity(&directory, external_identity, destination)?;
        Ok(LiveTarget {
            root: LiveRoot::Owned(directory),
            relative: leaf,
        })
    } else {
        let directory = project_directory.ok_or_else(|| {
            AppError::PathSecurity("operation has no retained project-root handle".into())
        })?;
        Ok(LiveTarget {
            root: LiveRoot::Borrowed(directory),
            relative: destination.to_string(),
        })
    }
}

/// Whether an external destination's parent exists at its path. A missing
/// parent reports `false`: no destination or quarantine can exist at that
/// path. Callers decide what a missing bound parent means; recovery of an
/// operation that may have changed its destination treats it as an error
/// (`existing_operation_target`). A link or other non-directory at the path
/// of a bound parent is identity drift, never an absent destination; an
/// unbound legacy parent keeps the earlier absent reading.
fn external_parent_present(
    parent: &Path,
    external_identity: Option<&str>,
    destination: &str,
) -> Result<bool, AppError> {
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.is_dir() && !is_link_metadata(&metadata) => Ok(true),
        Ok(_) if external_identity.is_some() => Err(AppError::PathSecurity(format!(
            "external destination parent is no longer the directory bound to this transaction: {destination}"
        ))),
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) if external_identity.is_some() => Err(error.into()),
        Err(_) => Ok(false),
    }
}

/// Like `live_target`, but reports `None` when an external parent no longer
/// exists at its path. Journal operations go through
/// `existing_operation_target`, which refuses a missing bound parent when the
/// operation may have changed its destination.
fn existing_live_target<'a>(
    project_directory: Option<&'a RootedDir>,
    external: bool,
    destination: &str,
    external_identity: Option<&str>,
) -> Result<Option<LiveTarget<'a>>, AppError> {
    if external {
        let absolute = validate_external_destination(destination)?;
        let Some(parent) = absolute.parent() else {
            return Ok(None);
        };
        if !external_parent_present(parent, external_identity, destination)? {
            return Ok(None);
        }
    } else if project_directory.is_none() {
        return Ok(None);
    }
    live_target(
        project_directory,
        external,
        destination,
        false,
        external_identity,
    )
    .map(Some)
}

/// Whether a journal operation may have changed its live destination or
/// left a quarantine beside it. Only a leaf-changing action that reached the
/// apply intent can; a skip, an external action, a legacy record without an
/// action, and an operation still `pending` or `staged` have changed nothing,
/// so their rollback genuinely requires no inspection of the destination.
fn operation_may_have_changed_destination(operation: &JournalOperation) -> bool {
    !matches!(
        operation.action,
        Some(OperationAction::Skip | OperationAction::External) | None
    ) && !matches!(operation.status.as_str(), "pending" | "staged")
}

/// The error for a bound external parent that no longer exists while its
/// operation may have changed the destination. The destination, or a
/// quarantine holding the user's bytes, may have moved with the folder, so
/// reading the destination as absent could report a removal or restore that
/// never happened. The journal is left as it was, so the same recovery can be
/// retried once the folder is back.
fn missing_bound_external_parent(destination: &str) -> AppError {
    let folder = validate_external_destination(destination)
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.display().to_string()))
        .unwrap_or_else(|| "its original location".into());
    AppError::PathSecurity(format!(
        "the folder bound to this transaction for {destination} is missing; move it back to {folder} and retry, because recovery cannot confirm the destination while the folder is elsewhere"
    ))
}

/// The retained target of one journal operation, bound to its journaled
/// external parent identity. `None` when the project root capability no
/// longer exists, or when an external parent no longer exists and the
/// operation has not changed its destination. A missing bound parent of an
/// operation that may have changed its destination is an error instead of an
/// absent destination; see `missing_bound_external_parent`.
fn existing_operation_target<'a>(
    project_directory: Option<&'a RootedDir>,
    operation: &JournalOperation,
) -> Result<Option<LiveTarget<'a>>, AppError> {
    let target = existing_live_target(
        project_directory,
        operation.external,
        &operation.destination,
        operation.external_parent_identity.as_deref(),
    )?;
    if target.is_none()
        && operation.external
        && operation.external_parent_identity.is_some()
        && operation_may_have_changed_destination(operation)
    {
        return Err(missing_bound_external_parent(&operation.destination));
    }
    Ok(target)
}

/// Hash and executable state of a live destination read through its retained
/// target. An absent target or leaf reads as `(None, None)`.
fn target_hash_and_executable(
    target: Option<&LiveTarget<'_>>,
) -> Result<(Option<String>, Option<bool>), AppError> {
    let Some(target) = target else {
        return Ok((None, None));
    };
    let current = target.hash()?;
    let executable = if current.is_some() {
        #[cfg(unix)]
        {
            target.dir().observed_executable(&target.relative)?
        }
        #[cfg(not(unix))]
        {
            None
        }
    } else {
        None
    };
    Ok((current, executable))
}

/// Durable journal binding for a quarantine record.
struct QuarantineJournal<'a> {
    journal: &'a mut TransactionJournal,
    store: &'a TransactionStore,
    index: usize,
}

impl QuarantineJournal<'_> {
    fn record(
        &mut self,
        leaf: &str,
        observed: Option<&str>,
        checkpoint: &str,
    ) -> Result<(), AppError> {
        let operation = self.journal.operations.get_mut(self.index).ok_or_else(|| {
            AppError::Transaction("quarantine checkpoint operation is missing".into())
        })?;
        operation.quarantine_leaf = Some(leaf.to_string());
        operation.quarantine_sha256 = observed.map(str::to_string);
        self.journal.last_checkpoint = format!("{checkpoint}-{}", operation.id);
        persist_operation_checkpoint_batch(self.store, self.journal, &[self.index])
    }
}

/// The new state of a live leaf after the displaced bytes are safe.
enum LiveChange<'a> {
    /// Copy a verified staged or backup file into the destination.
    Copy {
        source: &'a RootedDir,
        source_relative: &'a str,
    },
    /// Write exact bytes into the destination.
    Bytes(&'a [u8]),
    /// Move another file from the same directory into the destination.
    MoveFrom(&'a str),
    /// Leave the destination absent; the quarantine holds the removed bytes.
    Delete,
}

fn place_new_leaf(
    directory: &RootedDir,
    destination: &str,
    change: &LiveChange<'_>,
) -> Result<bool, AppError> {
    match change {
        LiveChange::Copy {
            source,
            source_relative,
        } => source.copy_file_atomic_noreplace_to(source_relative, directory, destination),
        LiveChange::Bytes(bytes) => directory.write_atomic_noreplace(destination, bytes),
        LiveChange::MoveFrom(from) => directory.rename_file_noreplace(from, destination),
        LiveChange::Delete => Ok(true),
    }
}

fn no_live_barrier(_barrier: LiveMutationBarrier) {}

/// Replace or delete one live leaf without losing bytes that changed after
/// the precondition hash was taken.
///
/// An absent precondition places the new bytes with an exclusive rename, so a
/// file created in the meantime is kept and the operation fails. An existing
/// precondition first records the quarantine intent, then moves the leaf to
/// its quarantine name through the retained parent handle, verifies the moved
/// bytes, and only then places the new bytes with an exclusive rename. Changed
/// bytes are moved back unless a new file took the name, in which case both
/// are kept and the quarantine stays recorded. On success the caller receives
/// the held quarantine path and releases it after its own result checkpoint.
#[allow(clippy::too_many_arguments)]
fn mutate_live_leaf(
    directory: &RootedDir,
    destination: &str,
    expected_current: Option<&str>,
    change: LiveChange<'_>,
    quarantine_leaf: &str,
    mut journal: Option<QuarantineJournal<'_>>,
    checkpoint_prefix: &str,
    fault: &dyn Fn(QuarantineBoundary) -> Result<(), AppError>,
    barrier: &dyn Fn(LiveMutationBarrier),
) -> Result<Option<String>, AppError> {
    let Some(expected) = expected_current else {
        if !place_new_leaf(directory, destination, &change)? {
            return Err(AppError::Transaction(format!(
                "local precondition changed during apply: a file appeared at {destination} and was kept"
            )));
        }
        return Ok(None);
    };
    let quarantine = quarantine_relative(destination, quarantine_leaf)?;
    if directory.exists(&quarantine)? {
        return Err(AppError::Transaction(format!(
            "quarantine name for {destination} is already in use; manual review is required"
        )));
    }
    if let Some(journal) = journal.as_mut() {
        journal.record(
            quarantine_leaf,
            None,
            &format!("{checkpoint_prefix}-quarantine-intent"),
        )?;
    }
    fault(QuarantineBoundary::BeforeRename)?;
    match directory.rename_file_noreplace(destination, &quarantine) {
        Ok(true) => {}
        Ok(false) => {
            return Err(AppError::Transaction(format!(
                "quarantine name for {destination} appeared during apply; manual review is required"
            )))
        }
        Err(error) => {
            // The rename itself may have succeeded before its directory sync
            // or binding check failed. Report that case accurately and try to
            // put the bytes back instead of claiming nothing changed.
            if directory.is_regular_file(&quarantine).unwrap_or(false) {
                return Err(restore_quarantine_after_error(
                    directory,
                    &quarantine,
                    destination,
                    error,
                ));
            }
            if !directory.exists(destination)? {
                return Err(AppError::Transaction(format!(
                    "local precondition changed during apply: {destination} disappeared and nothing was changed"
                )));
            }
            return Err(error);
        }
    }
    // Quarantine faults model a process stop, so they leave the moved bytes
    // for recovery. Ordinary errors below move them back first.
    fault(QuarantineBoundary::AfterRename)?;
    let observed = match test_fault(&format!("{checkpoint_prefix}_quarantine_hash_error"))
        .and_then(|()| directory.hash_file(&quarantine))
    {
        Ok(observed) => observed,
        Err(error) => {
            return Err(restore_quarantine_after_error(
                directory,
                &quarantine,
                destination,
                error,
            ))
        }
    };
    if observed != expected {
        let recorded = match journal.as_mut() {
            Some(journal) => journal.record(
                quarantine_leaf,
                Some(&observed),
                &format!("{checkpoint_prefix}-quarantine-changed"),
            ),
            None => Ok(()),
        };
        if recorded.is_ok() {
            fault(QuarantineBoundary::BeforeMoveBack)?;
        }
        let moved_back = directory.rename_file_noreplace(&quarantine, destination);
        recorded?;
        if moved_back? {
            return Err(AppError::Transaction(format!(
                "local precondition changed during apply: {destination} changed after review and was left unchanged"
            )));
        }
        return Err(AppError::Transaction(format!(
            "local precondition changed during apply: {destination} changed after review and a new file took its name; the changed bytes are kept at {quarantine}"
        )));
    }
    if let Some(journal) = journal.as_mut() {
        if let Err(error) = test_fault(&format!("{checkpoint_prefix}_quarantine_journal_error"))
            .and_then(|()| {
                journal.record(
                    quarantine_leaf,
                    Some(&observed),
                    &format!("{checkpoint_prefix}-quarantine-verified"),
                )
            })
        {
            return Err(restore_quarantine_after_error(
                directory,
                &quarantine,
                destination,
                error,
            ));
        }
    }
    fault(QuarantineBoundary::AfterVerification)?;
    barrier(LiveMutationBarrier::AfterQuarantineVerified);
    match place_new_leaf(directory, destination, &change) {
        Ok(true) => Ok(Some(quarantine)),
        Ok(false) => Err(AppError::Transaction(format!(
            "local precondition changed during apply: a file appeared at {destination} and was kept; the reviewed bytes are kept at {quarantine}"
        ))),
        Err(error) => Err(restore_quarantine_after_error(
            directory,
            &quarantine,
            destination,
            error,
        )),
    }
}

/// Best-effort recovery after an ordinary error that followed a successful
/// quarantine rename: move the displaced bytes back with an exclusive rename
/// so the destination does not look deleted while recovery is pending. The
/// journal intent stays recorded, so rollback still applies when the move
/// back is impossible, for example because new bytes now hold the name.
fn restore_quarantine_after_error(
    directory: &RootedDir,
    quarantine: &str,
    destination: &str,
    error: AppError,
) -> AppError {
    let outcome = match directory.rename_file_noreplace(quarantine, destination) {
        Ok(true) => format!("{destination} was moved back from its quarantine"),
        Ok(false) => format!(
            "{destination} is occupied, so its earlier bytes are kept at {quarantine} for recovery"
        ),
        Err(move_error) => format!(
            "{destination} could not be moved back ({move_error}); its bytes are kept at {quarantine} for recovery"
        ),
    };
    AppError::Transaction(format!("{error}; {outcome}"))
}

/// Remove a held quarantine only while it still contains the verified bytes.
fn release_quarantine(
    directory: &RootedDir,
    quarantine: &str,
    verified_sha256: &str,
) -> Result<(), AppError> {
    if !directory.exists(quarantine)? {
        return Ok(());
    }
    if directory.remove_file_if_hash(quarantine, verified_sha256)? {
        Ok(())
    } else {
        Err(AppError::Transaction(format!(
            "quarantined bytes changed after verification and were kept at {quarantine}; manual review is required"
        )))
    }
}

/// Settle a quarantine left by an interrupted rollback step before the step
/// is retried. A verified quarantine beside a completed destination is
/// released; a quarantine beside an absent destination is moved back so the
/// step can start again. Any other state keeps both files for review.
fn settle_interrupted_quarantine(
    target: &LiveTarget<'_>,
    quarantine_leaf: &str,
    verified_sha256: Option<&str>,
    completed_states: &[Option<&str>],
) -> Result<(), AppError> {
    let directory = target.dir();
    let quarantine = quarantine_relative(&target.relative, quarantine_leaf)?;
    if !directory.exists(&quarantine)? {
        return Ok(());
    }
    if !directory.is_regular_file(&quarantine)? {
        return Err(AppError::PathSecurity(format!(
            "quarantine is not a regular file: {quarantine}"
        )));
    }
    let held = directory.hash_file(&quarantine)?;
    let current = target.hash()?;
    if verified_sha256 == Some(held.as_str()) && completed_states.contains(&current.as_deref()) {
        return release_quarantine(directory, &quarantine, &held);
    }
    if current.is_none() && directory.rename_file_noreplace(&quarantine, &target.relative)? {
        return Ok(());
    }
    Err(AppError::Transaction(format!(
        "an interrupted operation left {quarantine} beside {}; both files were kept for manual review",
        target.relative
    )))
}

/// Reconcile a lock quarantine whose name is derived from the transaction.
/// `displaced` is the lock hash that the quarantine must hold and `placed` is
/// the lock state that completes the step.
fn settle_lock_quarantine(
    project: &RootedDir,
    leaf: &str,
    displaced: Option<&str>,
    placed: Option<&str>,
) -> Result<(), AppError> {
    let target = LiveTarget {
        root: LiveRoot::Borrowed(project),
        relative: LOCK_RELATIVE_PATH.to_string(),
    };
    settle_interrupted_quarantine(&target, leaf, displaced, &[placed])
}

/// Which completed state a quarantine sweep settles against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuarantineSweep {
    /// Every operation reached its verified result (apply or finalization).
    Result,
    /// Every actionable operation was rolled back.
    Rollback,
}

enum SweepDecision {
    Release,
    MoveBack,
    Keep,
}

/// Decide what to do with a leftover quarantine of `operation` by hash. Bytes
/// equal to the destination are a redundant copy. Precondition or backup
/// bytes beside an absent destination move back during rollback and are
/// released beside a verified result. Planned result bytes are reproducible
/// from staging and may be released. Any other bytes are kept.
fn quarantine_sweep_decision(
    operation: &JournalOperation,
    held: &str,
    current: Option<&str>,
    purpose: QuarantineSweep,
) -> SweepDecision {
    if current == Some(held) {
        return SweepDecision::Release;
    }
    let matches = |hash: &Option<String>| hash.as_deref() == Some(held);
    let precondition = matches(&operation.before_sha256) || matches(&operation.backup_sha256);
    let result = matches(&operation.expected_sha256)
        || matches(&operation.result_sha256)
        || matches(&operation.staged_sha256);
    match purpose {
        QuarantineSweep::Rollback if precondition && current.is_none() => SweepDecision::MoveBack,
        QuarantineSweep::Rollback if result => SweepDecision::Release,
        QuarantineSweep::Rollback => SweepDecision::Keep,
        QuarantineSweep::Result => {
            let completed = if operation.action == Some(OperationAction::DeleteManaged) {
                current.is_none()
            } else {
                operation.after_sha256.is_some() && current == operation.after_sha256.as_deref()
            };
            if (precondition && completed) || result {
                SweepDecision::Release
            } else {
                SweepDecision::Keep
            }
        }
    }
}

/// Settle every leftover `.hoi4ms-quarantine-<transaction id>-*` file in the
/// destination directories of `journal`'s operations, including the step
/// quarantines of its rollback transaction when one is given. Each file is
/// matched to its operation by derived name and settled by hash; bytes that
/// match no recorded state, and names that match no operation, are kept and
/// the sweep fails for manual review. Lock quarantines are settled
/// separately and skipped here. The scan is bounded by directory and entry
/// counts.
fn sweep_transaction_quarantines(
    project_directory: Option<&RootedDir>,
    journal: &TransactionJournal,
    rollback_transaction_id: Option<Uuid>,
    purpose: QuarantineSweep,
) -> Result<(), AppError> {
    let mut owners = vec![(journal.transaction_id, "")];
    if let Some(rollback_id) = rollback_transaction_id {
        owners.push((rollback_id, "rollback-"));
    }
    let prefixes = owners
        .iter()
        .map(|(id, _)| format!("{QUARANTINE_PREFIX}{}-", id.simple()))
        .collect::<Vec<_>>();
    let reserved = owners
        .iter()
        .flat_map(|(id, _)| {
            [
                lock_quarantine_leaf(*id, "commit"),
                lock_quarantine_leaf(*id, "restore"),
            ]
        })
        .collect::<Vec<_>>();

    // Group operations by destination directory; each group is listed once.
    let mut groups: Vec<(bool, String, Vec<usize>)> = Vec::new();
    let mut group_index: HashMap<(bool, String), usize> = HashMap::new();
    for (index, operation) in journal.operations.iter().enumerate() {
        // Skips, external actions, and legacy records without an action never
        // change a live leaf, so they own no quarantine.
        if matches!(
            operation.action,
            Some(OperationAction::Skip | OperationAction::External) | None
        ) {
            continue;
        }
        let parent = if operation.external {
            let absolute = validate_external_destination(&operation.destination)?;
            match absolute.parent() {
                Some(parent) => parent.display().to_string(),
                None => continue,
            }
        } else {
            let normalized = normalize_relative_path(&operation.destination)?;
            normalized
                .rsplit_once('/')
                .map(|(parent, _)| parent.to_string())
                .unwrap_or_default()
        };
        let key = (
            operation.external,
            if cfg!(windows) {
                parent.to_lowercase()
            } else {
                parent.clone()
            },
        );
        match group_index.get(&key) {
            Some(position) => groups[*position].2.push(index),
            None => {
                group_index.insert(key, groups.len());
                groups.push((operation.external, parent, vec![index]));
            }
        }
    }
    if groups.len() > QUARANTINE_SWEEP_MAX_DIRECTORIES {
        return Err(AppError::Transaction(
            "quarantine sweep exceeds its bounded directory count; manual review is required"
                .into(),
        ));
    }

    let mut scanned = 0usize;
    let mut kept = Vec::new();
    for (external, parent, indices) in groups {
        let directory = if external {
            let path = PathBuf::from(&parent);
            let mut present = true;
            for index in &indices {
                let operation = &journal.operations[*index];
                present &= external_parent_present(
                    &path,
                    operation.external_parent_identity.as_deref(),
                    &operation.destination,
                )?;
            }
            if !present {
                // A quarantine of an operation that may have changed its
                // destination could have moved with a bound folder.
                if let Some(index) = indices.iter().find(|index| {
                    let operation = &journal.operations[**index];
                    operation.external_parent_identity.is_some()
                        && operation_may_have_changed_destination(operation)
                }) {
                    return Err(missing_bound_external_parent(
                        &journal.operations[*index].destination,
                    ));
                }
                continue;
            }
            let directory = RootedDir::open(&path)?;
            for index in &indices {
                let operation = &journal.operations[*index];
                verify_external_parent_identity(
                    &directory,
                    operation.external_parent_identity.as_deref(),
                    &operation.destination,
                )?;
            }
            LiveRoot::Owned(directory)
        } else {
            let Some(project) = project_directory else {
                continue;
            };
            if parent.is_empty() {
                LiveRoot::Borrowed(project)
            } else if project.is_directory(&parent)? {
                LiveRoot::Owned(project.open_dir(&parent)?)
            } else {
                continue;
            }
        };
        let directory = match &directory {
            LiveRoot::Borrowed(directory) => *directory,
            LiveRoot::Owned(directory) => directory,
        };
        let mut owned_names: HashMap<String, usize> = HashMap::new();
        for index in &indices {
            for (id, operation_prefix) in &owners {
                owned_names.insert(
                    quarantine_leaf_name(
                        *id,
                        &format!("{operation_prefix}{}", journal.operations[*index].id),
                    ),
                    *index,
                );
            }
        }
        let names = directory.read_dir_names()?;
        scanned = scanned.saturating_add(names.len());
        if scanned > QUARANTINE_SWEEP_MAX_ENTRIES {
            return Err(AppError::Transaction(
                "quarantine sweep exceeds its bounded entry count; manual review is required"
                    .into(),
            ));
        }
        for name in names {
            let Some(name) = name.to_str() else {
                continue;
            };
            if !prefixes.iter().any(|prefix| name.starts_with(prefix))
                || reserved.iter().any(|leaf| leaf == name)
                || !directory.exists(name)?
            {
                continue;
            }
            let shown = if parent.is_empty() {
                name.to_string()
            } else {
                format!("{parent}/{name}")
            };
            let Some(index) = owned_names.get(name).copied() else {
                kept.push(shown);
                continue;
            };
            if !directory.is_regular_file(name)? {
                kept.push(shown);
                continue;
            }
            let operation = &journal.operations[index];
            let leaf = destination_leaf(operation)?;
            let held = directory.hash_file(name)?;
            let current = live_leaf_hash(directory, &leaf)?;
            match quarantine_sweep_decision(operation, &held, current.as_deref(), purpose) {
                SweepDecision::Release => release_quarantine(directory, name, &held)?,
                SweepDecision::MoveBack => {
                    if !directory.rename_file_noreplace(name, &leaf)? {
                        kept.push(shown);
                    }
                }
                SweepDecision::Keep => kept.push(shown),
            }
        }
    }
    if kept.is_empty() {
        return Ok(());
    }
    Err(AppError::Transaction(format!(
        "{} quarantined file(s) of transaction {} match no recorded state and were kept for manual review, including {}",
        kept.len(),
        journal.transaction_id,
        kept[0]
    )))
}

/// The final path component of an operation destination.
fn destination_leaf(operation: &JournalOperation) -> Result<String, AppError> {
    let leaf = if operation.external {
        validate_external_destination(&operation.destination)?
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
    } else {
        let normalized = normalize_relative_path(&operation.destination)?;
        normalized
            .rsplit_once('/')
            .map(|(_, leaf)| leaf.to_string())
            .or(Some(normalized))
    };
    leaf.filter(|leaf| !leaf.is_empty()).ok_or_else(|| {
        AppError::PathSecurity(format!(
            "quarantine sweep cannot name the destination of {}",
            operation.id
        ))
    })
}

/// Settle a forward quarantine that survives beside a destination rollback
/// already considers restored. It is released only when it holds exactly the
/// restored bytes; otherwise both files are kept and rollback stops, so an
/// earlier local edit is never hidden behind a reported success.
fn settle_forward_quarantine_beside_restored(
    project_directory: Option<&RootedDir>,
    transaction_id: Uuid,
    operation: &JournalOperation,
) -> Result<(), AppError> {
    let leaf = forward_quarantine_leaf(transaction_id, operation)?;
    let Some(target) = existing_operation_target(project_directory, operation)? else {
        return Ok(());
    };
    let quarantine = quarantine_relative(&target.relative, &leaf)?;
    if !target.dir().exists(&quarantine)? {
        return Ok(());
    }
    if !target.dir().is_regular_file(&quarantine)? {
        return Err(AppError::PathSecurity(format!(
            "quarantine is not a regular file: {quarantine}"
        )));
    }
    let held = target.dir().hash_file(&quarantine)?;
    if target.hash()?.as_deref() == Some(held.as_str()) {
        return release_quarantine(target.dir(), &quarantine, &held);
    }
    Err(AppError::Transaction(format!(
        "{} already holds its restored bytes while different earlier bytes are kept at {quarantine}; both files were kept for manual review",
        operation.destination
    )))
}

#[cfg(test)]
thread_local! {
    static TEST_FAULT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Return an error at a named recovery checkpoint in tests. The fault is
/// thread-local, so parallel tests do not observe each other's faults.
#[cfg(test)]
fn test_fault(checkpoint: &str) -> Result<(), AppError> {
    TEST_FAULT.with(|fault| {
        if fault.borrow().as_deref() == Some(checkpoint) {
            Err(AppError::Transaction(format!(
                "fault injected at {checkpoint}"
            )))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn test_fault(_checkpoint: &str) -> Result<(), AppError> {
    Ok(())
}

fn rollback_quarantine_fault(boundary: QuarantineBoundary) -> Result<(), AppError> {
    test_fault(&format!("rollback_quarantine_{boundary:?}"))
}

fn rollback_lock_quarantine_fault(boundary: QuarantineBoundary) -> Result<(), AppError> {
    test_fault(&format!("rollback_lock_quarantine_{boundary:?}"))
}

fn read_file_path(path: &Path) -> Result<Vec<u8>, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("read file has no parent directory".into()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::PathSecurity("read file name is invalid".into()))?;
    RootedDir::open_read(parent)?.read_file(name)
}

fn remove_directory_path_if_empty(path: &Path) -> Result<bool, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("removed directory has no parent".into()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::PathSecurity("removed directory name is invalid".into()))?;
    RootedDir::open(parent)?.remove_dir_if_empty(name)
}

/// Verify every applied destination by reading it through the retained
/// project capability or the bound external parent. The bytes that are
/// validated are the bytes that are hashed, and neither read re-resolves the
/// project root or an external parent by path.
fn post_install_checks(
    project_root: &Path,
    project_directory: &RootedDir,
    plan: &InstallationPlan,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    for operation in &plan.operations {
        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) {
            continue;
        }
        let record_index = journal
            .operations
            .iter()
            .position(|record| record.id == operation.id)
            .ok_or_else(|| AppError::Transaction("journal operation missing".into()))?;
        let target =
            existing_operation_target(Some(project_directory), &journal.operations[record_index])?;
        let (actual, executable) = if operation.action == OperationAction::DeleteManaged {
            if let Some(target) = target.as_ref() {
                if target.dir().exists(&target.relative)? {
                    return Err(AppError::Transaction(format!(
                        "managed delete was not completed: {}",
                        operation.destination
                    )));
                }
            }
            (None, None)
        } else {
            let target = target.as_ref().ok_or_else(|| {
                AppError::Transaction(format!(
                    "destination parent is missing after apply: {}",
                    operation.destination
                ))
            })?;
            let bytes = target.dir().read_file(&target.relative)?;
            validate_managed_bytes(project_root, operation, &bytes)?;
            #[cfg(unix)]
            let executable = target.dir().observed_executable(&target.relative)?;
            #[cfg(not(unix))]
            let executable = None;
            (Some(sha256_bytes(&bytes)), executable)
        };
        let record = &journal.operations[record_index];
        if record.after_sha256.as_deref() != actual.as_deref()
            || (operation.action != OperationAction::DeleteManaged
                && actual.as_deref()
                    != operation
                        .result_sha256
                        .as_deref()
                        .or(operation.source_sha256.as_deref()))
            || (operation.action != OperationAction::DeleteManaged
                && executable.is_some_and(|value: bool| value != operation.executable))
        {
            return Err(AppError::Transaction(format!(
                "post-install hash mismatch for {}",
                operation.destination
            )));
        }
    }
    journal.last_checkpoint = "post-install-verified".into();
    persist_journal(store, journal)
}

/// Re-check every destination immediately before the success lock is built.
/// The apply and readiness passes are necessary but not sufficient: a local
/// editor or another process may have changed a skipped or applied file after
/// those checkpoints. A changed live precondition fails closed instead of
/// allowing the lock to record bytes that were never reviewed.
fn final_live_verification(
    project_directory: &RootedDir,
    plan: &InstallationPlan,
    journal: &TransactionJournal,
) -> Result<(), AppError> {
    for operation in &plan.operations {
        let (current, executable) = if operation.external {
            let bound = journal
                .operations
                .iter()
                .find(|record| record.id == operation.id)
                .and_then(|record| record.external_parent_identity.as_deref());
            let target = existing_live_target(None, true, &operation.destination, bound)?;
            target_hash_and_executable(target.as_ref())?
        } else {
            let exists = project_directory.exists(&operation.destination)?;
            let is_file = project_directory.is_regular_file(&operation.destination)?;
            if exists && !is_file {
                return Err(AppError::PathSecurity(format!(
                    "final destination is not a regular file: {}",
                    operation.destination
                )));
            }
            let current = is_file
                .then(|| project_directory.hash_file(&operation.destination))
                .transpose()?;
            let executable = if is_file {
                #[cfg(unix)]
                {
                    project_directory.observed_executable(&operation.destination)?
                }
                #[cfg(not(unix))]
                {
                    None
                }
            } else {
                None
            };
            (current, executable)
        };
        let journal_operation = journal
            .operations
            .iter()
            .find(|record| record.id == operation.id)
            .ok_or_else(|| {
                AppError::Transaction(format!(
                    "journal operation is missing before final verification: {}",
                    operation.id
                ))
            })?;
        match operation.action {
            OperationAction::Skip | OperationAction::External => {
                if current.as_deref() != operation.local_sha256.as_deref() {
                    return Err(AppError::Transaction(format!(
                        "skipped destination changed before lock finalization: {}",
                        operation.destination
                    )));
                }
            }
            OperationAction::DeleteManaged => {
                if current.is_some() || journal_operation.after_exists != Some(false) {
                    return Err(AppError::Transaction(format!(
                        "managed delete changed before lock finalization: {}",
                        operation.destination
                    )));
                }
            }
            _ => {
                let expected = operation
                    .result_sha256
                    .as_deref()
                    .or(operation.source_sha256.as_deref())
                    .ok_or_else(|| {
                        AppError::Transaction(format!(
                            "operation has no final checksum: {}",
                            operation.destination
                        ))
                    })?;
                if current.as_deref() != Some(expected)
                    || journal_operation.after_sha256.as_deref() != Some(expected)
                    || journal_operation.after_exists != Some(true)
                    || executable.is_some_and(|value| value != operation.executable)
                    || journal_operation
                        .after_executable
                        .is_some_and(|value| value != operation.executable)
                {
                    return Err(AppError::Transaction(format!(
                        "destination changed before lock finalization: {}",
                        operation.destination
                    )));
                }
            }
        }
    }
    Ok(())
}

fn build_transaction_readiness(
    project_root: &Path,
    plan: &InstallationPlan,
    journal: &TransactionJournal,
) -> Result<crate::models::ReadinessReport, AppError> {
    let removing = plan.maintenance_mode.as_deref() == Some("remove")
        && !plan.operations.is_empty()
        && plan.operations.iter().all(|operation| {
            matches!(
                operation.action,
                OperationAction::Skip | OperationAction::DeleteManaged
            )
        });
    if removing {
        let (integration, auth_mode) = crate::ai::integration_and_auth_mode(&plan.ai_provider);
        let confirmed_field_count = plan
            .codex_analysis
            .as_ref()
            .map(|analysis| analysis.confirmed_fields.len() as u32)
            .unwrap_or(0);
        return Ok(crate::models::ReadinessReport {
            schema_version: "1.0.0".into(),
            report_id: Uuid::new_v4(),
            project_id: plan.project_id.clone(),
            generated_at: Utc::now().to_rfc3339(),
            codex: ReadinessCodexSummary {
                provider: plan.ai_provider.clone(),
                model: plan.ai_model.clone(),
                integration: integration.into(),
                auth_mode: auth_mode.into(),
                authenticated_during_setup: plan.codex_analysis.is_some(),
                analysis_status: if plan.codex_analysis.is_some() { "confirmed" } else { "block" }.into(),
                confirmed_field_count,
                no_account_metadata_persisted: true,
                blocking_check_ids: vec![],
            },
            checks: vec![ReadinessCheck {
                id: "installation.removed".into(),
                category: "transaction".into(),
                label: "Managed removal".into(),
                status: "pass".into(),
                blocking: false,
                message: Some(
                    "Managed, unmodified content was removed; user-owned and modified content was preserved for review.".into(),
                ),
                evidence: vec![ReadinessEvidence {
                    kind: "transaction".into(),
                    value: serde_json::json!({
                        "deleted_operations": plan
                            .operations
                            .iter()
                            .filter(|operation| operation.action == OperationAction::DeleteManaged)
                            .count(),
                        "preserved_operations": plan
                            .operations
                            .iter()
                            .filter(|operation| operation.action == OperationAction::Skip)
                            .count(),
                    }),
                    path: Some("transaction journal".into()),
                }],
            }],
            summary: ReadinessSummary {
                pass: 1,
                ..Default::default()
            },
            core_ready: false,
            open_in_codex: OpenInCodex {
                enabled: false,
                blocking_check_ids: vec![],
                command_preview: None,
            },
            notes: vec![
                "This transaction removed managed content; the project is not presented as Codex-ready until a new setup or repair is completed.".into(),
            ],
        });
    }
    let is_file = |relative: &str| {
        safe_join(project_root, relative)
            .map(|path| path.is_file())
            .unwrap_or(false)
    };
    let has_component = |id: &str| plan.selected_components.iter().any(|item| item == id);
    // Legacy and synthetic transaction fixtures may predate the coding-client
    // package closure. Production plans always carry at least one native
    // environment component; only those plans require an on-disk package
    // readiness check here.
    let coding_environment_selected = plan.selected_components.iter().any(|id| {
        matches!(
            id.as_str(),
            "codex.config"
                | "core.claude.instructions"
                | "runtime.claude"
                | "runtime.claude.mcp"
                | "runtime.cursor"
                | "runtime.qoder"
                | "runtime.opencode"
                | "runtime.opencode.config"
        ) || id.starts_with("environment.")
    }) || plan
        .operations
        .iter()
        .any(|operation| looks_like_coding_environment_destination(&operation.destination));
    let mut readiness_components = plan.selected_components.clone();
    if plan
        .operations
        .iter()
        .any(|operation| operation.destination == "descriptor.mod")
    {
        readiness_components.push("project.descriptor".into());
    }
    if plan.operations.iter().any(|operation| {
        operation.external && operation.component_id.starts_with("project.launcher")
    }) {
        readiness_components.push("project.launcher_descriptor".into());
    }
    if plan
        .operations
        .iter()
        .any(|operation| operation.destination == "thumbnail.png")
    {
        readiness_components.push("project.thumbnail".into());
    }
    let descriptor_valid = is_file("descriptor.mod")
        && read_file_path(&safe_join(project_root, "descriptor.mod")?)
            .ok()
            .and_then(|bytes| crate::descriptors::parse_descriptor(&bytes).ok())
            .is_some_and(|descriptor| {
                descriptor.fields.contains_key("name")
                    && descriptor.fields.contains_key("supported_version")
                    && descriptor
                        .fields
                        .get("picture")
                        .is_some_and(|value| value == "thumbnail.png")
            });
    let agents_valid = crate::readiness::valid_agents_file(project_root);
    let skills_valid =
        !has_component("core.skills") || crate::readiness::valid_skill_tree(project_root);
    let subagents_valid =
        !has_component("core.subagents") || crate::readiness::valid_subagent_tree(project_root);
    let codex_path = safe_join(project_root, ".codex/config.toml")?;
    let codex_valid = !has_component("codex.config")
        || (codex_path.is_file()
            && fs::read_to_string(codex_path)
                .ok()
                .and_then(|text| text.parse::<toml::Value>().ok())
                .is_some());
    let wiki_pages = plan.wiki_required_pages.clone();
    let wiki_broken_links = if !has_component("wiki.snapshot") {
        Vec::new()
    } else if project_root.join("paradox_wiki").is_dir() {
        crate::readiness::wiki_link_integrity(project_root)
    } else {
        vec!["paradox_wiki/".into()]
    };
    let wiki_metadata_valid = !has_component("wiki.snapshot")
        || (!plan.wiki_required_pages.is_empty() && plan.wiki_metadata.is_some());
    let wiki_status = if !has_component("wiki.snapshot") {
        "not_selected".to_string()
    } else if project_root.join("paradox_wiki").is_dir()
        && wiki_broken_links.is_empty()
        && wiki_metadata_valid
        && wiki_pages.iter().all(|page| {
            safe_join(project_root, &format!("paradox_wiki/{page}"))
                .map(|path| path.is_file())
                .unwrap_or(false)
        })
    {
        "pass".into()
    } else {
        "block".into()
    };
    let mcp_status = if !has_component("mcp.hoi4_agent_tools") {
        "not_selected".into()
    } else if cfg!(target_os = "macos") {
        "unsupported_platform".into()
    } else {
        match plan
            .optional_workflows
            .get(crate::mcp::COMPONENT_ID)
            .map(String::as_str)
        {
            Some("ready") => "pass".into(),
            Some("unsupported_platform") => "unsupported_platform".into(),
            Some("incomplete" | "selected_pending") | None => "block".into(),
            Some(_) => "block".into(),
        }
    };
    let git_status = match plan.git_setup.as_ref() {
        None => "not_selected".into(),
        Some(_) if crate::git::read_git_head(project_root).repository_present => "pass".into(),
        Some(_) => "block".into(),
    };
    let hashes_valid = journal.operations.iter().all(|operation| {
        operation.status == "verified"
            || (operation.status == "pending"
                && plan.operations.iter().any(|candidate| {
                    candidate.id == operation.id && candidate.action == OperationAction::Skip
                }))
    });
    let thumbnail_operation = plan
        .operations
        .iter()
        .find(|operation| !operation.external && operation.destination == "thumbnail.png")
        .cloned();
    let thumbnail_valid = thumbnail_operation.is_some_and(|operation| {
        safe_join(project_root, "thumbnail.png")
            .ok()
            .and_then(|path| read_file_path(&path).ok())
            .is_some_and(|bytes| {
                crate::descriptors::validate_thumbnail_png(&bytes).is_ok()
                    && (operation.action == OperationAction::Skip
                        || operation
                            .result_sha256
                            .as_ref()
                            .or(operation.source_sha256.as_ref())
                            .is_some_and(|expected| sha256_bytes(&bytes) == *expected))
            })
    });
    let workflow_3d_state = plan
        .optional_workflows
        .get("workflow.3d")
        .cloned()
        .unwrap_or_else(|| "not_selected".into());
    let workflow_super_events_state = plan
        .optional_workflows
        .get("workflow.super_events")
        .cloned()
        .unwrap_or_else(|| "not_selected".into());
    let portrait_provider = plan
        .portrait_pipeline
        .as_ref()
        .map(|portrait| portrait.provider.clone())
        .unwrap_or_else(|| "disabled".into());
    let portrait_provider_status = plan
        .portrait_pipeline
        .as_ref()
        .map(|portrait| portrait.provider_status.clone())
        .unwrap_or_else(|| "not_selected".into());
    let ai_provider = if plan.ai_provider.trim().is_empty() {
        "codex".to_string()
    } else {
        plan.ai_provider.clone()
    };
    let ai_authenticated = plan
        .codex_analysis
        .as_ref()
        .is_some_and(|record| crate::codex::validate_confirmed_record(record).is_ok());
    let ai_analysis_status = if plan
        .codex_analysis
        .as_ref()
        .is_some_and(|record| !record.confirmed_fields.is_empty())
    {
        "confirmed"
    } else {
        "blocked"
    };
    let ai_confirmed_field_count = plan
        .codex_analysis
        .as_ref()
        .map(|record| record.confirmed_fields.len() as u32)
        .unwrap_or(0);
    let mcp_blocking = has_component("mcp.hoi4_agent_tools")
        && cfg!(target_os = "windows")
        && mcp_status == "block";
    let mut report = crate::readiness::evaluate(&crate::readiness::ReadinessInput {
        project_id: plan.project_id.clone(),
        project_root: project_root.display().to_string(),
        selected_components: readiness_components,
        primary_coding_environment: plan.primary_coding_environment.clone(),
        additional_coding_environments: plan.additional_coding_environments.clone(),
        coding_environments_status: if !coding_environment_selected
            || (crate::coding_environment::validate_selection(&CodingEnvironmentSelection {
                primary: plan.primary_coding_environment.clone(),
                additional: plan.additional_coding_environments.clone(),
            })
            .is_ok()
                && crate::coding_environment::selected_environment_ids(
                    &CodingEnvironmentSelection {
                        primary: plan.primary_coding_environment.clone(),
                        additional: plan.additional_coding_environments.clone(),
                    },
                )
                .iter()
                .all(|environment| {
                    let require_mcp = crate::models::Platform::current()
                        == crate::models::Platform::Windows
                        && crate::coding_environment::mcp_registration_component_id(environment)
                            .is_some_and(|component_id| {
                                plan.selected_components
                                    .iter()
                                    .any(|selected| selected == component_id)
                            });
                    crate::readiness::valid_coding_environment_with_mcp(
                        project_root,
                        environment,
                        require_mcp,
                    )
                })) {
            "pass".into()
        } else {
            "block".into()
        },
        source_verified: crate::source::validate_commit(&plan.source.resolved_revision).is_ok()
            && crate::source::validate_sha256(&plan.source.manifest_sha256).is_ok()
            && matches!(
                plan.source.manifest_origin.as_str(),
                "remote" | "bundled_revision_bootstrap"
            ),
        descriptors_valid: descriptor_valid,
        launcher_valid: {
            let launchers = plan
                .operations
                .iter()
                .filter(|operation| {
                    operation.external && operation.component_id.starts_with("project.launcher")
                })
                .collect::<Vec<_>>();
            !launchers.is_empty()
                && launchers.iter().all(|operation| {
                    let expected = operation
                        .result_sha256
                        .as_ref()
                        .or(operation.source_sha256.as_ref());
                    validate_external_destination(&operation.destination)
                        .ok()
                        .and_then(|path| read_file_path(&path).ok())
                        .is_some_and(|bytes| {
                            crate::readiness::launcher_descriptor_matches_project(
                                project_root,
                                &bytes,
                            ) && (operation.action == OperationAction::Skip
                                || expected
                                    .is_some_and(|expected| sha256_bytes(&bytes) == *expected))
                        })
                })
        },
        thumbnail_valid,
        structure_valid: project_root.is_dir(),
        agents_valid,
        skills_valid,
        subagents_valid,
        codex_valid,
        codex_authenticated: plan.codex_analysis.as_ref().is_some_and(|record| {
            record.engine == "codex_app_server"
                && record.auth_mode == "chatgpt"
                && !record.account_identity_persisted
        }),
        codex_analysis_status: if plan
            .codex_analysis
            .as_ref()
            .is_some_and(|record| !record.confirmed_fields.is_empty())
        {
            "confirmed".into()
        } else {
            "blocked".into()
        },
        codex_confirmed_field_count: plan
            .codex_analysis
            .as_ref()
            .map(|record| record.confirmed_fields.len() as u32)
            .unwrap_or(0),
        ai_provider: ai_provider.clone(),
        ai_model: plan.ai_model.clone(),
        ai_authenticated,
        ai_analysis_status: ai_analysis_status.into(),
        ai_confirmed_field_count,
        flatten_status: if plan.flatten_chat_sources {
            crate::readiness::flattened_artifact_status(project_root, &plan.generated_artifacts)
        } else {
            "not_selected".into()
        },
        mcp_status,
        mcp_blocking,
        wiki_status,
        wiki_required_pages: wiki_pages,
        wiki_broken_links,
        git_status,
        environment_status: "pass".into(),
        hashes_valid,
        conflict_status: if plan
            .conflicts
            .iter()
            .all(|conflict| conflict.selected.is_some())
        {
            "pass".into()
        } else {
            "block".into()
        },
        dependency_status: "pass".into(),
        workflow_3d_state,
        workflow_super_events_state,
        portrait_provider,
        portrait_provider_status,
        source_license_status: plan
            .wiki_metadata
            .as_ref()
            .map(|metadata| metadata.repository_license_status.clone())
            .unwrap_or_else(|| "unknown".into()),
        wiki_source_status: plan
            .wiki_metadata
            .as_ref()
            .map(|metadata| metadata.source_status.clone())
            .unwrap_or_else(|| "unknown".into()),
        wiki_license_status: plan
            .wiki_metadata
            .as_ref()
            .map(|metadata| metadata.license_status.clone())
            .unwrap_or_else(|| "unknown".into()),
        notes: vec![
            "Transaction readiness is evaluated before the success lock is written.".into(),
        ],
    });
    if let Some(check) = report
        .checks
        .iter_mut()
        .find(|check| check.id == "mcp.hoi4" && check.status == "block")
    {
        if let Some(detail) = journal
            .stages
            .iter()
            .filter(|stage| stage.id == "post-install checks")
            .flat_map(|stage| stage.evidence.iter())
            .find_map(|evidence| {
                evidence.strip_prefix("external-action:mcp.hoi4_agent_tools:incomplete:")
            })
        {
            let bounded: String = redact_secrets(detail, &[]).chars().take(736).collect();
            check.message = Some(redact_secrets(&bounded, &[]));
        }
    }
    Ok(report)
}

fn looks_like_coding_environment_destination(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized == "claude.md"
        || normalized == ".mcp.json"
        || normalized == "opencode.json"
        || normalized.starts_with(".codex/")
        || normalized.starts_with(".claude/")
        || normalized.starts_with(".cursor/")
        || normalized.starts_with(".qoder/")
        || normalized.starts_with(".opencode/")
}

fn build_lock(
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    journal: &TransactionJournal,
    previous_lock: Option<&InstallationLock>,
    project_root: &Path,
    project_directory: &RootedDir,
) -> Result<InstallationLock, AppError> {
    let removing = plan.maintenance_mode.as_deref() == Some("remove")
        && !plan.operations.is_empty()
        && plan.operations.iter().all(|operation| {
            matches!(
                operation.action,
                OperationAction::Skip | OperationAction::DeleteManaged
            )
        });
    let prepared: HashMap<&str, &PreparedFile> = prepared_files
        .iter()
        .map(|file| (file.operation_id.as_str(), file))
        .collect();
    let key_for = |path: &str, external: bool| (external, path.to_ascii_lowercase());
    let mut files = previous_lock
        .map(|lock| {
            lock.files
                .iter()
                .cloned()
                .map(|file| (key_for(&file.path, file.external), file))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let mut local_modifications = previous_lock
        .map(|lock| lock.local_modifications.clone())
        .unwrap_or_default();
    for operation in &plan.operations {
        let key = key_for(&operation.destination, operation.external);
        let ownership = operation.ownership.ok_or_else(|| {
            AppError::Transaction(format!(
                "operation ownership is missing for {}",
                operation.destination
            ))
        })?;
        match operation.action {
            OperationAction::Skip | OperationAction::External => {
                if let Some(existing_hash) = files
                    .get(&key)
                    .map(|existing| existing.installed_sha256.clone())
                {
                    if operation.local_state == LocalState::Modified {
                        if removing && operation.local_sha256.is_none() {
                            // A managed-removal target that is already absent
                            // is not a user modification. Drop its stale lock
                            // baseline instead of fabricating a current hash.
                            files.remove(&key);
                            remove_local_modification(
                                &mut local_modifications,
                                &operation.destination,
                            );
                        } else {
                            record_local_modification(
                                &mut local_modifications,
                                operation,
                                &existing_hash,
                            );
                        }
                    }
                    // A kept or skipped file in a maintenance plan joins the
                    // plan's revision: readiness requires one revision across
                    // the lock, and the installed hash still records the
                    // bytes the user kept. A file the new release no longer
                    // ships is preserved so maintenance never removes it.
                    if plan.maintenance_mode.is_some() && !removing {
                        if let Some(existing) = files.get_mut(&key) {
                            if existing.source_revision != plan.source.resolved_revision {
                                existing.source_revision = plan.source.resolved_revision.clone();
                                match (&operation.source_path, &operation.source_sha256) {
                                    (Some(source_path), Some(source_sha256)) => {
                                        existing.source_path = source_path.clone();
                                        existing.source_sha256 = source_sha256.clone();
                                        existing.source_size = operation.source_size;
                                    }
                                    _ => existing.preserved_local = true,
                                }
                            }
                        }
                    }
                } else if operation.local_state != LocalState::Absent {
                    // A first install can intentionally keep a user-owned
                    // file (most importantly an existing thumbnail). It must
                    // still be represented in the lock so readiness can hash
                    // it and future maintenance cannot treat it as absent.
                    let bytes = if operation.external {
                        let destination = operation_destination(project_root, operation)?;
                        read_file_path(&destination)
                    } else {
                        project_directory.read_file(&operation.destination)
                    }
                    .map_err(|error| {
                        AppError::Transaction(format!(
                            "cannot lock preserved file {}: {error}",
                            operation.destination
                        ))
                    })?;
                    let current_sha256 = sha256_bytes(&bytes);
                    let installed_sha256 = current_sha256.clone();
                    remove_local_modification(&mut local_modifications, &operation.destination);
                    files.insert(
                        key,
                        LockedFile {
                            path: operation.destination.clone(),
                            location_scope: Some(if operation.external {
                                "external_launcher".into()
                            } else if operation.destination.starts_with(".hoi4-mod-setup/") {
                                "application_data".into()
                            } else {
                                "project".into()
                            }),
                            component_id: operation.component_id.clone(),
                            source_path: operation
                                .source_path
                                .clone()
                                .unwrap_or_else(|| operation.destination.clone()),
                            source_revision: plan.source.resolved_revision.clone(),
                            source_sha256: operation
                                .source_sha256
                                .clone()
                                .unwrap_or_else(|| current_sha256.clone()),
                            source_size: operation.source_size,
                            base_sha256: operation.base_sha256.clone(),
                            installed_sha256,
                            installed_size: Some(bytes.len() as u64),
                            ownership,
                            preserved_local: operation.local_state == LocalState::Modified,
                            external: operation.external,
                            generated_content: None,
                            generated_bytes: None,
                            executable: operation.executable,
                            platform: operation.platform.or(Some(ManifestPlatform::All)),
                        },
                    );
                }
            }
            OperationAction::DeleteManaged => {
                files.remove(&key);
                remove_local_modification(&mut local_modifications, &operation.destination);
            }
            _ => {
                let prepared = prepared.get(operation.id.as_str()).ok_or_else(|| {
                    AppError::Transaction(format!(
                        "lock content is missing for operation {}",
                        operation.id
                    ))
                })?;
                let source_sha256 = operation
                    .source_sha256
                    .clone()
                    .unwrap_or_else(|| prepared.expected_sha256.clone());
                let generated = operation.action == OperationAction::Generate
                    || operation
                        .source_path
                        .as_deref()
                        .is_some_and(|path| path.starts_with("generated:"));
                let locked_file = LockedFile {
                    path: operation.destination.clone(),
                    location_scope: Some(if operation.external {
                        "external_launcher".into()
                    } else if operation.destination.starts_with(".hoi4-mod-setup/") {
                        "application_data".into()
                    } else {
                        "project".into()
                    }),
                    component_id: operation.component_id.clone(),
                    source_path: operation
                        .source_path
                        .clone()
                        .unwrap_or_else(|| operation.destination.clone()),
                    source_revision: plan.source.resolved_revision.clone(),
                    source_sha256,
                    source_size: operation.source_size.or(Some(prepared.bytes.len() as u64)),
                    base_sha256: operation.base_sha256.clone(),
                    installed_sha256: prepared.expected_sha256.clone(),
                    installed_size: Some(prepared.bytes.len() as u64),
                    ownership,
                    preserved_local: false,
                    external: operation.external,
                    // Repair reproduces generated files from this recorded
                    // content, and an update replans them as replacements of
                    // a `generated:` source, so keep it for either action.
                    generated_content: if generated {
                        String::from_utf8(prepared.bytes.clone()).ok()
                    } else {
                        None
                    },
                    generated_bytes: generated.then_some(prepared.bytes.clone()),
                    executable: operation.executable,
                    platform: operation.platform.or(Some(ManifestPlatform::All)),
                };
                files.insert(key, locked_file);
                remove_local_modification(&mut local_modifications, &operation.destination);
            }
        }
    }
    let mut files = files.into_values().collect::<Vec<_>>();
    files.sort_by(|left, right| {
        left.external.cmp(&right.external).then_with(|| {
            left.path
                .to_ascii_lowercase()
                .cmp(&right.path.to_ascii_lowercase())
        })
    });
    let mut component_ids = plan.selected_components.clone();
    if let Some(previous) = previous_lock {
        for component in &previous.components {
            if component_ids.iter().any(|id| id == &component.id) {
                continue;
            }
            // Maintenance plans intentionally keep predecessor components in
            // the lock for auditability.  A deselected coding-environment
            // component is the exception: when every one of its managed
            // files was removed, retaining the old component would make the
            // package appear installed again on the next repair.  Use the
            // resulting file set and operation actions rather than a static
            // component-ID allowlist so newly published manifest components
            // follow the same rule without an app release.
            let has_live_file = files.iter().any(|file| file.component_id == component.id);
            let has_non_delete_operation = plan.operations.iter().any(|operation| {
                operation.component_id == component.id
                    && operation.action != OperationAction::DeleteManaged
            });
            let looks_like_environment =
                crate::coding_environment::is_known_environment_component_id(&component.id)
                    || component.id.starts_with("environment.")
                    || component.id.starts_with("runtime.")
                    || plan.operations.iter().any(|operation| {
                        operation.component_id == component.id
                            && looks_like_coding_environment_destination(&operation.destination)
                    });
            if !looks_like_environment || has_live_file || has_non_delete_operation {
                component_ids.push(component.id.clone());
            }
        }
    }
    let mcp_route_planned_unavailable = plan
        .selected_components
        .iter()
        .any(|id| id == "mcp.hoi4_agent_tools")
        && matches!(
            crate::mcp::reviewed_plan_target(&plan.external_actions),
            Err(AppError::UnsupportedPlatform(_))
        );
    let components = component_ids
        .iter()
        .map(|id| LockComponent {
            id: id.clone(),
            version: previous_lock.and_then(|lock| {
                lock.components
                    .iter()
                    .find(|component| component.id == *id)
                    .and_then(|component| component.version.clone())
            }),
            state: if removing {
                "removed".into()
            } else {
                lock_component_state(
                    plan.optional_workflows
                        .get(id)
                        .cloned()
                        .or_else(|| {
                            previous_lock.and_then(|lock| {
                                lock.components
                                    .iter()
                                    .find(|component| component.id == *id)
                                    .map(|component| component.state.clone())
                            })
                        })
                        .unwrap_or_else(|| "installed".into()),
                )
            },
            source_revision: Some(plan.source.resolved_revision.clone()),
            validation: Some(
                if id == "mcp.hoi4_agent_tools" && mcp_route_planned_unavailable {
                    "planned_unavailable"
                } else {
                    "pass"
                }
                .into(),
            ),
        })
        .collect();
    let mut optional_workflows = previous_lock
        .map(|lock| lock.optional_workflows.clone())
        .unwrap_or_default();
    // Legacy releases stored a portrait-workflow interest preference. It is no
    // longer a setup feature, so every newly verified lock drops that state.
    optional_workflows.remove("workflow.lora_comfyui_interest");
    if removing {
        for workflow in optional_workflows.values_mut() {
            workflow.credential_reference = None;
        }
    }
    for (id, state) in &plan.optional_workflows {
        if id == "workflow.lora_comfyui_interest" {
            continue;
        }
        optional_workflows.insert(
            id.clone(),
            OptionalWorkflowLock {
                state: state.clone(),
                reason: if state == "planned_unavailable" {
                    Some("Automated setup is not implemented in version 1.".into())
                } else {
                    None
                },
                credential_reference: if removing || id != "workflow.3d" {
                    None
                } else {
                    plan.credential_references
                        .iter()
                        .find(|reference| {
                            reference.name == crate::credentials::MESHY_ENVIRONMENT_NAME
                        })
                        .map(|reference| reference.reference.clone())
                        .or_else(|| {
                            previous_lock.and_then(|lock| {
                                lock.optional_workflows
                                    .get(id)
                                    .and_then(|workflow| workflow.credential_reference.clone())
                            })
                        })
                },
            },
        );
    }
    optional_workflows.remove("workflow.lora_comfyui_interest");
    let mut merge_choices = previous_lock
        .map(|lock| lock.merge_choices.clone())
        .unwrap_or_default();
    for conflict in &plan.conflicts {
        if let Some(choice) = conflict.selected.clone() {
            merge_choices.retain(|item| item.path != conflict.path);
            merge_choices.push(MergeChoice {
                path: conflict.path.clone(),
                choice,
                result_sha256: plan
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.destination == conflict.path
                            || operation.resolution.as_deref() == conflict.selected.as_deref()
                    })
                    .and_then(|operation| {
                        journal
                            .operations
                            .iter()
                            .find(|entry| entry.id == operation.id)
                    })
                    .and_then(|entry| entry.after_sha256.clone()),
            });
        }
    }
    local_modifications.sort_by(|left, right| left.path.cmp(&right.path));
    local_modifications.dedup_by(|left, right| left.path == right.path);
    let mut rollback_records = previous_lock
        .map(|lock| lock.rollback_records.clone())
        .unwrap_or_default();
    let rollback_record = format!(
        "transactions/{}/rollback-record.json",
        journal.transaction_id
    );
    if !rollback_records.contains(&rollback_record) {
        rollback_records.push(rollback_record);
    }
    let now = Utc::now().to_rfc3339();
    let installed_at = previous_lock
        .map(|lock| lock.installed_at.clone())
        .unwrap_or_else(|| now.clone());
    let updated_at = previous_lock.map(|_| now);
    Ok(InstallationLock {
        schema_version: crate::migrations::CURRENT_LOCK_SCHEMA.into(),
        project_id: plan.project_id.clone(),
        script_prefix: plan
            .script_prefix
            .clone()
            .or_else(|| previous_lock.and_then(|lock| lock.script_prefix.clone())),
        primary_namespace: plan
            .primary_namespace
            .clone()
            .or_else(|| previous_lock.and_then(|lock| lock.primary_namespace.clone())),
        installed_at,
        updated_at,
        source: LockSourceIdentity {
            repository: plan.source.repository.clone(),
            mode: plan.source.mode,
            revision: plan.source.resolved_revision.clone(),
            requested_ref: plan.source.requested_ref.clone(),
            release: plan.source.release.clone(),
            manifest_sha256: plan.source.manifest_sha256.clone(),
            manifest_origin: plan.source.manifest_origin.clone(),
        },
        ai_provider: plan.ai_provider.clone(),
        ai_model: plan.ai_model.clone(),
        ai_reasoning_effort: plan.ai_reasoning_effort.clone(),
        ai_endpoint: plan.ai_endpoint.clone(),
        ai_optimization_profile: plan.ai_optimization_profile.clone(),
        primary_coding_environment: plan.primary_coding_environment.clone(),
        additional_coding_environments: plan.additional_coding_environments.clone(),
        flatten_chat_sources: plan.flatten_chat_sources,
        // Managed removal is deliberately available without provider
        // authentication. Preserve a prior non-secret analysis record when
        // one exists so a removal lock remains useful for audit, while the
        // schemas also allow the explicit null state for legacy/partial locks.
        codex_analysis: plan
            .codex_analysis
            .clone()
            .or_else(|| previous_lock.and_then(|lock| lock.codex_analysis.clone())),
        wiki_required_pages: plan.wiki_required_pages.clone(),
        wiki_metadata: plan.wiki_metadata.clone(),
        components,
        files,
        merge_choices,
        optional_workflows,
        portrait_pipeline: plan
            .portrait_pipeline
            .clone()
            .or_else(|| previous_lock.and_then(|lock| lock.portrait_pipeline.clone())),
        local_modifications,
        rollback_records,
    })
}

fn remove_local_modification(items: &mut Vec<LocalModification>, path: &str) {
    items.retain(|item| item.path != path);
}

fn record_local_modification(
    items: &mut Vec<LocalModification>,
    operation: &PlanOperation,
    installed_sha256: &str,
) {
    let current_sha256 = operation
        .local_sha256
        .clone()
        .unwrap_or_else(|| installed_sha256.to_string());
    remove_local_modification(items, &operation.destination);
    items.push(LocalModification {
        path: operation.destination.clone(),
        installed_sha256: installed_sha256.to_string(),
        current_sha256,
        detected_at: Utc::now().to_rfc3339(),
    });
}

fn lock_component_state(state: String) -> String {
    match state.as_str() {
        "installed"
        | "incomplete"
        | "not_selected"
        | "planned_unavailable"
        | "unsupported_platform"
        | "removed" => state,
        "ready" | "selected_pending" => "installed".into(),
        _ => "incomplete".into(),
    }
}

fn regular_file_hash(path: &Path) -> Result<Option<String>, AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link_metadata(&metadata) => Err(AppError::PathSecurity(format!(
            "refusing to inspect a rollback destination link: {}",
            path.display()
        ))),
        Ok(metadata) if metadata.is_file() => Ok(Some(sha256_file(path)?)),
        Ok(_) => Err(AppError::PathSecurity(format!(
            "rollback destination is not a regular file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn rollback_destination_is_restored(
    operation: &JournalOperation,
    project_directory: Option<&RootedDir>,
    journal: &TransactionJournal,
    backups: &JournalBackups,
) -> Result<bool, AppError> {
    let current = rollback_live_hash(operation, project_directory)?;
    match operation.rollback {
        Some(RollbackAction::None) | None => Ok(true),
        Some(RollbackAction::RemoveCreated) => Ok(current.is_none()),
        Some(RollbackAction::RestoreBackup | RollbackAction::ReverseMerge) => {
            // Local bytes that changed after review are kept at the
            // destination, either by the forward apply or by a rollback that
            // moved its quarantine back. Once no quarantine remains, those
            // bytes are the restored state and must not be replaced.
            if let Some(kept) = operation.quarantine_sha256.as_deref() {
                if operation.backup_sha256.as_deref() != Some(kept)
                    && current.as_deref() == Some(kept)
                {
                    if let Some(leaf) =
                        journaled_quarantine_leaf(journal.transaction_id, operation)?
                    {
                        let quarantine_present =
                            match existing_operation_target(project_directory, operation)? {
                                Some(target) => target
                                    .dir()
                                    .exists(&quarantine_relative(&target.relative, &leaf)?)?,
                                None => false,
                            };
                        if !quarantine_present {
                            return Ok(true);
                        }
                    }
                }
            }
            let Some(backup_path) = operation.backup_path.as_ref() else {
                return Err(AppError::Transaction(format!(
                    "rollback backup metadata is missing for {}",
                    operation.destination
                )));
            };
            let backup_leaf = format!("{}.bak", operation.id);
            let (backup_directory, backup_leaf) = backups.resolve(backup_path, &backup_leaf)?;
            if !backup_directory.is_regular_file(backup_leaf)? {
                return Err(AppError::PathSecurity(
                    "rollback backup is not a regular file".into(),
                ));
            }
            let expected = operation
                .backup_sha256
                .clone()
                .unwrap_or(backup_directory.hash_file(backup_leaf)?);
            let executable_matches = match (operation.before_executable, current.as_ref()) {
                (Some(expected), Some(_)) => {
                    rollback_live_executable(operation, project_directory)? == Some(expected)
                }
                (Some(_), None) => false,
                (None, _) => true,
            };
            Ok(current.as_deref() == Some(expected.as_str()) && executable_matches)
        }
    }
}

/// Live hash of a rollback destination, read through the retained project
/// capability or through the external parent bound to the journal. A
/// missing unbound external parent reads as absent; a missing bound parent of
/// an operation that may have changed its destination is an error.
fn rollback_live_hash(
    operation: &JournalOperation,
    project_directory: Option<&RootedDir>,
) -> Result<Option<String>, AppError> {
    if operation.external {
        match existing_operation_target(None, operation)? {
            Some(target) => target.hash(),
            None => Ok(None),
        }
    } else {
        let project = project_directory.ok_or_else(|| {
            AppError::PathSecurity("rollback has no retained project-root handle".into())
        })?;
        let exists = project.exists(&operation.destination)?;
        let is_file = project.is_regular_file(&operation.destination)?;
        if exists && !is_file {
            return Err(AppError::PathSecurity(format!(
                "rollback destination is not a regular file: {}",
                operation.destination
            )));
        }
        is_file
            .then(|| project.hash_file(&operation.destination))
            .transpose()
    }
}

fn rollback_live_executable(
    operation: &JournalOperation,
    project_directory: Option<&RootedDir>,
) -> Result<Option<bool>, AppError> {
    if operation.external {
        let target = existing_operation_target(None, operation)?;
        Ok(target_hash_and_executable(target.as_ref())?.1)
    } else {
        #[cfg(unix)]
        {
            let project = project_directory.ok_or_else(|| {
                AppError::PathSecurity("rollback has no retained project-root handle".into())
            })?;
            project.observed_executable(&operation.destination)
        }
        #[cfg(not(unix))]
        {
            let _ = project_directory;
            Ok(None)
        }
    }
}

fn rollback_operation_is_actionable(
    parent: &TransactionJournal,
    operation: &JournalOperation,
) -> bool {
    (matches!(
        operation.status.as_str(),
        "rollback_applying" | "applying" | "applied" | "verified"
    ) || (parent.transaction_kind == "rollback" && operation.status == "rolled_back"))
        && !matches!(
            operation.action,
            Some(OperationAction::Skip | OperationAction::External) | None
        )
        && !matches!(operation.rollback, Some(RollbackAction::None) | None)
}

fn new_rollback_journal(
    parent: &TransactionJournal,
    transaction_id: Uuid,
    project_root: &Path,
) -> TransactionJournal {
    let now = Utc::now().to_rfc3339();
    TransactionJournal {
        schema_version: crate::migrations::CURRENT_JOURNAL_SCHEMA.into(),
        transaction_id,
        transaction_kind: "rollback".into(),
        parent_transaction_id: Some(parent.transaction_id),
        rollback_transaction_id: None,
        result_lock_sha256: None,
        result_lock_exists: None,
        rollback_record_sha256: None,
        project_id: parent.project_id.clone(),
        project_root: project_root.display().to_string(),
        project_root_lifecycle: parent.project_root_lifecycle.clone(),
        primary_coding_environment: parent.primary_coding_environment.clone(),
        additional_coding_environments: parent.additional_coding_environments.clone(),
        state: "preflight".into(),
        created_at: now.clone(),
        updated_at: now,
        last_checkpoint: "rollback-preflight".into(),
        plan_sha256: parent.plan_sha256.clone(),
        stages: TRANSACTION_STAGES
            .iter()
            .map(|id| StageCheckpoint {
                id: (*id).into(),
                status: "pending".into(),
                started_at: None,
                completed_at: None,
                evidence: vec![],
            })
            .collect(),
        operations: parent
            .operations
            .iter()
            .rev()
            .map(|operation| {
                let actionable = rollback_operation_is_actionable(parent, operation);
                let desired_sha256 = if actionable
                    && !matches!(operation.rollback, Some(RollbackAction::RemoveCreated))
                {
                    operation.before_sha256.clone()
                } else {
                    None
                };
                JournalOperation {
                    id: format!("rollback-{}", operation.id),
                    status: if actionable {
                        "pending".into()
                    } else {
                        "rolled_back".into()
                    },
                    destination: operation.destination.clone(),
                    ownership: operation.ownership,
                    component_id: operation.component_id.clone(),
                    source_path: operation.source_path.clone(),
                    source_size: operation.source_size,
                    action: if actionable {
                        Some(if desired_sha256.is_some() {
                            OperationAction::Replace
                        } else {
                            OperationAction::DeleteManaged
                        })
                    } else {
                        Some(OperationAction::Skip)
                    },
                    location_scope: operation.location_scope.clone(),
                    external: operation.external,
                    backup_path: None,
                    before_sha256: None,
                    before_executable: None,
                    expected_sha256: desired_sha256.clone(),
                    source_sha256: desired_sha256.clone(),
                    result_sha256: desired_sha256,
                    expected_executable: operation.before_executable,
                    rollback: if actionable {
                        Some(RollbackAction::RestoreBackup)
                    } else {
                        Some(RollbackAction::None)
                    },
                    rollback_source_path: operation.backup_path.clone(),
                    resolution: operation.resolution.clone(),
                    backup_sha256: None,
                    staged_sha256: None,
                    after_sha256: None,
                    after_exists: None,
                    after_executable: None,
                    quarantine_leaf: None,
                    quarantine_sha256: None,
                    // The inverse rollback reopens the same bound parent.
                    external_parent_identity: operation.external_parent_identity.clone(),
                }
            })
            .collect(),
        created_directories: parent.created_directories.clone(),
        recovery: RecoveryState {
            resume_allowed: false,
            rollback_allowed: false,
            discard_staging_allowed: false,
            project_apply_started: true,
            recommended_action: "inspect".into(),
        },
        git_initialized: false,
        git_remote_added_name: None,
        git_remote_added_url: None,
        previous_lock_backup_path: None,
        previous_lock_sha256: None,
        checkpoint_sequence: None,
        // Bound by the caller once the transaction's storage is open.
        app_data_identity: None,
        error: None,
    }
}

fn rollback_operation_destination(
    project_root: &Path,
    operation: &JournalOperation,
) -> Result<PathBuf, AppError> {
    if operation.external {
        validate_external_destination(&operation.destination)
    } else {
        safe_join(project_root, &operation.destination)
    }
}

fn rollback_lock_hash(
    project_root: &Path,
    project_directory: Option<&RootedDir>,
) -> Result<Option<String>, AppError> {
    const LOCK_RELATIVE: &str = ".hoi4-mod-setup/install.lock.json";
    if let Some(project) = project_directory {
        return project
            .is_regular_file(LOCK_RELATIVE)?
            .then(|| project.hash_file(LOCK_RELATIVE))
            .transpose();
    }
    let lock_path = safe_join(project_root, LOCK_RELATIVE)?;
    regular_file_hash(&lock_path)
}

/// Validate the lock before any rollback file is touched. A rollback may only
/// start when the live lock is either the transaction's recorded result or the
/// predecessor state already restored by an interrupted retry. This prevents
/// a later user/tool edit from being overwritten after the first file restore.
fn validate_rollback_lock_precondition(
    project_root: &Path,
    parent: &TransactionJournal,
    project_directory: Option<&RootedDir>,
) -> Result<(), AppError> {
    let current = rollback_lock_hash(project_root, project_directory)?;
    if parent.transaction_kind == "rollback" {
        let expected =
            match (
                parent.result_lock_exists,
                parent.result_lock_sha256.as_deref(),
            ) {
                (Some(true), Some(hash)) => Some(hash),
                (Some(false), None) => None,
                _ => return Err(AppError::Transaction(
                    "rollback journal has no exact result-lock evidence; manual review is required"
                        .into(),
                )),
            };
        if current.as_deref() != expected {
            return Err(AppError::Transaction(
                "installation lock changed after rollback; refusing inverse rollback".into(),
            ));
        }
        return Ok(());
    }

    let result = parent.result_lock_sha256.as_deref();
    if result.is_some_and(|expected| current.as_deref() == Some(expected)) {
        return Ok(());
    }
    let predecessor = parent.previous_lock_sha256.as_deref();
    let stage_incomplete = parent
        .stages
        .get(11)
        .is_none_or(|stage| stage.status != "complete");
    let rollback_retry = parent.state == "rolling_back";
    if (stage_incomplete || rollback_retry) && current.as_deref() == predecessor {
        return Ok(());
    }
    if (stage_incomplete || rollback_retry) && predecessor.is_none() && current.is_none() {
        return Ok(());
    }
    if result.is_none() && predecessor.is_none() && current.is_none() {
        return Ok(());
    }
    Err(AppError::Transaction(
        "installation lock changed outside the transaction; refusing rollback".into(),
    ))
}

fn capture_rollback_lock_backup(
    project_directory: Option<&RootedDir>,
    backup_root: &Path,
    backup_directory: &RootedDir,
    rollback: &mut TransactionJournal,
    parent: &TransactionJournal,
) -> Result<(), AppError> {
    const LOCK_RELATIVE: &str = ".hoi4-mod-setup/install.lock.json";
    let Some(project) = project_directory else {
        rollback.previous_lock_backup_path = None;
        rollback.previous_lock_sha256 = None;
        return Ok(());
    };
    let backup_leaf = "install.lock.json.bak";
    let backup_path = backup_root.join(backup_leaf);
    if path_has_link_component(&backup_path) {
        return Err(AppError::PathSecurity(
            "rollback lock backup path contains a symlink or junction".into(),
        ));
    }
    if project.exists(LOCK_RELATIVE)? {
        if !project.is_regular_file(LOCK_RELATIVE)? {
            return Err(AppError::PathSecurity(
                "installation lock is not a regular file".into(),
            ));
        }
        let current_hash = project.hash_file(LOCK_RELATIVE)?;
        if backup_directory.exists(backup_leaf)? {
            if !backup_directory.is_regular_file(backup_leaf)? {
                return Err(AppError::PathSecurity(
                    "rollback lock backup is not a regular file".into(),
                ));
            }
            let backup_hash = backup_directory.hash_file(backup_leaf)?;
            if rollback.previous_lock_sha256.as_deref() != Some(backup_hash.as_str()) {
                return Err(AppError::Transaction(
                    "rollback lock backup checksum changed before apply".into(),
                ));
            }
            let restored_hash = parent.previous_lock_sha256.as_deref();
            if current_hash != backup_hash && restored_hash != Some(current_hash.as_str()) {
                return Err(AppError::Transaction(
                    "rollback lock changed after the rollback checkpoint".into(),
                ));
            }
        } else if !project.copy_file_atomic_noreplace_to(
            LOCK_RELATIVE,
            backup_directory,
            backup_leaf,
        )? {
            return Err(AppError::Transaction(
                "rollback lock backup name was taken; refusing to replace it".into(),
            ));
        }
        rollback.previous_lock_backup_path = Some(backup_path.display().to_string());
        rollback.previous_lock_sha256 = Some(backup_directory.hash_file(backup_leaf)?);
    } else {
        rollback.previous_lock_backup_path = None;
        rollback.previous_lock_sha256 = None;
    }
    Ok(())
}

/// Create or reopen the child rollback journal and capture its inverse
/// backups. The child's transaction and backup directories are created
/// through the parent's retained application-data root and bound into the
/// child journal; a retry requires the same directories.
fn prepare_rollback_transaction(
    project_root: &Path,
    parent: &TransactionJournal,
    app: &AppDataRoot,
    project_directory: Option<&RootedDir>,
) -> Result<(TransactionJournal, TransactionStore), AppError> {
    let transaction_id = parent.rollback_transaction_id.unwrap_or_else(Uuid::new_v4);
    let store = app.transaction_store(transaction_id, true)?;
    let mut rollback = if store.directory.exists(JOURNAL_FILE)? {
        let journal = app.read_bound_journal(&store)?;
        if journal.transaction_id != transaction_id
            || journal.transaction_kind != "rollback"
            || journal.parent_transaction_id != Some(parent.transaction_id)
        {
            return Err(AppError::Transaction(
                "rollback transaction identity does not match its parent journal".into(),
            ));
        }
        journal
    } else {
        let mut journal = new_rollback_journal(parent, transaction_id, project_root);
        journal.app_data_identity = Some(app.bind_new_storage(&store)?);
        store.write_json(JOURNAL_FILE, &journal)?;
        journal
    };
    let backup_root = app.area_path(BACKUPS_AREA, transaction_id);
    let backup_directory = open_journal_area(app, &mut rollback, BACKUPS_AREA, transaction_id)?;
    persist_journal(&store, &mut rollback)?;

    validate_rollback_lock_precondition(project_root, parent, project_directory)?;
    capture_rollback_lock_backup(
        project_directory,
        &backup_root,
        &backup_directory,
        &mut rollback,
        parent,
    )?;
    compact_operation_checkpoints(&store, &mut rollback)?;

    let mut checkpointed = 0usize;
    for index in 0..rollback.operations.len() {
        let operation = rollback.operations[index].clone();
        if operation.status == "rolled_back" || operation.action == Some(OperationAction::Skip) {
            continue;
        }
        if operation.quarantine_leaf.is_some() {
            // This rollback step already reached its quarantine intent. Its
            // inverse backup evidence was compacted before that point, and the
            // live destination may be quarantined, so it is not re-captured.
            continue;
        }
        if operation.status == "rollback_applying" {
            if let Some(backup_path) = operation.backup_path.as_ref() {
                let backup_leaf = format!("{}.bak", operation.id);
                let expected_backup = backup_root.join(&backup_leaf);
                let supplied_backup = PathBuf::from(backup_path);
                let matches = if cfg!(target_os = "windows") {
                    supplied_backup
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&expected_backup.to_string_lossy())
                } else {
                    supplied_backup == expected_backup
                };
                if !matches {
                    return Err(AppError::PathSecurity(
                        "rollback backup path is outside the rollback transaction root".into(),
                    ));
                }
                // The retry backup is checked through the retained, bound
                // rollback backup directory, never by reopening its path.
                if !backup_directory.exists(&backup_leaf)? {
                    return Err(AppError::Transaction(format!(
                        "rollback retry backup is unavailable for {}",
                        operation.destination
                    )));
                }
                if !backup_directory.is_regular_file(&backup_leaf)? {
                    return Err(AppError::PathSecurity(
                        "rollback retry backup is not a regular file".into(),
                    ));
                }
                let expected_hash = operation.backup_sha256.as_deref().ok_or_else(|| {
                    AppError::Transaction(format!(
                        "rollback retry backup has no checksum: {}",
                        operation.destination
                    ))
                })?;
                if backup_directory.hash_file(&backup_leaf)? != expected_hash {
                    return Err(AppError::Transaction(format!(
                        "rollback retry backup checksum mismatch: {}",
                        operation.destination
                    )));
                }
            } else if operation.before_sha256.is_some() || operation.backup_sha256.is_some() {
                return Err(AppError::Transaction(format!(
                    "rollback retry is missing its inverse backup: {}",
                    operation.destination
                )));
            }
            continue;
        }
        rollback_operation_destination(project_root, &operation)?;
        let backup_leaf = format!("{}.bak", operation.id);
        let backup = backup_root.join(&backup_leaf);
        if path_has_link_component(&backup) {
            return Err(AppError::PathSecurity(
                "rollback backup path contains a symlink or junction".into(),
            ));
        }
        // The live destination is read through the retained project
        // capability or the bound external parent, and its inverse backup is
        // copied and hashed in one pass from one opened handle, so the
        // recorded before and backup hashes describe the same bytes.
        if !operation.external && project_directory.is_none() {
            let root_present = match fs::symlink_metadata(project_root) {
                Ok(_) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.into()),
            };
            if root_present {
                return Err(AppError::PathSecurity(format!(
                    "rollback has no retained project-root handle for {}",
                    operation.destination
                )));
            }
        }
        let target = existing_operation_target(project_directory, &operation)?;
        if target.is_none() && operation.external && operation.external_parent_identity.is_some() {
            // Only a step whose parent operation may have changed its
            // destination reaches this capture. The child record is still
            // `pending`, so the check is made here rather than from its status.
            return Err(missing_bound_external_parent(&operation.destination));
        }
        let live_state = match target.as_ref() {
            Some(target) if target.dir().exists(&target.relative)? => {
                if !target.dir().is_regular_file(&target.relative)? {
                    return Err(AppError::Transaction(format!(
                        "rollback destination is not a regular file: {}",
                        operation.destination
                    )));
                }
                Some(target)
            }
            _ => None,
        };
        if let Some(target) = live_state {
            let captured_hash = if backup_directory.exists(&backup_leaf)? {
                if !backup_directory.is_regular_file(&backup_leaf)? {
                    return Err(AppError::PathSecurity(
                        "rollback backup is not a regular file".into(),
                    ));
                }
                let existing = backup_directory.hash_file(&backup_leaf)?;
                if existing != target.dir().hash_file(&target.relative)? {
                    return Err(AppError::Transaction(format!(
                        "rollback backup changed before apply: {}",
                        operation.destination
                    )));
                }
                existing
            } else {
                target
                    .dir()
                    .copy_file_atomic_noreplace_hashed_to(
                        &target.relative,
                        &backup_directory,
                        &backup_leaf,
                    )?
                    .ok_or_else(|| {
                        AppError::Transaction(format!(
                            "rollback backup name was taken during backup; refusing to replace it: {}",
                            operation.destination
                        ))
                    })?
            };
            if backup_directory.hash_file(&backup_leaf)? != captured_hash {
                return Err(AppError::Transaction(format!(
                    "rollback backup verification failed after rooted copy: {}",
                    operation.destination
                )));
            }
            #[cfg(unix)]
            let before_executable = target.dir().observed_executable(&target.relative)?;
            #[cfg(not(unix))]
            let before_executable = None;
            rollback.operations[index].before_sha256 = Some(captured_hash.clone());
            rollback.operations[index].before_executable = before_executable;
            rollback.operations[index].after_exists = Some(true);
            rollback.operations[index].backup_path = Some(backup.display().to_string());
            rollback.operations[index].backup_sha256 = Some(captured_hash);
        } else {
            rollback.operations[index].before_sha256 = None;
            rollback.operations[index].after_exists = Some(false);
            rollback.operations[index].backup_path = None;
            rollback.operations[index].backup_sha256 = None;
        }
        rollback.last_checkpoint = format!("rollback-backup-{}", operation.id);
        append_operation_checkpoint(&store, &mut rollback, index)?;
        checkpointed += 1;
        if checkpointed % OPERATION_CHECKPOINT_BATCH == 0 {
            compact_operation_checkpoints(&store, &mut rollback)?;
        }
    }
    rollback.state = "applying".into();
    rollback.last_checkpoint = "rollback-backup-complete".into();
    if let Some(stage) = rollback.stages.get_mut(5) {
        stage.status = "complete".into();
        stage.completed_at = Some(Utc::now().to_rfc3339());
    }
    compact_operation_checkpoints(&store, &mut rollback)?;
    Ok((rollback, store))
}

fn persist_rollback_checkpoint(
    rollback: &mut TransactionJournal,
    rollback_store: &TransactionStore,
    parent_operation_id: &str,
    status: &str,
    checkpoint: &str,
) -> Result<(), AppError> {
    write_rollback_checkpoint(
        rollback,
        rollback_store,
        parent_operation_id,
        status,
        checkpoint,
        false,
    )
}

/// Record the child rollback status, synced when `durable` is set.
fn write_rollback_checkpoint(
    rollback: &mut TransactionJournal,
    rollback_store: &TransactionStore,
    parent_operation_id: &str,
    status: &str,
    checkpoint: &str,
    durable: bool,
) -> Result<(), AppError> {
    let operation_index = rollback
        .operations
        .iter()
        .position(|operation| operation.id == format!("rollback-{parent_operation_id}"))
        .ok_or_else(|| AppError::Transaction("rollback checkpoint operation is missing".into()))?;
    let operation = &mut rollback.operations[operation_index];
    operation.status = status.into();
    if status == "rolled_back" {
        operation.after_sha256 = operation.expected_sha256.clone();
        operation.after_exists = Some(operation.expected_sha256.is_some());
        operation.after_executable = operation.expected_executable;
    }
    rollback.last_checkpoint = checkpoint.into();
    if durable {
        persist_operation_checkpoint_batch(rollback_store, rollback, &[operation_index])
    } else {
        append_operation_checkpoint(rollback_store, rollback, operation_index)
    }
}

/// Settle the quarantine of one interrupted rollback step. A verified
/// quarantine beside the completed destination is released; a quarantine
/// beside an absent destination moves back so the step starts again from
/// known bytes. Any other state keeps both files for review.
fn settle_rollback_step_quarantine(
    project_directory: Option<&RootedDir>,
    operation: &JournalOperation,
    child_transaction_id: Uuid,
    child: &JournalOperation,
) -> Result<(), AppError> {
    let Some(child_leaf) = journaled_quarantine_leaf(child_transaction_id, child)? else {
        return Ok(());
    };
    let Some(target) = existing_operation_target(project_directory, operation)? else {
        return Ok(());
    };
    let restore_target = if operation.backup_path.is_some() {
        operation
            .backup_sha256
            .as_deref()
            .or(operation.before_sha256.as_deref())
    } else {
        None
    };
    let mut completed = vec![restore_target];
    if let Some(forward) = operation.quarantine_sha256.as_deref() {
        completed.push(Some(forward));
    }
    settle_interrupted_quarantine(
        &target,
        &child_leaf,
        child.quarantine_sha256.as_deref(),
        &completed,
    )
}

/// Record a rollback step that restored bytes other than the planned
/// predecessor, such as local bytes moved back from a forward quarantine.
fn persist_rollback_result(
    rollback: &mut TransactionJournal,
    rollback_store: &TransactionStore,
    index: usize,
    observed: Option<String>,
    checkpoint: &str,
) -> Result<(), AppError> {
    let operation = rollback
        .operations
        .get_mut(index)
        .ok_or_else(|| AppError::Transaction("rollback checkpoint operation is missing".into()))?;
    operation.status = "rolled_back".into();
    operation.after_executable = if observed == operation.expected_sha256 {
        operation.expected_executable
    } else {
        None
    };
    operation.after_exists = Some(observed.is_some());
    operation.after_sha256 = observed;
    rollback.last_checkpoint = checkpoint.into();
    persist_operation_checkpoint_batch(rollback_store, rollback, &[index])
}

fn ensure_project_root_for_inverse_rollback(
    project_root: &Path,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
    rollback_journal: &mut TransactionJournal,
    rollback_store: &TransactionStore,
) -> Result<(), AppError> {
    if journal.transaction_kind != "rollback"
        || journal.project_root_lifecycle.mode != ProjectRootMode::CreateLeaf
    {
        return Ok(());
    }

    let checkpoint = journal.project_root_lifecycle.checkpoint.clone();
    match checkpoint.as_str() {
        "removed" | "applying" => {
            validate_project_root_lifecycle_identity(
                project_root,
                &journal.project_root_lifecycle,
            )?;
            let (validated, exists) = validate_project_root_or_destination(project_root)?;
            if !same_root_path(&validated, project_root) {
                return Err(AppError::PathSecurity(
                    "inverse rollback project root changed after review".into(),
                ));
            }
            if checkpoint == "removed" && exists {
                return Err(AppError::Transaction(
                    "new content appeared at the removed project root; refusing inverse rollback"
                        .into(),
                ));
            }
            if checkpoint == "applying" && exists {
                let root = RootedDir::open(&validate_project_root(project_root)?)?;
                if !root.read_dir_names()?.is_empty() {
                    return Err(AppError::Transaction(
                        "content appeared while recreating the project root; refusing inverse rollback"
                            .into(),
                    ));
                }
            }
            if !exists {
                journal.project_root_lifecycle.checkpoint = "applying".into();
                journal.project_root_lifecycle.observed_exists = false;
                journal.last_checkpoint = "inverse-rollback-project-root-intent".into();
                rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
                rollback_journal.last_checkpoint = "inverse-rollback-project-root-intent".into();
                persist_journal(store, journal)?;
                persist_journal(rollback_store, rollback_journal)?;
                maybe_abort_for_test("before_inverse_project_root_create");
                let parent_path = journal
                    .project_root_lifecycle
                    .canonical_parent
                    .as_deref()
                    .ok_or_else(|| {
                        AppError::PathSecurity("inverse rollback root has no parent".into())
                    })?;
                let leaf = journal
                    .project_root_lifecycle
                    .leaf
                    .as_deref()
                    .ok_or_else(|| {
                        AppError::PathSecurity("inverse rollback root has no leaf".into())
                    })?;
                let parent = RootedDir::open(Path::new(parent_path))?;
                let expected_parent = journal.project_root_lifecycle.parent_identity.as_deref();
                if Some(parent.identity_token()?.as_str()) != expected_parent {
                    return Err(AppError::PathSecurity(
                        "project root parent changed before inverse rollback".into(),
                    ));
                }
                let created = parent.create_dir(leaf)?;
                journal.project_root_lifecycle.root_identity = Some(created.identity_token()?);
                maybe_abort_for_test("after_inverse_project_root_create");
            }
            let root = RootedDir::open(project_root)?;
            journal.project_root_lifecycle.root_identity = Some(root.identity_token()?);
            journal.project_root_lifecycle.checkpoint = "created".into();
            journal.project_root_lifecycle.created_by_transaction = true;
            journal.project_root_lifecycle.observed_exists = true;
            journal.project_root_lifecycle.cleanup_result = None;
            journal.last_checkpoint = "inverse-rollback-project-root-created".into();
            rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
            rollback_journal.last_checkpoint = "inverse-rollback-project-root-created".into();
            persist_journal(store, journal)?;
            persist_journal(rollback_store, rollback_journal)
        }
        "created" | "retained_user_content" => {
            validate_project_root(project_root)?;
            Ok(())
        }
        other => Err(AppError::Transaction(format!(
            "project root lifecycle cannot be restored from checkpoint {other}"
        ))),
    }
}

#[cfg(test)]
fn maybe_abort_for_test(checkpoint: &str) {
    if std::env::var("HOI4_MOD_SETUP_TEST_ABORT_AT")
        .ok()
        .as_deref()
        == Some(checkpoint)
    {
        std::process::abort();
    }
}

#[cfg(not(test))]
fn maybe_abort_for_test(_checkpoint: &str) {}

pub fn rollback_transaction(
    project_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    let project_root = validate_journal_project_root(project_root, journal, journal_path)?;
    // The journal's storage is reopened through the retained application
    // data root and must still be the storage it recorded; the caller read
    // the journal by path, so a copy in a swapped-in directory is refused
    // here, before the first write.
    let app = AppDataRoot::for_journal(journal_path, journal)?;
    let transaction_store = app.transaction_store(journal.transaction_id, false)?;
    verify_app_data_binding(journal, Some(&app.directory), &transaction_store.directory)?;
    let backups = JournalBackups::open(&app, journal)?;
    let store = &transaction_store;
    let mut project_directory = match journal.project_root_lifecycle.mode {
        ProjectRootMode::Existing => Some(open_bound_project_root(
            &project_root,
            &journal.project_root_lifecycle,
        )?),
        ProjectRootMode::CreateLeaf
            if journal.project_root_lifecycle.root_identity.is_some()
                && fs::symlink_metadata(&project_root).is_ok() =>
        {
            Some(open_bound_project_root(
                &project_root,
                &journal.project_root_lifecycle,
            )?)
        }
        ProjectRootMode::CreateLeaf => None,
    };
    if !journal.recovery.rollback_allowed {
        return Err(AppError::Transaction(
            "rollback is not allowed by the journal".into(),
        ));
    }
    let rollback_transaction_id = journal.rollback_transaction_id.unwrap_or_else(Uuid::new_v4);
    let project_apply_started = journal.recovery.project_apply_started;
    journal.rollback_transaction_id = Some(rollback_transaction_id);
    journal.state = "rolling_back".into();
    journal.recovery = RecoveryState {
        resume_allowed: false,
        rollback_allowed: true,
        discard_staging_allowed: false,
        project_apply_started,
        recommended_action: "rollback".into(),
    };
    compact_operation_checkpoints(store, journal)?;
    if let Some(project) = project_directory.as_ref() {
        settle_journal_lock_quarantines(project, journal)?;
    }
    let (mut rollback_journal, rollback_store) =
        prepare_rollback_transaction(&project_root, journal, &app, project_directory.as_ref())?;
    maybe_abort_for_test(if journal.transaction_kind == "rollback" {
        "inverse_rollback_after_backup"
    } else {
        "rollback_after_backup"
    });
    ensure_project_root_for_inverse_rollback(
        &project_root,
        journal,
        store,
        &mut rollback_journal,
        &rollback_store,
    )?;
    if project_directory.is_none()
        && journal.project_root_lifecycle.root_identity.is_some()
        && fs::symlink_metadata(&project_root).is_ok()
    {
        project_directory = Some(open_bound_project_root(
            &project_root,
            &journal.project_root_lifecycle,
        )?);
    }
    let result = (|| -> Result<(), AppError> {
        // Remove transaction-created Git metadata before restoring files. The
        // newly initialized index would otherwise record the transaction's
        // applied files as user changes and make safe Git cleanup impossible.
        if journal.git_initialized {
            if let Some(project) = project_directory.as_ref() {
                project.verify_bound_to_path()?;
            }
            crate::git::rollback_initialized_git(&project_root)?;
            if let Some(project) = project_directory.as_ref() {
                project.verify_bound_to_path()?;
            }
            journal.git_initialized = false;
            persist_journal(store, journal)?;
            rollback_journal.last_checkpoint = "rollback-git-cleanup".into();
            persist_journal(&rollback_store, &mut rollback_journal)?;
        } else if let (Some(name), Some(url)) = (
            journal.git_remote_added_name.as_deref(),
            journal.git_remote_added_url.as_deref(),
        ) {
            if let Some(project) = project_directory.as_ref() {
                project.verify_bound_to_path()?;
            }
            crate::git::rollback_added_remote(&project_root, name, url)?;
            if let Some(project) = project_directory.as_ref() {
                project.verify_bound_to_path()?;
            }
            journal.git_remote_added_name = None;
            journal.git_remote_added_url = None;
            persist_journal(store, journal)?;
            rollback_journal.last_checkpoint = "rollback-git-cleanup".into();
            persist_journal(&rollback_store, &mut rollback_journal)?;
        }
        let operation_count = journal.operations.len();
        for index in (0..operation_count).rev() {
            let processed = operation_count - 1 - index;
            let batch_complete = (processed + 1) % OPERATION_CHECKPOINT_BATCH == 0 || index == 0;
            if processed % OPERATION_INTENT_BATCH == 0 {
                let batch_start = index.saturating_sub(OPERATION_INTENT_BATCH - 1);
                let intent_indices = (batch_start..=index)
                    .rev()
                    .filter(|candidate| {
                        let operation = &journal.operations[*candidate];
                        (journal.transaction_kind == "rollback"
                            || operation.status != "rolled_back")
                            && rollback_operation_is_actionable(journal, operation)
                    })
                    .collect::<Vec<_>>();
                for candidate in &intent_indices {
                    journal.operations[*candidate].status = "rollback_applying".into();
                }
                journal.last_checkpoint =
                    format!("rollback-batch-intent-{batch_start:05}-{:05}", index + 1);
                persist_operation_checkpoint_batch(store, journal, &intent_indices)?;
            }
            let operation = journal.operations[index].clone();
            if journal.transaction_kind != "rollback" && operation.status == "rolled_back" {
                if let Some(child_operation) = rollback_journal
                    .operations
                    .iter()
                    .find(|entry| entry.id == format!("rollback-{}", operation.id))
                {
                    // A stop after both rollback records but before the
                    // displaced bytes were released leaves a verified
                    // quarantine beside the restored destination.
                    settle_rollback_step_quarantine(
                        project_directory.as_ref(),
                        &operation,
                        rollback_journal.transaction_id,
                        child_operation,
                    )?;
                    if child_operation.status != "rolled_back"
                        && !rollback_destination_is_restored(
                            &operation,
                            project_directory.as_ref(),
                            journal,
                            &backups,
                        )?
                    {
                        return Err(AppError::Transaction(format!(
                            "rollback checkpoint is not reflected on disk: {}",
                            operation.destination
                        )));
                    }
                }
                persist_rollback_checkpoint(
                    &mut rollback_journal,
                    &rollback_store,
                    &operation.id,
                    "rolled_back",
                    &format!("rollback-{}", operation.id),
                )?;
                if batch_complete {
                    compact_operation_checkpoints(store, journal)?;
                    compact_operation_checkpoints(&rollback_store, &mut rollback_journal)?;
                }
                continue;
            }
            if !(matches!(
                operation.status.as_str(),
                "rollback_applying" | "applying" | "applied" | "verified"
            ) || (journal.transaction_kind == "rollback" && operation.status == "rolled_back"))
            {
                if batch_complete {
                    compact_operation_checkpoints(store, journal)?;
                    compact_operation_checkpoints(&rollback_store, &mut rollback_journal)?;
                }
                continue;
            }
            // A skip is a durable no-op. In particular, it may represent a
            // locally modified file or an external launcher descriptor that the
            // user explicitly kept. Never remove such a destination merely
            // because the operation has a verified journal status. Legacy
            // journals without action/rollback metadata are also treated as
            // non-destructive until they are re-planned with ownership evidence.
            if matches!(
                operation.action,
                Some(OperationAction::Skip | OperationAction::External) | None
            ) || matches!(operation.rollback, Some(RollbackAction::None) | None)
            {
                journal.operations[index].status = "rolled_back".into();
                journal.last_checkpoint = format!("rollback-noop-{}", operation.id);
                append_operation_checkpoint(store, journal, index)?;
                persist_rollback_checkpoint(
                    &mut rollback_journal,
                    &rollback_store,
                    &operation.id,
                    "rolled_back",
                    &format!("rollback-noop-{}", operation.id),
                )?;
                if batch_complete {
                    compact_operation_checkpoints(store, journal)?;
                    compact_operation_checkpoints(&rollback_store, &mut rollback_journal)?;
                }
                continue;
            }
            // Validate the journaled destination before any filesystem access.
            rollback_operation_destination(&project_root, &operation)?;
            let child_id = format!("rollback-{}", operation.id);
            let child_index = rollback_journal
                .operations
                .iter()
                .position(|entry| entry.id == child_id)
                .ok_or_else(|| {
                    AppError::Transaction("rollback checkpoint operation is missing".into())
                })?;
            // An earlier rollback attempt may have stopped while this step's
            // displaced bytes were quarantined.
            settle_rollback_step_quarantine(
                project_directory.as_ref(),
                &operation,
                rollback_journal.transaction_id,
                &rollback_journal.operations[child_index],
            )?;
            if operation.status == "rollback_applying"
                && rollback_destination_is_restored(
                    &operation,
                    project_directory.as_ref(),
                    journal,
                    &backups,
                )?
            {
                // A forward quarantine may survive beside bytes that already
                // equal the restored state; never report success while it
                // hides different bytes.
                settle_forward_quarantine_beside_restored(
                    project_directory.as_ref(),
                    journal.transaction_id,
                    &operation,
                )?;
                journal.operations[index].status = "rolled_back".into();
                journal.last_checkpoint = format!("rollback-{}", operation.id);
                append_operation_checkpoint(store, journal, index)?;
                persist_rollback_checkpoint(
                    &mut rollback_journal,
                    &rollback_store,
                    &operation.id,
                    "rolled_back",
                    &format!("rollback-{}", operation.id),
                )?;
                if batch_complete {
                    compact_operation_checkpoints(store, journal)?;
                    compact_operation_checkpoints(&rollback_store, &mut rollback_journal)?;
                }
                continue;
            }
            // The forward apply stopped while this destination's displaced
            // bytes were quarantined. Those bytes are the newest bytes the
            // user saw at this path, so they are moved back instead of being
            // overwritten with the backup copy. The derived name is probed
            // even when the journal does not record it.
            {
                let forward_leaf = forward_quarantine_leaf(journal.transaction_id, &operation)?;
                if let Some(target) =
                    existing_operation_target(project_directory.as_ref(), &operation)?
                {
                    let forward_quarantine = quarantine_relative(&target.relative, &forward_leaf)?;
                    if target.dir().exists(&forward_quarantine)? {
                        if !target.dir().is_regular_file(&forward_quarantine)? {
                            return Err(AppError::PathSecurity(format!(
                                "quarantine is not a regular file: {forward_quarantine}"
                            )));
                        }
                        let held = target.dir().hash_file(&forward_quarantine)?;
                        let current = target.hash()?;
                        let installed = current.is_some()
                            && (current == operation.expected_sha256
                                || (operation.after_sha256.is_some()
                                    && current == operation.after_sha256));
                        if current.is_some() && !installed {
                            return Err(AppError::Transaction(format!(
                                "{} differs from the installed bytes while its earlier bytes are quarantined at {forward_quarantine}; both files were kept for manual review",
                                operation.destination
                            )));
                        }
                        // New bytes are placed only after the displaced bytes
                        // were verified and journaled, so beside installed
                        // bytes the quarantine must hold exactly that hash.
                        // Anything else is not a quarantine this transaction
                        // can vouch for and is never moved into place.
                        if current.is_some()
                            && operation.quarantine_sha256.as_deref() != Some(held.as_str())
                        {
                            return Err(AppError::Transaction(format!(
                                "{forward_quarantine} beside {} holds bytes that match no recorded state; both files were kept for manual review",
                                operation.destination
                            )));
                        }
                        journal.operations[index].status = "rollback_applying".into();
                        journal.operations[index].quarantine_sha256 = Some(held.clone());
                        journal.last_checkpoint =
                            format!("rollback-quarantine-restore-{}", operation.id);
                        persist_operation_checkpoint_batch(store, journal, &[index])?;
                        let held_child = mutate_live_leaf(
                            target.dir(),
                            &target.relative,
                            current.as_deref(),
                            LiveChange::MoveFrom(&forward_quarantine),
                            &quarantine_leaf_name(rollback_journal.transaction_id, &child_id),
                            Some(QuarantineJournal {
                                journal: &mut rollback_journal,
                                store: &rollback_store,
                                index: child_index,
                            }),
                            "rollback",
                            &rollback_quarantine_fault,
                            &no_live_barrier,
                        )?;
                        test_fault("rollback_after_placement")?;
                        if target.hash()?.as_deref() != Some(held.as_str()) {
                            return Err(AppError::Transaction(format!(
                                "rollback destination checksum mismatch after quarantine restore: {}",
                                operation.destination
                            )));
                        }
                        journal.operations[index].status = "rolled_back".into();
                        journal.last_checkpoint = format!("rollback-{}", operation.id);
                        persist_operation_checkpoint_batch(store, journal, &[index])?;
                        persist_rollback_result(
                            &mut rollback_journal,
                            &rollback_store,
                            child_index,
                            Some(held),
                            &format!("rollback-{}", operation.id),
                        )?;
                        if let (Some(quarantine), Some(displaced)) =
                            (held_child, current.as_deref())
                        {
                            rollback_quarantine_fault(QuarantineBoundary::BeforeRelease)?;
                            release_quarantine(target.dir(), &quarantine, displaced)?;
                        }
                        if batch_complete {
                            compact_operation_checkpoints(store, journal)?;
                            compact_operation_checkpoints(&rollback_store, &mut rollback_journal)?;
                        }
                        continue;
                    }
                }
            }
            journal.operations[index].status = "rollback_applying".into();
            journal.last_checkpoint = format!("rollback-intent-{}", operation.id);
            // One retained target serves this step's live precondition, the
            // restore or removal, and the readback. An external parent must
            // still be the directory bound to the transaction.
            if !operation.external && project_directory.is_none() {
                return Err(AppError::PathSecurity(
                    "rollback has no retained project-root handle".into(),
                ));
            }
            let step_target = if operation.backup_path.is_some() {
                Some(live_target(
                    project_directory.as_ref(),
                    operation.external,
                    &operation.destination,
                    true,
                    operation.external_parent_identity.as_deref(),
                )?)
            } else {
                existing_operation_target(project_directory.as_ref(), &operation)?
            };
            let current = match step_target.as_ref() {
                Some(target) => target.hash()?,
                None => None,
            };
            if let Some(after) = &operation.after_sha256 {
                if current.as_deref() != Some(after.as_str())
                    || operation.after_exists != Some(true)
                {
                    return Err(AppError::Transaction(format!(
                        "user changes detected after apply; refusing rollback of {}",
                        operation.destination
                    )));
                }
            } else if operation.after_exists == Some(false) && current.is_some() {
                return Err(AppError::Transaction(format!(
                    "user created a file after managed deletion; refusing rollback of {}",
                    operation.destination
                )));
            } else if operation.after_exists == Some(true) && current.is_none() {
                return Err(AppError::Transaction(format!(
                    "managed destination was deleted after apply; refusing rollback of {}",
                    operation.destination
                )));
            } else if matches!(operation.status.as_str(), "applying" | "rollback_applying") {
                // The batch intent above marks every actionable operation
                // `rollback_applying` before this point, so an unverified
                // forward operation is recognized by its missing result
                // evidence rather than by its pre-rollback status.
                //
                // Delete intent is durable before the live removal. If the
                // process stops after removal but before the observed
                // `after_exists=false` checkpoint, an absent destination plus
                // a verified predecessor backup is the intended result, not
                // an ambiguous missing file. The backup is still checked
                // below before it can be restored.
                let interrupted_delete_completed =
                    matches!(operation.action, Some(OperationAction::DeleteManaged))
                        && current.is_none()
                        && operation.before_sha256.is_some()
                        && operation.backup_path.is_some();
                if interrupted_delete_completed {
                    // Continue to verified-backup restoration below.
                } else if let Some(current) = current.as_deref() {
                    if operation.before_sha256.as_deref() != Some(current)
                        && operation.expected_sha256.as_deref() != Some(current)
                    {
                        return Err(AppError::Transaction(format!(
                            "uncertain live state after interruption; refusing rollback of {}",
                            operation.destination
                        )));
                    }
                } else if operation.before_sha256.is_some() || operation.after_exists == Some(true)
                {
                    return Err(AppError::Transaction(format!(
                        "uncertain live state after interruption; refusing rollback of {}",
                        operation.destination
                    )));
                }
            }
            let expected_restored = operation
                .backup_sha256
                .clone()
                .or_else(|| operation.before_sha256.clone());
            let mut held_rollback_quarantine: Option<(String, String)> = None;
            if let Some(backup) = &operation.backup_path {
                // The backup bytes come from the journal's retained, bound
                // backup directory, not from a reopened path.
                let backup_leaf = format!("{}.bak", operation.id);
                let (backup_directory, backup_leaf) = backups.resolve(backup, &backup_leaf)?;
                if backup_directory.is_regular_file(backup_leaf)? {
                    let actual_backup = backup_directory.hash_file(backup_leaf)?;
                    if operation
                        .backup_sha256
                        .as_deref()
                        .is_some_and(|expected| expected != actual_backup)
                        || operation
                            .before_sha256
                            .as_deref()
                            .is_some_and(|expected| expected != actual_backup)
                    {
                        return Err(AppError::Transaction(format!(
                            "backup checksum mismatch for {}",
                            operation.destination
                        )));
                    }
                    let target = step_target.as_ref().ok_or_else(|| {
                        AppError::Transaction(format!(
                            "rollback destination has no retained target: {}",
                            operation.destination
                        ))
                    })?;
                    let held = mutate_live_leaf(
                        target.dir(),
                        &target.relative,
                        current.as_deref(),
                        LiveChange::Copy {
                            source: backup_directory,
                            source_relative: backup_leaf,
                        },
                        &quarantine_leaf_name(rollback_journal.transaction_id, &child_id),
                        Some(QuarantineJournal {
                            journal: &mut rollback_journal,
                            store: &rollback_store,
                            index: child_index,
                        }),
                        "rollback",
                        &rollback_quarantine_fault,
                        &no_live_barrier,
                    )?;
                    #[cfg(unix)]
                    if let Some(executable) = operation.before_executable {
                        target.dir().set_executable(&target.relative, executable)?;
                    }
                    if let (Some(quarantine), Some(displaced)) = (held, current.clone()) {
                        held_rollback_quarantine = Some((quarantine, displaced));
                    }
                } else {
                    return Err(AppError::Transaction(format!(
                        "rollback backup is missing for {}",
                        operation.destination
                    )));
                }
            } else if let Some(target) = step_target.as_ref() {
                let held = mutate_live_leaf(
                    target.dir(),
                    &target.relative,
                    current.as_deref(),
                    LiveChange::Delete,
                    &quarantine_leaf_name(rollback_journal.transaction_id, &child_id),
                    Some(QuarantineJournal {
                        journal: &mut rollback_journal,
                        store: &rollback_store,
                        index: child_index,
                    }),
                    "rollback",
                    &rollback_quarantine_fault,
                    &no_live_barrier,
                )?;
                if let (Some(quarantine), Some(displaced)) = (held, current.clone()) {
                    held_rollback_quarantine = Some((quarantine, displaced));
                }
            }
            test_fault("rollback_after_placement")?;
            let (restored, restored_executable) = target_hash_and_executable(step_target.as_ref())?;
            if let Some(expected) = expected_restored {
                if restored.as_deref() != Some(expected.as_str()) {
                    return Err(AppError::Transaction(format!(
                        "rollback destination checksum mismatch after restore: {}",
                        operation.destination
                    )));
                }
                if let Some(expected_executable) = operation.before_executable {
                    if restored_executable != Some(expected_executable) {
                        return Err(AppError::Transaction(format!(
                            "rollback executable metadata mismatch after restore: {}",
                            operation.destination
                        )));
                    }
                }
            } else if restored.is_some() {
                return Err(AppError::Transaction(format!(
                    "rollback destination still exists after removal: {}",
                    operation.destination
                )));
            }
            journal.operations[index].status = "rolled_back".into();
            journal.last_checkpoint = format!("rollback-{}", operation.id);
            let durable = held_rollback_quarantine.is_some();
            if durable {
                persist_operation_checkpoint_batch(store, journal, &[index])?;
            } else {
                append_operation_checkpoint(store, journal, index)?;
            }
            write_rollback_checkpoint(
                &mut rollback_journal,
                &rollback_store,
                &operation.id,
                "rolled_back",
                &format!("rollback-{}", operation.id),
                durable,
            )?;
            if let Some((quarantine, displaced)) = held_rollback_quarantine.take() {
                // Both rollback records are synced; the displaced
                // post-transaction bytes also remain in the child backup.
                rollback_quarantine_fault(QuarantineBoundary::BeforeRelease)?;
                let target = step_target.as_ref().ok_or_else(|| {
                    AppError::Transaction("held rollback quarantine has no retained target".into())
                })?;
                release_quarantine(target.dir(), &quarantine, &displaced)?;
            }
            if batch_complete {
                compact_operation_checkpoints(store, journal)?;
                compact_operation_checkpoints(&rollback_store, &mut rollback_journal)?;
            }
        }
        // Every actionable operation is rolled back. A quarantine of this
        // transaction or of its rollback that still exists is a leftover,
        // for example from an intent that was never replayed.
        sweep_transaction_quarantines(
            project_directory.as_ref(),
            journal,
            Some(rollback_journal.transaction_id),
            QuarantineSweep::Rollback,
        )?;
        restore_previous_lock(project_directory.as_ref(), &project_root, journal, &backups)?;
        if let Some(project) = project_directory.as_ref() {
            cleanup_created_profile_directories_rooted(project, journal, store)?;
        }
        if journal.transaction_kind != "rollback" {
            if journal.project_root_lifecycle.mode == ProjectRootMode::CreateLeaf {
                drop(project_directory.take());
            }
            cleanup_created_project_root(
                &project_root,
                journal,
                store,
                &mut rollback_journal,
                &rollback_store,
            )?;
        }
        rollback_journal.last_checkpoint = "rollback-lock-restored".into();
        persist_journal(&rollback_store, &mut rollback_journal)?;
        if let Some(project) = project_directory.as_ref() {
            let lock_relative = ".hoi4-mod-setup/install.lock.json";
            if project.exists(lock_relative)? {
                if !project.is_regular_file(lock_relative)? {
                    return Err(AppError::PathSecurity(
                        "rollback result lock is not a regular file".into(),
                    ));
                }
                rollback_journal.result_lock_exists = Some(true);
                rollback_journal.result_lock_sha256 = Some(project.hash_file(lock_relative)?);
            } else {
                rollback_journal.result_lock_exists = Some(false);
                rollback_journal.result_lock_sha256 = None;
            }
        } else {
            rollback_journal.result_lock_exists = Some(false);
            rollback_journal.result_lock_sha256 = None;
        }
        let completed_at = Utc::now().to_rfc3339();
        for (index, stage) in rollback_journal.stages.iter_mut().enumerate() {
            stage.status = if matches!(index, 5 | 8 | 9 | 11) {
                "complete"
            } else {
                "skipped"
            }
            .into();
            stage.started_at.get_or_insert_with(|| completed_at.clone());
            stage
                .completed_at
                .get_or_insert_with(|| completed_at.clone());
        }
        let mut child_record = rollback_journal.clone();
        child_record.state = "completed".into();
        child_record.recovery = RecoveryState {
            resume_allowed: false,
            rollback_allowed: true,
            discard_staging_allowed: false,
            project_apply_started: true,
            recommended_action: "rollback".into(),
        };
        child_record.last_checkpoint = "rollback-complete".into();
        rollback_store.write_json(ROLLBACK_RECORD_FILE, &child_record)?;
        let child_record_sha256 = rollback_store.directory.hash_file(ROLLBACK_RECORD_FILE)?;
        rollback_journal.rollback_record_sha256 = Some(child_record_sha256.clone());
        rollback_journal.last_checkpoint = "rollback-record-written".into();
        persist_journal(&rollback_store, &mut rollback_journal)?;
        maybe_abort_for_test("after_rollback_child_record");
        rollback_journal = child_record;
        rollback_journal.rollback_record_sha256 = Some(child_record_sha256);
        persist_journal(&rollback_store, &mut rollback_journal)?;
        maybe_abort_for_test("after_rollback_child_complete");
        let mut parent_record = journal.clone();
        parent_record.state = "rolled_back".into();
        parent_record.recovery = RecoveryState {
            resume_allowed: false,
            rollback_allowed: false,
            discard_staging_allowed: false,
            project_apply_started: false,
            recommended_action: "none".into(),
        };
        parent_record.last_checkpoint = "rollback-complete".into();
        store.write_json(ROLLBACK_RECORD_FILE, &parent_record)?;
        let parent_record_sha256 = store.directory.hash_file(ROLLBACK_RECORD_FILE)?;
        journal.rollback_record_sha256 = Some(parent_record_sha256.clone());
        journal.last_checkpoint = "rollback-record-written".into();
        persist_journal(store, journal)?;
        maybe_abort_for_test("after_rollback_parent_record");
        *journal = parent_record;
        journal.rollback_record_sha256 = Some(parent_record_sha256);
        persist_journal(store, journal)?;
        Ok(())
    })();
    if let Err(error) = &result {
        rollback_journal.state = "interrupted".into();
        rollback_journal.error = Some(JournalError {
            code: "ROLLBACK_TRANSACTION_FAILED".into(),
            message: error.to_string(),
            stage: rollback_journal.last_checkpoint.clone(),
        });
        rollback_journal.recovery.recommended_action = "inspect".into();
        let _ = persist_journal(&rollback_store, &mut rollback_journal);
    }
    result
}

#[cfg(test)]
fn cleanup_created_profile_directories(
    project_root: &Path,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    let root = open_bound_project_root(project_root, &journal.project_root_lifecycle)?;
    cleanup_created_profile_directories_rooted(&root, journal, store)
}

fn cleanup_created_profile_directories_rooted(
    root: &RootedDir,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
) -> Result<(), AppError> {
    let mut directories = journal.created_directories.clone();
    directories.sort_by_key(|path| std::cmp::Reverse(Path::new(path).components().count()));
    for directory in directories {
        if let Err(error) = root.remove_dir_if_empty(&directory) {
            return Err(AppError::Transaction(format!(
                "could not remove empty profile directory {directory}: {error}"
            )));
        }
    }
    journal.last_checkpoint = "rollback-profile-directories-checked".into();
    persist_journal(store, journal)
}

fn cleanup_created_project_root(
    project_root: &Path,
    journal: &mut TransactionJournal,
    store: &TransactionStore,
    rollback_journal: &mut TransactionJournal,
    rollback_store: &TransactionStore,
) -> Result<(), AppError> {
    let lifecycle = &journal.project_root_lifecycle;
    if lifecycle.mode != ProjectRootMode::CreateLeaf
        || !matches!(
            lifecycle.checkpoint.as_str(),
            "applying" | "created" | "removing" | "removed"
        )
    {
        return Ok(());
    }
    if !project_root.exists() {
        journal.project_root_lifecycle.checkpoint = "removed".into();
        journal.project_root_lifecycle.observed_exists = false;
        journal.project_root_lifecycle.cleanup_result = Some("removed".into());
        rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
        persist_journal(store, journal)?;
        persist_journal(rollback_store, rollback_journal)?;
        return Ok(());
    }
    if !lifecycle.created_by_transaction {
        journal.project_root_lifecycle.root_identity =
            Some(RootedDir::open_read(project_root)?.identity_token()?);
        journal.project_root_lifecycle.checkpoint = "retained_user_content".into();
        journal.project_root_lifecycle.observed_exists = true;
        journal.project_root_lifecycle.cleanup_result = Some("retained_user_content".into());
        rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
        persist_journal(store, journal)?;
        persist_journal(rollback_store, rollback_journal)?;
        return Ok(());
    }
    if lifecycle.checkpoint == "removed" {
        journal.project_root_lifecycle.checkpoint = "retained_user_content".into();
        journal.project_root_lifecycle.observed_exists = true;
        journal.project_root_lifecycle.cleanup_result = Some("retained_user_content".into());
        rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
        persist_journal(store, journal)?;
        persist_journal(rollback_store, rollback_journal)?;
        return Ok(());
    }
    let root = validate_project_root(project_root)?;
    let mut directories = std::collections::BTreeSet::new();
    directories.insert(root.join(".hoi4-mod-setup"));
    for operation in &journal.operations {
        if operation.external {
            continue;
        }
        let destination = safe_join(&root, &operation.destination)?;
        let mut parent = destination.parent();
        while let Some(current) = parent {
            if current == root {
                break;
            }
            directories.insert(current.to_path_buf());
            parent = current.parent();
        }
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        match remove_directory_path_if_empty(&directory) {
            Ok(_) => {}
            Err(error) => {
                return Err(AppError::Transaction(format!(
                    "could not remove empty managed directory {}: {error}",
                    directory.display()
                )))
            }
        }
    }
    journal.project_root_lifecycle.checkpoint = "removing".into();
    journal.last_checkpoint = "rollback-project-root-intent".into();
    persist_journal(store, journal)?;
    maybe_abort_for_test("before_project_root_remove");
    if remove_directory_path_if_empty(&root)? {
        maybe_abort_for_test("after_project_root_remove");
        journal.project_root_lifecycle.checkpoint = "removed".into();
        journal.project_root_lifecycle.observed_exists = false;
        journal.project_root_lifecycle.cleanup_result = Some("removed".into());
    } else {
        journal.project_root_lifecycle.checkpoint = "retained_user_content".into();
        journal.project_root_lifecycle.observed_exists = true;
        journal.project_root_lifecycle.cleanup_result = Some("retained_user_content".into());
    }
    rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
    persist_journal(store, journal)?;
    persist_journal(rollback_store, rollback_journal)
}

fn restore_previous_lock(
    project_directory: Option<&RootedDir>,
    project_root: &Path,
    journal: &TransactionJournal,
    backups: &JournalBackups,
) -> Result<(), AppError> {
    const LOCK_RELATIVE: &str = ".hoi4-mod-setup/install.lock.json";
    let lock_path = safe_join(project_root, LOCK_RELATIVE)?;
    let current = if let Some(project) = project_directory {
        if project.is_regular_file(LOCK_RELATIVE)? {
            Some(project.hash_file(LOCK_RELATIVE)?)
        } else {
            None
        }
    } else if journal.previous_lock_sha256.is_none()
        && journal.result_lock_exists != Some(true)
        && journal.result_lock_sha256.is_none()
    {
        None
    } else {
        return Err(AppError::PathSecurity(
            "rollback has no retained project-root handle for lock recovery".into(),
        ));
    };
    let previous = journal.previous_lock_sha256.as_deref();
    let current_is_previous = current.as_deref() == previous;
    let current_is_result = match (
        journal.result_lock_exists,
        journal.result_lock_sha256.as_deref(),
    ) {
        (Some(true), Some(expected)) => current.as_deref() == Some(expected),
        (Some(false), None) => current.is_none(),
        (None, _) => false,
        _ => {
            return Err(AppError::Transaction(
                "rollback journal has incomplete result-lock evidence".into(),
            ))
        }
    };
    if current_is_previous {
        return Ok(());
    }
    if !current_is_result {
        return Err(AppError::Transaction(
            "installation lock changed outside the transaction; refusing rollback".into(),
        ));
    }
    if let Some(path) = &journal.previous_lock_backup_path {
        // The predecessor lock is read through the journal's retained, bound
        // backup directory.
        let (backup_directory, backup_leaf) = backups.resolve(path, "install.lock.json.bak")?;
        if !backup_directory.is_regular_file(backup_leaf)? {
            return Err(AppError::PathSecurity(
                "previous installation lock backup is not a regular file".into(),
            ));
        }
        let expected_hash = journal
            .previous_lock_sha256
            .as_deref()
            .ok_or_else(|| AppError::Transaction("lock backup has no recorded checksum".into()))?;
        if backup_directory.hash_file(backup_leaf)? != expected_hash {
            return Err(AppError::Transaction(
                "previous installation lock backup checksum mismatch".into(),
            ));
        }
        let project = project_directory.ok_or_else(|| {
            AppError::PathSecurity("rollback has no retained project-root handle".into())
        })?;
        let held = mutate_live_leaf(
            project,
            LOCK_RELATIVE,
            current.as_deref(),
            LiveChange::Copy {
                source: backup_directory,
                source_relative: backup_leaf,
            },
            &lock_quarantine_leaf(journal.transaction_id, "restore"),
            None,
            "rollback-lock",
            &rollback_lock_quarantine_fault,
            &no_live_barrier,
        )?;
        if project.hash_file(LOCK_RELATIVE)? != expected_hash {
            return Err(AppError::Transaction(
                "restored installation lock checksum mismatch".into(),
            ));
        }
        if let (Some(quarantine), Some(displaced)) = (held, current.as_deref()) {
            rollback_lock_quarantine_fault(QuarantineBoundary::BeforeRelease)?;
            release_quarantine(project, &quarantine, displaced)?;
        }
    } else if previous.is_none() {
        if let Some(project) = project_directory {
            if project.is_regular_file(LOCK_RELATIVE)? {
                let held = mutate_live_leaf(
                    project,
                    LOCK_RELATIVE,
                    current.as_deref(),
                    LiveChange::Delete,
                    &lock_quarantine_leaf(journal.transaction_id, "restore"),
                    None,
                    "rollback-lock",
                    &rollback_lock_quarantine_fault,
                    &no_live_barrier,
                )?;
                if let (Some(quarantine), Some(displaced)) = (held, current.as_deref()) {
                    rollback_lock_quarantine_fault(QuarantineBoundary::BeforeRelease)?;
                    release_quarantine(project, &quarantine, displaced)?;
                }
            } else if project.exists(LOCK_RELATIVE)? {
                return Err(AppError::PathSecurity(
                    "refusing to remove an installation lock link during rollback".into(),
                ));
            }
        } else if lock_path.exists() {
            return Err(AppError::PathSecurity(
                "rollback cannot inspect an unbound installation lock".into(),
            ));
        }
    } else {
        return Err(AppError::Transaction(
            "previous installation lock backup is missing".into(),
        ));
    }
    Ok(())
}

/// Read a journal by path. The journal and its checkpoint log are read
/// through one retained handle on their directory, and a journal that
/// recorded its transaction directory's identity is refused when that handle
/// holds a different directory.
pub fn read_journal(path: &Path) -> Result<TransactionJournal, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("transaction journal has no parent".into()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::PathSecurity("transaction journal name is invalid".into()))?;
    let directory = RootedDir::open_read(parent)?;
    let journal = load_journal_in(&directory, name)?;
    verify_app_data_binding(&journal, None, &directory)?;
    Ok(journal)
}

/// Read, migrate, and replay a journal-shaped file through a retained
/// directory handle without comparing the directory's identity.
fn load_journal_in(directory: &RootedDir, name: &str) -> Result<TransactionJournal, AppError> {
    if !directory.is_regular_file(name)? {
        return Err(AppError::PathSecurity(
            "transaction journal is not a regular file".into(),
        ));
    }
    parse_journal_in(directory, &directory.read_file(name)?)
}

fn parse_journal_in(directory: &RootedDir, bytes: &[u8]) -> Result<TransactionJournal, AppError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| AppError::Transaction(format!("invalid transaction journal: {error}")))?;
    let mut journal = crate::migrations::migrate_journal(value)?;
    replay_operation_checkpoints(directory, &mut journal)?;
    sanitize_journal_error(&mut journal);
    Ok(journal)
}

fn finish_finalization(
    project_root: &Path,
    app: &AppDataRoot,
    store: &TransactionStore,
    transaction_id: Uuid,
    journal: &mut TransactionJournal,
) -> Result<(TransactionJournal, InstallationLock), AppError> {
    let project_root = validate_journal_project_root(project_root, journal, store.journal_path())?;
    let project_directory =
        open_bound_project_root(&project_root, &journal.project_root_lifecycle)?;
    if journal.transaction_id != transaction_id || journal.state != "finalizing" {
        return Err(AppError::Transaction(
            "transaction is not in the finalization state".into(),
        ));
    }
    if crate::security::path_has_link_component(&app.path) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    // A process stop inside the success-lock commit can leave the displaced
    // predecessor lock in its quarantine. Release it only beside the exact
    // committed lock; beside an absent lock it moves back for rollback.
    settle_journal_lock_quarantines(&project_directory, journal)?;
    let lock_bytes = project_directory
        .read_file(".hoi4-mod-setup/install.lock.json")
        .map_err(|error| {
            AppError::Transaction(format!(
                "finalization lock is unavailable; rollback or manual review is required: {error}"
            ))
        })?;
    if journal.result_lock_exists != Some(true) {
        return Err(AppError::Transaction(
            "finalization journal has no committed success-lock expectation; manual review is required"
                .into(),
        ));
    }
    let expected_lock_hash = journal.result_lock_sha256.as_deref().ok_or_else(|| {
        AppError::Transaction(
            "finalization journal has no success-lock checksum; manual review is required".into(),
        )
    })?;
    if sha256_bytes(&lock_bytes) != expected_lock_hash {
        return Err(AppError::Transaction(
            "finalization success lock checksum mismatch; manual review is required".into(),
        ));
    }
    let lock_value: serde_json::Value = serde_json::from_slice(&lock_bytes)?;
    let lock = crate::migrations::migrate_lock(lock_value)?;
    let record_suffix = format!("{transaction_id}/rollback-record.json");
    let record_reference = format!("transactions/{record_suffix}");
    if !lock
        .rollback_records
        .iter()
        .any(|record| record == &record_reference)
    {
        return Err(AppError::Transaction(
            "finalization lock has no verified rollback record; manual review is required".into(),
        ));
    }
    // The rollback record is read once through the retained transaction
    // directory: the bytes that are hashed are the bytes that are parsed.
    let record_path = app.path.join(&record_reference);
    if path_has_link_component(&record_path) {
        return Err(AppError::PathSecurity(
            "finalization rollback record contains a symlink or junction".into(),
        ));
    }
    if !store.directory.exists(ROLLBACK_RECORD_FILE)? {
        return Err(AppError::Transaction(
            "finalization rollback record is unavailable; manual review is required".into(),
        ));
    }
    if !store.directory.is_regular_file(ROLLBACK_RECORD_FILE)? {
        return Err(AppError::PathSecurity(
            "finalization rollback record is not a regular file".into(),
        ));
    }
    let expected_record_hash = journal.rollback_record_sha256.as_deref().ok_or_else(|| {
        AppError::Transaction(
            "finalization rollback record has no journaled checksum; manual review is required"
                .into(),
        )
    })?;
    let record_bytes = store.directory.read_file(ROLLBACK_RECORD_FILE)?;
    if sha256_bytes(&record_bytes) != expected_record_hash {
        return Err(AppError::Transaction(
            "finalization rollback record checksum mismatch; manual review is required".into(),
        ));
    }
    let record = parse_journal_in(&store.directory, &record_bytes)?;
    verify_app_data_binding(&record, Some(&app.directory), &store.directory)?;
    if record.transaction_id != transaction_id
        || record.transaction_kind != "installation"
        || record.project_id != journal.project_id
        || record.project_root != journal.project_root
        || record.plan_sha256 != journal.plan_sha256
        || record.state != "finalizing"
        || !record.recovery.project_apply_started
        || !record.recovery.rollback_allowed
        || record.operations.len() != journal.operations.len()
        || record
            .operations
            .iter()
            .zip(&journal.operations)
            .any(|(left, right)| {
                left.id != right.id
                    || left.status != right.status
                    || left.after_sha256 != right.after_sha256
                    || left.after_exists != right.after_exists
                    || left.after_executable != right.after_executable
            })
    {
        return Err(AppError::Transaction(
            "finalization rollback record is not bound to the current journal; manual review is required"
                .into(),
        ));
    }
    if lock.project_id != journal.project_id {
        return Err(AppError::Transaction(
            "finalization lock is not bound to the transaction project".into(),
        ));
    }
    for operation in &journal.operations {
        let (current, current_executable) = if operation.external {
            let target = existing_operation_target(None, operation)?;
            target_hash_and_executable(target.as_ref())?
        } else {
            let exists = project_directory.exists(&operation.destination)?;
            if exists && !project_directory.is_regular_file(&operation.destination)? {
                return Err(AppError::PathSecurity(format!(
                    "finalization destination is not a regular file: {}",
                    operation.destination
                )));
            }
            let current = if exists {
                Some(project_directory.hash_file(&operation.destination)?)
            } else {
                None
            };
            let executable = if exists {
                #[cfg(unix)]
                {
                    project_directory.observed_executable(&operation.destination)?
                }
                #[cfg(not(unix))]
                {
                    None
                }
            } else {
                None
            };
            (current, executable)
        };
        match operation.action {
            Some(OperationAction::Skip | OperationAction::External) => {
                if current != operation.before_sha256 {
                    return Err(AppError::Transaction(format!(
                        "finalization live result changed for skipped operation: {}",
                        operation.destination
                    )));
                }
            }
            Some(OperationAction::DeleteManaged) => {
                if current.is_some() || operation.after_exists != Some(false) {
                    return Err(AppError::Transaction(format!(
                        "finalization live delete result changed: {}",
                        operation.destination
                    )));
                }
            }
            Some(_) => {
                if operation.after_exists != Some(true)
                    || operation.after_sha256.as_deref() != current.as_deref()
                    || operation
                        .after_executable
                        .is_some_and(|expected| current_executable != Some(expected))
                {
                    return Err(AppError::Transaction(format!(
                        "finalization live result changed: {}",
                        operation.destination
                    )));
                }
            }
            None => {
                return Err(AppError::Transaction(
                    "finalization journal operation has no action; manual review is required"
                        .into(),
                ))
            }
        }
    }
    sweep_transaction_quarantines(
        Some(&project_directory),
        journal,
        None,
        QuarantineSweep::Result,
    )?;
    if let Some(stage) = journal.stages.get_mut(11) {
        stage.status = "complete".into();
        if stage.completed_at.is_none() {
            stage.completed_at = Some(Utc::now().to_rfc3339());
        }
    }
    journal.state = "completed".into();
    journal.recovery = RecoveryState {
        resume_allowed: false,
        rollback_allowed: true,
        discard_staging_allowed: false,
        project_apply_started: true,
        recommended_action: "none".into(),
    };
    persist_journal(store, journal)?;
    Ok((journal.clone(), lock))
}

/// Resume only from a journal that proves that project apply had not started.
/// The plan and every staged byte are revalidated from disk before the normal
/// transaction runner is replayed. Replaying from the pre-apply checkpoint is
/// deterministic and avoids guessing which individual filesystem operations
/// completed after an interruption.
pub fn resume_transaction(
    project_root: &Path,
    app_root: &Path,
    transaction_id: Uuid,
) -> Result<(TransactionJournal, InstallationLock), AppError> {
    resume_transaction_with_options(project_root, app_root, transaction_id, None)
}

pub fn resume_transaction_with_options(
    project_root: &Path,
    app_root: &Path,
    transaction_id: Uuid,
    post_install_action_runner: Option<PostInstallActionRunner>,
) -> Result<(TransactionJournal, InstallationLock), AppError> {
    let (project_root, _) = validate_project_root_or_destination(project_root)?;
    if crate::security::path_has_link_component(app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    // The storage of the interrupted transaction is opened once through the
    // retained application-data root and held through the replay, and each
    // directory must be the one the journal recorded. A refusal here leaves
    // the interrupted journal untouched.
    let app = AppDataRoot::open(app_root)?;
    let store = app.transaction_store(transaction_id, false)?;
    let mut journal = app.read_bound_journal(&store)?;
    if journal.transaction_id != transaction_id {
        return Err(AppError::Transaction(
            "transaction journal ID does not match the requested transaction".into(),
        ));
    }
    if journal.state == "finalizing" {
        return finish_finalization(&project_root, &app, &store, transaction_id, &mut journal);
    }
    normalize_incomplete_recovery(&mut journal);
    if transaction_state_is_terminal(&journal.state) {
        return Err(AppError::Transaction(
            "transaction is not in a resumable pre-apply state".into(),
        ));
    }
    if !journal.recovery.resume_allowed {
        let guidance = if journal.recovery.rollback_allowed
            || journal.recovery.recommended_action == "rollback"
        {
            "rollback or manual review is required"
        } else {
            "discard staging or manual review is required"
        };
        return Err(AppError::Transaction(format!(
            "transaction is not in a resumable pre-apply state; {guidance}"
        )));
    }
    if journal.recovery.project_apply_started {
        return Err(AppError::Transaction(
            "project apply already started; rollback or manual review is required".into(),
        ));
    }
    if journal.project_root.is_empty() {
        return Err(AppError::Transaction(
            "interrupted journal has no project-root binding; manual review is required".into(),
        ));
    }
    validate_journal_project_root(&project_root, &journal, store.journal_path())?;

    let plan_bytes = store.directory.read_file(PLAN_FILE).map_err(|error| {
        AppError::Transaction(format!("cannot read interrupted transaction plan: {error}"))
    })?;
    let plan: InstallationPlan = serde_json::from_slice(&plan_bytes).map_err(|error| {
        AppError::Transaction(format!("invalid interrupted transaction plan: {error}"))
    })?;
    if plan.plan_id != transaction_id {
        return Err(AppError::Transaction(
            "transaction plan ID does not match the requested transaction".into(),
        ));
    }
    validate_plan(&plan)?;
    let canonical_plan_bytes = serde_json::to_vec(&plan)?;
    let plan_hash = sha256_bytes(&canonical_plan_bytes);
    if journal.plan_sha256.as_deref() != Some(plan_hash.as_str()) {
        return Err(AppError::Transaction(
            "interrupted transaction plan hash does not match its journal".into(),
        ));
    }
    let lock_path = safe_join(&project_root, ".hoi4-mod-setup/install.lock.json")?;
    let current_lock_hash = regular_file_hash(&lock_path)?;
    if current_lock_hash.as_deref() != journal.previous_lock_sha256.as_deref() {
        return Err(AppError::Transaction(
            "predecessor installation lock changed or disappeared after interruption; refusing to replay the transaction"
                .into(),
        ));
    }

    // Staged bytes are read through the retained staging directory, which
    // must be the directory the interrupted run bound.
    let staging = if plan.operations.iter().any(|operation| {
        !matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External | OperationAction::DeleteManaged
        )
    }) {
        app.open_area(
            STAGING_AREA,
            transaction_id,
            false,
            journal
                .app_data_identity
                .as_ref()
                .and_then(|identity| identity.staging.as_deref()),
        )?
    } else {
        None
    };
    let mut prepared = Vec::new();
    for operation in &plan.operations {
        let record = journal
            .operations
            .iter()
            .find(|record| record.id == operation.id)
            .ok_or_else(|| {
                AppError::Transaction(format!(
                    "journal is missing operation checkpoint: {}",
                    operation.id
                ))
            })?;
        if !matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) && matches!(record.status.as_str(), "applying" | "applied" | "verified")
        {
            return Err(AppError::Transaction(format!(
                "operation {} may already have applied; rollback or manual review is required",
                operation.id
            )));
        }

        let expected = operation
            .result_sha256
            .as_ref()
            .or(operation.source_sha256.as_ref());
        if record.expected_sha256.as_ref() != expected {
            return Err(AppError::Transaction(format!(
                "journal checksum expectation is not bound to operation {}",
                operation.id
            )));
        }

        let destination = operation_destination(&project_root, operation)?;
        // An external parent bound by the interrupted run must still be that
        // directory. Refusing here, before the replay writes a fresh journal,
        // keeps the interrupted journal resumable once the directory returns.
        let current_hash = if operation.external && record.external_parent_identity.is_some() {
            match existing_operation_target(None, record)? {
                Some(target) => target.hash()?,
                None => None,
            }
        } else {
            regular_file_hash(&destination)?
        };
        if current_hash.as_deref() != operation.local_sha256.as_deref() {
            return Err(AppError::Transaction(format!(
                "live precondition changed before resume: {}",
                operation.destination
            )));
        }

        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) {
            continue;
        }

        if operation.action == OperationAction::DeleteManaged {
            continue;
        }
        let staged = staging_relative(operation)?;
        let staging = staging.as_ref().ok_or_else(|| {
            AppError::Transaction(format!(
                "staged bytes are missing for {}",
                operation.destination
            ))
        })?;
        if !staging.exists(&staged)? {
            return Err(AppError::Transaction(format!(
                "staged bytes are missing for {}",
                operation.destination
            )));
        }
        if !staging.is_regular_file(&staged)? {
            return Err(AppError::PathSecurity(format!(
                "staged destination is not a regular file: {}",
                operation.destination
            )));
        }
        let bytes = staging.read_file(&staged)?;
        let actual = sha256_bytes(&bytes);
        if expected != Some(&actual) {
            return Err(AppError::Source(format!(
                "staged checksum mismatch before resume: {}",
                operation.destination
            )));
        }
        prepared.push(PreparedFile {
            operation_id: operation.id.clone(),
            destination: operation.destination.clone(),
            bytes,
            expected_sha256: actual,
        });
    }

    // Preserve the failed checkpoint as an audit artifact before the replay
    // writes a fresh journal at the canonical path.
    let snapshot = journal.clone();
    store.write_json(
        &format!("journal.interrupted.{}.json", Uuid::new_v4()),
        &snapshot,
    )?;
    run_transaction(
        &project_root,
        &plan,
        &prepared,
        &TransactionOptions {
            app_data_root: Some(app_root.to_path_buf()),
            resume_transaction_id: Some(transaction_id),
            post_install_action_runner,
            ..Default::default()
        },
    )
}

/// Remove only the exact staging directory for an interrupted pre-apply
/// transaction. Backups and the journal remain available for audit/review.
pub fn discard_staging(
    project_root: &Path,
    app_root: &Path,
    transaction_id: Uuid,
) -> Result<TransactionJournal, AppError> {
    if crate::security::path_has_link_component(app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    let app = AppDataRoot::open(app_root)?;
    let store = app.transaction_store(transaction_id, false)?;
    let mut journal = app.read_bound_journal(&store)?;
    if journal.transaction_id != transaction_id {
        return Err(AppError::Transaction(
            "transaction journal ID does not match the requested transaction".into(),
        ));
    }
    let _project_root =
        validate_journal_project_root(project_root, &journal, store.journal_path())?;
    normalize_incomplete_recovery(&mut journal);
    if transaction_state_is_terminal(&journal.state) || !journal.recovery.discard_staging_allowed {
        return Err(AppError::Transaction(
            "staging cannot be discarded from the current transaction state".into(),
        ));
    }
    if journal.recovery.project_apply_started {
        return Err(AppError::Transaction(
            "project apply already started; rollback is required instead of discarding staging"
                .into(),
        ));
    }
    let staging_path = app.area_path(STAGING_AREA, transaction_id);
    if crate::security::path_has_link_component(&staging_path) {
        return Err(AppError::PathSecurity(
            "staging directory contains a symlink or junction".into(),
        ));
    }
    let staging_name = transaction_id.to_string();
    if app.directory.is_directory(STAGING_AREA)? {
        // Only the staging directory this transaction bound is removed, and
        // its contents are removed through the handle whose identity matched.
        let staging_parent = app.directory.open_dir(STAGING_AREA)?;
        if staging_parent.exists(&staging_name)? {
            if !staging_parent.is_directory(&staging_name)? {
                return Err(AppError::PathSecurity(
                    "staging path is not a directory".into(),
                ));
            }
            match journal
                .app_data_identity
                .as_ref()
                .and_then(|identity| identity.staging.as_deref())
            {
                Some(expected) => {
                    if !staging_parent.remove_tree_if_identity(&staging_name, expected)? {
                        return Err(app_data_drift(&staging_path));
                    }
                }
                None => staging_parent.remove_tree(&staging_name)?,
            }
        }
    }
    journal.state = "staging_discarded".into();
    journal.recovery = RecoveryState {
        resume_allowed: false,
        rollback_allowed: false,
        discard_staging_allowed: false,
        project_apply_started: false,
        recommended_action: "none".into(),
    };
    journal.last_checkpoint = "staging-discarded".into();
    persist_journal(&store, &mut journal)?;
    Ok(journal)
}

pub fn recovery_action(journal: &TransactionJournal) -> String {
    let mut normalized = journal.clone();
    normalize_incomplete_recovery(&mut normalized);
    if transaction_state_is_terminal(&normalized.state) {
        "none".into()
    } else {
        normalized.recovery.recommended_action
    }
}

/// Build a repair view from the lock without mutating the project. Missing or
/// unmodified managed files may be restored; modified files become explicit
/// review entries and are never silently replaced.
pub fn repair_operations(
    lock: &InstallationLock,
    project_root: &Path,
) -> Result<Vec<PlanOperation>, AppError> {
    let mut operations = Vec::new();
    for (index, file) in lock.files.iter().enumerate() {
        let destination = locked_file_destination(project_root, file)?;
        let current = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        let (action, local_state) = if file.preserved_local {
            (
                OperationAction::Skip,
                match current.as_deref() {
                    Some(hash) if hash == file.installed_sha256 => LocalState::Unmodified,
                    None => LocalState::Unknown,
                    Some(_) => LocalState::Modified,
                },
            )
        } else if file.ownership == Ownership::External {
            (
                OperationAction::Skip,
                if current.is_some() {
                    LocalState::Unmodified
                } else {
                    LocalState::Unknown
                },
            )
        } else if file.ownership == Ownership::Merged {
            match current.as_deref() {
                Some(hash) if hash == file.installed_sha256 => {
                    (OperationAction::Skip, LocalState::Unmodified)
                }
                None => (OperationAction::Skip, LocalState::Unknown),
                Some(_) => (OperationAction::Skip, LocalState::Modified),
            }
        } else {
            match current.as_deref() {
                None => (OperationAction::Create, LocalState::Absent),
                Some(hash) if hash == file.installed_sha256 => {
                    // A healthy file is already repaired. Planning a replace
                    // would create needless churn, a backup, and a rollback
                    // surface without changing its bytes.
                    (OperationAction::Skip, LocalState::Unmodified)
                }
                Some(_) => (OperationAction::Skip, LocalState::Modified),
            }
        };
        operations.push(PlanOperation {
            id: format!("repair-{index:05}"),
            component_id: file.component_id.clone(),
            ownership: Some(file.ownership),
            location_scope: Some(location_scope_for_file(file)),
            action,
            source_path: Some(file.source_path.clone()),
            destination: file.path.clone(),
            source_sha256: Some(file.source_sha256.clone()),
            source_size: file.source_size,
            platform: file.platform,
            executable: file.executable,
            result_sha256: None,
            base_sha256: file.base_sha256.clone(),
            local_sha256: current,
            local_state,
            resolution: if file.preserved_local {
                Some("preserved_local_review".into())
            } else if file.ownership == Ownership::External {
                Some("user_owned_review".into())
            } else if action == OperationAction::Skip {
                // A healthy file is skipped without any decision; only local
                // edits need the user's review.
                (local_state != LocalState::Unmodified).then(|| {
                    if file.ownership == Ownership::Merged {
                        "reverse_merge_required".into()
                    } else {
                        "review_required".into()
                    }
                })
            } else {
                None
            },
            external: file.external,
            rollback: if action == OperationAction::Create {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
            external_parent_identity: None,
        });
    }
    Ok(operations)
}

/// Build a managed-removal view. A file changed by the user is retained and
/// represented as a skipped operation for an explicit removal decision.
pub fn managed_removal_operations(
    lock: &InstallationLock,
    project_root: &Path,
) -> Result<Vec<PlanOperation>, AppError> {
    let mut operations = Vec::new();
    for (index, file) in lock.files.iter().enumerate() {
        let destination = locked_file_destination(project_root, file)?;
        let current = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        let unchanged = current.as_deref() == Some(file.installed_sha256.as_str());
        let reversible = !file.preserved_local
            && !matches!(file.ownership, Ownership::Merged | Ownership::External);
        operations.push(PlanOperation {
            id: format!("remove-{index:05}"),
            component_id: file.component_id.clone(),
            ownership: Some(file.ownership),
            location_scope: Some(location_scope_for_file(file)),
            action: if unchanged && reversible {
                OperationAction::DeleteManaged
            } else {
                OperationAction::Skip
            },
            source_path: None,
            destination: file.path.clone(),
            source_sha256: None,
            source_size: file.source_size,
            platform: file.platform,
            executable: file.executable,
            result_sha256: None,
            base_sha256: file.base_sha256.clone(),
            local_sha256: current,
            local_state: if unchanged {
                LocalState::Unmodified
            } else {
                LocalState::Modified
            },
            resolution: if unchanged && reversible {
                Some("managed_remove".into())
            } else if unchanged {
                Some("reverse_merge_required".into())
            } else {
                Some("keep_user_modification".into())
            },
            external: file.external,
            rollback: RollbackAction::RestoreBackup,
            external_parent_identity: None,
        });
    }
    Ok(operations)
}

pub fn reinstall_operations(
    lock: &InstallationLock,
    project_root: &Path,
) -> Result<Vec<PlanOperation>, AppError> {
    lock.files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let destination = locked_file_destination(project_root, file)?;
            let local_sha256 = if destination.is_file() {
                Some(sha256_file(&destination)?)
            } else {
                None
            };
            let local_state = match local_sha256.as_deref() {
                None => LocalState::Absent,
                Some(hash) if hash == file.installed_sha256 => LocalState::Unmodified,
                Some(_) => LocalState::Modified,
            };
            let merged = file.ownership == Ownership::Merged;
            let user_owned = file.ownership == Ownership::External || file.preserved_local;
            Ok(PlanOperation {
                id: format!("reinstall-{index:05}"),
                component_id: file.component_id.clone(),
                ownership: Some(file.ownership),
                location_scope: Some(location_scope_for_file(file)),
                action: if merged || user_owned || local_state == LocalState::Modified {
                    OperationAction::Skip
                } else if local_state == LocalState::Absent {
                    OperationAction::Create
                } else {
                    OperationAction::Replace
                },
                source_path: Some(file.source_path.clone()),
                destination: file.path.clone(),
                source_sha256: Some(file.source_sha256.clone()),
                source_size: file.source_size,
                platform: file.platform,
                executable: file.executable,
                result_sha256: None,
                base_sha256: file.base_sha256.clone(),
                local_sha256,
                local_state,
                resolution: Some(
                    if user_owned {
                        "user_owned_review"
                    } else if merged && local_state != LocalState::Unmodified {
                        "reverse_merge_required"
                    } else if local_state == LocalState::Modified {
                        "review_required"
                    } else {
                        "reinstall_reviewed"
                    }
                    .into(),
                ),
                external: file.external,
                rollback: RollbackAction::RestoreBackup,
                external_parent_identity: None,
            })
        })
        .collect()
}

/// Plan an update against an exact incoming lock view. Existing unchanged
/// files are replaceable, missing files are creates, and modified files stay
/// skipped until the conflict engine supplies an explicit decision.
pub fn update_operations(
    current_lock: &InstallationLock,
    incoming_files: &[LockedFile],
    project_root: &Path,
) -> Result<Vec<PlanOperation>, AppError> {
    let mut operations: Vec<PlanOperation> = incoming_files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let destination = locked_file_destination(project_root, file)?;
            let local_sha256 = if destination.is_file() {
                Some(sha256_file(&destination)?)
            } else {
                None
            };
            let previous_file = current_lock
                .files
                .iter()
                .find(|current| current.path == file.path && current.external == file.external);
            let previous = previous_file.map(|current| current.installed_sha256.as_str());
            let local_state = match (local_sha256.as_deref(), previous) {
                (None, _) => LocalState::Absent,
                (Some(hash), Some(previous)) if hash == previous => LocalState::Unmodified,
                (Some(hash), _) if hash == file.installed_sha256 => LocalState::Unmodified,
                (Some(_), _) => LocalState::Modified,
            };
            let merged_base_required = previous_file.is_some_and(|current| {
                current.ownership == Ownership::Merged || current.preserved_local
            });
            let action = match (merged_base_required, local_state) {
                (true, _) => OperationAction::Skip,
                (false, LocalState::Absent) => OperationAction::Create,
                (false, LocalState::Unmodified) => OperationAction::Replace,
                (false, LocalState::Modified | LocalState::Unknown) => OperationAction::Skip,
            };
            Ok(PlanOperation {
                id: format!("update-{index:05}"),
                component_id: file.component_id.clone(),
                ownership: Some(file.ownership),
                location_scope: Some(location_scope_for_file(file)),
                action,
                source_path: Some(file.source_path.clone()),
                destination: file.path.clone(),
                source_sha256: Some(file.source_sha256.clone()),
                source_size: file.source_size,
                platform: file.platform,
                executable: file.executable,
                result_sha256: None,
                base_sha256: previous.map(ToOwned::to_owned),
                local_sha256,
                local_state,
                resolution: (action == OperationAction::Skip).then(|| {
                    if merged_base_required {
                        "merged_base_required"
                    } else {
                        "review_required"
                    }
                    .into()
                }),
                external: file.external,
                rollback: if action == OperationAction::Create {
                    RollbackAction::RemoveCreated
                } else {
                    RollbackAction::RestoreBackup
                },
                external_parent_identity: None,
            })
        })
        .collect::<Result<_, AppError>>()?;
    for (index, file) in current_lock.files.iter().enumerate() {
        if incoming_files
            .iter()
            .any(|incoming| incoming.path == file.path && incoming.external == file.external)
        {
            continue;
        }
        let destination = locked_file_destination(project_root, file)?;
        let local_sha256 = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        let unchanged = local_sha256.as_deref() == Some(file.installed_sha256.as_str());
        let removable = !file.preserved_local
            && !matches!(file.ownership, Ownership::Merged | Ownership::External);
        operations.push(PlanOperation {
            id: format!("update-obsolete-{index:05}"),
            component_id: file.component_id.clone(),
            ownership: Some(file.ownership),
            location_scope: Some(location_scope_for_file(file)),
            action: if unchanged && removable {
                OperationAction::DeleteManaged
            } else {
                OperationAction::Skip
            },
            source_path: None,
            destination: file.path.clone(),
            source_sha256: None,
            source_size: file.source_size,
            platform: file.platform,
            executable: file.executable,
            result_sha256: None,
            base_sha256: Some(file.installed_sha256.clone()),
            local_sha256,
            local_state: if unchanged {
                LocalState::Unmodified
            } else {
                LocalState::Modified
            },
            resolution: Some(if unchanged && removable {
                "obsolete_managed_remove".into()
            } else {
                "obsolete_review".into()
            }),
            external: file.external,
            rollback: RollbackAction::RestoreBackup,
            external_parent_identity: None,
        });
    }
    Ok(operations)
}

#[cfg(test)]
mod tests {
    fn run_test_transaction(
        project_root: &Path,
        plan: &InstallationPlan,
        prepared: &[PreparedFile],
        options: &TransactionOptions,
    ) -> Result<(TransactionJournal, InstallationLock), AppError> {
        let mut reviewed_plan = plan.clone();
        if reviewed_plan.transaction.project_root_identity.is_none() {
            reviewed_plan.transaction.project_root_identity = Some(reviewed_project_root_identity(
                &reviewed_plan,
                project_root,
            )?);
        }
        run_transaction(project_root, &reviewed_plan, prepared, options)
    }

    use super::*;
    use crate::paths::transaction_root;
    use crate::readiness::manifest_wiki_pages;
    use crate::security::atomic_write_json;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn transaction_mutations_do_not_use_ambient_filesystem_apis() {
        let source = include_str!("transaction.rs");
        let production = source
            .split("mod tests {")
            .next()
            .expect("transaction module test boundary");
        for forbidden in [
            "fs::read(",
            "fs::read_dir(",
            "fs::write(",
            "fs::copy(",
            "fs::remove_file(",
            "fs::remove_dir(",
            "fs::remove_dir_all(",
            "fs::rename(",
            "fs::create_dir(",
            "fs::create_dir_all(",
            "fs::set_permissions(",
            "OpenOptions::new()",
        ] {
            assert!(
                !production.contains(forbidden),
                "transaction production code must use RootedDir instead of {forbidden}"
            );
        }
    }

    fn test_codex_analysis() -> CodexAnalysisRecord {
        CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: Some("gpt-5.6-luna".into()),
            reasoning_effort: Some("xhigh".into()),
            optimization_profile: Some("Codex setup analysis".into()),
            analysis_id: uuid::Uuid::new_v4(),
            schema_version: "1.0.0".into(),
            input_sha256: "a".repeat(64),
            output_sha256: "b".repeat(64),
            confirmed_fields: crate::codex::REQUIRED_ANALYSIS_PROPOSAL_KEYS
                .iter()
                .map(|field| (*field).into())
                .collect(),
            confirmed_at: Utc::now().to_rfc3339(),
            account_identity_persisted: false,
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
            evidence_sha256: None,
            source_revision: Some("599497ea2f93612d9094461c6fde114fc87a5c0f".into()),
            source_manifest_sha256: Some("a".repeat(64)),
        }
    }

    fn plan() -> InstallationPlan {
        InstallationPlan {
            schema_version: "1.0.0".into(),
            plan_id: uuid::Uuid::new_v4(),
            project_id: "example".into(),
            script_prefix: Some("example".into()),
            primary_namespace: Some("example".into()),
            created_at: Some(Utc::now().to_rfc3339()),
            maintenance_mode: None,
            source: SourceIdentity {
                repository: "klimPaskov/Agentic-HOI4-Modding".into(),
                mode: SourceMode::PinnedCommit,
                resolved_revision: "599497ea2f93612d9094461c6fde114fc87a5c0f".into(),
                requested_ref: None,
                release: None,
                manifest_sha256: "a".repeat(64),
                manifest_origin: "remote".into(),
            },
            ai_provider: "codex".into(),
            ai_model: "gpt-5.6-luna".into(),
            ai_reasoning_effort: "xhigh".into(),
            ai_endpoint: None,
            ai_optimization_profile: crate::models::default_ai_optimization_profile(),
            primary_coding_environment: "codex".into(),
            additional_coding_environments: vec![],
            flatten_chat_sources: false,
            codex_analysis: Some(test_codex_analysis()),
            selected_components: vec!["core.agents".into()],
            wiki_required_pages: manifest_wiki_pages(),
            wiki_metadata: None,
            generated_artifacts: vec![],
            download_ledger: vec![],
            git_setup: None,
            credential_references: vec![],
            optional_workflows: Default::default(),
            portrait_pipeline: None,
            operations: vec![PlanOperation {
                id: "op-1".into(),
                component_id: "core.agents".into(),
                ownership: Some(Ownership::Managed),
                location_scope: None,
                action: OperationAction::Create,
                source_path: Some("generated:test".into()),
                destination: "AGENTS.md".into(),
                source_sha256: Some(sha256_bytes(b"safe")),
                source_size: Some(4),
                platform: Some(ManifestPlatform::All),
                executable: false,
                result_sha256: None,
                base_sha256: None,
                local_sha256: None,
                local_state: LocalState::Absent,
                resolution: None,
                external: false,
                rollback: RollbackAction::RemoveCreated,
                external_parent_identity: None,
            }],
            conflicts: vec![],
            external_actions: vec![],
            transaction: TransactionPlanInfo {
                stages: TRANSACTION_STAGES
                    .iter()
                    .map(|stage| (*stage).into())
                    .collect(),
                backup_root: "external".into(),
                staging_root: "external".into(),
                directories: Vec::new(),
                atomic_apply_expected: true,
                project_root_mode: ProjectRootMode::Existing,
                project_root_parent: None,
                project_root_leaf: None,
                project_root_identity: None,
            },
            approvals: PlanApprovals {
                dry_run_reviewed: true,
                external_actions_reviewed: true,
                git_remote_approved: false,
                push_approved: false,
            },
        }
    }

    fn bind_first_operation_to_remote_download(plan: &mut InstallationPlan) {
        let operation = &mut plan.operations[0];
        operation.source_path = Some("AGENTS_template.md".into());
        let source_sha256 = operation
            .source_sha256
            .clone()
            .expect("test remote operation needs a source checksum");
        let source_size = operation
            .source_size
            .expect("test remote operation needs a source size");
        let ownership = operation
            .ownership
            .expect("test remote operation needs ownership");
        let platform = operation
            .platform
            .expect("test remote operation needs a platform");
        plan.download_ledger = vec![crate::models::DownloadedFile {
            operation_id: operation.id.clone(),
            source_path: operation.source_path.clone().unwrap(),
            destination: operation.destination.clone(),
            source_revision: plan.source.resolved_revision.clone(),
            manifest_sha256: plan.source.manifest_sha256.clone(),
            sha256: source_sha256,
            size: source_size,
            component_id: operation.component_id.clone(),
            ownership,
            platform,
            executable: false,
        }];
    }

    fn ready_plan(project_root: &Path) -> InstallationPlan {
        let mut plan = plan();
        plan.selected_components = vec!["core.agents".into(), "wiki.snapshot".into()];
        plan.wiki_metadata = Some(WikiInstallMetadata {
            snapshot_marker: None,
            required_media_policy: "all_declared".into(),
            source_status: "verified_snapshot".into(),
            license_status: "not_verified".into(),
            repository_license_status: "unknown".into(),
            notes: vec![],
        });
        fs::write(
            project_root.join("descriptor.mod"),
            "name=\"Example\"\nversion=\"0.1.0\"\nsupported_version=\"1.17.*\"\npicture=\"thumbnail.png\"\n",
        )
        .unwrap();
        let wiki_root = project_root.join("paradox_wiki");
        fs::create_dir_all(&wiki_root).unwrap();
        for page in manifest_wiki_pages() {
            let path = wiki_root.join(page.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, "# Test wiki page\n").unwrap();
        }
        plan
    }

    #[test]
    fn update_removes_an_unchanged_legacy_super_events_filename() {
        let project = tempdir().unwrap();
        let legacy_path = "interface/hoi4ms_super_events.gui";
        let adapted_path = "interface/example_super_events.gui";
        let legacy_bytes = b"legacy super events";
        fs::create_dir_all(project.path().join("interface")).unwrap();
        fs::write(project.path().join(legacy_path), legacy_bytes).unwrap();

        let mut current_lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let mut legacy_file = current_lock.files[0].clone();
        legacy_file.path = legacy_path.into();
        legacy_file.component_id = "workflow.super_events.runtime.interface".into();
        legacy_file.source_path = legacy_path.into();
        legacy_file.installed_sha256 = sha256_bytes(legacy_bytes);
        legacy_file.installed_size = Some(legacy_bytes.len() as u64);
        legacy_file.source_sha256 = sha256_bytes(legacy_bytes);
        current_lock.files = vec![legacy_file.clone()];

        let mut incoming_file = legacy_file;
        incoming_file.path = adapted_path.into();
        incoming_file.source_path = legacy_path.into();
        incoming_file.source_sha256 = sha256_bytes(b"adapted source");
        incoming_file.installed_sha256 = sha256_bytes(b"adapted source");
        incoming_file.installed_size = Some(b"adapted source".len() as u64);

        let operations =
            update_operations(&current_lock, &[incoming_file], project.path()).unwrap();
        assert!(operations.iter().any(|operation| {
            operation.destination == adapted_path && operation.action == OperationAction::Create
        }));
        assert!(operations.iter().any(|operation| {
            operation.destination == legacy_path
                && operation.action == OperationAction::DeleteManaged
                && operation.resolution.as_deref() == Some("obsolete_managed_remove")
        }));
    }

    #[test]
    fn an_update_keeps_the_content_of_replaced_generated_files_for_repair() {
        let project = tempdir().unwrap();
        let predecessor: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let readme = b"# Project\n";
        let mut plan = plan();
        plan.maintenance_mode = Some("update".into());
        plan.operations = vec![PlanOperation {
            id: "update-readme".into(),
            component_id: "project.readme".into(),
            ownership: Some(Ownership::Generated),
            location_scope: Some("project".into()),
            action: OperationAction::Replace,
            source_path: Some("generated:README.md".into()),
            destination: "README.md".into(),
            source_sha256: Some(sha256_bytes(readme)),
            source_size: Some(readme.len() as u64),
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: Some(sha256_bytes(readme)),
            base_sha256: None,
            local_sha256: None,
            local_state: LocalState::Unmodified,
            resolution: None,
            external: false,
            rollback: RollbackAction::RestoreBackup,
            external_parent_identity: None,
        }];
        let prepared = vec![PreparedFile {
            operation_id: "update-readme".into(),
            destination: "README.md".into(),
            bytes: readme.to_vec(),
            expected_sha256: sha256_bytes(readme),
        }];
        let journal = new_journal(&plan, &plan.project_id, project.path());
        let project_directory = RootedDir::open_read(project.path()).unwrap();
        let lock = build_lock(
            &plan,
            &prepared,
            &journal,
            Some(&predecessor),
            project.path(),
            &project_directory,
        )
        .unwrap();
        let locked = lock
            .files
            .iter()
            .find(|file| file.path == "README.md")
            .unwrap();
        assert_eq!(locked.generated_content.as_deref(), Some("# Project\n"));
        assert_eq!(locked.generated_bytes.as_deref(), Some(&readme[..]));
    }

    #[test]
    fn an_update_that_keeps_a_file_records_it_at_the_new_revision() {
        let project = tempdir().unwrap();
        fs::write(project.path().join(".gitignore"), b"kept by the user\n").unwrap();
        let kept_sha256 = sha256_bytes(b"kept by the user\n");
        let mut predecessor: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let old_revision = "b".repeat(40);
        predecessor.files = vec![LockedFile {
            path: ".gitignore".into(),
            location_scope: Some("project".into()),
            component_id: "core.agents".into(),
            source_path: ".gitignore".into(),
            source_revision: old_revision.clone(),
            source_sha256: "c".repeat(64),
            source_size: Some(1),
            base_sha256: None,
            installed_sha256: kept_sha256.clone(),
            installed_size: Some(17),
            ownership: Ownership::Merged,
            preserved_local: false,
            external: false,
            generated_content: None,
            generated_bytes: None,
            executable: false,
            platform: Some(ManifestPlatform::All),
        }];

        let mut plan = plan();
        plan.maintenance_mode = Some("update".into());
        assert_ne!(plan.source.resolved_revision, old_revision);
        plan.operations = vec![PlanOperation {
            id: "update-keep".into(),
            component_id: "core.agents".into(),
            ownership: Some(Ownership::Merged),
            location_scope: Some("project".into()),
            action: OperationAction::Skip,
            source_path: Some(".gitignore".into()),
            destination: ".gitignore".into(),
            source_sha256: Some("d".repeat(64)),
            source_size: Some(2),
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: None,
            base_sha256: Some(kept_sha256.clone()),
            local_sha256: Some(kept_sha256.clone()),
            local_state: LocalState::Unmodified,
            resolution: Some("merged_base_required".into()),
            external: false,
            rollback: RollbackAction::RestoreBackup,
            external_parent_identity: None,
        }];
        let journal = new_journal(&plan, &plan.project_id, project.path());
        let project_directory = RootedDir::open_read(project.path()).unwrap();
        let lock = build_lock(
            &plan,
            &[],
            &journal,
            Some(&predecessor),
            project.path(),
            &project_directory,
        )
        .unwrap();

        let kept = lock
            .files
            .iter()
            .find(|file| file.path == ".gitignore")
            .unwrap();
        assert_eq!(kept.source_revision, plan.source.resolved_revision);
        assert_eq!(kept.source_sha256, "d".repeat(64));
        assert_eq!(
            kept.installed_sha256, kept_sha256,
            "the kept bytes stay recorded"
        );
    }

    #[test]
    fn lock_reconciliation_drops_a_future_environment_component_after_its_files_are_removed() {
        let project = tempdir().unwrap();
        let mut predecessor: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let future_id = "runtime.future_client".to_string();
        predecessor.components.push(LockComponent {
            id: future_id.clone(),
            version: None,
            state: "installed".into(),
            source_revision: Some(predecessor.source.revision.clone()),
            validation: Some("pass".into()),
        });
        predecessor.files.push(LockedFile {
            path: ".future-client/agent.md".into(),
            location_scope: Some("project".into()),
            component_id: future_id.clone(),
            source_path: ".future-client/agent.md".into(),
            source_revision: predecessor.source.revision.clone(),
            source_sha256: "a".repeat(64),
            source_size: Some(1),
            base_sha256: None,
            installed_sha256: "a".repeat(64),
            installed_size: Some(1),
            ownership: Ownership::Managed,
            preserved_local: false,
            external: false,
            generated_content: None,
            generated_bytes: None,
            executable: false,
            platform: Some(ManifestPlatform::All),
        });

        let mut plan = plan();
        plan.maintenance_mode = Some("repair".into());
        plan.selected_components = vec!["core.agents".into()];
        plan.operations = vec![PlanOperation {
            id: "remove-future-client".into(),
            component_id: future_id.clone(),
            ownership: Some(Ownership::Managed),
            location_scope: Some("project".into()),
            action: OperationAction::DeleteManaged,
            source_path: None,
            destination: ".future-client/agent.md".into(),
            source_sha256: None,
            source_size: Some(1),
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: None,
            base_sha256: None,
            local_sha256: Some("a".repeat(64)),
            local_state: LocalState::Unmodified,
            resolution: Some("managed_remove".into()),
            external: false,
            rollback: RollbackAction::RestoreBackup,
            external_parent_identity: None,
        }];
        let journal = new_journal(&plan, &plan.project_id, project.path());
        let project_directory = RootedDir::open_read(project.path()).unwrap();
        let lock = build_lock(
            &plan,
            &[],
            &journal,
            Some(&predecessor),
            project.path(),
            &project_directory,
        )
        .unwrap();
        assert!(!lock
            .components
            .iter()
            .any(|component| component.id == future_id));
        assert!(!lock.files.iter().any(|file| file.component_id == future_id));
    }

    #[test]
    fn operation_checkpoint_replays_and_compacts_into_the_full_journal() {
        let root = tempdir().unwrap();
        let plan = plan();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&store, &mut journal).unwrap();

        journal.operations[0].status = "staged".into();
        journal.operations[0].staged_sha256 = Some("a".repeat(64));
        journal.last_checkpoint = "stage-file-op-1".into();
        persist_operation_checkpoint(&store, &mut journal, 0).unwrap();

        let replayed = read_journal(&journal_path).unwrap();
        assert_eq!(replayed.operations[0].status, "staged");
        assert_eq!(replayed.last_checkpoint, "stage-file-op-1");

        compact_operation_checkpoints(&store, &mut journal).unwrap();
        assert!(!operation_checkpoint_root(&journal_path).unwrap().exists());
        assert_eq!(
            read_journal(&journal_path).unwrap().operations[0].status,
            "staged"
        );
    }

    #[test]
    fn journal_error_messages_are_redacted_bounded_and_sanitized_on_read() {
        let root = tempdir().unwrap();
        let plan = plan();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        let secret = ["msy", "secretRecoveryValue123456789"].join("_");
        let secondary_secret = ["synthetic", "client", "credential"].join("-");
        let secondary_name = ["client", "secret"].join("_");
        let quoted_secret = ["synthetic", "private", "credential"].join("-");
        let quoted_name = ["private", "key"].join("_");
        let unsafe_message = format!(
            "validation failed; MESHY_API_KEY={secret}; {secondary_name}={secondary_secret}; \"{quoted_name}\":\"{quoted_secret}\"; {}",
            "🧪".repeat(JOURNAL_ERROR_MESSAGE_MAX_BYTES)
        );
        journal.error = Some(JournalError {
            code: "TRANSACTION_FAILED".into(),
            message: unsafe_message.clone(),
            stage: "validation".into(),
        });

        persist_journal(&store, &mut journal).unwrap();
        let persisted = fs::read_to_string(&journal_path).unwrap();
        assert!(!persisted.contains(&secret));
        assert!(!persisted.contains(&secondary_secret));
        assert!(!persisted.contains(&quoted_secret));
        assert!(persisted.contains("[REDACTED]"));
        let persisted_error = read_journal(&journal_path).unwrap().error.unwrap();
        assert!(persisted_error.message.len() <= JOURNAL_ERROR_MESSAGE_MAX_BYTES);
        assert!(persisted_error.message.ends_with("..."));

        journal.error.as_mut().unwrap().message = unsafe_message;
        fs::write(&journal_path, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();
        let migrated_error = read_journal(&journal_path).unwrap().error.unwrap();
        assert!(!migrated_error.message.contains(&secret));
        assert!(!migrated_error.message.contains(&secondary_secret));
        assert!(!migrated_error.message.contains(&quoted_secret));
        assert!(migrated_error.message.contains("[REDACTED]"));
        assert!(migrated_error.message.len() <= JOURNAL_ERROR_MESSAGE_MAX_BYTES);
        assert!(migrated_error.message.ends_with("..."));
    }

    #[test]
    fn profile_directories_are_created_without_markers_and_rollback_removes_only_empty_ones() {
        let project = tempdir().unwrap();
        let transaction = tempdir().unwrap();
        let mut plan = plan();
        plan.transaction.directories = vec!["events".into(), "localisation/english".into()];
        let journal_path = transaction.path().join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, project.path());
        persist_journal(&store, &mut journal).unwrap();

        apply_profile_directories(project.path(), &plan, &mut journal, &store).unwrap();
        assert!(project.path().join("events").is_dir());
        assert!(project.path().join("localisation/english").is_dir());
        assert!(!project.path().join("events/.gitkeep").exists());
        fs::write(project.path().join("events/user_event.txt"), "user content").unwrap();

        cleanup_created_profile_directories(project.path(), &mut journal, &store).unwrap();
        assert!(project.path().join("events").is_dir());
        assert!(project.path().join("events/user_event.txt").is_file());
        assert!(!project.path().join("localisation/english").exists());
        assert!(!project.path().join("localisation").exists());
    }

    #[test]
    fn operation_checkpoint_size_does_not_grow_with_the_full_plan() {
        let root = tempdir().unwrap();
        let mut plan = plan();
        let template = plan.operations[0].clone();
        plan.operations = (0..1_008)
            .map(|index| PlanOperation {
                id: format!("op-{index:04}"),
                destination: format!("files/file-{index:04}.txt"),
                ..template.clone()
            })
            .collect();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&store, &mut journal).unwrap();

        journal.operations[0].status = "applying".into();
        journal.last_checkpoint = "apply-intent-op-0000".into();
        persist_operation_checkpoint(&store, &mut journal, 0).unwrap();

        let checkpoint_path = operation_checkpoint_root(&journal_path).unwrap();
        let checkpoint_size = fs::metadata(checkpoint_path).unwrap().len();
        let journal_size = fs::metadata(journal_path).unwrap().len();
        assert!(checkpoint_size < 64 * 1024);
        assert!(journal_size > checkpoint_size * 100);
    }

    #[test]
    fn batched_operation_intents_replay_before_compaction() {
        let root = tempdir().unwrap();
        let mut plan = plan();
        let template = plan.operations[0].clone();
        plan.operations = (0..OPERATION_INTENT_BATCH)
            .map(|index| PlanOperation {
                id: format!("op-{index:04}"),
                destination: format!("files/file-{index:04}.txt"),
                ..template.clone()
            })
            .collect();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&store, &mut journal).unwrap();
        let indices = (0..OPERATION_INTENT_BATCH).collect::<Vec<_>>();
        for index in &indices {
            journal.operations[*index].status = "applying".into();
        }
        persist_operation_checkpoint_batch(&store, &mut journal, &indices).unwrap();

        let replayed = read_journal(&journal_path).unwrap();
        assert!(replayed
            .operations
            .iter()
            .all(|operation| operation.status == "applying"));
        compact_operation_checkpoints(&store, &mut journal).unwrap();
        assert!(!operation_checkpoint_root(&journal_path).unwrap().exists());
    }

    #[test]
    fn operation_checkpoint_replay_ignores_only_a_torn_final_record() {
        let root = tempdir().unwrap();
        let plan = plan();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&store, &mut journal).unwrap();

        journal.operations[0].status = "verified".into();
        journal.last_checkpoint = "apply-op-1".into();
        persist_operation_checkpoint(&store, &mut journal, 0).unwrap();
        let checkpoint_path = operation_checkpoint_root(&journal_path).unwrap();
        let mut file = OpenOptions::new()
            .append(true)
            .open(checkpoint_path)
            .unwrap();
        file.write_all(br#"{"schema_version":"1.0.0""#).unwrap();
        file.sync_data().unwrap();

        let replayed = read_journal(&journal_path).unwrap();
        assert_eq!(replayed.operations[0].status, "verified");
        assert_eq!(replayed.last_checkpoint, "apply-op-1");
    }

    #[test]
    fn offline_wiki_validation_accepts_declared_text_and_binary_media() {
        let project = tempdir().unwrap();
        let mut operation = plan().operations[0].clone();
        operation.component_id = "wiki.snapshot".into();

        operation.destination = "paradox_wiki/Overview.md".into();
        validate_managed_bytes(project.path(), &operation, b"# Offline wiki\n").unwrap();

        operation.destination = "paradox_wiki/media/example.svg".into();
        validate_managed_bytes(
            project.path(),
            &operation,
            br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
        )
        .unwrap();

        operation.destination = "paradox_wiki/media/example.png".into();
        validate_managed_bytes(
            project.path(),
            &operation,
            b"\x89PNG\r\n\x1a\nbinary payload",
        )
        .unwrap();

        operation.destination = "paradox_wiki/media/example.jpg".into();
        validate_managed_bytes(
            project.path(),
            &operation,
            b"\xff\xd8\xffbinary payload\xff\xd9",
        )
        .unwrap();
    }

    #[test]
    fn actual_project_state_path_enforces_reasoning_effort_schema() {
        let project = tempdir().unwrap();
        let mut operation = plan().operations[0].clone();
        operation.component_id = "project.state".into();
        operation.destination = ".hoi4-mod-setup/state.json".into();
        let incomplete = br#"{
            "schema_version":"1.0.0",
            "project_id":"example_mod",
            "project_root":"C:/mods/example_mod",
            "platform":"windows",
            "wizard":{"current_step":"ready","completed_steps":[]},
            "preferences":{"telemetry":false},
            "ai":{"provider":"deepseek","model":"deepseek-flash","optimization_profile":"DeepSeek setup analysis"},
            "codex":{"integration":"provider_api","auth_mode":"api_key","auth_status":"configured","analysis_required":true,"analysis_status":"confirmed","account_values_persisted":false},
            "credential_references":[]
        }"#;
        assert!(validate_managed_bytes(project.path(), &operation, incomplete).is_err());
    }

    #[test]
    fn offline_wiki_validation_rejects_malformed_or_unknown_media() {
        let project = tempdir().unwrap();
        let mut operation = plan().operations[0].clone();
        operation.component_id = "wiki.snapshot".into();

        operation.destination = "paradox_wiki/media/example.png".into();
        assert!(validate_managed_bytes(project.path(), &operation, b"not a png").is_err());

        operation.destination = "paradox_wiki/media/example.bin".into();
        assert!(validate_managed_bytes(project.path(), &operation, b"binary").is_err());
    }

    fn existing_file_fixture(project_root: &Path) -> (InstallationPlan, Vec<PreparedFile>) {
        let mut plan = ready_plan(project_root);
        fs::write(project_root.join("AGENTS.md"), "old").unwrap();
        plan.operations[0].action = OperationAction::Replace;
        plan.operations[0].rollback = RollbackAction::RestoreBackup;
        plan.operations[0].local_state = LocalState::Unmodified;
        plan.operations[0].local_sha256 = Some(sha256_bytes(b"old"));
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        (plan, prepared)
    }

    fn absent_root_fixture(parent: &Path) -> (PathBuf, InstallationPlan, Vec<PreparedFile>) {
        let project_root = parent.join("example");
        let mut plan = plan();
        plan.transaction.project_root_mode = ProjectRootMode::CreateLeaf;
        plan.transaction.project_root_parent =
            Some(validate_project_root(parent).unwrap().display().to_string());
        plan.transaction.project_root_leaf = Some("example".into());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        (project_root, plan, prepared)
    }

    #[test]
    fn project_root_identity_binding_rejects_replacement_at_the_reviewed_path() {
        let container = tempdir().unwrap();
        let project_root = container.path().join("project");
        fs::create_dir(&project_root).unwrap();
        let reviewed_identity = RootedDir::open_read(&project_root)
            .unwrap()
            .identity_token()
            .unwrap();
        let mut plan = plan();
        plan.transaction.project_root_identity = Some(reviewed_identity);
        let journal = new_journal(&plan, &plan.project_id, &project_root);

        let moved_root = container.path().join("project-reviewed");
        fs::rename(&project_root, &moved_root).unwrap();
        fs::create_dir(&project_root).unwrap();

        assert!(validate_project_root_lifecycle_identity(
            &project_root,
            &journal.project_root_lifecycle
        )
        .is_err());
    }

    #[test]
    fn create_leaf_binding_rejects_replacement_of_the_reviewed_parent() {
        let container = tempdir().unwrap();
        let parent = container.path().join("mods");
        fs::create_dir(&parent).unwrap();
        let reviewed_identity = RootedDir::open_read(&parent)
            .unwrap()
            .identity_token()
            .unwrap();
        let project_root = parent.join("example");
        let mut plan = plan();
        plan.transaction.project_root_mode = ProjectRootMode::CreateLeaf;
        plan.transaction.project_root_parent = Some(parent.display().to_string());
        plan.transaction.project_root_leaf = Some("example".into());
        plan.transaction.project_root_identity = Some(reviewed_identity);
        let journal = new_journal(&plan, &plan.project_id, &project_root);

        let moved_parent = container.path().join("mods-reviewed");
        fs::rename(&parent, &moved_parent).unwrap();
        fs::create_dir(&parent).unwrap();

        assert!(validate_project_root_lifecycle_identity(
            &project_root,
            &journal.project_root_lifecycle
        )
        .is_err());
    }

    #[test]
    fn transaction_rejects_plans_without_reviewed_root_identity() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = plan();
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];

        let error = run_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("no project-root identity binding"));
        assert!(!app.path().join("transactions").exists());
        assert!(!project.path().join("AGENTS.md").exists());
    }

    #[test]
    fn transaction_rejects_a_root_replaced_after_plan_review() {
        let container = tempdir().unwrap();
        let app = tempdir().unwrap();
        let project_root = container.path().join("project");
        fs::create_dir(&project_root).unwrap();
        let mut plan = plan();
        plan.transaction.project_root_identity = Some(
            RootedDir::open_read(&project_root)
                .unwrap()
                .identity_token()
                .unwrap(),
        );
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        fs::rename(&project_root, container.path().join("project-reviewed")).unwrap();
        fs::create_dir(&project_root).unwrap();

        let error = run_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("changed after planning"));
        assert!(!project_root.join("AGENTS.md").exists());
        assert!(!app.path().join("transactions").exists());
    }

    #[test]
    fn absent_project_root_is_created_only_at_apply_and_removed_by_rollback() {
        let parent = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (project_root, plan, prepared) = absent_root_fixture(parent.path());
        assert!(!project_root.exists());

        let error = run_test_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                fail_before_operation: Some(0),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("fault injected before operation"));
        assert!(project_root.is_dir());

        let journal_path = transaction_root(app.path(), plan.plan_id)
            .transaction
            .join("journal.json");
        let mut journal = read_journal(&journal_path).unwrap();
        assert_eq!(
            journal.project_root_lifecycle.mode,
            ProjectRootMode::CreateLeaf
        );
        assert!(journal.project_root_lifecycle.created_by_transaction);
        assert_eq!(journal.project_root_lifecycle.checkpoint, "created");
        assert!(journal.recovery.project_apply_started);
        assert!(!journal.recovery.resume_allowed);
        assert!(journal.recovery.rollback_allowed);
        assert!(!journal.recovery.discard_staging_allowed);
        assert_eq!(journal.recovery.recommended_action, "rollback");
        rollback_transaction(&project_root, &mut journal, &journal_path).unwrap();
        assert!(!project_root.exists());
        assert_eq!(journal.project_root_lifecycle.checkpoint, "removed");
    }

    #[test]
    fn inverse_rollback_recreates_an_absent_reviewed_root_before_restore() {
        let parent = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (project_root, plan, prepared) = absent_root_fixture(parent.path());
        assert!(run_test_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                fail_before_stage: Some(9),
                ..Default::default()
            },
        )
        .is_err());
        let installation_path = transaction_root(app.path(), plan.plan_id)
            .transaction
            .join("journal.json");
        let mut installation = read_journal(&installation_path).unwrap();

        rollback_transaction(&project_root, &mut installation, &installation_path).unwrap();
        assert!(!project_root.exists());
        let rollback_id = installation.rollback_transaction_id.unwrap();
        let rollback_path = transaction_root(app.path(), rollback_id)
            .transaction
            .join("journal.json");
        let mut rollback = read_journal(&rollback_path).unwrap();

        rollback_transaction(&project_root, &mut rollback, &rollback_path).unwrap();

        assert!(project_root.is_dir());
        assert_eq!(fs::read(project_root.join("AGENTS.md")).unwrap(), b"safe");
        assert!(!project_root
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        assert_eq!(rollback.project_root_lifecycle.checkpoint, "created");
        assert!(rollback.project_root_lifecycle.observed_exists);
    }

    #[test]
    fn pre_apply_failure_leaves_new_root_absent_and_staging_discardable() {
        let parent = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (project_root, plan, prepared) = absent_root_fixture(parent.path());
        assert!(run_test_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                fail_before_stage: Some(8),
                ..Default::default()
            },
        )
        .is_err());
        assert!(!project_root.exists());
        let found = find_incomplete_transaction(app.path(), &project_root)
            .unwrap()
            .expect("absent-root journal should remain discoverable");
        assert!(found.recovery.resume_allowed);
        let discarded = discard_staging(&project_root, app.path(), plan.plan_id).unwrap();
        assert_eq!(discarded.state, "staging_discarded");
        assert!(!project_root.exists());
    }

    #[test]
    fn rollback_preserves_a_created_root_when_unexpected_user_content_appears() {
        let parent = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (project_root, plan, prepared) = absent_root_fixture(parent.path());
        assert!(run_test_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                fail_before_operation: Some(0),
                ..Default::default()
            },
        )
        .is_err());
        fs::write(project_root.join("user-note.txt"), b"keep").unwrap();
        let journal_path = transaction_root(app.path(), plan.plan_id)
            .transaction
            .join("journal.json");
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(&project_root, &mut journal, &journal_path).unwrap();
        assert!(project_root.join("user-note.txt").is_file());
        assert_eq!(
            journal.project_root_lifecycle.cleanup_result.as_deref(),
            Some("retained_user_content")
        );
    }

    #[test]
    fn incomplete_transaction_is_discovered_before_a_new_mutation() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = plan();
        let roots = transaction_root(app.path(), plan.plan_id);
        fs::create_dir_all(&roots.transaction).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, project.path());
        let journal_path = roots.transaction.join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        persist_journal(&store, &mut journal).unwrap();
        let loaded = read_journal(&journal_path).unwrap();
        assert!(roots_match_for_transaction(
            &loaded.project_root,
            project.path()
        ));

        let found = find_incomplete_transaction(app.path(), project.path())
            .unwrap_or_else(|error| panic!("discovery failed: {error}"))
            .unwrap_or_else(|| {
                panic!(
                    "active journal should be visible to the core: {}",
                    journal_path.display()
                )
            });
        assert_eq!(found.transaction_id, plan.plan_id);

        journal.state = "completed".into();
        persist_journal(&store, &mut journal).unwrap();
        assert!(find_incomplete_transaction(app.path(), project.path())
            .unwrap()
            .is_none());
    }

    #[test]
    fn ordinary_active_checkpoints_derive_the_safe_recovery_action() {
        let project = tempdir().unwrap();
        let base = plan();

        let mut before_staging = new_journal(&base, &base.project_id, project.path());
        before_staging.state = "preflight".into();
        normalize_incomplete_recovery(&mut before_staging);
        assert_eq!(
            before_staging.recovery.recommended_action,
            "discard_staging"
        );
        assert!(before_staging.recovery.discard_staging_allowed);
        assert!(!before_staging.recovery.resume_allowed);

        let mut staged = new_journal(&base, &base.project_id, project.path());
        staged.state = "validation".into();
        staged
            .stages
            .iter_mut()
            .find(|stage| stage.id == "staging")
            .unwrap()
            .status = "complete".into();
        normalize_incomplete_recovery(&mut staged);
        assert_eq!(staged.recovery.recommended_action, "resume");
        assert!(staged.recovery.resume_allowed);
        assert!(!staged.recovery.rollback_allowed);

        let mut applying = staged;
        applying.state = "apply".into();
        applying.operations[0].status = "applying".into();
        normalize_incomplete_recovery(&mut applying);
        assert_eq!(applying.recovery.recommended_action, "rollback");
        assert!(applying.recovery.rollback_allowed);
        assert!(!applying.recovery.resume_allowed);
        assert!(!applying.recovery.discard_staging_allowed);
    }

    #[test]
    fn identityless_legacy_journal_is_inspect_only() {
        let project = tempdir().unwrap();
        let base = plan();
        let mut journal = new_journal(&base, &base.project_id, project.path());
        journal.schema_version = "1.0.0".into();
        journal.project_root_lifecycle.root_identity = None;
        journal.state = "interrupted".into();

        normalize_incomplete_recovery(&mut journal);

        assert!(!journal.recovery.resume_allowed);
        assert!(!journal.recovery.rollback_allowed);
        assert!(!journal.recovery.discard_staging_allowed);
        assert_eq!(journal.recovery.recommended_action, "inspect");
    }

    #[test]
    fn transaction_revalidates_prepared_bytes_before_backup() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, mut prepared) = existing_file_fixture(project.path());
        prepared[0].bytes = b"tampered after review".to_vec();

        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("prepared checksum mismatch"));
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        let journal = read_journal(&journal_path).unwrap();
        assert_eq!(journal.last_checkpoint, "selective download");
        assert_eq!(journal.stages[2].status, "active");
    }

    #[test]
    fn transaction_requires_revision_bound_download_evidence_before_backup() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (mut plan, prepared) = existing_file_fixture(project.path());
        bind_first_operation_to_remote_download(&mut plan);
        plan.download_ledger.clear();

        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("revision-bound download evidence"));
        assert!(!app
            .path()
            .join("backups")
            .join(plan.plan_id.to_string())
            .exists());
    }

    #[test]
    fn transaction_rejects_download_evidence_that_differs_from_the_reviewed_operation() {
        for field in ["revision", "manifest", "destination", "executable"] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (mut plan, prepared) = existing_file_fixture(project.path());
            bind_first_operation_to_remote_download(&mut plan);
            if field == "revision" {
                plan.download_ledger[0].source_revision =
                    "699497ea2f93612d9094461c6fde114fc87a5c0f".into();
            } else {
                match field {
                    "manifest" => plan.download_ledger[0].manifest_sha256 = "b".repeat(64),
                    "destination" => {
                        plan.download_ledger[0].destination = "different/AGENTS.md".into()
                    }
                    "executable" => plan.download_ledger[0].executable = true,
                    _ => unreachable!(),
                }
            }

            let error = run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().to_path_buf()),
                    ..Default::default()
                },
            )
            .unwrap_err();

            assert!(error
                .to_string()
                .contains("source download ledger does not match"));
            assert!(!app
                .path()
                .join("backups")
                .join(plan.plan_id.to_string())
                .exists());
        }
    }

    #[test]
    fn git_boundary_faults_leave_no_success_lock_and_remain_recoverable() {
        for boundary in ["before", "after"] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let mut plan = ready_plan(project.path());
            plan.git_setup = Some(crate::git::GitSetup {
                mode: crate::git::GitMode::Skip,
                branch: "main".into(),
                initial_commit: false,
                remote_name: None,
                remote_url: None,
                push_approved: false,
            });
            let prepared = vec![PreparedFile {
                operation_id: "op-1".into(),
                destination: "AGENTS.md".into(),
                bytes: b"safe".to_vec(),
                expected_sha256: sha256_bytes(b"safe"),
            }];

            let result = run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().to_path_buf()),
                    fail_before_git: boundary == "before",
                    fail_after_git: boundary == "after",
                    ..Default::default()
                },
            );

            assert!(result.is_err(), "{boundary}");
            assert!(!project
                .path()
                .join(".hoi4-mod-setup/install.lock.json")
                .exists());
            let journal = find_incomplete_transaction(app.path(), project.path())
                .unwrap()
                .expect("faulted Git boundary should retain a journal");
            assert_eq!(journal.last_checkpoint, "git-intent");
            assert_eq!(journal.recovery.recommended_action, "rollback");
            assert!(journal.recovery.rollback_allowed);
        }
    }

    #[test]
    fn first_install_resumes_after_validation_with_user_facing_launcher_path_and_keeps_user_thumbnail(
    ) {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let canonical_app = fs::canonicalize(app.path()).unwrap();
        let thumbnail = crate::descriptors::placeholder_thumbnail_png().unwrap();
        fs::write(project.path().join("thumbnail.png"), &thumbnail).unwrap();
        let mut plan = ready_plan(project.path());
        let launcher_path = canonical_app.join("example.mod");
        let canonical_project = validate_project_root(project.path()).unwrap();
        let identity = ProjectIdentity {
            display_name: "Example".into(),
            project_id: plan.project_id.clone(),
            author: String::new(),
            version: "0.1.0".into(),
            supported_game_version: "1.17.*".into(),
            project_root: canonical_project.clone(),
            default_branch: "main".into(),
            script_prefix: plan.script_prefix.clone(),
            primary_namespace: plan.primary_namespace.clone(),
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: Some(launcher_path.clone()),
        };
        let launcher_bytes =
            crate::descriptors::render_launcher_descriptor(&identity, &canonical_project)
                .unwrap()
                .into_bytes();
        plan.operations.push(PlanOperation {
            id: "launcher".into(),
            component_id: "project.launcher_descriptor".into(),
            ownership: Some(Ownership::Generated),
            location_scope: Some("external_launcher".into()),
            action: OperationAction::Create,
            source_path: Some("generated:example.mod".into()),
            destination: launcher_path.display().to_string(),
            source_sha256: Some(sha256_bytes(&launcher_bytes)),
            source_size: Some(launcher_bytes.len() as u64),
            platform: None,
            executable: false,
            result_sha256: Some(sha256_bytes(&launcher_bytes)),
            base_sha256: None,
            local_sha256: None,
            local_state: LocalState::Absent,
            resolution: None,
            external: true,
            rollback: RollbackAction::RemoveCreated,
            external_parent_identity: None,
        });
        plan.operations.push(PlanOperation {
            id: "thumbnail".into(),
            component_id: "project.thumbnail".into(),
            ownership: Some(Ownership::Generated),
            location_scope: Some("project".into()),
            action: OperationAction::Skip,
            source_path: Some("generated:thumbnail.png".into()),
            destination: "thumbnail.png".into(),
            source_sha256: Some("c".repeat(64)),
            source_size: Some(1),
            platform: None,
            executable: false,
            result_sha256: None,
            base_sha256: None,
            local_sha256: Some(sha256_bytes(&thumbnail)),
            local_state: LocalState::Modified,
            resolution: Some("keep".into()),
            external: false,
            rollback: RollbackAction::None,
            external_parent_identity: None,
        });

        let validation_stage = TRANSACTION_STAGES
            .iter()
            .position(|stage| *stage == "validation")
            .unwrap();
        let interrupted = run_test_transaction(
            project.path(),
            &plan,
            &[
                PreparedFile {
                    operation_id: "op-1".into(),
                    destination: "AGENTS.md".into(),
                    bytes: b"safe".to_vec(),
                    expected_sha256: sha256_bytes(b"safe"),
                },
                PreparedFile {
                    operation_id: "launcher".into(),
                    destination: launcher_path.display().to_string(),
                    bytes: launcher_bytes.clone(),
                    expected_sha256: sha256_bytes(&launcher_bytes),
                },
            ],
            &TransactionOptions {
                app_data_root: Some(canonical_app.clone()),
                fail_after_stage: Some(validation_stage),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            interrupted
                .to_string()
                .contains("fault injected after stage validation"),
            "{interrupted}"
        );
        assert!(!launcher_path.exists());
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());

        let (_, lock) = resume_transaction(project.path(), &canonical_app, plan.plan_id).unwrap();
        assert!(launcher_path.is_file());

        let locked_thumbnail = lock
            .files
            .iter()
            .find(|file| file.path == "thumbnail.png")
            .expect("kept thumbnail should remain represented in the lock");
        assert!(locked_thumbnail.preserved_local);
        assert_eq!(locked_thumbnail.installed_sha256, sha256_bytes(&thumbnail));
        let reloaded: InstallationLock = serde_json::from_slice(
            &fs::read(project.path().join(".hoi4-mod-setup/install.lock.json")).unwrap(),
        )
        .unwrap();
        let removal = managed_removal_operations(&reloaded, project.path()).unwrap();
        let thumbnail_removal = removal
            .iter()
            .find(|operation| operation.destination == "thumbnail.png")
            .unwrap();
        assert_eq!(thumbnail_removal.action, OperationAction::Skip);
        assert!(project.path().join("thumbnail.png").is_file());
    }

    #[test]
    fn first_install_rejects_launcher_descriptor_for_a_different_complete_root() {
        let project = tempdir().unwrap();
        let other_project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let canonical_app = fs::canonicalize(app.path()).unwrap();
        let mut plan = ready_plan(project.path());
        let launcher_path = canonical_app.join("example.mod");
        let wrong_root = validate_project_root(other_project.path()).unwrap();
        let identity = ProjectIdentity {
            display_name: "Example".into(),
            project_id: plan.project_id.clone(),
            author: String::new(),
            version: "0.1.0".into(),
            supported_game_version: "1.17.*".into(),
            project_root: wrong_root.clone(),
            default_branch: "main".into(),
            script_prefix: plan.script_prefix.clone(),
            primary_namespace: plan.primary_namespace.clone(),
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: Some(launcher_path.clone()),
        };
        let launcher_bytes = crate::descriptors::render_launcher_descriptor(&identity, &wrong_root)
            .unwrap()
            .into_bytes();
        plan.operations.push(PlanOperation {
            id: "launcher".into(),
            component_id: "project.launcher_descriptor".into(),
            ownership: Some(Ownership::Generated),
            location_scope: Some("external_launcher".into()),
            action: OperationAction::Create,
            source_path: Some("generated:example.mod".into()),
            destination: launcher_path.display().to_string(),
            source_sha256: Some(sha256_bytes(&launcher_bytes)),
            source_size: Some(launcher_bytes.len() as u64),
            platform: None,
            executable: false,
            result_sha256: Some(sha256_bytes(&launcher_bytes)),
            base_sha256: None,
            local_sha256: None,
            local_state: LocalState::Absent,
            resolution: None,
            external: true,
            rollback: RollbackAction::RemoveCreated,
            external_parent_identity: None,
        });

        let error = run_test_transaction(
            project.path(),
            &plan,
            &[
                PreparedFile {
                    operation_id: "op-1".into(),
                    destination: "AGENTS.md".into(),
                    bytes: b"safe".to_vec(),
                    expected_sha256: sha256_bytes(b"safe"),
                },
                PreparedFile {
                    operation_id: "launcher".into(),
                    destination: launcher_path.display().to_string(),
                    bytes: launcher_bytes.clone(),
                    expected_sha256: sha256_bytes(&launcher_bytes),
                },
            ],
            &TransactionOptions {
                app_data_root: Some(canonical_app),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("launcher descriptor path does not match the selected project root"));
        assert!(!launcher_path.exists());
        assert!(!project.path().join("AGENTS.md").exists());
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
    }

    #[test]
    fn transaction_process_fault_worker() {
        let mode = match std::env::var("HOI4_MOD_SETUP_TEST_WORKER") {
            Ok(mode) => mode,
            Err(_) => return,
        };
        let project_root = PathBuf::from(std::env::var_os("HOI4_MOD_SETUP_TEST_PROJECT").unwrap());
        let app_root = PathBuf::from(std::env::var_os("HOI4_MOD_SETUP_TEST_APP").unwrap());
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&app_root).unwrap();
        let (plan, prepared) = existing_file_fixture(&project_root);
        let (mut journal, _) = run_test_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app_root.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        if mode == "rollback_after_backup" || mode.starts_with("after_rollback_") {
            let journal_path = app_root
                .join("transactions")
                .join(plan.plan_id.to_string())
                .join("journal.json");
            rollback_transaction(&project_root, &mut journal, &journal_path).unwrap();
        }
    }

    #[test]
    fn transaction_inverse_process_fault_worker() {
        if std::env::var("HOI4_MOD_SETUP_TEST_WORKER").ok().as_deref()
            != Some("inverse_rollback_after_backup")
        {
            return;
        }
        let project_root = PathBuf::from(std::env::var_os("HOI4_MOD_SETUP_TEST_PROJECT").unwrap());
        let app_root = PathBuf::from(std::env::var_os("HOI4_MOD_SETUP_TEST_APP").unwrap());
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&app_root).unwrap();
        let (plan, prepared) = existing_file_fixture(&project_root);
        let (mut installation, _) = run_test_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app_root.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let installation_path = app_root
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        rollback_transaction(&project_root, &mut installation, &installation_path).unwrap();
        let rollback_id = installation.rollback_transaction_id.unwrap();
        let rollback_path = app_root
            .join("transactions")
            .join(rollback_id.to_string())
            .join("journal.json");
        let mut rollback = read_journal(&rollback_path).unwrap();
        rollback_transaction(&project_root, &mut rollback, &rollback_path).unwrap();
    }

    #[test]
    fn transaction_absent_root_process_fault_worker() {
        let mode = match std::env::var("HOI4_MOD_SETUP_ABSENT_ROOT_WORKER") {
            Ok(mode) => mode,
            Err(_) => return,
        };
        let project_root = PathBuf::from(std::env::var_os("HOI4_MOD_SETUP_TEST_PROJECT").unwrap());
        let app_root = PathBuf::from(std::env::var_os("HOI4_MOD_SETUP_TEST_APP").unwrap());
        let parent = project_root.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::create_dir_all(&app_root).unwrap();
        let (fixture_root, plan, prepared) = absent_root_fixture(parent);
        assert_eq!(fixture_root, project_root);

        if matches!(
            mode.as_str(),
            "before_project_root_create" | "after_project_root_create"
        ) {
            let _ = run_test_transaction(
                &project_root,
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app_root),
                    ..Default::default()
                },
            );
            return;
        }

        assert!(run_test_transaction(
            &project_root,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app_root.clone()),
                fail_before_stage: Some(9),
                ..Default::default()
            },
        )
        .is_err());
        let installation_path = transaction_root(&app_root, plan.plan_id)
            .transaction
            .join("journal.json");
        let mut installation = read_journal(&installation_path).unwrap();
        rollback_transaction(&project_root, &mut installation, &installation_path).unwrap();

        if matches!(
            mode.as_str(),
            "before_inverse_project_root_create" | "after_inverse_project_root_create"
        ) {
            let rollback_id = installation.rollback_transaction_id.unwrap();
            let rollback_path = transaction_root(&app_root, rollback_id)
                .transaction
                .join("journal.json");
            let mut rollback = read_journal(&rollback_path).unwrap();
            rollback_transaction(&project_root, &mut rollback, &rollback_path).unwrap();
        }
    }

    #[test]
    fn cross_process_finalization_and_rollback_boundaries_are_recoverable() {
        for mode in [
            "after_rollback_record",
            "after_lock_write",
            "rollback_after_backup",
            "after_rollback_child_record",
            "after_rollback_parent_record",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "transaction::tests::transaction_process_fault_worker",
                    "--nocapture",
                ])
                .env("HOI4_MOD_SETUP_TEST_WORKER", mode)
                .env("HOI4_MOD_SETUP_TEST_ABORT_AT", mode)
                .env("HOI4_MOD_SETUP_TEST_PROJECT", project.path())
                .env("HOI4_MOD_SETUP_TEST_APP", app.path())
                .output()
                .unwrap();
            assert!(
                !output.status.success(),
                "fault worker unexpectedly succeeded for {mode}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            let transaction_entries = fs::read_dir(app.path().join("transactions"))
                .unwrap()
                .filter_map(Result::ok)
                .collect::<Vec<_>>();
            assert!(!transaction_entries.is_empty(), "no journal for {mode}");
            let mut installation = None;
            let mut rollback = None;
            for entry in transaction_entries {
                let journal_path = entry.path().join("journal.json");
                let journal = read_journal(&journal_path).unwrap();
                if journal.transaction_kind == "rollback" {
                    rollback = Some((journal, journal_path));
                } else {
                    installation = Some((journal, journal_path));
                }
            }
            let (mut installation, installation_path) = installation.expect("installation journal");
            if !mode.starts_with("rollback_") && !mode.starts_with("after_rollback_") {
                assert_eq!(installation.state, "finalizing");
            }
            match mode {
                "after_rollback_record" => {
                    assert!(!project
                        .path()
                        .join(".hoi4-mod-setup/install.lock.json")
                        .exists());
                    assert!(resume_transaction(
                        project.path(),
                        app.path(),
                        installation.transaction_id
                    )
                    .is_err());
                }
                "after_lock_write" => {
                    assert!(project
                        .path()
                        .join(".hoi4-mod-setup/install.lock.json")
                        .is_file());
                    let (reconciled, _) =
                        resume_transaction(project.path(), app.path(), installation.transaction_id)
                            .unwrap();
                    assert_eq!(reconciled.state, "completed");
                }
                "rollback_after_backup" => {
                    let (rollback_journal, rollback_path) = rollback.expect("rollback journal");
                    assert_eq!(installation.state, "rolling_back");
                    assert_eq!(rollback_journal.state, "applying");
                    assert_eq!(
                        rollback_journal.parent_transaction_id,
                        Some(installation.transaction_id)
                    );
                    assert!(rollback_journal.operations[0].backup_path.is_some());
                    rollback_transaction(project.path(), &mut installation, &installation_path)
                        .unwrap();
                    let resumed_rollback = read_journal(&rollback_path).unwrap();
                    assert_eq!(resumed_rollback.state, "completed");
                    assert!(resumed_rollback
                        .stages
                        .iter()
                        .all(|stage| matches!(stage.status.as_str(), "complete" | "skipped")));
                    assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
                }
                "after_rollback_child_record" | "after_rollback_parent_record" => {
                    let (rollback_journal, rollback_path) = rollback.expect("rollback journal");
                    assert_eq!(installation.state, "rolling_back");
                    let discovered = find_incomplete_transaction(app.path(), project.path())
                        .unwrap()
                        .expect("parent rollback should be discoverable");
                    assert_eq!(discovered.transaction_id, installation.transaction_id);
                    assert!(rollback_journal.rollback_record_sha256.is_some());
                    assert!(rollback_path
                        .parent()
                        .unwrap()
                        .join("rollback-record.json")
                        .is_file());
                    if mode == "after_rollback_parent_record" {
                        assert!(installation.rollback_record_sha256.is_some());
                        assert!(installation_path
                            .parent()
                            .unwrap()
                            .join("rollback-record.json")
                            .is_file());
                    }
                    rollback_transaction(project.path(), &mut installation, &installation_path)
                        .unwrap();
                    assert_eq!(
                        read_journal(&installation_path).unwrap().state,
                        "rolled_back"
                    );
                    let completed_rollback = read_journal(&rollback_path).unwrap();
                    assert_eq!(completed_rollback.state, "completed");
                    assert!(completed_rollback
                        .stages
                        .iter()
                        .all(|stage| matches!(stage.status.as_str(), "complete" | "skipped")));
                    assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn cross_process_inverse_rollback_boundary_is_recoverable() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mode = "inverse_rollback_after_backup";
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "transaction::tests::transaction_inverse_process_fault_worker",
                "--nocapture",
            ])
            .env("HOI4_MOD_SETUP_TEST_WORKER", mode)
            .env("HOI4_MOD_SETUP_TEST_ABORT_AT", mode)
            .env("HOI4_MOD_SETUP_TEST_PROJECT", project.path())
            .env("HOI4_MOD_SETUP_TEST_APP", app.path())
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "inverse fault worker unexpectedly succeeded: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let mut journals = fs::read_dir(app.path().join("transactions"))
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path().join("journal.json");
                read_journal(&path).ok().map(|journal| (journal, path))
            })
            .collect::<Vec<_>>();
        journals.sort_by_key(|(journal, _)| journal.created_at.clone());
        let (installation, _) = journals
            .iter()
            .find(|(journal, _)| journal.transaction_kind == "installation")
            .expect("installation journal");
        let (rollback, rollback_path) = journals
            .iter()
            .find(|(journal, _)| {
                journal.transaction_kind == "rollback"
                    && journal.parent_transaction_id == Some(installation.transaction_id)
            })
            .expect("rollback journal");
        let (inverse, _) = journals
            .iter()
            .find(|(journal, _)| {
                journal.transaction_kind == "rollback"
                    && journal.parent_transaction_id == Some(rollback.transaction_id)
            })
            .expect("inverse rollback journal");
        assert_eq!(rollback.state, "rolling_back");
        assert_eq!(rollback.operations[0].status, "rolled_back");
        assert_eq!(inverse.state, "applying");
        let inverse_backup = inverse.operations[0]
            .backup_path
            .as_ref()
            .expect("inverse rollback should back up the live pre-inverse bytes");
        assert_eq!(fs::read(inverse_backup).unwrap(), b"old");

        let mut rollback = read_journal(rollback_path).unwrap();
        rollback_transaction(project.path(), &mut rollback, rollback_path).unwrap();
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    #[test]
    fn absent_root_create_remove_and_inverse_boundaries_are_recoverable() {
        for mode in [
            "before_project_root_create",
            "after_project_root_create",
            "before_project_root_remove",
            "after_project_root_remove",
            "before_inverse_project_root_create",
            "after_inverse_project_root_create",
        ] {
            let parent = tempdir().unwrap();
            let app = tempdir().unwrap();
            let project_root = parent.path().join("example");
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "transaction::tests::transaction_absent_root_process_fault_worker",
                    "--nocapture",
                ])
                .env("HOI4_MOD_SETUP_ABSENT_ROOT_WORKER", mode)
                .env("HOI4_MOD_SETUP_TEST_ABORT_AT", mode)
                .env("HOI4_MOD_SETUP_TEST_PROJECT", &project_root)
                .env("HOI4_MOD_SETUP_TEST_APP", app.path())
                .output()
                .unwrap();
            assert!(
                !output.status.success(),
                "absent-root fault worker unexpectedly succeeded for {mode}: {}",
                String::from_utf8_lossy(&output.stdout)
            );

            let journals = fs::read_dir(app.path().join("transactions"))
                .unwrap()
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let path = entry.path().join("journal.json");
                    read_journal(&path).ok().map(|journal| (journal, path))
                })
                .collect::<Vec<_>>();
            let (installation, installation_path) = journals
                .iter()
                .find(|(journal, _)| journal.transaction_kind == "installation")
                .cloned()
                .expect("installation journal");

            if mode.ends_with("project_root_create") && !mode.contains("inverse") {
                let mut installation = installation;
                rollback_transaction(&project_root, &mut installation, &installation_path).unwrap();
                if mode == "after_project_root_create" {
                    assert!(project_root.is_dir());
                    assert!(fs::read_dir(&project_root).unwrap().next().is_none());
                    assert_eq!(
                        installation
                            .project_root_lifecycle
                            .cleanup_result
                            .as_deref(),
                        Some("retained_user_content")
                    );
                } else {
                    assert!(
                        !project_root.exists(),
                        "root remained after recovering {mode}"
                    );
                }
                continue;
            }

            if mode.ends_with("project_root_remove") {
                let mut installation = installation;
                rollback_transaction(&project_root, &mut installation, &installation_path).unwrap();
                assert!(
                    !project_root.exists(),
                    "root remained after recovering {mode}"
                );
                continue;
            }

            let (rollback, rollback_path) = journals
                .iter()
                .find(|(journal, _)| {
                    journal.transaction_kind == "rollback"
                        && journal.parent_transaction_id == Some(installation.transaction_id)
                })
                .cloned()
                .expect("ordinary rollback journal");
            if mode == "after_inverse_project_root_create" {
                let (mut inverse, inverse_path) = journals
                    .iter()
                    .find(|(journal, _)| {
                        journal.transaction_kind == "rollback"
                            && journal.parent_transaction_id == Some(rollback.transaction_id)
                    })
                    .cloned()
                    .expect("interrupted inverse rollback journal");
                assert!(rollback_transaction(&project_root, &mut inverse, &inverse_path).is_err());
                assert!(project_root.is_dir());
                assert!(fs::read_dir(&project_root).unwrap().next().is_none());
                assert_eq!(
                    read_journal(&inverse_path)
                        .unwrap()
                        .recovery
                        .recommended_action,
                    "inspect"
                );
                continue;
            }
            let mut rollback = rollback;
            rollback_transaction(&project_root, &mut rollback, &rollback_path).unwrap();
            assert!(project_root.is_dir(), "root was not restored after {mode}");
            assert_eq!(fs::read(project_root.join("AGENTS.md")).unwrap(), b"safe");
        }
    }

    #[test]
    fn managed_removal_has_a_nonblocking_completion_report() {
        let project = tempdir().unwrap();
        let mut plan = plan();
        plan.maintenance_mode = Some("remove".into());
        plan.operations[0].action = OperationAction::DeleteManaged;
        plan.operations[0].source_sha256 = None;
        plan.operations[0].result_sha256 = None;
        plan.operations[0].local_state = LocalState::Unmodified;
        plan.operations[0].local_sha256 = Some(sha256_bytes(b"safe"));
        let journal = new_journal(&plan, &plan.project_id, project.path());
        let report = build_transaction_readiness(project.path(), &plan, &journal).unwrap();
        assert!(!report.open_in_codex.enabled);
        assert!(report.checks.iter().all(|check| !check.blocking));
        assert_eq!(report.checks[0].id, "installation.removed");
    }

    #[test]
    fn all_skip_removal_writes_an_empty_managed_lock_and_clears_workflow_refs() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let initial_plan = ready_plan(project.path());
        let initial_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (_, mut installed_lock) = run_test_transaction(
            project.path(),
            &initial_plan,
            &initial_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        installed_lock.optional_workflows.insert(
            "workflow.3d".into(),
            OptionalWorkflowLock {
                state: "incomplete".into(),
                reason: Some("missing key".into()),
                credential_reference: Some(
                    "credential://meshy_api_key/00000000-0000-0000-0000-000000000001".into(),
                ),
            },
        );
        installed_lock.optional_workflows.insert(
            "workflow.lora_comfyui_interest".into(),
            OptionalWorkflowLock {
                state: "planned_unavailable".into(),
                reason: Some("legacy interest preference".into()),
                credential_reference: None,
            },
        );
        atomic_write_json(
            &project.path().join(".hoi4-mod-setup/install.lock.json"),
            &installed_lock,
        )
        .unwrap();

        let mut removal_plan = ready_plan(project.path());
        removal_plan.maintenance_mode = Some("remove".into());
        removal_plan.plan_id = Uuid::new_v4();
        removal_plan.codex_analysis = None;
        removal_plan.generated_artifacts.clear();
        removal_plan.external_actions.clear();
        removal_plan.git_setup = None;
        removal_plan.optional_workflows.clear();
        removal_plan.operations =
            managed_removal_operations(&installed_lock, project.path()).unwrap();

        let (_, removal_lock) = run_test_transaction(
            project.path(),
            &removal_plan,
            &[],
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(removal_lock.files.is_empty());
        assert_eq!(
            removal_lock
                .optional_workflows
                .get("workflow.3d")
                .and_then(|workflow| workflow.credential_reference.as_deref()),
            None
        );
        assert!(!removal_lock
            .optional_workflows
            .contains_key("workflow.lora_comfyui_interest"));
        assert!(!project.path().join("AGENTS.md").exists());
    }

    #[test]
    fn managed_removal_validation_does_not_require_codex_semantics() {
        let mut plan = plan();
        plan.maintenance_mode = Some("remove".into());
        plan.codex_analysis = None;
        plan.operations[0].action = OperationAction::DeleteManaged;
        plan.operations[0].source_sha256 = None;
        plan.operations[0].result_sha256 = None;
        plan.operations[0].local_state = LocalState::Unmodified;

        validate_plan(&plan).unwrap();
    }

    fn ready_three_d_action(
        _project_root: &Path,
        _plan: &InstallationPlan,
        component_id: &str,
    ) -> Result<PostInstallActionOutcome, AppError> {
        Ok(PostInstallActionOutcome {
            component_id: component_id.into(),
            state: "ready".into(),
            evidence: "bootstrap exit=0 timed_out=false".into(),
        })
    }

    fn failing_three_d_action(
        _project_root: &Path,
        _plan: &InstallationPlan,
        _component_id: &str,
    ) -> Result<PostInstallActionOutcome, AppError> {
        Err(AppError::Transaction(
            "reviewed 3D bootstrap failed before readiness".into(),
        ))
    }

    fn mismatched_component_action(
        _project_root: &Path,
        _plan: &InstallationPlan,
        _component_id: &str,
    ) -> Result<PostInstallActionOutcome, AppError> {
        Ok(PostInstallActionOutcome {
            component_id: crate::mcp::COMPONENT_ID.into(),
            state: "ready".into(),
            evidence: "wrong component".into(),
        })
    }

    fn reviewed_three_d_external_action() -> ExternalAction {
        ExternalAction {
            id: "external.workflow.3d.bootstrap".into(),
            component_id: "workflow.3d".into(),
            platform: Platform::Windows,
            command_source: "repository_script".into(),
            executable: Some("manifest-declared Python tool".into()),
            arguments: vec![
                ".tools/3d_pipeline/bootstrap_3d_workflow.py".into(),
                "--project-root".into(),
                "<project_root>".into(),
                "--verify-reviewed-config".into(),
                "--quiet".into(),
            ],
            working_directory: Some("<project_root>".into()),
            environment_names: vec!["MESHY_API_KEY".into()],
            network_access: "approved HTTPS dependency and Meshy health endpoints".into(),
            expected_writes: vec![".tools/3d_pipeline/runtime/**".into()],
            privilege: "current_user".into(),
            rollback_boundary: "external dependency state is reported".into(),
            display_command: Some("Verified 3D bootstrap".into()),
            risk: "high".into(),
            requires_approval: true,
            contains_secret: false,
            verified_executable_sha256: None,
            verified_executable_size: None,
            verified_interpreter_sha256: None,
            verified_interpreter_size: None,
            verified_runtime_sha256: None,
            verified_runtime_size: None,
            verified_package_name: None,
            verified_package_version: None,
            verified_package_integrity: None,
            verified_package_tree_sha256: None,
            verified_package_file_count: None,
            verified_runtime_entry: None,
            required_tool_names: vec![],
        }
    }

    fn reviewed_mcp_external_action() -> ExternalAction {
        let mut action = reviewed_three_d_external_action();
        action.id = "external.mcp.hoi4_agent_tools.mcp.hoi4.health".into();
        action.component_id = "mcp.hoi4_agent_tools".into();
        action.arguments = vec!["hoi4-agent-tools.cmd".into()];
        action.environment_names.clear();
        action
    }

    #[test]
    fn reviewed_post_install_action_updates_readiness_and_persisted_workflow_state() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.optional_workflows
            .insert("workflow.3d".into(), "selected_pending".into());
        plan.external_actions = vec![reviewed_three_d_external_action()];
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];

        let (journal, lock) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                post_install_action_runner: Some(ready_three_d_action),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            lock.optional_workflows
                .get("workflow.3d")
                .map(|workflow| workflow.state.as_str()),
            Some("ready")
        );
        assert!(journal.stages[9]
            .evidence
            .iter()
            .any(|item| item.contains("external-action:workflow.3d:ready")));
        let reviewed = journal.stages[9]
            .evidence
            .iter()
            .find(|item| item.starts_with("external-action-reviewed:"))
            .expect("the exact reviewed action must be journaled");
        assert!(reviewed.contains("repository_script"));
        assert!(reviewed.contains("network_access"));
        assert!(reviewed.contains("expected_writes"));
        assert!(reviewed.contains("rollback_boundary"));
    }

    #[test]
    fn reviewed_post_install_action_fault_boundaries_require_rollback() {
        for fail_after in [false, true] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let mut plan = ready_plan(project.path());
            plan.optional_workflows
                .insert("workflow.3d".into(), "selected_pending".into());
            plan.external_actions = vec![reviewed_three_d_external_action()];
            let prepared = vec![PreparedFile {
                operation_id: "op-1".into(),
                destination: "AGENTS.md".into(),
                bytes: b"safe".to_vec(),
                expected_sha256: sha256_bytes(b"safe"),
            }];

            let error = run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    fail_before_post_install_action: !fail_after,
                    fail_after_post_install_action: fail_after,
                    post_install_action_runner: Some(ready_three_d_action),
                    ..Default::default()
                },
            )
            .unwrap_err();

            assert!(error.to_string().contains("reviewed post-install action"));
            assert!(!project
                .path()
                .join(".hoi4-mod-setup/install.lock.json")
                .exists());
            let journal = read_journal(
                &transaction_root(app.path(), plan.plan_id)
                    .transaction
                    .join("journal.json"),
            )
            .unwrap();
            assert_eq!(journal.state, "interrupted");
            assert!(journal.recovery.project_apply_started);
            assert!(journal.recovery.rollback_allowed);
            assert_eq!(journal.recovery.recommended_action, "rollback");
            assert!(journal.stages[9]
                .evidence
                .iter()
                .any(|item| item.starts_with("external-action-reviewed:")));
        }
    }

    #[test]
    fn each_reviewed_post_install_action_has_its_own_fault_checkpoint() {
        let template_root = tempdir().unwrap();
        let mut plan = ready_plan(template_root.path());
        plan.optional_workflows
            .insert(crate::mcp::COMPONENT_ID.into(), "selected_pending".into());
        plan.optional_workflows
            .insert("workflow.3d".into(), "selected_pending".into());
        plan.external_actions = vec![
            reviewed_mcp_external_action(),
            reviewed_three_d_external_action(),
        ];
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];

        for action_index in 0..2 {
            for fail_after in [false, true] {
                let project = tempdir().unwrap();
                let app = tempdir().unwrap();
                plan.plan_id = uuid::Uuid::new_v4();
                let error = run_test_transaction(
                    project.path(),
                    &plan,
                    &prepared,
                    &TransactionOptions {
                        app_data_root: Some(app.path().into()),
                        fail_before_post_install_action_index: (!fail_after)
                            .then_some(action_index),
                        fail_after_post_install_action_index: fail_after.then_some(action_index),
                        post_install_action_runner: Some(ready_three_d_action),
                        ..Default::default()
                    },
                )
                .unwrap_err();
                let component = if action_index == 0 {
                    crate::mcp::COMPONENT_ID
                } else {
                    "workflow.3d"
                };
                assert!(error.to_string().contains(component));
                let journal = read_journal(
                    &transaction_root(app.path(), plan.plan_id)
                        .transaction
                        .join("journal.json"),
                )
                .unwrap();
                assert_eq!(
                    journal.last_checkpoint,
                    format!("post-install-action-intent:{component}")
                );
                assert!(!project
                    .path()
                    .join(".hoi4-mod-setup/install.lock.json")
                    .exists());
            }
        }
    }

    #[test]
    fn failed_post_install_action_never_writes_a_success_lock_and_requires_rollback() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.optional_workflows
            .insert("workflow.3d".into(), "selected_pending".into());
        plan.external_actions = vec![reviewed_three_d_external_action()];
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];

        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                post_install_action_runner: Some(failing_three_d_action),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("reviewed 3D bootstrap failed"));
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        let journal = read_journal(
            &transaction_root(app.path(), plan.plan_id)
                .transaction
                .join("journal.json"),
        )
        .unwrap();
        assert_eq!(journal.state, "interrupted");
        assert!(journal.recovery.project_apply_started);
        assert!(journal.recovery.rollback_allowed);
        assert_eq!(journal.recovery.recommended_action, "rollback");
        assert_eq!(
            journal.error.as_ref().unwrap().stage,
            "post-install-action-intent:workflow.3d"
        );
    }

    #[test]
    fn post_install_result_must_match_the_requested_component() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.optional_workflows
            .insert("workflow.3d".into(), "selected_pending".into());
        plan.external_actions = vec![reviewed_three_d_external_action()];
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                post_install_action_runner: Some(mismatched_component_action),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("returned result for"));
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
    }

    #[test]
    fn repair_does_not_replace_a_healthy_file() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (_, lock) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let operations = repair_operations(&lock, project.path()).unwrap();
        let agents = operations
            .iter()
            .find(|operation| operation.destination == "AGENTS.md")
            .unwrap();
        assert_eq!(agents.action, OperationAction::Skip);
        assert_eq!(agents.local_state, LocalState::Unmodified);
        assert_eq!(
            agents.resolution, None,
            "a healthy file must not ask the user to resolve a conflict"
        );

        fs::write(project.path().join("AGENTS.md"), b"edited locally").unwrap();
        let operations = repair_operations(&lock, project.path()).unwrap();
        let agents = operations
            .iter()
            .find(|operation| operation.destination == "AGENTS.md")
            .unwrap();
        assert_eq!(agents.action, OperationAction::Skip);
        assert_eq!(agents.local_state, LocalState::Modified);
        assert!(
            agents.resolution.is_some(),
            "a local edit still needs review"
        );
    }

    #[test]
    fn first_install_preserved_modification_stays_non_removable() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let preserved = b"user instructions\n";
        fs::write(project.path().join("AGENTS.md"), preserved).unwrap();
        let mut plan = ready_plan(project.path());
        plan.operations[0].action = OperationAction::Skip;
        plan.operations[0].rollback = RollbackAction::None;
        plan.operations[0].local_state = LocalState::Modified;
        plan.operations[0].local_sha256 = Some(sha256_bytes(preserved));
        plan.operations[0].resolution = Some("keep".into());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (_, lock) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let preserved = lock
            .files
            .iter()
            .find(|file| file.path == "AGENTS.md")
            .expect("preserved first-install file should be locked");
        assert!(preserved.preserved_local);
        assert_eq!(
            managed_removal_operations(&lock, project.path()).unwrap()[0].action,
            OperationAction::Skip
        );
    }

    #[test]
    fn apply_then_rollback_restores_original_hashes() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        fs::write(project.path().join("AGENTS.md"), "old").unwrap();
        plan.operations[0].action = OperationAction::Replace;
        plan.operations[0].rollback = RollbackAction::RestoreBackup;
        plan.operations[0].local_state = LocalState::Unmodified;
        plan.operations[0].local_sha256 = Some(sha256_bytes(b"old"));
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (mut journal, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        let rollback_id = journal
            .rollback_transaction_id
            .expect("rollback must have a child transaction");
        let rollback_journal_path = app
            .path()
            .join("transactions")
            .join(rollback_id.to_string())
            .join("journal.json");
        let rollback_journal = read_journal(&rollback_journal_path).unwrap();
        assert_eq!(rollback_journal.transaction_kind, "rollback");
        assert_eq!(
            rollback_journal.parent_transaction_id,
            Some(journal.transaction_id)
        );
        assert_eq!(rollback_journal.state, "completed");
        assert!(rollback_journal.recovery.rollback_allowed);
        assert_eq!(rollback_journal.result_lock_exists, Some(false));
        let rollback_backup = rollback_journal.operations[0]
            .backup_path
            .as_ref()
            .map(PathBuf::from)
            .expect("rollback should retain an inverse backup");
        assert_eq!(fs::read(rollback_backup).unwrap(), b"safe");
        assert!(rollback_journal_path
            .parent()
            .unwrap()
            .join("rollback-record.json")
            .is_file());
        let mut rollback_journal = rollback_journal;
        rollback_transaction(
            project.path(),
            &mut rollback_journal,
            &rollback_journal_path,
        )
        .unwrap();
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
        assert!(project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .is_file());
        assert_eq!(rollback_journal.state, "rolled_back");
        assert!(rollback_journal.rollback_transaction_id.is_some());
    }

    #[test]
    fn inverse_rollback_refuses_a_user_modified_file() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let (mut installation, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let installation_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        rollback_transaction(project.path(), &mut installation, &installation_path).unwrap();
        let rollback_id = installation.rollback_transaction_id.unwrap();
        let rollback_path = app
            .path()
            .join("transactions")
            .join(rollback_id.to_string())
            .join("journal.json");
        let mut rollback = read_journal(&rollback_path).unwrap();
        fs::write(project.path().join("AGENTS.md"), b"user edit").unwrap();

        let error = rollback_transaction(project.path(), &mut rollback, &rollback_path)
            .expect_err("inverse rollback must refuse a later user edit");
        assert!(error.to_string().contains("user changes detected"));
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"user edit"
        );
        assert_eq!(rollback.state, "rolling_back");
    }

    #[test]
    fn inverse_rollback_checks_the_recorded_lock_before_file_apply() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let (mut installation, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let installation_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        rollback_transaction(project.path(), &mut installation, &installation_path).unwrap();
        let rollback_id = installation.rollback_transaction_id.unwrap();
        let rollback_path = app
            .path()
            .join("transactions")
            .join(rollback_id.to_string())
            .join("journal.json");
        let mut rollback = read_journal(&rollback_path).unwrap();
        fs::write(
            project.path().join(".hoi4-mod-setup/install.lock.json"),
            b"user-created lock",
        )
        .unwrap();

        let error = rollback_transaction(project.path(), &mut rollback, &rollback_path)
            .expect_err("inverse rollback must refuse a changed lock");
        assert!(error.to_string().contains("installation lock changed"));
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
        assert_eq!(
            fs::read(project.path().join(".hoi4-mod-setup/install.lock.json")).unwrap(),
            b"user-created lock"
        );
    }

    #[test]
    fn rollback_never_removes_an_explicitly_skipped_file() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let preserved_path = project.path().join("user-owned.txt");
        fs::write(&preserved_path, b"user edit").unwrap();
        let mut plan = ready_plan(project.path());
        plan.operations.push(PlanOperation {
            id: "op-preserve".into(),
            component_id: "project.user-owned".into(),
            ownership: Some(Ownership::External),
            location_scope: Some("project".into()),
            action: OperationAction::Skip,
            source_path: None,
            destination: "user-owned.txt".into(),
            source_sha256: None,
            source_size: None,
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: None,
            base_sha256: None,
            local_sha256: Some(sha256_bytes(b"user edit")),
            local_state: LocalState::Modified,
            resolution: Some("keep".into()),
            external: false,
            rollback: RollbackAction::None,
            external_parent_identity: None,
        });
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (mut journal, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&preserved_path).unwrap(), b"user edit");
    }

    #[test]
    fn rollback_resumes_an_operation_checkpoint_after_restore() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        fs::write(project.path().join("AGENTS.md"), b"old").unwrap();
        plan.operations[0].action = OperationAction::Replace;
        plan.operations[0].rollback = RollbackAction::RestoreBackup;
        plan.operations[0].local_state = LocalState::Unmodified;
        plan.operations[0].local_sha256 = Some(sha256_bytes(b"old"));
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (mut journal, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        fs::write(project.path().join("AGENTS.md"), b"old").unwrap();
        journal.state = "rolling_back".into();
        journal.recovery.rollback_allowed = true;
        journal.recovery.project_apply_started = true;
        journal.operations[0].status = "rollback_applying".into();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(journal.operations[0].status, "rolled_back");
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
    }

    #[test]
    fn merged_ownership_survives_a_replace_and_stays_non_removable() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut first_plan = ready_plan(project.path());
        first_plan.operations[0].ownership = Some(Ownership::Merged);
        first_plan.operations[0].result_sha256 = Some(sha256_bytes(b"safe"));
        let first_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (_, first_lock) = run_test_transaction(
            project.path(),
            &first_plan,
            &first_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            first_lock
                .files
                .iter()
                .find(|file| file.path == "AGENTS.md")
                .unwrap()
                .ownership,
            Ownership::Merged
        );

        let mut second_plan = ready_plan(project.path());
        second_plan.operations[0].action = OperationAction::Replace;
        second_plan.operations[0].ownership = Some(Ownership::Merged);
        second_plan.operations[0].local_state = LocalState::Unmodified;
        second_plan.operations[0].local_sha256 = Some(sha256_bytes(b"safe"));
        second_plan.operations[0].result_sha256 = Some(sha256_bytes(b"new"));
        let second_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"new".to_vec(),
            expected_sha256: sha256_bytes(b"new"),
        }];
        let (_, second_lock) = run_test_transaction(
            project.path(),
            &second_plan,
            &second_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let merged = second_lock
            .files
            .iter()
            .find(|file| file.path == "AGENTS.md")
            .unwrap();
        assert_eq!(merged.ownership, Ownership::Merged);
        assert_eq!(
            managed_removal_operations(&second_lock, project.path()).unwrap()[0].action,
            OperationAction::Skip
        );
    }

    #[test]
    fn plans_without_ownership_evidence_are_rejected_before_mutation() {
        let project = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.operations[0].ownership = None;
        let error = validate_plan(&plan).unwrap_err();
        assert!(error
            .to_string()
            .contains("operation ownership is required"));
    }

    #[test]
    fn configured_git_remote_requires_explicit_plan_approval() {
        let project = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.git_setup = Some(crate::git::GitSetup {
            mode: crate::git::GitMode::Initialize,
            branch: "main".into(),
            initial_commit: false,
            remote_name: Some("origin".into()),
            remote_url: Some("https://github.com/example/mod.git".into()),
            push_approved: false,
        });
        let error = validate_plan(&plan).unwrap_err();
        assert!(error
            .to_string()
            .contains("configured Git remote requires explicit approval"));
    }

    #[test]
    fn setup_provider_does_not_restrict_development_client_components() {
        let project = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.ai_provider = "claude".into();
        plan.ai_model = "claude-model".into();
        plan.ai_reasoning_effort = "high".into();
        plan.ai_endpoint = Some("https://api.anthropic.com/v1/messages".into());
        plan.ai_optimization_profile = "Claude setup analysis".into();
        let analysis = plan.codex_analysis.as_mut().unwrap();
        analysis.engine = "provider_api".into();
        analysis.auth_mode = "api_key".into();
        analysis.provider = Some("claude".into());
        analysis.model = Some("claude-model".into());
        analysis.reasoning_effort = Some("high".into());
        analysis.optimization_profile = Some("Claude setup analysis".into());
        plan.selected_components.push("codex.config".into());
        plan.flatten_chat_sources = true;
        validate_plan(&plan).unwrap();
    }

    #[test]
    fn rollback_rejects_a_different_project_root() {
        let project = tempdir().unwrap();
        let other_project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (mut journal, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        let error =
            rollback_transaction(other_project.path(), &mut journal, &journal_path).unwrap_err();
        assert!(error.to_string().contains("does not match"));
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    #[test]
    fn rollback_restores_predecessor_lock_after_a_successful_maintenance_transaction() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut first_plan = ready_plan(project.path());
        first_plan.operations[0].source_sha256 = Some(sha256_bytes(b"old"));
        first_plan.operations[0].source_size = Some(3);
        let first_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"old".to_vec(),
            expected_sha256: sha256_bytes(b"old"),
        }];
        let (_, _) = run_test_transaction(
            project.path(),
            &first_plan,
            &first_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let lock_path = project.path().join(".hoi4-mod-setup/install.lock.json");
        let predecessor = fs::read(&lock_path).unwrap();

        let mut second_plan = ready_plan(project.path());
        second_plan.operations[0].action = OperationAction::Replace;
        second_plan.operations[0].local_state = LocalState::Unmodified;
        second_plan.operations[0].local_sha256 = Some(sha256_bytes(b"old"));
        second_plan.operations[0].result_sha256 = Some(sha256_bytes(b"new"));
        second_plan.operations[0].source_size = Some(3);
        let second_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"new".to_vec(),
            expected_sha256: sha256_bytes(b"new"),
        }];
        let (mut journal, _) = run_test_transaction(
            project.path(),
            &second_plan,
            &second_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(second_plan.plan_id.to_string())
            .join("journal.json");
        assert_ne!(fs::read(&lock_path).unwrap(), predecessor);
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&lock_path).unwrap(), predecessor);
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
    }

    #[test]
    fn blocked_readiness_never_writes_a_success_lock() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = plan();
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("readiness is blocked"));
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn failed_mcp_bootstrap_cause_reaches_recovery_without_a_success_lock() {
        fn fail_mcp(
            _: &Path,
            _: &InstallationPlan,
            component: &str,
        ) -> Result<PostInstallActionOutcome, AppError> {
            Ok(PostInstallActionOutcome {
                component_id: component.into(),
                state: "incomplete".into(),
                evidence: "MCP package tree does not match the reviewed release. client_secret=private-regression-value".into(),
            })
        }
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.selected_components
            .push(crate::mcp::COMPONENT_ID.into());
        plan.optional_workflows
            .insert(crate::mcp::COMPONENT_ID.into(), "selected_pending".into());
        plan.external_actions = vec![reviewed_mcp_external_action()];
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                post_install_action_runner: Some(fail_mcp),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("MCP package tree does not match"),
            "{error}"
        );
        assert!(!error.to_string().contains("private-regression-value"));
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        let journal_path = transaction_root(app.path(), plan.plan_id)
            .transaction
            .join("journal.json");
        let mut journal = read_journal(&journal_path).unwrap();
        assert!(journal.recovery.rollback_allowed);
        assert!(journal
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("MCP package tree does not match"));
        assert!(!serde_json::to_string(&journal)
            .unwrap()
            .contains("private-regression-value"));
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert!(!project.path().join("AGENTS.md").exists());
    }

    #[test]
    fn injected_failure_does_not_write_success_lock() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let result = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_after_operation: Some(0),
                ..Default::default()
            },
        );
        assert!(result.is_err());
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        let journal = read_journal(
            &app.path()
                .join("transactions")
                .join(plan.plan_id.to_string())
                .join("journal.json"),
        )
        .unwrap();
        assert_eq!(journal.state, "interrupted");
    }

    #[test]
    fn interrupted_pre_apply_transaction_can_resume_from_verified_staging() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let result = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_before_stage: Some(7),
                ..Default::default()
            },
        );
        assert!(result.is_err());
        let staging = app
            .path()
            .join("staging")
            .join(plan.plan_id.to_string())
            .join("AGENTS.md");
        assert!(staging.is_file());
        let (journal, lock) = resume_transaction(project.path(), app.path(), plan.plan_id).unwrap();
        assert_eq!(journal.state, "completed");
        assert_eq!(lock.files[0].installed_sha256, sha256_bytes(b"safe"));
        assert_eq!(
            journal.operations[0].source_path.as_deref(),
            Some("generated:test")
        );
        assert_eq!(journal.operations[0].source_size, Some(4));
        assert_eq!(journal.operations[0].resolution, None);
        assert_eq!(
            journal.operations[0].staged_sha256,
            Some(sha256_bytes(b"safe"))
        );
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    #[test]
    fn interrupted_after_apply_refuses_resume() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        assert!(run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_after_operation: Some(0),
                ..Default::default()
            },
        )
        .is_err());
        let error = resume_transaction(project.path(), app.path(), plan.plan_id).unwrap_err();
        assert!(error.to_string().contains("rollback"));
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    #[test]
    fn finalizing_journal_can_be_reconciled_after_lock_commit() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (_, lock) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        let mut journal = read_journal(&journal_path).unwrap();
        journal.state = "finalizing".into();
        journal.recovery.resume_allowed = true;
        journal.recovery.recommended_action = "resume".into();
        atomic_write_json(&journal_path, &journal).unwrap();
        let (reconciled, recovered_lock) =
            resume_transaction(project.path(), app.path(), plan.plan_id).unwrap();
        assert_eq!(reconciled.state, "completed");
        assert_eq!(
            recovered_lock.files[0].installed_sha256,
            lock.files[0].installed_sha256
        );
    }

    #[test]
    fn finalization_rejects_a_substituted_success_lock() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        let lock_path = project.path().join(".hoi4-mod-setup/install.lock.json");
        let mut lock: serde_json::Value =
            serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
        lock["ai_model"] = serde_json::Value::String("substituted-model".into());
        atomic_write_json(&lock_path, &lock).unwrap();
        let mut journal = read_journal(&journal_path).unwrap();
        journal.state = "finalizing".into();
        journal.recovery.resume_allowed = true;
        journal.recovery.recommended_action = "resume".into();
        atomic_write_json(&journal_path, &journal).unwrap();

        let error = resume_transaction(project.path(), app.path(), plan.plan_id).unwrap_err();
        assert!(error.to_string().contains("success lock checksum mismatch"));
        assert_eq!(read_journal(&journal_path).unwrap().state, "finalizing");
    }

    #[test]
    fn ordinary_rollback_refuses_a_later_lock_edit_before_file_restore() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let (mut journal, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json");
        let lock_path = project.path().join(".hoi4-mod-setup/install.lock.json");
        let mut lock: serde_json::Value =
            serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
        lock["ai_model"] = serde_json::Value::String("later-model".into());
        atomic_write_json(&lock_path, &lock).unwrap();

        let error = rollback_transaction(project.path(), &mut journal, &journal_path)
            .expect_err("rollback must not overwrite a later lock edit");
        assert!(error.to_string().contains("installation lock changed"));
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    #[test]
    fn pre_apply_resume_refuses_a_missing_predecessor_lock() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (first_plan, first_prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &first_plan,
            &first_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let lock_path = project.path().join(".hoi4-mod-setup/install.lock.json");
        let mut second_plan = ready_plan(project.path());
        second_plan.operations[0].action = OperationAction::Replace;
        second_plan.operations[0].local_state = LocalState::Unmodified;
        second_plan.operations[0].local_sha256 = Some(sha256_bytes(b"safe"));
        second_plan.operations[0].result_sha256 = Some(sha256_bytes(b"new"));
        let second_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"new".to_vec(),
            expected_sha256: sha256_bytes(b"new"),
        }];
        assert!(run_test_transaction(
            project.path(),
            &second_plan,
            &second_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_before_stage: Some(7),
                ..Default::default()
            },
        )
        .is_err());
        fs::remove_file(&lock_path).unwrap();
        let error = resume_transaction(project.path(), app.path(), second_plan.plan_id)
            .expect_err("resume must not rebuild a maintenance transaction without its lock");
        assert!(error.to_string().contains("predecessor installation lock"));
    }

    #[test]
    fn discard_staging_preserves_journal_and_removes_only_staging() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        assert!(run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_before_stage: Some(7),
                ..Default::default()
            },
        )
        .is_err());
        let journal = discard_staging(project.path(), app.path(), plan.plan_id).unwrap();
        assert_eq!(journal.state, "staging_discarded");
        assert!(!app
            .path()
            .join("staging")
            .join(plan.plan_id.to_string())
            .exists());
        assert!(app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string())
            .join("journal.json")
            .is_file());
    }

    #[test]
    fn discard_staging_rejects_a_different_project_root() {
        let project = tempdir().unwrap();
        let other_project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        assert!(run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_before_stage: Some(7),
                ..Default::default()
            },
        )
        .is_err());
        let error = discard_staging(other_project.path(), app.path(), plan.plan_id).unwrap_err();
        assert!(error.to_string().contains("does not match"));
        assert!(app
            .path()
            .join("staging")
            .join(plan.plan_id.to_string())
            .is_dir());
    }

    #[test]
    fn failure_before_final_rollback_checkpoint_does_not_write_lock() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let result = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_before_stage: Some(11),
                ..Default::default()
            },
        );
        assert!(result.is_err());
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
    }

    #[test]
    fn repair_and_removal_preserve_modified_files() {
        let project = tempdir().unwrap();
        fs::write(project.path().join("managed.txt"), b"user edit").unwrap();
        let lock = InstallationLock {
            schema_version: "1.0.0".into(),
            project_id: "demo".into(),
            script_prefix: Some("demo".into()),
            primary_namespace: Some("demo".into()),
            installed_at: Utc::now().to_rfc3339(),
            updated_at: None,
            source: LockSourceIdentity {
                repository: "owner/repo".into(),
                mode: SourceMode::PinnedCommit,
                revision: "599497ea2f93612d9094461c6fde114fc87a5c0f".into(),
                requested_ref: None,
                release: None,
                manifest_sha256: "a".repeat(64),
                manifest_origin: "remote".into(),
            },
            ai_provider: "codex".into(),
            ai_model: "gpt-5.6-luna".into(),
            ai_reasoning_effort: "xhigh".into(),
            ai_endpoint: None,
            ai_optimization_profile: crate::models::default_ai_optimization_profile(),
            primary_coding_environment: "codex".into(),
            additional_coding_environments: vec![],
            flatten_chat_sources: false,
            codex_analysis: None,
            wiki_required_pages: manifest_wiki_pages(),
            wiki_metadata: None,
            components: vec![],
            files: vec![LockedFile {
                path: "managed.txt".into(),
                location_scope: None,
                component_id: "core".into(),
                source_path: "managed.txt".into(),
                source_revision: "599497ea2f93612d9094461c6fde114fc87a5c0f".into(),
                source_sha256: sha256_bytes(b"incoming"),
                source_size: Some(8),
                base_sha256: None,
                installed_sha256: sha256_bytes(b"installed"),
                installed_size: Some(9),
                ownership: Ownership::Managed,
                preserved_local: false,
                external: false,
                generated_content: None,
                generated_bytes: None,
                executable: false,
                platform: Some(ManifestPlatform::All),
            }],
            merge_choices: vec![],
            optional_workflows: std::collections::BTreeMap::new(),
            portrait_pipeline: None,
            local_modifications: vec![],
            rollback_records: vec![],
        };
        let repair = repair_operations(&lock, project.path()).unwrap();
        assert_eq!(repair[0].action, OperationAction::Skip);
        let removal = managed_removal_operations(&lock, project.path()).unwrap();
        assert_eq!(removal[0].action, OperationAction::Skip);
        let reinstall = reinstall_operations(&lock, project.path()).unwrap();
        assert_eq!(reinstall[0].action, OperationAction::Skip);
    }

    #[test]
    fn merged_files_require_reverse_merge_review_on_removal() {
        let project = tempdir().unwrap();
        fs::write(project.path().join("config.toml"), b"value = 1\n").unwrap();
        let installed = sha256_bytes(b"value = 1\n");
        let lock = InstallationLock {
            schema_version: "1.0.0".into(),
            project_id: "demo".into(),
            script_prefix: Some("demo".into()),
            primary_namespace: Some("demo".into()),
            installed_at: Utc::now().to_rfc3339(),
            updated_at: None,
            source: LockSourceIdentity {
                repository: "owner/repo".into(),
                mode: SourceMode::PinnedCommit,
                revision: "599497ea2f93612d9094461c6fde114fc87a5c0f".into(),
                requested_ref: None,
                release: None,
                manifest_sha256: "a".repeat(64),
                manifest_origin: "remote".into(),
            },
            ai_provider: "codex".into(),
            ai_model: "gpt-5.6-luna".into(),
            ai_reasoning_effort: "xhigh".into(),
            ai_endpoint: None,
            ai_optimization_profile: crate::models::default_ai_optimization_profile(),
            primary_coding_environment: "codex".into(),
            additional_coding_environments: vec![],
            flatten_chat_sources: false,
            codex_analysis: None,
            wiki_required_pages: manifest_wiki_pages(),
            wiki_metadata: None,
            components: vec![],
            files: vec![LockedFile {
                path: "config.toml".into(),
                location_scope: None,
                component_id: "codex.config".into(),
                source_path: "config.toml".into(),
                source_revision: "599497ea2f93612d9094461c6fde114fc87a5c0f".into(),
                source_sha256: installed.clone(),
                source_size: Some(10),
                base_sha256: None,
                installed_sha256: installed,
                installed_size: Some(10),
                ownership: Ownership::Merged,
                preserved_local: false,
                external: false,
                generated_content: None,
                generated_bytes: None,
                executable: false,
                platform: Some(ManifestPlatform::All),
            }],
            merge_choices: vec![],
            optional_workflows: Default::default(),
            portrait_pipeline: None,
            local_modifications: vec![],
            rollback_records: vec![],
        };
        let operations = managed_removal_operations(&lock, project.path()).unwrap();
        assert_eq!(operations[0].action, OperationAction::Skip);
        assert_eq!(
            operations[0].resolution.as_deref(),
            Some("reverse_merge_required")
        );
    }

    #[test]
    fn stage_and_operation_fault_matrix_never_writes_success_lock() {
        for stage in 0..TRANSACTION_STAGES.len() {
            for after in [false, true] {
                let project = tempdir().unwrap();
                let app = tempdir().unwrap();
                let plan = ready_plan(project.path());
                let prepared = vec![PreparedFile {
                    operation_id: "op-1".into(),
                    destination: "AGENTS.md".into(),
                    bytes: b"safe".to_vec(),
                    expected_sha256: sha256_bytes(b"safe"),
                }];
                let mut options = TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    ..Default::default()
                };
                if after {
                    options.fail_after_stage = Some(stage);
                } else {
                    options.fail_before_stage = Some(stage);
                }
                assert!(run_test_transaction(project.path(), &plan, &prepared, &options).is_err());
                assert!(!project
                    .path()
                    .join(".hoi4-mod-setup/install.lock.json")
                    .exists());
            }
        }
        for after in [false, true] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let plan = ready_plan(project.path());
            let prepared = vec![PreparedFile {
                operation_id: "op-1".into(),
                destination: "AGENTS.md".into(),
                bytes: b"safe".to_vec(),
                expected_sha256: sha256_bytes(b"safe"),
            }];
            let options = TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_before_operation: (!after).then_some(0),
                fail_after_operation: after.then_some(0),
                ..Default::default()
            };
            assert!(run_test_transaction(project.path(), &plan, &prepared, &options).is_err());
            assert!(!project
                .path()
                .join(".hoi4-mod-setup/install.lock.json")
                .exists());
        }

        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let transaction_id = plan.plan_id;
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let options = TransactionOptions {
            app_data_root: Some(app.path().into()),
            fail_after_live_mutation: Some(0),
            ..Default::default()
        };
        assert!(run_test_transaction(project.path(), &plan, &prepared, &options).is_err());
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        let journal_path = transaction_root(app.path(), transaction_id)
            .transaction
            .join("journal.json");
        let mut journal = read_journal(&journal_path).unwrap();
        assert_eq!(journal.operations[0].status, "applying");
        assert!(journal.recovery.rollback_allowed);
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert!(!project.path().join("AGENTS.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn direct_journal_reads_reject_a_linked_transaction_directory() {
        use std::os::unix::fs::symlink;

        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let journal = new_journal(&plan, &plan.project_id, project.path());
        fs::write(
            outside.path().join("journal.json"),
            serde_json::to_vec_pretty(&journal).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(app.path().join("transactions")).unwrap();
        let linked = app
            .path()
            .join("transactions")
            .join(plan.plan_id.to_string());
        symlink(outside.path(), &linked).unwrap();

        let error = read_journal(&linked.join("journal.json")).unwrap_err();
        assert!(matches!(error, AppError::PathSecurity(_)));
    }

    #[cfg(unix)]
    #[test]
    fn transaction_applies_and_rolls_back_executable_metadata() {
        use std::os::unix::fs::PermissionsExt;

        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (mut plan, prepared) = existing_file_fixture(project.path());
        let original = project.path().join("AGENTS.md");
        fs::set_permissions(&original, fs::Permissions::from_mode(0o644)).unwrap();
        plan.operations[0].executable = true;

        let completed = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(
            fs::metadata(&original).unwrap().permissions().mode() & 0o111,
            0
        );
        assert!(completed.1.files[0].executable);

        let journal_path = transaction_root(app.path(), plan.plan_id)
            .transaction
            .join("journal.json");
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();

        assert_eq!(fs::read(&original).unwrap(), b"old");
        assert_eq!(
            fs::metadata(&original).unwrap().permissions().mode() & 0o111,
            0
        );
    }

    fn quarantine_files(directory: &Path) -> Vec<PathBuf> {
        let mut files = fs::read_dir(directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(QUARANTINE_PREFIX))
            })
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    fn is_agents_destination(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("AGENTS.md")
    }

    fn edit_agents_after_precondition(path: &Path, _index: usize, barrier: LiveMutationBarrier) {
        if barrier == LiveMutationBarrier::AfterPrecondition && is_agents_destination(path) {
            fs::write(path, b"concurrent user edit").unwrap();
        }
    }

    fn create_agents_after_precondition(path: &Path, _index: usize, barrier: LiveMutationBarrier) {
        if barrier == LiveMutationBarrier::AfterPrecondition && is_agents_destination(path) {
            assert!(!path.exists(), "the reviewed destination must be absent");
            fs::write(path, b"user created").unwrap();
        }
    }

    fn create_agents_after_quarantine(path: &Path, _index: usize, barrier: LiveMutationBarrier) {
        if barrier == LiveMutationBarrier::AfterQuarantineVerified && is_agents_destination(path) {
            assert!(
                !path.exists(),
                "the verified destination must be quarantined"
            );
            fs::write(path, b"user created").unwrap();
        }
    }

    fn transaction_journal_path(app: &Path, transaction_id: Uuid) -> PathBuf {
        transaction_root(app, transaction_id)
            .transaction
            .join("journal.json")
    }

    #[test]
    fn concurrent_edit_after_precondition_is_preserved_as_a_conflict() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                live_mutation_barrier: Some(edit_agents_after_precondition),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("local precondition changed"));
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"concurrent user edit"
        );
        assert!(quarantine_files(project.path()).is_empty());
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());

        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        assert_eq!(
            journal.operations[0].quarantine_leaf.as_deref(),
            Some(quarantine_leaf_name(plan.plan_id, "op-1").as_str())
        );
        assert_eq!(
            journal.operations[0].quarantine_sha256.as_deref(),
            Some(sha256_bytes(b"concurrent user edit").as_str())
        );
        // Rollback treats the preserved local bytes as the restored state.
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"concurrent user edit"
        );
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn file_created_in_the_apply_window_is_never_clobbered() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = ready_plan(project.path());
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                live_mutation_barrier: Some(create_agents_after_precondition),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("a file appeared"));
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"user created"
        );
        assert!(quarantine_files(project.path()).is_empty());
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        // The unreviewed file is not removed by rollback either.
        let error = rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
        assert!(error.to_string().contains("uncertain live state"));
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"user created"
        );
    }

    #[test]
    fn file_created_after_quarantine_keeps_both_user_files() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                live_mutation_barrier: Some(create_agents_after_quarantine),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("a file appeared"));
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"user created"
        );
        let quarantined = quarantine_files(project.path());
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read(&quarantined[0]).unwrap(), b"old");

        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        assert_eq!(
            journal.operations[0].quarantine_sha256.as_deref(),
            Some(sha256_bytes(b"old").as_str())
        );
        let error = rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
        assert!(error.to_string().contains("manual review"));
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"user created"
        );
        assert_eq!(fs::read(&quarantined[0]).unwrap(), b"old");
    }

    #[test]
    fn crash_at_each_quarantine_boundary_rolls_back_to_the_original_bytes() {
        for boundary in [
            QuarantineBoundary::BeforeRename,
            QuarantineBoundary::AfterRename,
            QuarantineBoundary::AfterVerification,
            QuarantineBoundary::BeforeRelease,
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared) = existing_file_fixture(project.path());
            let error = run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    fail_at_quarantine: Some((0, boundary)),
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("fault injected"));
            let destination = project.path().join("AGENTS.md");
            let quarantined = quarantine_files(project.path());
            match boundary {
                QuarantineBoundary::BeforeRename => {
                    assert_eq!(fs::read(&destination).unwrap(), b"old");
                    assert!(quarantined.is_empty());
                }
                QuarantineBoundary::AfterRename | QuarantineBoundary::AfterVerification => {
                    assert!(!destination.exists(), "{boundary:?}");
                    assert_eq!(quarantined.len(), 1);
                    assert_eq!(fs::read(&quarantined[0]).unwrap(), b"old");
                }
                QuarantineBoundary::BeforeRelease => {
                    assert_eq!(fs::read(&destination).unwrap(), b"safe");
                    assert_eq!(quarantined.len(), 1);
                    assert_eq!(fs::read(&quarantined[0]).unwrap(), b"old");
                }
                QuarantineBoundary::BeforeMoveBack => unreachable!("not in this matrix"),
            }
            assert!(!project
                .path()
                .join(".hoi4-mod-setup/install.lock.json")
                .exists());

            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            assert!(journal.operations[0].quarantine_leaf.is_some());
            assert!(resume_transaction(project.path(), app.path(), plan.plan_id).is_err());
            rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
            assert_eq!(fs::read(&destination).unwrap(), b"old", "{boundary:?}");
            assert!(quarantine_files(project.path()).is_empty(), "{boundary:?}");
        }
    }

    #[test]
    fn managed_delete_preserves_a_concurrent_edit() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let initial_plan = ready_plan(project.path());
        let initial_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (_, installed_lock) = run_test_transaction(
            project.path(),
            &initial_plan,
            &initial_prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut removal_plan = ready_plan(project.path());
        removal_plan.maintenance_mode = Some("remove".into());
        removal_plan.plan_id = Uuid::new_v4();
        removal_plan.codex_analysis = None;
        removal_plan.generated_artifacts.clear();
        removal_plan.external_actions.clear();
        removal_plan.git_setup = None;
        removal_plan.optional_workflows.clear();
        removal_plan.operations =
            managed_removal_operations(&installed_lock, project.path()).unwrap();
        assert!(removal_plan.operations.iter().any(|operation| {
            operation.destination == "AGENTS.md"
                && operation.action == OperationAction::DeleteManaged
        }));

        let error = run_test_transaction(
            project.path(),
            &removal_plan,
            &[],
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                live_mutation_barrier: Some(edit_agents_after_precondition),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("local precondition changed"));
        assert_eq!(
            fs::read(project.path().join("AGENTS.md")).unwrap(),
            b"concurrent user edit"
        );
        assert!(quarantine_files(project.path()).is_empty());

        // A delete interrupted after its quarantine rename restores the
        // displaced bytes on rollback.
        let mut retry_plan = removal_plan.clone();
        retry_plan.plan_id = Uuid::new_v4();
        let journal_path = transaction_journal_path(app.path(), removal_plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        fs::write(project.path().join("AGENTS.md"), b"safe").unwrap();
        let error = run_test_transaction(
            project.path(),
            &retry_plan,
            &[],
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_at_quarantine: Some((
                    retry_plan
                        .operations
                        .iter()
                        .position(|operation| operation.destination == "AGENTS.md")
                        .unwrap(),
                    QuarantineBoundary::AfterVerification,
                )),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("fault injected"));
        assert!(!project.path().join("AGENTS.md").exists());
        let journal_path = transaction_journal_path(app.path(), retry_plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn rollback_quarantine_interruptions_are_settled_on_retry() {
        for checkpoint in [
            "rollback_quarantine_BeforeRename",
            "rollback_quarantine_AfterRename",
            "rollback_quarantine_AfterVerification",
            "rollback_after_placement",
            "rollback_quarantine_BeforeRelease",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared) = existing_file_fixture(project.path());
            run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    ..Default::default()
                },
            )
            .unwrap();
            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            TEST_FAULT.with(|fault| *fault.borrow_mut() = Some(checkpoint.into()));
            let result = rollback_transaction(project.path(), &mut journal, &journal_path);
            TEST_FAULT.with(|fault| *fault.borrow_mut() = None);
            assert!(result.is_err(), "{checkpoint}");

            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path)
                .unwrap_or_else(|error| panic!("{checkpoint}: {error}"));
            assert_eq!(
                fs::read(project.path().join("AGENTS.md")).unwrap(),
                b"old",
                "{checkpoint}"
            );
            assert!(quarantine_files(project.path()).is_empty(), "{checkpoint}");
            assert!(!project
                .path()
                .join(".hoi4-mod-setup/install.lock.json")
                .exists());
        }
    }

    fn maintenance_fixture(
        project_root: &Path,
        app_root: &Path,
    ) -> (InstallationPlan, Vec<PreparedFile>, Vec<u8>) {
        let mut first_plan = ready_plan(project_root);
        first_plan.operations[0].source_sha256 = Some(sha256_bytes(b"old"));
        first_plan.operations[0].source_size = Some(3);
        let first_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"old".to_vec(),
            expected_sha256: sha256_bytes(b"old"),
        }];
        run_test_transaction(
            project_root,
            &first_plan,
            &first_prepared,
            &TransactionOptions {
                app_data_root: Some(app_root.into()),
                ..Default::default()
            },
        )
        .unwrap();
        let predecessor = fs::read(project_root.join(".hoi4-mod-setup/install.lock.json")).unwrap();
        let mut second_plan = ready_plan(project_root);
        second_plan.operations[0].action = OperationAction::Replace;
        second_plan.operations[0].local_state = LocalState::Unmodified;
        second_plan.operations[0].local_sha256 = Some(sha256_bytes(b"old"));
        second_plan.operations[0].result_sha256 = Some(sha256_bytes(b"new"));
        second_plan.operations[0].source_size = Some(3);
        let second_prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"new".to_vec(),
            expected_sha256: sha256_bytes(b"new"),
        }];
        (second_plan, second_prepared, predecessor)
    }

    #[test]
    fn crash_before_lock_quarantine_release_is_finished_by_resume() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared, predecessor) = maintenance_fixture(project.path(), app.path());
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_at_lock_quarantine: Some(QuarantineBoundary::BeforeRelease),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("fault injected"));
        let lock_directory = project.path().join(".hoi4-mod-setup");
        let quarantined = quarantine_files(&lock_directory);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read(&quarantined[0]).unwrap(), predecessor);
        let journal = read_journal(&transaction_journal_path(app.path(), plan.plan_id)).unwrap();
        assert_eq!(journal.state, "finalizing");
        let committed = fs::read(lock_directory.join("install.lock.json")).unwrap();
        assert_eq!(
            Some(sha256_bytes(&committed)),
            journal.result_lock_sha256.clone()
        );

        let (completed, _) = resume_transaction(project.path(), app.path(), plan.plan_id).unwrap();
        assert_eq!(completed.state, "completed");
        assert!(quarantine_files(&lock_directory).is_empty());
        assert_eq!(
            fs::read(lock_directory.join("install.lock.json")).unwrap(),
            committed
        );
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"new");
    }

    #[test]
    fn crash_inside_lock_commit_restores_the_predecessor_for_rollback() {
        for boundary in [
            QuarantineBoundary::AfterRename,
            QuarantineBoundary::AfterVerification,
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared, predecessor) = maintenance_fixture(project.path(), app.path());
            run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    fail_at_lock_quarantine: Some(boundary),
                    ..Default::default()
                },
            )
            .unwrap_err();
            let lock_directory = project.path().join(".hoi4-mod-setup");
            assert!(!lock_directory.join("install.lock.json").exists());
            assert_eq!(quarantine_files(&lock_directory).len(), 1);

            // Resume moves the predecessor back and then requires rollback.
            assert!(resume_transaction(project.path(), app.path(), plan.plan_id).is_err());
            assert_eq!(
                fs::read(lock_directory.join("install.lock.json")).unwrap(),
                predecessor
            );
            assert!(quarantine_files(&lock_directory).is_empty());
            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
            assert_eq!(
                fs::read(lock_directory.join("install.lock.json")).unwrap(),
                predecessor
            );
            assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
        }
    }

    #[test]
    fn rollback_lock_restore_interruptions_are_settled_on_retry() {
        for checkpoint in [
            "rollback_lock_quarantine_AfterRename",
            "rollback_lock_quarantine_BeforeRelease",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared, predecessor) = maintenance_fixture(project.path(), app.path());
            run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    ..Default::default()
                },
            )
            .unwrap();
            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            TEST_FAULT.with(|fault| *fault.borrow_mut() = Some(checkpoint.into()));
            let result = rollback_transaction(project.path(), &mut journal, &journal_path);
            TEST_FAULT.with(|fault| *fault.borrow_mut() = None);
            assert!(result.is_err(), "{checkpoint}");
            let lock_directory = project.path().join(".hoi4-mod-setup");
            assert_eq!(quarantine_files(&lock_directory).len(), 1, "{checkpoint}");

            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path)
                .unwrap_or_else(|error| panic!("{checkpoint}: {error}"));
            assert_eq!(
                fs::read(lock_directory.join("install.lock.json")).unwrap(),
                predecessor,
                "{checkpoint}"
            );
            assert!(quarantine_files(&lock_directory).is_empty(), "{checkpoint}");
            assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
        }
    }

    #[test]
    fn journaled_quarantine_names_are_bound_to_their_operation() {
        let transaction_id = Uuid::new_v4();
        let mut operation = JournalOperation {
            id: "op-1".into(),
            status: "applying".into(),
            destination: "AGENTS.md".into(),
            ownership: Some(Ownership::Managed),
            component_id: None,
            source_path: None,
            source_size: None,
            action: Some(OperationAction::Replace),
            location_scope: None,
            external: false,
            backup_path: None,
            before_sha256: None,
            before_executable: None,
            expected_sha256: None,
            source_sha256: None,
            result_sha256: None,
            expected_executable: None,
            rollback: Some(RollbackAction::RestoreBackup),
            rollback_source_path: None,
            resolution: None,
            backup_sha256: None,
            staged_sha256: None,
            after_sha256: None,
            after_exists: None,
            after_executable: None,
            quarantine_leaf: Some(quarantine_leaf_name(transaction_id, "op-1")),
            quarantine_sha256: None,
            external_parent_identity: None,
        };
        assert!(journaled_quarantine_leaf(transaction_id, &operation)
            .unwrap()
            .is_some());
        operation.quarantine_leaf = Some("descriptor.mod".into());
        assert!(matches!(
            journaled_quarantine_leaf(transaction_id, &operation),
            Err(AppError::PathSecurity(_))
        ));
        operation.quarantine_leaf = Some(quarantine_leaf_name(Uuid::new_v4(), "op-1"));
        assert!(journaled_quarantine_leaf(transaction_id, &operation).is_err());
        assert!(quarantine_leaf_name(transaction_id, "../escape").starts_with(QUARANTINE_PREFIX));
        assert!(!quarantine_leaf_name(transaction_id, "../escape").contains('/'));
    }

    fn edit_agents_after_placement(path: &Path, _index: usize, barrier: LiveMutationBarrier) {
        if barrier == LiveMutationBarrier::AfterPlacement && is_agents_destination(path) {
            assert_eq!(fs::read(path).unwrap(), b"safe");
            fs::write(path, b"edit after placement").unwrap();
        }
    }

    fn with_test_fault<T>(checkpoint: &str, run: impl FnOnce() -> T) -> T {
        TEST_FAULT.with(|fault| *fault.borrow_mut() = Some(checkpoint.into()));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
        TEST_FAULT.with(|fault| *fault.borrow_mut() = None);
        result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    }

    #[test]
    fn edit_after_placement_is_kept_as_a_conflict_and_never_treated_as_installed() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                live_mutation_barrier: Some(edit_agents_after_placement),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("changed after the reviewed bytes were placed"),
            "{error}"
        );
        let destination = project.path().join("AGENTS.md");
        assert_eq!(fs::read(&destination).unwrap(), b"edit after placement");
        let quarantined = quarantine_files(project.path());
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read(&quarantined[0]).unwrap(), b"old");
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());

        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        assert_eq!(journal.operations[0].status, "applying");
        assert!(journal.operations[0].after_sha256.is_none());
        assert_eq!(
            journal.operations[0].quarantine_sha256.as_deref(),
            Some(sha256_bytes(b"old").as_str())
        );
        // Rollback must not treat the edit as installed bytes and delete it.
        let error = rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
        assert!(error.to_string().contains("manual review"), "{error}");
        assert_eq!(fs::read(&destination).unwrap(), b"edit after placement");
        assert_eq!(fs::read(&quarantined[0]).unwrap(), b"old");
    }

    #[test]
    fn forward_replace_interrupted_after_placement_rolls_back_through_its_quarantine() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_after_live_mutation: Some(0),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("fault injected after live mutation"));
        let destination = project.path().join("AGENTS.md");
        assert_eq!(fs::read(&destination).unwrap(), b"safe");
        let quarantined = quarantine_files(project.path());
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read(&quarantined[0]).unwrap(), b"old");

        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        assert!(journal.operations[0].after_sha256.is_none());
        assert!(resume_transaction(project.path(), app.path(), plan.plan_id).is_err());
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn crash_before_moving_changed_bytes_back_keeps_them_for_rollback() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                live_mutation_barrier: Some(edit_agents_after_precondition),
                fail_at_quarantine: Some((0, QuarantineBoundary::BeforeMoveBack)),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("fault injected"), "{error}");
        let destination = project.path().join("AGENTS.md");
        assert!(!destination.exists());
        let quarantined = quarantine_files(project.path());
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read(&quarantined[0]).unwrap(), b"concurrent user edit");

        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        assert_eq!(
            journal.operations[0].quarantine_sha256.as_deref(),
            Some(sha256_bytes(b"concurrent user edit").as_str())
        );
        // Rollback moves the changed bytes back instead of the backup copy.
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"concurrent user edit");
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn rollback_restoring_a_forward_quarantine_settles_each_boundary_on_retry() {
        for checkpoint in [
            "rollback_quarantine_BeforeRename",
            "rollback_quarantine_AfterRename",
            "rollback_quarantine_AfterVerification",
            "rollback_after_placement",
            "rollback_quarantine_BeforeRelease",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared) = existing_file_fixture(project.path());
            run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    fail_at_quarantine: Some((0, QuarantineBoundary::BeforeRelease)),
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert_eq!(quarantine_files(project.path()).len(), 1, "{checkpoint}");
            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            let result = with_test_fault(checkpoint, || {
                rollback_transaction(project.path(), &mut journal, &journal_path)
            });
            assert!(result.is_err(), "{checkpoint}");

            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path)
                .unwrap_or_else(|error| panic!("{checkpoint}: {error}"));
            assert_eq!(
                fs::read(project.path().join("AGENTS.md")).unwrap(),
                b"old",
                "{checkpoint}"
            );
            assert!(quarantine_files(project.path()).is_empty(), "{checkpoint}");
        }
    }

    #[test]
    fn rollback_removing_a_created_file_settles_each_boundary_on_retry() {
        for checkpoint in [
            "rollback_quarantine_BeforeRename",
            "rollback_quarantine_AfterRename",
            "rollback_quarantine_AfterVerification",
            "rollback_after_placement",
            "rollback_quarantine_BeforeRelease",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let plan = ready_plan(project.path());
            let prepared = vec![PreparedFile {
                operation_id: "op-1".into(),
                destination: "AGENTS.md".into(),
                bytes: b"safe".to_vec(),
                expected_sha256: sha256_bytes(b"safe"),
            }];
            run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    ..Default::default()
                },
            )
            .unwrap();
            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            let result = with_test_fault(checkpoint, || {
                rollback_transaction(project.path(), &mut journal, &journal_path)
            });
            assert!(result.is_err(), "{checkpoint}");

            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path)
                .unwrap_or_else(|error| panic!("{checkpoint}: {error}"));
            assert!(!project.path().join("AGENTS.md").exists(), "{checkpoint}");
            assert!(quarantine_files(project.path()).is_empty(), "{checkpoint}");
        }
    }

    #[test]
    fn rollback_probes_the_derived_quarantine_name_when_the_journal_lacks_it() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_at_quarantine: Some((0, QuarantineBoundary::AfterRename)),
                ..Default::default()
            },
        )
        .unwrap_err();
        let destination = project.path().join("AGENTS.md");
        assert!(!destination.exists());
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        journal.operations[0].quarantine_leaf = None;
        journal.operations[0].quarantine_sha256 = None;
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn rollback_sweep_restores_the_quarantine_of_an_operation_left_pending() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_at_quarantine: Some((0, QuarantineBoundary::AfterRename)),
                ..Default::default()
            },
        )
        .unwrap_err();
        let destination = project.path().join("AGENTS.md");
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        // Model an apply intent and quarantine intent that recovery never
        // saw: the operation reads as untouched and is not actionable.
        journal.operations[0].status = "pending".into();
        journal.operations[0].quarantine_leaf = None;
        journal.operations[0].quarantine_sha256 = None;
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn rollback_keeps_quarantines_it_cannot_vouch_for() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let destination = project.path().join("AGENTS.md");
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);

        // Unknown bytes under the operation's own name beside installed
        // bytes are never moved into place.
        let owned = project
            .path()
            .join(quarantine_leaf_name(plan.plan_id, "op-1"));
        fs::write(&owned, b"mystery").unwrap();
        let mut journal = read_journal(&journal_path).unwrap();
        let error = rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
        assert!(
            error.to_string().contains("match no recorded state"),
            "{error}"
        );
        assert_eq!(fs::read(&destination).unwrap(), b"safe");
        assert_eq!(fs::read(&owned).unwrap(), b"mystery");
        fs::remove_file(&owned).unwrap();

        // A leftover name of this transaction that no operation owns is kept
        // and stops rollback after the operations are restored.
        let stray = project
            .path()
            .join(quarantine_leaf_name(plan.plan_id, "op-unknown"));
        fs::write(&stray, b"stray").unwrap();
        let mut journal = read_journal(&journal_path).unwrap();
        let error = rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
        assert!(
            error.to_string().contains("match no recorded state"),
            "{error}"
        );
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert_eq!(fs::read(&stray).unwrap(), b"stray");

        fs::remove_file(&stray).unwrap();
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn forward_quarantine_beside_restored_bytes_is_settled_by_hash() {
        // Equal bytes: the quarantine is a redundant copy and is released.
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_at_quarantine: Some((0, QuarantineBoundary::AfterRename)),
                ..Default::default()
            },
        )
        .unwrap_err();
        let destination = project.path().join("AGENTS.md");
        fs::write(&destination, b"old").unwrap();
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert!(quarantine_files(project.path()).is_empty());

        // Different bytes: a sync client re-created the reviewed bytes while
        // the user's edit is quarantined. Both are kept for review.
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                live_mutation_barrier: Some(edit_agents_after_precondition),
                fail_at_quarantine: Some((0, QuarantineBoundary::BeforeMoveBack)),
                ..Default::default()
            },
        )
        .unwrap_err();
        let destination = project.path().join("AGENTS.md");
        fs::write(&destination, b"old").unwrap();
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        let error = rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
        assert!(error.to_string().contains("manual review"), "{error}");
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        let quarantined = quarantine_files(project.path());
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read(&quarantined[0]).unwrap(), b"concurrent user edit");
    }

    #[test]
    fn errors_after_the_quarantine_rename_move_the_bytes_back() {
        for checkpoint in [
            "apply_quarantine_hash_error",
            "apply_quarantine_journal_error",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared) = existing_file_fixture(project.path());
            let error = with_test_fault(checkpoint, || {
                run_test_transaction(
                    project.path(),
                    &plan,
                    &prepared,
                    &TransactionOptions {
                        app_data_root: Some(app.path().into()),
                        ..Default::default()
                    },
                )
                .unwrap_err()
            });
            assert!(error.to_string().contains(checkpoint), "{error}");
            assert!(error.to_string().contains("moved back"), "{error}");
            let destination = project.path().join("AGENTS.md");
            assert_eq!(fs::read(&destination).unwrap(), b"old", "{checkpoint}");
            assert!(quarantine_files(project.path()).is_empty(), "{checkpoint}");

            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path)
                .unwrap_or_else(|error| panic!("{checkpoint}: {error}"));
            assert_eq!(fs::read(&destination).unwrap(), b"old", "{checkpoint}");
            assert!(quarantine_files(project.path()).is_empty(), "{checkpoint}");
        }
    }

    #[test]
    fn checkpoint_replay_orders_records_by_sequence_not_wall_clock() {
        let root = tempdir().unwrap();
        let plan = plan();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let store = TransactionStore::open_journal_directory(&journal_path).unwrap();
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&store, &mut journal).unwrap();

        // The clock stepped backwards after this snapshot was written.
        journal.updated_at = "2999-01-01T00:00:00+00:00".into();
        atomic_write_json(&journal_path, &journal).unwrap();
        journal.operations[0].status = "applying".into();
        journal.operations[0].quarantine_leaf = Some(quarantine_leaf_name(plan.plan_id, "op-1"));
        journal.last_checkpoint = "apply-quarantine-intent-op-1".into();
        persist_operation_checkpoint(&store, &mut journal, 0).unwrap();
        let replayed = read_journal(&journal_path).unwrap();
        assert_eq!(replayed.operations[0].status, "applying");
        assert!(replayed.operations[0].quarantine_leaf.is_some());
        assert_eq!(replayed.last_checkpoint, "apply-quarantine-intent-op-1");

        // A snapshot covers earlier records even when its timestamp is older.
        journal.operations[0].status = "verified".into();
        journal.last_checkpoint = "snapshot".into();
        journal.updated_at = "2000-01-01T00:00:00+00:00".into();
        atomic_write_json(&journal_path, &journal).unwrap();
        let replayed = read_journal(&journal_path).unwrap();
        assert_eq!(replayed.operations[0].status, "verified");
        assert_eq!(replayed.last_checkpoint, "snapshot");

        // Journals and records written before sequences still replay by time.
        clear_operation_checkpoints(&store).unwrap();
        let mut legacy = journal.clone();
        legacy.checkpoint_sequence = None;
        legacy.operations[0].status = "pending".into();
        atomic_write_json(&journal_path, &legacy).unwrap();
        let mut operation = legacy.operations[0].clone();
        operation.status = "staged".into();
        let record = OperationCheckpoint {
            schema_version: LEGACY_OPERATION_CHECKPOINT_SCHEMA.into(),
            transaction_id: legacy.transaction_id,
            operation_index: 0,
            operation,
            journal_state: legacy.state.clone(),
            last_checkpoint: "legacy-record".into(),
            recovery: legacy.recovery.clone(),
            updated_at: "2001-01-01T00:00:00+00:00".into(),
            sequence: None,
        };
        let mut line = serde_json::to_vec(&record).unwrap();
        line.push(b'\n');
        fs::write(operation_checkpoint_root(&journal_path).unwrap(), line).unwrap();
        let replayed = read_journal(&journal_path).unwrap();
        assert_eq!(replayed.operations[0].status, "staged");
        assert_eq!(replayed.last_checkpoint, "legacy-record");
    }

    #[cfg(windows)]
    #[test]
    fn quarantine_release_falls_back_to_the_classic_delete_disposition() {
        struct ClassicDelete;
        impl Drop for ClassicDelete {
            fn drop(&mut self) {
                crate::safe_fs::force_classic_delete_for_test(false);
            }
        }
        crate::safe_fs::force_classic_delete_for_test(true);
        let _guard = ClassicDelete;
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let destination = project.path().join("AGENTS.md");
        assert_eq!(fs::read(&destination).unwrap(), b"safe");
        assert!(quarantine_files(project.path()).is_empty());
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert!(quarantine_files(project.path()).is_empty());
    }

    #[test]
    fn lock_commit_interrupted_before_its_rename_rolls_back_to_the_predecessor() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared, predecessor) = maintenance_fixture(project.path(), app.path());
        let error = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().into()),
                fail_at_lock_quarantine: Some(QuarantineBoundary::BeforeRename),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("fault injected"), "{error}");
        let lock_directory = project.path().join(".hoi4-mod-setup");
        assert_eq!(
            fs::read(lock_directory.join("install.lock.json")).unwrap(),
            predecessor
        );
        assert!(quarantine_files(&lock_directory).is_empty());
        // The predecessor is not the committed lock, so resume refuses.
        assert!(resume_transaction(project.path(), app.path(), plan.plan_id).is_err());
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(
            fs::read(lock_directory.join("install.lock.json")).unwrap(),
            predecessor
        );
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
        assert!(quarantine_files(&lock_directory).is_empty());
    }

    #[test]
    fn rollback_lock_restore_remaining_boundaries_are_settled_on_retry() {
        for checkpoint in [
            "rollback_lock_quarantine_BeforeRename",
            "rollback_lock_quarantine_AfterVerification",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared, predecessor) = maintenance_fixture(project.path(), app.path());
            run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    ..Default::default()
                },
            )
            .unwrap();
            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            let result = with_test_fault(checkpoint, || {
                rollback_transaction(project.path(), &mut journal, &journal_path)
            });
            assert!(result.is_err(), "{checkpoint}");

            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path)
                .unwrap_or_else(|error| panic!("{checkpoint}: {error}"));
            let lock_directory = project.path().join(".hoi4-mod-setup");
            assert_eq!(
                fs::read(lock_directory.join("install.lock.json")).unwrap(),
                predecessor,
                "{checkpoint}"
            );
            assert!(quarantine_files(&lock_directory).is_empty(), "{checkpoint}");
            assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
        }
    }

    #[test]
    fn rollback_lock_removal_after_a_first_install_is_settled_on_retry() {
        for checkpoint in [
            "rollback_lock_quarantine_BeforeRename",
            "rollback_lock_quarantine_AfterRename",
            "rollback_lock_quarantine_AfterVerification",
            "rollback_lock_quarantine_BeforeRelease",
        ] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let plan = ready_plan(project.path());
            let prepared = vec![PreparedFile {
                operation_id: "op-1".into(),
                destination: "AGENTS.md".into(),
                bytes: b"safe".to_vec(),
                expected_sha256: sha256_bytes(b"safe"),
            }];
            run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().into()),
                    ..Default::default()
                },
            )
            .unwrap();
            let lock_directory = project.path().join(".hoi4-mod-setup");
            assert!(lock_directory.join("install.lock.json").exists());
            let journal_path = transaction_journal_path(app.path(), plan.plan_id);
            let mut journal = read_journal(&journal_path).unwrap();
            let result = with_test_fault(checkpoint, || {
                rollback_transaction(project.path(), &mut journal, &journal_path)
            });
            assert!(result.is_err(), "{checkpoint}");

            let mut journal = read_journal(&journal_path).unwrap();
            rollback_transaction(project.path(), &mut journal, &journal_path)
                .unwrap_or_else(|error| panic!("{checkpoint}: {error}"));
            assert!(
                !lock_directory.join("install.lock.json").exists(),
                "{checkpoint}"
            );
            if lock_directory.exists() {
                assert!(quarantine_files(&lock_directory).is_empty(), "{checkpoint}");
            }
            assert!(!project.path().join("AGENTS.md").exists(), "{checkpoint}");
        }
    }

    /// A project plus an existing external launcher descriptor in a separate
    /// launcher directory that the transaction replaces.
    struct ExternalLauncherCase {
        project: tempfile::TempDir,
        _app: tempfile::TempDir,
        launchers: tempfile::TempDir,
        app_root: PathBuf,
        launcher_parent: PathBuf,
        launcher_path: PathBuf,
        plan: InstallationPlan,
        prepared: Vec<PreparedFile>,
        old_launcher: Vec<u8>,
        new_launcher: Vec<u8>,
    }

    impl ExternalLauncherCase {
        fn new() -> Self {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let launchers = tempdir().unwrap();
            let app_root = fs::canonicalize(app.path()).unwrap();
            let launcher_parent = fs::canonicalize(launchers.path()).unwrap().join("mod");
            fs::create_dir(&launcher_parent).unwrap();
            let launcher_path = launcher_parent.join("example.mod");
            let old_launcher = b"name=\"Example\"\npath=\"elsewhere\"\n".to_vec();
            fs::write(&launcher_path, &old_launcher).unwrap();
            let thumbnail = crate::descriptors::placeholder_thumbnail_png().unwrap();
            fs::write(project.path().join("thumbnail.png"), &thumbnail).unwrap();
            let mut plan = ready_plan(project.path());
            plan.operations.push(PlanOperation {
                id: "thumbnail".into(),
                component_id: "project.thumbnail".into(),
                ownership: Some(Ownership::Generated),
                location_scope: Some("project".into()),
                action: OperationAction::Skip,
                source_path: Some("generated:thumbnail.png".into()),
                destination: "thumbnail.png".into(),
                source_sha256: Some("c".repeat(64)),
                source_size: Some(1),
                platform: None,
                executable: false,
                result_sha256: None,
                base_sha256: None,
                local_sha256: Some(sha256_bytes(&thumbnail)),
                local_state: LocalState::Modified,
                resolution: Some("keep".into()),
                external: false,
                rollback: RollbackAction::None,
                external_parent_identity: None,
            });
            let canonical_project = validate_project_root(project.path()).unwrap();
            let identity = ProjectIdentity {
                display_name: "Example".into(),
                project_id: plan.project_id.clone(),
                author: String::new(),
                version: "0.1.0".into(),
                supported_game_version: "1.17.*".into(),
                project_root: canonical_project.clone(),
                default_branch: "main".into(),
                script_prefix: plan.script_prefix.clone(),
                primary_namespace: plan.primary_namespace.clone(),
                descriptor_tags: Vec::new(),
                launcher_descriptor_path: Some(launcher_path.clone()),
            };
            let new_launcher =
                crate::descriptors::render_launcher_descriptor(&identity, &canonical_project)
                    .unwrap()
                    .into_bytes();
            plan.operations.push(PlanOperation {
                id: "launcher".into(),
                component_id: "project.launcher_descriptor".into(),
                ownership: Some(Ownership::Generated),
                location_scope: Some("external_launcher".into()),
                action: OperationAction::Replace,
                source_path: Some("generated:example.mod".into()),
                destination: launcher_path.display().to_string(),
                source_sha256: Some(sha256_bytes(&new_launcher)),
                source_size: Some(new_launcher.len() as u64),
                platform: None,
                executable: false,
                result_sha256: Some(sha256_bytes(&new_launcher)),
                base_sha256: None,
                local_sha256: Some(sha256_bytes(&old_launcher)),
                local_state: LocalState::Unmodified,
                resolution: None,
                external: true,
                rollback: RollbackAction::RestoreBackup,
                external_parent_identity: None,
            });
            let prepared = vec![
                PreparedFile {
                    operation_id: "op-1".into(),
                    destination: "AGENTS.md".into(),
                    bytes: b"safe".to_vec(),
                    expected_sha256: sha256_bytes(b"safe"),
                },
                PreparedFile {
                    operation_id: "launcher".into(),
                    destination: launcher_path.display().to_string(),
                    bytes: new_launcher.clone(),
                    expected_sha256: sha256_bytes(&new_launcher),
                },
            ];
            Self {
                project,
                _app: app,
                launchers,
                app_root,
                launcher_parent,
                launcher_path,
                plan,
                prepared,
                old_launcher,
                new_launcher,
            }
        }

        fn options(&self) -> TransactionOptions {
            TransactionOptions {
                app_data_root: Some(self.app_root.clone()),
                ..Default::default()
            }
        }

        fn journal_path(&self) -> PathBuf {
            transaction_journal_path(&self.app_root, self.plan.plan_id)
        }

        fn away(&self) -> PathBuf {
            self.launcher_parent.with_file_name("mod-away")
        }

        fn bound_identity(journal: &TransactionJournal) -> Option<String> {
            journal
                .operations
                .iter()
                .find(|operation| operation.id == "launcher")
                .and_then(|operation| operation.external_parent_identity.clone())
        }
    }

    fn link_directory(link: &Path, target: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        {
            let output = Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .expect("cmd.exe is present on Windows");
            assert!(
                output.status.success(),
                "junction creation failed: {output:?}"
            );
        }
    }

    fn remove_directory_link(link: &Path) {
        #[cfg(unix)]
        fs::remove_file(link).unwrap();
        #[cfg(windows)]
        fs::remove_dir(link).unwrap();
    }

    #[test]
    fn external_parent_replaced_after_backup_blocks_resume_until_the_bound_directory_returns() {
        let case = ExternalLauncherCase::new();
        // Stop after staged-output validation: the backup stage has bound the
        // launcher parent and the transaction is still resumable.
        let validation_stage = TRANSACTION_STAGES
            .iter()
            .position(|stage| *stage == "validation")
            .unwrap();
        let interrupted = run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &TransactionOptions {
                fail_after_stage: Some(validation_stage),
                ..case.options()
            },
        )
        .unwrap_err();
        assert!(
            interrupted
                .to_string()
                .contains("fault injected after stage validation"),
            "{interrupted}"
        );
        let bound =
            ExternalLauncherCase::bound_identity(&read_journal(&case.journal_path()).unwrap())
                .expect("the backup stage binds the launcher parent");
        assert_eq!(
            bound,
            RootedDir::open_read(&case.launcher_parent)
                .unwrap()
                .identity_token()
                .unwrap()
        );

        // Swap the reviewed parent away for a different directory that holds
        // the same reviewed bytes, so every content precondition still passes.
        fs::rename(&case.launcher_parent, case.away()).unwrap();
        fs::create_dir(&case.launcher_parent).unwrap();
        fs::write(&case.launcher_path, &case.old_launcher).unwrap();

        let refused =
            resume_transaction(case.project.path(), &case.app_root, case.plan.plan_id).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("no longer the directory bound to this transaction"),
            "{refused}"
        );
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.old_launcher);
        assert_eq!(
            fs::read(case.away().join("example.mod")).unwrap(),
            case.old_launcher
        );
        assert!(!case.project.path().join("AGENTS.md").exists());
        assert!(!case
            .project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());
        assert_eq!(
            ExternalLauncherCase::bound_identity(&read_journal(&case.journal_path()).unwrap()),
            Some(bound.clone()),
            "a refused replay keeps the original binding"
        );

        // Swap the bound directory back; the replay now proceeds there.
        fs::remove_file(&case.launcher_path).unwrap();
        fs::remove_dir(&case.launcher_parent).unwrap();
        fs::rename(case.away(), &case.launcher_parent).unwrap();
        let (journal, _) =
            resume_transaction(case.project.path(), &case.app_root, case.plan.plan_id).unwrap();
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.new_launcher);
        assert_eq!(ExternalLauncherCase::bound_identity(&journal), Some(bound));
    }

    /// Install the launcher, then swap its parent away for an impostor that
    /// holds the installed bytes, either as a plain directory or as a
    /// directory link. Post-install checks, final verification, and rollback
    /// must refuse it and leave both directories unchanged; once the bound
    /// directory returns, rollback restores the original launcher there.
    fn external_parent_swapped_after_apply_is_refused(use_link: bool) {
        let case = ExternalLauncherCase::new();
        let (journal, _) = run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &case.options(),
        )
        .unwrap();
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.new_launcher);
        assert!(ExternalLauncherCase::bound_identity(&journal).is_some());

        let impostor = if use_link {
            fs::canonicalize(case.launchers.path())
                .unwrap()
                .join("impostor")
        } else {
            case.launcher_parent.clone()
        };
        fs::rename(&case.launcher_parent, case.away()).unwrap();
        fs::create_dir(&impostor).unwrap();
        fs::write(impostor.join("example.mod"), &case.new_launcher).unwrap();
        if use_link {
            link_directory(&case.launcher_parent, &impostor);
        }

        let canonical_project = validate_project_root(case.project.path()).unwrap();
        let project_directory =
            open_bound_project_root(&canonical_project, &journal.project_root_lifecycle).unwrap();
        let mut checked = journal.clone();
        let post_install = post_install_checks(
            &canonical_project,
            &project_directory,
            &case.plan,
            &mut checked,
            &TransactionStore::open_journal_directory(&case.journal_path()).unwrap(),
        )
        .unwrap_err();
        let final_verification =
            final_live_verification(&project_directory, &case.plan, &journal).unwrap_err();
        drop(project_directory);
        if !use_link {
            for error in [&post_install, &final_verification] {
                assert!(
                    error
                        .to_string()
                        .contains("no longer the directory bound to this transaction"),
                    "{error}"
                );
            }
        }

        let mut journal = read_journal(&case.journal_path()).unwrap();
        let refused = rollback_transaction(case.project.path(), &mut journal, &case.journal_path())
            .unwrap_err();
        if !use_link {
            assert!(
                refused
                    .to_string()
                    .contains("no longer the directory bound to this transaction"),
                "{refused}"
            );
        }
        assert_eq!(
            fs::read(impostor.join("example.mod")).unwrap(),
            case.new_launcher
        );
        assert_eq!(
            fs::read(case.away().join("example.mod")).unwrap(),
            case.new_launcher
        );
        assert_eq!(
            fs::read(case.project.path().join("AGENTS.md")).unwrap(),
            b"safe"
        );

        if use_link {
            remove_directory_link(&case.launcher_parent);
        }
        fs::remove_file(impostor.join("example.mod")).unwrap();
        fs::remove_dir(&impostor).unwrap();
        fs::rename(case.away(), &case.launcher_parent).unwrap();
        let mut journal = read_journal(&case.journal_path()).unwrap();
        rollback_transaction(case.project.path(), &mut journal, &case.journal_path()).unwrap();
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.old_launcher);
        assert!(!case.project.path().join("AGENTS.md").exists());
    }

    #[test]
    fn external_parent_replaced_by_a_directory_after_apply_is_refused() {
        external_parent_swapped_after_apply_is_refused(false);
    }

    #[test]
    fn external_parent_replaced_by_a_link_after_apply_is_refused() {
        external_parent_swapped_after_apply_is_refused(true);
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct ApplySwapState {
        attempted: bool,
        swapped: bool,
        impostor_received_launcher: bool,
    }

    static APPLY_PARENT_SWAP: std::sync::Mutex<ApplySwapState> =
        std::sync::Mutex::new(ApplySwapState {
            attempted: false,
            swapped: false,
            impostor_received_launcher: false,
        });

    /// Swap the launcher parent away after its precondition passed and back
    /// after placement. The swap can only succeed where the platform lets a
    /// directory with an open handle be renamed.
    fn swap_launcher_parent_around_apply(path: &Path, _index: usize, point: LiveMutationBarrier) {
        if path.file_name().and_then(|name| name.to_str()) != Some("example.mod") {
            return;
        }
        let parent = path.parent().unwrap().to_path_buf();
        let away = parent.with_file_name("mod-away");
        let mut state = APPLY_PARENT_SWAP.lock().unwrap();
        match point {
            LiveMutationBarrier::AfterPrecondition => {
                state.attempted = true;
                if fs::rename(&parent, &away).is_ok() {
                    fs::create_dir(&parent).unwrap();
                    state.swapped = true;
                }
            }
            LiveMutationBarrier::AfterPlacement if state.swapped => {
                state.impostor_received_launcher |= path.exists();
                fs::remove_dir_all(&parent).unwrap();
                fs::rename(&away, &parent).unwrap();
            }
            _ => {}
        }
    }

    #[test]
    fn external_parent_swap_during_apply_cannot_redirect_the_launcher() {
        let case = ExternalLauncherCase::new();
        let result = run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &TransactionOptions {
                live_mutation_barrier: Some(swap_launcher_parent_around_apply),
                ..case.options()
            },
        );
        let mut state = *APPLY_PARENT_SWAP.lock().unwrap();
        assert!(state.attempted);
        // An apply that stopped before placement leaves the swap in place.
        if case.away().exists() {
            state.impostor_received_launcher |= case.launcher_path.exists();
            fs::remove_dir_all(&case.launcher_parent).unwrap();
            fs::rename(case.away(), &case.launcher_parent).unwrap();
        }
        assert!(
            !state.impostor_received_launcher,
            "the launcher was written into the swapped-in directory"
        );
        #[cfg(windows)]
        {
            assert!(
                !state.swapped,
                "the retained launcher parent handle must refuse the rename"
            );
            assert!(result.is_ok(), "{:?}", result.as_ref().err());
        }
        match result {
            Ok(_) => assert_eq!(fs::read(&case.launcher_path).unwrap(), case.new_launcher),
            Err(_) => {
                let mut journal = read_journal(&case.journal_path()).unwrap();
                rollback_transaction(case.project.path(), &mut journal, &case.journal_path())
                    .unwrap();
                assert_eq!(fs::read(&case.launcher_path).unwrap(), case.old_launcher);
            }
        }
    }

    #[test]
    fn project_root_swap_during_post_install_checks_cannot_redirect_the_reads() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        let (journal, _) = run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        let canonical_project = validate_project_root(project.path()).unwrap();
        let project_directory =
            open_bound_project_root(&canonical_project, &journal.project_root_lifecycle).unwrap();
        let away = canonical_project.with_file_name(format!(
            "{}-away",
            canonical_project.file_name().unwrap().to_string_lossy()
        ));
        let swapped = fs::rename(&canonical_project, &away).is_ok();
        #[cfg(windows)]
        assert!(
            !swapped,
            "the retained project capability must refuse a rename of the project root"
        );
        if swapped {
            // An impostor root with the installed bytes at the same path.
            fs::create_dir(&canonical_project).unwrap();
            fs::write(canonical_project.join("AGENTS.md"), b"safe").unwrap();
        }
        let mut checked = journal.clone();
        let result = post_install_checks(
            &canonical_project,
            &project_directory,
            &plan,
            &mut checked,
            &TransactionStore::open_journal_directory(&transaction_journal_path(
                app.path(),
                plan.plan_id,
            ))
            .unwrap(),
        );
        drop(project_directory);
        if swapped {
            assert!(
                result.is_err(),
                "post-install checks read the swapped-in project root"
            );
            fs::remove_dir_all(&canonical_project).unwrap();
            fs::rename(&away, &canonical_project).unwrap();
        } else {
            result.unwrap();
        }
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    impl ExternalLauncherCase {
        /// The same case for a launcher that does not exist yet, which the
        /// transaction creates and rollback removes.
        fn new_created() -> Self {
            let mut case = Self::new();
            fs::remove_file(&case.launcher_path).unwrap();
            let launcher = case
                .plan
                .operations
                .iter_mut()
                .find(|operation| operation.id == "launcher")
                .unwrap();
            launcher.action = OperationAction::Generate;
            launcher.local_sha256 = None;
            launcher.local_state = LocalState::Absent;
            launcher.rollback = RollbackAction::RemoveCreated;
            case
        }

        /// Review the plan the way plan construction does.
        fn bind_plan(&mut self) -> String {
            bind_plan_external_parents(&mut self.plan).unwrap();
            self.plan
                .operations
                .iter()
                .find(|operation| operation.id == "launcher")
                .and_then(|operation| operation.external_parent_identity.clone())
                .expect("the reviewed launcher parent is bound in the plan")
        }

        /// Replace the launcher parent with a different directory holding
        /// `bytes` (or nothing) at the launcher path.
        fn swap_parent(&self, bytes: Option<&[u8]>) {
            fs::rename(&self.launcher_parent, self.away()).unwrap();
            fs::create_dir(&self.launcher_parent).unwrap();
            if let Some(bytes) = bytes {
                fs::write(&self.launcher_path, bytes).unwrap();
            }
        }

        fn restore_parent(&self) {
            fs::remove_dir_all(&self.launcher_parent).unwrap();
            fs::rename(self.away(), &self.launcher_parent).unwrap();
        }
    }

    #[test]
    fn plan_binding_records_the_reviewed_launcher_parent_through_one_handle() {
        let mut case = ExternalLauncherCase::new();
        let bound = case.bind_plan();
        assert_eq!(
            bound,
            RootedDir::open_read(&case.launcher_parent)
                .unwrap()
                .identity_token()
                .unwrap()
        );
        assert!(case
            .plan
            .operations
            .iter()
            .filter(|operation| !operation.external)
            .all(|operation| operation.external_parent_identity.is_none()));
        // The journal starts from the reviewed identity, before any stage.
        let journal = new_journal(&case.plan, &case.plan.project_id, case.project.path());
        assert_eq!(
            ExternalLauncherCase::bound_identity(&journal),
            Some(bound.clone())
        );
        // A plan round-trips the binding, and one without it stays readable.
        let serialized = serde_json::to_value(&case.plan).unwrap();
        let launcher = serialized["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|operation| operation["id"] == "launcher")
            .unwrap();
        assert_eq!(launcher["external_parent_identity"], bound.as_str());
        let mut legacy = serialized.clone();
        for operation in legacy["operations"].as_array_mut().unwrap() {
            operation
                .as_object_mut()
                .unwrap()
                .remove("external_parent_identity");
        }
        let legacy: InstallationPlan = serde_json::from_value(legacy).unwrap();
        assert!(legacy
            .operations
            .iter()
            .all(|operation| operation.external_parent_identity.is_none()));
        assert!(
            ExternalLauncherCase::bound_identity(&new_journal(
                &legacy,
                &legacy.project_id,
                case.project.path()
            ))
            .is_none(),
            "a plan without the binding keeps the backup-stage binding"
        );

        // Bytes that differ from the reviewed hash mean the plan no longer
        // describes this directory.
        fs::write(&case.launcher_path, b"changed after review").unwrap();
        let changed = bind_plan_external_parents(&mut case.plan).unwrap_err();
        assert!(
            changed
                .to_string()
                .contains("changed while the plan was built"),
            "{changed}"
        );

        // A parent that does not exist at review stays unbound.
        let mut absent = ExternalLauncherCase::new_created();
        let missing_parent = absent.launcher_parent.join("missing").join("example.mod");
        for operation in absent.plan.operations.iter_mut() {
            if operation.id == "launcher" {
                operation.destination = missing_parent.display().to_string();
            }
        }
        bind_plan_external_parents(&mut absent.plan).unwrap();
        assert!(absent
            .plan
            .operations
            .iter()
            .all(|operation| operation.external_parent_identity.is_none()));

        // Only an external destination may carry a parent identity.
        let mut project_bound = ready_plan(case.project.path());
        project_bound.operations[0].external_parent_identity = Some(bound);
        assert!(validate_plan(&project_bound)
            .unwrap_err()
            .to_string()
            .contains("only an external destination"));
    }

    #[test]
    fn external_parent_swapped_after_plan_review_is_refused_before_transaction_storage() {
        let mut case = ExternalLauncherCase::new();
        let bound = case.bind_plan();
        // A different directory with the reviewed bytes at the same path:
        // every content precondition still passes.
        case.swap_parent(Some(&case.old_launcher));

        let refused = run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &case.options(),
        )
        .unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("no longer the directory bound to this transaction"),
            "{refused}"
        );
        assert!(
            !case.journal_path().exists(),
            "a plan refused at preflight writes no transaction storage"
        );
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.old_launcher);
        assert_eq!(
            fs::read(case.away().join("example.mod")).unwrap(),
            case.old_launcher
        );
        assert!(!case.project.path().join("AGENTS.md").exists());

        // The reviewed directory returns; the same plan now applies there.
        case.restore_parent();
        let (journal, _) = run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &case.options(),
        )
        .unwrap();
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.new_launcher);
        assert_eq!(ExternalLauncherCase::bound_identity(&journal), Some(bound));
    }

    #[test]
    fn the_journal_starts_from_the_plan_bound_parent_before_backup() {
        let mut case = ExternalLauncherCase::new();
        let bound = case.bind_plan();
        // Stop at dry-run review, before the backup stage opens the parent.
        let review_stage = TRANSACTION_STAGES
            .iter()
            .position(|stage| *stage == "dry-run review")
            .unwrap();
        run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &TransactionOptions {
                fail_after_stage: Some(review_stage),
                ..case.options()
            },
        )
        .unwrap_err();
        let interrupted = read_journal(&case.journal_path()).unwrap();
        assert_eq!(
            interrupted.stages[5].status, "pending",
            "backup has not run"
        );
        assert_eq!(
            ExternalLauncherCase::bound_identity(&interrupted),
            Some(bound),
            "the journal carries the reviewed identity before the backup stage"
        );
    }

    #[test]
    fn plan_bound_parent_missing_at_apply_time_is_refused() {
        let mut case = ExternalLauncherCase::new_created();
        case.bind_plan();
        fs::rename(&case.launcher_parent, case.away()).unwrap();
        let refused = run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &case.options(),
        )
        .unwrap_err();
        assert!(refused.to_string().contains("is missing"), "{refused}");
        assert!(
            !case.launcher_parent.exists(),
            "a bound parent is never recreated"
        );
        assert!(!case.journal_path().exists());
    }

    /// Install the launcher, move its bound parent away, and roll back.
    /// Rollback must stop instead of reading the destination as absent and
    /// reporting it restored, keep the journal recoverable, and finish once
    /// the folder is back.
    fn rollback_with_a_missing_bound_parent_stops(created: bool) {
        let mut case = if created {
            ExternalLauncherCase::new_created()
        } else {
            ExternalLauncherCase::new()
        };
        case.bind_plan();
        run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &case.options(),
        )
        .unwrap();
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.new_launcher);
        fs::rename(&case.launcher_parent, case.away()).unwrap();

        let mut journal = read_journal(&case.journal_path()).unwrap();
        let refused = rollback_transaction(case.project.path(), &mut journal, &case.journal_path())
            .unwrap_err();
        assert!(matches!(refused, AppError::PathSecurity(_)), "{refused:?}");
        assert!(
            refused.to_string().contains("is missing; move it back"),
            "{refused}"
        );
        let stopped = read_journal(&case.journal_path()).unwrap();
        assert_ne!(stopped.state, "rolled_back");
        assert!(stopped.recovery.rollback_allowed);
        // The inverse-backup capture stops first, before any rollback intent
        // or project change.
        assert!(
            stopped
                .operations
                .iter()
                .all(|operation| operation.status == "verified"),
            "{:?}",
            stopped
                .operations
                .iter()
                .map(|operation| (&operation.id, &operation.status))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            fs::read(case.project.path().join("AGENTS.md")).unwrap(),
            b"safe"
        );
        assert!(
            !case.launcher_parent.exists(),
            "a bound parent is never recreated"
        );
        assert_eq!(
            fs::read(case.away().join("example.mod")).unwrap(),
            case.new_launcher
        );

        fs::rename(case.away(), &case.launcher_parent).unwrap();
        let mut journal = read_journal(&case.journal_path()).unwrap();
        rollback_transaction(case.project.path(), &mut journal, &case.journal_path()).unwrap();
        if created {
            assert!(!case.launcher_path.exists());
        } else {
            assert_eq!(fs::read(&case.launcher_path).unwrap(), case.old_launcher);
        }
        assert!(!case.project.path().join("AGENTS.md").exists());
    }

    #[test]
    fn rollback_of_a_created_launcher_stops_while_its_bound_parent_is_missing() {
        rollback_with_a_missing_bound_parent_stops(true);
    }

    #[test]
    fn rollback_of_a_replaced_launcher_stops_while_its_bound_parent_is_missing() {
        rollback_with_a_missing_bound_parent_stops(false);
    }

    #[test]
    fn rollback_of_a_kept_launcher_needs_nothing_from_a_missing_bound_parent() {
        let mut case = ExternalLauncherCase::new();
        let launcher = case
            .plan
            .operations
            .iter_mut()
            .find(|operation| operation.id == "launcher")
            .unwrap();
        // The kept launcher already registers this project.
        launcher.action = OperationAction::Skip;
        launcher.result_sha256 = None;
        launcher.local_sha256 = Some(sha256_bytes(&case.new_launcher));
        launcher.local_state = LocalState::Unmodified;
        launcher.rollback = RollbackAction::None;
        fs::write(&case.launcher_path, &case.new_launcher).unwrap();
        case.old_launcher = case.new_launcher.clone();
        case.bind_plan();
        run_test_transaction(
            case.project.path(),
            &case.plan,
            &case.prepared,
            &case.options(),
        )
        .unwrap();
        assert_eq!(fs::read(&case.launcher_path).unwrap(), case.old_launcher);
        fs::rename(&case.launcher_parent, case.away()).unwrap();
        // A kept destination is never changed, so nothing in the moved
        // folder needs restoring and rollback completes without it.
        let mut journal = read_journal(&case.journal_path()).unwrap();
        rollback_transaction(case.project.path(), &mut journal, &case.journal_path()).unwrap();
        assert!(
            !case.launcher_parent.exists(),
            "a bound parent is never recreated"
        );
        assert_eq!(
            fs::read(case.away().join("example.mod")).unwrap(),
            case.old_launcher
        );
        assert!(!case.project.path().join("AGENTS.md").exists());
    }

    #[test]
    fn a_missing_bound_parent_is_absent_only_for_operations_that_changed_nothing() {
        let case = ExternalLauncherCase::new();
        let identity = RootedDir::open_read(&case.launcher_parent)
            .unwrap()
            .identity_token()
            .unwrap();
        fs::rename(&case.launcher_parent, case.away()).unwrap();
        let mut operation = new_journal(&case.plan, &case.plan.project_id, case.project.path())
            .operations
            .into_iter()
            .find(|operation| operation.id == "launcher")
            .unwrap();
        operation.external_parent_identity = Some(identity);
        for (status, action, missing_is_error) in [
            ("pending", Some(OperationAction::Replace), false),
            ("staged", Some(OperationAction::Replace), false),
            ("verified", Some(OperationAction::Skip), false),
            ("verified", None, false),
            ("applying", Some(OperationAction::Replace), true),
            ("verified", Some(OperationAction::Generate), true),
            ("rollback_applying", Some(OperationAction::Replace), true),
            ("rolled_back", Some(OperationAction::Replace), true),
        ] {
            operation.status = status.into();
            operation.action = action;
            match existing_operation_target(None, &operation) {
                Ok(target) => {
                    assert!(!missing_is_error, "{status} {action:?} read as absent");
                    assert!(target.is_none());
                }
                Err(error) => {
                    assert!(missing_is_error, "{status} {action:?}: {error}");
                    assert!(error.to_string().contains("is missing; move it back"));
                }
            }
        }
        // The quarantine sweep applies the same rule to the folder it lists.
        let mut journal = new_journal(&case.plan, &case.plan.project_id, case.project.path());
        journal.operations.retain(|record| record.id == "launcher");
        journal.operations[0] = operation.clone();
        journal.operations[0].status = "staged".into();
        journal.operations[0].action = Some(OperationAction::Replace);
        sweep_transaction_quarantines(None, &journal, None, QuarantineSweep::Rollback).unwrap();
        journal.operations[0].status = "rolled_back".into();
        let swept = sweep_transaction_quarantines(None, &journal, None, QuarantineSweep::Rollback)
            .unwrap_err();
        assert!(
            swept.to_string().contains("is missing; move it back"),
            "{swept}"
        );

        // An unbound legacy record keeps the absent reading.
        operation.external_parent_identity = None;
        operation.status = "verified".into();
        operation.action = Some(OperationAction::Replace);
        assert!(existing_operation_target(None, &operation)
            .unwrap()
            .is_none());
    }

    /// Copy a tree of regular files, standing in for a directory swapped in
    /// at the same path with the same bytes.
    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    /// Every regular file below `root` with its bytes, for proving that a
    /// refused call wrote nothing into a directory.
    fn tree_snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut files = std::collections::BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                } else {
                    files.insert(
                        entry.path().strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(entry.path()).unwrap(),
                    );
                }
            }
        }
        files
    }

    /// How an application-data directory is replaced at its path.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AppDataSwap {
        /// A plain directory holding a copy of the same files.
        Directory,
        /// A directory link (a junction on Windows) to such a copy.
        Link,
    }

    /// An application-data directory moved away from its path, with the
    /// impostor that now occupies the path.
    struct SwappedAppData {
        path: PathBuf,
        away: PathBuf,
        link_target: Option<PathBuf>,
    }

    impl SwappedAppData {
        /// Move `<app>/<area>/<id>` out of the transaction areas and put an
        /// impostor holding the same files at its path. Returns `None` when
        /// the platform refuses to rename a directory with an open handle.
        fn try_swap(
            app: &Path,
            area: &str,
            transaction_id: Uuid,
            swap: AppDataSwap,
        ) -> Option<Self> {
            let path = app.join(area).join(transaction_id.to_string());
            let away = app.join("moved-away").join(area);
            fs::create_dir_all(away.parent().unwrap()).unwrap();
            if fs::rename(&path, &away).is_err() {
                return None;
            }
            let link_target = match swap {
                AppDataSwap::Directory => {
                    copy_tree(&away, &path);
                    None
                }
                AppDataSwap::Link => {
                    let target = app.join("impostors").join(area);
                    copy_tree(&away, &target);
                    link_directory(&path, &target);
                    Some(target)
                }
            };
            Some(Self {
                path,
                away,
                link_target,
            })
        }

        fn swap(app: &Path, area: &str, transaction_id: Uuid, swap: AppDataSwap) -> Self {
            Self::try_swap(app, area, transaction_id, swap).expect("the directory can be renamed")
        }

        /// The directory whose files the impostor exposes at the path.
        fn impostor(&self) -> &Path {
            self.link_target.as_deref().unwrap_or(&self.path)
        }

        /// Remove the impostor and move the bound directory back.
        fn restore(self) {
            match &self.link_target {
                Some(target) => {
                    remove_directory_link(&self.path);
                    fs::remove_dir_all(target).unwrap();
                }
                None => fs::remove_dir_all(&self.path).unwrap(),
            }
            fs::rename(&self.away, &self.path).unwrap();
        }
    }

    fn assert_app_data_refusal(error: &AppError, swap: AppDataSwap) {
        assert!(matches!(error, AppError::PathSecurity(_)), "{error}");
        if swap == AppDataSwap::Directory {
            // A plain directory with the same bytes passes every content and
            // link check, so only the identity comparison can refuse it.
            assert!(
                error
                    .to_string()
                    .contains("application data directory is no longer the directory bound to this transaction"),
                "{error}"
            );
        }
    }

    /// Run a transaction that stops after staged-output validation, so it is
    /// resumable and has created all of its application-data directories.
    fn interrupted_before_apply(project: &Path, app: &Path) -> InstallationPlan {
        let (plan, prepared) = existing_file_fixture(project);
        let validation_stage = TRANSACTION_STAGES
            .iter()
            .position(|stage| *stage == "validation")
            .unwrap();
        let interrupted = run_test_transaction(
            project,
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.to_path_buf()),
                fail_after_stage: Some(validation_stage),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            interrupted
                .to_string()
                .contains("fault injected after stage validation"),
            "{interrupted}"
        );
        plan
    }

    #[test]
    fn journal_binds_the_application_data_directories_it_creates() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = interrupted_before_apply(project.path(), app.path());
        let journal = read_journal(&transaction_journal_path(app.path(), plan.plan_id)).unwrap();
        let identity = journal
            .app_data_identity
            .expect("application data identity");
        let observed = |relative: &Path| {
            RootedDir::open_read(&app.path().join(relative))
                .unwrap()
                .identity_token()
                .unwrap()
        };
        let id = plan.plan_id.to_string();
        assert_eq!(
            identity.root,
            RootedDir::open_read(app.path())
                .unwrap()
                .identity_token()
                .unwrap()
        );
        assert_eq!(
            identity.transaction,
            observed(&Path::new("transactions").join(&id))
        );
        assert_eq!(
            identity.backup,
            Some(observed(&Path::new("backups").join(&id)))
        );
        assert_eq!(
            identity.staging,
            Some(observed(&Path::new("staging").join(&id)))
        );
    }

    /// Swap one application-data directory of a resumable transaction for an
    /// impostor with the same files. Resume must refuse it without writing
    /// anything, and resume normally once the bound directory returns.
    fn app_data_swap_before_resume_is_refused(area: &str, swap: AppDataSwap) {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let plan = interrupted_before_apply(project.path(), app.path());

        let swapped = SwappedAppData::swap(app.path(), area, plan.plan_id, swap);
        let impostor_before = tree_snapshot(swapped.impostor());
        let bound_before = tree_snapshot(&swapped.away);
        let refused = resume_transaction(project.path(), app.path(), plan.plan_id).unwrap_err();
        assert_app_data_refusal(&refused, swap);
        if area == TRANSACTIONS_AREA {
            // The impostor journal also blocks a new transaction for the
            // project instead of being ignored.
            let blocked = find_incomplete_transaction(app.path(), project.path()).unwrap_err();
            assert_app_data_refusal(&blocked, swap);
        }
        assert_eq!(tree_snapshot(swapped.impostor()), impostor_before);
        assert_eq!(tree_snapshot(&swapped.away), bound_before);
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
        assert!(!project
            .path()
            .join(".hoi4-mod-setup/install.lock.json")
            .exists());

        swapped.restore();
        let (journal, _) = resume_transaction(project.path(), app.path(), plan.plan_id).unwrap();
        assert_eq!(journal.state, "completed");
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    #[test]
    fn transaction_directory_replaced_before_resume_is_refused() {
        app_data_swap_before_resume_is_refused(TRANSACTIONS_AREA, AppDataSwap::Directory);
    }

    #[test]
    fn transaction_directory_replaced_by_a_link_before_resume_is_refused() {
        app_data_swap_before_resume_is_refused(TRANSACTIONS_AREA, AppDataSwap::Link);
    }

    #[test]
    fn staging_directory_replaced_before_resume_is_refused() {
        app_data_swap_before_resume_is_refused(STAGING_AREA, AppDataSwap::Directory);
    }

    #[test]
    fn staging_directory_replaced_by_a_link_before_resume_is_refused() {
        app_data_swap_before_resume_is_refused(STAGING_AREA, AppDataSwap::Link);
    }

    #[test]
    fn application_data_root_replaced_before_recovery_is_refused() {
        let project = tempdir().unwrap();
        let parent = tempdir().unwrap();
        let app = parent.path().join("app-data");
        fs::create_dir(&app).unwrap();
        let plan = interrupted_before_apply(project.path(), &app);

        // The whole application-data root moves away and a copy takes its
        // place, so every per-transaction folder is a copy as well.
        let away = parent.path().join("app-data-away");
        fs::rename(&app, &away).unwrap();
        copy_tree(&away, &app);
        let impostor_before = tree_snapshot(&app);
        let refused = resume_transaction(project.path(), &app, plan.plan_id).unwrap_err();
        assert_app_data_refusal(&refused, AppDataSwap::Directory);
        let refused = discard_staging(project.path(), &app, plan.plan_id).unwrap_err();
        assert_app_data_refusal(&refused, AppDataSwap::Directory);
        let blocked = find_incomplete_transaction(&app, project.path()).unwrap_err();
        assert_app_data_refusal(&blocked, AppDataSwap::Directory);
        assert_eq!(tree_snapshot(&app), impostor_before);
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");

        fs::remove_dir_all(&app).unwrap();
        fs::rename(&away, &app).unwrap();
        let (journal, _) = resume_transaction(project.path(), &app, plan.plan_id).unwrap();
        assert_eq!(journal.state, "completed");
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
    }

    #[test]
    fn staging_discard_never_removes_a_swapped_in_staging_directory() {
        for swap in [AppDataSwap::Directory, AppDataSwap::Link] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let plan = interrupted_before_apply(project.path(), app.path());
            let swapped = SwappedAppData::swap(app.path(), STAGING_AREA, plan.plan_id, swap);
            let impostor_before = tree_snapshot(swapped.impostor());

            let refused = discard_staging(project.path(), app.path(), plan.plan_id).unwrap_err();
            assert_app_data_refusal(&refused, swap);
            assert_eq!(
                tree_snapshot(swapped.impostor()),
                impostor_before,
                "{swap:?}"
            );
            assert_ne!(
                read_journal(&transaction_journal_path(app.path(), plan.plan_id))
                    .unwrap()
                    .state,
                "staging_discarded"
            );

            swapped.restore();
            let journal = discard_staging(project.path(), app.path(), plan.plan_id).unwrap();
            assert_eq!(journal.state, "staging_discarded");
            assert!(!app
                .path()
                .join(STAGING_AREA)
                .join(plan.plan_id.to_string())
                .exists());
        }
    }

    /// Install, then swap one application-data directory for an impostor
    /// with the same files. Rollback must refuse before it changes the
    /// journal or the project, and roll back normally once the bound
    /// directory returns.
    fn app_data_swap_before_rollback_is_refused(area: &str, swap: AppDataSwap) {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        // The caller reads the journal before the swap, so only the
        // rollback's own storage binding can notice it.
        let mut journal = read_journal(&journal_path).unwrap();

        let swapped = SwappedAppData::swap(app.path(), area, plan.plan_id, swap);
        let impostor_before = tree_snapshot(swapped.impostor());
        let bound_before = tree_snapshot(&swapped.away);
        if area == TRANSACTIONS_AREA {
            assert_app_data_refusal(&read_journal(&journal_path).unwrap_err(), swap);
        }
        let refused =
            rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
        assert_app_data_refusal(&refused, swap);
        assert_eq!(tree_snapshot(swapped.impostor()), impostor_before);
        assert_eq!(tree_snapshot(&swapped.away), bound_before);
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
        assert!(!app
            .path()
            .join(TRANSACTIONS_AREA)
            .read_dir()
            .unwrap()
            .any(|entry| {
                entry.unwrap().file_name().to_string_lossy() != plan.plan_id.to_string()
            }));

        swapped.restore();
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(journal.state, "rolled_back");
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
    }

    #[test]
    fn transaction_directory_replaced_before_rollback_is_refused() {
        app_data_swap_before_rollback_is_refused(TRANSACTIONS_AREA, AppDataSwap::Directory);
    }

    #[test]
    fn transaction_directory_replaced_by_a_link_before_rollback_is_refused() {
        app_data_swap_before_rollback_is_refused(TRANSACTIONS_AREA, AppDataSwap::Link);
    }

    #[test]
    fn backup_directory_replaced_before_rollback_is_refused() {
        app_data_swap_before_rollback_is_refused(BACKUPS_AREA, AppDataSwap::Directory);
    }

    #[test]
    fn backup_directory_replaced_by_a_link_before_rollback_is_refused() {
        app_data_swap_before_rollback_is_refused(BACKUPS_AREA, AppDataSwap::Link);
    }

    #[test]
    fn child_rollback_storage_replaced_before_a_retry_is_refused() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, prepared) = existing_file_fixture(project.path());
        run_test_transaction(
            project.path(),
            &plan,
            &prepared,
            &TransactionOptions {
                app_data_root: Some(app.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        let journal_path = transaction_journal_path(app.path(), plan.plan_id);
        // Stop the first rollback after its child journal and inverse backup
        // exist, so the retry reopens that child storage.
        let mut journal = read_journal(&journal_path).unwrap();
        let stopped = with_test_fault("rollback_after_placement", || {
            rollback_transaction(project.path(), &mut journal, &journal_path)
        });
        assert!(stopped.is_err());
        let child_id = read_journal(&journal_path)
            .unwrap()
            .rollback_transaction_id
            .expect("the first rollback recorded its child");
        let child = read_journal(&transaction_journal_path(app.path(), child_id)).unwrap();
        let child_identity = child.app_data_identity.expect("child storage identity");
        assert!(child_identity.backup.is_some());
        assert!(child_identity.staging.is_none());

        for area in [TRANSACTIONS_AREA, BACKUPS_AREA] {
            let swapped = SwappedAppData::swap(app.path(), area, child_id, AppDataSwap::Directory);
            let impostor_before = tree_snapshot(swapped.impostor());
            let mut journal = read_journal(&journal_path).unwrap();
            let refused =
                rollback_transaction(project.path(), &mut journal, &journal_path).unwrap_err();
            assert_app_data_refusal(&refused, AppDataSwap::Directory);
            assert_eq!(tree_snapshot(swapped.impostor()), impostor_before, "{area}");
            swapped.restore();
        }
        let mut journal = read_journal(&journal_path).unwrap();
        rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
        assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
    }

    struct ApplyAppDataSwap {
        app: PathBuf,
        transaction_id: Uuid,
        swap: AppDataSwap,
        attempted: bool,
        swapped: Option<SwappedAppData>,
        impostor_before: Option<std::collections::BTreeMap<PathBuf, Vec<u8>>>,
    }

    static APPLY_APP_DATA_SWAP: std::sync::Mutex<Option<ApplyAppDataSwap>> =
        std::sync::Mutex::new(None);

    /// Swap the transaction directory once the first live precondition has
    /// passed. The swap succeeds only where the platform lets a directory
    /// with an open handle be renamed.
    fn swap_transaction_directory_during_apply(
        _path: &Path,
        _index: usize,
        point: LiveMutationBarrier,
    ) {
        if point != LiveMutationBarrier::AfterPrecondition {
            return;
        }
        let mut guard = APPLY_APP_DATA_SWAP.lock().unwrap();
        let Some(state) = guard.as_mut() else {
            return;
        };
        if state.attempted {
            return;
        }
        state.attempted = true;
        state.swapped = SwappedAppData::try_swap(
            &state.app,
            TRANSACTIONS_AREA,
            state.transaction_id,
            state.swap,
        );
        state.impostor_before = state
            .swapped
            .as_ref()
            .map(|swapped| tree_snapshot(swapped.impostor()));
    }

    #[test]
    fn transaction_directory_swap_during_apply_cannot_redirect_the_journal() {
        for swap in [AppDataSwap::Directory, AppDataSwap::Link] {
            let project = tempdir().unwrap();
            let app = tempdir().unwrap();
            let (plan, prepared) = existing_file_fixture(project.path());
            *APPLY_APP_DATA_SWAP.lock().unwrap() = Some(ApplyAppDataSwap {
                app: app.path().to_path_buf(),
                transaction_id: plan.plan_id,
                swap,
                attempted: false,
                swapped: None,
                impostor_before: None,
            });
            let result = run_test_transaction(
                project.path(),
                &plan,
                &prepared,
                &TransactionOptions {
                    app_data_root: Some(app.path().to_path_buf()),
                    live_mutation_barrier: Some(swap_transaction_directory_during_apply),
                    ..Default::default()
                },
            );
            let state = APPLY_APP_DATA_SWAP.lock().unwrap().take().unwrap();
            assert!(state.attempted, "{swap:?}");
            #[cfg(windows)]
            {
                assert!(
                    state.swapped.is_none(),
                    "the retained transaction directory handle must refuse the rename"
                );
                assert!(result.is_ok(), "{:?}", result.as_ref().err());
            }
            // Unix lets a directory with an open descriptor be renamed, so the
            // refusal path below is the one exercised there.
            #[cfg(unix)]
            assert!(state.swapped.is_some(), "{swap:?}");
            match state.swapped {
                Some(swapped) => {
                    let error = result.unwrap_err();
                    assert!(
                        matches!(error, AppError::PathSecurity(_)),
                        "{swap:?}: {error}"
                    );
                    assert_eq!(
                        Some(tree_snapshot(swapped.impostor())),
                        state.impostor_before,
                        "the swapped-in directory received journal writes"
                    );
                    assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
                    swapped.restore();
                    let journal_path = transaction_journal_path(app.path(), plan.plan_id);
                    let mut journal = read_journal(&journal_path).unwrap();
                    rollback_transaction(project.path(), &mut journal, &journal_path).unwrap();
                    assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"old");
                }
                None => {
                    result.unwrap();
                    assert_eq!(fs::read(project.path().join("AGENTS.md")).unwrap(), b"safe");
                }
            }
        }
    }
}
