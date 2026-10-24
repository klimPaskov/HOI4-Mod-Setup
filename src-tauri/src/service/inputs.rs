//! Parsing of the reviewed setup state into the inputs of a plan.
//!
//! These functions read only the state they are given and, where noted, the
//! project root. The desktop's command layer and the service's plan builder
//! call the same code, so a state means the same thing to both.

use crate::ai;
use crate::coding_environment;
use crate::credentials::{image_generation_credential_reference, provider_name};
use crate::merge::FileKind;
use crate::models::{CredentialReference, *};
use crate::paths::{validate_project_root, validate_project_root_or_destination};
use crate::security::{canonical_relative_key, safe_join, sha256_bytes};
use crate::service::evidence::{project_root_field, same_project_root, string_field};
use crate::source::SourceRequest;
use crate::AppError;
use hoi4_setup_core::adapt::*;
use hoi4_setup_core::image_generation::*;
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn ai_credential_reference(provider: &str) -> Option<CredentialReference> {
    let profile = ai::profile(provider.trim())?;
    if !profile.requires_credential {
        return None;
    }
    Some(CredentialReference {
        name: crate::credentials::AI_PROVIDER_ENVIRONMENT_NAME.into(),
        provider: provider_name(Platform::current()).into(),
        reference: format!("credential://ai_provider_api_key/{}", provider.trim()),
        provider_id: Some(provider.trim().into()),
    })
}

pub(crate) fn validate_analysis_source_binding(
    record: &CodexAnalysisRecord,
    source: &SourceIdentity,
) -> Result<(), AppError> {
    if record.source_revision.as_deref() != Some(source.resolved_revision.as_str())
        || record.source_manifest_sha256.as_deref() != Some(source.manifest_sha256.as_str())
    {
        return Err(AppError::Credential(
            "the confirmed semantic analysis belongs to a different source manifest; rerun analysis for the resolved source revision".into(),
        ));
    }
    Ok(())
}

pub(crate) fn source_request_from_state(state: &Value) -> Result<SourceRequest, AppError> {
    let mode = match string_field(state, "sourceMode").as_deref() {
        None | Some("latest") => SourceMode::Latest,
        Some("pinned_commit") => SourceMode::PinnedCommit,
        Some("pinned_release") => SourceMode::PinnedRelease,
        Some(value) => {
            return Err(AppError::InvalidInput(format!(
                "unsupported source mode: {value}"
            )))
        }
    };
    let requested_ref = string_field(state, "pinnedRef").filter(|value| !value.trim().is_empty());
    Ok(SourceRequest {
        mode,
        requested_ref: requested_ref.clone(),
        release: if mode == SourceMode::PinnedRelease {
            requested_ref
        } else {
            None
        },
    })
}

pub(crate) fn portrait_component_ids(provider: &str) -> Option<[&'static str; 4]> {
    match provider {
        "cloud" => Some([
            "workflow.portraits.core",
            "workflow.portraits.cloud",
            "workflow.portraits.subagent",
            "workflow.portraits.config",
        ]),
        "local" => Some([
            "workflow.portraits.core",
            "workflow.portraits.local",
            "workflow.portraits.subagent",
            "workflow.portraits.config",
        ]),
        "runpod" => Some([
            "workflow.portraits.core",
            "workflow.portraits.runpod",
            "workflow.portraits.subagent",
            "workflow.portraits.config",
        ]),
        _ => None,
    }
}

