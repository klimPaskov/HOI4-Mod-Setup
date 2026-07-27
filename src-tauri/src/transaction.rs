use crate::models::*;
use crate::paths::{application_data_root, transaction_root, validate_project_root};
use crate::security::{
    atomic_write_json, canonical_relative_key, is_link_metadata, normalize_relative_path,
    path_has_link_component, safe_join, sha256_bytes, sha256_file, validate_external_destination,
};
use crate::AppError;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct TransactionOptions {
    pub app_data_root: Option<PathBuf>,
    pub fail_before_stage: Option<usize>,
    pub fail_after_stage: Option<usize>,
    pub fail_before_operation: Option<usize>,
    pub fail_after_operation: Option<usize>,
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
    crate::source::validate_commit(&plan.source.resolved_revision)?;
    crate::source::validate_sha256(&plan.source.manifest_sha256)?;
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
            "a confirmed ChatGPT-authenticated Codex analysis is required before apply".into(),
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

pub fn new_journal(
    plan: &InstallationPlan,
    project_id: &str,
    project_root: &Path,
) -> TransactionJournal {
    let now = Utc::now().to_rfc3339();
    TransactionJournal {
        schema_version: "1.0.0".into(),
        transaction_id: plan.plan_id,
        project_id: project_id.into(),
        project_root: project_root.display().to_string(),
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
                action: Some(operation.action),
                location_scope: operation.location_scope.clone(),
                external: operation.external,
                backup_path: None,
                before_sha256: operation.local_sha256.clone(),
                expected_sha256: operation
                    .result_sha256
                    .clone()
                    .or_else(|| operation.source_sha256.clone()),
                source_sha256: operation.source_sha256.clone(),
                result_sha256: operation.result_sha256.clone(),
                rollback: Some(operation.rollback),
                backup_sha256: None,
                after_sha256: None,
                after_exists: None,
            })
            .collect(),
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
    let requested_root = validate_project_root(project_root)?;
    if journal.project_root.trim().is_empty() {
        return Err(AppError::PathSecurity(
            "transaction journal has no project-root binding".into(),
        ));
    }
    let bound_root = validate_project_root(Path::new(&journal.project_root))?;
    let roots_match = if cfg!(target_os = "windows") {
        bound_root
            .to_string_lossy()
            .eq_ignore_ascii_case(&requested_root.to_string_lossy())
    } else {
        bound_root == requested_root
    };
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

fn journal_app_root(journal_path: &Path) -> Result<PathBuf, AppError> {
    journal_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| AppError::PathSecurity("journal has no application root".into()))
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
    let project_root = validate_project_root(project_root)?;
    let app_root = options
        .app_data_root
        .clone()
        .unwrap_or_else(application_data_root);
    if crate::security::path_has_link_component(&app_root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    let previous_lock = read_existing_lock(&project_root)?;
    let roots = transaction_root(&app_root, plan.plan_id);
    fs::create_dir_all(&roots.transaction)?;
    fs::create_dir_all(&roots.backup)?;
    fs::create_dir_all(&roots.staging)?;
    for root in [&roots.transaction, &roots.backup, &roots.staging] {
        if path_has_link_component(root) {
            return Err(AppError::PathSecurity(format!(
                "transaction storage contains a symlink or junction: {}",
                root.display()
            )));
        }
    }
    let journal_path = roots.transaction.join("journal.json");
    let plan_path = roots.transaction.join("plan.json");
    atomic_write_json(&plan_path, plan)?;
    let mut journal = new_journal(plan, &plan.project_id, &project_root);
    persist_journal(&journal_path, &mut journal)?;

    let result: Result<InstallationLock, AppError> = (|| {
        // Bind the predecessor lock before any fault-injectable checkpoint.
        // This makes resume/rollback safe even when interruption happens
        // during preflight or source verification.
        capture_previous_lock(&project_root, &roots.backup, &mut journal, &journal_path)?;
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
        backup_existing(
            &project_root,
            plan,
            &roots.backup,
            &mut journal,
            &journal_path,
        )?;
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
        stage_files(
            plan,
            prepared_files,
            &roots.staging,
            &mut journal,
            &journal_path,
        )?;
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
        validate_staging(plan, prepared_files, &roots.staging)?;
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
        apply_operations(
            &project_root,
            plan,
            &roots.staging,
            &mut journal,
            &journal_path,
            options,
        )?;
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
        if let Some(setup) = &plan.git_setup {
            journal.git_initialized = setup.mode == crate::git::GitMode::Initialize;
            persist_journal(&journal_path, &mut journal)?;
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
            journal.git_initialized = git_result.initialized;
            if git_result.remote_configured && setup.mode == crate::git::GitMode::Preserve {
                journal.git_remote_added_name = setup.remote_name.clone();
                journal.git_remote_added_url = setup.remote_url.clone();
            }
            journal.last_checkpoint = "git-verified".into();
            persist_journal(&journal_path, &mut journal)?;
        }
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
        let lock = build_lock(
            plan,
            prepared_files,
            &journal,
            previous_lock.as_ref(),
            &project_root,
        )?;
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
        stage_complete(
            &mut journal,
            11,
            "rollback record",
            &journal_path,
            options.fail_after_stage,
        )?;
        let metadata_root = project_root.join(".hoi4-mod-setup");
        fs::create_dir_all(&metadata_root)?;
        // The lock is the final success artifact. Journal finalization after
        // this point is best-effort: a stale `finalizing` journal is safely
        // reconciled by resume only after the lock and rollback record verify.
        atomic_write_json(&metadata_root.join("install.lock.json"), &lock)?;
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
            journal.state = "interrupted".into();
            journal.updated_at = Utc::now().to_rfc3339();
            journal.recovery.recommended_action = if journal.recovery.project_apply_started {
                "rollback".into()
            } else {
                "resume".into()
            };
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
    for operation in &plan.operations {
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
            persist_journal(journal_path, journal)?;
        }
    }
    persist_journal(journal_path, journal)
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
    for operation in &plan.operations {
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
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "staged".into();
            record.after_sha256 = Some(hash);
        }
        journal.last_checkpoint = format!("stage-file-{}", operation.id);
        persist_journal(journal_path, journal)?;
    }
    Ok(())
}

fn validate_staging(
    plan: &InstallationPlan,
    prepared_files: &[PreparedFile],
    staging_root: &Path,
) -> Result<(), AppError> {
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
        let bytes = fs::read(&staged)?;
        validate_managed_bytes(&operation.destination, &bytes)?;
    }
    Ok(())
}

