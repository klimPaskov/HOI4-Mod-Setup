//! The default plan builder.
//!
//! [`build_plan_with`] is the desktop's installation-plan builder, moved here
//! so a server can run it. It reads nothing from the process: every ambient
//! input the desktop used (its analysis and scan stores, credential vault,
//! application-data folders, installed game, context attachments and source
//! client) comes from a [`PlanHost`], which the builder asks at the points
//! where the desktop used to reach for the process.
//!
//! * The desktop's host (in the command layer) answers from those process-wide
//!   stores and folders, so the desktop plans exactly as before.
//! * [`DefaultPlanBuilder`] answers from a [`PlanRequest`]: the root, its
//!   identity and the confirmed analysis the session already verified, the
//!   session's state root, the Node runtime the service was given, and a
//!   [`SourceFetcher`] the host chose.

use super::inputs::*;
use super::{BuiltPlan, PlanBuilder, PlanRequest};
use crate::ai::{self, AiProviderConfig};
use crate::context::ContextFile;
use crate::credentials::validate_credential_reference;
use crate::descriptors::generated_artifacts as render_generated_artifacts;
use crate::mcp::VerifiedMcpTarget;
use crate::merge::allowed_choices;
use crate::models::*;
use crate::security::{
    canonical_relative_key, safe_join, sha256_bytes, sha256_file, validate_external_destination,
};
use crate::service::evidence::{
    ai_endpoint_from_state, ai_model_from_state, ai_provider_from_state,
    ai_reasoning_effort_from_state, string_field,
};
use crate::service::plans::{existing_project_file_is_kept_by_default, renamed_destination};
use crate::service::runtime::mcp_client_launch;
use crate::source::{
    expand_components, resolve_source, select_manifest_files, verify_download, HttpSourceClient,
    SourceFetcher,
};
use crate::AppError;
use hoi4_setup_core::adapt::*;
use hoi4_setup_core::image_generation::*;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Everything the plan builder takes from its surroundings.
///
/// The methods are called at the points in planning where the desktop read
/// the process. See [`build_plan_with`].
pub(crate) trait PlanHost {
    /// The project root the plan is for.
    fn project_root(&self, state: &Value) -> Result<PathBuf, AppError>;
    /// The analysis the reviewer confirmed for this state.
    fn confirmed_analysis(
        &self,
        state: &Value,
        root: &Path,
    ) -> Result<CodexAnalysisRecord, AppError>;
    /// The identity the plan binds: the scanned root's, or the parent's of a
    /// root that will be created.
    fn project_root_identity(
        &self,
        root: &Path,
        root_exists: bool,
        analysis: &CodexAnalysisRecord,
    ) -> Result<String, AppError>;
    /// Whether a new project is registered in the HOI4 launcher.
    fn launcher_policy(&self) -> LauncherPolicy;
    /// Where the game and the Workshop are, as the instructions name them.
    fn game_folders(&self) -> GameFolders;
    /// The app-owned launcher the 3D workflow registers.
    fn three_d_launcher(&self) -> Result<String, AppError>;
    /// The reference to the user's Meshy key.
    fn meshy_credential_reference(&self) -> Result<Option<CredentialReference>, AppError>;
    /// The thumbnail rendered from an image the user chose.
    fn context_thumbnail(&self, state: &Value) -> Result<Option<Vec<u8>>, AppError>;
    /// The documents and images the user asked to keep in the project.
    fn context_copies(&self, state: &Value) -> Result<Vec<ContextFile>, AppError>;
    /// The command that starts the installed MCP package.
    fn mcp_launch(
        &self,
        target: &VerifiedMcpTarget,
        platform: Platform,
    ) -> Result<(String, Vec<String>), AppError>;
    /// Where the transaction keeps backups and staging.
    fn transaction_roots(&self) -> Result<(PathBuf, PathBuf), AppError>;
    /// Where the package comes from.
    fn source(&self) -> Result<Arc<dyn SourceFetcher>, AppError>;
    /// A last check of the finished plan.
    fn check_plan(&self, plan: &InstallationPlan) -> Result<(), AppError>;
}

