//! The dropped mod folder: a read-only scan, the evidence a setup assistant
//! may read, and an existing setup's installation lock.

use crate::{js, to_json};
use hoi4_setup_core::adapters::ProjectFs;
use hoi4_setup_core::analysis::ApprovedEvidence;
use hoi4_setup_core::descriptors::parse_descriptor;
use hoi4_setup_core::export::LOCK_PATH;
use hoi4_setup_core::memory_fs::MemoryFs;
use hoi4_setup_core::models::*;
use hoi4_setup_core::AppError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::prelude::*;

/// Where the desktop app records project conventions for a new mod.
pub(crate) const STATE_PATH: &str = ".hoi4-mod-setup/state.json";

/// The dropped mod folder, held in memory.
#[wasm_bindgen]
pub struct Project {
    pub(crate) fs: MemoryFs,
    findings: Vec<ScanFinding>,
    pub(crate) lock: Option<InstallationLock>,
    pub(crate) state: Option<Value>,
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

impl Project {
    /// A copy of the folder for an export, so the scan can be reused.
    pub(crate) fn snapshot(&self) -> Project {
        Project {
            fs: self.fs.clone(),
            findings: Vec::new(),
            lock: self.lock.clone(),
            state: self.state.clone(),
        }
    }

    /// The descriptor's `name`, when the folder has a readable descriptor.
    pub(crate) fn descriptor_name(&self) -> Option<String> {
        self.descriptor_field("name")
    }