pub(crate) fn selected_ids(state: &Value, provider: &str) -> Vec<String> {
    let mut selected = state
        .get("selectedComponents")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    if state
        .get("meshSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !selected.iter().any(|id| id == "workflow.3d")
    {
        selected.push("workflow.3d".into());
    }
    if state
        .get("superEventsSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !selected.iter().any(|id| id == "workflow.super_events")
    {
        selected.push("workflow.super_events".into());
    }
    let portrait_provider = state
        .pointer("/portraitPipeline/provider")
        .and_then(Value::as_str)
        .unwrap_or("disabled");
    let portrait_enabled = state
        .pointer("/portraitPipeline/enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && portrait_provider != "disabled"
        && portrait_component_ids(portrait_provider).is_some();
    if portrait_enabled {
        for id in portrait_component_ids(portrait_provider).unwrap_or_default() {
            if !selected.iter().any(|selected_id| selected_id == id) {
                selected.push(id.into());
            }
        }
    } else {
        selected.retain(|id| !id.starts_with("workflow.portraits"));
    }
    let _ = provider;
    selected
}

pub(crate) fn requested_components_for_selection(
    manifest: &RemoteManifest,
    state: &Value,
    provider: &str,
) -> Result<(Vec<String>, CodingEnvironmentSelection), AppError> {
    let selection = coding_environment::selection_from_manifest(manifest, state)?;
    let mut requested = hoi4_setup_core::manifest::installation_components(
        manifest,
        &selected_ids(state, provider),
        &selection,
        Platform::current(),
    )?;
    let image_selection = image_generation_selection_from_state(state, manifest, &selection)?;
    for provider in image_generation_provider_rows(manifest, &image_selection)? {
        let component_id = provider
            .pointer("/route/component_id")
            .and_then(Value::as_str);
        if let Some(component_id) = component_id {
            if !manifest
                .components
                .iter()
                .any(|component| component.id == component_id)
            {
                return Err(AppError::Source(format!("image provider component is missing from the verified manifest: {component_id}")));
            }
            if !requested.iter().any(|selected| selected == component_id) {
                requested.push(component_id.to_owned());
            }
        }
    }
    let live = crate::live_testing::from_state(state, manifest, &requested, Platform::current())?;
    crate::live_testing::components(&mut requested, &live, manifest)?;
    Ok((requested, selection))
}

pub(crate) fn image_generation_selection_from_state(
    state: &Value,
    manifest: &RemoteManifest,
    environments: &CodingEnvironmentSelection,
) -> Result<ImageGenerationSelection, AppError> {
    let default_provider = manifest
        .image_generation
        .as_ref()
        .and_then(|value| value.get("default_provider"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    let selection_value = state.get("imageGeneration").cloned().unwrap_or_else(|| {
        serde_json::json!({
            "primary_provider": default_provider,
            "additional_providers": [],
            "route_settings": {},
            "credential_references": {}
        })
    });
    let mut selection: ImageGenerationSelection =
        serde_json::from_value(selection_value).map_err(|_| {
            AppError::InvalidInput("image provider selection has an invalid format".into())
        })?;
    // These fields are maintenance evidence from the trusted installation
    // lock, never renderer input.
    selection.installed_package_tree_sha256 = None;
    selection.installed_package_lock_sha256 = None;
    if selection.primary_provider.trim().is_empty() {
        selection.primary_provider = default_provider.to_owned();
    }
    validate_image_generation_selection(&selection, manifest, environments)?;
    let catalog_providers = manifest
        .image_generation
        .as_ref()
        .and_then(|value| value.get("providers"));
    if catalog_providers.is_none()
        && selection.primary_provider == "none"
        && selection.additional_providers.is_empty()
    {
        return Ok(selection);
    }
    let selected = std::iter::once(selection.primary_provider.as_str())
        .chain(selection.additional_providers.iter().map(String::as_str))
        .collect::<Vec<_>>();
    selection.credential_references.retain(|key, _| {
        key.split_once('.')
            .is_some_and(|(provider, _)| selected.contains(&provider))
    });
    for key in selection.credential_references.keys() {
        let (provider, field) = key.split_once('.').ok_or_else(|| {
            AppError::Credential("image credential reference has an invalid field name".into())
        })?;
        if !image_generation_credential_field_supported(provider, field)
            || !selected.contains(&provider)
        {
            return Err(AppError::Credential(
                "image credential reference is not for a selected provider".into(),
            ));
        }
        let expected = image_generation_credential_reference(provider, field)?;
        if selection.credential_references.get(key) != Some(&expected.reference) {
            return Err(AppError::Credential(
                "image credential reference does not match its provider field".into(),
            ));
        }
    }
    Ok(selection)
}

pub(crate) fn manifest_external_actions(
    manifest: &RemoteManifest,
    selected_components: &[String],
) -> Vec<ExternalAction> {
    manifest
        .components
        .iter()
        .filter(|component| selected_components.iter().any(|id| id == &component.id))
        .flat_map(|component| {
            component
                .validation
                .iter()
                .filter(|rule| rule.kind == "command" && rule.target.is_some())
                .filter_map(|rule| {
                    let platform = component
                        .platforms
                        .iter()
                        .find(|declared| declared.supports(Platform::current()))
                        .map(|_| Platform::current())
                        .or_else(|| {
                            component.platforms.first().map(|declared| match declared {
                                ManifestPlatform::Windows => Platform::Windows,
                                ManifestPlatform::Macos => Platform::Macos,
                                ManifestPlatform::Linux => Platform::Linux,
                                ManifestPlatform::All => Platform::current(),
                            })
                        })?;
                    let mut rule = rule.clone();
                    if let Some(pins) = rule
                        .parameters
                        .get("platform_pins")
                        .and_then(|value| value.get(platform.manifest_name()))
                        .and_then(Value::as_object)
                        .cloned()
                    {
                        for (key, value) in pins {
                            rule.parameters.insert(key, value);
                        }
                    }
                    // A Python validation target names the interpreter; its
                    // repository script remains the reviewed first argument.
                    // Execution still requires the app-owned script allowlist
                    // and the installed operation's exact checksum evidence.
                    let target = rule
                        .parameters
                        .get("script_path")
                        .and_then(Value::as_str)
                        .filter(|_| component.source.kind != SourceKind::Generated)
                        .or_else(|| {
                            rule.parameters
                                .get("platform_targets")
                                .and_then(|value| value.get(platform.manifest_name()))
                                .and_then(Value::as_str)
                        })
                        .or(rule.target.as_deref())?;
                    Some(ExternalAction {
                        id: format!("external.{}.{}", component.id, rule.id),
                        component_id: component.id.clone(),
                        platform,
                        command_source: match component.source.kind {
                            SourceKind::Generated => "remote_manifest".into(),
                            SourceKind::File | SourceKind::Tree => "repository_script".into(),
                        },
                        executable: Some(if target.to_ascii_lowercase().ends_with(".py") {
                            "manifest-declared Python tool".into()
                        } else {
                            "manifest-declared executable".into()
                        }),
                        arguments: std::iter::once(target.to_string())
                            .chain(
                                rule.parameters
                                    .get("arguments")
                                    .and_then(Value::as_array)
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                    .map(ToOwned::to_owned),
                            )
                            .collect(),
                        working_directory: Some("<project_root>".into()),
                        environment_names: component
                            .environment
                            .iter()
                            .map(|environment| environment.name.clone())
                            .collect(),
                        network_access: rule
                            .parameters
                            .get("network_access")
                            .and_then(Value::as_str)
                            .unwrap_or("not_declared")
                            .to_string(),
                        // `expected_files` describe the install payload, not
                        // the side effects of the command itself. Only the
                        // manifest's explicit command write declaration is
                        // presented in dry run.
                        expected_writes: rule
                            .parameters
                            .get("expected_writes")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect(),
                        privilege: rule
                            .parameters
                            .get("privilege")
                            .and_then(Value::as_str)
                            .unwrap_or("not_declared")
                            .to_string(),
                        rollback_boundary: rule
                            .parameters
                            .get("rollback_boundary")
                            .and_then(Value::as_str)
                            .unwrap_or("not_declared_by_source")
                            .to_string(),
                        display_command: Some(format!(
                            "Repository-declared validation target: {target}"
                        )),
                        risk: "high".into(),
                        requires_approval: true,
                        contains_secret: false,
                        verified_executable_sha256: rule
                            .parameters
                            .get("executable_sha256")
                            .or_else(|| rule.parameters.get("runtime_entry_sha256"))
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_executable_size: rule
                            .parameters
                            .get("executable_size")
                            .or_else(|| rule.parameters.get("runtime_entry_size"))
                            .and_then(Value::as_u64),
                        verified_interpreter_sha256: rule
                            .parameters
                            .get("interpreter_sha256")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_interpreter_size: rule
                            .parameters
                            .get("interpreter_size")
                            .and_then(Value::as_u64),
                        verified_runtime_sha256: rule
                            .parameters
                            .get("runtime_sha256")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_runtime_size: rule
                            .parameters
                            .get("runtime_size")
                            .and_then(Value::as_u64),
                        verified_package_symlinks: rule
                            .parameters
                            .get("package_symlinks")
                            .cloned()
                            .and_then(|value| serde_json::from_value(value).ok())
                            .unwrap_or_default(),
                        verified_package_name: rule
                            .parameters
                            .get("package_name")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_package_version: rule
                            .parameters
                            .get("package_version")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_package_integrity: rule
                            .parameters
                            .get("package_integrity")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_package_tree_sha256: rule
                            .parameters
                            .get("package_tree_sha256")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_package_file_count: rule
                            .parameters
                            .get("package_file_count")
                            .and_then(Value::as_u64),
                        verified_runtime_entry: rule
                            .parameters
                            .get("runtime_entry")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        required_tool_names: rule
                            .parameters
                            .get("required_tools")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect(),
                    })
                })
        })
        .collect()
}

// Only the desktop reads the root from the state or requires a launcher.
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
pub(crate) fn project_root_from_state(state: &Value) -> Result<PathBuf, AppError> {
    validate_project_root_or_destination(&project_root_field(state)?).map(|(root, _)| root)
}

pub(crate) fn component_kind(component: &ComponentDefinition, path: &str) -> FileKind {
    if component.destination.path.ends_with(".toml") || path.ends_with(".toml") {
        FileKind::Toml
    } else if component.destination.path.ends_with(".json") || path.ends_with(".json") {
        FileKind::Json
    } else if component.destination.path.ends_with(".md")
        || path.ends_with(".md")
        || path.ends_with(".txt")
    {
        FileKind::Text
    } else {
        FileKind::Binary
    }
}

pub(crate) fn path_file_kind(path: &str) -> FileKind {
    if path.ends_with(".toml") {
        FileKind::Toml
    } else if path.ends_with(".json") {
        FileKind::Json
    } else if path.ends_with(".md") || path.ends_with(".txt") {
        FileKind::Text
    } else {
        FileKind::Binary
    }
}

pub(crate) fn project_conventions_from_value(
    value: Option<&Value>,
) -> Result<ProjectConventions, AppError> {
    let Some(value) = value else {
        return Ok(ProjectConventions::default());
    };
    let object = value
        .as_object()
        .ok_or_else(|| AppError::InvalidInput("project conventions must be an object".into()))?;
    let fields = [
        ("agents_profile", crate::codex::ProposalKey::AgentsProfile),
        (
            "localisation_convention",
            crate::codex::ProposalKey::LocalisationConvention,
        ),
        (
            "documentation_convention",
            crate::codex::ProposalKey::DocumentationConvention,
        ),
    ];
    if object
        .keys()
        .any(|key| !fields.iter().any(|(name, _)| name == key))
    {
        return Err(AppError::InvalidInput(
            "project conventions contain an unsupported field".into(),
        ));
    }
    let mut result = ProjectConventions::default();
    for (field, key) in fields {
        let value = object
            .get(field)
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    AppError::InvalidInput(format!("project convention is invalid: {field}"))
                })
            })
            .transpose()?
            .unwrap_or_default();
        if !value.is_empty() {
            crate::codex::validate_proposal_value(&key, &Value::String(value.to_owned()))?;
        }
        match field {
            "agents_profile" => result.agents_profile = value.to_owned(),
            "localisation_convention" => result.localisation_convention = value.to_owned(),
            "documentation_convention" => result.documentation_convention = value.to_owned(),
            _ => unreachable!("the checked-in convention field set is exhaustive"),
        }
    }
    Ok(result)
}

pub(crate) fn project_conventions_from_state(
    state: &Value,
) -> Result<ProjectConventions, AppError> {
    project_conventions_from_value(
        state
            .get("conventions")
            .or_else(|| state.get("project_conventions")),
    )
}

pub(crate) fn portrait_pipeline_from_state(
    state: &Value,
) -> Result<PortraitPipelineConfig, AppError> {
    const REPOSITORY: &str = "https://github.com/klimPaskov/comfyui-hoi4-portraits";
    const BRANCH: &str = "codex/portrait-pipeline";
    const COMMIT: &str = "b47222a77f2f6454704530865aa1441fad48bdd3";
    let draft = state.get("portraitPipeline");
    let enabled = draft
        .and_then(|value| value.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let provider = draft
        .and_then(|value| value.get("provider"))
        .and_then(Value::as_str)
        .unwrap_or(if enabled { "cloud" } else { "disabled" })
        .trim()
        .to_ascii_lowercase();
    if !matches!(provider.as_str(), "cloud" | "local" | "runpod" | "disabled") {
        return Err(AppError::InvalidInput(
            "portrait provider must be Cloud, Local, RunPod, or Disabled".into(),
        ));
    }
    if provider == "disabled" && enabled {
        return Err(AppError::InvalidInput(
            "a Disabled portrait provider cannot be enabled".into(),
        ));
    }
    if provider != "disabled" && !enabled {
        return Err(AppError::InvalidInput(
            "an enabled portrait workflow must select a provider".into(),
        ));
    }
    let status = draft
        .and_then(|value| value.get("providerStatus"))
        .and_then(Value::as_str)
        .unwrap_or(if provider == "disabled" {
            "not_selected"
        } else if provider == "cloud" {
            "needs_authorization"
        } else if provider == "local" {
            "needs_hardware"
        } else {
            "needs_runpod"
        })
        .trim()
        .to_ascii_lowercase();
    const VALID_STATUSES: &[&str] = &[
        "not_selected",
        "ready",
        "needs_authorization",
        "needs_subscription",
        "needs_huggingface_access",
        "needs_models",
        "needs_workflow_install",
        "needs_hardware",
        "needs_runpod",
        "unreachable",
        "temporarily_unavailable",
    ];
    if !VALID_STATUSES.contains(&status.as_str()) {
        return Err(AppError::InvalidInput(
            "portrait provider status is unsupported".into(),
        ));
    }
    let preferred_workflow = draft
        .and_then(|value| value.get("preferredWorkflow"))
        .and_then(Value::as_str)
        .unwrap_or("source")
        .trim()
        .to_ascii_lowercase();
    if !matches!(preferred_workflow.as_str(), "source" | "processing_only") {
        return Err(AppError::InvalidInput(
            "portrait workflow must be source or processing_only; non-sourced portraits use native ImageGen".into(),
        ));
    }
    let workflow_commit = draft
        .and_then(|value| value.get("workflowCommit"))
        .and_then(Value::as_str)
        .unwrap_or(COMMIT)
        .trim()
        .to_ascii_lowercase();
    if !Regex::new(r"^[0-9a-f]{40}$")
        .expect("static portrait commit regex")
        .is_match(&workflow_commit)
    {
        return Err(AppError::InvalidInput(
            "portrait workflow commit must be one exact lowercase 40-character commit".into(),
        ));
    }
    if workflow_commit != COMMIT {
        return Err(AppError::Source(
            "portrait workflow commit is not the current verified upstream revision".into(),
        ));
    }
    let local_server_url = string_field(draft.unwrap_or(&Value::Null), "localServerUrl")
        .unwrap_or_else(|| "http://127.0.0.1:8188".into());
    if provider == "local" {
        crate::portraits::validate_loopback_url(&local_server_url)
            .map_err(AppError::InvalidInput)?;
    }
    let runpod_url = string_field(draft.unwrap_or(&Value::Null), "runpodUrl").unwrap_or_default();
    if provider == "runpod" && !runpod_url.is_empty() {
        let url = reqwest::Url::parse(&runpod_url).map_err(|_| {
            AppError::InvalidInput("RunPod portrait URLs must be valid HTTPS URLs".into())
        })?;
        if url.scheme() != "https"
            || url.username() != ""
            || url.password().is_some()
            || url.host_str().is_none()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AppError::InvalidInput(
                "RunPod portrait URLs must use HTTPS without credentials or query data".into(),
            ));
        }
    }
    Ok(PortraitPipelineConfig {
        enabled,
        provider: provider.clone(),
        provider_status: if provider == "disabled" {
            "not_selected".into()
        } else {
            status
        },
        workflow_repository: REPOSITORY.into(),
        workflow_branch: BRANCH.into(),
        workflow_commit,
        preferred_workflow,
        local_comfyui_root: string_field(draft.unwrap_or(&Value::Null), "localComfyuiRoot")
            .unwrap_or_default(),
        local_server_url,
        runpod_url,
        runpod_workspace: string_field(draft.unwrap_or(&Value::Null), "runpodWorkspace")
            .unwrap_or_else(|| "/workspace/comfyui-hoi4-portraits".into()),
        mcp_registered: provider == "cloud" && enabled,
    })
}