fn validate_managed_bytes(destination: &str, bytes: &[u8]) -> Result<(), AppError> {
    let lower = destination.to_ascii_lowercase();
    if lower == "descriptor.mod" {
        let descriptor = crate::descriptors::parse_descriptor(bytes).map_err(|error| {
            AppError::Transaction(format!("descriptor validation failed: {error}"))
        })?;
        if !descriptor.fields.contains_key("name") {
            return Err(AppError::Transaction(
                "descriptor validation failed: name is missing".into(),
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
        serde_json::from_slice::<serde_json::Value>(bytes).map_err(|error| {
            AppError::Transaction(format!("JSON validation failed for {destination}: {error}"))
        })?;
    } else if lower == "agents.md" {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| AppError::Transaction("AGENTS.md is not valid UTF-8".into()))?;
        if text.trim().is_empty() || text.contains("{{") {
            return Err(AppError::Transaction(
                "AGENTS.md contains no usable rendered instructions".into(),
            ));
        }
    } else if lower.starts_with("paradox_wiki/") {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            AppError::Transaction(format!("offline wiki page is not UTF-8: {destination}"))
        })?;
        if text.trim().is_empty() {
            return Err(AppError::Transaction(format!(
                "offline wiki page is empty: {destination}"
            )));
        }
    }
    Ok(())
}

