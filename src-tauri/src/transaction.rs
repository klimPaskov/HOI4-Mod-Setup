use crate::models::*;
use crate::paths::{
    application_data_root, transaction_root, validate_project_root,
    validate_project_root_or_destination,
};
use crate::security::{
    atomic_write, atomic_write_json, canonical_relative_key, is_link_metadata,
    normalize_relative_path, path_has_link_component, safe_join, sha256_bytes, sha256_file,
    validate_external_destination,
};
use crate::AppError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const OPERATION_INTENT_BATCH: usize = 64;
const OPERATION_CHECKPOINT_BATCH: usize = 1_024;
const OPERATION_CHECKPOINT_MAX_RECORDS: usize = 2_048;
const OPERATION_CHECKPOINT_MAX_BYTES: usize = 32 * 1024 * 1024;

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
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
    pub fail_before_git: bool,
    pub fail_after_git: bool,
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
    if plan.ai_optimization_profile != ai_profile.optimization_profile {
        return Err(AppError::Credential(
            "plan AI optimization profile does not match the selected provider".into(),
        ));
    }
    if plan.ai_provider != "codex"
        && plan
            .selected_components
            .iter()
            .any(|id| id == "codex.config")
    {
        return Err(AppError::Transaction(
            "non-Codex plans cannot install Codex configuration".into(),
        ));
    }
    if plan.flatten_chat_sources && plan.ai_provider != "codex" {
        return Err(AppError::Transaction(
            "flattened ChatGPT sources require the Codex provider".into(),
        ));
    }
    crate::ai::validate_endpoint_for_provider(
        plan.ai_provider.as_str(),
        plan.ai_endpoint.as_deref(),
    )?;
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
    TransactionJournal {
        schema_version: "1.0.0".into(),
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
            checkpoint: if plan.transaction.project_root_mode == ProjectRootMode::CreateLeaf {
                "pending".into()
            } else {
                "not_required".into()
            },
            created_by_transaction: false,
            observed_exists: plan.transaction.project_root_mode == ProjectRootMode::Existing,
            cleanup_result: None,
        },
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
    if transaction_state_is_terminal(&journal.state) || journal.state == "finalizing" {
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
    let entries = match fs::read_dir(&transactions_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if is_link_metadata(&metadata) {
            return Err(AppError::PathSecurity(
                "transaction storage contains a linked transaction directory".into(),
            ));
        }
        if !metadata.is_dir() {
            continue;
        }
        let journal_path = path.join("journal.json");
        let journal_metadata = match fs::symlink_metadata(&journal_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if is_link_metadata(&journal_metadata) || !journal_metadata.is_file() {
            return Err(AppError::PathSecurity(
                "transaction journal is not a regular file".into(),
            ));
        }
        match read_journal(&journal_path) {
            Ok(mut journal) => {
                if !transaction_state_is_terminal(&journal.state)
                    && roots_match_for_transaction(&journal.project_root, project_root)
                {
                    normalize_incomplete_recovery(&mut journal);
                    candidates.push(journal);
                }
            }
            Err(error) => {
                // A corrupt journal cannot be safely recovered, but if its
                // bounded root field identifies this project it must still
                // block a second transaction instead of being ignored.
                let bytes = fs::read(&journal_path)?;
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

fn expected_operation_backup(
    journal: &TransactionJournal,
    journal_path: &Path,
    operation_id: &str,
) -> Result<PathBuf, AppError> {
    Ok(journal_app_root(journal_path)?
        .join("backups")
        .join(journal.transaction_id.to_string())
        .join(format!("{operation_id}.bak")))
}

fn read_existing_lock(project_root: &Path) -> Result<Option<InstallationLock>, AppError> {
    let lock_path = safe_join(project_root, ".hoi4-mod-setup/install.lock.json")?;
    let bytes = match fs::read(&lock_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Transaction(error.to_string())),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::Transaction(format!("invalid existing installation lock: {error}"))
    })?;
    crate::migrations::migrate_lock(value).map(Some)
}

fn capture_previous_lock(
    project_root: &Path,
    backup_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    let lock_path = safe_join(project_root, ".hoi4-mod-setup/install.lock.json")?;
    let metadata = match fs::symlink_metadata(&lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if is_link_metadata(&metadata) || !metadata.is_file() {
        return Err(AppError::PathSecurity(
            "installation lock is not a regular file".into(),
        ));
    }
    let bytes = fs::read(&lock_path)?;
    let backup = backup_root.join("install.lock.json.bak");
    fs::write(&backup, &bytes)?;
    let digest = sha256_bytes(&bytes);
    if sha256_file(&backup)? != digest {
        return Err(AppError::Transaction(
            "installation lock backup verification failed".into(),
        ));
    }
    journal.previous_lock_backup_path = Some(backup.display().to_string());
    journal.previous_lock_sha256 = Some(digest);
    persist_journal(journal_path, journal)
}

pub fn run_transaction(
    project_root: &Path,
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    options: &TransactionOptions,
) -> Result<(TransactionJournal, InstallationLock), AppError> {
    validate_plan(plan)?;
    let (project_root, root_exists) = validate_project_root_or_destination(project_root)?;
    validate_plan_project_root(plan, &project_root, root_exists)?;
    validate_flatten_transaction_inputs(plan, prepared_files, &project_root)?;
    let app_root = match options.app_data_root.clone() {
        Some(root) => root,
        None => application_data_root()?,
    };
    if crate::security::path_has_link_component(&app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    if let Some(journal) = find_incomplete_transaction(&app_root, &project_root)? {
        if options.resume_transaction_id != Some(journal.transaction_id) {
            return Err(AppError::Transaction(format!(
                "an incomplete transaction must be recovered before starting another: {}",
                journal.transaction_id
            )));
        }
    }
    let previous_lock = read_existing_lock(&project_root)?;
    let roots = transaction_root(&app_root, plan.plan_id);
    fs::create_dir_all(&roots.transaction)?;
    if path_has_link_component(&roots.transaction) {
        return Err(AppError::PathSecurity(format!(
            "transaction storage contains a symlink or junction: {}",
            roots.transaction.display()
        )));
    }
    let journal_path = roots.transaction.join("journal.json");
    let plan_path = roots.transaction.join("plan.json");
    atomic_write_json(&plan_path, plan)?;
    let mut journal = new_journal(plan, &plan.project_id, &project_root);
    persist_journal(&journal_path, &mut journal)?;

    let result: Result<InstallationLock, AppError> = (|| {
        stage_start(
            &mut journal,
            0,
            "preflight",
            &journal_path,
            options.fail_before_stage,
        )?;
        stage_complete(
            &mut journal,
            0,
            "preflight",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            1,
            "repository source resolution",
            &journal_path,
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
            &journal_path,
        )?;
        stage_complete(
            &mut journal,
            1,
            "repository source resolution",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            2,
            "selective download",
            &journal_path,
            options.fail_before_stage,
        )?;
        let selected_evidence = validate_prepared_files(plan, prepared_files)?;
        add_stage_evidence(&mut journal, 2, selected_evidence, &journal_path)?;
        stage_complete(
            &mut journal,
            2,
            "selective download",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            3,
            "checksum verification",
            &journal_path,
            options.fail_before_stage,
        )?;
        let verified_evidence = validate_prepared_files(plan, prepared_files)?;
        add_stage_evidence(&mut journal, 3, verified_evidence, &journal_path)?;
        stage_complete(
            &mut journal,
            3,
            "checksum verification",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            4,
            "dry-run review",
            &journal_path,
            options.fail_before_stage,
        )?;
        stage_complete(
            &mut journal,
            4,
            "dry-run review",
            &journal_path,
            options.fail_after_stage,
        )?;

        stage_start(
            &mut journal,
            5,
            "backup",
            &journal_path,
            options.fail_before_stage,
        )?;
        fs::create_dir_all(&roots.backup)?;
        if path_has_link_component(&roots.backup) {
            return Err(AppError::PathSecurity(format!(
                "backup root contains a symlink or junction: {}",
                roots.backup.display()
            )));
        }
        capture_previous_lock(&project_root, &roots.backup, &mut journal, &journal_path)?;
        backup_existing(
            &project_root,
            plan,
            &roots.backup,
            &mut journal,
            &journal_path,
        )?;
        compact_operation_checkpoints(&journal_path, &mut journal)?;
        stage_complete(
            &mut journal,
            5,
            "backup",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            6,
            "staging",
            &journal_path,
            options.fail_before_stage,
        )?;
        fs::create_dir_all(&roots.staging)?;
        if path_has_link_component(&roots.staging) {
            return Err(AppError::PathSecurity(format!(
                "staging root contains a symlink or junction: {}",
                roots.staging.display()
            )));
        }
        stage_files(
            plan,
            prepared_files,
            &roots.staging,
            &mut journal,
            &journal_path,
        )?;
        stage_profile_directories(plan, &roots.staging)?;
        compact_operation_checkpoints(&journal_path, &mut journal)?;
        stage_complete(
            &mut journal,
            6,
            "staging",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            7,
            "validation",
            &journal_path,
            options.fail_before_stage,
        )?;
        validate_staging(&project_root, plan, prepared_files, &roots.staging)?;
        stage_complete(
            &mut journal,
            7,
            "validation",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            8,
            "apply",
            &journal_path,
            options.fail_before_stage,
        )?;
        ensure_project_root_for_apply(&project_root, plan, &mut journal, &journal_path)?;
        apply_profile_directories(&project_root, plan, &mut journal, &journal_path)?;
        apply_operations(
            &project_root,
            plan,
            &roots.staging,
            &mut journal,
            &journal_path,
            options,
        )?;
        compact_operation_checkpoints(&journal_path, &mut journal)?;
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
            persist_journal(&journal_path, &mut journal)?;
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
                .map(|operation| operation.destination.clone())
                .collect::<Vec<_>>();
            let git_result =
                crate::git::apply_git_setup(project_root.as_path(), setup, &managed_paths)?;
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
            persist_journal(&journal_path, &mut journal)?;
        }
        stage_complete(
            &mut journal,
            8,
            "apply",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            9,
            "post-install checks",
            &journal_path,
            options.fail_before_stage,
        )?;
        post_install_checks(&project_root, plan, &mut journal, &journal_path)?;
        stage_complete(
            &mut journal,
            9,
            "post-install checks",
            &journal_path,
            options.fail_after_stage,
        )?;
        stage_start(
            &mut journal,
            10,
            "readiness report",
            &journal_path,
            options.fail_before_stage,
        )?;
        let readiness = build_transaction_readiness(&project_root, plan, &journal)?;
        let readiness_path = roots.transaction.join("readiness-report.json");
        atomic_write_json(&readiness_path, &readiness)?;
        if let Some(stage) = journal.stages.get_mut(10) {
            stage.evidence.push(readiness_path.display().to_string());
        }
        stage_complete(
            &mut journal,
            10,
            "readiness report",
            &journal_path,
            options.fail_after_stage,
        )?;
        let blocking_checks = readiness
            .checks
            .iter()
            .filter(|check| check.blocking && check.status == "block")
            .map(|check| check.id.as_str())
            .collect::<Vec<_>>();
        if !blocking_checks.is_empty() {
            return Err(AppError::Transaction(format!(
                "readiness is blocked; success lock was not written: {}",
                blocking_checks.join(", ")
            )));
        }
        final_live_verification(&project_root, plan, &journal)?;
        let lock = build_lock(
            plan,
            prepared_files,
            &journal,
            previous_lock.as_ref(),
            &project_root,
        )?;
        let lock_bytes = serialized_json_bytes(&lock)?;
        journal.result_lock_exists = Some(true);
        journal.result_lock_sha256 = Some(sha256_bytes(&lock_bytes));
        stage_start(
            &mut journal,
            11,
            "rollback record",
            &journal_path,
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
        persist_journal(&journal_path, &mut journal)?;
        atomic_write_json(&roots.transaction.join("rollback-record.json"), &journal)?;
        journal.rollback_record_sha256 = Some(sha256_file(
            &roots.transaction.join("rollback-record.json"),
        )?);
        persist_journal(&journal_path, &mut journal)?;
        maybe_abort_for_test("after_rollback_record");
        if options.fail_after_stage == Some(11) {
            return Err(AppError::Transaction(
                "fault injected after stage rollback record".into(),
            ));
        }
        let metadata_root = project_root.join(".hoi4-mod-setup");
        fs::create_dir_all(&metadata_root)?;
        // The lock is the final success artifact. Journal finalization after
        // this point is best-effort: a stale `finalizing` journal is safely
        // reconciled by resume only after the lock and rollback record verify.
        atomic_write_json(&metadata_root.join("install.lock.json"), &lock)?;
        maybe_abort_for_test("after_lock_write");
        stage_complete(&mut journal, 11, "rollback record", &journal_path, None)?;
        journal.state = "completed".into();
        journal.recovery = RecoveryState {
            resume_allowed: false,
            rollback_allowed: true,
            discard_staging_allowed: false,
            project_apply_started: true,
            recommended_action: "none".into(),
        };
        let _ = persist_journal(&journal_path, &mut journal);
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
            let _ = persist_journal(&journal_path, &mut journal);
            Err(error)
        }
    }
}

fn persist_journal(path: &Path, journal: &mut TransactionJournal) -> Result<(), AppError> {
    journal.updated_at = Utc::now().to_rfc3339();
    atomic_write_json(path, journal)
}

fn operation_checkpoint_root(journal_path: &Path) -> Result<PathBuf, AppError> {
    let parent = journal_path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("transaction journal has no parent".into()))?;
    Ok(parent.join("operation-checkpoints.jsonl"))
}

#[cfg(test)]
fn persist_operation_checkpoint(
    journal_path: &Path,
    journal: &mut TransactionJournal,
    operation_index: usize,
) -> Result<(), AppError> {
    append_operation_checkpoints(journal_path, journal, &[operation_index], true)
}

fn append_operation_checkpoint(
    journal_path: &Path,
    journal: &mut TransactionJournal,
    operation_index: usize,
) -> Result<(), AppError> {
    append_operation_checkpoints(journal_path, journal, &[operation_index], false)
}

fn persist_operation_checkpoint_batch(
    journal_path: &Path,
    journal: &mut TransactionJournal,
    operation_indices: &[usize],
) -> Result<(), AppError> {
    append_operation_checkpoints(journal_path, journal, operation_indices, true)
}

fn append_operation_checkpoints(
    journal_path: &Path,
    journal: &mut TransactionJournal,
    operation_indices: &[usize],
    sync: bool,
) -> Result<(), AppError> {
    if operation_indices.is_empty() {
        return Ok(());
    }
    let checkpoint_path = operation_checkpoint_root(journal_path)?;
    if path_has_link_component(&checkpoint_path) {
        return Err(AppError::PathSecurity(
            "operation checkpoint storage contains a symlink or junction".into(),
        ));
    }
    if !checkpoint_path.exists() {
        // Atomically create the append log once so its directory entry is
        // durable before an apply-intent checkpoint can guard a live change.
        atomic_write(&checkpoint_path, b"")?;
    }
    journal.updated_at = Utc::now().to_rfc3339();
    let mut bytes = Vec::new();
    for operation_index in operation_indices {
        let operation = journal
            .operations
            .get(*operation_index)
            .cloned()
            .ok_or_else(|| AppError::Transaction("operation checkpoint index is invalid".into()))?;
        let checkpoint = OperationCheckpoint {
            schema_version: "1.0.0".into(),
            transaction_id: journal.transaction_id,
            operation_index: *operation_index,
            operation,
            journal_state: journal.state.clone(),
            last_checkpoint: journal.last_checkpoint.clone(),
            recovery: journal.recovery.clone(),
            updated_at: journal.updated_at.clone(),
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

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&checkpoint_path)?;
    file.write_all(&bytes)?;
    if sync {
        file.sync_data()?;
    }
    Ok(())
}

fn clear_operation_checkpoints(journal_path: &Path) -> Result<(), AppError> {
    let root = operation_checkpoint_root(journal_path)?;
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if is_link_metadata(&metadata) || !metadata.is_file() || path_has_link_component(&root) {
        return Err(AppError::PathSecurity(
            "operation checkpoint storage is not a regular file".into(),
        ));
    }
    fs::remove_file(root)?;
    Ok(())
}

fn compact_operation_checkpoints(
    journal_path: &Path,
    journal: &mut TransactionJournal,
) -> Result<(), AppError> {
    persist_journal(journal_path, journal)?;
    clear_operation_checkpoints(journal_path)
}

fn replay_operation_checkpoints(
    journal_path: &Path,
    journal: &mut TransactionJournal,
) -> Result<(), AppError> {
    let root = operation_checkpoint_root(journal_path)?;
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if is_link_metadata(&metadata) || !metadata.is_file() || path_has_link_component(&root) {
        return Err(AppError::PathSecurity(
            "operation checkpoint storage is not a regular file".into(),
        ));
    }
    let snapshot_updated_at = journal.updated_at.clone();
    let bytes = fs::read(&root)?;
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
        if checkpoint.schema_version != "1.0.0"
            || checkpoint.transaction_id != journal.transaction_id
            || checkpoint.operation_index >= journal.operations.len()
            || checkpoint.operation.id != journal.operations[checkpoint.operation_index].id
        {
            return Err(AppError::Transaction(
                "operation checkpoint does not match its transaction".into(),
            ));
        }
        if checkpoint.updated_at <= snapshot_updated_at {
            continue;
        }
        journal.operations[checkpoint.operation_index] = checkpoint.operation;
        if checkpoint.updated_at > journal.updated_at {
            journal.state = checkpoint.journal_state;
            journal.last_checkpoint = checkpoint.last_checkpoint;
            journal.recovery = checkpoint.recovery;
            journal.updated_at = checkpoint.updated_at;
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

fn staging_destination(
    staging_root: &Path,
    operation: &PlanOperation,
) -> Result<PathBuf, AppError> {
    if operation.external {
        let relative = normalize_relative_path(&format!("external/{}", operation.id))?;
        Ok(staging_root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR)))
    } else {
        Ok(staging_root.join(
            normalize_relative_path(&operation.destination)?
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        ))
    }
}

fn stage_start(
    journal: &mut TransactionJournal,
    index: usize,
    id: &str,
    journal_path: &Path,
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
    persist_journal(journal_path, journal)
}

fn stage_complete(
    journal: &mut TransactionJournal,
    index: usize,
    id: &str,
    journal_path: &Path,
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
    persist_journal(journal_path, journal)
}

fn add_stage_evidence(
    journal: &mut TransactionJournal,
    index: usize,
    evidence: Vec<String>,
    journal_path: &Path,
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
    persist_journal(journal_path, journal)
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

fn backup_existing(
    project_root: &Path,
    plan: &InstallationPlan,
    backup_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    if path_has_link_component(backup_root) {
        return Err(AppError::PathSecurity(
            "backup root contains a symlink or junction".into(),
        ));
    }
    let mut checkpointed = 0usize;
    for (operation_index, operation) in plan.operations.iter().enumerate() {
        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) {
            continue;
        }
        let destination = operation_destination(project_root, operation)?;
        if !destination.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&destination)?;
        if is_link_metadata(&metadata) {
            return Err(AppError::PathSecurity(format!(
                "refusing to back up symlink: {}",
                operation.destination
            )));
        }
        let backup = backup_root.join(format!("{}.bak", operation.id));
        if path_has_link_component(&backup) {
            return Err(AppError::PathSecurity(format!(
                "backup path contains a symlink or junction: {}",
                backup.display()
            )));
        }
        if metadata.is_file() {
            let current_hash = sha256_file(&destination)?;
            if operation.local_sha256.as_deref() != Some(current_hash.as_str()) {
                return Err(AppError::Transaction(format!(
                    "live precondition changed before backup: {}",
                    operation.destination
                )));
            }
            fs::copy(&destination, &backup)?;
            if sha256_file(&backup)? != current_hash {
                return Err(AppError::Transaction(format!(
                    "backup verification failed: {}",
                    operation.destination
                )));
            }
            let before_executable = observed_executable(&destination)?;
            if observed_executable(&backup)? != before_executable {
                return Err(AppError::Transaction(format!(
                    "backup executable metadata verification failed: {}",
                    operation.destination
                )));
            }
            if let Some(record) = journal
                .operations
                .iter_mut()
                .find(|record| record.id == operation.id)
            {
                record.before_executable = before_executable;
            }
        } else if metadata.is_dir() {
            return Err(AppError::Transaction(format!(
                "directory replacement requires an explicit removal plan: {}",
                operation.destination
            )));
        }
        let recorded = if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.backup_path = Some(backup.display().to_string());
            true
        } else {
            false
        };
        if recorded {
            if let Some(record) = journal
                .operations
                .iter_mut()
                .find(|record| record.id == operation.id)
            {
                record.backup_sha256 = Some(sha256_file(&backup)?);
            }
            journal.last_checkpoint = format!("backup-file-{}", operation.id);
            append_operation_checkpoint(journal_path, journal, operation_index)?;
            checkpointed += 1;
            if checkpointed % OPERATION_CHECKPOINT_BATCH == 0 {
                compact_operation_checkpoints(journal_path, journal)?;
            }
        }
    }
    Ok(())
}

fn stage_files(
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    staging_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
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
        let staged = staging_destination(staging_root, operation)?;
        if path_has_link_component(&staged) {
            return Err(AppError::PathSecurity(format!(
                "staging path contains a symlink or junction: {}",
                staged.display()
            )));
        }
        if let Some(parent) = staged.parent() {
            fs::create_dir_all(parent)?;
            if path_has_link_component(parent) {
                return Err(AppError::PathSecurity(format!(
                    "staging parent contains a symlink or junction: {}",
                    parent.display()
                )));
            }
        }
        if let Ok(metadata) = fs::symlink_metadata(&staged) {
            if is_link_metadata(&metadata) || !metadata.is_file() {
                return Err(AppError::PathSecurity(format!(
                    "staging destination is not a regular file: {}",
                    staged.display()
                )));
            }
        }
        fs::write(&staged, &file.bytes)?;
        apply_executable_state(&staged, operation.executable)?;
        if observed_executable(&staged)?.is_some_and(|value| value != operation.executable) {
            return Err(AppError::Transaction(format!(
                "staging executable metadata mismatch for {}",
                operation.destination
            )));
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
        append_operation_checkpoint(journal_path, journal, operation_index)?;
        checkpointed += 1;
        if checkpointed % OPERATION_CHECKPOINT_BATCH == 0 {
            compact_operation_checkpoints(journal_path, journal)?;
        }
    }
    Ok(())
}

fn stage_profile_directories(plan: &InstallationPlan, staging_root: &Path) -> Result<(), AppError> {
    for directory in &plan.transaction.directories {
        let staged = safe_join(staging_root, directory)?;
        if path_has_link_component(&staged) {
            return Err(AppError::PathSecurity(format!(
                "staged profile directory contains a symlink or junction: {directory}"
            )));
        }
        fs::create_dir_all(staged)?;
    }
    Ok(())
}

fn validate_staging(
    project_root: &Path,
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    staging_root: &Path,
) -> Result<(), AppError> {
    for directory in &plan.transaction.directories {
        let staged = safe_join(staging_root, directory)?;
        let metadata = fs::symlink_metadata(&staged)?;
        if is_link_metadata(&metadata) || !metadata.is_dir() {
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
        let staged = staging_destination(staging_root, operation)?;
        if path_has_link_component(&staged) {
            return Err(AppError::PathSecurity(format!(
                "staging path contains a symlink or junction: {}",
                staged.display()
            )));
        }
        let staged_metadata = fs::symlink_metadata(&staged)?;
        if is_link_metadata(&staged_metadata) || !staged_metadata.is_file() {
            return Err(AppError::PathSecurity(format!(
                "staging destination is not a regular file: {}",
                staged.display()
            )));
        }
        let actual = sha256_file(&staged)?;
        let expected = prepared
            .get(operation.id.as_str())
            .map(|file| file.expected_sha256.as_str())
            .unwrap_or("");
        if actual != expected {
            return Err(AppError::Transaction(format!(
                "staging hash mismatch for {}",
                operation.destination
            )));
        }
        if observed_executable(&staged)?.is_some_and(|value| value != operation.executable) {
            return Err(AppError::Transaction(format!(
                "staging executable metadata changed for {}",
                operation.destination
            )));
        }
        let bytes = fs::read(&staged)?;
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
        let expected = validate_project_root_or_destination(project_root)?
            .0
            .to_string_lossy()
            .replace('\\', "/");
        let declared = declared_path.replace('\\', "/");
        let matches = if cfg!(target_os = "windows") {
            declared.eq_ignore_ascii_case(&expected)
        } else {
            declared == expected
        };
        if !matches {
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
        let schema = if lower.ends_with("/.hoi4-mod-setup/project-state.json")
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
    journal_path: &Path,
) -> Result<(), AppError> {
    match plan.transaction.project_root_mode {
        ProjectRootMode::Existing => {
            validate_project_root(project_root)?;
            Ok(())
        }
        ProjectRootMode::CreateLeaf => {
            if journal.project_root_lifecycle.mode != ProjectRootMode::CreateLeaf {
                return Err(AppError::Transaction(
                    "journal root lifecycle does not match the reviewed plan".into(),
                ));
            }
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
            persist_journal(journal_path, journal)?;
            maybe_abort_for_test("before_project_root_create");
            fs::create_dir(project_root).map_err(|error| {
                AppError::Transaction(format!(
                    "could not create reviewed project folder {}: {error}",
                    project_root.display()
                ))
            })?;
            maybe_abort_for_test("after_project_root_create");
            let metadata = fs::symlink_metadata(project_root)?;
            if is_link_metadata(&metadata) || !metadata.is_dir() {
                return Err(AppError::PathSecurity(
                    "created project root is not a regular directory".into(),
                ));
            }
            journal.project_root_lifecycle.checkpoint = "created".into();
            journal.project_root_lifecycle.created_by_transaction = true;
            journal.project_root_lifecycle.observed_exists = true;
            journal.last_checkpoint = "apply-project-root-created".into();
            persist_journal(journal_path, journal)
        }
    }
}

fn apply_profile_directories(
    project_root: &Path,
    plan: &InstallationPlan,
    journal: &mut TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    let mut missing = std::collections::BTreeSet::new();
    for directory in &plan.transaction.directories {
        let destination = safe_join(project_root, directory)?;
        if destination == project_root {
            return Err(AppError::PathSecurity(
                "profile directory cannot be the project root".into(),
            ));
        }
        let mut current = project_root.to_path_buf();
        for component in Path::new(directory).components() {
            current.push(component.as_os_str());
            match fs::symlink_metadata(&current) {
                Ok(metadata) if is_link_metadata(&metadata) || !metadata.is_dir() => {
                    return Err(AppError::PathSecurity(format!(
                        "profile destination is not a regular directory: {directory}"
                    )))
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let relative = current.strip_prefix(project_root).map_err(|_| {
                        AppError::PathSecurity("profile directory escaped the project root".into())
                    })?;
                    missing.insert(relative.to_string_lossy().replace('\\', "/"));
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    journal.created_directories = missing.into_iter().collect();
    journal.last_checkpoint = "apply-profile-directories-intent".into();
    persist_journal(journal_path, journal)?;
    for directory in &plan.transaction.directories {
        let destination = safe_join(project_root, directory)?;
        fs::create_dir_all(&destination)?;
        let metadata = fs::symlink_metadata(&destination)?;
        if is_link_metadata(&metadata) || !metadata.is_dir() {
            return Err(AppError::PathSecurity(format!(
                "created profile path is not a regular directory: {directory}"
            )));
        }
    }
    journal.last_checkpoint = "apply-profile-directories-created".into();
    persist_journal(journal_path, journal)
}

fn apply_operations(
    project_root: &Path,
    plan: &InstallationPlan,
    staging_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
    options: &TransactionOptions,
) -> Result<(), AppError> {
    mark_project_apply_started(journal);
    persist_journal(journal_path, journal)?;
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
            persist_operation_checkpoint_batch(journal_path, journal, &intent_indices)?;
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
            append_operation_checkpoint(journal_path, journal, index)?;
            if options.fail_after_operation == Some(index) {
                return Err(AppError::Transaction(format!(
                    "fault injected after no-op operation {}",
                    operation.id
                )));
            }
            if (index + 1) % OPERATION_CHECKPOINT_BATCH == 0 || index + 1 == plan.operations.len() {
                compact_operation_checkpoints(journal_path, journal)?;
            }
            continue;
        }
        let destination = operation_destination(project_root, operation)?;
        let current_hash = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        if destination.exists() && !destination.is_file() {
            return Err(AppError::Transaction(format!(
                "destination is not a regular file: {}",
                operation.destination
            )));
        }
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
        let staged = staging_destination(staging_root, operation)?;
        if path_has_link_component(&staged) {
            return Err(AppError::PathSecurity(format!(
                "staging path contains a symlink or junction: {}",
                staged.display()
            )));
        }
        let staged_hash = if operation.action != OperationAction::DeleteManaged {
            let staged_metadata = fs::symlink_metadata(&staged)?;
            if is_link_metadata(&staged_metadata) || !staged_metadata.is_file() {
                return Err(AppError::PathSecurity(format!(
                    "staging destination is not a regular file: {}",
                    staged.display()
                )));
            }
            let staged_hash = sha256_file(&staged)?;
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
        match operation.action {
            OperationAction::DeleteManaged => {
                if current_hash.is_some() {
                    fs::remove_file(&destination)?;
                }
            }
            _ => {
                copy_atomic(&staged, &destination)?;
                apply_executable_state(&destination, operation.executable)?;
            }
        }
        if options.fail_after_live_mutation == Some(index) {
            return Err(AppError::Transaction(format!(
                "fault injected after live mutation {}",
                operation.id
            )));
        }
        let after_hash = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        let after_executable = if destination.is_file() {
            observed_executable(&destination)?
        } else {
            None
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
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "verified".into();
            record.staged_sha256 = staged_hash;
            record.after_sha256 = after_hash;
            record.after_exists = Some(destination.is_file());
            record.after_executable = after_executable;
        }
        journal.last_checkpoint = format!("apply-{}", operation.id);
        append_operation_checkpoint(journal_path, journal, index)?;
        if options.fail_after_operation == Some(index) {
            return Err(AppError::Transaction(format!(
                "fault injected after operation {}",
                operation.id
            )));
        }
        if (index + 1) % OPERATION_CHECKPOINT_BATCH == 0 || index + 1 == plan.operations.len() {
            compact_operation_checkpoints(journal_path, journal)?;
        }
    }
    Ok(())
}

fn copy_atomic(source: &Path, destination: &Path) -> Result<(), AppError> {
    if path_has_link_component(source) || path_has_link_component(destination) {
        return Err(AppError::PathSecurity(
            "atomic copy path contains a symlink or junction".into(),
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::Transaction("destination has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| {
        AppError::Transaction(format!(
            "could not create atomic destination parent {}: {error}",
            parent.display()
        ))
    })?;
    let temporary = parent.join(format!(
        ".{}.{}.apply.tmp",
        destination.file_name().unwrap().to_string_lossy(),
        Uuid::new_v4()
    ));
    fs::copy(source, &temporary).map_err(|error| {
        AppError::Transaction(format!(
            "could not stage atomic copy {} -> {}: {error}",
            source.display(),
            temporary.display()
        ))
    })?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| {
            AppError::Transaction(format!(
                "could not reopen atomic temporary {}: {error}",
                temporary.display()
            ))
        })?;
    file.sync_all().map_err(|error| {
        AppError::Transaction(format!(
            "could not flush atomic temporary {}: {error}",
            temporary.display()
        ))
    })?;
    drop(file);
    replace_path(&temporary, destination).map_err(|error| {
        AppError::Transaction(format!(
            "could not atomically replace {}: {error}",
            destination.display()
        ))
    })?;
    Ok(())
}

#[cfg(unix)]
fn observed_executable(path: &Path) -> Result<Option<bool>, AppError> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link_metadata(&metadata) || !metadata.is_file() {
        return Err(AppError::PathSecurity(format!(
            "executable metadata target is not a regular file: {}",
            path.display()
        )));
    }
    Ok(Some(metadata.permissions().mode() & 0o111 != 0))
}

#[cfg(not(unix))]
fn observed_executable(_path: &Path) -> Result<Option<bool>, AppError> {
    Ok(None)
}

#[cfg(unix)]
fn apply_executable_state(path: &Path, executable: bool) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link_metadata(&metadata) || !metadata.is_file() {
        return Err(AppError::PathSecurity(format!(
            "executable metadata target is not a regular file: {}",
            path.display()
        )));
    }
    let mut permissions = metadata.permissions();
    let mut mode = permissions.mode();
    if executable {
        mode |= 0o111;
    } else {
        mode &= !0o111;
    }
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn apply_executable_state(_path: &Path, _executable: bool) -> Result<(), AppError> {
    Ok(())
}

#[cfg(windows)]
fn replace_path(temporary: &Path, destination: &Path) -> Result<(), AppError> {
    use std::os::windows::ffi::OsStrExt;
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let temporary_wide = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = if destination.exists() {
        unsafe {
            windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
                destination_wide.as_ptr(),
                temporary_wide.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        }
    } else {
        0
    };
    if result == 0 {
        if destination.exists() {
            return Err(AppError::Transaction(format!(
                "atomic replacement failed for {}: {}",
                destination.display(),
                std::io::Error::last_os_error()
            )));
        }
        fs::rename(temporary, destination)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_path(temporary: &Path, destination: &Path) -> Result<(), AppError> {
    fs::rename(temporary, destination)?;
    Ok(())
}

fn post_install_checks(
    project_root: &Path,
    plan: &InstallationPlan,
    journal: &mut TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    for operation in &plan.operations {
        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) {
            continue;
        }
        let destination = operation_destination(project_root, operation)?;
        let actual = if operation.action == OperationAction::DeleteManaged {
            if destination.exists() {
                return Err(AppError::Transaction(format!(
                    "managed delete was not completed: {}",
                    operation.destination
                )));
            }
            None
        } else {
            let bytes = fs::read(&destination)?;
            validate_managed_bytes(project_root, operation, &bytes)?;
            Some(sha256_bytes(&bytes))
        };
        let record = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
            .ok_or_else(|| AppError::Transaction("journal operation missing".into()))?;
        if record.after_sha256.as_deref() != actual.as_deref()
            || (operation.action != OperationAction::DeleteManaged
                && actual.as_deref()
                    != operation
                        .result_sha256
                        .as_deref()
                        .or(operation.source_sha256.as_deref()))
            || (operation.action != OperationAction::DeleteManaged
                && observed_executable(&destination)?
                    .is_some_and(|value| value != operation.executable))
        {
            return Err(AppError::Transaction(format!(
                "post-install hash mismatch for {}",
                operation.destination
            )));
        }
    }
    journal.last_checkpoint = "post-install-verified".into();
    persist_journal(journal_path, journal)
}

/// Re-check every destination immediately before the success lock is built.
/// The apply and readiness passes are necessary but not sufficient: a local
/// editor or another process may have changed a skipped or applied file after
/// those checkpoints. A changed live precondition fails closed instead of
/// allowing the lock to record bytes that were never reviewed.
fn final_live_verification(
    project_root: &Path,
    plan: &InstallationPlan,
    journal: &TransactionJournal,
) -> Result<(), AppError> {
    for operation in &plan.operations {
        let destination = operation_destination(project_root, operation)?;
        let current = regular_file_hash(&destination)?;
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
                    || observed_executable(&destination)?
                        .is_some_and(|value| value != operation.executable)
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
                integration: "codex_app_server".into(),
                auth_mode: "chatgpt".into(),
                authenticated_during_setup: plan.codex_analysis.is_some(),
                analysis_status: "not_required_for_removal".into(),
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
        && fs::read(safe_join(project_root, "descriptor.mod")?)
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
        match crate::mcp::reviewed_plan_target(&plan.external_actions)
            .and_then(|target| crate::mcp::initialize_health(project_root, &target))
        {
            Ok(_) => "pass".into(),
            Err(AppError::UnsupportedPlatform(_)) => "planned_unavailable".into(),
            Err(_) => "block".into(),
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
            .and_then(|path| fs::read(path).ok())
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
    let report = crate::readiness::evaluate(&crate::readiness::ReadinessInput {
        project_id: plan.project_id.clone(),
        project_root: project_root.display().to_string(),
        selected_components: readiness_components,
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
                        .and_then(|path| fs::read(path).ok())
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
    Ok(report)
}

fn build_lock(
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    journal: &TransactionJournal,
    previous_lock: Option<&InstallationLock>,
    project_root: &Path,
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
                } else if operation.local_state != LocalState::Absent {
                    // A first install can intentionally keep a user-owned
                    // file (most importantly an existing thumbnail). It must
                    // still be represented in the lock so readiness can hash
                    // it and future maintenance cannot treat it as absent.
                    let destination = operation_destination(project_root, operation)?;
                    let bytes = fs::read(&destination).map_err(|error| {
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
                    generated_content: if operation.action == OperationAction::Generate {
                        String::from_utf8(prepared.bytes.clone()).ok()
                    } else {
                        None
                    },
                    generated_bytes: (operation.action == OperationAction::Generate)
                        .then_some(prepared.bytes.clone()),
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
            if !component_ids.iter().any(|id| id == &component.id) {
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
        schema_version: "1.0.0".into(),
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
        ai_endpoint: plan.ai_endpoint.clone(),
        ai_optimization_profile: plan.ai_optimization_profile.clone(),
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
    destination: &Path,
    journal: &TransactionJournal,
    journal_path: &Path,
) -> Result<bool, AppError> {
    let current = regular_file_hash(destination)?;
    match operation.rollback {
        Some(RollbackAction::None) | None => Ok(true),
        Some(RollbackAction::RemoveCreated) => Ok(current.is_none()),
        Some(RollbackAction::RestoreBackup | RollbackAction::ReverseMerge) => {
            let Some(backup_path) = operation.backup_path.as_ref() else {
                return Err(AppError::Transaction(format!(
                    "rollback backup metadata is missing for {}",
                    operation.destination
                )));
            };
            let expected_backup = expected_operation_backup(journal, journal_path, &operation.id)?;
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
                    "journal backup path is outside the transaction backup root".into(),
                ));
            }
            let metadata = fs::symlink_metadata(&expected_backup).map_err(|error| {
                AppError::Transaction(format!(
                    "rollback backup is unavailable for {}: {error}",
                    operation.destination
                ))
            })?;
            if is_link_metadata(&metadata) || !metadata.is_file() {
                return Err(AppError::PathSecurity(
                    "rollback backup is not a regular file".into(),
                ));
            }
            let expected = operation
                .backup_sha256
                .clone()
                .unwrap_or(sha256_file(&expected_backup)?);
            let executable_matches = match (operation.before_executable, current.as_ref()) {
                (Some(expected), Some(_)) => observed_executable(destination)? == Some(expected),
                (Some(_), None) => false,
                (None, _) => true,
            };
            Ok(current.as_deref() == Some(expected.as_str()) && executable_matches)
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
        schema_version: "1.0.0".into(),
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

fn rollback_lock_hash(project_root: &Path) -> Result<Option<String>, AppError> {
    let lock_path = safe_join(project_root, ".hoi4-mod-setup/install.lock.json")?;
    regular_file_hash(&lock_path)
}

/// Validate the lock before any rollback file is touched. A rollback may only
/// start when the live lock is either the transaction's recorded result or the
/// predecessor state already restored by an interrupted retry. This prevents
/// a later user/tool edit from being overwritten after the first file restore.
fn validate_rollback_lock_precondition(
    project_root: &Path,
    parent: &TransactionJournal,
) -> Result<(), AppError> {
    let current = rollback_lock_hash(project_root)?;
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
    project_root: &Path,
    backup_root: &Path,
    rollback: &mut TransactionJournal,
    parent: &TransactionJournal,
) -> Result<(), AppError> {
    let lock_path = safe_join(project_root, ".hoi4-mod-setup/install.lock.json")?;
    let backup_path = backup_root.join("install.lock.json.bak");
    if path_has_link_component(&backup_path) {
        return Err(AppError::PathSecurity(
            "rollback lock backup path contains a symlink or junction".into(),
        ));
    }
    match fs::symlink_metadata(&lock_path) {
        Ok(metadata) if is_link_metadata(&metadata) || !metadata.is_file() => Err(
            AppError::PathSecurity("installation lock is not a regular file".into()),
        ),
        Ok(_) => {
            let current_hash = sha256_file(&lock_path)?;
            if backup_path.is_file() {
                let backup_hash = sha256_file(&backup_path)?;
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
            } else {
                copy_atomic(&lock_path, &backup_path)?;
            }
            rollback.previous_lock_backup_path = Some(backup_path.display().to_string());
            rollback.previous_lock_sha256 = Some(sha256_file(&backup_path)?);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            rollback.previous_lock_backup_path = None;
            rollback.previous_lock_sha256 = None;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn prepare_rollback_transaction(
    project_root: &Path,
    parent: &TransactionJournal,
    parent_journal_path: &Path,
) -> Result<(TransactionJournal, PathBuf), AppError> {
    let app_root = journal_app_root(parent_journal_path)?;
    if path_has_link_component(&app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    let transaction_id = parent.rollback_transaction_id.unwrap_or_else(Uuid::new_v4);
    let roots = transaction_root(&app_root, transaction_id);
    if path_has_link_component(&roots.transaction) || path_has_link_component(&roots.backup) {
        return Err(AppError::PathSecurity(
            "rollback transaction storage contains a symlink or junction".into(),
        ));
    }
    fs::create_dir_all(&roots.transaction)?;
    fs::create_dir_all(&roots.backup)?;
    let journal_path = roots.transaction.join("journal.json");
    let mut rollback = if journal_path.is_file() {
        let journal = read_journal(&journal_path)?;
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
        let journal = new_rollback_journal(parent, transaction_id, project_root);
        atomic_write_json(&journal_path, &journal)?;
        journal
    };

    validate_rollback_lock_precondition(project_root, parent)?;
    capture_rollback_lock_backup(project_root, &roots.backup, &mut rollback, parent)?;
    compact_operation_checkpoints(&journal_path, &mut rollback)?;

    let mut checkpointed = 0usize;
    for index in 0..rollback.operations.len() {
        let operation = rollback.operations[index].clone();
        if operation.status == "rolled_back" || operation.action == Some(OperationAction::Skip) {
            continue;
        }
        if operation.status == "rollback_applying" {
            if let Some(backup_path) = operation.backup_path.as_ref() {
                let expected_backup =
                    expected_operation_backup(&rollback, &journal_path, &operation.id)?;
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
                let metadata = fs::symlink_metadata(&expected_backup).map_err(|error| {
                    AppError::Transaction(format!(
                        "rollback retry backup is unavailable for {}: {error}",
                        operation.destination
                    ))
                })?;
                if is_link_metadata(&metadata) || !metadata.is_file() {
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
                if sha256_file(&expected_backup)? != expected_hash {
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
        let destination = rollback_operation_destination(project_root, &operation)?;
        let backup = roots.backup.join(format!("{}.bak", operation.id));
        if path_has_link_component(&backup) {
            return Err(AppError::PathSecurity(
                "rollback backup path contains a symlink or junction".into(),
            ));
        }
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if is_link_metadata(&metadata) => {
                return Err(AppError::PathSecurity(format!(
                    "refusing to back up rollback destination link: {}",
                    operation.destination
                )));
            }
            Ok(metadata) if metadata.is_file() => {
                let current_hash = sha256_file(&destination)?;
                if backup.is_file() {
                    if sha256_file(&backup)? != current_hash {
                        return Err(AppError::Transaction(format!(
                            "rollback backup changed before apply: {}",
                            operation.destination
                        )));
                    }
                } else {
                    copy_atomic(&destination, &backup)?;
                }
                rollback.operations[index].before_sha256 = Some(current_hash.clone());
                rollback.operations[index].before_executable = observed_executable(&destination)?;
                rollback.operations[index].after_exists = Some(true);
                rollback.operations[index].backup_path = Some(backup.display().to_string());
                rollback.operations[index].backup_sha256 = Some(sha256_file(&backup)?);
            }
            Ok(_) => {
                return Err(AppError::Transaction(format!(
                    "rollback destination is not a regular file: {}",
                    operation.destination
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                rollback.operations[index].before_sha256 = None;
                rollback.operations[index].after_exists = Some(false);
                rollback.operations[index].backup_path = None;
                rollback.operations[index].backup_sha256 = None;
            }
            Err(error) => return Err(error.into()),
        }
        rollback.last_checkpoint = format!("rollback-backup-{}", operation.id);
        append_operation_checkpoint(&journal_path, &mut rollback, index)?;
        checkpointed += 1;
        if checkpointed % OPERATION_CHECKPOINT_BATCH == 0 {
            compact_operation_checkpoints(&journal_path, &mut rollback)?;
        }
    }
    rollback.state = "applying".into();
    rollback.last_checkpoint = "rollback-backup-complete".into();
    if let Some(stage) = rollback.stages.get_mut(5) {
        stage.status = "complete".into();
        stage.completed_at = Some(Utc::now().to_rfc3339());
    }
    compact_operation_checkpoints(&journal_path, &mut rollback)?;
    Ok((rollback, journal_path))
}

fn persist_rollback_checkpoint(
    rollback: &mut TransactionJournal,
    rollback_path: &Path,
    parent_operation_id: &str,
    status: &str,
    checkpoint: &str,
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
    append_operation_checkpoint(rollback_path, rollback, operation_index)
}

fn ensure_project_root_for_inverse_rollback(
    project_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
    rollback_journal: &mut TransactionJournal,
    rollback_path: &Path,
) -> Result<(), AppError> {
    if journal.transaction_kind != "rollback"
        || journal.project_root_lifecycle.mode != ProjectRootMode::CreateLeaf
    {
        return Ok(());
    }

    let checkpoint = journal.project_root_lifecycle.checkpoint.clone();
    match checkpoint.as_str() {
        "removed" | "applying" => {
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
                let root = validate_project_root(project_root)?;
                if fs::read_dir(&root)?.next().transpose()?.is_some() {
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
                persist_journal(journal_path, journal)?;
                persist_journal(rollback_path, rollback_journal)?;
                maybe_abort_for_test("before_inverse_project_root_create");
                fs::create_dir(project_root).map_err(|error| {
                    AppError::Transaction(format!(
                        "could not recreate reviewed project folder {}: {error}",
                        project_root.display()
                    ))
                })?;
                maybe_abort_for_test("after_inverse_project_root_create");
            }
            let metadata = fs::symlink_metadata(project_root)?;
            if is_link_metadata(&metadata) || !metadata.is_dir() {
                return Err(AppError::PathSecurity(
                    "recreated project root is not a regular directory".into(),
                ));
            }
            journal.project_root_lifecycle.checkpoint = "created".into();
            journal.project_root_lifecycle.created_by_transaction = true;
            journal.project_root_lifecycle.observed_exists = true;
            journal.project_root_lifecycle.cleanup_result = None;
            journal.last_checkpoint = "inverse-rollback-project-root-created".into();
            rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
            rollback_journal.last_checkpoint = "inverse-rollback-project-root-created".into();
            persist_journal(journal_path, journal)?;
            persist_journal(rollback_path, rollback_journal)
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
    compact_operation_checkpoints(journal_path, journal)?;
    let (mut rollback_journal, rollback_path) =
        prepare_rollback_transaction(&project_root, journal, journal_path)?;
    maybe_abort_for_test(if journal.transaction_kind == "rollback" {
        "inverse_rollback_after_backup"
    } else {
        "rollback_after_backup"
    });
    ensure_project_root_for_inverse_rollback(
        &project_root,
        journal,
        journal_path,
        &mut rollback_journal,
        &rollback_path,
    )?;
    let result = (|| -> Result<(), AppError> {
        // Remove transaction-created Git metadata before restoring files. The
        // newly initialized index would otherwise record the transaction's
        // applied files as user changes and make safe Git cleanup impossible.
        if journal.git_initialized {
            crate::git::rollback_initialized_git(&project_root)?;
            journal.git_initialized = false;
            persist_journal(journal_path, journal)?;
            rollback_journal.last_checkpoint = "rollback-git-cleanup".into();
            persist_journal(&rollback_path, &mut rollback_journal)?;
        } else if let (Some(name), Some(url)) = (
            journal.git_remote_added_name.as_deref(),
            journal.git_remote_added_url.as_deref(),
        ) {
            crate::git::rollback_added_remote(&project_root, name, url)?;
            journal.git_remote_added_name = None;
            journal.git_remote_added_url = None;
            persist_journal(journal_path, journal)?;
            rollback_journal.last_checkpoint = "rollback-git-cleanup".into();
            persist_journal(&rollback_path, &mut rollback_journal)?;
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
                persist_operation_checkpoint_batch(journal_path, journal, &intent_indices)?;
            }
            let operation = journal.operations[index].clone();
            if journal.transaction_kind != "rollback" && operation.status == "rolled_back" {
                if let Some(child_operation) = rollback_journal
                    .operations
                    .iter()
                    .find(|entry| entry.id == format!("rollback-{}", operation.id))
                {
                    if child_operation.status != "rolled_back"
                        && !rollback_destination_is_restored(
                            &operation,
                            &rollback_operation_destination(&project_root, &operation)?,
                            journal,
                            journal_path,
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
                    &rollback_path,
                    &operation.id,
                    "rolled_back",
                    &format!("rollback-{}", operation.id),
                )?;
                if batch_complete {
                    compact_operation_checkpoints(journal_path, journal)?;
                    compact_operation_checkpoints(&rollback_path, &mut rollback_journal)?;
                }
                continue;
            }
            if !(matches!(
                operation.status.as_str(),
                "rollback_applying" | "applying" | "applied" | "verified"
            ) || (journal.transaction_kind == "rollback" && operation.status == "rolled_back"))
            {
                if batch_complete {
                    compact_operation_checkpoints(journal_path, journal)?;
                    compact_operation_checkpoints(&rollback_path, &mut rollback_journal)?;
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
                append_operation_checkpoint(journal_path, journal, index)?;
                persist_rollback_checkpoint(
                    &mut rollback_journal,
                    &rollback_path,
                    &operation.id,
                    "rolled_back",
                    &format!("rollback-noop-{}", operation.id),
                )?;
                if batch_complete {
                    compact_operation_checkpoints(journal_path, journal)?;
                    compact_operation_checkpoints(&rollback_path, &mut rollback_journal)?;
                }
                continue;
            }
            let destination = if operation.external {
                validate_external_destination(&operation.destination)?
            } else {
                safe_join(&project_root, &operation.destination)?
            };
            if operation.status == "rollback_applying"
                && rollback_destination_is_restored(
                    &operation,
                    &destination,
                    journal,
                    journal_path,
                )?
            {
                journal.operations[index].status = "rolled_back".into();
                journal.last_checkpoint = format!("rollback-{}", operation.id);
                append_operation_checkpoint(journal_path, journal, index)?;
                persist_rollback_checkpoint(
                    &mut rollback_journal,
                    &rollback_path,
                    &operation.id,
                    "rolled_back",
                    &format!("rollback-{}", operation.id),
                )?;
                if batch_complete {
                    compact_operation_checkpoints(journal_path, journal)?;
                    compact_operation_checkpoints(&rollback_path, &mut rollback_journal)?;
                }
                continue;
            }
            journal.operations[index].status = "rollback_applying".into();
            journal.last_checkpoint = format!("rollback-intent-{}", operation.id);
            let current = regular_file_hash(&destination)?;
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
            } else if operation.status == "applying" {
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
            if let Some(backup) = &operation.backup_path {
                let expected_backup =
                    expected_operation_backup(journal, journal_path, &operation.id)?;
                let supplied_backup = PathBuf::from(backup);
                let matches = if cfg!(target_os = "windows") {
                    supplied_backup
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&expected_backup.to_string_lossy())
                } else {
                    supplied_backup == expected_backup
                };
                if !matches {
                    return Err(AppError::PathSecurity(
                        "journal backup path is outside the transaction backup root".into(),
                    ));
                }
                let backup_metadata = fs::symlink_metadata(&expected_backup).ok();
                if backup_metadata.as_ref().is_some_and(is_link_metadata) {
                    return Err(AppError::PathSecurity(
                        "refusing to restore a backup symlink".into(),
                    ));
                }
                if expected_backup.is_file() {
                    let actual_backup = sha256_file(&expected_backup)?;
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
                    copy_atomic(&expected_backup, &destination)?;
                    if let Some(executable) = operation.before_executable {
                        apply_executable_state(&destination, executable)?;
                    }
                } else {
                    return Err(AppError::Transaction(format!(
                        "rollback backup is missing for {}",
                        operation.destination
                    )));
                }
            } else if destination.is_file() {
                fs::remove_file(&destination)?;
            }
            let restored = regular_file_hash(&destination)?;
            if let Some(expected) = expected_restored {
                if restored.as_deref() != Some(expected.as_str()) {
                    return Err(AppError::Transaction(format!(
                        "rollback destination checksum mismatch after restore: {}",
                        operation.destination
                    )));
                }
                if let Some(expected_executable) = operation.before_executable {
                    if observed_executable(&destination)? != Some(expected_executable) {
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
            append_operation_checkpoint(journal_path, journal, index)?;
            persist_rollback_checkpoint(
                &mut rollback_journal,
                &rollback_path,
                &operation.id,
                "rolled_back",
                &format!("rollback-{}", operation.id),
            )?;
            if batch_complete {
                compact_operation_checkpoints(journal_path, journal)?;
                compact_operation_checkpoints(&rollback_path, &mut rollback_journal)?;
            }
        }
        restore_previous_lock(&project_root, journal, journal_path)?;
        cleanup_created_profile_directories(&project_root, journal, journal_path)?;
        if journal.transaction_kind != "rollback" {
            cleanup_created_project_root(
                &project_root,
                journal,
                journal_path,
                &mut rollback_journal,
                &rollback_path,
            )?;
        }
        rollback_journal.last_checkpoint = "rollback-lock-restored".into();
        persist_journal(&rollback_path, &mut rollback_journal)?;
        let result_lock_path = safe_join(&project_root, ".hoi4-mod-setup/install.lock.json")?;
        match fs::symlink_metadata(&result_lock_path) {
            Ok(metadata) if is_link_metadata(&metadata) || !metadata.is_file() => {
                return Err(AppError::PathSecurity(
                    "rollback result lock is not a regular file".into(),
                ));
            }
            Ok(_) => {
                rollback_journal.result_lock_exists = Some(true);
                rollback_journal.result_lock_sha256 = Some(sha256_file(&result_lock_path)?);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                rollback_journal.result_lock_exists = Some(false);
                rollback_journal.result_lock_sha256 = None;
            }
            Err(error) => return Err(error.into()),
        }
        let child_record_path = rollback_path
            .parent()
            .ok_or_else(|| AppError::Transaction("rollback journal has no directory".into()))?
            .join("rollback-record.json");
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
        atomic_write_json(&child_record_path, &child_record)?;
        let child_record_sha256 = sha256_file(&child_record_path)?;
        rollback_journal.rollback_record_sha256 = Some(child_record_sha256.clone());
        rollback_journal.last_checkpoint = "rollback-record-written".into();
        persist_journal(&rollback_path, &mut rollback_journal)?;
        maybe_abort_for_test("after_rollback_child_record");
        rollback_journal = child_record;
        rollback_journal.rollback_record_sha256 = Some(child_record_sha256);
        persist_journal(&rollback_path, &mut rollback_journal)?;
        maybe_abort_for_test("after_rollback_child_complete");
        let parent_record_path = journal_path
            .parent()
            .ok_or_else(|| AppError::Transaction("journal has no transaction directory".into()))?
            .join("rollback-record.json");
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
        atomic_write_json(&parent_record_path, &parent_record)?;
        let parent_record_sha256 = sha256_file(&parent_record_path)?;
        journal.rollback_record_sha256 = Some(parent_record_sha256.clone());
        journal.last_checkpoint = "rollback-record-written".into();
        persist_journal(journal_path, journal)?;
        maybe_abort_for_test("after_rollback_parent_record");
        *journal = parent_record;
        journal.rollback_record_sha256 = Some(parent_record_sha256);
        persist_journal(journal_path, journal)?;
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
        let _ = persist_journal(&rollback_path, &mut rollback_journal);
    }
    result
}

fn cleanup_created_profile_directories(
    project_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    let mut directories = journal.created_directories.clone();
    directories.sort_by_key(|path| std::cmp::Reverse(Path::new(path).components().count()));
    for directory in directories {
        let destination = safe_join(project_root, &directory)?;
        match fs::remove_dir(&destination) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => {
                return Err(AppError::Transaction(format!(
                    "could not remove empty profile directory {}: {error}",
                    destination.display()
                )))
            }
        }
    }
    journal.last_checkpoint = "rollback-profile-directories-checked".into();
    persist_journal(journal_path, journal)
}

fn cleanup_created_project_root(
    project_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
    rollback_journal: &mut TransactionJournal,
    rollback_path: &Path,
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
        persist_journal(journal_path, journal)?;
        persist_journal(rollback_path, rollback_journal)?;
        return Ok(());
    }
    if lifecycle.checkpoint == "removed" {
        journal.project_root_lifecycle.checkpoint = "retained_user_content".into();
        journal.project_root_lifecycle.observed_exists = true;
        journal.project_root_lifecycle.cleanup_result = Some("retained_user_content".into());
        rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
        persist_journal(journal_path, journal)?;
        persist_journal(rollback_path, rollback_journal)?;
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
        match fs::remove_dir(&directory) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => {
                return Err(AppError::Transaction(format!(
                    "could not remove empty managed directory {}: {error}",
                    directory.display()
                )))
            }
        }
    }
    let mut entries = fs::read_dir(&root)?;
    if entries.next().transpose()?.is_some() {
        journal.project_root_lifecycle.checkpoint = "retained_user_content".into();
        journal.project_root_lifecycle.observed_exists = true;
        journal.project_root_lifecycle.cleanup_result = Some("retained_user_content".into());
    } else {
        journal.project_root_lifecycle.checkpoint = "removing".into();
        journal.last_checkpoint = "rollback-project-root-intent".into();
        persist_journal(journal_path, journal)?;
        maybe_abort_for_test("before_project_root_remove");
        fs::remove_dir(&root)?;
        maybe_abort_for_test("after_project_root_remove");
        journal.project_root_lifecycle.checkpoint = "removed".into();
        journal.project_root_lifecycle.observed_exists = false;
        journal.project_root_lifecycle.cleanup_result = Some("removed".into());
    }
    rollback_journal.project_root_lifecycle = journal.project_root_lifecycle.clone();
    persist_journal(journal_path, journal)?;
    persist_journal(rollback_path, rollback_journal)
}

fn restore_previous_lock(
    project_root: &Path,
    journal: &TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    let lock_path = safe_join(project_root, ".hoi4-mod-setup/install.lock.json")?;
    let current = rollback_lock_hash(project_root)?;
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
    let backup_path = if let Some(path) = &journal.previous_lock_backup_path {
        let expected = journal_app_root(journal_path)?
            .join("backups")
            .join(journal.transaction_id.to_string())
            .join("install.lock.json.bak");
        let supplied = PathBuf::from(path);
        let matches = if cfg!(target_os = "windows") {
            supplied
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected.to_string_lossy())
        } else {
            supplied == expected
        };
        if !matches {
            return Err(AppError::PathSecurity(
                "journal lock backup path is outside the transaction backup root".into(),
            ));
        }
        Some(expected)
    } else {
        None
    };
    if let Some(backup_path) = backup_path {
        let metadata = fs::symlink_metadata(&backup_path).map_err(|error| {
            AppError::Transaction(format!(
                "previous installation lock backup is unavailable: {error}"
            ))
        })?;
        if is_link_metadata(&metadata) || !metadata.is_file() {
            return Err(AppError::PathSecurity(
                "previous installation lock backup is not a regular file".into(),
            ));
        }
        let expected_hash = journal
            .previous_lock_sha256
            .as_deref()
            .ok_or_else(|| AppError::Transaction("lock backup has no recorded checksum".into()))?;
        if sha256_file(&backup_path)? != expected_hash {
            return Err(AppError::Transaction(
                "previous installation lock backup checksum mismatch".into(),
            ));
        }
        copy_atomic(&backup_path, &lock_path)?;
        if sha256_file(&lock_path)? != expected_hash {
            return Err(AppError::Transaction(
                "restored installation lock checksum mismatch".into(),
            ));
        }
    } else if previous.is_none() {
        if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
            if is_link_metadata(&metadata) || !metadata.is_file() {
                return Err(AppError::PathSecurity(
                    "refusing to remove an installation lock link during rollback".into(),
                ));
            }
            fs::remove_file(&lock_path)?;
        } else if lock_path.exists() {
            return Err(AppError::Transaction(
                "installation lock could not be inspected during rollback".into(),
            ));
        }
    } else {
        return Err(AppError::Transaction(
            "previous installation lock backup is missing".into(),
        ));
    }
    Ok(())
}

pub fn read_journal(path: &Path) -> Result<TransactionJournal, AppError> {
    if path_has_link_component(path) {
        return Err(AppError::PathSecurity(
            "transaction journal path contains a symlink or junction".into(),
        ));
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| AppError::Transaction(error.to_string()))?;
    if is_link_metadata(&metadata) || !metadata.is_file() {
        return Err(AppError::PathSecurity(
            "transaction journal is not a regular file".into(),
        ));
    }
    let bytes = fs::read(path).map_err(|error| AppError::Transaction(error.to_string()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Transaction(format!("invalid transaction journal: {error}")))?;
    let mut journal = crate::migrations::migrate_journal(value)?;
    replay_operation_checkpoints(path, &mut journal)?;
    Ok(journal)
}

fn finish_finalization(
    project_root: &Path,
    app_root: &Path,
    transaction_id: Uuid,
    journal: &mut TransactionJournal,
    journal_path: &Path,
) -> Result<(TransactionJournal, InstallationLock), AppError> {
    let project_root = validate_journal_project_root(project_root, journal, journal_path)?;
    if journal.transaction_id != transaction_id || journal.state != "finalizing" {
        return Err(AppError::Transaction(
            "transaction is not in the finalization state".into(),
        ));
    }
    if crate::security::path_has_link_component(app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    let lock_path = safe_join(&project_root, ".hoi4-mod-setup/install.lock.json")?;
    let lock_bytes = fs::read(&lock_path).map_err(|error| {
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
    let record_path = app_root.join(&record_reference);
    if path_has_link_component(&record_path) {
        return Err(AppError::PathSecurity(
            "finalization rollback record contains a symlink or junction".into(),
        ));
    }
    let record_metadata = fs::symlink_metadata(&record_path).map_err(|error| {
        AppError::Transaction(format!(
            "finalization rollback record is unavailable; manual review is required: {error}"
        ))
    })?;
    if is_link_metadata(&record_metadata) || !record_metadata.is_file() {
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
    if sha256_file(&record_path)? != expected_record_hash {
        return Err(AppError::Transaction(
            "finalization rollback record checksum mismatch; manual review is required".into(),
        ));
    }
    let record = read_journal(&record_path)?;
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
        let destination = rollback_operation_destination(&project_root, operation)?;
        let current = regular_file_hash(&destination)?;
        let current_executable = if current.is_some() {
            observed_executable(&destination)?
        } else {
            None
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
    persist_journal(journal_path, journal)?;
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
    let (project_root, _) = validate_project_root_or_destination(project_root)?;
    if crate::security::path_has_link_component(app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    let roots = transaction_root(app_root, transaction_id);
    let journal_path = roots.transaction.join("journal.json");
    let mut journal = read_journal(&journal_path)?;
    if journal.transaction_id != transaction_id {
        return Err(AppError::Transaction(
            "transaction journal ID does not match the requested transaction".into(),
        ));
    }
    if journal.state == "finalizing" {
        return finish_finalization(
            &project_root,
            app_root,
            transaction_id,
            &mut journal,
            &journal_path,
        );
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
    validate_journal_project_root(&project_root, &journal, &journal_path)?;

    let plan_path = roots.transaction.join("plan.json");
    let plan_bytes = fs::read(&plan_path).map_err(|error| {
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
        let current_hash = regular_file_hash(&destination)?;
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
        let staged = staging_destination(&roots.staging, operation)?;
        if crate::security::path_has_link_component(&staged) {
            return Err(AppError::PathSecurity(format!(
                "staged path contains a symlink or junction: {}",
                operation.destination
            )));
        }
        let staged_metadata = fs::symlink_metadata(&staged).map_err(|error| {
            AppError::Transaction(format!(
                "staged bytes are missing for {}: {error}",
                operation.destination
            ))
        })?;
        if is_link_metadata(&staged_metadata) || !staged_metadata.is_file() {
            return Err(AppError::PathSecurity(format!(
                "staged destination is not a regular file: {}",
                operation.destination
            )));
        }
        let bytes = fs::read(&staged)?;
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
    let snapshot_path = roots
        .transaction
        .join(format!("journal.interrupted.{}.json", Uuid::new_v4()));
    let snapshot = journal.clone();
    atomic_write_json(&snapshot_path, &snapshot)?;
    run_transaction(
        &project_root,
        &plan,
        &prepared,
        &TransactionOptions {
            app_data_root: Some(app_root.to_path_buf()),
            resume_transaction_id: Some(transaction_id),
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
    let roots = transaction_root(app_root, transaction_id);
    let journal_path = roots.transaction.join("journal.json");
    let mut journal = read_journal(&journal_path)?;
    if journal.transaction_id != transaction_id {
        return Err(AppError::Transaction(
            "transaction journal ID does not match the requested transaction".into(),
        ));
    }
    let _project_root = validate_journal_project_root(project_root, &journal, &journal_path)?;
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
    if crate::security::path_has_link_component(&roots.staging) {
        return Err(AppError::PathSecurity(
            "staging directory contains a symlink or junction".into(),
        ));
    }
    match fs::symlink_metadata(&roots.staging) {
        Ok(metadata) if is_link_metadata(&metadata) => {
            return Err(AppError::PathSecurity(
                "refusing to remove a staging symlink".into(),
            ));
        }
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&roots.staging)?,
        Ok(_) => {
            return Err(AppError::PathSecurity(
                "staging path is not a directory".into(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
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
    persist_journal(&journal_path, &mut journal)?;
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
                if file.ownership == Ownership::Merged {
                    (local_state != LocalState::Unmodified).then(|| "reverse_merge_required".into())
                } else {
                    Some("review_required".into())
                }
            } else {
                None
            },
            external: file.external,
            rollback: if action == OperationAction::Create {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
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
        });
    }
    Ok(operations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readiness::manifest_wiki_pages;
    use std::process::Command;
    use tempfile::tempdir;

    fn test_codex_analysis() -> CodexAnalysisRecord {
        CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: None,
            optimization_profile: Some("Codex project and ChatGPT Chat".into()),
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
            ai_model: "default".into(),
            ai_endpoint: None,
            ai_optimization_profile: crate::models::default_ai_optimization_profile(),
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
    fn operation_checkpoint_replays_and_compacts_into_the_full_journal() {
        let root = tempdir().unwrap();
        let plan = plan();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&journal_path, &mut journal).unwrap();

        journal.operations[0].status = "staged".into();
        journal.operations[0].staged_sha256 = Some("a".repeat(64));
        journal.last_checkpoint = "stage-file-op-1".into();
        persist_operation_checkpoint(&journal_path, &mut journal, 0).unwrap();

        let replayed = read_journal(&journal_path).unwrap();
        assert_eq!(replayed.operations[0].status, "staged");
        assert_eq!(replayed.last_checkpoint, "stage-file-op-1");

        compact_operation_checkpoints(&journal_path, &mut journal).unwrap();
        assert!(!operation_checkpoint_root(&journal_path).unwrap().exists());
        assert_eq!(
            read_journal(&journal_path).unwrap().operations[0].status,
            "staged"
        );
    }

    #[test]
    fn profile_directories_are_created_without_markers_and_rollback_removes_only_empty_ones() {
        let project = tempdir().unwrap();
        let transaction = tempdir().unwrap();
        let mut plan = plan();
        plan.transaction.directories = vec!["events".into(), "localisation/english".into()];
        let journal_path = transaction.path().join("journal.json");
        let mut journal = new_journal(&plan, &plan.project_id, project.path());
        persist_journal(&journal_path, &mut journal).unwrap();

        apply_profile_directories(project.path(), &plan, &mut journal, &journal_path).unwrap();
        assert!(project.path().join("events").is_dir());
        assert!(project.path().join("localisation/english").is_dir());
        assert!(!project.path().join("events/.gitkeep").exists());
        fs::write(project.path().join("events/user_event.txt"), "user content").unwrap();

        cleanup_created_profile_directories(project.path(), &mut journal, &journal_path).unwrap();
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
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&journal_path, &mut journal).unwrap();

        journal.operations[0].status = "applying".into();
        journal.last_checkpoint = "apply-intent-op-0000".into();
        persist_operation_checkpoint(&journal_path, &mut journal, 0).unwrap();

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
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&journal_path, &mut journal).unwrap();
        let indices = (0..OPERATION_INTENT_BATCH).collect::<Vec<_>>();
        for index in &indices {
            journal.operations[*index].status = "applying".into();
        }
        persist_operation_checkpoint_batch(&journal_path, &mut journal, &indices).unwrap();

        let replayed = read_journal(&journal_path).unwrap();
        assert!(replayed
            .operations
            .iter()
            .all(|operation| operation.status == "applying"));
        compact_operation_checkpoints(&journal_path, &mut journal).unwrap();
        assert!(!operation_checkpoint_root(&journal_path).unwrap().exists());
    }

    #[test]
    fn operation_checkpoint_replay_ignores_only_a_torn_final_record() {
        let root = tempdir().unwrap();
        let plan = plan();
        let transaction_dir = root.path().join(plan.plan_id.to_string());
        fs::create_dir_all(&transaction_dir).unwrap();
        let journal_path = transaction_dir.join("journal.json");
        let mut journal = new_journal(&plan, &plan.project_id, root.path());
        persist_journal(&journal_path, &mut journal).unwrap();

        journal.operations[0].status = "verified".into();
        journal.last_checkpoint = "apply-op-1".into();
        persist_operation_checkpoint(&journal_path, &mut journal, 0).unwrap();
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
    fn absent_project_root_is_created_only_at_apply_and_removed_by_rollback() {
        let parent = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (project_root, plan, prepared) = absent_root_fixture(parent.path());
        assert!(!project_root.exists());

        let error = run_transaction(
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
        assert!(run_transaction(
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
        assert!(run_transaction(
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
        assert!(run_transaction(
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
        persist_journal(&journal_path, &mut journal).unwrap();
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
        persist_journal(&journal_path, &mut journal).unwrap();
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
    fn transaction_revalidates_prepared_bytes_before_backup() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let (plan, mut prepared) = existing_file_fixture(project.path());
        prepared[0].bytes = b"tampered after review".to_vec();

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

            let result = run_transaction(
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
    fn first_install_kept_thumbnail_is_locked_and_never_managed_removed() {
        let project = tempdir().unwrap();
        let app = tempdir().unwrap();
        let canonical_app = fs::canonicalize(app.path()).unwrap();
        let thumbnail = crate::descriptors::placeholder_thumbnail_png().unwrap();
        fs::write(project.path().join("thumbnail.png"), &thumbnail).unwrap();
        let mut plan = ready_plan(project.path());
        let launcher_path = canonical_app.join("example.mod");
        let canonical_project = validate_project_root(project.path()).unwrap();
        let launcher_bytes = format!(
            "name=\"Example\"\nversion=\"0.1.0\"\nsupported_version=\"1.17.*\"\npicture=\"thumbnail.png\"\npath=\"{}\"\n",
            canonical_project.display().to_string().replace('\\', "/")
        )
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
        });

        let (_, lock) = run_transaction(
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
        .unwrap();

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
        let (mut journal, _) = run_transaction(
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
        let (mut installation, _) = run_transaction(
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
            let _ = run_transaction(
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

        assert!(run_transaction(
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
                assert!(
                    !project_root.exists(),
                    "root remained after recovering {mode}"
                );
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
        let (_, mut installed_lock) = run_transaction(
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

        let (_, removal_lock) = run_transaction(
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
        let (_, lock) = run_transaction(
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
        let (_, lock) = run_transaction(
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
        let (mut journal, _) = run_transaction(
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
        let (mut installation, _) = run_transaction(
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
        let (mut installation, _) = run_transaction(
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
        });
        let prepared = vec![PreparedFile {
            operation_id: "op-1".into(),
            destination: "AGENTS.md".into(),
            bytes: b"safe".to_vec(),
            expected_sha256: sha256_bytes(b"safe"),
        }];
        let (mut journal, _) = run_transaction(
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
        let (mut journal, _) = run_transaction(
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
        let (_, first_lock) = run_transaction(
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
        let (_, second_lock) = run_transaction(
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
    fn non_codex_plans_cannot_retain_codex_configuration_or_chat_flattening() {
        let project = tempdir().unwrap();
        let mut plan = ready_plan(project.path());
        plan.ai_provider = "claude".into();
        plan.ai_model = "claude-model".into();
        plan.ai_optimization_profile = "Claude Code / Anthropic conventions".into();
        plan.selected_components.push("codex.config".into());
        let error = validate_plan(&plan).unwrap_err();
        assert!(error
            .to_string()
            .contains("cannot install Codex configuration"));

        plan.selected_components.retain(|id| id != "codex.config");
        plan.flatten_chat_sources = true;
        let error = validate_plan(&plan).unwrap_err();
        assert!(error.to_string().contains("require the Codex provider"));

        plan.flatten_chat_sources = false;
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
        let (mut journal, _) = run_transaction(
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
        let (_, _) = run_transaction(
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
        let (mut journal, _) = run_transaction(
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
        let error = run_transaction(
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
        let result = run_transaction(
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
        let result = run_transaction(
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
        assert!(run_transaction(
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
        let (_, lock) = run_transaction(
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
        run_transaction(
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
        let (mut journal, _) = run_transaction(
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
        run_transaction(
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
        assert!(run_transaction(
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
        assert!(run_transaction(
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
        assert!(run_transaction(
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
        let result = run_transaction(
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
            ai_model: "default".into(),
            ai_endpoint: None,
            ai_optimization_profile: crate::models::default_ai_optimization_profile(),
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
            ai_model: "default".into(),
            ai_endpoint: None,
            ai_optimization_profile: crate::models::default_ai_optimization_profile(),
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
                assert!(run_transaction(project.path(), &plan, &prepared, &options).is_err());
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
            assert!(run_transaction(project.path(), &plan, &prepared, &options).is_err());
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
        assert!(run_transaction(project.path(), &plan, &prepared, &options).is_err());
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

        let completed = run_transaction(
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
}
