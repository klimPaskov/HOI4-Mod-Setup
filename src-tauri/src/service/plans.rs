//! Prepared installation plans and the conflict decisions made on them.
//!
//! These types and functions hold no process-global state. The desktop
//! commands keep their plans in one process-wide map and the service facade
//! keeps them per session, but both call the same functions here, so a
//! conflict decision means the same thing on either path.

use crate::merge::{
    structured_json_merge, structured_toml_merge, three_way_merge, validate_merged_result, FileKind,
};
use crate::models::*;
use crate::security::{safe_join, sha256_bytes, sha256_file, validate_external_destination};
use crate::AppError;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) struct PreparedPlan {
    pub(crate) plan: InstallationPlan,
    pub(crate) prepared_files: Vec<crate::models::PreparedFile>,
    pub(crate) merge_contexts: HashMap<String, MergeContext>,
    pub(crate) canonical_root: PathBuf,
    pub(crate) approved: bool,
}

/// The three versions of a file that a merge decision combines.
#[derive(Clone)]
pub struct MergeContext {
    pub kind: FileKind,
    pub base: Vec<u8>,
    pub local: Vec<u8>,
    pub incoming: Vec<u8>,
}

pub(crate) fn merge_bytes(context: &MergeContext, path: &str) -> Result<Vec<u8>, AppError> {
    let base = std::str::from_utf8(&context.base)
        .map_err(|error| AppError::Merge(format!("base is not UTF-8: {error}")))?;
    let local = std::str::from_utf8(&context.local)
        .map_err(|error| AppError::Merge(format!("local is not UTF-8: {error}")))?;
    let incoming = std::str::from_utf8(&context.incoming)
        .map_err(|error| AppError::Merge(format!("incoming is not UTF-8: {error}")))?;
    let merged = match context.kind {
        FileKind::Text => three_way_merge(base, local, incoming)?,
        FileKind::Toml => structured_toml_merge(base, local, incoming)?,
        FileKind::Json => structured_json_merge(base, local, incoming)?,
        FileKind::Binary | FileKind::Symlink | FileKind::Directory => {
            return Err(AppError::Merge(format!(
                "text merge is not valid for {path}"
            )))
        }
    };
    let bytes = merged.into_bytes();
    validate_merged_result(context.kind, path, &bytes)?;
    Ok(bytes)
}

/// What an update does with a project instruction file whose installed bytes
/// are an adaptation of the source (ownership "merged").
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MergedUpdate {
    /// The file holds exactly what the old source adapted to: take the new one.
    Replace,
    /// The file differs from the old adaptation only where the new source is
    /// unchanged, so both sets of changes apply without a decision.
    Merged(Vec<u8>),
    /// The user edited the file, or the changes overlap: show it for review.
    NeedsReview,
}

/// Decide an update of a merged instruction file from three byte strings:
/// `base` is the old source adapted for this project (rebuilt from the exact
/// revision the lock recorded), `local` is the file now, `incoming` is the new
/// source adapted the same way. A file the user has edited since the last
/// install (`local_modified`) is never changed without review.
pub(crate) fn reconcile_merged_update(
    base: &[u8],
    local: &[u8],
    incoming: &[u8],
    local_modified: bool,
) -> MergedUpdate {
    if local == base {
        return MergedUpdate::Replace;
    }
    if local_modified {
        return MergedUpdate::NeedsReview;
    }
    let (Ok(base), Ok(local), Ok(incoming)) = (
        std::str::from_utf8(base),
        std::str::from_utf8(local),
        std::str::from_utf8(incoming),
    ) else {
        return MergedUpdate::NeedsReview;
    };
    match three_way_merge(base, local, incoming) {
        Ok(merged) => MergedUpdate::Merged(merged.into_bytes()),
        Err(_) => MergedUpdate::NeedsReview,
    }
}

