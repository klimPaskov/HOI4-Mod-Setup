//! Analysis, scan-evidence and confirmation gates.
//!
//! Everything here operates on explicit state passed by the caller. The
//! desktop commands pass the process-wide stores they have always used and the
//! service facade passes the state of one session, so both paths enforce the
//! same gates with the same code: evidence is bound to the latest completed
//! scan and the project-root identity it observed, an analysis is usable only
//! after an explicit confirmation of exact values, and a plan can be built
//! only from a confirmed analysis that is still present in the caller's state.

use crate::codex::{ApprovedEvidence, CodexAnalysis, CodexAnalysisRequest};
use crate::models::*;
use crate::paths::validate_project_root;
use crate::safe_fs::RootedDir;
use crate::security::sha256_bytes;
use crate::AppError;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The number of unconfirmed or confirmed analyses a store keeps.
pub(crate) const MAX_RETAINED_ANALYSES: usize = 32;

pub(crate) struct PendingCodexAnalysis {
    pub(crate) analysis: CodexAnalysis,
    pub(crate) record: CodexAnalysisRecord,
    pub(crate) confirmed: Option<CodexAnalysisRecord>,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) scan_id: Option<Uuid>,
    pub(crate) endpoint_fingerprint: Option<String>,
    pub(crate) confirmed_values_sha256: Option<String>,
}

#[derive(Default)]
pub(crate) struct ApprovedScanEvidence {
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) project_root_identity: Option<String>,
    pub(crate) scan_id: Option<Uuid>,
    pub(crate) entries: HashMap<String, Vec<(String, String)>>,
    /// Hash of the exact evidence vector the user approved for the next
    /// semantic turn. A completed scan is not itself an approval.
    pub(crate) evidence_sha256: Option<String>,
}

pub(crate) fn validate_approved_project_root_identity(
    project_root: &Path,
    approved: &ApprovedScanEvidence,
    expected_scan_id: Option<Uuid>,
) -> Result<String, AppError> {
    if expected_scan_id.is_some_and(|scan_id| approved.scan_id != Some(scan_id)) {
        return Err(AppError::PathSecurity(
            "approved project-root identity belongs to a different scan".into(),
        ));
    }
    let expected = approved.project_root_identity.as_deref().ok_or_else(|| {
        AppError::PathSecurity("approved scan has no project-root identity".into())
    })?;
    let observed = RootedDir::open_read(project_root)?.identity_token()?;
    if observed != expected {
        return Err(AppError::PathSecurity(
            "project root changed after the approved read-only scan".into(),
        ));
    }
    Ok(observed)
}

