//! The only filesystem, network, process, and credential boundary exposed to React.

use crate::codex::{
    find_codex_executable, missing_status, AppServerProtocol, CodexAccountStatus, CodexAnalysis,
    CodexAnalysisRequest, CodexAnalysisResult, CodexLoginStart, ProcessJsonlTransport,
};
use crate::credentials::{
    provider_name, save_meshy_key, validate_credential_reference, CredentialStore,
    OsCredentialStore, ScopedSecretEnvironment, MESHY_ENVIRONMENT_NAME,
};
use crate::descriptors::generated_artifacts as render_generated_artifacts;
use crate::merge::{
    allowed_choices, structured_json_merge, structured_toml_merge, three_way_merge,
    validate_merged_result, FileKind,
};
use crate::models::{CredentialReference, *};
use crate::paths::{application_data_root, validate_project_root};
use crate::readiness::ReadinessInput;
use crate::scanner::{scan_project_with_progress as scan_project_files, ScanOptions, ScanProgress};
use crate::security::{
    path_has_link_component, redact_secrets, safe_join, sha256_bytes, sha256_file,
    validate_external_destination,
};
use crate::source::{
    expand_components, resolve_source, select_component_files, verify_download, HttpSourceClient,
    SourceRequest,
};
use crate::transaction::{
    discard_staging, resume_transaction, run_transaction, TransactionOptions,
};
use crate::AppError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::Emitter;
use uuid::Uuid;

struct PreparedPlan {
    plan: InstallationPlan,
    prepared_files: Vec<crate::models::PreparedFile>,
    merge_contexts: HashMap<String, MergeContext>,
    canonical_root: PathBuf,
    approved: bool,
}

#[derive(Clone)]
struct MergeContext {
    kind: FileKind,
    base: Vec<u8>,
    local: Vec<u8>,
    incoming: Vec<u8>,
}

const CONFLICT_PREVIEW_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, serde::Serialize)]
struct ConflictPreview {
    path: String,
    kind: String,
    base: Option<String>,
    local: Option<String>,
    incoming: Option<String>,
    base_sha256: Option<String>,
    local_sha256: Option<String>,
    incoming_sha256: Option<String>,
    truncated: bool,
    redacted: bool,
}

#[derive(Debug, serde::Serialize)]
struct FolderSelection {
    path: Option<String>,
    error: Option<String>,
    cancelled: bool,
}

#[derive(Debug, serde::Serialize)]
struct SourceManifestPreview {
    schema_version: String,
    manifest_id: String,
    source: SourceIdentity,
    repository: RepositoryDescriptor,
    components: Vec<ComponentDefinition>,
}

#[derive(Debug, serde::Serialize)]
struct WorkflowHealthResult {
    status: String,
    exit_code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ScanProgressEvent {
    request_id: String,
    #[serde(flatten)]
    progress: ScanProgress,
}

#[derive(Debug, Clone)]
struct CachedThreeDHealth {
    lock_fingerprint: String,
    state: String,
}

static PREPARED_PLANS: OnceLock<Mutex<std::collections::HashMap<Uuid, PreparedPlan>>> =
    OnceLock::new();
static CODEX_SESSION: OnceLock<Mutex<Option<AppServerProtocol<ProcessJsonlTransport>>>> =
    OnceLock::new();
static CODEX_ANALYSES: OnceLock<Mutex<HashMap<Uuid, PendingCodexAnalysis>>> = OnceLock::new();
static CODEX_APPROVED_EVIDENCE: OnceLock<Mutex<ApprovedScanEvidence>> = OnceLock::new();
static CODEX_LOGIN_CANCEL: AtomicBool = AtomicBool::new(false);
static SCAN_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
static MESHY_CREDENTIAL_REFERENCE: OnceLock<Mutex<Option<CredentialReference>>> = OnceLock::new();
static THREE_D_HEALTH: OnceLock<Mutex<HashMap<String, CachedThreeDHealth>>> = OnceLock::new();

struct PendingCodexAnalysis {
    analysis: CodexAnalysis,
    record: CodexAnalysisRecord,
    confirmed: Option<CodexAnalysisRecord>,
    project_root: Option<PathBuf>,
    scan_id: Option<Uuid>,
}

#[derive(Default)]
struct ApprovedScanEvidence {
    project_root: Option<PathBuf>,
    scan_id: Option<Uuid>,
    entries: HashMap<String, Vec<(String, String)>>,
}

fn prepared_plans() -> &'static Mutex<std::collections::HashMap<Uuid, PreparedPlan>> {
    PREPARED_PLANS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn codex_session() -> &'static Mutex<Option<AppServerProtocol<ProcessJsonlTransport>>> {
    CODEX_SESSION.get_or_init(|| Mutex::new(None))
}

fn scan_cancellations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    SCAN_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn codex_analyses() -> &'static Mutex<HashMap<Uuid, PendingCodexAnalysis>> {
    CODEX_ANALYSES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn codex_approved_evidence() -> &'static Mutex<ApprovedScanEvidence> {
    CODEX_APPROVED_EVIDENCE.get_or_init(|| Mutex::new(ApprovedScanEvidence::default()))
}

fn clear_approved_scan_evidence() -> Result<(), String> {
    *codex_approved_evidence()
        .lock()
        .map_err(|_| "approved scan evidence store is unavailable".to_string())? =
        ApprovedScanEvidence::default();
    Ok(())
}

fn meshy_credential_reference() -> &'static Mutex<Option<CredentialReference>> {
    MESHY_CREDENTIAL_REFERENCE.get_or_init(|| Mutex::new(None))
}

fn three_d_health() -> &'static Mutex<HashMap<String, CachedThreeDHealth>> {
    THREE_D_HEALTH.get_or_init(|| Mutex::new(HashMap::new()))
}

fn invalidate_three_d_health() {
    if let Ok(mut entries) = three_d_health().lock() {
        entries.clear();
    }
}