pub(crate) fn image_generation_guidance_artifact(
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<Option<GeneratedArtifact>, AppError> {
    Ok(
        image_generation_guidance_content(manifest, selection)?.map(|content| GeneratedArtifact {
            component_id: "image_generation.guidance".into(),
            destination: IMAGE_GUIDANCE_REFERENCE.into(),
            expected_sha256: sha256_bytes(content.as_bytes()),
            content,
            external: false,
            bytes: None,
        }),
    )
}

pub(crate) fn image_generation_config_artifact(
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<Option<GeneratedArtifact>, AppError> {
    Ok(
        image_generation_config_content(manifest, selection)?.map(|content| GeneratedArtifact {
            component_id: "image_generation.config".into(),
            destination: ".hoi4-setup/image_generation.json".into(),
            expected_sha256: sha256_bytes(content.as_bytes()),
            content,
            external: false,
            bytes: None,
        }),
    )
}

pub(crate) fn image_client_config_path(environment: &str) -> Option<&'static str> {
    coding_environment::image_and_live_config_path(environment)
}

pub(crate) fn image_client_config_ownership(
    exists_before_setup: bool,
    prior_ownership: Option<Ownership>,
) -> Ownership {
    prior_ownership.unwrap_or(if exists_before_setup {
        Ownership::Merged
    } else {
        Ownership::Generated
    })
}

pub(crate) fn merge_existing_image_client_config(
    root: &Path,
    destination: &str,
    incoming: &[u8],
) -> Result<Vec<u8>, AppError> {
    let path = safe_join(root, destination)?;
    let local_bytes = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if crate::security::is_link_metadata(&metadata) || !metadata.is_file() {
                return Err(AppError::PathSecurity(format!(
                    "coding client configuration is not a regular file: {destination}"
                )));
            }
            crate::flatten::read_regular_file_no_follow_under_root(root, destination)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(incoming.to_vec()),
        Err(error) => {
            return Err(AppError::Transaction(format!(
                "could not inspect coding client configuration {destination}: {error}"
            )))
        }
    };
    merge_image_client_config(destination, incoming, Some(&local_bytes))
}