pub(crate) fn same_project_root(left: &Path, right: &Path) -> bool {
    if cfg!(target_os = "windows") {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

pub(crate) fn canonical_component_recommendations(value: &Value) -> Result<Value, AppError> {
    let mut recommendations: Vec<crate::codex::ComponentRecommendation> =
        serde_json::from_value(value.clone())?;
    if recommendations.len() > 64 {
        return Err(AppError::InvalidInput(
            "confirmed component recommendations exceed the supported limit".into(),
        ));
    }
    recommendations.sort_by(|left, right| left.component_id.cmp(&right.component_id));
    if recommendations
        .windows(2)
        .any(|pair| pair[0].component_id == pair[1].component_id)
    {
        return Err(AppError::InvalidInput(
            "confirmed component recommendations contain duplicate IDs".into(),
        ));
    }
    serde_json::to_value(recommendations).map_err(AppError::from)
}

pub(crate) fn canonical_confirmation_values(value: &Value) -> Result<Value, AppError> {
    let object = value.as_object().ok_or_else(|| {
        AppError::InvalidInput("confirmed render values must be an object".into())
    })?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "identity"
                | "description"
                | "folderProfile"
                | "conventions"
                | "componentRecommendations"
        )
    }) {
        return Err(AppError::InvalidInput(
            "confirmed render values contain an unsupported field".into(),
        ));
    }
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidInput("confirmed description is missing".into()))?;
    crate::codex::validate_proposal_value(
        &crate::codex::ProposalKey::ProjectDescription,
        &Value::String(description.to_owned()),
    )?;
    let folder_profile = object
        .get("folderProfile")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::InvalidInput("confirmed folder profile is missing".into()))?;
    let folders = folder_profile
        .iter()
        .map(|folder| {
            folder
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| AppError::InvalidInput("confirmed folder profile is invalid".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    crate::codex::validate_folder_profile_paths(&folders)?;

    let identity = object
        .get("identity")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::InvalidInput("confirmed project identity is missing".into()))?;
    const IDENTITY_FIELDS: &[(&str, bool)] = &[
        ("displayName", true),
        ("projectId", true),
        ("author", true),
        ("version", true),
        ("supportedGameVersion", true),
        ("projectRoot", true),
        ("defaultBranch", true),
        ("scriptPrefix", false),
        ("primaryNamespace", false),
        ("descriptorTags", false),
        ("launcherDescriptorPath", false),
    ];
    if identity
        .keys()
        .any(|key| !IDENTITY_FIELDS.iter().any(|(name, _)| name == key))
    {
        return Err(AppError::InvalidInput(
            "confirmed project identity contains an unsupported field".into(),
        ));
    }
    let mut canonical_identity = serde_json::Map::new();
    for (field, required) in IDENTITY_FIELDS {
        let current = identity.get(*field).cloned().unwrap_or(Value::Null);
        if *required && !current.is_string() {
            return Err(AppError::InvalidInput(format!(
                "confirmed project identity field is missing: {field}"
            )));
        }
        if *field == "descriptorTags" {
            if !current.is_null()
                && (!current.is_array()
                    || current
                        .as_array()
                        .is_some_and(|values| values.iter().any(|value| !value.is_string())))
            {
                return Err(AppError::InvalidInput(
                    "confirmed descriptor tags are invalid".into(),
                ));
            }
        } else if !current.is_null() && !current.is_string() {
            return Err(AppError::InvalidInput(format!(
                "confirmed project identity field is invalid: {field}"
            )));
        }
        canonical_identity.insert((*field).into(), current);
    }
    let conventions = object
        .get("conventions")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::InvalidInput("confirmed project conventions are missing".into())
        })?;
    const CONVENTION_FIELDS: &[(&str, crate::codex::ProposalKey)] = &[
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
    if conventions
        .keys()
        .any(|key| !CONVENTION_FIELDS.iter().any(|(name, _)| name == key))
    {
        return Err(AppError::InvalidInput(
            "confirmed project conventions contain an unsupported field".into(),
        ));
    }
    let mut canonical_conventions = serde_json::Map::new();
    for (field, proposal_key) in CONVENTION_FIELDS {
        let value = conventions
            .get(*field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                AppError::InvalidInput(format!("confirmed project convention is missing: {field}"))
            })?;
        crate::codex::validate_proposal_value(proposal_key, &Value::String(value.to_owned()))?;
        canonical_conventions.insert((*field).into(), Value::String(value.to_owned()));
    }
    let component_recommendations = object.get("componentRecommendations").ok_or_else(|| {
        AppError::InvalidInput("confirmed component recommendations are missing".into())
    })?;
    let component_recommendations = canonical_component_recommendations(component_recommendations)?;
    Ok(serde_json::json!({
        "description": description,
        "folderProfile": folders,
        "identity": canonical_identity,
        "conventions": canonical_conventions,
        "componentRecommendations": component_recommendations,
    }))
}

pub(crate) fn confirmation_values_sha256(value: &Value) -> Result<String, AppError> {
    Ok(sha256_bytes(&serde_json::to_vec(
        &canonical_confirmation_values(value)?,
    )?))
}

pub(crate) fn confirmation_values_from_state(state: &Value) -> Result<Value, AppError> {
    Ok(serde_json::json!({
        "description": state.get("description").cloned().unwrap_or(Value::String(String::new())),
        "folderProfile": state.get("folderProfile").cloned().unwrap_or_else(|| serde_json::json!([])),
        "identity": state.get("identity").cloned().unwrap_or(Value::Null),
        "conventions": state.get("conventions").cloned().unwrap_or(Value::Null),
        "componentRecommendations": state
            .pointer("/codexAnalysis/component_recommendations")
            .cloned()
            .or_else(|| state.get("componentRecommendations").cloned())
            .unwrap_or_else(|| serde_json::json!([])),
    }))
}

