//! WebAssembly bindings for the HOI4 Mod Setup web edition.
//!
//! The browser keeps the dropped mod folder in a [`Project`], downloads the
//! manifest and selected source files itself, and hands them to [`Source`],
//! which verifies every file against the manifest's SHA-256 evidence before
//! anything is adapted. [`Export`] compares the result with the dropped folder
//! and writes the ZIP. Everything runs in the page; nothing is uploaded.
//!
//! Values cross the boundary as JSON strings and byte arrays.

use base64::Engine;
use hoi4_setup_core::adapt::{
    adapt_codex_config, adapt_selected_source, adapt_super_events_destination, AdaptContext,
    GameFolders, ProjectConventions,
};
use hoi4_setup_core::adapters::ProjectFs;
use hoi4_setup_core::coding_environment::{definitions_for_platform, unavailable_definitions};
use hoi4_setup_core::descriptors::{
    launcher_descriptor_text, parse_descriptor, placeholder_thumbnail_png, render_descriptor_mod,
    validate_thumbnail_png,
};
use hoi4_setup_core::export::{
    build_export, plan_export, Decision, ExportEntry, ExportLayout, ExportRequest, IncomingFile,
    LockInputs,
};
use hoi4_setup_core::manifest::{
    expand_components, installation_components, parse_manifest, resolve_platform_support,
    select_manifest_files, validate_commit, verify_download, wiki_install_metadata,
    SelectedSourceFile,
};
use hoi4_setup_core::memory_fs::MemoryFs;
use hoi4_setup_core::models::*;
use hoi4_setup_core::project_files::{merge_gitignore, project_readme};
use hoi4_setup_core::zip::ZipEntry;
use hoi4_setup_core::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

fn js(error: AppError) -> JsError {
    JsError::new(&error.to_string())
}

fn to_json<T: Serialize>(value: &T) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|error| JsError::new(&error.to_string()))
}

fn platform_of(name: &str) -> Result<Platform, AppError> {
    match name {
        "windows" => Ok(Platform::Windows),
        "macos" => Ok(Platform::Macos),
        "linux" => Ok(Platform::Linux),
        _ => Err(AppError::InvalidInput(
            "choose Windows, macOS, or Linux".into(),
        )),
    }
}

/// The dropped mod folder, held in memory.
#[wasm_bindgen]
pub struct Project {
    fs: MemoryFs,
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Project {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Project {
        Project {
            fs: MemoryFs::new(),
        }
    }

    /// Add one file of the dropped folder by its folder-relative path.
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, path: &str, bytes: &[u8]) -> Result<(), JsError> {
        self.fs.add_file(path, bytes.to_vec()).map_err(js)
    }

    #[wasm_bindgen(js_name = fileCount)]
    pub fn file_count(&self) -> usize {
        self.fs.file_count()
    }

    #[wasm_bindgen(js_name = totalBytes)]
    pub fn total_bytes(&self) -> f64 {
        self.fs.total_bytes() as f64
    }

    /// Read-only scan: `{ findings, detected, descriptor, stats }`.
    pub fn scan(&self) -> Result<String, JsError> {
        scan_project(&self.fs).map_err(js)
    }
}

fn scan_finding(id: &str, key: &str, value: Value, note: &str) -> ScanFinding {
    let excerpt = match &value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    ScanFinding {
        id: id.into(),
        category: "descriptor".into(),
        key: key.into(),
        model_excerpt: Some(excerpt.clone()),
        value,
        status: "accepted".into(),
        origin: "deterministic".into(),
        user_value: None,
        confidence: 1.0,
        blocking: false,
        proposed_action: None,
        decision_state: "accepted".into(),
        evidence: vec![ScanEvidence {
            detector: "descriptor_parser".into(),
            path: "descriptor.mod".into(),
            line_start: None,
            line_end: None,
            excerpt_sha256: Some(hoi4_setup_core::text::sha256_bytes(excerpt.as_bytes())),
            confidence: 1.0,
            note: Some(note.into()),
        }],
        recommendation: None,
    }
}

