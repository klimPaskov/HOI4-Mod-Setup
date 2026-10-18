//! One exact source revision: its verified manifest, the user's selection,
//! and the downloaded files that passed verification.

use crate::scan::Project;
use crate::{js, platform_of, to_json, DESKTOP_ONLY_COMPONENTS};
use hoi4_setup_core::coding_environment::{definitions_for_platform, unavailable_definitions};
use hoi4_setup_core::image_generation::{
    image_generation_provider_rows, validate_image_generation_selection,
};
use hoi4_setup_core::manifest::{
    expand_components, installation_components, parse_manifest, resolve_platform_support,
    select_manifest_files, validate_commit, verify_download, SelectedSourceFile,
};
use hoi4_setup_core::models::*;
use hoi4_setup_core::portrait::{portrait_component_ids, portrait_config, PortraitChoice};
use hoi4_setup_core::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

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
    #[serde(default)]
    image_generation: Option<ImageChoice>,
    #[serde(default)]
    live_testing: Option<LiveTestingSelection>,
    #[serde(default)]
    portrait: Option<PortraitChoice>,
}

/// Image providers without credentials; keys stay in the OS credential store.
#[derive(Debug, Deserialize)]
struct ImageChoice {
    primary_provider: String,
    #[serde(default)]
    additional_providers: Vec<String>,
    #[serde(default)]
    route_settings: BTreeMap<String, BTreeMap<String, String>>,
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

/// The validated choices an export adapts files for.
#[derive(Debug, Clone)]
pub(crate) struct Chosen {
    pub clients: CodingEnvironmentSelection,
    pub platform: Platform,
    pub image: ImageGenerationSelection,
    pub live: LiveTestingSelection,
    pub portrait: PortraitPipelineConfig,
    pub components: Vec<String>,
}

#[wasm_bindgen]
pub struct Source {
    pub(crate) manifest: RemoteManifest,
    pub(crate) manifest_sha256: String,
    pub(crate) revision: String,
    pub(crate) mode: SourceMode,
    pub(crate) requested_ref: Option<String>,
    pub(crate) chosen: Option<Chosen>,
    pub(crate) files: Vec<SelectedSourceFile>,
    pub(crate) verified: BTreeMap<String, Vec<u8>>,
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

    /// Components, profiles, coding clients, image providers, and live
    /// testing routes for the chosen platform.
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

    /// The component IDs an assistant may recommend, bound to this exact
    /// revision and manifest.
    #[wasm_bindgen(js_name = componentRegistry)]
    pub fn component_registry(&self) -> Result<String, JsError> {
        let mut component_ids = self
            .manifest
            .components
            .iter()
            .map(|component| component.id.as_str())
            .collect::<Vec<_>>();
        component_ids.sort_unstable();
        component_ids.dedup();
        to_json(&json!({
            "source_revision": self.revision,
            "manifest_sha256": self.manifest_sha256,
            "component_ids": component_ids,
        }))
    }

    /// Adapt every verified file for a new mod.
    #[wasm_bindgen(js_name = exportNew)]
    pub fn export_new(&self, request_json: &str) -> Result<crate::Export, JsError> {
        crate::build::prepare(self, request_json, None).map_err(js)
    }