/// The descriptor, thumbnail and README an existing mod already has are the
/// mod's own files, not setup output. A valid one is kept by default so the
/// first import of a real mod does not ask the user to compare their own
/// descriptor with a generated one. An invalid file, or a thumbnail the user
/// chose in the app, still goes through the normal conflict review.
pub(crate) fn existing_project_file_is_kept_by_default(
    component_id: &str,
    path: &Path,
    thumbnail_chosen_by_user: bool,
) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    match component_id {
        "project.descriptor" => crate::descriptors::parse_descriptor(&bytes)
            .ok()
            .is_some_and(|descriptor| {
                descriptor
                    .fields
                    .get("name")
                    .is_some_and(|name| !name.trim().is_empty())
            }),
        "project.thumbnail" => {
            !thumbnail_chosen_by_user && crate::descriptors::validate_thumbnail_png(&bytes).is_ok()
        }
        "project.readme" => !bytes.is_empty(),
        _ => false,
    }
}

pub(crate) fn renamed_destination(
    root: &Path,
    original: &str,
    external: bool,
) -> Result<String, AppError> {
    let original_path = if external {
        validate_external_destination(original)?
    } else {
        safe_join(root, original)?
    };
    let parent = original_path
        .parent()
        .ok_or_else(|| AppError::Transaction("cannot create a renamed destination".into()))?;
    let stem = original_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::Transaction("destination filename is not Unicode".into()))?;
    let extension = original_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 0..1000 {
        let suffix = if index == 0 {
            ".incoming".to_string()
        } else {
            format!(".incoming-{index}")
        };
        let candidate = parent.join(format!("{stem}{suffix}{extension}"));
        if !candidate.exists() {
            return if external {
                Ok(
                    validate_external_destination(&candidate.display().to_string())?
                        .display()
                        .to_string(),
                )
            } else {
                let relative = candidate.strip_prefix(root).map_err(|_| {
                    AppError::PathSecurity("renamed destination escaped root".into())
                })?;
                Ok(relative.to_string_lossy().replace('\\', "/"))
            };
        }
    }
    Err(AppError::Transaction(
        "could not find an unused renamed destination".into(),
    ))
}

pub(crate) fn is_flattened_destination(path: &str) -> bool {
    path.replace('\\', "/")
        .starts_with(&format!("{}/", crate::flatten::FLAT_DESTINATION_ROOT))
}

pub(crate) fn is_flattened_source_path(path: Option<&str>) -> bool {
    path.is_some_and(|value| {
        value.replace('\\', "/").starts_with(&format!(
            "generated:{}/",
            crate::flatten::FLAT_DESTINATION_ROOT
        ))
    })
}