fn scan_project(fs: &MemoryFs) -> Result<String, AppError> {
    let profile = hoi4_setup_core::profile::profile(fs, &|| false, &mut |_, _| {})?;
    let mut findings = profile.findings;
    let mut descriptor = Value::Null;
    if let Ok(bytes) = fs.read("descriptor.mod", 1024 * 1024) {
        if let Ok(parsed) = parse_descriptor(&bytes) {
            if let Some(name) = parsed.fields.get("name") {
                findings.push(scan_finding(
                    "descriptor.name",
                    "mod_name",
                    json!(hoi4_setup_core::profile::clean_text(name, 256)),
                    "Matched the project descriptor name field.",
                ));
            }
            if let Some(tags) = parsed.fields.get("tags") {
                findings.push(scan_finding(
                    "descriptor.metadata",
                    "descriptor_metadata",
                    json!({ "tags": hoi4_setup_core::profile::clean_text(tags, 1024) }),
                    "Parsed descriptor fields.",
                ));
            }
            descriptor = json!(parsed
                .fields
                .iter()
                .map(|(key, value)| (
                    key.clone(),
                    hoi4_setup_core::profile::clean_text(value, 1024)
                ))
                .collect::<BTreeMap<_, _>>());
        }
    }
    let detected = hoi4_setup_core::proposals::detect(&findings);
    Ok(serde_json::to_string(&json!({
        "findings": findings,
        "detected": detected,
        "descriptor": descriptor,
        "stats": profile.stats,
        "issues": profile.problems.iter().collect::<Vec<_>>(),
    }))?)
}

/// What the user chose for the installation.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionRequest {
    #[serde(default)]
    selected_components: Vec<String>,
    primary_coding_environment: String,
    #[serde(default)]
    additional_coding_environments: Vec<String>,
    platform: String,
}

#[derive(Debug, Serialize)]
struct SelectionResult {
    components: Vec<String>,
    unsupported: Vec<ComponentSupportView>,
    files: Vec<SelectedFileView>,
}

#[derive(Debug, Serialize)]
struct ComponentSupportView {
    component_id: String,
    state: String,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct SelectedFileView {
    component_id: String,
    source_path: String,
    destination: String,
    expected_sha256: Option<String>,
    expected_size: Option<u64>,
}

/// One exact source revision: its verified manifest, the user's selection,
/// and the downloaded files that passed verification.
#[wasm_bindgen]
pub struct Source {
    manifest: RemoteManifest,
    manifest_sha256: String,
    revision: String,
    mode: SourceMode,
    requested_ref: Option<String>,
    selection: Option<CodingEnvironmentSelection>,
    components: Vec<String>,
    files: Vec<SelectedSourceFile>,
    verified: BTreeMap<String, Vec<u8>>,
}

#[wasm_bindgen]
impl Source {
    /// Validate a manifest fetched at an exact commit. `mode` is `latest`,
    /// `pinned_commit`, or `pinned_release`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        manifest_bytes: &[u8],
        revision: &str,
        mode: &str,
        requested_ref: Option<String>,
    ) -> Result<Source, JsError> {
        Source::open(manifest_bytes, revision, mode, requested_ref).map_err(js)
    }

    /// Components, profiles, and coding clients for the chosen platform.
    pub fn preview(&self, platform_name: &str) -> Result<String, JsError> {
        self.preview_json(platform_name).map_err(js)
    }

    /// Resolve the full component closure and the files to download.
    pub fn select(&mut self, request_json: &str) -> Result<String, JsError> {
        self.select_json(request_json).map_err(js)
    }

    /// Accept one downloaded source file only if its size and SHA-256 match
    /// the manifest evidence for a selected file.
    pub fn accept(&mut self, source_path: &str, bytes: &[u8]) -> Result<(), JsError> {
        self.accept_verified(source_path, bytes).map_err(js)
    }