/// The generated coding-client configuration files for the selected clients.
/// `launch` supplies the command that starts the installed MCP package; the
/// desktop and a server differ in how they find a Node runtime.
#[allow(clippy::too_many_arguments)]
pub(crate) fn image_client_config_artifacts_with_launch(
    root: &Path,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    already_selected_destinations: &[String],
    live_selection: &LiveTestingSelection,
    platform: Platform,
    launch: &ClientLaunch<'_>,
) -> Result<Vec<GeneratedArtifact>, AppError> {
    let mut artifacts = Vec::new();
    for environment in coding_environment::selected_environment_ids(coding_environments) {
        let Some(destination) = image_client_config_path(&environment) else {
            continue;
        };
        let destination_key = canonical_relative_key(destination)
            .map_err(|error| AppError::Source(error.to_string()))?;
        if already_selected_destinations.iter().any(|path| {
            canonical_relative_key(path).is_ok_and(|selected| selected == destination_key)
        }) {
            continue;
        }
        let path = safe_join(root, destination)?;
        let source_bytes = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if crate::security::is_link_metadata(&metadata) || !metadata.is_file() {
                    return Err(AppError::PathSecurity(format!(
                        "coding client configuration is not a regular file: {destination}"
                    )));
                }
                crate::flatten::read_regular_file_no_follow_under_root(root, destination)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                image_client_config_default(&environment).to_vec()
            }
            Err(error) => {
                return Err(AppError::Transaction(format!(
                    "could not inspect coding client configuration {destination}: {error}"
                )))
            }
        };
        let Some(adapted) = image_client_config_update(
            &environment,
            destination,
            &source_bytes,
            manifest,
            selection,
            coding_environments,
            live_selection,
            platform,
            cfg!(windows),
            launch,
        )?
        else {
            continue;
        };
        let content = String::from_utf8(adapted).map_err(|error| {
            AppError::Source(format!(
                "coding client configuration is not valid UTF-8: {error}"
            ))
        })?;
        artifacts.push(GeneratedArtifact {
            component_id: format!("image_generation.client_config.{environment}"),
            destination: destination.into(),
            expected_sha256: sha256_bytes(content.as_bytes()),
            content,
            external: false,
            bytes: None,
        });
    }
    Ok(artifacts)
}