pub(crate) fn scan_evidence_entries(
    result: &ScanResult,
) -> Result<HashMap<String, Vec<(String, String)>>, String> {
    let mut entries = HashMap::<String, Vec<(String, String)>>::new();
    for finding in &result.findings {
        let excerpt = if let Some(excerpt) = &finding.model_excerpt {
            excerpt.clone()
        } else {
            match &finding.value {
                Value::String(value) => value.clone(),
                value => serde_json::to_string(value)
                    .map_err(|error| format!("scan evidence could not be serialized: {error}"))?,
            }
        };
        let excerpt_sha256 = sha256_bytes(excerpt.as_bytes());
        let finding_entries = entries.entry(finding.id.clone()).or_default();
        for evidence in &finding.evidence {
            finding_entries.push((evidence.path.clone(), excerpt_sha256.clone()));
        }
    }
    for conflict in &result.conflicts {
        let excerpt = conflict
            .details
            .clone()
            .unwrap_or_else(|| conflict.kind.clone());
        entries
            .entry(conflict.id.clone())
            .or_default()
            .push((conflict.path.clone(), sha256_bytes(excerpt.as_bytes())));
    }
    if entries.len() > 4096 {
        return Err("scan produced too many evidence references for a safe Codex review".into());
    }
    Ok(entries)
}

pub(crate) fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub(crate) fn ai_provider_from_state(state: &Value) -> Result<String, AppError> {
    let provider = match state.get("aiProvider") {
        None => "codex",
        Some(value) => value.as_str().ok_or_else(|| {
            AppError::InvalidInput("selected AI provider must be a string".into())
        })?,
    }
    .trim();
    if provider.is_empty() {
        return Err(AppError::InvalidInput(
            "selected AI provider cannot be empty".into(),
        ));
    }
    if crate::ai::profile(provider).is_none() {
        return Err(AppError::InvalidInput(format!(
            "unsupported AI provider: {provider}"
        )));
    }
    Ok(provider.to_string())
}

pub(crate) fn ai_model_from_state(state: &Value) -> String {
    state
        .get("aiModel")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("default")
        .to_string()
}

pub(crate) fn ai_reasoning_effort_from_state(state: &Value) -> String {
    state
        .get("aiReasoningEffort")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("xhigh")
        .to_string()
}