fn apply_operations(
    project_root: &Path,
    plan: &InstallationPlan,
    staging_root: &Path,
    journal: &mut TransactionJournal,
    journal_path: &Path,
    options: &TransactionOptions,
) -> Result<(), AppError> {
    journal.recovery.project_apply_started = true;
    persist_journal(journal_path, journal)?;
    for (index, operation) in plan.operations.iter().enumerate() {
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
            persist_journal(journal_path, journal)?;
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
        if operation.action != OperationAction::DeleteManaged {
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
        }
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "applying".into();
            record.after_exists = None;
        }
        journal.last_checkpoint = format!("apply-intent-{}", operation.id);
        persist_journal(journal_path, journal)?;
        match operation.action {
            OperationAction::DeleteManaged => {
                if current_hash.is_some() {
                    fs::remove_file(&destination)?;
                }
            }
            _ => copy_atomic(&staged, &destination)?,
        }
        let after_hash = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        if operation.action != OperationAction::DeleteManaged && after_hash.is_none() {
            return Err(AppError::Transaction(format!(
                "destination missing after apply: {}",
                operation.destination
            )));
        }
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "applied".into();
            record.after_sha256 = after_hash;
            record.after_exists = Some(destination.is_file());
        }
        journal.last_checkpoint = format!("apply-{}", operation.id);
        persist_journal(journal_path, journal)?;
        if options.fail_after_operation == Some(index) {
            return Err(AppError::Transaction(format!(
                "fault injected after operation {}",
                operation.id
            )));
        }
        if let Some(record) = journal
            .operations
            .iter_mut()
            .find(|record| record.id == operation.id)
        {
            record.status = "verified".into();
        }
        persist_journal(journal_path, journal)?;
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
            validate_managed_bytes(&operation.destination, &bytes)?;
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

