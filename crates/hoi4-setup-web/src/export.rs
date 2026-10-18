//! The reviewed export: what changes, merge previews for files the user
//! changed, the checks still to finish, and the ZIP once every conflict has a
//! decision.

use crate::build::Pipeline;
use crate::scan::Project;
use crate::{js, to_json};
use hoi4_setup_core::adapters::ProjectFs;
use hoi4_setup_core::descriptors::launcher_descriptor_text;
use hoi4_setup_core::export::{
    build_export, Decision, ExportAction, ExportEntry, ExportLayout, ExportRequest, IncomingFile,
    LockInputs, MAX_EXPORT_FILE_BYTES,
};
use hoi4_setup_core::merge::{
    structured_json_merge, structured_toml_merge, three_way_merge, validate_merged_result, FileKind,
};
use hoi4_setup_core::models::ProjectIdentity;
use hoi4_setup_core::text::sha256_bytes;
use hoi4_setup_core::zip::ZipEntry;
use hoi4_setup_core::AppError;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

/// Where to fetch the version a changed managed file was installed from.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct BaseSource {
    pub component_id: String,
    pub revision: String,
    pub source_path: String,
    pub sha256: String,
}

/// One thing the user still has to do, or a confirmation that it is done.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Check {
    pub id: String,
    pub label: String,
    /// `pass`, `action`, or `info`.
    pub state: &'static str,
    pub detail: String,
}

impl Check {
    pub(crate) fn pass(id: &str, label: &str, detail: String) -> Check {
        Check {
            id: id.into(),
            label: label.into(),
            state: "pass",
            detail,
        }
    }

    pub(crate) fn action(id: &str, label: &str, detail: String) -> Check {
        Check {
            id: id.into(),
            label: label.into(),
            state: "action",
            detail,
        }
    }

    pub(crate) fn info(id: &str, label: &str, detail: String) -> Check {
        Check {
            id: id.into(),
            label: label.into(),
            state: "info",
            detail,
        }
    }
}

#[wasm_bindgen]
pub struct Export {
    pub(crate) incoming: Vec<IncomingFile>,
    pub(crate) entries: Vec<ExportEntry>,
    pub(crate) local: Option<Project>,
    pub(crate) layout: ExportLayout,
    pub(crate) root_folder: String,
    pub(crate) root_files: Vec<ZipEntry>,
    pub(crate) lock: LockInputs,
    pub(crate) pipeline: Option<Pipeline>,
    pub(crate) base_sources: BTreeMap<String, BaseSource>,
    pub(crate) bases: BTreeMap<String, Vec<u8>>,
    pub(crate) merged: BTreeMap<String, Vec<u8>>,
    pub(crate) remove_setup: bool,
    pub(crate) readiness: Vec<Check>,
}

fn kind_of(path: &str) -> FileKind {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".json") {
        FileKind::Json
    } else if lower.ends_with(".toml") {
        FileKind::Toml
    } else {
        FileKind::Text
    }
}

impl Export {
    fn local_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.local
            .as_ref()
            .and_then(|project| project.fs.read(path, MAX_EXPORT_FILE_BYTES).ok())
    }

    pub(crate) fn identity(&self) -> Option<&ProjectIdentity> {
        self.pipeline.as_ref().map(Pipeline::identity)
    }

    /// Verify and adapt the version a changed file was installed from.
    pub(crate) fn add_base_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), AppError> {
        let source = self
            .base_sources
            .get(path)
            .ok_or_else(|| AppError::InvalidInput("this file has no merge base".into()))?;
        if sha256_bytes(bytes) != source.sha256 {
            return Err(AppError::Source(format!(
                "the installed version of {path} does not match the lock"
            )));
        }
        let pipeline = self
            .pipeline
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("this export cannot merge".into()))?;
        let adapted = pipeline.adapt(&source.component_id, path, bytes, None)?;
        self.bases.insert(path.to_owned(), adapted);
        Ok(())
    }

    /// The three-way merge of the installed version, the user's file, and
    /// the incoming file; it must validate before it can be chosen.
    pub(crate) fn merge_bytes(&mut self, path: &str) -> Result<Vec<u8>, AppError> {
        let base = self
            .bases
            .get(path)
            .ok_or_else(|| AppError::InvalidInput("fetch the installed version first".into()))?;
        let local = self
            .local_bytes(path)
            .ok_or_else(|| AppError::InvalidInput("the folder has no file to merge".into()))?;
        let incoming = self
            .incoming
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| AppError::InvalidInput("unknown file".into()))?;
        let text = |bytes: &[u8], label: &str| {
            String::from_utf8(bytes.to_vec())
                .map_err(|_| AppError::Merge(format!("{label} is not UTF-8 text")))
        };
        let (base, local, incoming_text) = (
            text(base, "the installed version")?,
            text(&local, "your file")?,
            text(&incoming.bytes, "the incoming file")?,
        );
        let kind = kind_of(path);
        let merged = match kind {
            FileKind::Json => structured_json_merge(&base, &local, &incoming_text)?,
            FileKind::Toml => structured_toml_merge(&base, &local, &incoming_text)?,
            _ => three_way_merge(&base, &local, &incoming_text)?,
        }
        .into_bytes();
        validate_merged_result(kind, path, &merged)?;
        self.merged.insert(path.to_owned(), merged.clone());
        Ok(merged)
    }

    pub(crate) fn build_zip(
        &self,
        decisions: &BTreeMap<String, Decision>,
    ) -> Result<Vec<u8>, AppError> {
        self.build_zip_with(decisions, self.layout)
    }

    pub(crate) fn build_zip_with(
        &self,
        decisions: &BTreeMap<String, Decision>,
        layout: ExportLayout,
    ) -> Result<Vec<u8>, AppError> {
        build_export(ExportRequest {
            incoming: &self.incoming,
            entries: &self.entries,
            decisions,
            merged: &self.merged,
            local: self
                .local
                .as_ref()
                .map(|project| &project.fs as &dyn ProjectFs),
            layout,
            root_folder: &self.root_folder,
            root_files: self.root_files.clone(),
            lock: self.lock.clone(),
            remove_setup: self.remove_setup,
        })
    }
}