pub(crate) fn ai_endpoint_from_state(state: &Value) -> Option<String> {
    state
        .get("aiEndpoint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Make a finished scan the only scan the store knows. A cancelled or partial
/// scan leaves the store empty, so its evidence cannot be approved.
///
/// The store represents only the most recently completed scan. Replacing it
/// prevents an evidence reference from one project from being replayed after
/// the user switches projects.
pub(crate) fn record_completed_scan(
    approved: &mut ApprovedScanEvidence,
    root: PathBuf,
    root_identity: String,
    result: &ScanResult,
) -> Result<(), String> {
    if result.cancelled || result.partial {
        *approved = ApprovedScanEvidence::default();
    } else {
        let entries = scan_evidence_entries(result)?;
        approved.project_root = Some(root);
        approved.project_root_identity = Some(root_identity);
        approved.scan_id = Some(result.scan_id);
        approved.entries = entries;
        approved.evidence_sha256 = None;
    }
    Ok(())
}

/// Approve the exact evidence vector that will be sent for analysis. Every
/// item must be a value the core itself scanned in the latest scan of this
/// root.
pub(crate) fn approve_evidence(
    approved: &mut ApprovedScanEvidence,
    project_root: &Path,
    scan_id: Uuid,
    evidence: &[ApprovedEvidence],
) -> Result<(), String> {
    if evidence
        .iter()
        .any(|item| item.reference == crate::scanner::LOCAL_ONLY_PORTRAIT_ROUTES_FINDING)
    {
        return Err(
            "local portrait routes stay on this computer and cannot be sent for analysis".into(),
        );
    }
    crate::codex::validate_analysis_evidence(evidence).map_err(|error| error.to_string())?;
    if approved
        .project_root
        .as_ref()
        .is_none_or(|root| !same_project_root(root, project_root))
        || approved.scan_id != Some(scan_id)
    {
        return Err("scan evidence is stale or belongs to a different project".into());
    }
    validate_approved_project_root_identity(project_root, approved, Some(scan_id))
        .map_err(|error| error.to_string())?;
    let approved_sha256 =
        crate::codex::evidence_manifest_sha256(evidence).map_err(|error| error.to_string())?;
    for item in evidence {
        let entries = approved.entries.get_mut(&item.reference).ok_or_else(|| {
            format!(
                "scan evidence reference is not present in the completed scan: {}",
                item.reference
            )
        })?;
        if !entries.iter().any(|(path, _)| path == &item.path) {
            return Err(format!(
                "scan evidence path is not present in the completed scan: {}",
                item.path
            ));
        }
        let hash = &item.excerpt_sha256;
        if !entries
            .iter()
            .any(|(path, existing)| path == &item.path && existing == hash)
        {
            return Err(format!(
                "scan evidence excerpt is not the exact core-scanned value: {}",
                item.reference
            ));
        }
    }
    approved.evidence_sha256 = Some(approved_sha256);
    Ok(())
}

/// Check that an analysis request carries only evidence the user approved
/// after the latest read-only scan of the same root.
///
/// `validate_context` checks one user-attached context document. The desktop
/// passes its attachment store; a caller with no attachment store passes a
/// function that refuses every document.
pub(crate) fn validate_evidence_request(
    approved: &ApprovedScanEvidence,
    request: &CodexAnalysisRequest,
    validate_context: &dyn Fn(&str, &str, &str) -> Result<(), AppError>,
) -> Result<(), AppError> {
    if request.evidence.is_empty() {
        if request.analysis_purpose.as_deref() == Some("maintenance_reanalysis") {
            return Err(AppError::Credential(
                "maintenance analysis requires approved read-only evidence".into(),
            ));
        }
        return Ok(());
    }
    if matches!(
        request.analysis_purpose.as_deref(),
        Some("existing_project_import") | Some("maintenance_reanalysis")
    ) {
        let requested_root = request.project_root.as_deref().ok_or_else(|| {
            AppError::PathSecurity("existing-project analysis has no project root".into())
        })?;
        let requested_root = validate_project_root(Path::new(requested_root))?;
        let approved_root = approved.project_root.as_ref().ok_or_else(|| {
            AppError::Credential("existing-project analysis has no current read-only scan".into())
        })?;
        if !same_project_root(&requested_root, approved_root) || request.scan_id != approved.scan_id
        {
            return Err(AppError::Credential(
                "existing-project analysis is not bound to the latest read-only scan".into(),
            ));
        }
        validate_approved_project_root_identity(&requested_root, approved, request.scan_id)?;
    }
    // Documents the user added are bound by the context store; the scan
    // approval covers only scan evidence.
    let (context, scan): (Vec<_>, Vec<_>) = request
        .evidence
        .iter()
        .cloned()
        .partition(|evidence| crate::context::is_context_evidence_path(&evidence.path));
    for evidence in &context {
        validate_context(
            &evidence.reference,
            &evidence.path,
            &evidence.excerpt_sha256,
        )?;
    }
    let mut references = BTreeSet::new();
    for evidence in &context {
        if !references.insert(evidence.reference.as_str()) {
            return Err(AppError::InvalidInput(
                "Codex evidence references must be unique".into(),
            ));
        }
    }
    if scan.is_empty() {
        if request.analysis_purpose.as_deref() == Some("maintenance_reanalysis") {
            return Err(AppError::Credential(
                "maintenance analysis requires approved read-only evidence".into(),
            ));
        }
        if request.analysis_purpose.as_deref() != Some("existing_project_import") {
            return Ok(());
        }
    }
    let requested_evidence_sha256 = crate::codex::evidence_manifest_sha256(&scan)?;
    if approved.evidence_sha256.as_deref() != Some(requested_evidence_sha256.as_str()) {
        return Err(AppError::Credential(
            "Codex evidence must be explicitly approved after the latest read-only scan".into(),
        ));
    }
    for evidence in &scan {
        if !references.insert(evidence.reference.as_str()) {
            return Err(AppError::InvalidInput(
                "Codex evidence references must be unique".into(),
            ));
        }
        let matches = approved
            .entries
            .get(&evidence.reference)
            .is_some_and(|items| {
                items.iter().any(|(path, excerpt_sha256)| {
                    path == &evidence.path && excerpt_sha256 == &evidence.excerpt_sha256
                })
            });
        if !matches {
            return Err(AppError::InvalidInput(format!(
                "Codex evidence is not bound to a completed read-only scan: {}",
                evidence.reference
            )));
        }
    }
    Ok(())
}

/// Keep one analysis in the store, dropping an arbitrary old entry when the
/// store is full.
pub(crate) fn retain_analysis(
    analyses: &mut HashMap<Uuid, PendingCodexAnalysis>,
    analysis_id: Uuid,
    pending: PendingCodexAnalysis,
) {
    if analyses.len() >= MAX_RETAINED_ANALYSES {
        if let Some(oldest) = analyses.keys().next().copied() {
            analyses.remove(&oldest);
        }
    }
    analyses.insert(analysis_id, pending);
}

/// Confirm an analysis the store still holds. The confirmation is bound to the
/// analysis the provider returned: only the selected proposal keys and the
/// exact reviewed values come from the caller.
pub(crate) fn confirm_analysis(
    analyses: &mut HashMap<Uuid, PendingCodexAnalysis>,
    record: &CodexAnalysisRecord,
    confirmed_fields: &[String],
    confirmed_values: &Value,
) -> Result<CodexAnalysisRecord, String> {
    let canonical_values =
        canonical_confirmation_values(confirmed_values).map_err(|error| error.to_string())?;
    let canonical_bytes = serde_json::to_vec(&canonical_values)
        .map_err(AppError::from)
        .map_err(|error| error.to_string())?;
    let confirmed_values_sha256 = sha256_bytes(&canonical_bytes);
    let pending = analyses
        .get_mut(&record.analysis_id)
        .ok_or_else(|| "Codex analysis is no longer available in the core session".to_string())?;
    let expected_recommendations = canonical_component_recommendations(
        &serde_json::to_value(&pending.analysis.component_recommendations)
            .map_err(AppError::from)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if canonical_values.get("componentRecommendations") != Some(&expected_recommendations) {
        return Err(
            "component recommendations changed after the selected provider returned its analysis"
                .into(),
        );
    }
    if let Some(confirmed) = &pending.confirmed {
        if confirmed.confirmed_fields == confirmed_fields {
            if pending.confirmed_values_sha256.as_deref() == Some(confirmed_values_sha256.as_str())
            {
                return Ok(confirmed.clone());
            }
            pending.confirmed_values_sha256 = Some(confirmed_values_sha256);
            return Ok(confirmed.clone());
        }
        return Err("Codex analysis has already been confirmed with different fields".into());
    }
    let confirmed =
        crate::codex::confirm_analysis_record(&pending.record, &pending.analysis, confirmed_fields)
            .map_err(|error| error.to_string())?;
    pending.confirmed = Some(confirmed.clone());
    pending.confirmed_values_sha256 = Some(confirmed_values_sha256);
    Ok(confirmed)
}

/// Compare two records without the core-only root and scan bindings, which
/// never cross the renderer.
pub(crate) fn same_persisted_codex_record(
    left: &CodexAnalysisRecord,
    right: &CodexAnalysisRecord,
) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.project_root = None;
    left.scan_id = None;
    right.project_root = None;
    right.scan_id = None;
    left == right
}

/// Whether the store holds this exact confirmed record, bound to the expected
/// root, scan, endpoint and reviewed values.
pub(crate) fn record_confirmed_in_session(
    analyses: &HashMap<Uuid, PendingCodexAnalysis>,
    record: &CodexAnalysisRecord,
    expected_project_root: Option<&Path>,
    expected_scan_id: Option<Uuid>,
    expected_endpoint_fingerprint: Option<&str>,
    expected_confirmation_values_sha256: Option<&str>,
) -> bool {
    analyses.get(&record.analysis_id).is_some_and(|pending| {
        pending
            .confirmed
            .as_ref()
            .is_some_and(|confirmed| same_persisted_codex_record(confirmed, record))
            && expected_endpoint_fingerprint
                .is_none_or(|expected| pending.endpoint_fingerprint.as_deref() == Some(expected))
            && expected_confirmation_values_sha256
                .is_none_or(|expected| pending.confirmed_values_sha256.as_deref() == Some(expected))
            && if record.analysis_purpose.as_deref().is_some_and(|purpose| {
                matches!(
                    purpose,
                    "existing_project_import" | "maintenance_reanalysis"
                )
            }) {
                expected_project_root.is_none_or(|root| {
                    pending
                        .project_root
                        .as_deref()
                        .is_some_and(|pending_root| same_project_root(pending_root, root))
                }) && expected_scan_id.is_none_or(|scan_id| pending.scan_id == Some(scan_id))
            } else {
                true
            }
    })
}

/// The confirmed analysis a plan is built from. The record in `state` must be
/// confirmed in `analyses`, match the selected provider, model, reasoning
/// effort and profile, and carry the values the reviewer confirmed.
pub(crate) fn confirmed_analysis_for_plan(
    analyses: &HashMap<Uuid, PendingCodexAnalysis>,
    state: &Value,
    project_root: &Path,
) -> Result<CodexAnalysisRecord, AppError> {
    let value = state
        .get("codexAnalysisRecord")
        .or_else(|| state.pointer("/codexAnalysis/record"))
        .ok_or_else(|| {
            AppError::Credential(
                "selected-provider analysis must be confirmed before planning".into(),
            )
        })?;
    let record: CodexAnalysisRecord = serde_json::from_value(value.clone())?;
    crate::codex::validate_confirmed_record(&record)?;
    let provider = ai_provider_from_state(state)?;
    if record.provider.as_deref().unwrap_or("codex") != provider {
        return Err(AppError::Credential(
            "confirmed analysis belongs to a different AI provider".into(),
        ));
    }
    let profile = crate::ai::profile(&provider).ok_or_else(|| {
        AppError::Credential("confirmed analysis uses an unsupported AI provider".into())
    })?;
    let selected_model = ai_model_from_state(state);
    let selected_reasoning_effort = ai_reasoning_effort_from_state(state);
    if record.model.as_deref() != Some(selected_model.as_str())
        || record.reasoning_effort.as_deref() != Some(selected_reasoning_effort.as_str())
        || record.optimization_profile.as_deref() != Some(profile.optimization_profile.as_str())
    {
        return Err(AppError::Credential(
            "confirmed analysis does not match the selected provider, model, reasoning effort, or optimization profile".into(),
        ));
    }
    let expected_scan_id = state
        .pointer("/scanContext/scanId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let endpoint_fingerprint = (provider != "codex")
        .then(|| sha256_bytes(ai_endpoint_from_state(state).unwrap_or_default().as_bytes()));
    let expected_confirmation_values_sha256 =
        confirmation_values_sha256(&confirmation_values_from_state(state)?)?;
    if !record_confirmed_in_session(
        analyses,
        &record,
        Some(project_root),
        expected_scan_id,
        endpoint_fingerprint.as_deref(),
        Some(&expected_confirmation_values_sha256),
    ) {
        return Err(AppError::Credential(
            "selected-provider confirmation is no longer present in the core session; rerun semantic analysis"
                .into(),
        ));
    }
    // The project root and scan ID are core-only bindings that never cross
    // the renderer; restore them from the confirmed session entry so plan
    // construction can bind the plan to the same scan.
    let mut record = record;
    if let Some(pending) = analyses.get(&record.analysis_id) {
        record.scan_id = pending.scan_id;
        record.project_root = pending
            .project_root
            .as_ref()
            .map(|root| root.display().to_string());
    }
    Ok(record)
}

/// The root identity a plan binds. An existing root must be the root of the
/// latest scan and unchanged since it. A root to be created binds its parent.
pub(crate) fn plan_root_identity(
    approved: &ApprovedScanEvidence,
    root: &Path,
    root_exists: bool,
    analysis_scan_id: Option<Uuid>,
) -> Result<String, AppError> {
    if root_exists {
        if approved
            .project_root
            .as_ref()
            .is_none_or(|approved_root| !same_project_root(approved_root, root))
            || approved.scan_id != analysis_scan_id
        {
            return Err(AppError::PathSecurity(
                "project plan is not bound to the latest scan of this root".into(),
            ));
        }
        validate_approved_project_root_identity(root, approved, analysis_scan_id)
    } else {
        let parent = root.parent().ok_or_else(|| {
            AppError::PathSecurity("new project root has no existing parent".into())
        })?;
        RootedDir::open_read(parent)?.identity_token()
    }
}

/// Tell the analysis which components the resolved source offers, so a
/// recommendation can name only components that exist at that revision.
pub(crate) fn bind_component_registry(
    request: &mut CodexAnalysisRequest,
    resolution: &crate::source::SourceResolution,
) -> Result<(), AppError> {
    let mut component_ids = resolution
        .manifest
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect::<Vec<_>>();
    component_ids.sort();
    component_ids.dedup();
    let constraints = request.constraints.as_object_mut().ok_or_else(|| {
        AppError::InvalidInput("semantic analysis constraints must be an object".into())
    })?;
    constraints.insert(
        "component_registry".into(),
        serde_json::json!({
            "source_revision": resolution.identity.resolved_revision,
            "manifest_sha256": resolution.identity.manifest_sha256,
            "component_ids": component_ids,
        }),
    );
    Ok(())
}

/// The project root a plan state names, before any containment or existence
/// check. The state carries it either as an object or as a JSON string.
pub(crate) fn project_root_field(state: &Value) -> Result<PathBuf, AppError> {
    let root = string_field(state, "identity")
        .and_then(|identity| serde_json::from_str::<Value>(&identity).ok())
        .and_then(|identity| string_field(&identity, "projectRoot"))
        .or_else(|| {
            state
                .pointer("/identity/projectRoot")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::InvalidInput(
                "choose a project folder before creating an installation plan".into(),
            )
        })?;
    Ok(PathBuf::from(root))
}