pub(crate) fn choice_from_state(state: &Value) -> Option<String> {
    string_field(state, "conflictChoice")
}

/// Whether a plan registers the project with the HOI4 launcher, which lives
/// outside the project root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LauncherPolicy {
    /// The desktop registers a new project in the user's mod folder, so a new
    /// project must name its launcher descriptor there.
    Required,
    /// The plan may write only inside the project root, so a state that names
    /// a launcher descriptor is refused.
    Refused,
}

// Only the desktop reads the root from the state or requires a launcher.
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
pub(crate) fn project_identity_from_state(
    state: &Value,
    root: &Path,
) -> Result<ProjectIdentity, AppError> {
    project_identity_with_launcher(state, root, LauncherPolicy::Required)
}

pub(crate) fn project_identity_with_launcher(
    state: &Value,
    root: &Path,
    launcher_policy: LauncherPolicy,
) -> Result<ProjectIdentity, AppError> {
    let identity = state
        .pointer("/identity")
        .ok_or_else(|| AppError::InvalidInput("project identity is missing".into()))?;
    let field = |name: &str| {
        identity
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let launcher = field("launcherDescriptorPath");
    if launcher_policy == LauncherPolicy::Refused && !launcher.is_empty() {
        return Err(AppError::PathSecurity(format!(
            "the plan writes outside the project root: {launcher}"
        )));
    }
    if launcher_policy == LauncherPolicy::Required
        && state.get("mode").and_then(Value::as_str) == Some("new")
        && launcher.is_empty()
    {
        return Err(AppError::InvalidInput(
            "choose the HOI4 user mod directory before creating a launcher-ready project".into(),
        ));
    }
    let identity = ProjectIdentity {
        display_name: field("displayName"),
        project_id: field("projectId"),
        author: field("author"),
        version: field("version"),
        supported_game_version: field("supportedGameVersion"),
        project_root: root.to_path_buf(),
        default_branch: field("defaultBranch"),
        script_prefix: identity
            .get("scriptPrefix")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        primary_namespace: identity
            .get("primaryNamespace")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        descriptor_tags: identity
            .get("descriptorTags")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        launcher_descriptor_path: (!launcher.is_empty()).then(|| PathBuf::from(launcher)),
    };
    crate::descriptors::validate_launcher_destination(&identity)?;
    if launcher_policy == LauncherPolicy::Required
        && state.get("mode").and_then(Value::as_str) == Some("new")
    {
        let launcher_parent = identity
            .launcher_descriptor_path
            .as_deref()
            .and_then(Path::parent)
            .ok_or_else(|| AppError::InvalidInput("launcher descriptor has no parent".into()))?;
        let launcher_parent = validate_project_root(launcher_parent)?;
        let project_parent = root
            .parent()
            .ok_or_else(|| AppError::InvalidInput("project root has no parent".into()))?;
        let project_parent = validate_project_root(project_parent)?;
        if !same_project_root(&launcher_parent, &project_parent) {
            return Err(AppError::InvalidInput(
                "project folder and launcher descriptor must use the same HOI4 mod directory"
                    .into(),
            ));
        }
    }
    Ok(identity)
}

pub(crate) fn generated_state(state: &Value) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mesh_selected = state
        .get("meshSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mesh_key_status = string_field(state, "meshKeyStatus").unwrap_or_default();
    result.insert(
        "workflow.3d".into(),
        if !mesh_selected {
            "not_selected".into()
        } else if matches!(mesh_key_status.as_str(), "present" | "verified") {
            "selected_pending".into()
        } else {
            "incomplete".into()
        },
    );
    result.insert(
        "workflow.super_events".into(),
        if state
            .get("superEventsSelected")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "ready".into()
        } else {
            "not_selected".into()
        },
    );
    let portrait_enabled = state
        .pointer("/portraitPipeline/enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && state
            .pointer("/portraitPipeline/provider")
            .and_then(Value::as_str)
            .is_some_and(|provider| provider != "disabled");
    let portrait_status = state
        .pointer("/portraitPipeline/providerStatus")
        .and_then(Value::as_str)
        .unwrap_or("not_selected");
    result.insert(
        "workflow.portraits".into(),
        if !portrait_enabled {
            "not_selected".into()
        } else if portrait_status == "ready" {
            "ready".into()
        } else if portrait_status == "unsupported_platform" {
            "unsupported_platform".into()
        } else {
            "incomplete".into()
        },
    );
    result
}

pub(crate) fn seed_manifest_optional_states(
    optional_workflows: &mut BTreeMap<String, String>,
    manifest: &RemoteManifest,
) {
    for component in manifest
        .components
        .iter()
        .filter(|component| component.optional)
    {
        optional_workflows
            .entry(component.id.clone())
            .or_insert_with(|| "not_selected".into());
    }
}

pub(crate) fn selected_folder_profile(state: &Value) -> Result<Vec<String>, AppError> {
    let defaults = [
        "common",
        "events",
        "gfx",
        "interface",
        "localisation/english",
        "docs",
    ];
    let values = state
        .get("folderProfile")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|value| (*value).into()).collect());
    crate::codex::validate_folder_profile_paths(&values)
}