fn build_transaction_readiness(
    project_root: &Path,
    plan: &InstallationPlan,
    journal: &TransactionJournal,
) -> Result<crate::models::ReadinessReport, AppError> {
    let removing = plan
        .operations
        .iter()
        .any(|operation| operation.action == OperationAction::DeleteManaged)
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
    let wiki_metadata_valid =
        !has_component("wiki.snapshot") || !plan.wiki_required_pages.is_empty();
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
        mcp_status,
        mcp_blocking: has_component("mcp.hoi4_agent_tools") && cfg!(target_os = "windows"),
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
        lora_interest: plan
            .optional_workflows
            .get("workflow.lora_comfyui_interest")
            .is_some_and(|state| state == "planned_unavailable"),
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
    let removing = plan
        .operations
        .iter()
        .any(|operation| operation.action == OperationAction::DeleteManaged)
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
                if let Some(existing) = files.get(&key) {
                    if operation.local_state == LocalState::Modified {
                        record_local_modification(
                            &mut local_modifications,
                            operation,
                            &existing.installed_sha256,
                        );
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
                    let installed_sha256 = if operation.local_state == LocalState::Modified {
                        operation
                            .result_sha256
                            .as_deref()
                            .or(operation.source_sha256.as_deref())
                            .unwrap_or(&current_sha256)
                            .to_string()
                    } else {
                        current_sha256.clone()
                    };
                    if operation.local_state == LocalState::Modified {
                        if let Some(expected) = operation
                            .result_sha256
                            .as_deref()
                            .or(operation.source_sha256.as_deref())
                        {
                            record_local_modification(
                                &mut local_modifications,
                                operation,
                                expected,
                            );
                        }
                    }
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
                            installed_size: Some(
                                if operation.local_state == LocalState::Modified {
                                    prepared
                                        .get(operation.id.as_str())
                                        .map(|file| file.bytes.len() as u64)
                                        .or(operation.source_size)
                                        .unwrap_or(bytes.len() as u64)
                                } else {
                                    bytes.len() as u64
                                },
                            ),
                            ownership,
                            external: operation.external,
                            generated_content: None,
                            generated_bytes: None,
                            executable: false,
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
                    external: operation.external,
                    generated_content: if operation.action == OperationAction::Generate {
                        String::from_utf8(prepared.bytes.clone()).ok()
                    } else {
                        None
                    },
                    generated_bytes: (operation.action == OperationAction::Generate)
                        .then_some(prepared.bytes.clone()),
                    executable: false,
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
            validation: Some("pass".into()),
        })
        .collect();
    let mut optional_workflows = previous_lock
        .map(|lock| lock.optional_workflows.clone())
        .unwrap_or_default();
    for (id, state) in &plan.optional_workflows {
        optional_workflows.insert(
            id.clone(),
            OptionalWorkflowLock {
                state: state.clone(),
                reason: if state == "planned_unavailable" {
                    Some("Automated setup is not implemented in version 1.".into())
                } else {
                    None
                },
                credential_reference: plan
                    .credential_references
                    .iter()
                    .find(|reference| reference.name == crate::credentials::MESHY_ENVIRONMENT_NAME)
                    .map(|reference| reference.reference.clone())
                    .or_else(|| {
                        previous_lock.and_then(|lock| {
                            lock.optional_workflows
                                .get(id)
                                .and_then(|workflow| workflow.credential_reference.clone())
                        })
                    }),
            },
        );
    }
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
        codex_analysis: plan.codex_analysis.clone(),
        wiki_required_pages: plan.wiki_required_pages.clone(),
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
        "interest_recorded" => "planned_unavailable".into(),
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
                return Ok(current.is_none());
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
            Ok(current.as_deref() == Some(expected.as_str()))
        }
    }
}

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
    let project_apply_started = journal.recovery.project_apply_started;
    journal.state = "rolling_back".into();
    journal.recovery = RecoveryState {
        resume_allowed: false,
        rollback_allowed: true,
        discard_staging_allowed: false,
        project_apply_started,
        recommended_action: "rollback".into(),
    };
    persist_journal(journal_path, journal)?;
    // Remove transaction-created Git metadata before restoring files. The
    // newly initialized index would otherwise record the transaction's
    // applied files as user changes and make safe Git cleanup impossible.
    if journal.git_initialized {
        crate::git::rollback_initialized_git(&project_root)?;
        journal.git_initialized = false;
        persist_journal(journal_path, journal)?;
    } else if let (Some(name), Some(url)) = (
        journal.git_remote_added_name.as_deref(),
        journal.git_remote_added_url.as_deref(),
    ) {
        crate::git::rollback_added_remote(&project_root, name, url)?;
        journal.git_remote_added_name = None;
        journal.git_remote_added_url = None;
        persist_journal(journal_path, journal)?;
    }
    for index in (0..journal.operations.len()).rev() {
        let operation = journal.operations[index].clone();
        if !matches!(
            operation.status.as_str(),
            "rollback_applying" | "applying" | "applied" | "verified"
        ) {
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
            persist_journal(journal_path, journal)?;
            continue;
        }
        let destination = if operation.external {
            validate_external_destination(&operation.destination)?
        } else {
            safe_join(&project_root, &operation.destination)?
        };
        if operation.status == "rollback_applying"
            && rollback_destination_is_restored(&operation, &destination, journal, journal_path)?
        {
            journal.operations[index].status = "rolled_back".into();
            journal.last_checkpoint = format!("rollback-{}", operation.id);
            persist_journal(journal_path, journal)?;
            continue;
        }
        journal.operations[index].status = "rollback_applying".into();
        journal.last_checkpoint = format!("rollback-intent-{}", operation.id);
        persist_journal(journal_path, journal)?;
        let _ = regular_file_hash(&destination)?;
        if let Some(after) = &operation.after_sha256 {
            if destination.is_file() && sha256_file(&destination)? != *after {
                return Err(AppError::Transaction(format!(
                    "user changes detected after apply; refusing rollback of {}",
                    operation.destination
                )));
            }
        } else if operation.after_exists == Some(false) && destination.is_file() {
            return Err(AppError::Transaction(format!(
                "user created a file after managed deletion; refusing rollback of {}",
                operation.destination
            )));
        } else if operation.status == "applying" && destination.is_file() {
            let current = sha256_file(&destination)?;
            if operation.before_sha256.as_deref() != Some(current.as_str())
                && operation.expected_sha256.as_deref() != Some(current.as_str())
            {
                return Err(AppError::Transaction(format!(
                    "uncertain live state after interruption; refusing rollback of {}",
                    operation.destination
                )));
            }
        }
        if let Some(backup) = &operation.backup_path {
            let expected_backup = expected_operation_backup(journal, journal_path, &operation.id)?;
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
            } else {
                return Err(AppError::Transaction(format!(
                    "rollback backup is missing for {}",
                    operation.destination
                )));
            }
        } else if destination.is_file() {
            fs::remove_file(&destination)?;
        }
        journal.operations[index].status = "rolled_back".into();
        journal.last_checkpoint = format!("rollback-{}", operation.id);
        persist_journal(journal_path, journal)?;
    }
    restore_previous_lock(&project_root, journal, journal_path)?;
    journal.state = "rolled_back".into();
    journal.recovery = RecoveryState {
        resume_allowed: false,
        rollback_allowed: false,
        discard_staging_allowed: true,
        project_apply_started: false,
        recommended_action: "none".into(),
    };
    persist_journal(journal_path, journal)?;
    atomic_write_json(
        &journal_path
            .parent()
            .ok_or_else(|| AppError::Transaction("journal has no transaction directory".into()))?
            .join("rollback-record.json"),
        journal,
    )
}