fn three_d_lock_fingerprint(lock: &InstallationLock) -> Result<String, AppError> {
    let mut files = lock
        .files
        .iter()
        .filter(|file| file.component_id == "workflow.3d")
        .map(|file| {
            (
                file.path.as_str(),
                file.external,
                file.installed_sha256.as_str(),
                file.installed_size,
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(right.0).then(left.1.cmp(&right.1)));
    Ok(sha256_bytes(&serde_json::to_vec(&(
        lock.source.revision.as_str(),
        lock.source.manifest_sha256.as_str(),
        lock.optional_workflows.get("workflow.3d"),
        files,
    ))?))
}

fn normalized_three_d_state(
    lock_state: &str,
    cached_state: Option<&str>,
    fingerprint_matches: bool,
) -> String {
    if !matches!(lock_state, "selected_pending" | "incomplete" | "ready") {
        return lock_state.to_string();
    }
    if fingerprint_matches {
        if let Some(state) = cached_state.filter(|state| matches!(*state, "ready" | "incomplete")) {
            return state.to_string();
        }
    }
    if lock_state == "selected_pending" {
        "incomplete".into()
    } else {
        lock_state.to_string()
    }
}

fn cached_three_d_state(project_root: &Path, lock: &InstallationLock) -> String {
    let lock_state = lock
        .optional_workflows
        .get("workflow.3d")
        .map(|workflow| workflow.state.as_str())
        .unwrap_or("not_selected");
    if !matches!(lock_state, "selected_pending" | "incomplete" | "ready") {
        return lock_state.into();
    }
    let Ok(fingerprint) = three_d_lock_fingerprint(lock) else {
        return normalized_three_d_state(lock_state, None, false);
    };
    let key = project_root.to_string_lossy().into_owned();
    let cached = three_d_health()
        .lock()
        .ok()
        .and_then(|entries| entries.get(&key).cloned());
    normalized_three_d_state(
        lock_state,
        cached.as_ref().map(|entry| entry.state.as_str()),
        cached
            .as_ref()
            .is_some_and(|entry| entry.lock_fingerprint == fingerprint),
    )
}

fn record_three_d_health(
    project_root: &Path,
    lock: &InstallationLock,
    state: &str,
) -> Result<(), AppError> {
    if !matches!(state, "ready" | "incomplete") {
        return Err(AppError::InvalidInput(
            "3D health state is not recordable".into(),
        ));
    }
    let fingerprint = three_d_lock_fingerprint(lock)?;
    let key = project_root.to_string_lossy().into_owned();
    let mut entries = three_d_health()
        .lock()
        .map_err(|_| AppError::Process("3D health cache is unavailable".into()))?;
    if entries.len() >= 32 && !entries.contains_key(&key) {
        if let Some(oldest) = entries.keys().next().cloned() {
            entries.remove(&oldest);
        }
    }
    entries.insert(
        key,
        CachedThreeDHealth {
            lock_fingerprint: fingerprint,
            state: state.into(),
        },
    );
    Ok(())
}

fn with_codex_session<R>(
    callback: impl FnOnce(&mut AppServerProtocol<ProcessJsonlTransport>) -> Result<R, AppError>,
) -> Result<R, AppError> {
    let mut session = codex_session()
        .lock()
        .map_err(|_| AppError::Process("Codex session lock is unavailable".into()))?;
    if session
        .as_mut()
        .is_some_and(|protocol| !protocol.is_alive())
    {
        // Drop the dead transport before creating a replacement. The
        // transport's Drop implementation supervises and terminates any
        // remaining child tree, while Codex retains authentication state.
        *session = None;
    }
    if session.is_none() {
        let executable = find_codex_executable().ok_or_else(|| {
            AppError::Process("official Codex executable was not found on the reviewed PATH".into())
        })?;
        let mut protocol = AppServerProtocol::new(ProcessJsonlTransport::start(executable)?);
        protocol.initialize()?;
        *session = Some(protocol);
    }
    let result = callback(
        session
            .as_mut()
            .expect("Codex session was just initialized"),
    );
    if result.is_err() {
        *session = None;
        if let Ok(mut analyses) = codex_analyses().lock() {
            analyses.clear();
        }
    }
    result
}

fn clear_codex_local_state() -> Result<(), String> {
    codex_analyses()
        .lock()
        .map_err(|_| "Codex analysis store is unavailable".to_string())?
        .clear();
    clear_approved_scan_evidence()?;
    Ok(())
}

fn require_codex_chatgpt_session() -> Result<(), AppError> {
    with_codex_session(|session| {
        let status = session.account_read(false)?;
        validate_codex_planning_account(&status)
    })
}

fn validate_codex_planning_account(status: &CodexAccountStatus) -> Result<(), AppError> {
    if let Some(error) = status.error.as_deref() {
        return Err(AppError::Credential(error.into()));
    }
    if status.usage_limited {
        return Err(AppError::Credential(
            "Codex usage is currently limited; planning is paused without changing the project"
                .into(),
        ));
    }
    if status.available && status.authenticated && status.auth_mode == "chatgpt" {
        Ok(())
    } else {
        Err(AppError::Credential(
            "sign in with ChatGPT through the official Codex App Server before planning".into(),
        ))
    }
}

fn plan_fingerprint(plan: &InstallationPlan) -> Result<String, AppError> {
    let mut comparable = plan.clone();
    comparable.approvals.dry_run_reviewed = false;
    comparable.approvals.external_actions_reviewed = false;
    comparable.approvals.git_remote_approved = false;
    comparable.approvals.push_approved = false;
    let bytes = serde_json::to_vec(&comparable)?;
    Ok(sha256_bytes(&bytes))
}

fn store_prepared_plan(
    plan: InstallationPlan,
    prepared_files: Vec<crate::models::PreparedFile>,
    merge_contexts: HashMap<String, MergeContext>,
    canonical_root: PathBuf,
) -> Result<InstallationPlan, AppError> {
    let id = plan.plan_id;
    prepared_plans()
        .lock()
        .map_err(|_| AppError::Transaction("prepared plan store is unavailable".into()))?
        .insert(
            id,
            PreparedPlan {
                plan: plan.clone(),
                prepared_files,
                merge_contexts,
                canonical_root,
                approved: false,
            },
        );
    Ok(plan)
}

fn command_error(error: AppError) -> String {
    error.to_string()
}

fn journal_bound_to_root(
    journal: &TransactionJournal,
    project_root: &Path,
) -> Result<bool, AppError> {
    if journal.project_root.is_empty() {
        return Ok(false);
    }
    let bound_root = validate_project_root(Path::new(&journal.project_root))?;
    Ok(if cfg!(target_os = "windows") {
        bound_root
            .to_string_lossy()
            .eq_ignore_ascii_case(&project_root.to_string_lossy())
    } else {
        bound_root == project_root
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                CODEX_LOGIN_CANCEL.store(true, Ordering::SeqCst);
                if let Ok(mut session) = codex_session().lock() {
                    *session = None;
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            codex_account_read,
            codex_login_start,
            codex_login_wait,
            codex_login_cancel,
            open_codex_login_url,
            codex_logout,
            codex_analyze,
            confirm_codex_analysis,
            pick_project_folder,
            pick_launcher_folder,
            scan_project,
            cancel_scan,
            store_meshy_credential,
            remove_meshy_credential,
            run_3d_health_check,
            run_mcp_health_check,
            evaluate_readiness,
            preview_descriptors,
            preview_source_manifest,
            preview_installation_conflict,
            build_installation_plan,
            build_maintenance_plan,
            approve_installation,
            resolve_installation_conflict,
            apply_installation,
            rollback_installation,
            read_transaction_journal,
            find_interrupted_transaction,
            resume_installation,
            discard_installation_staging,
            open_in_codex,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HOI4 Mod Setup");
}

#[tauri::command]
fn app_info() -> Value {
    serde_json::json!({
        "product": "HOI4 Mod Setup",
        "source_repository": crate::source::SOURCE_REPOSITORY,
        "manifest": crate::source::MANIFEST_PATH,
        "supported_platforms": ["windows", "macos"],
    })
}

#[tauri::command]
fn codex_account_read() -> CodexAccountStatus {
    match with_codex_session(|session| session.account_read(false)) {
        Ok(status) => status,
        Err(error) => missing_status(error.to_string()),
    }
}

#[tauri::command]
fn codex_login_start(mode: String) -> CodexLoginStart {
    CODEX_LOGIN_CANCEL.store(false, Ordering::SeqCst);
    let device_code = match mode.as_str() {
        "browser" => false,
        "device" => true,
        _ => {
            return CodexLoginStart {
                available: false,
                error: Some("login mode must be browser or device".into()),
                ..Default::default()
            }
        }
    };
    match with_codex_session(|session| session.login_start(device_code)) {
        Ok(start) => start,
        Err(error) => CodexLoginStart {
            available: false,
            error: Some(error.to_string()),
            device_code,
            ..Default::default()
        },
    }
}

#[tauri::command]
fn codex_login_wait(login_id: String) -> Result<CodexAccountStatus, String> {
    let result = with_codex_session(|session| {
        session.wait_for_login_with_cancel(&login_id, Duration::from_secs(120), || {
            CODEX_LOGIN_CANCEL.load(Ordering::SeqCst)
        })
    });
    CODEX_LOGIN_CANCEL.store(false, Ordering::SeqCst);
    result.map_err(command_error)
}

#[tauri::command]
fn codex_login_cancel() -> Result<(), String> {
    CODEX_LOGIN_CANCEL.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn open_codex_login_url(url: String) -> Result<(), String> {
    if !crate::codex::is_safe_login_url(&url) {
        return Err("Codex returned an invalid HTTPS authentication URL".into());
    }
    let executable = crate::process::system_browser_executable()
        .ok_or_else(|| "No supported system-browser opener is available".to_string())?;
    let spec = crate::process::ProcessSpec {
        executable: executable.clone(),
        args: vec![url],
        cwd: None,
        platform: Platform::current(),
        environment_names: Vec::new(),
        timeout_seconds: 10,
        max_output_bytes: 1,
    };
    spec.spawn_detached(&[executable]).map_err(command_error)
}

#[tauri::command]
fn codex_logout() -> Result<(), String> {
    CODEX_LOGIN_CANCEL.store(true, Ordering::SeqCst);
    let logout_result = {
        let mut session = codex_session()
            .lock()
            .map_err(|_| "Codex session lock is unavailable".to_string())?;
        let result = session
            .as_mut()
            .map(|protocol| protocol.logout().map_err(command_error))
            .unwrap_or(Ok(()));
        // Clear the local process even if the remote request failed. This
        // prevents stale analysis state from remaining usable after an
        // attempted sign-out; the returned error still tells the UI that
        // Codex may need a retry.
        *session = None;
        result
    };
    let result = logout_result.and(clear_codex_local_state());
    CODEX_LOGIN_CANCEL.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
fn codex_analyze(request: CodexAnalysisRequest) -> Result<CodexAnalysisResult, String> {
    let mut request = request;
    if let Some(project_root) = request.project_root.as_deref() {
        request.project_root = Some(
            validate_project_root(Path::new(project_root))
                .map_err(command_error)?
                .display()
                .to_string(),
        );
    }
    validate_codex_evidence_approval(&request).map_err(command_error)?;
    let result = with_codex_session(|session| session.analyze(&request)).map_err(command_error)?;
    let mut analyses = codex_analyses()
        .lock()
        .map_err(|_| "Codex analysis store is unavailable".to_string())?;
    if analyses.len() >= 32 {
        if let Some(oldest) = analyses.keys().next().copied() {
            analyses.remove(&oldest);
        }
    }
    analyses.insert(
        result.record.analysis_id,
        PendingCodexAnalysis {
            analysis: result.analysis.clone(),
            record: result.record.clone(),
            confirmed: None,
            project_root: request.project_root.clone().map(PathBuf::from),
            scan_id: request.scan_id,
        },
    );
    Ok(result)
}

fn validate_codex_evidence_approval(request: &CodexAnalysisRequest) -> Result<(), AppError> {
    if request.evidence.is_empty() {
        if request.analysis_purpose.as_deref() == Some("maintenance_reanalysis") {
            return Err(AppError::Credential(
                "maintenance analysis requires approved read-only evidence".into(),
            ));
        }
        return Ok(());
    }
    let approved = codex_approved_evidence().lock().map_err(|_| {
        AppError::Serialization("approved scan evidence store is unavailable".into())
    })?;
    if request.analysis_purpose.as_deref() == Some("maintenance_reanalysis") {
        let requested_root = request.project_root.as_deref().ok_or_else(|| {
            AppError::PathSecurity("maintenance analysis has no project root".into())
        })?;
        let requested_root = validate_project_root(Path::new(requested_root))?;
        let approved_root = approved.project_root.as_ref().ok_or_else(|| {
            AppError::Credential("maintenance analysis has no current read-only scan".into())
        })?;
        if !same_project_root(&requested_root, approved_root) || request.scan_id != approved.scan_id
        {
            return Err(AppError::Credential(
                "maintenance analysis is not bound to the latest read-only scan".into(),
            ));
        }
    }
    let mut references = BTreeSet::new();
    for evidence in &request.evidence {
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

fn same_project_root(left: &Path, right: &Path) -> bool {
    if cfg!(target_os = "windows") {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

#[tauri::command]
fn confirm_codex_analysis(
    record: CodexAnalysisRecord,
    confirmed_fields: Vec<String>,
) -> Result<CodexAnalysisRecord, String> {
    let mut analyses = codex_analyses()
        .lock()
        .map_err(|_| "Codex analysis store is unavailable".to_string())?;
    let pending = analyses
        .get_mut(&record.analysis_id)
        .ok_or_else(|| "Codex analysis is no longer available in the core session".to_string())?;
    if let Some(confirmed) = &pending.confirmed {
        if confirmed.confirmed_fields == confirmed_fields {
            return Ok(confirmed.clone());
        }
        return Err("Codex analysis has already been confirmed with different fields".into());
    }
    let confirmed = crate::codex::confirm_analysis_record(
        &pending.record,
        &pending.analysis,
        &confirmed_fields,
    )
    .map_err(command_error)?;
    pending.confirmed = Some(confirmed.clone());
    Ok(confirmed)
}

#[tauri::command]
async fn pick_project_folder(app: tauri::AppHandle) -> Result<FolderSelection, String> {
    use tauri_plugin_dialog::DialogExt;

    let Some(selected) = app
        .dialog()
        .file()
        .set_title("Choose a HOI4 mod project folder")
        .blocking_pick_folder()
    else {
        return Ok(FolderSelection {
            path: None,
            error: None,
            cancelled: true,
        });
    };
    let path = match selected.into_path() {
        Ok(path) => path,
        Err(error) => {
            return Ok(FolderSelection {
                path: None,
                error: Some(format!("selected folder is not a local path: {error}")),
                cancelled: false,
            });
        }
    };
    match validate_project_root(&path) {
        Ok(canonical) => Ok(FolderSelection {
            path: Some(canonical.to_string_lossy().into_owned()),
            error: None,
            cancelled: false,
        }),
        Err(error) => Ok(FolderSelection {
            path: None,
            error: Some(error.to_string()),
            cancelled: false,
        }),
    }
}

#[tauri::command]
async fn pick_launcher_folder(app: tauri::AppHandle) -> Result<FolderSelection, String> {
    use tauri_plugin_dialog::DialogExt;

    let Some(selected) = app
        .dialog()
        .file()
        .set_title("Choose the HOI4 user mod directory")
        .blocking_pick_folder()
    else {
        return Ok(FolderSelection {
            path: None,
            error: None,
            cancelled: true,
        });
    };
    let path = selected
        .into_path()
        .map_err(|error| format!("selected folder is not a local path: {error}"))?;
    let canonical = validate_project_root(&path).map_err(command_error)?;
    Ok(FolderSelection {
        path: Some(canonical.to_string_lossy().into_owned()),
        error: None,
        cancelled: false,
    })
}

#[tauri::command]
fn scan_project(
    app: tauri::AppHandle,
    root: String,
    request_id: String,
) -> Result<ScanResult, String> {
    validate_scan_request_id(&request_id)?;
    let root = validate_project_root(Path::new(&root)).map_err(command_error)?;
    let cancellation = Arc::new(AtomicBool::new(false));
    {
        let mut active = scan_cancellations()
            .lock()
            .map_err(|_| "scan cancellation store is unavailable".to_string())?;
        if active
            .insert(request_id.clone(), cancellation.clone())
            .is_some()
        {
            return Err("scan request ID is already active".into());
        }
    }
    let event_request_id = request_id.clone();
    let event_app = app.clone();
    let result = scan_project_files(
        &root,
        &ScanOptions {
            cancel_flag: Some(cancellation),
            ..ScanOptions::default()
        },
        |progress| {
            let _ = event_app.emit(
                "scan-progress",
                ScanProgressEvent {
                    request_id: event_request_id.clone(),
                    progress,
                },
            );
        },
    );
    if let Ok(mut active) = scan_cancellations().lock() {
        active.remove(&request_id);
    }
    let result = result.map_err(command_error)?;
    let mut approved = codex_approved_evidence()
        .lock()
        .map_err(|_| "approved scan evidence store is unavailable".to_string())?;
    if result.cancelled || result.partial {
        drop(approved);
        clear_approved_scan_evidence()?;
    } else {
        let mut entries = HashMap::<String, Vec<(String, String)>>::new();
        // The approval store represents only the most recently completed scan.
        // Clearing it prevents an evidence reference from one project from being
        // replayed after the user switches projects in the same desktop session.
        for finding in &result.findings {
            let excerpt = match &finding.value {
                Value::String(value) => value.clone(),
                value => serde_json::to_string(value)
                    .map_err(|error| format!("scan evidence could not be serialized: {error}"))?,
            };
            let excerpt_sha256 = sha256_bytes(excerpt.as_bytes());
            let finding_entries = entries.entry(finding.id.clone()).or_default();
            for evidence in &finding.evidence {
                finding_entries.push((evidence.path.clone(), excerpt_sha256.clone()));
            }
        }
        if entries.len() > 4096 {
            return Err(
                "scan produced too many evidence references for a safe Codex review".into(),
            );
        }
        approved.project_root = Some(root);
        approved.scan_id = Some(result.scan_id);
        approved.entries = entries;
    }
    Ok(result)
}

#[tauri::command]
fn cancel_scan(request_id: String) -> Result<(), String> {
    validate_scan_request_id(&request_id)?;
    let active = scan_cancellations()
        .lock()
        .map_err(|_| "scan cancellation store is unavailable".to_string())?;
    let cancellation = active
        .get(&request_id)
        .ok_or_else(|| "scan is no longer running".to_string())?;
    cancellation.store(true, Ordering::SeqCst);
    Ok(())
}

fn validate_scan_request_id(request_id: &str) -> Result<(), String> {
    if request_id.is_empty() || request_id.len() > 128 || request_id.chars().any(char::is_control) {
        return Err("scan request ID is invalid".into());
    }
    Ok(())
}

#[tauri::command]
fn store_meshy_credential(value: String) -> Result<CredentialReference, String> {
    let reference = save_meshy_key(&OsCredentialStore, &value).map_err(command_error)?;
    invalidate_three_d_health();
    *meshy_credential_reference()
        .lock()
        .map_err(|_| "Meshy credential reference store is unavailable".to_string())? =
        Some(reference.clone());
    Ok(reference)
}

#[tauri::command]
fn remove_meshy_credential(reference: CredentialReference) -> Result<(), String> {
    validate_credential_reference(&reference).map_err(command_error)?;
    OsCredentialStore
        .delete(&reference)
        .map_err(command_error)?;
    invalidate_three_d_health();
    let mut current = meshy_credential_reference()
        .lock()
        .map_err(|_| "Meshy credential reference store is unavailable".to_string())?;
    if current
        .as_ref()
        .is_some_and(|value| value.reference == reference.reference)
    {
        *current = None;
    }
    Ok(())
}

fn resolve_installed_manifest(lock: &InstallationLock) -> Result<RemoteManifest, AppError> {
    if lock.source.manifest_origin == "bundled_revision_bootstrap" {
        let bundled_bytes = include_bytes!("../../source-manifest/hoi4-mod-setup.manifest.json");
        if sha256_bytes(bundled_bytes) != lock.source.manifest_sha256 {
            return Err(AppError::Source(
                "the bundled manifest no longer matches its lock evidence".into(),
            ));
        }
        return crate::source::parse_manifest(bundled_bytes, Some(&lock.source.revision));
    }
    let client = HttpSourceClient::new()?;
    let resolution = resolve_source(
        &client,
        &SourceRequest {
            mode: SourceMode::PinnedCommit,
            requested_ref: Some(lock.source.revision.clone()),
            release: None,
        },
    )?;
    if resolution.identity.manifest_sha256 != lock.source.manifest_sha256
        || resolution.identity.manifest_origin != lock.source.manifest_origin
    {
        return Err(AppError::Source(
            "the installed source manifest no longer matches its lock evidence".into(),
        ));
    }
    Ok(resolution.manifest)
}

fn bound_process_output(value: String) -> String {
    const MAX_OUTPUT: usize = 8 * 1024;
    if value.len() <= MAX_OUTPUT {
        value
    } else {
        let mut output = value.chars().take(MAX_OUTPUT).collect::<String>();
        output.push_str("\n[output truncated]");
        output
    }
}

#[tauri::command]
fn run_3d_health_check(project_root: String) -> Result<WorkflowHealthResult, String> {
    if Platform::current() != Platform::Windows {
        return Ok(WorkflowHealthResult {
            status: "unsupported_platform".into(),
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: "the verified 3D workflow is currently supported only on Windows".into(),
        });
    }
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let lock = read_project_lock(&root).map_err(command_error)?;
    let workflow = lock
        .optional_workflows
        .get("workflow.3d")
        .ok_or_else(|| "the 3D workflow is not selected in the installed lock".to_string())?;
    if workflow.state == "unsupported_platform" || workflow.state == "removed" {
        return Err("the installed 3D workflow has no runnable route".into());
    }
    let manifest = resolve_installed_manifest(&lock).map_err(command_error)?;
    let component = manifest
        .components
        .iter()
        .find(|component| component.id == "workflow.3d")
        .ok_or_else(|| "the locked manifest does not declare the 3D workflow".to_string())?;
    if !component
        .platforms
        .iter()
        .any(|platform| platform.supports(Platform::Windows))
    {
        return Err("the resolved 3D component has no verified Windows route".into());
    }
    let command_rules = component
        .validation
        .iter()
        .filter(|rule| rule.kind == "command" && rule.target.is_some())
        .collect::<Vec<_>>();
    if command_rules.len() != 1 {
        return Err(
            "the resolved 3D workflow does not declare one unambiguous bootstrap check".into(),
        );
    }
    let target = command_rules[0]
        .target
        .as_deref()
        .ok_or_else(|| "the 3D bootstrap validation target is missing".to_string())?;
    if !target.to_ascii_lowercase().ends_with(".py") {
        return Err("the resolved 3D bootstrap is not a supported Python route".into());
    }
    let target_path = safe_join(&root, target).map_err(command_error)?;
    if path_has_link_component(&target_path) {
        return Err("the installed 3D bootstrap path contains a link or junction".into());
    }
    let locked_file = lock
        .files
        .iter()
        .find(|file| {
            !file.external
                && file.component_id == "workflow.3d"
                && file.path.eq_ignore_ascii_case(target)
        })
        .ok_or_else(|| {
            "the 3D bootstrap is not hash-tracked in the installation lock".to_string()
        })?;
    let metadata = std::fs::symlink_metadata(&target_path)
        .map_err(|error| format!("the installed 3D bootstrap is unavailable: {error}"))?;
    if !metadata.is_file()
        || locked_file
            .installed_size
            .is_some_and(|expected| expected != metadata.len())
        || sha256_file(&target_path).map_err(command_error)? != locked_file.installed_sha256
    {
        return Err("the installed 3D bootstrap failed its lock integrity check".into());
    }
    let credential_reference = workflow
        .credential_reference
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "store MESHY_API_KEY in the OS credential vault before running the 3D check".to_string()
        })?;
    let reference = CredentialReference {
        name: MESHY_ENVIRONMENT_NAME.into(),
        provider: provider_name(Platform::Windows).into(),
        reference: credential_reference.into(),
    };
    let environment = ScopedSecretEnvironment::from_credential(
        &OsCredentialStore,
        &reference,
        MESHY_ENVIRONMENT_NAME,
    )
    .map_err(command_error)?;
    let python =
        crate::process::find_path_executable(&["python.exe", "python"]).map_err(command_error)?;
    let spec = crate::process::ProcessSpec {
        executable: python.clone(),
        args: vec![target_path.display().to_string()],
        cwd: Some(root.clone()),
        platform: Platform::Windows,
        environment_names: vec![MESHY_ENVIRONMENT_NAME.into()],
        timeout_seconds: 10 * 60,
        max_output_bytes: 2 * 1024 * 1024,
    };
    let result = spec
        .run(&[python], Some(&environment))
        .map_err(command_error)?;
    let health_state = if result.status_code == Some(0) && !result.timed_out {
        "ready"
    } else {
        "incomplete"
    };
    record_three_d_health(&root, &lock, health_state).map_err(command_error)?;
    Ok(WorkflowHealthResult {
        status: health_state.into(),
        exit_code: result.status_code,
        timed_out: result.timed_out,
        stdout: bound_process_output(result.stdout),
        stderr: bound_process_output(result.stderr),
    })
}

fn mcp_failure(error: AppError) -> WorkflowHealthResult {
    let message = redact_secrets(&error.to_string(), &[]);
    WorkflowHealthResult {
        status: "incomplete".into(),
        exit_code: None,
        timed_out: message.to_ascii_lowercase().contains("timed out"),
        stdout: String::new(),
        stderr: bound_process_output(message),
    }
}

fn installed_mcp_target(project_root: &Path, lock: &InstallationLock) -> Result<String, AppError> {
    let manifest = resolve_installed_manifest(lock)?;
    let target = crate::mcp::manifest_target(&manifest)?;
    let config_path = safe_join(project_root, ".codex/config.toml")?;
    let config = std::fs::read_to_string(config_path)
        .map_err(|error| AppError::Process(format!("read installed MCP configuration: {error}")))?;
    let parsed = config.parse::<toml::Value>().map_err(|error| {
        AppError::Serialization(format!("parse installed MCP configuration: {error}"))
    })?;
    let configured = parsed
        .get("mcp_servers")
        .and_then(|value| value.get("hoi4_agent_tools"))
        .and_then(|value| value.get("command"))
        .and_then(toml::Value::as_str);
    if configured != Some(target.as_str()) {
        return Err(AppError::Process(
            "installed MCP configuration does not match the locked manifest command".into(),
        ));
    }
    Ok(target)
}

#[tauri::command]
fn run_mcp_health_check(project_root: String) -> Result<WorkflowHealthResult, String> {
    if Platform::current() != Platform::Windows {
        return Ok(WorkflowHealthResult {
            status: "unsupported_platform".into(),
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: "the verified MCP workflow is currently supported only on Windows".into(),
        });
    }
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let lock = read_project_lock(&root).map_err(command_error)?;
    let component = lock
        .components
        .iter()
        .find(|component| component.id == crate::mcp::COMPONENT_ID);
    let Some(component) = component else {
        return Ok(mcp_failure(AppError::InvalidInput(
            "the installed project does not select the MCP component".into(),
        )));
    };
    if component.state == "unsupported_platform" {
        return Ok(WorkflowHealthResult {
            status: "unsupported_platform".into(),
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: "the installed MCP component has no verified route on this platform".into(),
        });
    }
    if matches!(component.state.as_str(), "not_selected" | "removed") {
        return Ok(WorkflowHealthResult {
            status: "not_selected".into(),
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: "the installed project does not select the MCP component".into(),
        });
    }
    let target = match installed_mcp_target(&root, &lock) {
        Ok(target) => target,
        Err(error) => return Ok(mcp_failure(error)),
    };
    match crate::mcp::initialize_health(&root, &target) {
        Ok(evidence) => Ok(WorkflowHealthResult {
            status: "ready".into(),
            exit_code: Some(0),
            timed_out: false,
            stdout: bound_process_output(redact_secrets(
                &serde_json::to_string(&evidence).map_err(|error| error.to_string())?,
                &[],
            )),
            stderr: String::new(),
        }),
        Err(error) => Ok(mcp_failure(error)),
    }
}

fn refresh_installed_mcp_readiness(
    project_root: &Path,
    detected: &mut ReadinessInput,
) -> Result<(), AppError> {
    if !matches!(detected.mcp_status.as_str(), "pass" | "health_not_run")
        || Platform::current() != Platform::Windows
        || !detected
            .selected_components
            .iter()
            .any(|id| id == crate::mcp::COMPONENT_ID)
    {
        return Ok(());
    }
    let mcp_result = read_project_lock(project_root)
        .and_then(|lock| installed_mcp_target(project_root, &lock))
        .and_then(|target| crate::mcp::initialize_health(project_root, &target));
    if let Err(error) = mcp_result {
        detected.mcp_status = "block".into();
        detected.notes.push(format!(
            "MCP initialize health failed: {}",
            redact_secrets(&error.to_string(), &[])
        ));
    } else {
        detected.mcp_status = "pass".into();
        detected
            .notes
            .push("MCP initialize health and read-only tools/list completed successfully.".into());
    }
    Ok(())
}

fn evaluate_installed_readiness(
    project_root: &Path,
    project_id: &str,
    workflow_3d_state: String,
    lora_interest: bool,
    notes: Vec<String>,
) -> Result<ReadinessReport, AppError> {
    let mut detected = crate::readiness::project_input(project_root, project_id)?;
    refresh_installed_mcp_readiness(project_root, &mut detected)?;
    let live_codex = with_codex_session(|session| session.account_read(false)).ok();
    detected.codex_authenticated = detected.codex_authenticated
        && live_codex.as_ref().is_some_and(|status| {
            status.error.is_none()
                && status.authenticated
                && status.auth_mode == "chatgpt"
                && !status.usage_limited
        });
    if !detected.codex_authenticated {
        detected.codex_analysis_status = "blocked".into();
    }
    detected.workflow_3d_state = read_project_lock(project_root)
        .map(|lock| cached_three_d_state(project_root, &lock))
        .unwrap_or(workflow_3d_state);
    detected.lora_interest = lora_interest;
    detected.notes.extend(notes);
    Ok(crate::readiness::evaluate(&detected))
}

#[tauri::command]
fn evaluate_readiness(input: ReadinessInput) -> Result<ReadinessReport, String> {
    let root = PathBuf::from(&input.project_root);
    if root.is_dir() {
        let root = validate_project_root(&root).map_err(command_error)?;
        evaluate_installed_readiness(
            &root,
            &input.project_id,
            input.workflow_3d_state,
            input.lora_interest,
            input.notes,
        )
        .map_err(command_error)
    } else {
        Ok(crate::readiness::evaluate(&input))
    }
}

#[tauri::command]
fn preview_descriptors(state: Value) -> Result<Vec<GeneratedArtifact>, String> {
    require_codex_chatgpt_session().map_err(command_error)?;
    let _ = codex_analysis_from_state(&state).map_err(command_error)?;
    let root = state
        .pointer("/identity/projectRoot")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let identity = project_identity_from_state(&state, &root).map_err(command_error)?;
    render_generated_artifacts(&identity).map_err(command_error)
}

fn conflict_kind_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Text => "text",
        FileKind::Toml => "toml",
        FileKind::Json => "json",
        FileKind::Binary => "binary",
        FileKind::Symlink => "symlink",
        FileKind::Directory => "directory",
    }
}

fn preview_text(kind: FileKind, bytes: Option<&[u8]>) -> (Option<String>, bool, bool) {
    if !matches!(kind, FileKind::Text | FileKind::Toml | FileKind::Json) {
        return (None, false, false);
    }
    let Some(bytes) = bytes else {
        return (None, false, false);
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return (None, false, false);
    };
    let redacted = redact_secrets(text, &[]);
    let was_redacted = redacted != text;
    if redacted.len() <= CONFLICT_PREVIEW_MAX_BYTES {
        return (Some(redacted), false, was_redacted);
    }
    let mut end = CONFLICT_PREVIEW_MAX_BYTES;
    while end > 0 && !redacted.is_char_boundary(end) {
        end -= 1;
    }
    (
        Some(format!(
            "{}\n\n[Preview truncated at {} bytes; SHA-256 covers the complete file.]",
            &redacted[..end],
            CONFLICT_PREVIEW_MAX_BYTES
        )),
        true,
        was_redacted,
    )
}

#[tauri::command]
fn preview_installation_conflict(plan_id: String, path: String) -> Result<ConflictPreview, String> {
    let id = Uuid::parse_str(&plan_id).map_err(|_| "invalid installation plan ID".to_string())?;
    let plans = prepared_plans()
        .lock()
        .map_err(|_| "prepared plan store is unavailable".to_string())?;
    let prepared = plans
        .get(&id)
        .ok_or_else(|| "installation plan is not available in the core session".to_string())?;
    if !prepared
        .plan
        .conflicts
        .iter()
        .any(|conflict| conflict.path == path)
    {
        return Err("installation conflict is not present in the core plan".into());
    }
    let operation = prepared
        .plan
        .operations
        .iter()
        .find(|operation| operation.destination == path)
        .ok_or_else(|| "installation conflict has no bound operation".to_string())?;
    let context = prepared.merge_contexts.get(&operation.id);
    let kind = context
        .map(|context| context.kind)
        .unwrap_or_else(|| path_file_kind(&operation.destination));
    let base = context.map(|context| context.base.clone());
    let mut local = context.map(|context| context.local.clone());
    let incoming = context.map(|context| context.incoming.clone()).or_else(|| {
        prepared
            .prepared_files
            .iter()
            .find(|file| file.operation_id == operation.id)
            .map(|file| file.bytes.clone())
    });
    if local.is_none() && operation.local_state != LocalState::Absent {
        let local_path = if operation.external {
            validate_external_destination(&operation.destination)
        } else {
            safe_join(&prepared.canonical_root, &operation.destination)
        }
        .map_err(command_error)?;
        local = Some(std::fs::read(&local_path).map_err(|error| {
            format!(
                "cannot read conflict preview {}: {error}",
                operation.destination
            )
        })?);
    }
    let (base_text, base_truncated, base_redacted) = preview_text(kind, base.as_deref());
    let (local_text, local_truncated, local_redacted) = preview_text(kind, local.as_deref());
    let (incoming_text, incoming_truncated, incoming_redacted) =
        preview_text(kind, incoming.as_deref());
    Ok(ConflictPreview {
        path,
        kind: conflict_kind_label(kind).into(),
        base: base_text,
        local: local_text,
        incoming: incoming_text,
        base_sha256: base.as_deref().map(sha256_bytes),
        local_sha256: local.as_deref().map(sha256_bytes),
        incoming_sha256: incoming.as_deref().map(sha256_bytes),
        truncated: base_truncated || local_truncated || incoming_truncated,
        redacted: base_redacted || local_redacted || incoming_redacted,
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn source_request_from_state(state: &Value) -> Result<SourceRequest, AppError> {
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

#[tauri::command]
fn preview_source_manifest(
    source_mode: String,
    pinned_ref: String,
) -> Result<SourceManifestPreview, String> {
    let state = serde_json::json!({
        "sourceMode": source_mode,
        "pinnedRef": pinned_ref,
    });
    let request = source_request_from_state(&state).map_err(command_error)?;
    let client = HttpSourceClient::new().map_err(command_error)?;
    let resolution = resolve_source(&client, &request).map_err(command_error)?;
    Ok(SourceManifestPreview {
        schema_version: resolution.manifest.schema_version.clone(),
        manifest_id: resolution.manifest.manifest_id.clone(),
        source: resolution.identity,
        repository: resolution.manifest.repository.clone(),
        components: resolution.manifest.components,
    })
}

fn selected_ids(state: &Value) -> Vec<String> {
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
    selected
}

fn manifest_external_actions(
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
                    let platform =
                        component
                            .platforms
                            .iter()
                            .find_map(|declared| match declared {
                                ManifestPlatform::Windows => Some(Platform::Windows),
                                ManifestPlatform::Macos => Some(Platform::Macos),
                                ManifestPlatform::All => None,
                            })?;
                    let target = rule.target.as_deref()?;
                    Some(ExternalAction {
                        id: format!("external.{}.{}", component.id, rule.id),
                        component_id: component.id.clone(),
                        platform,
                        command_source: "repository_script".into(),
                        executable: Some(if target.to_ascii_lowercase().ends_with(".py") {
                            "manifest-declared Python tool".into()
                        } else {
                            "manifest-declared executable".into()
                        }),
                        arguments: vec![target.into()],
                        working_directory: Some("<project_root>".into()),
                        environment_names: component
                            .environment
                            .iter()
                            .map(|environment| environment.name.clone())
                            .collect(),
                        network_access: "not_declared".into(),
                        // `expected_files` describe the install payload, not
                        // the side effects of the command itself. Do not
                        // present them as an external process write set.
                        expected_writes: Vec::new(),
                        privilege: "not_declared".into(),
                        rollback_boundary: "not_declared_by_source".into(),
                        display_command: Some(format!(
                            "Repository-declared validation target: {target}"
                        )),
                        risk: "high".into(),
                        requires_approval: true,
                        contains_secret: false,
                    })
                })
        })
        .collect()
}

fn project_root_from_state(state: &Value) -> Result<PathBuf, AppError> {
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
    let root = PathBuf::from(root);
    validate_project_root(&root)
}

fn component_kind(component: &ComponentDefinition, path: &str) -> FileKind {
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

fn path_file_kind(path: &str) -> FileKind {
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

fn adapt_codex_config_for_selection(bytes: &[u8], mcp_selected: bool) -> Result<Vec<u8>, AppError> {
    if mcp_selected {
        return Ok(bytes.to_vec());
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("Codex configuration is not UTF-8: {error}")))?;
    let mut config = text.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("Codex configuration is not valid TOML: {error}"))
    })?;
    if let Some(table) = config.as_table_mut() {
        let empty = table
            .get_mut("mcp_servers")
            .and_then(toml::Value::as_table_mut)
            .map(|mcp| {
                mcp.remove("hoi4_agent_tools");
                mcp.is_empty()
            })
            .unwrap_or(false);
        if empty {
            table.remove("mcp_servers");
        }
    }
    let rendered = config.to_string();
    rendered.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("adapted Codex configuration is invalid: {error}"))
    })?;
    Ok(rendered.into_bytes())
}

fn merge_bytes(context: &MergeContext, path: &str) -> Result<Vec<u8>, AppError> {
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

fn choice_from_state(state: &Value) -> Option<String> {
    string_field(state, "conflictChoice")
}

fn project_identity_from_state(state: &Value, root: &Path) -> Result<ProjectIdentity, AppError> {
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
    if state.get("mode").and_then(Value::as_str) == Some("new") && launcher.is_empty() {
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
    Ok(identity)
}

fn renamed_destination(root: &Path, original: &str, external: bool) -> Result<String, AppError> {
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

fn generated_state(state: &Value) -> BTreeMap<String, String> {
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
    let lora_interest = state
        .get("loraInterest")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    result.insert(
        "workflow.lora_comfyui_interest".into(),
        if lora_interest {
            "planned_unavailable".into()
        } else {
            "not_selected".into()
        },
    );
    result
}

fn selected_folder_profile(state: &Value) -> Result<Vec<String>, AppError> {
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
    let mut values = values;
    let mut normalized = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        normalized.push(
            crate::security::normalize_relative_path(&value).map_err(|error| {
                AppError::InvalidInput(format!("folder profile contains an unsafe path: {error}"))
            })?,
        );
    }
    normalized.sort();
    normalized.dedup();
    if normalized.len() > 32 {
        return Err(AppError::InvalidInput(
            "folder profile contains too many paths".into(),
        ));
    }
    Ok(normalized)
}

fn project_readme(identity: &ProjectIdentity, description: &str) -> Result<String, AppError> {
    if description.len() > 32 * 1024 || description.contains('\0') {
        return Err(AppError::InvalidInput(
            "project description exceeds the bounded README input limit".into(),
        ));
    }
    Ok(format!(
        "# {}\n\n{}\n\n## Local development\n\nThis project was prepared by HOI4 Mod Setup for Codex development.\n\n- Project ID: `{}`\n- Supported game version: `{}`\n- Workshop identity: none assigned\n",
        identity.display_name,
        description.trim(),
        identity.project_id,
        identity.supported_game_version,
    ))
}

fn codex_analysis_from_state(state: &Value) -> Result<CodexAnalysisRecord, AppError> {
    let value = state
        .get("codexAnalysisRecord")
        .or_else(|| state.pointer("/codexAnalysis/record"))
        .ok_or_else(|| {
            AppError::Credential(
                "ChatGPT-authenticated Codex analysis must be confirmed before planning".into(),
            )
        })?;
    let record: CodexAnalysisRecord = serde_json::from_value(value.clone())?;
    crate::codex::validate_confirmed_record(&record)?;
    if !codex_record_confirmed_in_session(&record)? {
        return Err(AppError::Credential(
            "Codex confirmation is no longer present in the core session; rerun semantic analysis"
                .into(),
        ));
    }
    Ok(record)
}

fn same_persisted_codex_record(left: &CodexAnalysisRecord, right: &CodexAnalysisRecord) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.project_root = None;
    left.scan_id = None;
    right.project_root = None;
    right.scan_id = None;
    left == right
}

fn codex_record_confirmed_in_session(record: &CodexAnalysisRecord) -> Result<bool, AppError> {
    let analyses = codex_analyses()
        .lock()
        .map_err(|_| AppError::Process("Codex analysis store is unavailable".into()))?;
    Ok(analyses
        .get(&record.analysis_id)
        .and_then(|pending| pending.confirmed.as_ref())
        .is_some_and(|confirmed| same_persisted_codex_record(confirmed, record)))
}

fn git_setup_from_state(
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

fn build_plan(state: &Value) -> Result<(InstallationPlan, Vec<PreparedFile>), AppError> {
    require_codex_chatgpt_session()?;
    let root = project_root_from_state(state)?;
    let identity = project_identity_from_state(state, &root)?;
    let codex_analysis = codex_analysis_from_state(state)?;
    let git_setup = git_setup_from_state(state, &root)?;
    let request = source_request_from_state(state)?;
    let client = HttpSourceClient::new()?;
    let resolution = resolve_source(&client, &request)?;
    let requested = selected_ids(state);
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
    let tree = client.fetch_tree(&resolution.identity.resolved_revision)?;
    let selections = select_component_files(&resolution.manifest, &download_selected, &tree)?;
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
    for (index, selection) in selections.iter().enumerate() {
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
        let verified = verify_download(
            selection,
            &source_bytes,
            &resolution.identity.resolved_revision,
        )?;
        let bytes = if selection.component_id == "codex.config" {
            adapt_codex_config_for_selection(&source_bytes, mcp_selected)?
        } else {
            source_bytes
        };
        let incoming_sha256 = sha256_bytes(&bytes);
        let source_destination = safe_join(&root, &selection.destination)?;
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
        let mut destination = selection.destination.clone();
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
            let kind = component_kind(component, &selection.destination);
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
                    destination = renamed_destination(&root, &selection.destination, false)?;
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
                id: format!("conflict-{}", index),
                path: selection.destination.clone(),
                options,
                selected: selected_choice.clone(),
                apply_to_identical: false,
            });
        }
        operations.push(PlanOperation {
            id: format!("op-{index:05}"),
            component_id: selection.component_id.clone(),
            ownership: Some(selection.ownership),
            location_scope: Some("project".into()),
            action,
            source_path: Some(selection.source_path.clone()),
            destination: destination.clone(),
            source_sha256: Some(verified.sha256.clone()),
            source_size: selection.expected_size,
            platform: Some(selection.platform),
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
        });
        // Keep a verified incoming copy even for an unresolved conflict. It
        // remains in the core-owned plan session and is discarded when the
        // user chooses keep/skip; this makes replace/rename decisions possible
        // without re-fetching or trusting renderer-supplied bytes.
        let prepared_sha256 = sha256_bytes(&bytes);
        prepared.push(PreparedFile {
            operation_id: format!("op-{index:05}"),
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
    if mode == "new" {
        let description = string_field(state, "description").unwrap_or_default();
        let readme = project_readme(&identity, &description)?;
        generated.push(GeneratedArtifact {
            component_id: "project.readme".into(),
            destination: "README.md".into(),
            expected_sha256: sha256_bytes(readme.as_bytes()),
            content: readme,
            external: false,
            bytes: None,
        });
        for folder in selected_folder_profile(state)? {
            let destination = format!("{folder}/.gitkeep");
            let content = format!("# Managed folder marker for the confirmed {folder} profile.\n");
            generated.push(GeneratedArtifact {
                component_id: "project.folder_profile".into(),
                destination,
                expected_sha256: sha256_bytes(content.as_bytes()),
                content,
                external: false,
                bytes: None,
            });
        }
    }
    let lora_interest = state
        .get("loraInterest")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let credential_references = meshy_credential_reference()
        .lock()
        .map_err(|_| {
            AppError::Credential("Meshy credential reference store is unavailable".into())
        })?
        .clone()
        .into_iter()
        .collect::<Vec<_>>();
    if mode == "new" || lora_interest {
        let state_content = serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "1.0.0",
            "project_id": identity.project_id.clone(),
            "project_root": identity.project_root.display().to_string(),
            "platform": Platform::current().manifest_name(),
            "wizard": {
                "current_step": "ready",
                "completed_steps": ["welcome", "description", "identity", "components", "workflows"]
            },
            "preferences": {
                "lora_comfyui_interest": lora_interest,
                "telemetry": false
            },
            "codex": {
                "integration": "codex_app_server",
                "auth_mode": "chatgpt",
                "auth_status": "signed_in",
                "analysis_required": true,
                "analysis_status": "confirmed",
                "account_values_persisted": false
            },
            "credential_references": credential_references.clone()
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
    if git_setup.is_some() {
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
    }
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
        let mut selected_choice = None;
        if local_state == LocalState::Modified {
            let options = allowed_choices(
                path_file_kind(&artifact.destination),
                crate::merge::MergeClassification::UserOwnedConflict,
            );
            if let Some(choice) = choice_from_state(state)
                .filter(|value| options.iter().any(|option| option == value))
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
            result_sha256: (action != OperationAction::Skip)
                .then_some(artifact.expected_sha256.clone()),
            base_sha256: None,
            local_sha256: operation_local_sha,
            local_state,
            resolution: selected_choice,
            external: artifact.external,
            rollback: if matches!(action, OperationAction::Generate | OperationAction::Rename) {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
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
    for item in &support {
        if item.state == "unsupported_platform" {
            optional_workflows.insert(item.component_id.clone(), "unsupported_platform".into());
        }
    }
    let plan = InstallationPlan {
        schema_version: "1.0.0".into(),
        plan_id: Uuid::new_v4(),
        project_id: state
            .pointer("/identity/projectId")
            .and_then(Value::as_str)
            .unwrap_or("project")
            .into(),
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        maintenance_mode: None,
        source: resolution.identity,
        codex_analysis: Some(codex_analysis),
        selected_components: selected,
        wiki_required_pages: resolution.manifest.wiki.required_pages.clone(),
        generated_artifacts: generated,
        git_setup,
        credential_references,
        optional_workflows,
        operations,
        conflicts,
        external_actions,
        transaction: TransactionPlanInfo {
            stages: TRANSACTION_STAGES
                .iter()
                .map(|stage| (*stage).into())
                .collect(),
            backup_root: application_data_root()
                .join("backups")
                .display()
                .to_string(),
            staging_root: application_data_root()
                .join("staging")
                .display()
                .to_string(),
            atomic_apply_expected: true,
        },
        approvals: PlanApprovals {
            dry_run_reviewed: false,
            external_actions_reviewed: false,
            git_remote_approved: false,
            push_approved: false,
        },
    };
    Ok((plan, prepared))
}

#[tauri::command]
fn build_installation_plan(state: Value) -> Result<InstallationPlan, String> {
    build_plan(&state)
        .and_then(|(plan, prepared_files)| {
            store_prepared_plan(
                plan,
                prepared_files,
                HashMap::new(),
                project_root_from_state(&state)?,
            )
        })
        .map_err(command_error)
}

fn read_project_lock(root: &Path) -> Result<InstallationLock, AppError> {
    let bytes = std::fs::read(root.join(".hoi4-mod-setup/install.lock.json")).map_err(|error| {
        AppError::Transaction(format!("installation lock is unavailable: {error}"))
    })?;
    let value: Value = serde_json::from_slice(&bytes)?;
    crate::migrations::migrate_lock(value)
}

fn lock_mcp_selected(lock: &InstallationLock) -> bool {
    lock.components.iter().any(|component| {
        component.id == "mcp.hoi4_agent_tools"
            && !matches!(
                component.state.as_str(),
                "unsupported_platform" | "not_selected" | "removed"
            )
            && Platform::current() == Platform::Windows
    })
}

fn require_maintenance_reanalysis(
    mode: &str,
    analysis_override: Option<&CodexAnalysisRecord>,
    project_root: &Path,
) -> Result<(), AppError> {
    if mode != "update" {
        return Ok(());
    }
    let record = analysis_override.ok_or_else(|| {
        AppError::Credential(
            "update planning requires a fresh, confirmed Codex reanalysis of the installed project"
                .into(),
        )
    })?;
    crate::codex::validate_confirmed_record(record)?;
    if record.analysis_purpose.as_deref() != Some("maintenance_reanalysis") {
        return Err(AppError::Credential(
            "update planning requires a maintenance-purpose Codex reanalysis".into(),
        ));
    }
    let (record_root, record_scan_id) = {
        let analyses = codex_analyses()
            .lock()
            .map_err(|_| AppError::Process("Codex analysis store is unavailable".into()))?;
        let pending = analyses.get(&record.analysis_id).ok_or_else(|| {
            AppError::Credential(
                "update reanalysis is not confirmed in the current core session".into(),
            )
        })?;
        if pending
            .confirmed
            .as_ref()
            .is_none_or(|confirmed| !same_persisted_codex_record(confirmed, record))
        {
            return Err(AppError::Credential(
                "update reanalysis is not confirmed in the current core session".into(),
            ));
        }
        (pending.project_root.clone(), pending.scan_id)
    };
    let record_root = record_root.ok_or_else(|| {
        AppError::Credential("update reanalysis has no core project-root binding".into())
    })?;
    if !same_project_root(&record_root, project_root) {
        return Err(AppError::Credential(
            "update reanalysis belongs to a different project root".into(),
        ));
    }
    let approved = codex_approved_evidence()
        .lock()
        .map_err(|_| AppError::Process("approved scan evidence store is unavailable".into()))?;
    if approved
        .project_root
        .as_ref()
        .is_none_or(|root| !same_project_root(root, project_root))
        || approved.scan_id != record_scan_id
    {
        return Err(AppError::Credential(
            "update reanalysis is not bound to the latest read-only scan".into(),
        ));
    }
    if record.evidence_sha256.is_none() {
        return Err(AppError::Credential(
            "update reanalysis has no evidence-manifest binding".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
fn build_maintenance_plan(
    mode: String,
    project_root: String,
    analysis_override: Option<CodexAnalysisRecord>,
) -> Result<InstallationPlan, String> {
    if mode != "remove" {
        require_codex_chatgpt_session().map_err(command_error)?;
    }
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let lock = read_project_lock(&root).map_err(command_error)?;
    require_maintenance_reanalysis(&mode, analysis_override.as_ref(), &root)
        .map_err(command_error)?;
    let codex_analysis = if mode == "remove" {
        None
    } else {
        let record = analysis_override
            .clone()
            .or_else(|| lock.codex_analysis.clone())
            .ok_or_else(|| {
                "the installed project has no confirmed Codex analysis; run Import review before maintenance"
                    .to_string()
            })?;
        crate::codex::validate_confirmed_record(&record).map_err(command_error)?;
        if analysis_override.is_some()
            && !codex_record_confirmed_in_session(&record).map_err(command_error)?
        {
            return Err(
                "the Codex reanalysis is not confirmed in the current core session; review it again"
                    .into(),
            );
        }
        Some(record)
    };
    let client = HttpSourceClient::new().map_err(command_error)?;
    let mut maintenance_components = lock
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect::<Vec<_>>();
    let mut wiki_required_pages = lock.wiki_required_pages.clone();
    let mut maintenance_mcp_manifest: Option<RemoteManifest> = None;
    let (mut operations, plan_source, source_revision) = if mode == "update" {
        let resolution = resolve_source(
            &client,
            &SourceRequest {
                mode: lock.source.mode,
                requested_ref: lock.source.requested_ref.clone(),
                release: lock.source.release.clone(),
            },
        )
        .map_err(command_error)?;
        if lock.source.mode == SourceMode::PinnedRelease
            && resolution.identity.resolved_revision != lock.source.revision
        {
            return Err(
                "the pinned release resolved to a different commit; choose a new release explicitly"
                    .into(),
            );
        }
        let manifest_component_ids = resolution
            .manifest
            .components
            .iter()
            .map(|component| component.id.as_str())
            .collect::<Vec<_>>();
        let requested = lock
            .components
            .iter()
            .filter(|component| {
                component.state != "removed"
                    && manifest_component_ids.contains(&component.id.as_str())
            })
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        let expanded =
            expand_components(&resolution.manifest, &requested).map_err(command_error)?;
        maintenance_components = expanded.clone();
        let support = crate::source::resolve_platform_support(
            &resolution.manifest,
            &expanded,
            Platform::current(),
        )
        .map_err(command_error)?;
        if support.iter().any(|item| item.state == "blocked") {
            return Err("the current source has an unsupported required component".into());
        }
        let supported = expanded
            .into_iter()
            .filter(|id| {
                support
                    .iter()
                    .find(|item| item.component_id == id.as_str())
                    .is_some_and(|item| item.state == "supported")
            })
            .collect::<Vec<_>>();
        let tree = client
            .fetch_tree(&resolution.identity.resolved_revision)
            .map_err(command_error)?;
        let selections = select_component_files(&resolution.manifest, &supported, &tree)
            .map_err(command_error)?;
        let mcp_selected = lock_mcp_selected(&lock);
        if mcp_selected {
            maintenance_mcp_manifest = Some(resolution.manifest.clone());
        }
        let incoming = selections
            .iter()
            .map(|selection| {
                let expected = selection.expected_sha256.as_deref().ok_or_else(|| {
                    AppError::Source(format!(
                        "selected source file lacks SHA-256 evidence: {}",
                        selection.source_path
                    ))
                })?;
                let source_bytes = client.fetch_verified_file(
                    &resolution.identity.resolved_revision,
                    &selection.source_path,
                    expected,
                    selection.expected_size,
                )?;
                let source_verified = verify_download(
                    selection,
                    &source_bytes,
                    &resolution.identity.resolved_revision,
                )?;
                let desired_bytes = if selection.component_id == "codex.config" {
                    adapt_codex_config_for_selection(&source_bytes, mcp_selected)?
                } else {
                    source_bytes
                };
                Ok(LockedFile {
                    path: selection.destination.clone(),
                    location_scope: Some("project".into()),
                    component_id: selection.component_id.clone(),
                    source_path: selection.source_path.clone(),
                    source_revision: resolution.identity.resolved_revision.clone(),
                    source_sha256: source_verified.sha256,
                    source_size: selection.expected_size,
                    base_sha256: None,
                    installed_sha256: sha256_bytes(&desired_bytes),
                    installed_size: Some(desired_bytes.len() as u64),
                    ownership: selection.ownership,
                    external: false,
                    generated_content: None,
                    generated_bytes: None,
                    executable: selection.executable,
                    platform: Some(selection.platform),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map_err(command_error)?;
        let mut incoming = incoming;
        for generated in lock
            .files
            .iter()
            .filter(|file| file.ownership == Ownership::Generated)
        {
            if !incoming
                .iter()
                .any(|file| file.path == generated.path && file.external == generated.external)
            {
                incoming.push(generated.clone());
            }
        }
        let operations = crate::transaction::update_operations(&lock, &incoming, &root)
            .map_err(command_error)?;
        wiki_required_pages = resolution.manifest.wiki.required_pages.clone();
        (
            operations,
            resolution.identity.clone(),
            resolution.identity.resolved_revision,
        )
    } else {
        if mode != "remove" && lock_mcp_selected(&lock) {
            maintenance_mcp_manifest =
                Some(resolve_installed_manifest(&lock).map_err(command_error)?);
        }
        let operations = match mode.as_str() {
            "repair" => crate::transaction::repair_operations(&lock, &root),
            "reinstall" => crate::transaction::reinstall_operations(&lock, &root),
            "remove" => crate::transaction::managed_removal_operations(&lock, &root),
            other => return Err(format!("unsupported maintenance mode: {other}")),
        }
        .map_err(command_error)?;
        let source = SourceIdentity {
            repository: lock.source.repository.clone(),
            mode: lock.source.mode,
            resolved_revision: lock.source.revision.clone(),
            requested_ref: lock.source.requested_ref.clone(),
            release: lock.source.release.clone(),
            manifest_sha256: lock.source.manifest_sha256.clone(),
            manifest_origin: lock.source.manifest_origin.clone(),
        };
        (operations, source, lock.source.revision.clone())
    };
    let mut prepared = Vec::new();
    let mut merge_contexts = HashMap::new();
    for operation in &mut operations {
        if operation.action == OperationAction::DeleteManaged
            || operation.source_sha256.is_none()
            || (mode == "remove" && operation.action == OperationAction::Skip)
        {
            continue;
        }
        let expected = operation
            .source_sha256
            .as_deref()
            .ok_or_else(|| "maintenance operation has no expected source hash".to_string())?;
        let source_bytes = if operation
            .source_path
            .as_deref()
            .is_some_and(|path| path.starts_with("generated:"))
        {
            lock.files
                .iter()
                .find(|file| {
                    file.path == operation.destination && file.external == operation.external
                })
                .and_then(|file| file.generated_content.as_deref())
                .map(|content| content.as_bytes().to_vec())
                .or_else(|| {
                    lock.files
                        .iter()
                        .find(|file| {
                            file.path == operation.destination
                                && file.external == operation.external
                        })
                        .and_then(|file| file.generated_bytes.clone())
                })
                .ok_or_else(|| {
                    format!(
                        "generated maintenance artifact has no recorded content: {}",
                        operation.destination
                    )
                })?
        } else {
            client
                .fetch_verified_file(
                    &source_revision,
                    operation.source_path.as_deref().unwrap_or(""),
                    expected,
                    None,
                )
                .map_err(command_error)?
        };
        let mcp_selected = lock_mcp_selected(&lock);
        let bytes = if operation.component_id == "codex.config"
            && !operation
                .source_path
                .as_deref()
                .is_some_and(|path| path.starts_with("generated:"))
        {
            adapt_codex_config_for_selection(&source_bytes, mcp_selected).map_err(command_error)?
        } else {
            source_bytes
        };
        let actual = sha256_bytes(&bytes);
        if operation
            .source_path
            .as_deref()
            .is_some_and(|path| path.starts_with("generated:"))
        {
            if actual != expected {
                return Err(format!(
                    "maintenance generated checksum mismatch: {}",
                    operation.destination
                ));
            }
        } else if actual != expected
            && operation.component_id != "codex.config"
            && operation.result_sha256.is_none()
        {
            // A transformed configuration has source evidence for the
            // downloaded blob and a separate result hash for the selected
            // project output. Other files must still match source evidence.
            return Err(format!(
                "maintenance checksum mismatch: {}",
                operation.destination
            ));
        }
        if operation.action != OperationAction::Skip {
            operation.result_sha256 = Some(actual.clone());
        }
        prepared.push(PreparedFile {
            operation_id: operation.id.clone(),
            destination: operation.destination.clone(),
            bytes: bytes.clone(),
            expected_sha256: actual,
        });
        if operation.local_state == LocalState::Modified
            && !operation
                .source_path
                .as_deref()
                .is_some_and(|path| path.starts_with("generated:"))
        {
            if let Some(current_file) = lock.files.iter().find(|file| {
                file.path == operation.destination && file.external == operation.external
            }) {
                let kind = path_file_kind(&operation.destination);
                if matches!(kind, FileKind::Text | FileKind::Toml | FileKind::Json) {
                    let base_source = client
                        .fetch_verified_file(
                            &current_file.source_revision,
                            &current_file.source_path,
                            &current_file.source_sha256,
                            None,
                        )
                        .map_err(command_error)?;
                    let base = if current_file.component_id == "codex.config" {
                        adapt_codex_config_for_selection(&base_source, mcp_selected)
                            .map_err(command_error)?
                    } else {
                        base_source
                    };
                    let local_path = if current_file.external {
                        validate_external_destination(&current_file.path)
                    } else {
                        safe_join(&root, &current_file.path)
                    }
                    .map_err(command_error)?;
                    let local = std::fs::read(local_path).map_err(|error| {
                        format!(
                            "cannot read modified file for merge preview {}: {error}",
                            operation.destination
                        )
                    })?;
                    merge_contexts.insert(
                        operation.id.clone(),
                        MergeContext {
                            kind,
                            base,
                            local,
                            incoming: bytes,
                        },
                    );
                }
            }
        }
    }
    let conflicts = operations
        .iter()
        .filter(|operation| {
            operation.local_state == LocalState::Modified
                || operation.resolution.as_deref() == Some("reverse_merge_required")
        })
        .map(|operation| {
            let removal_review = matches!(
                operation.resolution.as_deref(),
                Some("reverse_merge_required" | "obsolete_review")
            );
            let mut options = if removal_review {
                vec!["keep".into(), "skip".into()]
            } else {
                vec![
                    "keep".into(),
                    "replace".into(),
                    "rename".into(),
                    "skip".into(),
                ]
            };
            if !removal_review && merge_contexts.contains_key(&operation.id) {
                options.insert(2, "merge".into());
            }
            PlanConflict {
                id: format!("maintenance-{}", operation.id),
                path: operation.destination.clone(),
                options,
                selected: None,
                apply_to_identical: false,
            }
        })
        .collect::<Vec<_>>();
    let optional_workflows = lock
        .optional_workflows
        .iter()
        .map(|(id, workflow)| (id.clone(), workflow.state.clone()))
        .collect();
    let external_actions = if mode != "remove" && lock_mcp_selected(&lock) {
        let manifest = maintenance_mcp_manifest.as_ref().ok_or_else(|| {
            "the maintenance plan could not retain the locked MCP manifest evidence".to_string()
        })?;
        crate::mcp::manifest_target(manifest).map_err(command_error)?;
        let actions = manifest_external_actions(manifest, &[crate::mcp::COMPONENT_ID.to_string()]);
        if actions.len() != 1 {
            return Err(
                "the locked MCP manifest does not declare one reviewed health action".into(),
            );
        }
        actions
    } else {
        Vec::new()
    };
    let external_actions_reviewed = external_actions.is_empty();
    let plan = InstallationPlan {
        schema_version: "1.0.0".into(),
        plan_id: Uuid::new_v4(),
        project_id: lock.project_id.clone(),
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        maintenance_mode: Some(mode.clone()),
        source: plan_source,
        codex_analysis,
        selected_components: maintenance_components,
        wiki_required_pages,
        generated_artifacts: vec![],
        git_setup: None,
        credential_references: vec![],
        optional_workflows,
        operations,
        conflicts,
        external_actions,
        transaction: TransactionPlanInfo {
            stages: TRANSACTION_STAGES
                .iter()
                .map(|stage| (*stage).into())
                .collect(),
            backup_root: application_data_root()
                .join("backups")
                .display()
                .to_string(),
            staging_root: application_data_root()
                .join("staging")
                .display()
                .to_string(),
            atomic_apply_expected: true,
        },
        approvals: PlanApprovals {
            dry_run_reviewed: false,
            external_actions_reviewed,
            git_remote_approved: false,
            push_approved: false,
        },
    };
    store_prepared_plan(plan, prepared, merge_contexts, root).map_err(command_error)
}

#[tauri::command]
fn approve_installation(plan_id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&plan_id).map_err(|_| "invalid installation plan ID".to_string())?;
    let mut plans = prepared_plans()
        .lock()
        .map_err(|_| "prepared plan store is unavailable".to_string())?;
    let prepared = plans
        .get_mut(&id)
        .ok_or_else(|| "installation plan is not available in the core session".to_string())?;
    prepared.approved = true;
    prepared.plan.approvals.dry_run_reviewed = true;
    prepared.plan.approvals.external_actions_reviewed = true;
    // The final dry-run approval is the explicit approval for a configured
    // local remote. It does not approve repository creation or pushing: those
    // actions are not part of the transaction and remain separately gated by
    // the Git plan's push_approved flag.
    prepared.plan.approvals.git_remote_approved = true;
    Ok(())
}

#[tauri::command]
fn resolve_installation_conflict(
    plan_id: String,
    path: String,
    choice: String,
) -> Result<InstallationPlan, String> {
    let id = Uuid::parse_str(&plan_id).map_err(|_| "invalid installation plan ID".to_string())?;
    let mut plans = prepared_plans()
        .lock()
        .map_err(|_| "prepared plan store is unavailable".to_string())?;
    let prepared = plans
        .get_mut(&id)
        .ok_or_else(|| "installation plan is not available in the core session".to_string())?;
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
                .and_then(|context| merge_bytes(context, &path).map_err(command_error))?;
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
                .map_err(command_error)?;
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
                file.destination = destination;
            } else {
                return Err("rename operation has no prepared incoming bytes".into());
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
    Ok(prepared.plan.clone())
}

#[tauri::command]
fn apply_installation(
    plan: InstallationPlan,
    project_root: String,
) -> Result<TransactionJournal, String> {
    let id = plan.plan_id;
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let (approved_plan, prepared) = {
        let plans = prepared_plans()
            .lock()
            .map_err(|_| "prepared plan store is unavailable".to_string())?;
        let prepared = plans
            .get(&id)
            .ok_or_else(|| "installation plan is not available in the core session".to_string())?;
        if prepared.canonical_root != root {
            return Err("installation root changed after review".into());
        }
        let submitted_fingerprint = plan_fingerprint(&plan).map_err(command_error)?;
        let approved_fingerprint = plan_fingerprint(&prepared.plan).map_err(command_error)?;
        if submitted_fingerprint != approved_fingerprint {
            return Err("installation plan changed after core review".into());
        }
        if !prepared.approved || !prepared.plan.approvals.dry_run_reviewed {
            return Err("explicit dry-run approval is required before installation".into());
        }
        (prepared.plan.clone(), prepared.prepared_files.clone())
    };
    let (journal, _) = run_transaction(
        &root,
        &approved_plan,
        &prepared,
        &TransactionOptions::default(),
    )
    .map_err(command_error)?;
    prepared_plans()
        .lock()
        .map_err(|_| "prepared plan store is unavailable".to_string())?
        .remove(&id);
    Ok(journal)
}

#[tauri::command]
fn rollback_installation(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    let journal_path = crate::paths::transaction_root(&application_data_root(), id)
        .transaction
        .join("journal.json");
    let mut journal = crate::transaction::read_journal(&journal_path).map_err(command_error)?;
    crate::transaction::rollback_transaction(&root, &mut journal, &journal_path)
        .map_err(command_error)?;
    Ok(journal)
}

#[tauri::command]
fn read_transaction_journal(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    let journal_path = crate::paths::transaction_root(&application_data_root(), id)
        .transaction
        .join("journal.json");
    let journal = crate::transaction::read_journal(&journal_path).map_err(command_error)?;
    if journal.project_root.is_empty() {
        return Err("transaction journal has no project-root binding".into());
    }
    if !journal_bound_to_root(&journal, &root).map_err(command_error)? {
        return Err("transaction journal is bound to a different project root".into());
    }
    Ok(journal)
}

#[tauri::command]
fn find_interrupted_transaction(
    project_root: String,
) -> Result<Option<TransactionJournal>, String> {
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let transactions_root = application_data_root().join("transactions");
    if crate::security::path_has_link_component(&transactions_root) {
        return Err("transaction storage contains a symlink or junction".into());
    }
    let mut candidates = Vec::new();
    let entries = match std::fs::read_dir(&transactions_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if crate::security::is_link_metadata(&metadata) || !metadata.is_dir() {
            continue;
        }
        let journal = match crate::transaction::read_journal(&path.join("journal.json")) {
            Ok(journal) => journal,
            Err(_) => continue,
        };
        if matches!(
            journal.state.as_str(),
            "interrupted" | "rolling_back" | "finalizing"
        ) && (journal.recovery.resume_allowed
            || journal.recovery.rollback_allowed
            || journal.recovery.discard_staging_allowed)
            && journal_bound_to_root(&journal, &root).unwrap_or(false)
        {
            candidates.push(journal);
        }
    }
    candidates.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(candidates.into_iter().next())
}

#[tauri::command]
fn resume_installation(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    let (journal, _) =
        resume_transaction(&root, &application_data_root(), id).map_err(command_error)?;
    Ok(journal)
}

#[tauri::command]
fn discard_installation_staging(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    discard_staging(&root, &application_data_root(), id).map_err(command_error)
}

#[tauri::command]
fn open_in_codex(project_root: String) -> Result<OpenInCodexResult, String> {
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let live_codex =
        with_codex_session(|session| session.account_read(false)).map_err(command_error)?;
    if live_codex.error.is_some()
        || !live_codex.authenticated
        || live_codex.auth_mode != "chatgpt"
        || live_codex.usage_limited
    {
        return Err("Open in Codex requires an active ChatGPT-authenticated Codex session".into());
    }
    let lock = read_project_lock(&root).map_err(command_error)?;
    let workflow_3d_state = lock
        .optional_workflows
        .get("workflow.3d")
        .map(|workflow| workflow.state.clone())
        .unwrap_or_else(|| "not_selected".into());
    let lora_interest = lock
        .optional_workflows
        .get("workflow.lora_comfyui_interest")
        .is_some_and(|workflow| {
            matches!(
                workflow.state.as_str(),
                "planned_unavailable" | "interest_recorded"
            )
        });
    let readiness = evaluate_installed_readiness(
        &root,
        &lock.project_id,
        workflow_3d_state,
        lora_interest,
        Vec::new(),
    )
    .map_err(command_error)?;
    if !crate::readiness::core_ready(&readiness) {
        return Err(format!(
            "Codex remains disabled until core readiness passes: {}",
            readiness.open_in_codex.blocking_check_ids.join(", ")
        ));
    }
    let Some(executable) = find_codex_executable() else {
        return Ok(manual_open_in_codex_result(&root));
    };
    let spec = crate::process::ProcessSpec {
        executable: executable.clone(),
        args: vec!["--cd".into(), root.display().to_string()],
        cwd: Some(root),
        platform: Platform::current(),
        environment_names: vec![],
        timeout_seconds: 1,
        max_output_bytes: 1,
    };
    spec.spawn_detached(&[executable]).map_err(command_error)?;
    Ok(OpenInCodexResult {
        opened: true,
        message: "Codex was launched for the verified project.".into(),
    })
}

fn manual_open_in_codex_result(root: &Path) -> OpenInCodexResult {
    OpenInCodexResult {
        opened: false,
        message: format!(
            "No verified Codex opener was found. Open this folder manually: {}",
            root.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn project_identity_reads_nested_descriptor_fields() {
        let root = tempdir().unwrap();
        let project_root = root.path().join("project");
        let launcher = root.path().join("cold_war_curtain.mod");
        let state = serde_json::json!({
            "mode": "new",
            "identity": {
                "displayName": "Cold War Curtain",
                "projectId": "cold_war_curtain",
                "author": "",
                "version": "0.1.0",
                "supportedGameVersion": "1.17.*",
                "defaultBranch": "main",
                "scriptPrefix": "cwc",
                "primaryNamespace": "cold_war_curtain",
                "descriptorTags": ["Gameplay", "Total Conversion"],
                "launcherDescriptorPath": launcher.display().to_string()
            }
        });

        let identity = project_identity_from_state(&state, &project_root).unwrap();

        assert_eq!(identity.script_prefix.as_deref(), Some("cwc"));
        assert_eq!(
            identity.primary_namespace.as_deref(),
            Some("cold_war_curtain")
        );
        assert_eq!(
            identity.descriptor_tags,
            vec!["Gameplay".to_string(), "Total Conversion".to_string()]
        );
    }

    #[test]
    fn missing_codex_opener_returns_manual_path_result() {
        let root = tempdir().unwrap();
        let result = manual_open_in_codex_result(root.path());

        assert!(!result.opened);
        assert!(result.message.contains("Open this folder manually"));
        assert!(result.message.contains(&root.path().display().to_string()));
    }

    #[test]
    fn login_url_opener_rejects_untrusted_urls_before_process_resolution() {
        let error = open_codex_login_url("javascript:alert(1)".into()).unwrap_err();
        assert!(error.contains("invalid HTTPS authentication URL"));
    }

    #[test]
    fn usage_limited_chatgpt_accounts_are_blocked_before_authenticated_planning() {
        let status = CodexAccountStatus {
            available: true,
            authenticated: true,
            auth_mode: "chatgpt".into(),
            usage_limited: true,
            ..Default::default()
        };

        let error = validate_codex_planning_account(&status).unwrap_err();

        assert!(error.to_string().contains("usage is currently limited"));
    }

    #[test]
    fn login_cancel_command_sets_only_the_in_memory_cancellation_flag() {
        CODEX_LOGIN_CANCEL.store(false, Ordering::SeqCst);

        codex_login_cancel().unwrap();

        assert!(CODEX_LOGIN_CANCEL.load(Ordering::SeqCst));
        CODEX_LOGIN_CANCEL.store(false, Ordering::SeqCst);
    }

    #[test]
    fn scan_request_ids_are_bounded_and_cancellation_sets_the_active_flag() {
        assert!(validate_scan_request_id("scan-1").is_ok());
        assert!(validate_scan_request_id("").is_err());
        assert!(validate_scan_request_id("scan\n1").is_err());
        assert!(validate_scan_request_id(&"x".repeat(129)).is_err());

        let request_id = format!("test-{}", Uuid::new_v4());
        let cancellation = Arc::new(AtomicBool::new(false));
        scan_cancellations()
            .lock()
            .unwrap()
            .insert(request_id.clone(), cancellation.clone());
        assert!(cancel_scan(request_id.clone()).is_ok());
        assert!(cancellation.load(Ordering::SeqCst));
        scan_cancellations().lock().unwrap().remove(&request_id);
    }

    #[test]
    fn cancelled_scan_clears_previous_approved_evidence() {
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(PathBuf::from("C:/mods/example")),
            scan_id: Some(Uuid::new_v4()),
            entries: HashMap::from([("finding".into(), Vec::new())]),
        };

        clear_approved_scan_evidence().unwrap();

        let evidence = codex_approved_evidence().lock().unwrap();
        assert!(evidence.project_root.is_none());
        assert!(evidence.scan_id.is_none());
        assert!(evidence.entries.is_empty());
    }

    #[test]
    fn signed_out_or_wrong_account_types_are_blocked_from_planning() {
        for status in [
            CodexAccountStatus {
                available: true,
                authenticated: false,
                auth_mode: "chatgpt".into(),
                ..Default::default()
            },
            CodexAccountStatus {
                available: true,
                authenticated: true,
                auth_mode: "apiKey".into(),
                ..Default::default()
            },
        ] {
            assert!(validate_codex_planning_account(&status).is_err());
        }
    }

    #[test]
    fn approval_transition_does_not_change_plan_fingerprint() {
        let mut plan = InstallationPlan {
            schema_version: "1.0.0".into(),
            plan_id: Uuid::new_v4(),
            project_id: "example".into(),
            created_at: None,
            maintenance_mode: None,
            source: SourceIdentity {
                repository: crate::source::SOURCE_REPOSITORY.into(),
                mode: SourceMode::PinnedCommit,
                resolved_revision: "599497ea2f93612d9094461c6fde114fc87a5c0f".into(),
                requested_ref: None,
                release: None,
                manifest_sha256: "a".repeat(64),
                manifest_origin: "remote".into(),
            },
            codex_analysis: None,
            selected_components: vec![],
            wiki_required_pages: vec![],
            generated_artifacts: vec![],
            git_setup: None,
            credential_references: vec![],
            optional_workflows: Default::default(),
            operations: vec![],
            conflicts: vec![],
            external_actions: vec![],
            transaction: TransactionPlanInfo {
                stages: TRANSACTION_STAGES
                    .iter()
                    .map(|stage| (*stage).into())
                    .collect(),
                backup_root: "backup".into(),
                staging_root: "staging".into(),
                atomic_apply_expected: true,
            },
            approvals: PlanApprovals {
                dry_run_reviewed: false,
                external_actions_reviewed: false,
                git_remote_approved: false,
                push_approved: false,
            },
        };
        let before = plan_fingerprint(&plan).unwrap();
        plan.approvals.dry_run_reviewed = true;
        plan.approvals.external_actions_reviewed = true;
        plan.approvals.git_remote_approved = true;
        plan.approvals.push_approved = true;
        assert_eq!(plan_fingerprint(&plan).unwrap(), before);
    }

    #[test]
    fn update_maintenance_requires_a_fresh_reanalysis_record() {
        let project = tempfile::tempdir().unwrap();
        let error = require_maintenance_reanalysis("update", None, project.path()).unwrap_err();
        assert!(error
            .to_string()
            .contains("fresh, confirmed Codex reanalysis"));
        assert!(require_maintenance_reanalysis("repair", None, project.path()).is_ok());
        assert!(require_maintenance_reanalysis(
            "update",
            Some(&CodexAnalysisRecord {
                engine: "codex_app_server".into(),
                auth_mode: "chatgpt".into(),
                analysis_id: Uuid::new_v4(),
                schema_version: "1.0.0".into(),
                input_sha256: "a".repeat(64),
                output_sha256: "b".repeat(64),
                confirmed_fields: crate::codex::REQUIRED_ANALYSIS_PROPOSAL_KEYS
                    .iter()
                    .map(|field| (*field).into())
                    .collect(),
                confirmed_at: "2026-07-26T00:00:00Z".into(),
                account_identity_persisted: false,
                analysis_purpose: Some("maintenance_reanalysis".into()),
                project_root: Some(project.path().canonicalize().unwrap().display().to_string()),
                scan_id: Some(Uuid::new_v4()),
                evidence_sha256: Some("c".repeat(64)),
            }),
            project.path(),
        )
        .is_err());
    }

    #[test]
    fn update_reanalysis_is_bound_to_the_latest_scan_context() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        let scan_id = Uuid::new_v4();
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(root.clone()),
            scan_id: Some(scan_id),
            entries: HashMap::from([(
                "git.repository".into(),
                vec![(".git/HEAD".into(), "a".repeat(64))],
            )]),
        };
        let analysis_id = Uuid::new_v4();
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            analysis_id,
            schema_version: "1.0.0".into(),
            input_sha256: "a".repeat(64),
            output_sha256: "b".repeat(64),
            confirmed_fields: crate::codex::REQUIRED_ANALYSIS_PROPOSAL_KEYS
                .iter()
                .map(|field| (*field).into())
                .collect(),
            confirmed_at: "2026-07-26T00:00:00Z".into(),
            account_identity_persisted: false,
            analysis_purpose: Some("maintenance_reanalysis".into()),
            project_root: Some(root.display().to_string()),
            scan_id: Some(scan_id),
            evidence_sha256: Some("c".repeat(64)),
        };
        codex_analyses().lock().unwrap().insert(
            analysis_id,
            PendingCodexAnalysis {
                analysis: CodexAnalysis {
                    schema_version: "1.0.0".into(),
                    analysis_id,
                    mode: crate::codex::AnalysisMode::ExistingProjectSemantics,
                    input_sha256: "a".repeat(64),
                    project_summary: "summary".into(),
                    proposals: Vec::new(),
                    component_recommendations: Vec::new(),
                    warnings: Vec::new(),
                },
                record: record.clone(),
                confirmed: Some(record.clone()),
                project_root: Some(root.clone()),
                scan_id: Some(scan_id),
            },
        );
        assert!(require_maintenance_reanalysis("update", Some(&record), &root).is_ok());
        let mut stale = record;
        stale.evidence_sha256 = Some("d".repeat(64));
        assert!(require_maintenance_reanalysis("update", Some(&stale), &root).is_err());
        codex_analyses().lock().unwrap().clear();
        codex_approved_evidence().lock().unwrap().entries.clear();
    }

    #[test]
    fn three_d_health_state_requires_a_matching_core_lock_result() {
        assert_eq!(
            normalized_three_d_state("selected_pending", Some("ready"), true),
            "ready"
        );
        assert_eq!(
            normalized_three_d_state("selected_pending", Some("ready"), false),
            "incomplete"
        );
        assert_eq!(
            normalized_three_d_state("selected_pending", None, false),
            "incomplete"
        );
        assert_eq!(
            normalized_three_d_state("not_selected", Some("ready"), true),
            "not_selected"
        );
    }

    #[test]
    fn meshy_vault_changes_invalidate_cached_three_d_health() {
        three_d_health().lock().unwrap().insert(
            "C:/mods/example".into(),
            CachedThreeDHealth {
                lock_fingerprint: "fingerprint".into(),
                state: "ready".into(),
            },
        );
        invalidate_three_d_health();
        assert!(three_d_health().lock().unwrap().is_empty());
    }

    #[test]
    fn logout_clears_local_analysis_and_scan_evidence_without_a_remote_session() {
        let analysis_id = Uuid::new_v4();
        *codex_session().lock().unwrap() = None;
        codex_analyses().lock().unwrap().insert(
            analysis_id,
            PendingCodexAnalysis {
                analysis: CodexAnalysis {
                    schema_version: "1.0.0".into(),
                    analysis_id,
                    mode: crate::codex::AnalysisMode::NewProjectIdentity,
                    input_sha256: "a".repeat(64),
                    project_summary: "summary".into(),
                    proposals: Vec::new(),
                    component_recommendations: Vec::new(),
                    warnings: Vec::new(),
                },
                record: CodexAnalysisRecord {
                    engine: "codex_app_server".into(),
                    auth_mode: "chatgpt".into(),
                    analysis_id,
                    schema_version: "1.0.0".into(),
                    input_sha256: "a".repeat(64),
                    output_sha256: "b".repeat(64),
                    confirmed_fields: Vec::new(),
                    confirmed_at: "pending".into(),
                    account_identity_persisted: false,
                    analysis_purpose: None,
                    project_root: None,
                    scan_id: None,
                    evidence_sha256: None,
                },
                confirmed: None,
                project_root: None,
                scan_id: None,
            },
        );
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(PathBuf::from("C:/mods/example")),
            scan_id: Some(Uuid::new_v4()),
            entries: HashMap::from([("finding".into(), Vec::new())]),
        };

        codex_logout().unwrap();

        assert!(codex_analyses().lock().unwrap().is_empty());
        let evidence = codex_approved_evidence().lock().unwrap();
        assert!(evidence.project_root.is_none());
        assert!(evidence.scan_id.is_none());
        assert!(evidence.entries.is_empty());
    }
}
