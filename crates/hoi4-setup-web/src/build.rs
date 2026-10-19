//! Build the reviewed export: adapt every verified source file with the same
//! pipeline the desktop plan uses, add the generated project files, compare
//! the result with the dropped folder and its installation lock, and list
//! the checks the user still has to finish.

use crate::export::{BaseSource, Check, Export};
use crate::scan::{Project, STATE_PATH};
use crate::source::{Chosen, Source};
use base64::Engine;
use hoi4_setup_core::adapt::{
    adapt_codex_config, adapt_selected_source, adapt_super_events_destination, AdaptContext,
    GameFolders, ProjectConventions,
};
use hoi4_setup_core::adapters::ProjectFs;
use hoi4_setup_core::coding_environment::{
    definitions_for_platform, image_and_live_config_path, is_native_mcp_config_path,
    selected_environment_ids, serves_environment,
};
use hoi4_setup_core::context::{context_copies, document_from_bytes, DocumentCopy, ImageCopy};
use hoi4_setup_core::descriptors::{
    launcher_descriptor_text, parse_descriptor, placeholder_thumbnail_png, render_descriptor_mod,
    validate_project_id, validate_thumbnail_png,
};
use hoi4_setup_core::export::{
    plan_export, plan_maintenance, ExportAction, ExportLayout, IncomingFile, LockInputs,
    MAX_EXPORT_FILE_BYTES,
};
use hoi4_setup_core::free_assistant::{
    free_assistant, FREE_AUTH_MODE, FREE_PROVIDER_ID, OPENROUTER_ENDPOINT, OPENROUTER_PROVIDER_ID,
};
use hoi4_setup_core::image_generation::{
    adapt_image_generation_client_config_with_launch, adapt_image_generation_guidance,
    image_client_config_default, image_client_config_update, image_generation_config_content,
    image_generation_guidance_content, image_generation_provider_rows, merge_image_client_config,
    portable_client_launch, IMAGE_GUIDANCE_REFERENCE,
};
use hoi4_setup_core::manifest::wiki_install_metadata;
use hoi4_setup_core::memory_fs::MemoryFs;
use hoi4_setup_core::models::*;
use hoi4_setup_core::project_files::{merge_gitignore, project_readme};
use hoi4_setup_core::text::sha256_bytes;
use hoi4_setup_core::zip::ZipEntry;
use hoi4_setup_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

/// The lock's identifier rule for a script prefix or namespace.
fn regex_lite_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
        && value.len() <= 64
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

/// Where the image runtime reads the selected providers' settings.
const IMAGE_CONFIG_PATH: &str = ".hoi4-setup/image_generation.json";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportOptions {
    /// `update`, `repair`, or `change` for a folder that already has a setup.
    #[serde(default)]
    maintenance: Option<String>,
    #[serde(default)]
    identity: Option<IdentityInput>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    conventions: ConventionsInput,
    #[serde(default)]
    layout: Option<ExportLayout>,
    installed_at: String,
    /// Base64 PNG rendered from an image the user chose.
    #[serde(default)]
    thumbnail_png: Option<String>,
    /// The browser assistant that made suggestions.
    #[serde(default)]
    assistant: Option<AssistantProvenance>,
    /// Documents to keep under `docs/reference/`.
    #[serde(default)]
    context_documents: Vec<ContextCopyInput>,
    /// Visual references to keep under `docs/visual_direction/`, rendered by
    /// the page as PNG or JPEG.
    #[serde(default)]
    context_images: Vec<ContextCopyInput>,
}

#[derive(Debug, Deserialize)]
struct ContextCopyInput {
    name: String,
    /// Base64 bytes.
    data: String,
}

/// Non-secret provenance of the suggestions the user reviewed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantProvenance {
    provider: String,
    model: String,
    reasoning_effort: String,
    endpoint: Option<String>,
    optimization_profile: String,
}

/// Hosted routes the web edition can reach from the browser, plus a local
/// model server on this computer.
const WEB_ASSISTANT_PROVIDERS: &[&str] = &[
    "free",
    "openrouter",
    "claude",
    "gemini",
    "grok",
    "deepseek",
    "glm",
    "local",
];