fn restore_previous_lock(
    project_root: &Path,
    journal: &TransactionJournal,
    journal_path: &Path,
) -> Result<(), AppError> {
    let lock_path = safe_join(project_root, ".hoi4-mod-setup/install.lock.json")?;
    let record_suffix = format!("{}/rollback-record.json", journal.transaction_id);
    let current_has_record = if lock_path.is_file() {
        let bytes = fs::read(&lock_path)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::Transaction(format!(
                "refusing to change an unreadable installation lock during rollback: {error}"
            ))
        })?;
        let lock = crate::migrations::migrate_lock(value)?;
        lock.rollback_records
            .iter()
            .any(|record| record.ends_with(&record_suffix))
    } else {
        false
    };
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
        if current_has_record {
            copy_atomic(&backup_path, &lock_path)?;
            if sha256_file(&lock_path)? != expected_hash {
                return Err(AppError::Transaction(
                    "restored installation lock checksum mismatch".into(),
                ));
            }
        } else if lock_path.is_file() && sha256_file(&lock_path)? != expected_hash {
            return Err(AppError::Transaction(
                "installation lock changed outside the transaction; refusing rollback".into(),
            ));
        } else if !lock_path.is_file() {
            copy_atomic(&backup_path, &lock_path)?;
        }
    } else if current_has_record {
        let metadata = fs::symlink_metadata(&lock_path)?;
        if is_link_metadata(&metadata) {
            return Err(AppError::PathSecurity(
                "refusing to remove an installation lock symlink during rollback".into(),
            ));
        }
        fs::remove_file(lock_path)?;
    }
    Ok(())
}