pub(crate) fn transaction_directories(state: &Value, mode: &str) -> Result<Vec<String>, AppError> {
    let mut directories = if mode == "new" {
        selected_folder_profile(state)?
    } else {
        Vec::new()
    };
    if !directories.iter().any(|directory| directory == ".tmp") {
        directories.push(".tmp".into());
    }
    Ok(directories)
}

pub(crate) fn ai_optimization_profile(provider: &str) -> String {
    ai::profile(provider)
        .map(|profile| profile.optimization_profile)
        .unwrap_or_else(crate::models::default_ai_optimization_profile)
}

pub(crate) fn project_readme(
    identity: &ProjectIdentity,
    description: &str,
    _provider: &str,
    _model: &str,
    _reasoning_effort: &str,
) -> Result<String, AppError> {
    hoi4_setup_core::project_files::project_readme(identity, description)
}

pub(crate) fn git_setup_from_state(
    state: &Value,
    root: &Path,
) -> Result<Option<crate::git::GitSetup>, AppError> {
    let mode = match string_field(state, "gitMode").as_deref() {
        Some("initialize") => crate::git::GitMode::Initialize,
        Some("preserve") => crate::git::GitMode::Preserve,
        _ => crate::git::GitMode::Skip,
    };
    let branch = string_field(state, "gitBranch")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "main".into());
    let setup = crate::git::GitSetup {
        mode,
        branch,
        initial_commit: state
            .get("initialCommit")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        remote_name: string_field(state, "gitRemoteName").filter(|value| !value.is_empty()),
        remote_url: string_field(state, "gitRemoteUrl").filter(|value| !value.is_empty()),
        push_approved: false,
    };
    crate::git::plan_git(&setup, &crate::git::read_git_head(root), &[])?;
    Ok((mode != crate::git::GitMode::Skip).then_some(setup))
}

/// Native client MCP files are merged with the project's local content so
/// user-owned servers and settings survive install, update, and repair.
pub(crate) fn is_image_generation_client_config_path(path: &str) -> bool {
    coding_environment::is_native_mcp_config_path(path)
}

pub(crate) fn image_generation_credential_field_supported(provider: &str, field: &str) -> bool {
    match provider {
        "openai_api"
        | "gemini_api"
        | "fal_ai"
        | "replicate"
        | "black_forest_labs"
        | "stability_ai"
        | "ideogram"
        | "recraft"
        | "leonardo"
        | "xai_grok"
        | "together_ai"
        | "openrouter"
        | "huggingface_inference" => field == "api_key",
        "adobe_firefly" => matches!(field, "ims_token" | "service_key"),
        "vertex_imagen" => field == "service_account",
        "microsoft_foundry" => matches!(field, "resource_url" | "api_key"),
        "aws_bedrock" => matches!(field, "access_key" | "secret_key"),
        _ => false,
    }
}