/// Adapt one selected source file to the confirmed project.
#[allow(clippy::too_many_arguments)]
pub(crate) fn adapt_selected_source(
    host: &dyn PlanHost,
    component_id: &str,
    destination: &str,
    bytes: &[u8],
    identity: &ProjectIdentity,
    conventions: &ProjectConventions,
    _ai_provider: &str,
    _ai_model: &str,
    _ai_reasoning_effort: &str,
    super_events_selected: bool,
    mcp_selected: bool,
    three_d_selected: bool,
    portrait: &PortraitPipelineConfig,
    selected_components: &[String],
) -> Result<Vec<u8>, AppError> {
    let folders = host.game_folders();
    hoi4_setup_core::adapt::adapt_selected_source(
        component_id,
        destination,
        bytes,
        &AdaptContext {
            identity,
            conventions,
            folders: &folders,
            super_events_selected,
            portrait,
            selected_components,
        },
        &|codex| {
            let launcher = if three_d_selected {
                Some(host.three_d_launcher()?)
            } else {
                None
            };
            hoi4_setup_core::adapt::adapt_codex_config(
                codex,
                mcp_selected,
                launcher.as_deref(),
                super_events_selected,
                portrait,
            )
        },
    )
}

/// Build the plan for the reviewed `state`.
///
/// Every ambient input comes from `host`; see [`PlanHost`]. The body is the
/// desktop's builder, unchanged except where it used to read the process.
pub(crate) fn build_plan_with(
    state: &Value,
    host: &dyn PlanHost,
) -> Result<(InstallationPlan, Vec<PreparedFile>), AppError> {
    let root = host.project_root(state)?;
    let root_exists = root.exists();
    let root_mode = if root_exists {
        ProjectRootMode::Existing
    } else {
        ProjectRootMode::CreateLeaf
    };
    let identity = project_identity_with_launcher(state, &root, host.launcher_policy())?;
    let conventions = project_conventions_from_state(state)?;
    let codex_analysis = host.confirmed_analysis(state, &root)?;
    let project_root_identity = host.project_root_identity(&root, root_exists, &codex_analysis)?;
    let ai_provider = ai_provider_from_state(state)?;
    let ai_model = ai_model_from_state(state);
    let portrait_pipeline = portrait_pipeline_from_state(state)?;
    let mesh_selected = state
        .get("meshSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    ai::validate_config(&AiProviderConfig {
        provider: ai_provider.clone(),
        model: ai_model.clone(),
        reasoning_effort: ai_reasoning_effort_from_state(state),
        endpoint: ai_endpoint_from_state(state).unwrap_or_default(),
        credential_reference: if ai_provider == "codex" {
            None
        } else {
            ai_credential_reference(&ai_provider)
        },
    })?;
    let git_setup = git_setup_from_state(state, &root)?;
    let request = source_request_from_state(state)?;
    let client = host.source()?;
    let resolution = resolve_source(client.as_ref(), &request)?;
    validate_analysis_source_binding(&codex_analysis, &resolution.identity)?;
    let (requested, coding_environment_selection) =
        requested_components_for_selection(&resolution.manifest, state, &ai_provider)?;
    let image_generation = image_generation_selection_from_state(
        state,
        &resolution.manifest,
        &coding_environment_selection,
    )?;
    let live_testing = crate::live_testing::from_state(
        state,
        &resolution.manifest,
        &requested,
        Platform::current(),
    )?;
    if requested.is_empty() {
        return Err(AppError::InvalidInput(
            "select at least one manifest component".into(),
        ));
    }
    let selected = expand_components(&resolution.manifest, &requested)?;
    let support = crate::source::resolve_platform_support(
        &resolution.manifest,
        &selected,
        Platform::current(),
    )?;
    if support.iter().any(|item| item.state == "blocked") {
        return Err(AppError::UnsupportedPlatform(
            "a required selected component has no verified platform route".into(),
        ));
    }
    let mcp_selected = support
        .iter()
        .any(|item| item.component_id == "mcp.hoi4_agent_tools" && item.state == "supported");
    let three_d_selected = support
        .iter()
        .any(|item| item.component_id == "workflow.3d" && item.state == "supported");
    let super_events_selected = selected.iter().any(|id| id == "workflow.super_events");
    let download_selected = selected
        .iter()
        .filter(|id| {
            support
                .iter()
                .find(|item| item.component_id == id.as_str())
                .is_some_and(|item| item.state == "supported")
        })
        .cloned()
        .collect::<Vec<_>>();
    let selections = select_manifest_files(&resolution.manifest, &download_selected)?;
    let by_id = resolution
        .manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect::<std::collections::HashMap<_, _>>();
    let external_actions = manifest_external_actions(&resolution.manifest, &download_selected);
    let mut operations = Vec::new();
    let mut conflicts = Vec::new();
    let mut prepared = Vec::new();
    let mut download_ledger = Vec::new();
    let mut adapted_destinations = BTreeSet::new();
    client.prefetch_verified_files(
        &resolution.identity.resolved_revision,
        &selections
            .iter()
            .filter_map(|selection| {
                selection.expected_sha256.clone().map(|sha256| {
                    (
                        selection.source_path.clone(),
                        sha256,
                        selection.expected_size,
                    )
                })
            })
            .collect::<Vec<_>>(),
    );
    for (index, selection) in selections.iter().enumerate() {
        let adapted_destination = adapt_super_events_destination(
            &selection.component_id,
            &selection.destination,
            &identity,
        )?;
        let destination_key = canonical_relative_key(&adapted_destination)
            .map_err(|error| AppError::Source(error.to_string()))?;
        if !adapted_destinations.insert(destination_key) {
            return Err(AppError::Source(format!(
                "duplicate adapted selected destination: {adapted_destination}"
            )));
        }
        let expected_sha256 = selection.expected_sha256.as_deref().ok_or_else(|| {
            AppError::Source(format!(
                "selected source file lacks SHA-256 evidence: {}",
                selection.source_path
            ))
        })?;
        let source_bytes = client.fetch_verified_file(
            &resolution.identity.resolved_revision,
            &selection.source_path,
            expected_sha256,
            selection.expected_size,
        )?;
        let mut verified = verify_download(
            selection,
            &source_bytes,
            &resolution.identity.resolved_revision,
        )?;
        let bytes = adapt_selected_source(
            host,
            &selection.component_id,
            &adapted_destination,
            &source_bytes,
            &identity,
            &conventions,
            &ai_provider,
            &ai_model,
            &ai_reasoning_effort_from_state(state),
            super_events_selected,
            mcp_selected,
            three_d_selected,
            &portrait_pipeline,
            &download_selected,
        )?;
        let bytes = if is_image_generation_client_config_path(&adapted_destination) {
            merge_existing_image_client_config(&root, &adapted_destination, &bytes)?
        } else {
            bytes
        };
        let bytes =
            adapt_image_generation_guidance(&bytes, &resolution.manifest, &image_generation)?;
        let bytes = adapt_image_generation_client_config_with_launch(
            &bytes,
            &adapted_destination,
            &resolution.manifest,
            &image_generation,
            &coding_environment_selection,
            &live_testing,
            Platform::current(),
            cfg!(windows),
            &|target| host.mcp_launch(target, Platform::current()),
        )?;
        let incoming_sha256 = sha256_bytes(&bytes);
        let source_destination = safe_join(&root, &adapted_destination)?;
        let mut local_sha = if source_destination.is_file() {
            Some(sha256_file(&source_destination)?)
        } else {
            None
        };
        let component = by_id.get(selection.component_id.as_str()).ok_or_else(|| {
            AppError::Source("selected component disappeared from manifest".into())
        })?;
        let local_state = match local_sha.as_deref() {
            None => LocalState::Absent,
            Some(hash) if hash == incoming_sha256 => LocalState::Unmodified,
            Some(_) => LocalState::Modified,
        };
        let mut destination = adapted_destination.clone();
        let mut operation_local_state = local_state;
        let mut action = if local_state == LocalState::Absent {
            OperationAction::Create
        } else if selection.ownership == Ownership::Merged {
            OperationAction::Merge
        } else {
            OperationAction::Replace
        };
        let mut selected_choice = None;
        if local_state == LocalState::Modified {
            let kind = component_kind(component, &adapted_destination);
            let options =
                allowed_choices(kind, crate::merge::MergeClassification::UserOwnedConflict);
            let choice = choice_from_state(state)
                .filter(|value| options.iter().any(|option| option == value));
            if let Some(choice) = choice {
                action = match choice.as_str() {
                    "keep" | "skip" => OperationAction::Skip,
                    "rename" => OperationAction::Rename,
                    "replace" => OperationAction::Replace,
                    _ => return Err(AppError::Transaction("invalid conflict choice".into())),
                };
                if action == OperationAction::Rename {
                    destination = renamed_destination(&root, &adapted_destination, false)?;
                    local_sha = None;
                    operation_local_state = LocalState::Absent;
                }
                selected_choice = Some(choice);
            } else {
                // Keep the incoming bytes in the core session so a later
                // conflict decision can replace or rename them, but make the
                // unresolved operation a safe no-op until that decision is
                // explicitly recorded.
                action = OperationAction::Skip;
            }
            conflicts.push(PlanConflict {
                id: format!("conflict-{index}"),
                path: adapted_destination.clone(),
                options,
                selected: selected_choice.clone(),
                apply_to_identical: false,
            });
        }
        let operation_id = format!("op-{index:05}");
        operations.push(PlanOperation {
            id: operation_id.clone(),
            component_id: selection.component_id.clone(),
            ownership: Some(selection.ownership),
            location_scope: Some("project".into()),
            action,
            source_path: Some(selection.source_path.clone()),
            destination: destination.clone(),
            source_sha256: Some(verified.sha256.clone()),
            source_size: selection.expected_size,
            platform: Some(selection.platform),
            executable: selection.executable,
            result_sha256: (action != OperationAction::Skip).then_some(incoming_sha256),
            base_sha256: None,
            local_sha256: local_sha,
            local_state: operation_local_state,
            resolution: selected_choice,
            external: false,
            rollback: if matches!(action, OperationAction::Create | OperationAction::Rename) {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
            external_parent_identity: None,
        });
        verified.operation_id = operation_id.clone();
        verified.destination = destination.clone();
        verified.manifest_sha256 = resolution.identity.manifest_sha256.clone();
        download_ledger.push(verified);
        // Keep a verified incoming copy even for an unresolved conflict. It
        // remains in the core-owned plan session and is discarded when the
        // user chooses keep/skip; this makes replace/rename decisions possible
        // without re-fetching or trusting renderer-supplied bytes.
        let prepared_sha256 = sha256_bytes(&bytes);
        prepared.push(PreparedFile {
            operation_id,
            destination,
            bytes,
            expected_sha256: prepared_sha256,
        });
    }
    let mode = string_field(state, "mode").unwrap_or_else(|| "new".into());
    // Descriptor, launcher registration, and thumbnail are independent
    // managed artifacts. Existing projects must still receive an explicit
    // review operation for each one; an existing descriptor is not evidence
    // that the other two routes are valid.
    let mut generated = render_generated_artifacts(&identity)?;
    let mut thumbnail_chosen_by_user = false;
    if let Some(thumbnail) = host.context_thumbnail(state)? {
        thumbnail_chosen_by_user = true;
        if let Some(artifact) = generated
            .iter_mut()
            .find(|artifact| artifact.component_id == "project.thumbnail")
        {
            artifact.content = "Thumbnail rendered from an image the user chose.".into();
            artifact.expected_sha256 = sha256_bytes(&thumbnail);
            artifact.bytes = Some(thumbnail);
        }
    }
    for file in host.context_copies(state)? {
        generated.push(GeneratedArtifact {
            component_id: file.component_id.into(),
            destination: file.destination,
            content: "Context file the user added during setup.".into(),
            expected_sha256: sha256_bytes(&file.bytes),
            external: false,
            bytes: Some(file.bytes),
        });
    }
    if let Some(image_config) =
        image_generation_config_artifact(&resolution.manifest, &image_generation)?
    {
        generated.push(image_config);
    }
    if let Some(guidance) =
        image_generation_guidance_artifact(&resolution.manifest, &image_generation)?
    {
        generated.push(guidance);
    }
    let selected_config_destinations = selections
        .iter()
        .map(|selection| selection.destination.clone())
        .collect::<Vec<_>>();
    generated.extend(image_client_config_artifacts_with_launch(
        &root,
        &resolution.manifest,
        &image_generation,
        &coding_environment_selection,
        &selected_config_destinations,
        &live_testing,
        Platform::current(),
        &|target| host.mcp_launch(target, Platform::current()),
    )?);
    if mode == "new" || mode == "existing" {
        let description = string_field(state, "description").unwrap_or_default();
        let readme = project_readme(
            &identity,
            &description,
            &ai_provider,
            &ai_model_from_state(state),
            &ai_reasoning_effort_from_state(state),
        )?;
        generated.push(GeneratedArtifact {
            component_id: "project.readme".into(),
            destination: "README.md".into(),
            expected_sha256: sha256_bytes(readme.as_bytes()),
            content: readme,
            external: false,
            bytes: None,
        });
    }
    let mut credential_references = if mesh_selected {
        host.meshy_credential_reference()?
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    credential_references.sort_by(|left, right| left.reference.cmp(&right.reference));
    credential_references.dedup_by(|left, right| left.reference == right.reference);
    for reference in &credential_references {
        validate_credential_reference(reference)?;
    }
    if mode == "new" {
        let provider_profile = ai::profile(&ai_provider).ok_or_else(|| {
            AppError::InvalidInput(format!("unsupported AI provider: {ai_provider}"))
        })?;
        let (provider_integration, provider_auth_mode) =
            ai::integration_and_auth_mode(&ai_provider);
        let state_content = serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "1.0.0",
            "project_id": identity.project_id.clone(),
            "project_root": identity.project_root.display().to_string(),
            "platform": Platform::current().manifest_name(),
            "wizard": {
                "current_step": "ready",
                "completed_steps": ["welcome", "description", "identity", "environments", "components", "workflows"]
            },
            "preferences": {
                "telemetry": false
            },
            "ai": {
                "provider": ai_provider.clone(),
                "model": ai_model_from_state(state),
                "reasoning_effort": ai_reasoning_effort_from_state(state),
                "optimization_profile": provider_profile.optimization_profile
            },
            "codex": {
                "integration": provider_integration,
                "auth_mode": provider_auth_mode,
                "auth_status": if matches!(provider_auth_mode, "chatgpt" | "claude_account" | "provider_account") { "signed_in" } else { "configured" },
                "analysis_required": true,
                "analysis_status": "confirmed",
                "account_values_persisted": false
            },
            "coding_environments": {
                "primary": coding_environment_selection.primary.clone(),
                "additional": coding_environment_selection.additional.clone()
            },
            "project_conventions": {
                "agents_profile": conventions.agents_profile,
                "localisation_convention": conventions.localisation_convention,
                "documentation_convention": conventions.documentation_convention
            },
            "credential_references": credential_references.clone(),
            "portrait_pipeline": portrait_pipeline.clone(),
            "image_generation": image_generation.clone(),
            "live_testing": live_testing.clone()
        }))?;
        generated.push(GeneratedArtifact {
            component_id: "project.state".into(),
            destination: ".hoi4-mod-setup/state.json".into(),
            expected_sha256: sha256_bytes(state_content.as_bytes()),
            content: state_content,
            external: false,
            bytes: None,
        });
    }
    let current = std::fs::read_to_string(root.join(".gitignore")).ok();
    let content = crate::git::merge_gitignore(current.as_deref());
    generated.push(GeneratedArtifact {
        component_id: "git.ignore".into(),
        destination: ".gitignore".into(),
        expected_sha256: sha256_bytes(content.as_bytes()),
        content,
        external: false,
        bytes: None,
    });
    for (generated_index, artifact) in generated.iter().enumerate() {
        let destination_path = if artifact.external {
            validate_external_destination(&artifact.destination)?
        } else {
            safe_join(&root, &artifact.destination)?
        };
        let local_sha = if destination_path.is_file() {
            Some(sha256_file(&destination_path)?)
        } else {
            None
        };
        let mut destination = artifact.destination.clone();
        let mut local_state = match local_sha.as_deref() {
            None => LocalState::Absent,
            Some(hash) if hash == artifact.expected_sha256 => LocalState::Unmodified,
            Some(_) => LocalState::Modified,
        };
        let mut action = if local_state == LocalState::Absent {
            OperationAction::Generate
        } else if local_state == LocalState::Unmodified {
            OperationAction::Skip
        } else {
            OperationAction::Generate
        };
        let mut operation_local_sha = local_sha;
        let had_existing_destination = operation_local_sha.is_some();
        let mut selected_choice = None;
        if local_state == LocalState::Modified {
            let options = allowed_choices(
                path_file_kind(&artifact.destination),
                crate::merge::MergeClassification::UserOwnedConflict,
            );
            let default_keep = (mode == "existing"
                && !artifact.external
                && existing_project_file_is_kept_by_default(
                    &artifact.component_id,
                    &destination_path,
                    thumbnail_chosen_by_user,
                ))
            .then(|| "keep".to_string());
            if let Some(choice) = choice_from_state(state)
                .filter(|value| options.iter().any(|option| option == value))
                .or(default_keep)
            {
                action = match choice.as_str() {
                    "keep" | "skip" => OperationAction::Skip,
                    "rename" => OperationAction::Rename,
                    "replace" => OperationAction::Generate,
                    "merge" => {
                        return Err(AppError::Merge(
                            "generated descriptor has no verified merge base".into(),
                        ))
                    }
                    _ => return Err(AppError::Transaction("invalid conflict choice".into())),
                };
                if action == OperationAction::Rename {
                    destination =
                        renamed_destination(&root, &artifact.destination, artifact.external)?;
                    operation_local_sha = None;
                    local_state = LocalState::Absent;
                }
                selected_choice = Some(choice);
            } else {
                action = OperationAction::Skip;
            }
            conflicts.push(PlanConflict {
                id: format!("conflict.generated-{generated_index}"),
                path: artifact.destination.clone(),
                options,
                selected: selected_choice.clone(),
                apply_to_identical: false,
            });
        }
        let operation_id = format!("generated-{generated_index:05}");
        operations.push(PlanOperation {
            id: operation_id.clone(),
            component_id: artifact.component_id.clone(),
            ownership: Some(if artifact.component_id == "git.ignore" {
                Ownership::Merged
            } else if artifact
                .component_id
                .starts_with("image_generation.client_config.")
            {
                image_client_config_ownership(had_existing_destination, None)
            } else {
                Ownership::Generated
            }),
            location_scope: Some(if artifact.external {
                "external_launcher".into()
            } else {
                "project".into()
            }),
            action,
            source_path: Some(format!("generated:{}", artifact.destination)),
            destination: destination.clone(),
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
            local_sha256: operation_local_sha,
            local_state,
            resolution: selected_choice,
            external: artifact.external,
            rollback: if action == OperationAction::Rename {
                RollbackAction::RemoveCreated
            } else if had_existing_destination && action != OperationAction::Skip {
                RollbackAction::RestoreBackup
            } else if action == OperationAction::Generate {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
            external_parent_identity: None,
        });
        prepared.push(PreparedFile {
            operation_id,
            destination,
            bytes: artifact
                .bytes
                .clone()
                .unwrap_or_else(|| artifact.content.as_bytes().to_vec()),
            expected_sha256: artifact.expected_sha256.clone(),
        });
    }
    let mut optional_workflows = generated_state(state);
    // Persist every manifest-declared optional state, including choices the
    // current app version did not know by name. This lets a later compatible
    // manifest distinguish a deliberate decline from a newly published
    // default without a binary update.
    seed_manifest_optional_states(&mut optional_workflows, &resolution.manifest);
    if mesh_selected && !credential_references.is_empty() {
        optional_workflows.insert("workflow.3d".into(), "selected_pending".into());
    }
    for item in &support {
        if item.state == "unsupported_platform" {
            optional_workflows.insert(item.component_id.clone(), "unsupported_platform".into());
        }
    }
    for component in resolution
        .manifest
        .components
        .iter()
        .filter(|component| component.optional && selected.iter().any(|id| id == &component.id))
    {
        let existing = optional_workflows.get(&component.id).map(String::as_str);
        let selected_state = if existing.is_some_and(|state| state != "not_selected") {
            existing.unwrap().to_string()
        } else if external_actions
            .iter()
            .any(|action| action.component_id == component.id)
        {
            "selected_pending".into()
        } else {
            "ready".into()
        };
        optional_workflows.insert(component.id.clone(), selected_state);
    }
    let (backup_root, staging_root) = host.transaction_roots()?;
    let mut plan = InstallationPlan {
        schema_version: crate::migrations::CURRENT_PLAN_SCHEMA.into(),
        plan_id: Uuid::new_v4(),
        project_id: state
            .pointer("/identity/projectId")
            .and_then(Value::as_str)
            .unwrap_or("project")
            .into(),
        script_prefix: identity.script_prefix.clone(),
        primary_namespace: identity.primary_namespace.clone(),
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        maintenance_mode: None,
        source: resolution.identity,
        ai_optimization_profile: ai_optimization_profile(&ai_provider),
        ai_provider,
        ai_model: ai_model_from_state(state),
        ai_reasoning_effort: ai_reasoning_effort_from_state(state),
        ai_endpoint: ai_endpoint_from_state(state),
        primary_coding_environment: coding_environment_selection.primary.clone(),
        additional_coding_environments: coding_environment_selection.additional.clone(),
        // The ChatGPT packager is an optional `.tools` script the user runs;
        // setup never prepares a ChatGPT folder for a new project.
        flatten_chat_sources: false,
        codex_analysis: Some(codex_analysis),
        selected_components: selected,
        wiki_required_pages: resolution.manifest.wiki.required_pages.clone(),
        wiki_metadata: Some(crate::source::wiki_install_metadata(&resolution.manifest)),
        generated_artifacts: generated,
        download_ledger,
        git_setup,
        credential_references,
        optional_workflows,
        portrait_pipeline: Some(portrait_pipeline),
        image_generation: Some(image_generation),
        live_testing: Some(live_testing),
        operations,
        conflicts,
        external_actions,
        transaction: TransactionPlanInfo {
            stages: TRANSACTION_STAGES
                .iter()
                .map(|stage| (*stage).into())
                .collect(),
            backup_root: backup_root.display().to_string(),
            staging_root: staging_root.display().to_string(),
            directories: transaction_directories(state, &mode)?,
            atomic_apply_expected: true,
            project_root_mode: root_mode,
            project_root_parent: (!root_exists)
                .then(|| root.parent().map(|path| path.display().to_string()))
                .flatten(),
            project_root_leaf: (!root_exists)
                .then(|| {
                    root.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .flatten(),
            project_root_identity: Some(project_root_identity),
        },
        approvals: PlanApprovals {
            dry_run_reviewed: false,
            external_actions_reviewed: false,
            git_remote_approved: false,
            push_approved: false,
        },
    };
    crate::transaction::bind_plan_external_parents(&mut plan)?;
    host.check_plan(&plan)?;
    Ok((plan, prepared))
}

/// The plan builder a server runs.
///
/// It builds the plan the desktop builds for the same state, from the facts
/// the session verified and nothing else:
///
/// * the project root, its identity and the confirmed analysis come from the
///   [`PlanRequest`], not from a process-wide store;
/// * backups and staging are named under the session's state root, never the
///   user's application-data folder;
/// * the Node runtime registered in coding-client MCP configuration is the one
///   the service was given;
/// * the workflow package comes from the [`SourceFetcher`] it was built with,
///   whose manifest is validated and whose every selected file is checked
///   against the manifest's size and SHA-256 before it is planned;
/// * the game and Workshop folders in the project instructions are the
///   placeholders `[HOI4_GAME_DIR]` and `[STEAM_WORKSHOP_DIR]`, as in the web
///   edition's export, because the server's own folders mean nothing to the
///   person who plays the mod.
///
/// What a server cannot do is refused instead of guessed. A plan that would
/// write outside the project root (the HOI4 launcher's mod folder, user-level
/// client configuration) is refused, and so are the 3D workflow and context
/// documents and images, which need the desktop's credential vault, its
/// app-owned launcher and its process-wide attachment store.
pub struct DefaultPlanBuilder {
    source: Arc<dyn SourceFetcher>,
}

impl DefaultPlanBuilder {
    /// A builder that downloads the package from the public repository and
    /// keeps verified files under `cache_root`.
    ///
    /// `cache_root` must exist, be absolute and be free of symlinks. It holds
    /// content-addressed files that are verified again each time they are
    /// read, so one cache can serve every account. It must be outside every
    /// workspace root. The developer's local-checkout environment variable is
    /// not consulted.
    pub fn new(cache_root: impl AsRef<Path>) -> Result<Self, AppError> {
        let cache_root = super::existing_absolute_directory(cache_root.as_ref(), "cache")?;
        Ok(Self::with_source(Arc::new(
            HttpSourceClient::for_cache_root(cache_root)?,
        )))
    }

    /// A builder that gets the package from `source`. Tests and hosts that
    /// fetch the package themselves use this. Integrity does not depend on the
    /// source: see [`SourceFetcher`].
    pub fn with_source(source: Arc<dyn SourceFetcher>) -> Self {
        Self { source }
    }
}

impl PlanBuilder for DefaultPlanBuilder {
    fn build(&self, request: &PlanRequest<'_>) -> Result<BuiltPlan, AppError> {
        let host = ServiceHost {
            request,
            source: self.source.clone(),
        };
        let (plan, prepared_files) = build_plan_with(request.state, &host)?;
        Ok(BuiltPlan {
            plan,
            prepared_files,
            // A new project and an import offer keep, replace, rename and
            // skip. Merge decisions belong to maintenance plans.
            merge_contexts: HashMap::new(),
        })
    }
}

struct ServiceHost<'a> {
    request: &'a PlanRequest<'a>,
    source: Arc<dyn SourceFetcher>,
}

const NEEDS_DESKTOP_3D: &str =
    "the 3D workflow needs the desktop app's credential vault and launcher, which this service does not have";
const NEEDS_DESKTOP_CONTEXT: &str =
    "context documents and images are not available through this service";

impl PlanHost for ServiceHost<'_> {
    fn project_root(&self, _state: &Value) -> Result<PathBuf, AppError> {
        // The session verified this root and rewrote the state to name it. A
        // root that appeared or vanished since is not the one that was
        // verified.
        let root = self.request.project_root.to_path_buf();
        if root.exists() != self.request.project_root_exists {
            return Err(AppError::PathSecurity(
                "the project root changed while the plan was being prepared".into(),
            ));
        }
        Ok(root)
    }

    fn confirmed_analysis(
        &self,
        _state: &Value,
        _root: &Path,
    ) -> Result<CodexAnalysisRecord, AppError> {
        Ok(self.request.analysis.clone())
    }

    fn project_root_identity(
        &self,
        _root: &Path,
        _root_exists: bool,
        _analysis: &CodexAnalysisRecord,
    ) -> Result<String, AppError> {
        Ok(self.request.project_root_identity.to_owned())
    }

    fn launcher_policy(&self) -> LauncherPolicy {
        LauncherPolicy::Refused
    }

    fn game_folders(&self) -> GameFolders {
        GameFolders {
            game_dir: "[HOI4_GAME_DIR]".into(),
            workshop_dir: "[STEAM_WORKSHOP_DIR]".into(),
        }
    }

    fn three_d_launcher(&self) -> Result<String, AppError> {
        Err(AppError::UnsupportedPlatform(NEEDS_DESKTOP_3D.into()))
    }

    fn meshy_credential_reference(&self) -> Result<Option<CredentialReference>, AppError> {
        Err(AppError::UnsupportedPlatform(NEEDS_DESKTOP_3D.into()))
    }

    fn context_thumbnail(&self, state: &Value) -> Result<Option<Vec<u8>>, AppError> {
        if state
            .get("contextThumbnail")
            .is_some_and(|value| !value.is_null())
        {
            return Err(AppError::InvalidInput(NEEDS_DESKTOP_CONTEXT.into()));
        }
        Ok(None)
    }

    fn context_copies(&self, state: &Value) -> Result<Vec<ContextFile>, AppError> {
        let kept = |key: &str, flag: &str| {
            state
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|item| item.get(flag).and_then(Value::as_bool) == Some(true))
        };
        if kept("contextDocuments", "keepCopy") || kept("contextImages", "saveReference") {
            return Err(AppError::InvalidInput(NEEDS_DESKTOP_CONTEXT.into()));
        }
        Ok(Vec::new())
    }

    fn mcp_launch(
        &self,
        target: &VerifiedMcpTarget,
        platform: Platform,
    ) -> Result<(String, Vec<String>), AppError> {
        mcp_client_launch(target, platform, self.request.node)
    }

    fn transaction_roots(&self) -> Result<(PathBuf, PathBuf), AppError> {
        Ok((
            self.request.state_root.join("backups"),
            self.request.state_root.join("staging"),
        ))
    }

    fn source(&self) -> Result<Arc<dyn SourceFetcher>, AppError> {
        Ok(self.source.clone())
    }

    fn check_plan(&self, plan: &InstallationPlan) -> Result<(), AppError> {
        // Every file the plan writes is inside the project root, by a path
        // that cannot leave it.
        for operation in &plan.operations {
            if operation.external {
                return Err(AppError::PathSecurity(format!(
                    "the plan writes outside the project root: {}",
                    operation.destination
                )));
            }
            safe_join(self.request.project_root, &operation.destination)?;
        }
        if let Some(artifact) = plan
            .generated_artifacts
            .iter()
            .find(|artifact| artifact.external)
        {
            return Err(AppError::PathSecurity(format!(
                "the plan writes outside the project root: {}",
                artifact.destination
            )));
        }
        Ok(())
    }
}