    /// Source paths still waiting for a verified download.
    pub fn missing(&self) -> Result<String, JsError> {
        to_json(&self.missing_paths())
    }

    /// Adapt every verified file for a new mod.
    #[wasm_bindgen(js_name = exportNew)]
    pub fn export_new(&self, request_json: &str) -> Result<Export, JsError> {
        prepare_export(self, request_json, None).map_err(js)
    }

    /// Adapt every verified file and compare the result with the dropped
    /// folder. The folder is copied, so the scan can be reused.
    #[wasm_bindgen(js_name = exportExisting)]
    pub fn export_existing(
        &self,
        request_json: &str,
        project: &Project,
    ) -> Result<Export, JsError> {
        prepare_export(
            self,
            request_json,
            Some(Project {
                fs: project.fs.clone(),
            }),
        )
        .map_err(js)
    }
}

impl Source {
    fn open(
        manifest_bytes: &[u8],
        revision: &str,
        mode: &str,
        requested_ref: Option<String>,
    ) -> Result<Source, AppError> {
        validate_commit(revision)?;
        let manifest = parse_manifest(manifest_bytes, Some(revision))?;
        let mode = match mode {
            "latest" => SourceMode::Latest,
            "pinned_commit" => SourceMode::PinnedCommit,
            "pinned_release" => SourceMode::PinnedRelease,
            _ => return Err(AppError::InvalidInput("unknown source mode".into())),
        };
        Ok(Source {
            manifest,
            manifest_sha256: hoi4_setup_core::text::sha256_bytes(manifest_bytes),
            revision: revision.to_owned(),
            mode,
            requested_ref,
            selection: None,
            components: Vec::new(),
            files: Vec::new(),
            verified: BTreeMap::new(),
        })
    }