pub fn read_journal(path: &Path) -> Result<TransactionJournal, AppError> {
    let bytes = fs::read(path).map_err(|error| AppError::Transaction(error.to_string()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Transaction(format!("invalid transaction journal: {error}")))?;
    crate::migrations::migrate_journal(value)
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
    let lock_value: serde_json::Value = serde_json::from_slice(&lock_bytes)?;
    let lock = crate::migrations::migrate_lock(lock_value)?;
    let record_suffix = format!("{transaction_id}/rollback-record.json");
    if !lock
        .rollback_records
        .iter()
        .any(|record| record.ends_with(&record_suffix))
    {
        return Err(AppError::Transaction(
            "finalization lock has no verified rollback record; manual review is required".into(),
        ));
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
    let project_root = validate_project_root(project_root)?;
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
    if journal.state != "interrupted" || !journal.recovery.resume_allowed {
        return Err(AppError::Transaction(
            "transaction is not in a resumable interrupted state".into(),
        ));
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
    let journal_root = fs::canonicalize(&journal.project_root).map_err(|error| {
        AppError::PathSecurity(format!("journal project root is not accessible: {error}"))
    })?;
    let roots_match = if cfg!(target_os = "windows") {
        journal_root
            .to_string_lossy()
            .eq_ignore_ascii_case(&project_root.to_string_lossy())
    } else {
        journal_root == project_root
    };
    if !roots_match {
        return Err(AppError::PathSecurity(
            "requested project root does not match the journal binding".into(),
        ));
    }

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
    if lock_path.is_file() {
        let lock_hash = sha256_file(&lock_path)?;
        if journal.previous_lock_sha256.as_deref() != Some(lock_hash.as_str()) {
            return Err(AppError::Transaction(
                "installation lock changed after interruption; refusing to replay the transaction"
                    .into(),
            ));
        }
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

        if matches!(
            operation.action,
            OperationAction::Skip | OperationAction::External
        ) {
            continue;
        }

        let destination = operation_destination(&project_root, operation)?;
        let current_hash = match fs::symlink_metadata(&destination) {
            Ok(metadata) if is_link_metadata(&metadata) => {
                return Err(AppError::PathSecurity(format!(
                    "refusing to resume through a destination link: {}",
                    operation.destination
                )))
            }
            Ok(metadata) if metadata.is_file() => Some(sha256_file(&destination)?),
            Ok(_) => {
                return Err(AppError::Transaction(format!(
                    "destination is not a regular file: {}",
                    operation.destination
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(expected_local) = &operation.local_sha256 {
            if current_hash.as_deref() != Some(expected_local.as_str()) {
                return Err(AppError::Transaction(format!(
                    "live precondition changed before resume: {}",
                    operation.destination
                )));
            }
        } else if current_hash.is_some() {
            return Err(AppError::Transaction(format!(
                "live destination changed before resume and has no hash precondition: {}",
                operation.destination
            )));
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
    if journal.state != "interrupted" || !journal.recovery.discard_staging_allowed {
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
    if matches!(
        journal.state.as_str(),
        "interrupted" | "rolling_back" | "finalizing"
    ) {
        journal.recovery.recommended_action.clone()
    } else {
        "none".into()
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
        let (action, local_state) = if file.ownership == Ownership::External {
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
            result_sha256: None,
            base_sha256: file.base_sha256.clone(),
            local_sha256: current,
            local_state,
            resolution: if file.ownership == Ownership::External {
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
        let reversible = !matches!(file.ownership, Ownership::Merged | Ownership::External);
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
            let user_owned = file.ownership == Ownership::External;
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
            let previous = current_lock
                .files
                .iter()
                .find(|current| current.path == file.path && current.external == file.external)
                .map(|current| current.installed_sha256.as_str());
            let local_state = match (local_sha256.as_deref(), previous) {
                (None, _) => LocalState::Absent,
                (Some(hash), Some(previous)) if hash == previous => LocalState::Unmodified,
                (Some(hash), _) if hash == file.installed_sha256 => LocalState::Unmodified,
                (Some(_), _) => LocalState::Modified,
            };
            let action = match local_state {
                LocalState::Absent => OperationAction::Create,
                LocalState::Unmodified => OperationAction::Replace,
                LocalState::Modified | LocalState::Unknown => OperationAction::Skip,
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
                result_sha256: None,
                base_sha256: previous.map(ToOwned::to_owned),
                local_sha256,
                local_state,
                resolution: (action == OperationAction::Skip).then(|| "review_required".into()),
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
        let removable = !matches!(file.ownership, Ownership::Merged | Ownership::External);
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
    use tempfile::tempdir;

    fn test_codex_analysis() -> CodexAnalysisRecord {
        CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
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
            codex_analysis: Some(test_codex_analysis()),
            selected_components: vec!["core.agents".into()],
            wiki_required_pages: manifest_wiki_pages(),
            generated_artifacts: vec![],
            git_setup: None,
            credential_references: vec![],
            optional_workflows: Default::default(),
            operations: vec![PlanOperation {
                id: "op-1".into(),
                component_id: "core.agents".into(),
                ownership: Some(Ownership::Managed),
                location_scope: None,
                action: OperationAction::Create,
                source_path: Some("AGENTS_template.md".into()),
                destination: "AGENTS.md".into(),
                source_sha256: Some(sha256_bytes(b"safe")),
                source_size: Some(4),
                platform: Some(ManifestPlatform::All),
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
                atomic_apply_expected: true,
            },
            approvals: PlanApprovals {
                dry_run_reviewed: true,
                external_actions_reviewed: true,
                git_remote_approved: false,
                push_approved: false,
            },
        }
    }

    fn ready_plan(project_root: &Path) -> InstallationPlan {
        let mut plan = plan();
        plan.selected_components = vec!["core.agents".into(), "wiki.snapshot".into()];
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
        assert_eq!(lock.local_modifications.len(), 1);
        assert_eq!(lock.local_modifications[0].path, "AGENTS.md");
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
            codex_analysis: None,
            wiki_required_pages: manifest_wiki_pages(),
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
            codex_analysis: None,
            wiki_required_pages: manifest_wiki_pages(),
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
    }
}