pub(crate) fn flatten_operation_uses_incoming(operation: &PlanOperation) -> bool {
    if matches!(
        operation.resolution.as_deref(),
        Some(
            "review_required"
                | "reverse_merge_required"
                | "user_owned_review"
                | "keep_user_modification"
                | "keep_reviewed_edit"
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

pub(crate) fn accepted_flatten_prepared_files(
    prepared_files: &[PreparedFile],
    operations: &[PlanOperation],
    project_root: &Path,
) -> Result<Vec<PreparedFile>, AppError> {
    let mut accepted = Vec::new();
    for file in prepared_files {
        let operation = operations
            .iter()
            .find(|operation| operation.id == file.operation_id);
        if operation.is_none_or(flatten_operation_uses_incoming) {
            accepted.push(file.clone());
            continue;
        }
        let Some(operation) = operation else {
            continue;
        };
        let kept_local = matches!(
            operation.resolution.as_deref(),
            Some("keep" | "keep_user_modification" | "keep_reviewed_edit")
        );
        let normalized_destination = file.destination.replace('\\', "/");
        let eligible = crate::flatten::FLATTEN_SOURCE_PREFIXES
            .iter()
            .any(|prefix| normalized_destination.starts_with(prefix));
        if kept_local && eligible {
            let bytes = crate::flatten::read_regular_file_no_follow_under_root(
                project_root,
                &file.destination,
            )?;
            accepted.push(PreparedFile {
                operation_id: file.operation_id.clone(),
                destination: file.destination.clone(),
                expected_sha256: sha256_bytes(&bytes),
                bytes,
            });
        }
    }
    Ok(accepted)
}

#[derive(Clone)]
struct FlattenedDecision {
    source_sha256: Option<String>,
    action: OperationAction,
    resolution: String,
    local_sha256: Option<String>,
    local_state: LocalState,
    destination: String,
}

pub(crate) fn refresh_flattened_outputs(prepared_plan: &mut PreparedPlan) -> Result<(), AppError> {
    if !prepared_plan.plan.flatten_chat_sources {
        return Ok(());
    }
    let previous_decisions = prepared_plan
        .plan
        .operations
        .iter()
        .filter(|operation| {
            is_flattened_destination(&operation.destination)
                || is_flattened_source_path(operation.source_path.as_deref())
        })
        .filter_map(|operation| {
            operation.resolution.clone().map(|resolution| {
                (
                    operation.source_path.clone().unwrap_or_else(|| {
                        format!("generated:{}", operation.destination.replace('\\', "/"))
                    }),
                    FlattenedDecision {
                        source_sha256: operation.source_sha256.clone(),
                        action: operation.action,
                        resolution,
                        local_sha256: operation.local_sha256.clone(),
                        local_state: operation.local_state,
                        destination: operation.destination.clone(),
                    },
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    prepared_plan
        .plan
        .generated_artifacts
        .retain(|artifact| !is_flattened_destination(&artifact.destination));
    prepared_plan
        .plan
        .operations
        .retain(|operation| !is_flattened_destination(&operation.destination));
    prepared_plan
        .plan
        .conflicts
        .retain(|conflict| !is_flattened_destination(&conflict.path));
    prepared_plan
        .prepared_files
        .retain(|file| !is_flattened_destination(&file.destination));

    let flatten_prepared = accepted_flatten_prepared_files(
        &prepared_plan.prepared_files,
        &prepared_plan.plan.operations,
        &prepared_plan.canonical_root,
    )?;
    let accepted_generated = prepared_plan
        .plan
        .generated_artifacts
        .iter()
        .filter(|artifact| {
            let source_path = format!("generated:{}", artifact.destination);
            prepared_plan
                .plan
                .operations
                .iter()
                .find(|operation| operation.source_path.as_deref() == Some(source_path.as_str()))
                .is_none_or(|operation| {
                    operation.destination == artifact.destination
                        && flatten_operation_uses_incoming(operation)
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    let flattened = crate::flatten::build_artifacts(
        &flatten_prepared,
        &accepted_generated,
        &prepared_plan.canonical_root,
    )?;
    let operation_offset = prepared_plan.plan.operations.len();
    for (index, artifact) in flattened.iter().enumerate() {
        let destination_path = safe_join(&prepared_plan.canonical_root, &artifact.destination)?;
        let local_sha = if destination_path.is_file() {
            Some(sha256_file(&destination_path)?)
        } else {
            None
        };
        let local_state = match local_sha.as_deref() {
            None => LocalState::Absent,
            Some(hash) if hash == artifact.expected_sha256 => LocalState::Unmodified,
            Some(_) => LocalState::Modified,
        };
        let action = if local_state == LocalState::Absent {
            OperationAction::Generate
        } else {
            OperationAction::Skip
        };
        let operation_id = format!("generated-flat-{operation_offset:05}-{index:05}");
        if local_state == LocalState::Modified {
            prepared_plan.plan.conflicts.push(PlanConflict {
                id: format!("conflict.{operation_id}"),
                path: artifact.destination.clone(),
                options: vec![
                    "keep".into(),
                    "replace".into(),
                    "rename".into(),
                    "skip".into(),
                ],
                selected: None,
                apply_to_identical: false,
            });
        }
        prepared_plan
            .plan
            .generated_artifacts
            .push(artifact.clone());
        prepared_plan.plan.operations.push(PlanOperation {
            id: operation_id.clone(),
            component_id: artifact.component_id.clone(),
            ownership: Some(Ownership::Generated),
            location_scope: Some("project".into()),
            action,
            source_path: Some(format!("generated:{}", artifact.destination)),
            destination: artifact.destination.clone(),
            source_sha256: Some(artifact.expected_sha256.clone()),
            source_size: Some(
                artifact
                    .bytes
                    .as_ref()
                    .map_or(artifact.content.len(), Vec::len) as u64,
            ),
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: (action != OperationAction::Skip)
                .then_some(artifact.expected_sha256.clone()),
            base_sha256: None,
            local_sha256: local_sha,
            local_state,
            resolution: None,
            external: false,
            rollback: if local_state == LocalState::Absent {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
            external_parent_identity: None,
        });
        prepared_plan.prepared_files.push(PreparedFile {
            operation_id,
            destination: artifact.destination.clone(),
            bytes: artifact
                .bytes
                .clone()
                .unwrap_or_else(|| artifact.content.as_bytes().to_vec()),
            expected_sha256: artifact.expected_sha256.clone(),
        });
    }

    for operation in prepared_plan
        .plan
        .operations
        .iter_mut()
        .filter(|operation| is_flattened_destination(&operation.destination))
    {
        let Some(source_path) = operation.source_path.as_deref() else {
            continue;
        };
        let Some(previous) = previous_decisions.get(source_path) else {
            continue;
        };
        if previous.source_sha256.as_deref() != operation.source_sha256.as_deref()
            || previous.resolution == "merge"
        {
            continue;
        }

        let same_local_state = previous.local_state == operation.local_state
            && previous.local_sha256.as_deref() == operation.local_sha256.as_deref();
        let preserve_rename = previous.resolution == "rename"
            && is_flattened_destination(&previous.destination)
            && previous.destination != operation.destination
            && safe_join(&prepared_plan.canonical_root, &previous.destination)
                .is_ok_and(|path| !path.exists());
        if previous.resolution != "rename" && !same_local_state {
            continue;
        }
        if previous.resolution == "rename" && !preserve_rename {
            continue;
        }

        operation.resolution = Some(previous.resolution.clone());
        match previous.resolution.as_str() {
            "keep" | "skip" => {
                operation.action = OperationAction::Skip;
                operation.result_sha256 = None;
                prepared_plan
                    .prepared_files
                    .retain(|file| file.operation_id != operation.id);
            }
            "replace" => {
                operation.action = previous.action;
                operation.result_sha256 = operation.source_sha256.clone();
            }
            "rename" if preserve_rename => {
                let destination = previous.destination.clone();
                operation.action = OperationAction::Rename;
                operation.destination = destination.clone();
                operation.local_sha256 = None;
                operation.local_state = LocalState::Absent;
                operation.result_sha256 = operation.source_sha256.clone();
                operation.rollback = RollbackAction::RemoveCreated;
                if let Some(file) = prepared_plan
                    .prepared_files
                    .iter_mut()
                    .find(|file| file.operation_id == operation.id)
                {
                    file.destination = destination;
                }
            }
            _ => continue,
        }
        if let Some(conflict) = prepared_plan
            .plan
            .conflicts
            .iter_mut()
            .find(|conflict| conflict.path == source_path.trim_start_matches("generated:"))
        {
            conflict.selected = Some(previous.resolution.clone());
        }
    }
    Ok(())
}

/// Store a plan the builder returned, after deriving the flattened view from
/// the currently accepted source set. An unresolved modified AGENTS, skill or
/// subagent file must never leak incoming bytes into the Chat export merely
/// because the reviewer has not opened the conflict screen yet.
pub(crate) fn store_prepared_plan_in(
    plans: &mut HashMap<Uuid, PreparedPlan>,
    plan: InstallationPlan,
    prepared_files: Vec<crate::models::PreparedFile>,
    merge_contexts: HashMap<String, MergeContext>,
    canonical_root: PathBuf,
) -> Result<InstallationPlan, AppError> {
    let id = plan.plan_id;
    let mut prepared_plan = PreparedPlan {
        plan,
        prepared_files,
        merge_contexts,
        canonical_root,
        approved: false,
    };
    refresh_flattened_outputs(&mut prepared_plan)?;
    let plan = prepared_plan.plan.clone();
    plans.insert(id, prepared_plan);
    Ok(plan)
}

/// Mark the reviewed plan approved. The final dry-run approval is also the
/// explicit approval for a configured local remote. It does not approve
/// repository creation or pushing: those actions are not part of the
/// transaction and remain separately gated by the Git plan's `push_approved`.
pub(crate) fn approve_prepared_plan(prepared: &mut PreparedPlan) {
    prepared.approved = true;
    prepared.plan.approvals.dry_run_reviewed = true;
    prepared.plan.approvals.external_actions_reviewed = true;
    prepared.plan.approvals.git_remote_approved = true;
}

/// Apply one conflict decision to a prepared plan and return the updated
/// plan. The decision is validated against the options the plan offered.
pub(crate) fn resolve_conflict(
    prepared: &mut PreparedPlan,
    path: &str,
    choice: &str,
) -> Result<InstallationPlan, String> {
    let path = path.to_string();
    let choice = choice.to_string();
    let conflict_index = prepared
        .plan
        .conflicts
        .iter()
        .position(|conflict| conflict.path == path)
        .ok_or_else(|| "installation conflict is not present in the core plan".to_string())?;
    {
        let conflict = &mut prepared.plan.conflicts[conflict_index];
        if !conflict.options.iter().any(|option| option == &choice) {
            return Err("invalid installation conflict choice".into());
        }
        conflict.selected = Some(choice.clone());
    }
    let operation_index = prepared
        .plan
        .operations
        .iter()
        .position(|operation| operation.destination == path)
        .ok_or_else(|| "installation conflict has no bound operation".to_string())?;
    let operation_id = prepared.plan.operations[operation_index].id.clone();
    match choice.as_str() {
        "keep" | "skip" => {
            let operation = &mut prepared.plan.operations[operation_index];
            operation.resolution = Some(choice.clone());
            operation.action = OperationAction::Skip;
            operation.result_sha256 = None;
            prepared
                .prepared_files
                .retain(|file| file.operation_id != operation_id);
        }
        "merge" => {
            let merged = prepared
                .merge_contexts
                .get(&operation_id)
                .ok_or_else(|| "a verified merge base is unavailable".to_string())
                .and_then(|context| {
                    merge_bytes(context, &path).map_err(|error| error.to_string())
                })?;
            let result_sha256 = sha256_bytes(&merged);
            let destination = {
                let operation = &mut prepared.plan.operations[operation_index];
                operation.resolution = Some(choice.clone());
                operation.action = OperationAction::Merge;
                operation.result_sha256 = Some(result_sha256.clone());
                operation.destination.clone()
            };
            if let Some(file) = prepared
                .prepared_files
                .iter_mut()
                .find(|file| file.operation_id == operation_id)
            {
                file.destination = destination;
                file.bytes = merged;
                file.expected_sha256 = result_sha256;
            } else {
                return Err("merge operation has no prepared incoming bytes".into());
            }
        }
        "rename" => {
            let external = prepared.plan.operations[operation_index].external;
            let original = prepared.plan.operations[operation_index]
                .destination
                .clone();
            let destination = renamed_destination(&prepared.canonical_root, &original, external)
                .map_err(|error| error.to_string())?;
            {
                let operation = &mut prepared.plan.operations[operation_index];
                operation.resolution = Some(choice.clone());
                operation.action = OperationAction::Rename;
                operation.destination = destination.clone();
                operation.local_sha256 = None;
                operation.local_state = LocalState::Absent;
                operation.result_sha256 = None;
                operation.rollback = RollbackAction::RemoveCreated;
            }
            if let Some(file) = prepared
                .prepared_files
                .iter_mut()
                .find(|file| file.operation_id == operation_id)
            {
                file.destination = destination.clone();
            } else {
                return Err("rename operation has no prepared incoming bytes".into());
            }
            if let Some(entry) = prepared
                .plan
                .download_ledger
                .iter_mut()
                .find(|entry| entry.operation_id == operation_id)
            {
                entry.destination = destination;
            }
            if let Some(file) = prepared
                .prepared_files
                .iter()
                .find(|file| file.operation_id == operation_id)
            {
                prepared.plan.operations[operation_index].result_sha256 =
                    Some(file.expected_sha256.clone());
            }
        }
        "replace" => {
            {
                let operation = &mut prepared.plan.operations[operation_index];
                operation.resolution = Some(choice.clone());
                if operation.action != OperationAction::Generate {
                    operation.action = OperationAction::Replace;
                }
            }
            let result_sha256 = prepared
                .prepared_files
                .iter()
                .find(|file| file.operation_id == operation_id)
                .map(|file| file.expected_sha256.clone())
                .ok_or_else(|| "replace operation has no prepared incoming bytes".to_string())?;
            prepared.plan.operations[operation_index].result_sha256 = Some(result_sha256);
        }
        _ => {}
    }
    if !is_flattened_destination(&path) {
        refresh_flattened_outputs(prepared).map_err(|error| error.to_string())?;
    }
    Ok(prepared.plan.clone())
}

#[cfg(test)]
mod merged_update_tests {
    use super::*;

    const OLD: &str = "# Rules

## 4. Rules
1. one
2. two

## 5. Next
last
";
    const NEW: &str = "# Rules

## 4. Rules
1. one
2. two

## 4a. Tools
run checks

## 5. Next
last
";

    #[test]
    fn an_unedited_file_takes_the_new_adaptation() {
        assert_eq!(
            reconcile_merged_update(OLD.as_bytes(), OLD.as_bytes(), NEW.as_bytes(), false),
            MergedUpdate::Replace
        );
    }

    #[test]
    fn custom_rules_survive_when_the_new_section_goes_somewhere_else() {
        let local = OLD.replace(
            "2. two
",
            "2. two
3. my own rule
",
        );
        match reconcile_merged_update(OLD.as_bytes(), local.as_bytes(), NEW.as_bytes(), false) {
            MergedUpdate::Merged(bytes) => {
                let text = String::from_utf8(bytes).unwrap();
                assert!(text.contains("3. my own rule"), "{text}");
                assert!(text.contains("## 4a. Tools"), "{text}");
            }
            other => panic!("expected a clean merge, got {other:?}"),
        }
    }

    #[test]
    fn an_edited_file_is_reviewed_even_when_the_merge_would_be_clean() {
        let local = OLD.replace(
            "2. two
",
            "2. two
3. my own rule
",
        );
        assert_eq!(
            reconcile_merged_update(OLD.as_bytes(), local.as_bytes(), NEW.as_bytes(), true),
            MergedUpdate::NeedsReview
        );
    }

    #[test]
    fn overlapping_changes_are_reviewed_not_guessed() {
        let local = OLD.replace(
            "last
",
            "my ending
",
        );
        let incoming = OLD.replace(
            "last
",
            "their ending
",
        );
        assert_eq!(
            reconcile_merged_update(OLD.as_bytes(), local.as_bytes(), incoming.as_bytes(), false),
            MergedUpdate::NeedsReview
        );
    }

    #[test]
    fn a_file_that_is_not_text_is_reviewed() {
        assert_eq!(
            reconcile_merged_update(b"a", &[0xff, 0xfe], b"b", false),
            MergedUpdate::NeedsReview
        );
    }
}