    /// Adapt every verified file and compare the result with the dropped
    /// folder. A folder with an installation lock is maintained: unchanged
    /// managed files update, changed ones are reviewed.
    #[wasm_bindgen(js_name = exportExisting)]
    pub fn export_existing(
        &self,
        request_json: &str,
        project: &Project,
    ) -> Result<crate::Export, JsError> {
        crate::build::prepare(self, request_json, Some(project.snapshot())).map_err(js)
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
            chosen: None,
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
        let catalog = self.manifest.image_generation.as_ref();
        let image_providers = catalog
            .and_then(|catalog| catalog.get("providers"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|provider| {
                let route = provider.get("route").unwrap_or(&Value::Null);
                json!({
                    "id": provider.get("id"),
                    "display_name": provider.get("display_name"),
                    "description": provider.get("description"),
                    "availability": provider.get("availability"),
                    "requirements": provider.get("requirements"),
                    "good_for": provider.get("good_for"),
                    "cost_summary": provider.get("cost_summary"),
                    "documentation_url": provider.get("documentation_url"),
                    "supported_coding_environments": provider.get("supported_coding_environments"),
                    "route_kind": route.get("kind"),
                    "auth_method": route.get("auth_method"),
                    "api_protocol": route.get("api_protocol"),
                    "supported_models": route.get("supported_models"),
                    "requires_primary_coding_environment": route.get("requires_primary_coding_environment"),
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
            "image_generation": catalog.map(|catalog| json!({
                "default_provider": catalog.get("default_provider"),
                "providers": image_providers,
            })),
            "live_testing": self.manifest.live_testing,
        }))?)
    }

    fn select_json(&mut self, request_json: &str) -> Result<String, AppError> {
        let request: SelectionRequest = serde_json::from_str(request_json)?;
        let platform = platform_of(&request.platform)?;
        let clients = CodingEnvironmentSelection {
            primary: request.primary_coding_environment,
            additional: request.additional_coding_environments,
        };
        let portrait = portrait_config(request.portrait.as_ref())?;
        // Desktop-only components are never installed from the browser, and
        // the portrait closure follows the portrait choice alone.
        let mut user_selected = request
            .selected_components
            .into_iter()
            .filter(|id| {
                !DESKTOP_ONLY_COMPONENTS.contains(&id.as_str())
                    && !id.starts_with("workflow.portraits")
            })
            .collect::<Vec<_>>();
        if portrait.enabled {
            if let Some(ids) = portrait_component_ids(&portrait.provider) {
                user_selected.extend(ids.iter().map(|id| (*id).to_owned()));
            }
        }
        let mut requested =
            installation_components(&self.manifest, &user_selected, &clients, platform)?;
        let image = image_selection(request.image_generation, &self.manifest);
        validate_image_generation_selection(&image, &self.manifest, &clients)?;
        for provider in image_generation_provider_rows(&self.manifest, &image)? {
            if let Some(component_id) = provider
                .pointer("/route/component_id")
                .and_then(Value::as_str)
            {
                if !self
                    .manifest
                    .components
                    .iter()
                    .any(|component| component.id == component_id)
                {
                    return Err(AppError::Source(format!(
                        "image provider component is missing from the verified manifest: {component_id}"
                    )));
                }
                if !requested.iter().any(|selected| selected == component_id) {
                    requested.push(component_id.to_owned());
                }
            }
        }
        let live = live_selection(request.live_testing, &self.manifest, platform)?;
        hoi4_setup_core::live_testing::components(&mut requested, &live, &self.manifest)?;
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
        self.chosen = Some(Chosen {
            clients,
            platform,
            image,
            live,
            portrait,
            components,
        });
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

/// The image selection: the user's choice, or the catalog's default when
/// none was made. Credential references are never part of a web selection.
fn image_selection(
    choice: Option<ImageChoice>,
    manifest: &RemoteManifest,
) -> ImageGenerationSelection {
    let default_provider = manifest
        .image_generation
        .as_ref()
        .and_then(|catalog| catalog.get("default_provider"))
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_owned();
    match choice {
        Some(choice) => ImageGenerationSelection {
            primary_provider: if choice.primary_provider.trim().is_empty() {
                default_provider
            } else {
                choice.primary_provider
            },
            additional_providers: choice.additional_providers,
            route_settings: choice.route_settings,
            ..Default::default()
        },
        None => ImageGenerationSelection {
            primary_provider: default_provider,
            ..Default::default()
        },
    }
}

/// The live-testing route. HOI4 Live Tools installs a server, which only the
/// desktop app can do.
fn live_selection(
    choice: Option<LiveTestingSelection>,
    manifest: &RemoteManifest,
    platform: Platform,
) -> Result<LiveTestingSelection, AppError> {
    let selection = choice.unwrap_or(LiveTestingSelection {
        provider: "none".into(),
        ..Default::default()
    });
    if selection.provider == "hoi4_live_tools" {
        return Err(AppError::InvalidInput(
            "HOI4 Live Tools installs a server; set it up with the desktop app".into(),
        ));
    }
    hoi4_setup_core::live_testing::validate(&selection, platform)?;
    if selection.provider != "none"
        && !manifest.live_testing.as_ref().is_some_and(|catalog| {
            catalog["providers"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["id"] == selection.provider))
        })
    {
        return Err(AppError::Source(
            "the selected live testing route is not in this source version".into(),
        ));
    }
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] =
        include_bytes!("../../../docs/fixtures/format6-candidate-manifest.json");
    const REVISION: &str = "0b124ad9d3121fda5eb3cc5ff365f74a28db0f8f";

    fn source() -> Source {
        Source::open(MANIFEST, REVISION, "pinned_commit", Some(REVISION.into())).unwrap()
    }

    #[test]
    fn a_selection_resolves_verified_files_and_refuses_unverified_bytes() {
        let mut source = source();
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

    #[test]
    fn image_portrait_and_live_choices_add_their_components() {
        let mut source = source();
        let request = json!({
            "selectedComponents": [],
            "primaryCodingEnvironment": "claude_code",
            "platform": "linux",
            "imageGeneration": {"primary_provider": "openai_api", "additional_providers": []},
            "portrait": {"provider": "cloud"},
            "liveTesting": {"provider": "clausewitz_mcp", "linux_ready": true, "wsl_distribution": "", "ssh_host": ""},
        });
        let result: Value =
            serde_json::from_str(&source.select_json(&request.to_string()).unwrap()).unwrap();
        let components = result["components"].as_array().unwrap();
        let has = |id: &str| components.iter().any(|component| component == id);
        assert!(
            has("imagegen.runtime"),
            "the adapter route's component is installed"
        );
        assert!(has("workflow.portraits.cloud") && has("workflow.portraits.config"));
        assert!(has("workflow.live_testing"));
        let chosen = source.chosen.as_ref().unwrap();
        assert!(chosen.portrait.enabled && chosen.image.primary_provider == "openai_api");

        let desktop_only = json!({
            "selectedComponents": ["workflow.3d"],
            "primaryCodingEnvironment": "claude_code",
            "platform": "windows",
            "liveTesting": {"provider": "hoi4_live_tools", "linux_ready": false, "wsl_distribution": "", "ssh_host": ""},
        });
        assert!(source.select_json(&desktop_only.to_string()).is_err());
        let unknown_image = json!({
            "selectedComponents": [],
            "primaryCodingEnvironment": "claude_code",
            "platform": "windows",
            "imageGeneration": {"primary_provider": "not_a_provider"},
        });
        assert!(source.select_json(&unknown_image.to_string()).is_err());
    }

    #[test]
    fn preview_lists_image_providers_without_route_secrets() {
        let preview: Value =
            serde_json::from_str(&source().preview_json("windows").unwrap()).unwrap();
        let providers = preview["image_generation"]["providers"].as_array().unwrap();
        assert!(providers
            .iter()
            .any(|provider| provider["id"] == "openai_api"));
        assert!(preview["live_testing"]["providers"].is_array());
    }
}