#[wasm_bindgen]
impl Export {
    /// Every file with its action.
    pub fn entries(&self) -> Result<String, JsError> {
        to_json(&self.entries)
    }

    /// The incoming file, the folder's version, and when available the
    /// installed version and the reviewed merge, for the comparison view.
    pub fn compare(&self, path: &str) -> Result<String, JsError> {
        let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).ok();
        let incoming = self.incoming.iter().find(|file| file.path == path);
        let local = self.local_bytes(path);
        let binary = incoming
            .map(|file| text(&file.bytes).is_none())
            .or_else(|| local.as_deref().map(|bytes| text(bytes).is_none()))
            .unwrap_or(false);
        to_json(&json!({
            "path": path,
            "incoming": incoming.and_then(|file| text(&file.bytes)),
            "local": local.as_deref().and_then(text),
            "base": self.bases.get(path).and_then(|bytes| text(bytes)),
            "merged": self.merged.get(path).and_then(|bytes| text(bytes)),
            "binary": binary,
        }))
    }

    /// Installed versions still to fetch for files that can merge:
    /// `[{ path, revision, source_path, sha256 }]`.
    #[wasm_bindgen(js_name = mergeBases)]
    pub fn merge_bases(&self) -> Result<String, JsError> {
        to_json(
            &self
                .base_sources
                .iter()
                .filter(|(path, _)| !self.bases.contains_key(*path))
                .map(|(path, source)| {
                    json!({
                        "path": path,
                        "revision": source.revision,
                        "source_path": source.source_path,
                        "sha256": source.sha256,
                    })
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Add a fetched installed version after checking it against the lock.
    #[wasm_bindgen(js_name = addBase)]
    pub fn add_base(&mut self, path: &str, bytes: &[u8]) -> Result<(), JsError> {
        self.add_base_bytes(path, bytes).map_err(js)
    }

    /// Merge a changed file and return the result for review.
    pub fn merge(&mut self, path: &str) -> Result<String, JsError> {
        let merged = self.merge_bytes(path).map_err(js)?;
        String::from_utf8(merged).map_err(|_| JsError::new("the merge is not text"))
    }

    /// What is ready and what the user still has to do.
    pub fn readiness(&self) -> Result<String, JsError> {
        to_json(&self.readiness)
    }

    /// The launcher file for a folder named `folder` in the game's mod
    /// directory, so the launcher lists an existing mod.
    #[wasm_bindgen(js_name = launcherFile)]
    pub fn launcher_file(&self, folder: &str) -> Result<String, JsError> {
        let identity = self
            .identity()
            .ok_or_else(|| JsError::new("this export has no project identity"))?;
        let folder = folder.trim();
        if folder.is_empty()
            || folder.contains(['/', '\\'])
            || folder == "."
            || folder == ".."
            || folder.chars().any(char::is_control)
        {
            return Err(JsError::new("the mod folder name is not valid"));
        }
        launcher_descriptor_text(identity, &format!("mod/{folder}")).map_err(js)
    }

    /// Counts by action, for the review summary.
    pub fn summary(&self) -> Result<String, JsError> {
        let count = |action: ExportAction| {
            self.entries
                .iter()
                .filter(|entry| entry.action == action)
                .count()
        };
        to_json(&json!({
            "create": count(ExportAction::Create),
            "unchanged": count(ExportAction::Unchanged),
            "conflict": count(ExportAction::Conflict),
            "update": count(ExportAction::Update),
            "restore": count(ExportAction::Restore),
            "remove": count(ExportAction::Remove),
            "release": count(ExportAction::Release),
            "modified": count(ExportAction::Modified),
            "remove_setup": self.remove_setup,
        }))
    }

    /// The ZIP. `decisions_json` maps each conflicting path to `keep`,
    /// `replace`, `merge`, `rename`, or `skip`; `layout` is `overlay` or
    /// `full_copy` and defaults to the export's layout.
    pub fn zip(&self, decisions_json: &str, layout: Option<String>) -> Result<Vec<u8>, JsError> {
        let decisions: BTreeMap<String, Decision> = serde_json::from_str(decisions_json)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let layout = match layout.as_deref() {
            None => self.layout,
            Some("overlay") => ExportLayout::Overlay,
            Some("full_copy") => ExportLayout::FullCopy,
            Some(_) => return Err(JsError::new("choose an overlay or a full copy")),
        };
        self.build_zip_with(&decisions, layout).map_err(js)
    }
}