    /// One field of the folder's descriptor, cleaned for display.
    pub(crate) fn descriptor_field(&self, field: &str) -> Option<String> {
        let bytes = self.fs.read("descriptor.mod", 1024 * 1024).ok()?;
        let parsed = parse_descriptor(&bytes).ok()?;
        parsed
            .fields
            .get(field)
            .map(|value| hoi4_setup_core::profile::clean_text(value, 256))
    }
}

#[wasm_bindgen]
impl Project {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Project {
        Project {
            fs: MemoryFs::new(),
            findings: Vec::new(),
            lock: None,
            state: None,
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

    /// Read-only scan: `{ findings, detected, descriptor, stats, issues,
    /// existing_setup }`.
    pub fn scan(&mut self) -> Result<String, JsError> {
        let (mut summary, findings) = scan_project(&self.fs).map_err(js)?;
        self.findings = findings;
        let (lock, setup) = read_existing_setup(&self.fs);
        self.lock = lock;
        self.state = self
            .fs
            .read(STATE_PATH, 1024 * 1024)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        summary["existing_setup"] = setup;
        to_json(&summary)
    }

    /// Managed removal of an existing setup: unchanged managed files are
    /// listed for deletion and no lock is written.
    #[wasm_bindgen(js_name = exportRemoval)]
    pub fn export_removal(&self, request_json: &str) -> Result<crate::Export, JsError> {
        crate::build::prepare_removal(self, request_json).map_err(js)
    }

    /// The scan findings a setup assistant may read, as approved evidence,
    /// without the findings the user excluded. Nothing else from the folder
    /// reaches a provider.
    #[wasm_bindgen(js_name = analysisEvidence)]
    pub fn analysis_evidence(&self, excluded_json: Option<String>) -> Result<String, JsError> {
        let excluded: BTreeSet<String> = match excluded_json.as_deref() {
            Some(json) if !json.is_empty() => serde_json::from_str(json)
                .map_err(|_| JsError::new("the excluded findings are not a list"))?,
            _ => BTreeSet::new(),
        };
        to_json(&evidence_from_findings(&self.findings, &excluded))
    }
}

pub(crate) fn evidence_from_findings(
    findings: &[ScanFinding],
    excluded: &BTreeSet<String>,
) -> Vec<ApprovedEvidence> {
    findings
        .iter()
        .filter(|finding| finding.status != "rejected" && !excluded.contains(&finding.id))
        .filter_map(|finding| {
            let excerpt = finding
                .model_excerpt
                .clone()
                .unwrap_or_else(|| match &finding.value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                });
            // A credential-shaped excerpt is left out rather than failing the
            // whole request.
            if hoi4_setup_core::text::redact_secrets(&excerpt, &[]) != excerpt {
                return None;
            }
            Some(ApprovedEvidence {
                reference: finding.id.clone(),
                path: finding
                    .evidence
                    .first()
                    .map(|evidence| evidence.path.clone())
                    .unwrap_or_default(),
                excerpt_sha256: hoi4_setup_core::text::sha256_bytes(excerpt.as_bytes()),
                excerpt,
                confidence: Some(finding.confidence),
            })
        })
        .collect()
}

/// An existing setup's lock and its summary for the page; an unreadable lock
/// is reported, never trusted.
fn read_existing_setup(fs: &MemoryFs) -> (Option<InstallationLock>, Value) {
    let Ok(bytes) = fs.read(LOCK_PATH, 16 * 1024 * 1024) else {
        return (None, Value::Null);
    };
    match serde_json::from_slice::<InstallationLock>(&bytes) {
        Ok(lock) if lock.schema_version.starts_with("1.") => {
            let summary = json!({
                "valid": true,
                "project_id": lock.project_id,
                "script_prefix": lock.script_prefix,
                "primary_namespace": lock.primary_namespace,
                "installed_at": lock.installed_at,
                "updated_at": lock.updated_at,
                "source": {
                    "mode": lock.source.mode,
                    "revision": lock.source.revision,
                    "requested_ref": lock.source.requested_ref,
                },
                "ai_provider": lock.ai_provider,
                "ai_model": lock.ai_model,
                "primary_coding_environment": lock.primary_coding_environment,
                "additional_coding_environments": lock.additional_coding_environments,
                "components": lock.components.iter().map(|component| component.id.as_str()).collect::<Vec<_>>(),
                "portrait_pipeline": lock.portrait_pipeline,
                "image_generation": lock.image_generation.as_ref().map(|selection| json!({
                    "primary_provider": selection.primary_provider,
                    "additional_providers": selection.additional_providers,
                    "route_settings": selection.route_settings,
                })),
                "live_testing": lock.live_testing,
                "file_count": lock.files.len(),
                "desktop_only": crate::build::desktop_only_reason(&lock),
            });
            (Some(lock), summary)
        }
        Ok(_) => (
            None,
            json!({ "valid": false, "reason": "The installation lock was written by a newer version." }),
        ),
        Err(_) => (
            None,
            json!({ "valid": false, "reason": "The installation lock could not be read." }),
        ),
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

pub(crate) fn scan_project(fs: &MemoryFs) -> Result<(Value, Vec<ScanFinding>), AppError> {
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
    // Values the files decide reach the assistant as one finding, so the core
    // keeps them over its guesses, as on the desktop.
    if !detected.is_empty() {
        let mut finding = scan_finding(
            "profile.detected",
            "detected_values",
            json!(detected),
            "Values decided by the project files: descriptor fields, the dominant namespace, localisation headers, content folders, and documentation.",
        );
        finding.category = "structure".into();
        if let Some(evidence) = finding.evidence.first_mut() {
            evidence.detector = "deterministic_detection".into();
            evidence.path = "@scan/profile/detected".into();
        }
        findings.push(finding);
    }
    let summary = json!({
        "findings": findings.iter().map(|finding| json!({
            "id": finding.id,
            "category": finding.category,
            "key": finding.key,
            "value": finding.value,
            "status": finding.status,
            "confidence": finding.confidence,
            "path": finding.evidence.first().map(|evidence| evidence.path.as_str()),
            "note": finding.evidence.first().and_then(|evidence| evidence.note.as_deref()),
        })).collect::<Vec<_>>(),
        "detected": detected,
        "descriptor": descriptor,
        "stats": profile.stats,
        "issues": profile.problems.iter().collect::<Vec<_>>(),
    });
    Ok((summary, findings))
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
        let (scan, findings) = scan_project(&project.fs).unwrap();
        let detected = scan["detected"].as_array().unwrap();
        assert!(detected
            .iter()
            .any(|value| value["key"] == "descriptor_tags" && value["value"] == json!(["Events"])));
        assert!(detected
            .iter()
            .any(|value| value["key"] == "primary_namespace" && value["value"] == "demo"));
        assert_eq!(scan["descriptor"]["name"], "Demo");
        // The assistant receives the detected values, so the core keeps them.
        let evidence = evidence_from_findings(&findings, &BTreeSet::new());
        let decided = evidence
            .iter()
            .find(|item| item.reference == "profile.detected")
            .expect("detected values reach the assistant");
        let values: Vec<hoi4_setup_core::proposals::DetectedValue> =
            serde_json::from_str(&decided.excerpt).unwrap();
        assert!(values.iter().any(|value| value.key == "primary_namespace"));
        // A finding the user excludes never reaches the assistant.
        let without =
            evidence_from_findings(&findings, &BTreeSet::from(["profile.detected".to_owned()]));
        assert!(without
            .iter()
            .all(|item| item.reference != "profile.detected"));
    }

    #[test]
    fn an_existing_setup_is_read_from_its_lock_and_a_bad_lock_is_not_trusted() {
        let mut project = Project::new();
        project.add_file(LOCK_PATH, b"{ not json").unwrap();
        project.scan().unwrap();
        assert!(project.lock.is_none());
        let (_, setup) = read_existing_setup(&project.fs);
        assert_eq!(setup["valid"], false);
    }
}