pub(crate) fn assistant_provenance(
    assistant: Option<AssistantProvenance>,
) -> Result<AssistantProvenance, AppError> {
    // Every web setup is prepared with a setup assistant, so an export always
    // names the reviewed route that filled in the details.
    let Some(assistant) = assistant else {
        return Err(AppError::InvalidInput(
            "ask the setup assistant for the project details before exporting".into(),
        ));
    };
    let valid_text = |value: &str| {
        !value.trim().is_empty()
            && value.len() <= 256
            && !value.chars().any(char::is_control)
            && hoi4_setup_core::text::redact_secrets(value, &[]) == value
    };
    if !WEB_ASSISTANT_PROVIDERS.contains(&assistant.provider.as_str())
        || !valid_text(&assistant.model)
        || !valid_text(&assistant.optimization_profile)
        || !matches!(
            assistant.reasoning_effort.as_str(),
            "none" | "low" | "medium" | "high" | "xhigh" | "max"
        )
        || !assistant.endpoint.as_deref().is_some_and(|endpoint| {
            valid_text(endpoint)
                && if assistant.provider == "local" {
                    endpoint.starts_with("http://127.0.0.1:")
                        || endpoint.starts_with("http://localhost:")
                } else if assistant.provider == FREE_PROVIDER_ID {
                    free_assistant().is_some_and(|free| {
                        free.endpoint == endpoint && free.model == assistant.model
                    })
                } else if assistant.provider == OPENROUTER_PROVIDER_ID {
                    endpoint == OPENROUTER_ENDPOINT
                } else {
                    endpoint.starts_with("https://")
                }
        })
    {
        return Err(AppError::InvalidInput(
            "the assistant provenance is not a reviewed browser route".into(),
        ));
    }
    Ok(assistant)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityInput {
    display_name: String,
    project_id: String,
    #[serde(default)]
    author: String,
    #[serde(default = "default_version")]
    version: String,
    #[serde(default = "default_game_version")]
    supported_game_version: String,
    #[serde(default)]
    script_prefix: Option<String>,
    #[serde(default)]
    primary_namespace: Option<String>,
    #[serde(default)]
    descriptor_tags: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".into()
}

fn default_game_version() -> String {
    "1.17.*".into()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConventionsInput {
    #[serde(default)]
    agents_profile: String,
    #[serde(default)]
    localisation_convention: String,
    #[serde(default)]
    documentation_convention: String,
}

/// Everything needed to adapt one source file, owned so a merge base can be
/// adapted the same way later.
#[derive(Clone)]
pub(crate) struct Pipeline {
    identity: ProjectIdentity,
    conventions: ProjectConventions,
    folders: GameFolders,
    chosen: Chosen,
    manifest: RemoteManifest,
}

impl Pipeline {
    pub(crate) fn identity(&self) -> &ProjectIdentity {
        &self.identity
    }

    /// The bytes the setup installs for one source file; `local` is the
    /// folder's current file, whose own MCP servers are kept.
    pub(crate) fn adapt(
        &self,
        component_id: &str,
        destination: &str,
        bytes: &[u8],
        local: Option<&[u8]>,
    ) -> Result<Vec<u8>, AppError> {
        let components = &self.chosen.components;
        let super_events_selected = components.iter().any(|id| id == "workflow.super_events");
        let mcp_selected = components.iter().any(|id| id == "mcp.hoi4_agent_tools");
        let context = AdaptContext {
            identity: &self.identity,
            conventions: &self.conventions,
            folders: &self.folders,
            super_events_selected,
            portrait: &self.chosen.portrait,
            selected_components: components,
        };
        let codex = |bytes: &[u8]| {
            adapt_codex_config(
                bytes,
                mcp_selected,
                None,
                super_events_selected,
                &self.chosen.portrait,
            )
        };
        let adapted = adapt_selected_source(component_id, destination, bytes, &context, &codex)?;
        let adapted = if is_native_mcp_config_path(destination) {
            merge_image_client_config(destination, &adapted, local)?
        } else {
            adapted
        };
        let adapted =
            adapt_image_generation_guidance(&adapted, &self.manifest, &self.chosen.image)?;
        adapt_image_generation_client_config_with_launch(
            &adapted,
            destination,
            &self.manifest,
            &self.chosen.image,
            &self.chosen.clients,
            &self.chosen.live,
            self.chosen.platform,
            self.chosen.platform == Platform::Windows,
            &portable_client_launch,
        )
    }
}

fn generated(path: &str, component_id: &str, bytes: Vec<u8>, ownership: Ownership) -> IncomingFile {
    IncomingFile {
        path: path.into(),
        component_id: component_id.into(),
        source_sha256: sha256_bytes(&bytes),
        source_size: bytes.len() as u64,
        bytes,
        ownership,
        source_path: format!("generated:{path}"),
        executable: false,
    }
}

fn decode(data: &str, label: &str) -> Result<Vec<u8>, AppError> {
    base64::engine::general_purpose::STANDARD
        .decode(
            data.split_once(";base64,")
                .map_or(data, |(_, data)| data)
                .trim(),
        )
        .map_err(|_| AppError::InvalidInput(format!("{label} is not valid base64")))
}

fn read_local(fs: Option<&MemoryFs>, path: &str) -> Option<Vec<u8>> {
    fs.and_then(|fs| fs.read(path, MAX_EXPORT_FILE_BYTES).ok())
}

/// The identity a maintenance run keeps: the lock's IDs and the descriptor's
/// name, as the desktop's maintenance does.
fn maintenance_identity(lock: &InstallationLock, project: &Project) -> ProjectIdentity {
    ProjectIdentity {
        display_name: project
            .descriptor_name()
            .unwrap_or_else(|| lock.project_id.clone()),
        project_id: lock.project_id.clone(),
        author: String::new(),
        version: "0.1.0".into(),
        supported_game_version: "*".into(),
        project_root: PathBuf::from(format!("mod/{}", lock.project_id)),
        default_branch: "main".into(),
        script_prefix: lock
            .script_prefix
            .clone()
            .or_else(|| project.descriptor_field("script_prefix")),
        primary_namespace: lock
            .primary_namespace
            .clone()
            .or_else(|| project.descriptor_field("namespace")),
        descriptor_tags: Vec::new(),
        launcher_descriptor_path: None,
    }
}

/// Conventions the desktop recorded for a new mod; empty when absent.
fn persisted_conventions(state: Option<&Value>) -> ProjectConventions {
    let field = |name: &str| {
        state
            .and_then(|state| state.pointer(&format!("/project_conventions/{name}")))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    ProjectConventions {
        agents_profile: field("agents_profile"),
        localisation_convention: field("localisation_convention"),
        documentation_convention: field("documentation_convention"),
    }
}

/// Generated project files a maintenance run keeps from the lock, so a
/// missing descriptor, README, or project state comes back exactly as it was
/// written.
fn generated_from_lock(lock: &InstallationLock) -> Vec<IncomingFile> {
    lock.files
        .iter()
        .filter(|file| {
            !file.external
                && file.source_path.starts_with("generated:")
                && (file.component_id.starts_with("project.") || file.component_id == "git.ignore")
        })
        .filter_map(|file| {
            let bytes = file
                .generated_bytes
                .clone()
                .or_else(|| file.generated_content.clone().map(String::into_bytes))?;
            Some(generated(
                &file.path,
                &file.component_id,
                bytes,
                file.ownership,
            ))
        })
        .collect()
}

fn state_document(
    identity: &ProjectIdentity,
    chosen: &Chosen,
    conventions: &ProjectConventions,
    assistant: &AssistantProvenance,
) -> Result<Vec<u8>, AppError> {
    let reasoning_effort = if assistant.reasoning_effort == "none" {
        "high"
    } else {
        assistant.reasoning_effort.as_str()
    };
    Ok(serde_json::to_string_pretty(&json!({
        "schema_version": "1.0.0",
        "project_id": identity.project_id,
        "project_root": identity.project_root.display().to_string(),
        "platform": chosen.platform.manifest_name(),
        "wizard": {
            "current_step": "ready",
            "completed_steps": ["welcome", "description", "identity", "environments", "components", "workflows"]
        },
        "preferences": { "telemetry": false },
        "ai": {
            "provider": assistant.provider,
            "model": assistant.model,
            "reasoning_effort": reasoning_effort,
            "optimization_profile": assistant.optimization_profile
        },
        "codex": {
            "integration": "provider_api",
            "auth_mode": match assistant.provider.as_str() {
                "local" => "local_endpoint",
                FREE_PROVIDER_ID => FREE_AUTH_MODE,
                _ => "api_key",
            },
            "auth_status": "configured",
            "analysis_required": true,
            "analysis_status": "confirmed",
            "account_values_persisted": false
        },
        "coding_environments": {
            "primary": chosen.clients.primary,
            "additional": chosen.clients.additional
        },
        "project_conventions": {
            "agents_profile": conventions.agents_profile,
            "localisation_convention": conventions.localisation_convention,
            "documentation_convention": conventions.documentation_convention
        },
        "credential_references": [],
        "portrait_pipeline": chosen.portrait,
        "image_generation": chosen.image,
        "live_testing": chosen.live
    }))?
    .into_bytes())
}

pub(crate) fn prepare(
    source: &Source,
    request_json: &str,
    project: Option<Project>,
) -> Result<Export, AppError> {
    let options: ExportOptions = serde_json::from_str(request_json)?;
    let chosen = source
        .chosen
        .clone()
        .ok_or_else(|| AppError::InvalidInput("choose the components first".into()))?;
    let missing = source
        .files
        .iter()
        .filter(|file| !source.verified.contains_key(&file.source_path))
        .count();
    if missing > 0 {
        return Err(AppError::Source(format!(
            "{missing} selected files have not been downloaded and verified yet"
        )));
    }
    let maintenance = match options.maintenance.as_deref() {
        None => None,
        Some(mode @ ("update" | "repair" | "change")) => Some(mode),
        Some(_) => return Err(AppError::InvalidInput("unknown maintenance mode".into())),
    };
    let lock = project.as_ref().and_then(|project| project.lock.clone());
    if maintenance.is_some() && lock.is_none() {
        return Err(AppError::InvalidInput(
            "this folder has no setup to maintain".into(),
        ));
    }
    if let Some(reason) = lock.as_ref().and_then(desktop_only_reason) {
        return Err(AppError::InvalidInput(reason.into()));
    }
    // The web edition never saves keys, but keeps the references a desktop
    // install recorded for providers that stay selected.
    let mut chosen = chosen;
    if let Some(recorded) = lock
        .as_ref()
        .and_then(|lock| lock.image_generation.as_ref())
    {
        let selected = std::iter::once(chosen.image.primary_provider.clone())
            .chain(chosen.image.additional_providers.iter().cloned())
            .collect::<Vec<_>>();
        chosen.image.credential_references = recorded
            .credential_references
            .iter()
            .filter(|(key, _)| {
                key.split_once('.')
                    .is_some_and(|(provider, _)| selected.iter().any(|id| id == provider))
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if selected.iter().any(|id| id == &recorded.primary_provider)
            || recorded
                .additional_providers
                .iter()
                .any(|id| selected.contains(id))
        {
            chosen.image.installed_package_tree_sha256 =
                recorded.installed_package_tree_sha256.clone();
            chosen.image.installed_package_lock_sha256 =
                recorded.installed_package_lock_sha256.clone();
        }
    }
    let (identity, conventions, provenance) = match (maintenance, lock.as_ref(), project.as_ref()) {
        (Some(_), Some(lock), Some(project)) => (
            maintenance_identity(lock, project),
            persisted_conventions(project.state.as_ref()),
            None,
        ),
        _ => {
            let input = options
                .identity
                .ok_or_else(|| AppError::InvalidInput("the project identity is missing".into()))?;
            validate_project_id(&input.project_id)?;
            let identifier = regex_lite_identifier;
            for (value, label) in [
                (&input.script_prefix, "script prefix"),
                (&input.primary_namespace, "event namespace"),
            ] {
                if value.as_deref().is_some_and(|value| !identifier(value)) {
                    return Err(AppError::InvalidInput(format!(
                        "the {label} must use lowercase letters, digits, and underscores and start with a letter or underscore"
                    )));
                }
            }
            let identity = ProjectIdentity {
                display_name: input.display_name,
                project_root: PathBuf::from(format!("mod/{}", input.project_id)),
                project_id: input.project_id,
                author: input.author,
                version: input.version,
                supported_game_version: input.supported_game_version,
                default_branch: "main".into(),
                script_prefix: input.script_prefix,
                primary_namespace: input.primary_namespace,
                descriptor_tags: input.descriptor_tags,
                launcher_descriptor_path: None,
            };
            let conventions = ProjectConventions {
                agents_profile: options.conventions.agents_profile,
                localisation_convention: options.conventions.localisation_convention,
                documentation_convention: options.conventions.documentation_convention,
            };
            (
                identity,
                conventions,
                Some(assistant_provenance(options.assistant)?),
            )
        }
    };
    let pipeline = Pipeline {
        identity,
        conventions,
        folders: GameFolders {
            game_dir: "[HOI4_GAME_DIR]".into(),
            workshop_dir: "[STEAM_WORKSHOP_DIR]".into(),
        },
        chosen: chosen.clone(),
        manifest: source.manifest.clone(),
    };
    let local = project.as_ref().map(|project| &project.fs);
    let mut incoming = Vec::new();
    let mut destinations = HashSet::new();
    for file in &source.files {
        let destination = adapt_super_events_destination(
            &file.component_id,
            &file.destination,
            &pipeline.identity,
        )?;
        if !destinations.insert(destination.to_lowercase()) {
            continue;
        }
        let raw = &source.verified[&file.source_path];
        let local_bytes = read_local(local, &destination);
        let bytes = pipeline.adapt(
            &file.component_id,
            &destination,
            raw,
            local_bytes.as_deref(),
        )?;
        incoming.push(IncomingFile {
            path: destination,
            component_id: file.component_id.clone(),
            bytes,
            ownership: file.ownership,
            source_path: file.source_path.clone(),
            source_sha256: sha256_bytes(raw),
            source_size: raw.len() as u64,
            executable: file.executable,
        });
    }
    let identity = &pipeline.identity;
    match (maintenance, lock.as_ref()) {
        (Some(_), Some(lock)) => {
            // The project state follows the current choices; everything else
            // comes back exactly as it was written.
            incoming.extend(generated_from_lock(lock).into_iter().map(|mut file| {
                if file.component_id == "project.state" {
                    file.bytes = refreshed_state(&file.bytes, &chosen);
                    file.source_sha256 = sha256_bytes(&file.bytes);
                    file.source_size = file.bytes.len() as u64;
                }
                file
            }))
        }
        _ => {
            let provenance = provenance.as_ref().expect("a setup has provenance");
            incoming.push(generated(
                "descriptor.mod",
                "project.descriptor",
                render_descriptor_mod(identity)?.into_bytes(),
                Ownership::Generated,
            ));
            let thumbnail = match options.thumbnail_png.as_deref() {
                Some(encoded) => {
                    let bytes = decode(encoded, "the thumbnail")?;
                    validate_thumbnail_png(&bytes)?;
                    Some(bytes)
                }
                None if read_local(local, "thumbnail.png").is_none() => {
                    Some(placeholder_thumbnail_png()?)
                }
                None => None,
            };
            if let Some(thumbnail) = thumbnail {
                incoming.push(generated(
                    "thumbnail.png",
                    "project.thumbnail",
                    thumbnail,
                    Ownership::Generated,
                ));
            }
            incoming.push(generated(
                "README.md",
                "project.readme",
                project_readme(identity, &options.description)?.into_bytes(),
                Ownership::Generated,
            ));
            let existing_gitignore =
                read_local(local, ".gitignore").and_then(|bytes| String::from_utf8(bytes).ok());
            incoming.push(generated(
                ".gitignore",
                "git.ignore",
                merge_gitignore(existing_gitignore.as_deref()).into_bytes(),
                Ownership::Generated,
            ));
            if local.is_none() {
                incoming.push(generated(
                    STATE_PATH,
                    "project.state",
                    state_document(identity, &chosen, &pipeline.conventions, provenance)?,
                    Ownership::Generated,
                ));
            }
            let documents = options
                .context_documents
                .iter()
                .map(|document| {
                    decode(&document.data, "a context document").map(|bytes| (document, bytes))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let images = options
                .context_images
                .iter()
                .map(|image| decode(&image.data, "a visual reference").map(|bytes| (image, bytes)))
                .collect::<Result<Vec<_>, _>>()?;
            // Copyability is the core's call, never the page's.
            let checked = documents
                .iter()
                .map(|(document, bytes)| {
                    document_from_bytes(0, &document.name, bytes).map(|parsed| parsed.copyable)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let copies = context_copies(
                &documents
                    .iter()
                    .zip(&checked)
                    .map(|((document, bytes), copyable)| DocumentCopy {
                        name: &document.name,
                        bytes,
                        copyable: *copyable,
                    })
                    .collect::<Vec<_>>(),
                &images
                    .iter()
                    .map(|(image, bytes)| ImageCopy {
                        name: &image.name,
                        bytes,
                    })
                    .collect::<Vec<_>>(),
            )?;
            for copy in copies {
                incoming.push(generated(
                    &copy.destination,
                    copy.component_id,
                    copy.bytes,
                    Ownership::Generated,
                ));
            }
        }
    }
    // Image generation: the runtime's settings, the provider reference, and
    // the routes in each selected client's MCP file not already shipped.
    if let Some(content) = image_generation_config_content(&source.manifest, &chosen.image)? {
        incoming.push(generated(
            IMAGE_CONFIG_PATH,
            "image_generation.config",
            content.into_bytes(),
            Ownership::Generated,
        ));
    }
    if let Some(content) = image_generation_guidance_content(&source.manifest, &chosen.image)? {
        incoming.push(generated(
            IMAGE_GUIDANCE_REFERENCE,
            "image_generation.guidance",
            content.into_bytes(),
            Ownership::Generated,
        ));
    }
    let shipped = incoming
        .iter()
        .map(|file| file.path.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    for environment in selected_environment_ids(&chosen.clients) {
        let Some(destination) = image_and_live_config_path(&environment) else {
            continue;
        };
        if shipped.contains(&destination.to_ascii_lowercase()) {
            continue;
        }
        let existing = read_local(local, destination);
        let source_bytes = existing
            .clone()
            .unwrap_or_else(|| image_client_config_default(&environment).to_vec());
        if let Some(bytes) = image_client_config_update(
            &environment,
            destination,
            &source_bytes,
            &source.manifest,
            &chosen.image,
            &chosen.clients,
            &chosen.live,
            chosen.platform,
            chosen.platform == Platform::Windows,
            &portable_client_launch,
        )? {
            incoming.push(generated(
                destination,
                &format!("image_generation.client_config.{environment}"),
                bytes,
                lock.as_ref()
                    .and_then(|lock| {
                        lock.files
                            .iter()
                            .find(|file| !file.external && file.path == destination)
                            .map(|file| file.ownership)
                    })
                    .unwrap_or(if existing.is_some() {
                        Ownership::Merged
                    } else {
                        Ownership::Generated
                    }),
            ));
        }
    }
    let entries = match (lock.as_ref(), local) {
        (Some(lock), Some(fs)) => plan_maintenance(&incoming, fs, lock, &removable)?,
        _ => plan_export(&incoming, local.map(|fs| fs as &dyn ProjectFs))?,
    };
    // A changed managed file can merge against the version installed before.
    let mut base_sources = BTreeMap::new();
    if let Some(lock) = lock.as_ref() {
        for entry in entries.iter().filter(|entry| entry.mergeable) {
            if let Some(locked) = lock.files.iter().find(|file| {
                !file.external
                    && file.path == entry.path
                    && hoi4_setup_core::text::normalize_relative_path(&file.source_path).is_ok()
                    && hoi4_setup_core::manifest::validate_commit(&file.source_revision).is_ok()
            }) {
                base_sources.insert(
                    entry.path.clone(),
                    BaseSource {
                        component_id: locked.component_id.clone(),
                        revision: locked.source_revision.clone(),
                        source_path: locked.source_path.clone(),
                        sha256: locked.source_sha256.clone(),
                    },
                );
            }
        }
    }
    let root_files = if local.is_none() {
        vec![ZipEntry {
            path: format!("{}.mod", identity.project_id),
            bytes: launcher_descriptor_text(identity, &format!("mod/{}", identity.project_id))?
                .into_bytes(),
        }]
    } else {
        Vec::new()
    };
    let readiness = readiness_checks(
        &incoming,
        &entries,
        local,
        &source.manifest,
        &chosen,
        lock.is_none() && local.is_none(),
    );
    let components = chosen
        .components
        .iter()
        .map(|id| LockComponent {
            id: id.clone(),
            version: None,
            state: "installed".into(),
            source_revision: Some(source.revision.clone()),
            validation: Some("not_run".into()),
        })
        .collect();
    let (ai_provider, ai_model, ai_reasoning_effort, ai_endpoint, ai_optimization_profile) =
        match (&provenance, lock.as_ref()) {
            (Some(assistant), _) => (
                assistant.provider.clone(),
                assistant.model.clone(),
                assistant.reasoning_effort.clone(),
                assistant.endpoint.clone(),
                assistant.optimization_profile.clone(),
            ),
            (None, Some(lock)) => (
                lock.ai_provider.clone(),
                lock.ai_model.clone(),
                lock.ai_reasoning_effort.clone(),
                lock.ai_endpoint.clone(),
                lock.ai_optimization_profile.clone(),
            ),
            (None, None) => unreachable!("a setup always has provenance"),
        };
    let (installed_at, updated_at) = match (maintenance, lock.as_ref()) {
        (Some(_), Some(lock)) => (lock.installed_at.clone(), Some(options.installed_at)),
        _ => (options.installed_at, None),
    };
    let lock_inputs = LockInputs {
        project_id: identity.project_id.clone(),
        script_prefix: identity.script_prefix.clone(),
        primary_namespace: identity.primary_namespace.clone(),
        installed_at,
        updated_at,
        source: LockSourceIdentity {
            repository: format!(
                "{}/{}",
                hoi4_setup_core::manifest::SOURCE_OWNER,
                hoi4_setup_core::manifest::SOURCE_NAME
            ),
            mode: source.mode,
            revision: source.revision.clone(),
            requested_ref: source.requested_ref.clone(),
            release: None,
            manifest_sha256: source.manifest_sha256.clone(),
            manifest_origin: "remote".into(),
        },
        ai_provider,
        ai_model,
        ai_reasoning_effort,
        ai_endpoint,
        ai_optimization_profile,
        primary_coding_environment: chosen.clients.primary.clone(),
        additional_coding_environments: chosen.clients.additional.clone(),
        wiki_required_pages: source.manifest.wiki.required_pages.clone(),
        wiki_metadata: wiki_install_metadata(&source.manifest),
        components,
        optional_workflows: lock
            .as_ref()
            .map(|lock| lock.optional_workflows.clone())
            .unwrap_or_default(),
        portrait_pipeline: Some(chosen.portrait.clone()),
        image_generation: source
            .manifest
            .image_generation
            .is_some()
            .then(|| chosen.image.clone()),
        live_testing: Some(chosen.live.clone()),
    };
    Ok(Export {
        incoming,
        entries,
        local: project,
        layout: options.layout.unwrap_or(ExportLayout::Overlay),
        root_folder: identity.project_id.clone(),
        root_files,
        lock: lock_inputs,
        base_pipeline: lock.as_ref().map(|lock| Pipeline {
            identity: pipeline.identity.clone(),
            conventions: pipeline.conventions.clone(),
            folders: pipeline.folders.clone(),
            chosen: chosen_from_lock(lock, chosen.platform),
            manifest: source.manifest.clone(),
        }),
        pipeline: Some(pipeline),
        base_sources,
        bases: BTreeMap::new(),
        merged: BTreeMap::new(),
        remove_setup: false,
        readiness,
    })
}

/// A managed file that is no longer selected may be removed when it is not a
/// project file and holds no user content merged into it.
fn removable(file: &LockedFile) -> bool {
    !file.component_id.starts_with("project.")
        && file.component_id != "git.ignore"
        && file.ownership != Ownership::Merged
        && !file.preserved_local
}

/// Workflows only the desktop app can rebuild; their setups are maintained
/// there, so a web run never drops them.
pub(crate) fn desktop_only_reason(lock: &InstallationLock) -> Option<&'static str> {
    if lock
        .components
        .iter()
        .any(|component| component.id == "workflow.3d")
    {
        return Some("This setup uses the 3D models workflow, which only the desktop app can maintain. Update, repair, or change it there.");
    }
    if lock
        .components
        .iter()
        .any(|component| component.id == "workflow.live_tools")
        || lock
            .live_testing
            .as_ref()
            .is_some_and(|live| live.provider == "hoi4_live_tools")
    {
        return Some("This setup uses HOI4 Live Tools, which only the desktop app can maintain. Update, repair, or change it there.");
    }
    None
}

/// The project state recorded for a new mod, with the current choices.
fn refreshed_state(bytes: &[u8], chosen: &Chosen) -> Vec<u8> {
    let Ok(mut state) = serde_json::from_slice::<Value>(bytes) else {
        return bytes.to_vec();
    };
    let Some(object) = state.as_object_mut() else {
        return bytes.to_vec();
    };
    object.insert(
        "coding_environments".into(),
        json!({ "primary": chosen.clients.primary, "additional": chosen.clients.additional }),
    );
    object.insert("portrait_pipeline".into(), json!(chosen.portrait));
    object.insert("image_generation".into(), json!(chosen.image));
    object.insert("live_testing".into(), json!(chosen.live));
    serde_json::to_string_pretty(&state)
        .map(String::into_bytes)
        .unwrap_or_else(|_| bytes.to_vec())
}

/// The choices recorded in a lock, so a merge base is adapted the way the
/// installed file was.
fn chosen_from_lock(lock: &InstallationLock, platform: Platform) -> Chosen {
    Chosen {
        clients: CodingEnvironmentSelection {
            primary: lock.primary_coding_environment.clone(),
            additional: lock.additional_coding_environments.clone(),
        },
        platform,
        image: lock
            .image_generation
            .clone()
            .unwrap_or(ImageGenerationSelection {
                primary_provider: "none".into(),
                ..Default::default()
            }),
        live: lock.live_testing.clone().unwrap_or(LiveTestingSelection {
            provider: "none".into(),
            ..Default::default()
        }),
        portrait: lock.portrait_pipeline.clone().unwrap_or_else(|| {
            hoi4_setup_core::portrait::portrait_config(None)
                .expect("the disabled portrait settings are valid")
        }),
        components: lock
            .components
            .iter()
            .map(|component| component.id.clone())
            .collect(),
    }
}

/// Managed removal: every unchanged managed file goes, the user's own and
/// changed files stay, and no lock is written.
pub(crate) fn prepare_removal(project: &Project, request_json: &str) -> Result<Export, AppError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RemovalOptions {
        #[serde(default)]
        layout: Option<ExportLayout>,
    }
    let options: RemovalOptions = serde_json::from_str(request_json)?;
    let lock = project
        .lock
        .clone()
        .ok_or_else(|| AppError::InvalidInput("this folder has no setup to remove".into()))?;
    let entries = plan_maintenance(&[], &project.fs, &lock, &removable)?;
    let removed = entries
        .iter()
        .filter(|entry| entry.action == ExportAction::Remove)
        .count();
    let lock_inputs = LockInputs {
        project_id: lock.project_id.clone(),
        script_prefix: lock.script_prefix.clone(),
        primary_namespace: lock.primary_namespace.clone(),
        installed_at: lock.installed_at.clone(),
        updated_at: None,
        source: lock.source.clone(),
        ai_provider: lock.ai_provider.clone(),
        ai_model: lock.ai_model.clone(),
        ai_reasoning_effort: lock.ai_reasoning_effort.clone(),
        ai_endpoint: lock.ai_endpoint.clone(),
        ai_optimization_profile: lock.ai_optimization_profile.clone(),
        primary_coding_environment: lock.primary_coding_environment.clone(),
        additional_coding_environments: lock.additional_coding_environments.clone(),
        wiki_required_pages: lock.wiki_required_pages.clone(),
        wiki_metadata: lock.wiki_metadata.clone().unwrap_or(WikiInstallMetadata {
            snapshot_marker: None,
            required_media_policy: "none".into(),
            source_status: "unknown".into(),
            license_status: "unknown".into(),
            repository_license_status: "unknown".into(),
            notes: Vec::new(),
        }),
        components: Vec::new(),
        optional_workflows: BTreeMap::new(),
        portrait_pipeline: None,
        image_generation: None,
        live_testing: None,
    };
    Ok(Export {
        incoming: Vec::new(),
        entries,
        local: Some(project.snapshot()),
        layout: options.layout.unwrap_or(ExportLayout::Overlay),
        root_folder: lock.project_id.clone(),
        root_files: Vec::new(),
        lock: lock_inputs,
        pipeline: None,
        base_pipeline: None,
        base_sources: BTreeMap::new(),
        bases: BTreeMap::new(),
        merged: BTreeMap::new(),
        remove_setup: true,
        readiness: vec![Check::info(
            "removal",
            "Managed files to delete",
            format!("{removed} files the setup installed and you never changed are listed in HOI4_MOD_SETUP_CHANGES.md. Your own files and files you changed stay."),
        )],
    })
}

fn readiness_checks(
    incoming: &[IncomingFile],
    entries: &[hoi4_setup_core::export::ExportEntry],
    local: Option<&MemoryFs>,
    manifest: &RemoteManifest,
    chosen: &Chosen,
    new_mod: bool,
) -> Vec<Check> {
    let file = |path: &str| -> Option<Vec<u8>> {
        incoming
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.bytes.clone())
            .or_else(|| read_local(local, path))
    };
    let mut checks = Vec::new();
    match file("descriptor.mod").and_then(|bytes| parse_descriptor(&bytes).ok()) {
        Some(descriptor) if descriptor.fields.contains_key("name") => checks.push(Check::pass(
            "descriptor",
            "Descriptor",
            "descriptor.mod names the mod for the launcher.".into(),
        )),
        _ => checks.push(Check::action(
            "descriptor",
            "Descriptor",
            "descriptor.mod is missing or has no name; the launcher will not list the mod.".into(),
        )),
    }
    match file("thumbnail.png").map(|bytes| validate_thumbnail_png(&bytes)) {
        Some(Ok(_)) => checks.push(Check::pass(
            "thumbnail",
            "Thumbnail",
            "thumbnail.png is a valid launcher image.".into(),
        )),
        _ => checks.push(Check::action(
            "thumbnail",
            "Thumbnail",
            "thumbnail.png is missing or not a valid PNG.".into(),
        )),
    }
    checks.push(if new_mod {
        Check::pass(
            "launcher",
            "Launcher file",
            "The ZIP holds the launcher file beside the mod folder.".into(),
        )
    } else {
        Check::action(
            "launcher",
            "Launcher file",
            "If the launcher does not list the mod yet, download its launcher file and put it in the Hearts of Iron IV mod folder.".into(),
        )
    });
    if file("CLAUDE.md").is_some() || file("AGENTS.md").is_some() {
        checks.push(Check::pass(
            "instructions",
            "Project instructions",
            "The modding guidance for your coding clients is included.".into(),
        ));
    }
    for environment in selected_environment_ids(&chosen.clients) {
        let name = definitions_for_platform(manifest, chosen.platform)
            .into_iter()
            .find(|definition| definition.id == environment)
            .map(|definition| definition.display_name)
            .unwrap_or_else(|| environment.clone());
        let served = incoming.iter().any(|file| {
            manifest
                .components
                .iter()
                .find(|component| component.id == file.component_id)
                .is_some_and(|component| serves_environment(component, &environment))
        });
        checks.push(if served {
            Check::pass(
                &format!("client.{environment}"),
                &name,
                format!("{name}'s instructions and settings are included."),
            )
        } else {
            Check::info(
                &format!("client.{environment}"),
                &name,
                format!("{name} reads the shared project files."),
            )
        });
    }
    if chosen
        .components
        .iter()
        .any(|id| id == "mcp.hoi4_agent_tools")
    {
        let bootstrap = manifest
            .components
            .iter()
            .find(|component| component.id == "mcp.hoi4_agent_tools.bootstrap")
            .map(|component| component.destination.path.clone());
        checks.push(Check::action(
            "mcp.hoi4_agent_tools",
            "HOI4 Agent Tools",
            match bootstrap {
                Some(path) => format!("Install the tools once by running `{path}` from the mod folder; the desktop app does this for you and checks it."),
                None => "Install HOI4 Agent Tools once; the desktop app does this for you and checks it.".into(),
            },
        ));
    }
    if let Ok(providers) = image_generation_provider_rows(manifest, &chosen.image) {
        for provider in providers {
            let id = provider
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = provider
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(id);
            let auth = provider
                .pointer("/route/auth_method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let detail = match auth {
                "api_key_vault" | "cloud_credentials" | "enterprise_credentials" => format!(
                    "{name} reads its key from the operating-system credential store. A browser cannot save it there; add it with the desktop app's image settings."
                ),
                "chatgpt_subscription" => format!("{name} needs your ChatGPT sign-in in the provider's own tool."),
                "local" => format!("{name} needs its server running on this computer."),
                "none" | "manual" => continue,
                _ => format!("Finish {name}'s own setup before generating images."),
            };
            checks.push(Check::action(&format!("image.{id}"), name, detail));
        }
    }
    if chosen.portrait.enabled {
        checks.push(Check::action(
            "portraits",
            "Portrait workflow",
            match chosen.portrait.provider.as_str() {
                "cloud" => "Authorize Comfy Cloud before making portraits.".into(),
                "local" => "Install ComfyUI and the portrait models on this computer.".into(),
                _ => "Start your RunPod pod before making portraits.".into(),
            },
        ));
    }
    if chosen.live.provider == "clausewitz_mcp" {
        checks.push(Check::action(
            "live_testing",
            "Live testing",
            "Start the Clausewitz MCP server with native Linux Hearts of Iron IV before using live tools.".into(),
        ));
    }
    let kept = entries
        .iter()
        .filter(|entry| entry.action == ExportAction::Release)
        .count();
    if kept > 0 {
        checks.push(Check::info(
            "released",
            "Files you changed",
            format!("{kept} files the setup no longer uses stay because you changed them."),
        ));
    }
    checks.push(Check::info(
        "git",
        "Version control",
        "The web edition does not create a Git repository; use the desktop app or Git yourself."
            .into(),
    ));
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use hoi4_setup_core::export::LOCK_PATH;
    use hoi4_setup_core::zip::read_zip;

    const MANIFEST: &[u8] =
        include_bytes!("../../../docs/fixtures/format6-candidate-manifest.json");
    const REVISION: &str = "0b124ad9d3121fda5eb3cc5ff365f74a28db0f8f";

    /// A source with every selected file "downloaded" as the manifest says,
    /// bypassing the network: tests replace the verified store directly.
    fn source_with(request: Value) -> Source {
        let mut source =
            Source::new(MANIFEST, REVISION, "pinned_commit", Some(REVISION.into())).unwrap();
        source.select(&request.to_string()).unwrap();
        for file in source.files.clone() {
            // Configuration files must parse; other files are plain text.
            let bytes: &[u8] = if file.destination.ends_with(".json") {
                b"{}"
            } else if file.destination.starts_with(".codex/agents/") {
                b"developer_instructions = \"Placeholder subagent.\""
            } else {
                b"# placeholder"
            };
            source
                .verified
                .insert(file.source_path.clone(), bytes.to_vec());
        }
        source
    }

    fn setup_request() -> Value {
        json!({
            "identity": { "displayName": "Iron Dawn", "projectId": "iron_dawn", "descriptorTags": ["Events"] },
            "description": "Balkan nations rise in 1936.",
            "installedAt": "2026-10-19T00:00:00Z",
            "assistant": {
                "provider": "claude",
                "model": "claude-haiku-5-5",
                "reasoningEffort": "medium",
                "endpoint": "https://api.anthropic.com/v1/messages",
                "optimizationProfile": "Claude setup analysis"
            }
        })
    }

    fn selection() -> Value {
        json!({
            "selectedComponents": [],
            "primaryCodingEnvironment": "claude_code",
            "platform": "linux",
            "imageGeneration": {"primary_provider": "openai_api"},
        })
    }

    #[test]
    fn a_new_mod_has_project_state_image_settings_and_a_valid_lock() {
        let source = source_with(selection());
        let export = prepare(&source, &setup_request().to_string(), None).unwrap();
        let paths = export
            .incoming
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&STATE_PATH));
        assert!(paths.contains(&IMAGE_CONFIG_PATH));
        assert!(paths.contains(&IMAGE_GUIDANCE_REFERENCE));
        let state: Value = serde_json::from_slice(
            &export
                .incoming
                .iter()
                .find(|file| file.path == STATE_PATH)
                .unwrap()
                .bytes,
        )
        .unwrap();
        let validator = hoi4_setup_core::schemas::validator(include_str!(
            "../../../docs/schemas/project-state.schema.json"
        ))
        .unwrap();
        if let Err(error) = validator.validate(&state) {
            panic!(
                "state does not match the schema at {}: {error}",
                error.instance_path()
            );
        }
        let archive = export.build_zip(&BTreeMap::new()).unwrap();
        let lock_entry = read_zip(&archive)
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == format!("iron_dawn/{LOCK_PATH}"))
            .unwrap();
        let lock: Value = serde_json::from_slice(&lock_entry.bytes).unwrap();
        let validator = hoi4_setup_core::schemas::validator(include_str!(
            "../../../docs/schemas/installation-lock.schema.json"
        ))
        .unwrap();
        if let Err(error) = validator.validate(&lock) {
            panic!(
                "lock does not match the schema at {}: {error}",
                error.instance_path()
            );
        }
        assert_eq!(lock["image_generation"]["primary_provider"], "openai_api");
        // Source files record the raw blob's hash, as the desktop does.
        let agents = lock["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == "AGENTS.md")
            .unwrap();
        assert_eq!(agents["source_sha256"], sha256_bytes(b"# placeholder"));
        assert!(export
            .readiness
            .iter()
            .any(|check| check.id == "image.openai_api" && check.state == "action"));
    }

    #[test]
    fn a_setup_without_reviewed_assistant_provenance_is_refused() {
        let source = source_with(selection());
        let mut request = setup_request();
        request.as_object_mut().unwrap().remove("assistant");
        assert!(prepare(&source, &request.to_string(), None).is_err());
        let mut request = setup_request();
        request["assistant"]["provider"] = json!("kimi");
        assert!(prepare(&source, &request.to_string(), None).is_err());
        // A local model must be on this computer; a hosted one must use HTTPS.
        let mut request = setup_request();
        request["assistant"]["provider"] = json!("local");
        request["assistant"]["endpoint"] = json!("https://models.example/v1");
        assert!(prepare(&source, &request.to_string(), None).is_err());
        request["assistant"]["endpoint"] = json!("http://127.0.0.1:11434/v1/chat/completions");
        request["assistant"]["reasoningEffort"] = json!("none");
        let export = prepare(&source, &request.to_string(), None).unwrap();
        assert_eq!(export.lock.ai_provider, "local");
        // OpenRouter keys go only to OpenRouter.
        let mut request = setup_request();
        request["assistant"]["provider"] = json!("openrouter");
        request["assistant"]["model"] = json!("nvidia/nemotron-3-super-120b-a12b:free");
        request["assistant"]["endpoint"] =
            json!("https://openrouter.example/api/v1/chat/completions");
        assert!(prepare(&source, &request.to_string(), None).is_err());
        request["assistant"]["endpoint"] = json!(OPENROUTER_ENDPOINT);
        assert_eq!(
            prepare(&source, &request.to_string(), None)
                .unwrap()
                .lock
                .ai_provider,
            "openrouter"
        );
        // The free route is accepted only on the checked-in relay.
        let mut request = setup_request();
        request["assistant"]["provider"] = json!("free");
        request["assistant"]["endpoint"] = json!("https://relay.example/v1/chat/completions");
        assert!(prepare(&source, &request.to_string(), None).is_err());
        if let Some(free) = free_assistant() {
            request["assistant"]["endpoint"] = json!(free.endpoint);
            request["assistant"]["model"] = json!(free.model);
            assert_eq!(
                prepare(&source, &request.to_string(), None)
                    .unwrap()
                    .lock
                    .ai_provider,
                "free"
            );
        }
    }

    #[test]
    fn maintenance_reuses_the_lock_and_removal_writes_none() {
        // Install, then maintain the result with a different client set.
        let source = source_with(selection());
        let export = prepare(&source, &setup_request().to_string(), None).unwrap();
        let archive = export.build_zip(&BTreeMap::new()).unwrap();
        let mut project = Project::new();
        for entry in read_zip(&archive).unwrap() {
            if let Some(path) = entry.path.strip_prefix("iron_dawn/") {
                project.add_file(path, &entry.bytes).unwrap();
            }
        }
        project.scan().unwrap();
        assert!(project.lock.is_some());

        let mut changed = selection();
        changed["additionalCodingEnvironments"] = json!(["codex"]);
        changed["imageGeneration"] = json!({"primary_provider": "none"});
        let maintained = source_with(changed);
        let request = json!({ "maintenance": "change", "installedAt": "2026-10-20T00:00:00Z" });
        let export = prepare(&maintained, &request.to_string(), Some(project.snapshot())).unwrap();
        assert!(export
            .entries
            .iter()
            .any(|entry| entry.path == IMAGE_CONFIG_PATH && entry.action == ExportAction::Remove));
        assert!(export
            .entries
            .iter()
            .filter(|entry| entry.path == "descriptor.mod")
            .all(|entry| entry.action == ExportAction::Unchanged));
        // Merged client files are reviewed, as on the desktop; the incoming
        // version keeps the user's own servers, so it can replace them.
        let decisions = export
            .entries
            .iter()
            .filter(|entry| entry.action == ExportAction::Conflict)
            .map(|entry| {
                (
                    entry.path.clone(),
                    hoi4_setup_core::export::Decision::Replace,
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert!(decisions.contains_key(".mcp.json"));
        let archive = export.build_zip(&decisions).unwrap();
        let lock: Value = serde_json::from_slice(
            &read_zip(&archive)
                .unwrap()
                .into_iter()
                .find(|entry| entry.path == LOCK_PATH)
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!(lock["installed_at"], "2026-10-19T00:00:00Z");
        assert_eq!(lock["updated_at"], "2026-10-20T00:00:00Z");
        assert_eq!(lock["additional_coding_environments"], json!(["codex"]));

        let removal = prepare_removal(&project, "{}").unwrap();
        let archive = removal.build_zip(&BTreeMap::new()).unwrap();
        assert!(read_zip(&archive)
            .unwrap()
            .iter()
            .all(|entry| entry.path != LOCK_PATH));
    }

    /// Install a new mod, then load the exported folder as a dropped project,
    /// after `edit` changes its lock.
    fn installed_project(edit: impl Fn(&mut Value)) -> Project {
        let source = source_with(selection());
        let export = prepare(&source, &setup_request().to_string(), None).unwrap();
        let archive = export.build_zip(&BTreeMap::new()).unwrap();
        let mut project = Project::new();
        for entry in read_zip(&archive).unwrap() {
            if let Some(path) = entry.path.strip_prefix("iron_dawn/") {
                let bytes = if path == LOCK_PATH {
                    let mut lock: Value = serde_json::from_slice(&entry.bytes).unwrap();
                    edit(&mut lock);
                    serde_json::to_vec(&lock).unwrap()
                } else {
                    entry.bytes
                };
                project.add_file(path, &bytes).unwrap();
            }
        }
        project.scan().unwrap();
        project
    }

    #[test]
    fn setups_with_desktop_only_workflows_are_left_to_the_desktop() {
        let project = installed_project(|lock| {
            lock["components"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id": "workflow.3d", "version": null, "state": "installed"}));
        });
        let source = source_with(selection());
        let request = json!({ "maintenance": "update", "installedAt": "2026-10-20T00:00:00Z" });
        let error = prepare(&source, &request.to_string(), Some(project.snapshot()))
            .err()
            .unwrap();
        assert!(error.to_string().contains("desktop app"));
        // Removal rebuilds nothing, so it stays available.
        assert!(prepare_removal(&project, "{}").is_ok());
    }

    #[test]
    fn maintenance_keeps_key_references_and_refreshes_project_state() {
        let reference = "credential://image/openai_api/api_key";
        let project = installed_project(|lock| {
            lock["image_generation"]["credential_references"] =
                json!({ "openai_api.api_key": reference });
        });
        let mut changed = selection();
        changed["additionalCodingEnvironments"] = json!(["codex"]);
        let source = source_with(changed);
        let request = json!({ "maintenance": "change", "installedAt": "2026-10-20T00:00:00Z" });
        let export = prepare(&source, &request.to_string(), Some(project.snapshot())).unwrap();
        let config = export
            .incoming
            .iter()
            .find(|file| file.path == IMAGE_CONFIG_PATH)
            .unwrap();
        assert!(String::from_utf8_lossy(&config.bytes).contains(reference));
        assert_eq!(
            export
                .lock
                .image_generation
                .as_ref()
                .unwrap()
                .credential_references["openai_api.api_key"],
            reference
        );
        let state: Value = serde_json::from_slice(
            &export
                .incoming
                .iter()
                .find(|file| file.path == STATE_PATH)
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!(state["coding_environments"]["additional"], json!(["codex"]));
    }
}