    fn preview_json(&self, platform_name: &str) -> Result<String, AppError> {
        let platform = platform_of(platform_name)?;
        // File inventories stay in the core; the page needs only a summary.
        let components = self
            .manifest
            .components
            .iter()
            .map(|component| {
                json!({
                    "id": component.id,
                    "display_name": component.display_name,
                    "description": component.description,
                    "category": component.category,
                    "coding_environment": component.coding_environment,
                    "optional": component.optional,
                    "platforms": component.platforms,
                    "dependencies": component.dependencies,
                    "file_count": component.expected_files.len(),
                    "total_size": component
                        .expected_files
                        .iter()
                        .filter_map(|file| file.size)
                        .sum::<u64>(),
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::to_string(&json!({
            "schema_version": self.manifest.schema_version,
            "revision": self.revision,
            "components": components,
            "profiles": self.manifest.profiles,
            "coding_environments": definitions_for_platform(&self.manifest, platform),
            "unavailable_coding_environments": unavailable_definitions(&self.manifest, platform),
        }))?)
    }

    fn select_json(&mut self, request_json: &str) -> Result<String, AppError> {
        let request: SelectionRequest = serde_json::from_str(request_json)?;
        let platform = platform_of(&request.platform)?;
        let selection = CodingEnvironmentSelection {
            primary: request.primary_coding_environment,
            additional: request.additional_coding_environments,
        };
        // The 3D workflow needs the desktop app's Meshy launcher.
        let user_selected = request
            .selected_components
            .into_iter()
            .filter(|id| id != "workflow.3d")
            .collect::<Vec<_>>();
        let requested =
            installation_components(&self.manifest, &user_selected, &selection, platform)?;
        let expanded = expand_components(&self.manifest, &requested)?;
        let support = resolve_platform_support(&self.manifest, &expanded, platform)?;
        if support.iter().any(|item| item.state == "blocked") {
            return Err(AppError::UnsupportedPlatform(
                "a required component has no verified route for this platform".into(),
            ));
        }
        let components = expanded
            .into_iter()
            .filter(|id| {
                support
                    .iter()
                    .any(|item| item.component_id == *id && item.state == "supported")
            })
            .collect::<Vec<_>>();
        let files = select_manifest_files(&self.manifest, &components)?;
        let result = SelectionResult {
            unsupported: support
                .iter()
                .filter(|item| item.state != "supported")
                .map(|item| ComponentSupportView {
                    component_id: item.component_id.clone(),
                    state: item.state.clone(),
                    reason: item.reason.clone(),
                })
                .collect(),
            files: files
                .iter()
                .map(|file| SelectedFileView {
                    component_id: file.component_id.clone(),
                    source_path: file.source_path.clone(),
                    destination: file.destination.clone(),
                    expected_sha256: file.expected_sha256.clone(),
                    expected_size: file.expected_size,
                })
                .collect(),
            components: components.clone(),
        };
        self.selection = Some(selection);
        self.components = components;
        self.files = files;
        self.verified.clear();
        Ok(serde_json::to_string(&result)?)
    }

    fn accept_verified(&mut self, source_path: &str, bytes: &[u8]) -> Result<(), AppError> {
        let selection = self
            .files
            .iter()
            .find(|file| file.source_path == source_path)
            .ok_or_else(|| AppError::InvalidInput("this file was not selected".into()))?;
        verify_download(selection, bytes, &self.revision)?;
        self.verified.insert(source_path.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn missing_paths(&self) -> Vec<String> {
        let mut missing = self
            .files
            .iter()
            .filter(|file| !self.verified.contains_key(&file.source_path))
            .map(|file| file.source_path.clone())
            .collect::<Vec<_>>();
        missing.sort();
        missing.dedup();
        missing
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportOptions {
    identity: IdentityInput,
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
    #[serde(default)]
    ai_provider: Option<String>,
    #[serde(default)]
    ai_model: Option<String>,
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

fn generated(path: &str, component_id: &str, bytes: Vec<u8>) -> IncomingFile {
    IncomingFile {
        path: path.into(),
        component_id: component_id.into(),
        bytes,
        ownership: Ownership::Generated,
        source_path: format!("generated:{path}"),
        executable: false,
    }
}

fn prepare_export(
    source: &Source,
    request_json: &str,
    project: Option<Project>,
) -> Result<Export, AppError> {
    let options: ExportOptions = serde_json::from_str(request_json)?;
    let selection = source
        .selection
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
    hoi4_setup_core::descriptors::validate_project_id(&options.identity.project_id)?;
    let identity = ProjectIdentity {
        display_name: options.identity.display_name,
        project_id: options.identity.project_id.clone(),
        author: options.identity.author,
        version: options.identity.version,
        supported_game_version: options.identity.supported_game_version,
        project_root: PathBuf::from(format!("mod/{}", options.identity.project_id)),
        default_branch: "main".into(),
        script_prefix: options.identity.script_prefix,
        primary_namespace: options.identity.primary_namespace,
        descriptor_tags: options.identity.descriptor_tags,
        launcher_descriptor_path: None,
    };
    let conventions = ProjectConventions {
        agents_profile: options.conventions.agents_profile,
        localisation_convention: options.conventions.localisation_convention,
        documentation_convention: options.conventions.documentation_convention,
    };
    let folders = GameFolders {
        game_dir: "[HOI4_GAME_DIR]".into(),
        workshop_dir: "[STEAM_WORKSHOP_DIR]".into(),
    };
    let portrait = PortraitPipelineConfig {
        enabled: false,
        provider: "disabled".into(),
        provider_status: "not_selected".into(),
        workflow_repository: String::new(),
        workflow_branch: String::new(),
        workflow_commit: String::new(),
        preferred_workflow: String::new(),
        local_comfyui_root: String::new(),
        local_server_url: String::new(),
        runpod_url: String::new(),
        runpod_workspace: String::new(),
        mcp_registered: false,
    };
    let super_events_selected = source
        .components
        .iter()
        .any(|id| id == "workflow.super_events");
    let mcp_selected = source
        .components
        .iter()
        .any(|id| id == "mcp.hoi4_agent_tools");
    let context = AdaptContext {
        identity: &identity,
        conventions: &conventions,
        folders: &folders,
        super_events_selected,
        portrait: &portrait,
        selected_components: &source.components,
    };
    let codex = |bytes: &[u8]| {
        adapt_codex_config(bytes, mcp_selected, None, super_events_selected, &portrait)
    };
    let mut incoming = Vec::new();
    let mut destinations = HashSet::new();
    for file in &source.files {
        let destination =
            adapt_super_events_destination(&file.component_id, &file.destination, &identity)?;
        if !destinations.insert(destination.to_lowercase()) {
            continue;
        }
        let bytes = adapt_selected_source(
            &file.component_id,
            &destination,
            &source.verified[&file.source_path],
            &context,
            &codex,
        )?;
        incoming.push(IncomingFile {
            path: destination,
            component_id: file.component_id.clone(),
            bytes,
            ownership: file.ownership,
            source_path: file.source_path.clone(),
            executable: file.executable,
        });
    }
    let local = project.as_ref().map(|project| &project.fs);
    incoming.push(generated(
        "descriptor.mod",
        "project.descriptor",
        render_descriptor_mod(&identity)?.into_bytes(),
    ));
    let thumbnail = match options.thumbnail_png.as_deref() {
        Some(encoded) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(
                    encoded
                        .split_once(";base64,")
                        .map_or(encoded, |(_, data)| data),
                )
                .map_err(|_| AppError::InvalidInput("the thumbnail is not valid base64".into()))?;
            validate_thumbnail_png(&bytes)?;
            bytes
        }
        None => placeholder_thumbnail_png()?,
    };
    if options.thumbnail_png.is_some()
        || local.is_none_or(|fs| fs.read("thumbnail.png", 16 * 1024 * 1024).is_err())
    {
        incoming.push(generated("thumbnail.png", "project.thumbnail", thumbnail));
    }
    incoming.push(generated(
        "README.md",
        "project.readme",
        project_readme(&identity, &options.description)?.into_bytes(),
    ));
    let existing_gitignore = local
        .and_then(|fs| fs.read(".gitignore", 1024 * 1024).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok());
    incoming.push(generated(
        ".gitignore",
        "git.ignore",
        merge_gitignore(existing_gitignore.as_deref()).into_bytes(),
    ));
    let entries = plan_export(&incoming, local.map(|fs| fs as &dyn ProjectFs))?;
    let root_files = if local.is_none() {
        vec![ZipEntry {
            path: format!("{}.mod", identity.project_id),
            bytes: launcher_descriptor_text(&identity, &format!("mod/{}", identity.project_id))?
                .into_bytes(),
        }]
    } else {
        Vec::new()
    };
    let components = source
        .components
        .iter()
        .map(|id| LockComponent {
            id: id.clone(),
            version: None,
            state: "installed".into(),
            source_revision: Some(source.revision.clone()),
            validation: None,
        })
        .collect();
    let lock = LockInputs {
        project_id: identity.project_id.clone(),
        script_prefix: identity.script_prefix.clone(),
        primary_namespace: identity.primary_namespace.clone(),
        installed_at: options.installed_at,
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
        ai_provider: options.ai_provider.unwrap_or_else(|| "none".into()),
        ai_model: options.ai_model.unwrap_or_else(|| "deterministic".into()),
        ai_reasoning_effort: "none".into(),
        ai_optimization_profile: "Web edition setup".into(),
        primary_coding_environment: selection.primary.clone(),
        additional_coding_environments: selection.additional.clone(),
        wiki_required_pages: source.manifest.wiki.required_pages.clone(),
        wiki_metadata: wiki_install_metadata(&source.manifest),
        components,
        optional_workflows: BTreeMap::new(),
    };
    Ok(Export {
        incoming,
        entries,
        local: project,
        layout: options.layout.unwrap_or(ExportLayout::Overlay),
        root_folder: identity.project_id,
        root_files,
        lock,
    })
}

/// The reviewed export: what changes, and the ZIP once every conflict has a
/// decision.
#[wasm_bindgen]
pub struct Export {
    incoming: Vec<IncomingFile>,
    entries: Vec<ExportEntry>,
    local: Option<Project>,
    layout: ExportLayout,
    root_folder: String,
    root_files: Vec<ZipEntry>,
    lock: LockInputs,
}

#[wasm_bindgen]
impl Export {
    /// Every file with its action: create, unchanged, or conflict.
    pub fn entries(&self) -> Result<String, JsError> {
        to_json(&self.entries)
    }

    /// Text of an incoming file and of the dropped folder's version, for the
    /// conflict comparison.
    pub fn compare(&self, path: &str) -> Result<String, JsError> {
        let incoming = self
            .incoming
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| JsError::new("unknown file"))?;
        let local = self
            .local
            .as_ref()
            .and_then(|project| project.fs.read(path, 16 * 1024 * 1024).ok());
        let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).ok();
        to_json(&json!({
            "path": path,
            "incoming": text(&incoming.bytes),
            "local": local.as_deref().and_then(text),
            "binary": text(&incoming.bytes).is_none(),
        }))
    }

    /// The ZIP. `decisions_json` maps each conflicting path to `keep` or
    /// `replace`.
    pub fn zip(&self, decisions_json: &str) -> Result<Vec<u8>, JsError> {
        let decisions: BTreeMap<String, Decision> = serde_json::from_str(decisions_json)
            .map_err(|error| JsError::new(&error.to_string()))?;
        build_export(ExportRequest {
            incoming: &self.incoming,
            entries: &self.entries,
            decisions: &decisions,
            local: self
                .local
                .as_ref()
                .map(|project| &project.fs as &dyn ProjectFs),
            layout: self.layout,
            root_folder: &self.root_folder,
            root_files: self.root_files.clone(),
            lock: self.lock.clone(),
        })
        .map_err(js)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_folder_scans_with_detected_values() {
        let mut project = Project::new();
        project
            .add_file("descriptor.mod", b"name=\"Demo\"\ntags={ \"Events\" }\n")
            .unwrap();
        project
            .add_file("events/demo.txt", b"add_namespace = demo\n")
            .unwrap();
        let scan: Value = serde_json::from_str(&scan_project(&project.fs).unwrap()).unwrap();
        let detected = scan["detected"].as_array().unwrap();
        assert!(detected
            .iter()
            .any(|value| value["key"] == "descriptor_tags" && value["value"] == json!(["Events"])));
        assert!(detected
            .iter()
            .any(|value| value["key"] == "primary_namespace" && value["value"] == "demo"));
        assert_eq!(scan["descriptor"]["name"], "Demo");
    }

    #[test]
    fn a_new_mod_exports_from_the_candidate_manifest() {
        let manifest = include_bytes!("../../../docs/fixtures/format6-candidate-manifest.json");
        let revision = "0b124ad9d3121fda5eb3cc5ff365f74a28db0f8f";
        let mut source =
            Source::open(manifest, revision, "pinned_commit", Some(revision.into())).unwrap();
        let selection: Value = serde_json::from_str(
            &source
                .select_json(
                    r#"{"selectedComponents":[],"primaryCodingEnvironment":"claude_code","platform":"windows"}"#,
                )
                .unwrap(),
        )
        .unwrap();
        let files = selection["files"].as_array().unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().all(|file| file["expected_sha256"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64)));
        assert!(source.accept_verified("not/selected.txt", b"x").is_err());
        let first = files[0]["source_path"].as_str().unwrap();
        assert!(
            source.accept_verified(first, b"wrong bytes").is_err(),
            "unverified bytes are refused"
        );
        assert!(source.missing_paths().contains(&first.to_owned()));
    }
}
