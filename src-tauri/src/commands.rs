//! The only filesystem, network, process, and credential boundary exposed to React.

use crate::ai::{self, AiProviderConfig};
use crate::codex::{
    find_codex_executable, missing_status, AiAccountStatus, AiAnalysisRequest, AppServerProtocol,
    ApprovedEvidence, CodexAccountStatus, CodexAnalysis, CodexAnalysisRequest, CodexAnalysisResult,
    CodexLoginStart, ProcessJsonlTransport,
};
use crate::credentials::{
    discover_meshy_credential_reference, provider_name, save_ai_provider_key, save_meshy_key,
    validate_ai_provider_credential_for, validate_credential_reference, CredentialStore,
    OsCredentialStore, ScopedSecretEnvironment, MESHY_ENVIRONMENT_NAME,
};
use crate::descriptors::generated_artifacts as render_generated_artifacts;
use crate::merge::{
    allowed_choices, structured_json_merge, structured_toml_merge, three_way_merge,
    validate_merged_result, FileKind,
};
use crate::models::{CredentialReference, *};
use crate::paths::{
    application_data_root, hoi4_user_mod_directory, validate_project_root,
    validate_project_root_or_destination,
};
use crate::readiness::ReadinessInput;
use crate::scanner::{
    discover_launcher_descriptor, scan_project_with_progress as scan_project_files, ScanOptions,
    ScanProgress,
};
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
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;
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
    launcher_descriptor_path: Option<String>,
    error: Option<String>,
    cancelled: bool,
}

#[derive(Debug, serde::Serialize)]
struct SuggestedProjectPaths {
    mod_directory: String,
    project_root: String,
    launcher_descriptor_path: String,
    project_exists: bool,
    launcher_descriptor_exists: bool,
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

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppUpdateStatus {
    current_version: String,
    available_version: Option<String>,
    available: bool,
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
static CODEX_LOGIN_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();
static SCAN_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
static MESHY_CREDENTIAL_REFERENCE: OnceLock<Mutex<Option<CredentialReference>>> = OnceLock::new();
static THREE_D_HEALTH: OnceLock<Mutex<HashMap<String, CachedThreeDHealth>>> = OnceLock::new();

struct PendingCodexAnalysis {
    analysis: CodexAnalysis,
    record: CodexAnalysisRecord,
    confirmed: Option<CodexAnalysisRecord>,
    project_root: Option<PathBuf>,
    scan_id: Option<Uuid>,
    endpoint_fingerprint: Option<String>,
    confirmed_values_sha256: Option<String>,
}

#[derive(Default)]
struct ApprovedScanEvidence {
    project_root: Option<PathBuf>,
    scan_id: Option<Uuid>,
    entries: HashMap<String, Vec<(String, String)>>,
    /// Hash of the exact evidence vector the user approved for the next
    /// semantic turn. A completed scan is not itself an approval.
    evidence_sha256: Option<String>,
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

fn codex_login_cancellations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    CODEX_LOGIN_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_codex_login(login_id: &str) -> Result<Arc<AtomicBool>, String> {
    let mut cancellations = codex_login_cancellations()
        .lock()
        .map_err(|_| "Codex login cancellation store is unavailable".to_string())?;
    cancellations.retain(|_, cancellation| !cancellation.load(Ordering::SeqCst));
    if cancellations.len() >= 8 {
        return Err(
            "too many Codex sign-in attempts are pending; cancel an earlier attempt first".into(),
        );
    }
    if cancellations.contains_key(login_id) {
        return Err("Codex returned a duplicate active login identifier".into());
    }
    let cancellation = Arc::new(AtomicBool::new(false));
    cancellations.insert(login_id.to_string(), cancellation.clone());
    Ok(cancellation)
}

fn cancel_all_codex_logins() {
    if let Ok(cancellations) = codex_login_cancellations().lock() {
        for cancellation in cancellations.values() {
            cancellation.store(true, Ordering::SeqCst);
        }
    }
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

fn current_meshy_credential_reference() -> Result<Option<CredentialReference>, AppError> {
    let mut current = meshy_credential_reference().lock().map_err(|_| {
        AppError::Credential("Meshy credential reference store is unavailable".into())
    })?;
    if current.as_ref().is_some_and(|reference| {
        crate::credentials::mesh_key_status(&OsCredentialStore, Some(reference)) == "present"
    }) {
        return Ok(current.clone());
    }
    let discovered = discover_meshy_credential_reference(&OsCredentialStore)?;
    *current = discovered.clone();
    Ok(discovered)
}

fn ai_credential_reference(provider: &str) -> Option<CredentialReference> {
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
    with_supervised_codex_session(
        &mut session,
        || {
            let executable = find_codex_executable().ok_or_else(|| {
                AppError::Process(
                    "official Codex executable was not found on the reviewed PATH".into(),
                )
            })?;
            let mut protocol = AppServerProtocol::new(ProcessJsonlTransport::start(executable)?);
            protocol.initialize()?;
            Ok(protocol)
        },
        AppServerProtocol::is_alive,
        callback,
        clear_codex_local_state,
    )
}

fn with_supervised_codex_session<S, R>(
    session: &mut Option<S>,
    start_initialized: impl FnOnce() -> Result<S, AppError>,
    is_alive: impl FnOnce(&mut S) -> bool,
    callback: impl FnOnce(&mut S) -> Result<R, AppError>,
    mut clear_local_state: impl FnMut() -> Result<(), String>,
) -> Result<R, AppError> {
    if session.as_mut().is_some_and(|protocol| !is_alive(protocol)) {
        *session = None;
        clear_local_state().map_err(AppError::Process)?;
    }
    if session.is_none() {
        *session = Some(start_initialized()?);
    }
    let result = callback(
        session
            .as_mut()
            .expect("Codex session was just initialized"),
    );
    if matches!(
        &result,
        Err(AppError::Process(_) | AppError::Serialization(_))
    ) {
        *session = None;
        clear_local_state().map_err(AppError::Process)?;
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

fn require_ai_session(state: &Value) -> Result<(), AppError> {
    let provider = ai_provider_from_state(state)?;
    if provider == "codex" {
        return require_codex_chatgpt_session();
    }
    let status = ai::account_status(
        &OsCredentialStore,
        &AiProviderConfig {
            provider: provider.clone(),
            model: ai_model_from_state(state),
            endpoint: ai_endpoint_from_state(state).unwrap_or_default(),
            credential_reference: ai_credential_reference(&provider),
        },
    );
    if status.error.is_some() || !status.available || !status.authenticated {
        return Err(AppError::Credential(format!(
            "connect the selected {provider} provider before planning"
        )));
    }
    Ok(())
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

fn store_prepared_plan(
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
    // Build the optional flattened view from the currently accepted source
    // set before exposing the plan to the renderer. In particular, an
    // unresolved modified AGENTS/skill/subagent file must never leak incoming
    // bytes into the Chat export merely because the user has not opened the
    // conflict screen yet.
    refresh_flattened_outputs(&mut prepared_plan)?;
    let plan = prepared_plan.plan.clone();
    prepared_plans()
        .lock()
        .map_err(|_| AppError::Transaction("prepared plan store is unavailable".into()))?
        .insert(id, prepared_plan);
    Ok(plan)
}

fn command_error(error: AppError) -> String {
    error.to_string()
}

fn run_blocking_command<T, F>(name: &'static str, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    std::thread::Builder::new()
        .name(format!("hoi4-setup-{name}"))
        .spawn(work)
        .map_err(|_| format!("{name} could not be started"))?
        .join()
        .map_err(|_| format!("{name} stopped unexpectedly"))?
}

fn codex_user_error(error: AppError) -> String {
    match error {
        AppError::Process(message)
            if message.contains("official Codex executable was not found") =>
        {
            "Codex is not installed or could not be found. Install or update Codex, then choose Check again."
                .into()
        }
        AppError::Process(message)
            if message.contains("Codex usage state could not be checked") =>
        {
            "Codex usage could not be checked. Choose Check again to retry.".into()
        }
        AppError::Process(_) => {
            "Codex is temporarily unavailable. Close and reopen Codex, then try again.".into()
        }
        AppError::Credential(message) => {
            let category = message.to_ascii_lowercase();
            if category.contains("cancel") {
                "ChatGPT sign-in was cancelled.".into()
            } else if category.contains("usage") && category.contains("limited") {
                "Codex usage is currently limited. Your draft is unchanged; try again when usage is available.".into()
            } else if category.contains("credential-shaped") {
                "Codex blocked private-looking information. Remove secrets from the description or selected evidence, then try again.".into()
            } else if category.contains("sign in") || category.contains("authentication") {
                "Sign in with ChatGPT before continuing.".into()
            } else {
                "ChatGPT sign-in needs attention. Try again.".into()
            }
        }
        AppError::Serialization(_) => {
            "Codex returned an unexpected response. Update Codex and try again.".into()
        }
        AppError::UnsupportedPlatform(_) => {
            "Codex sign-in is not available on this computer.".into()
        }
        _ => "Codex could not complete this request. Try again.".into(),
    }
}

fn journal_bound_to_root(
    journal: &TransactionJournal,
    project_root: &Path,
) -> Result<bool, AppError> {
    if journal.project_root.is_empty() {
        return Ok(false);
    }
    let bound_root = validate_project_root_or_destination(Path::new(&journal.project_root))?.0;
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                cancel_all_codex_logins();
                if let Ok(mut session) = codex_session().lock() {
                    *session = None;
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            app_update_check,
            app_update_install,
            ai_provider_profiles,
            ai_account_read,
            store_ai_provider_credential,
            remove_ai_provider_credential,
            codex_account_read,
            codex_login_start,
            codex_login_wait,
            codex_login_cancel,
            open_codex_login_url,
            open_external_url,
            codex_logout,
            codex_analyze,
            ai_analyze,
            approve_scan_evidence,
            confirm_codex_analysis,
            pick_project_folder,
            pick_launcher_folder,
            suggest_project_paths,
            scan_project,
            cancel_scan,
            store_meshy_credential,
            meshy_credential_status,
            remove_meshy_credential,
            run_3d_health_check,
            run_mcp_health_check,
            evaluate_readiness,
            preview_descriptors,
            preview_source_manifest,
            preview_installation_conflict,
            build_installation_plan,
            build_maintenance_plan,
            git_online_prepare,
            git_online_action,
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

#[tauri::command(async)]
fn app_info() -> Value {
    serde_json::json!({
        "product": "HOI4 Mod Setup",
        "source_repository": crate::source::SOURCE_REPOSITORY,
        "manifest": crate::source::MANIFEST_PATH,
        "supported_platforms": ["windows", "macos"],
    })
}

#[tauri::command(async)]
async fn app_update_check(app: tauri::AppHandle) -> Result<AppUpdateStatus, String> {
    let current_version = app.package_info().version.to_string();
    let update = app
        .updater()
        .map_err(|_| "Updates are unavailable in this build.".to_string())?
        .check()
        .await
        .map_err(|_| "The update service could not be reached.".to_string())?;
    Ok(AppUpdateStatus {
        current_version,
        available_version: update.as_ref().map(|candidate| candidate.version.clone()),
        available: update.is_some(),
    })
}

#[tauri::command(async)]
async fn app_update_install(app: tauri::AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|_| "Updates are unavailable in this build.".to_string())?
        .check()
        .await
        .map_err(|_| "The update service could not be reached.".to_string())?
        .ok_or_else(|| "HOI4 Mod Setup is already up to date.".to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|_| "The update could not be verified or installed.".to_string())?;
    app.restart();
}

#[tauri::command(async)]
fn ai_provider_profiles() -> Vec<AiProviderProfile> {
    ai::provider_profiles()
}

#[tauri::command(async)]
fn ai_account_read(provider: String, model: String, endpoint: String) -> AiAccountStatus {
    let credential_reference = ai_credential_reference(&provider);
    ai::account_status(
        &OsCredentialStore,
        &AiProviderConfig {
            provider,
            model,
            endpoint,
            credential_reference,
        },
    )
}

#[tauri::command(async)]
fn store_ai_provider_credential(provider: String, value: String) -> Result<bool, String> {
    if ai::profile(provider.trim())
        .is_none_or(|profile| provider.trim() == "codex" || !profile.requires_credential)
    {
        return Err(
            "AI provider credentials are available only for a configured hosted provider".into(),
        );
    }
    let reference =
        save_ai_provider_key(&OsCredentialStore, provider.trim(), &value).map_err(command_error)?;
    validate_ai_provider_credential_for(&reference, provider.trim()).map_err(command_error)?;
    Ok(true)
}

#[tauri::command(async)]
fn remove_ai_provider_credential(provider: String) -> Result<bool, String> {
    if ai::profile(provider.trim())
        .is_none_or(|profile| provider.trim() == "codex" || !profile.requires_credential)
    {
        return Err(
            "AI provider credentials are available only for a configured hosted provider".into(),
        );
    }
    let reference = ai_credential_reference(provider.trim())
        .ok_or_else(|| "no stored credential exists for the selected provider".to_string())?;
    validate_ai_provider_credential_for(&reference, provider.trim()).map_err(command_error)?;
    OsCredentialStore
        .delete(&reference)
        .map_err(command_error)?;
    Ok(true)
}

#[tauri::command(async)]
fn codex_account_read() -> CodexAccountStatus {
    match with_codex_session(|session| session.account_read(false)) {
        Ok(status) => status,
        Err(error) => missing_status(codex_user_error(error)),
    }
}

#[tauri::command(async)]
fn codex_login_start(mode: String) -> CodexLoginStart {
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
        Ok(mut start) => {
            if start.error.is_some() {
                if let Some(login_id) = start.login_id.as_deref() {
                    let _ = with_codex_session(|session| session.cancel_login(login_id));
                }
                start.login_id = None;
                return start;
            }
            if let Some(login_id) = start.login_id.as_deref() {
                if let Err(error) = register_codex_login(login_id) {
                    let _ = with_codex_session(|session| session.cancel_login(login_id));
                    start.login_id = None;
                    start.error = Some(error);
                }
            }
            start
        }
        Err(error) => CodexLoginStart {
            available: false,
            error: Some(codex_user_error(error)),
            device_code,
            ..Default::default()
        },
    }
}

#[tauri::command(async)]
fn codex_login_wait(login_id: String) -> Result<CodexAccountStatus, String> {
    let cancellation = codex_login_cancellations()
        .lock()
        .map_err(|_| "Codex login cancellation store is unavailable".to_string())?
        .get(&login_id)
        .cloned()
        .ok_or_else(|| "Codex login is not active or was already cancelled".to_string())?;
    let result = with_codex_session(|session| {
        session.wait_for_login_with_cancel(&login_id, Duration::from_secs(120), || {
            cancellation.load(Ordering::SeqCst)
        })
    });
    if let Ok(mut cancellations) = codex_login_cancellations().lock() {
        cancellations.remove(&login_id);
    }
    result.map_err(codex_user_error)
}

#[tauri::command(async)]
fn codex_login_cancel(login_id: String) -> Result<(), String> {
    let cancellation = codex_login_cancellations()
        .lock()
        .map_err(|_| "Codex login cancellation store is unavailable".to_string())?
        .get(&login_id)
        .cloned()
        .ok_or_else(|| "Codex login is not active or was already cancelled".to_string())?;
    cancellation.store(true, Ordering::SeqCst);

    if let Ok(mut session) = codex_session().try_lock() {
        if let Some(protocol) = session.as_mut() {
            if let Err(error) = protocol.cancel_login(&login_id) {
                *session = None;
                codex_login_cancellations()
                    .lock()
                    .map_err(|_| "Codex login cancellation store is unavailable".to_string())?
                    .remove(&login_id);
                return Err(codex_user_error(error));
            }
        }
        codex_login_cancellations()
            .lock()
            .map_err(|_| "Codex login cancellation store is unavailable".to_string())?
            .remove(&login_id);
    }
    Ok(())
}

#[tauri::command(async)]
fn open_codex_login_url(url: String) -> Result<(), String> {
    if !crate::codex::is_safe_login_url(&url) {
        return Err("Codex returned an invalid HTTPS authentication URL".into());
    }
    open_url_in_system_browser(url)
}

#[tauri::command(async)]
fn open_external_url(url: String) -> Result<(), String> {
    if !is_allowed_external_url(&url) {
        return Err("This external link is not allowed".into());
    }
    open_url_in_system_browser(url)
}

fn is_allowed_external_url(url: &str) -> bool {
    const ALLOWED_URLS: [&str; 2] = [
        "https://github.com/klimPaskov/Agentic-HOI4-Modding",
        "https://github.com/klimPaskov/comfyui-hoi4-portraits",
    ];
    ALLOWED_URLS.contains(&url)
        || ai::provider_profiles()
            .iter()
            .filter_map(|profile| profile.account_url.as_deref())
            .any(|account_url| account_url == url)
}

fn open_url_in_system_browser(url: String) -> Result<(), String> {
    let executable = crate::process::system_browser_executable()
        .ok_or_else(|| "No supported system-browser opener is available".to_string())?;
    let spec = crate::process::ProcessSpec {
        executable: executable.clone(),
        executable_sha256: Some(sha256_file(&executable).map_err(command_error)?),
        args: vec![url],
        cwd: None,
        platform: Platform::current(),
        environment_names: Vec::new(),
        timeout_seconds: 10,
        max_output_bytes: 1,
    };
    spec.spawn_detached(&[executable]).map_err(command_error)
}

#[tauri::command(async)]
fn codex_logout() -> Result<(), String> {
    cancel_all_codex_logins();
    // A prior App Server interruption may have cleared the supervised
    // process while Codex-owned authentication remains persisted. Start and
    // initialize a fresh App Server when needed so sign-out always attempts
    // the official account/logout method.
    let logout_result = with_codex_session(|protocol| protocol.logout()).map_err(codex_user_error);
    complete_codex_logout(logout_result)
}

fn complete_codex_logout(logout_result: Result<(), String>) -> Result<(), String> {
    if let Ok(mut session) = codex_session().lock() {
        return finalize_codex_logout_state(
            &mut *session,
            logout_result,
            clear_codex_local_state,
            || {
                if let Ok(mut cancellations) = codex_login_cancellations().lock() {
                    cancellations.clear();
                }
            },
        );
    }
    let clear_result = clear_codex_local_state();
    if let Ok(mut cancellations) = codex_login_cancellations().lock() {
        cancellations.clear();
    }
    clear_result?;
    logout_result
}

fn finalize_codex_logout_state<S>(
    session: &mut Option<S>,
    logout_result: Result<(), String>,
    clear_local_state: impl FnOnce() -> Result<(), String>,
    clear_cancellations: impl FnOnce(),
) -> Result<(), String> {
    *session = None;
    // Local proposals and evidence are session-scoped and must be discarded
    // even when the supervised App Server cannot complete account/logout.
    let clear_result = clear_local_state();
    clear_cancellations();
    clear_result?;
    logout_result
}

#[tauri::command(async)]
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
    let result =
        with_codex_session(|session| session.analyze(&request)).map_err(codex_user_error)?;
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
            endpoint_fingerprint: None,
            confirmed_values_sha256: None,
        },
    );
    Ok(result)
}

#[tauri::command(async)]
fn ai_analyze(request: AiAnalysisRequest) -> Result<CodexAnalysisResult, String> {
    run_blocking_command("provider-analysis", move || ai_analyze_blocking(request))
}

fn ai_analyze_blocking(mut request: AiAnalysisRequest) -> Result<CodexAnalysisResult, String> {
    if request.provider == "codex" {
        return Err("Codex analysis must use the official Codex App Server command".into());
    }
    if let Some(project_root) = request.analysis.project_root.as_deref() {
        request.analysis.project_root = Some(
            validate_project_root(Path::new(project_root))
                .map_err(command_error)?
                .display()
                .to_string(),
        );
    }
    let credential_reference = ai_credential_reference(&request.provider);
    if let Some(reference) = credential_reference.as_ref() {
        validate_ai_provider_credential_for(reference, request.provider.trim())
            .map_err(command_error)?;
    }
    validate_codex_evidence_approval(&request.analysis).map_err(command_error)?;
    let result = ai::analyze(
        &OsCredentialStore,
        &AiProviderConfig {
            provider: request.provider.clone(),
            model: request.model.clone(),
            endpoint: request.endpoint.clone(),
            credential_reference,
        },
        &request,
    )
    .map_err(command_error)?;
    let mut analyses = codex_analyses()
        .lock()
        .map_err(|_| "AI analysis store is unavailable".to_string())?;
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
            project_root: request.analysis.project_root.clone().map(PathBuf::from),
            scan_id: request.analysis.scan_id,
            endpoint_fingerprint: Some(sha256_bytes(request.endpoint.trim().as_bytes())),
            confirmed_values_sha256: None,
        },
    );
    Ok(result)
}

#[tauri::command(async)]
fn approve_scan_evidence(
    project_root: String,
    scan_id: String,
    evidence: Vec<ApprovedEvidence>,
) -> Result<(), String> {
    let project_root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let scan_id = Uuid::parse_str(&scan_id).map_err(|_| "scan ID is invalid".to_string())?;
    crate::codex::validate_analysis_evidence(&evidence).map_err(command_error)?;
    let mut approved = codex_approved_evidence()
        .lock()
        .map_err(|_| "approved scan evidence store is unavailable".to_string())?;
    if approved
        .project_root
        .as_ref()
        .is_none_or(|root| !same_project_root(root, &project_root))
        || approved.scan_id != Some(scan_id)
    {
        return Err("scan evidence is stale or belongs to a different project".into());
    }
    let approved_sha256 =
        crate::codex::evidence_manifest_sha256(&evidence).map_err(command_error)?;
    for item in &evidence {
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
    }
    let requested_evidence_sha256 = crate::codex::evidence_manifest_sha256(&request.evidence)?;
    if approved.evidence_sha256.as_deref() != Some(requested_evidence_sha256.as_str()) {
        return Err(AppError::Credential(
            "Codex evidence must be explicitly approved after the latest read-only scan".into(),
        ));
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

fn canonical_confirmation_values(value: &Value) -> Result<Value, AppError> {
    let object = value.as_object().ok_or_else(|| {
        AppError::InvalidInput("confirmed render values must be an object".into())
    })?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "identity" | "description" | "folderProfile"))
    {
        return Err(AppError::InvalidInput(
            "confirmed render values contain an unsupported field".into(),
        ));
    }
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidInput("confirmed description is missing".into()))?;
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
    Ok(serde_json::json!({
        "description": description,
        "folderProfile": folders,
        "identity": canonical_identity,
    }))
}

fn confirmation_values_sha256(value: &Value) -> Result<String, AppError> {
    Ok(sha256_bytes(&serde_json::to_vec(
        &canonical_confirmation_values(value)?,
    )?))
}

fn confirmation_values_from_state(state: &Value) -> Result<Value, AppError> {
    Ok(serde_json::json!({
        "description": state.get("description").cloned().unwrap_or(Value::String(String::new())),
        "folderProfile": state.get("folderProfile").cloned().unwrap_or_else(|| serde_json::json!([])),
        "identity": state.get("identity").cloned().unwrap_or(Value::Null),
    }))
}

#[tauri::command(async)]
fn confirm_codex_analysis(
    record: CodexAnalysisRecord,
    confirmed_fields: Vec<String>,
    confirmed_values: Value,
) -> Result<CodexAnalysisRecord, String> {
    let confirmed_values_sha256 =
        confirmation_values_sha256(&confirmed_values).map_err(command_error)?;
    let mut analyses = codex_analyses()
        .lock()
        .map_err(|_| "Codex analysis store is unavailable".to_string())?;
    let pending = analyses
        .get_mut(&record.analysis_id)
        .ok_or_else(|| "Codex analysis is no longer available in the core session".to_string())?;
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
    let confirmed = crate::codex::confirm_analysis_record(
        &pending.record,
        &pending.analysis,
        &confirmed_fields,
    )
    .map_err(command_error)?;
    pending.confirmed = Some(confirmed.clone());
    pending.confirmed_values_sha256 = Some(confirmed_values_sha256);
    Ok(confirmed)
}

#[tauri::command(async)]
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
            launcher_descriptor_path: None,
            error: None,
            cancelled: true,
        });
    };
    let path = match selected.into_path() {
        Ok(path) => path,
        Err(error) => {
            return Ok(FolderSelection {
                path: None,
                launcher_descriptor_path: None,
                error: Some(format!("selected folder is not a local path: {error}")),
                cancelled: false,
            });
        }
    };
    match validate_project_root(&path) {
        Ok(canonical) => {
            let launcher_descriptor_path = discover_launcher_descriptor(&canonical)
                .map_err(command_error)?
                .map(|path| crate::paths::user_facing_path(&path));
            Ok(FolderSelection {
                path: Some(crate::paths::user_facing_path(&canonical)),
                launcher_descriptor_path,
                error: None,
                cancelled: false,
            })
        }
        Err(error) => Ok(FolderSelection {
            path: None,
            launcher_descriptor_path: None,
            error: Some(error.to_string()),
            cancelled: false,
        }),
    }
}

#[tauri::command(async)]
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
            launcher_descriptor_path: None,
            error: None,
            cancelled: true,
        });
    };
    let path = selected
        .into_path()
        .map_err(|error| format!("selected folder is not a local path: {error}"))?;
    let canonical = validate_project_root(&path).map_err(command_error)?;
    Ok(FolderSelection {
        path: Some(crate::paths::user_facing_path(&canonical)),
        launcher_descriptor_path: None,
        error: None,
        cancelled: false,
    })
}

#[tauri::command(async)]
fn suggest_project_paths(project_id: String) -> Result<SuggestedProjectPaths, String> {
    crate::descriptors::validate_project_id(&project_id).map_err(command_error)?;
    let mod_directory = hoi4_user_mod_directory().map_err(command_error)?;
    let project_root = mod_directory.join(&project_id);
    let launcher_descriptor_path = mod_directory.join(format!("{project_id}.mod"));
    if path_has_link_component(&project_root) || path_has_link_component(&launcher_descriptor_path)
    {
        return Err("The standard HOI4 destination contains a symlink or junction".into());
    }
    Ok(SuggestedProjectPaths {
        mod_directory: crate::paths::user_facing_path(&mod_directory),
        project_exists: project_root.exists(),
        launcher_descriptor_exists: launcher_descriptor_path.exists(),
        project_root: crate::paths::user_facing_path(&project_root),
        launcher_descriptor_path: crate::paths::user_facing_path(&launcher_descriptor_path),
    })
}

#[tauri::command(async)]
fn scan_project(
    app: tauri::AppHandle,
    root: String,
    request_id: String,
    launcher_descriptor_path: Option<String>,
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
    let launcher_descriptor = launcher_descriptor_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(validate_external_destination)
        .transpose()
        .map_err(command_error)?;
    let result = scan_project_files(
        &root,
        &ScanOptions {
            approved_external_descriptor: launcher_descriptor,
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
        approved.evidence_sha256 = None;
    }
    Ok(result)
}

#[tauri::command(async)]
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

#[tauri::command(async)]
fn store_meshy_credential(value: String) -> Result<CredentialReference, String> {
    let reference = save_meshy_key(&OsCredentialStore, &value).map_err(command_error)?;
    invalidate_three_d_health();
    *meshy_credential_reference()
        .lock()
        .map_err(|_| "Meshy credential reference store is unavailable".to_string())? =
        Some(reference.clone());
    Ok(reference)
}

#[tauri::command(async)]
fn meshy_credential_status() -> Result<Option<CredentialReference>, String> {
    current_meshy_credential_reference().map_err(command_error)
}

#[tauri::command(async)]
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
        let bundled_bytes =
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
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

#[tauri::command(async)]
fn run_3d_health_check(project_root: String) -> Result<WorkflowHealthResult, String> {
    run_blocking_command("3d-health-check", move || {
        run_3d_health_check_blocking(project_root)
    })
}

fn run_3d_health_check_blocking(project_root: String) -> Result<WorkflowHealthResult, String> {
    if Platform::current() != Platform::Windows {
        return Ok(WorkflowHealthResult {
            status: "unsupported_platform".into(),
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: "the verified 3D workflow is currently supported only on Windows".into(),
        });
    }
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
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
    let script_bytes = crate::flatten::read_regular_file_no_follow_under_root(&root, target)
        .map_err(command_error)?;
    if locked_file
        .installed_size
        .is_some_and(|expected| expected != script_bytes.len() as u64)
        || sha256_bytes(&script_bytes) != locked_file.installed_sha256
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
        provider_id: None,
    };
    let environment = ScopedSecretEnvironment::from_credential(
        &OsCredentialStore,
        &reference,
        MESHY_ENVIRONMENT_NAME,
    )
    .map_err(command_error)?;
    let python =
        crate::process::find_path_executable(&["python.exe", "python"]).map_err(command_error)?;
    crate::process::validate_executable_publisher(&python, "Python Software Foundation")
        .map_err(command_error)?;
    let private_script = create_private_verified_script(&script_bytes).map_err(command_error)?;
    let spec = crate::process::ProcessSpec {
        executable: python.clone(),
        executable_sha256: Some(sha256_file(&python).map_err(command_error)?),
        args: vec![private_script.display().to_string()],
        cwd: Some(root.clone()),
        platform: Platform::Windows,
        environment_names: vec![MESHY_ENVIRONMENT_NAME.into()],
        timeout_seconds: 10 * 60,
        max_output_bytes: 2 * 1024 * 1024,
    };
    let run_result = spec.run(&[python], Some(&environment));
    let cleanup_result = remove_private_verified_script(&private_script);
    let result = run_result.map_err(command_error);
    cleanup_result.map_err(command_error)?;
    let result = result?;
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

fn create_private_verified_script(bytes: &[u8]) -> Result<PathBuf, AppError> {
    let root = application_data_root()?;
    if path_has_link_component(&root) {
        return Err(AppError::PathSecurity(
            "application data root contains a symlink or junction".into(),
        ));
    }
    let directory = root.join("health-checks").join(Uuid::new_v4().to_string());
    if path_has_link_component(&directory) {
        return Err(AppError::PathSecurity(
            "3D health-check storage contains a symlink or junction".into(),
        ));
    }
    std::fs::create_dir_all(&directory)?;
    if path_has_link_component(&directory) {
        return Err(AppError::PathSecurity(
            "3D health-check storage changed during creation".into(),
        ));
    }
    let path = directory.join("workflow.py");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| AppError::PathSecurity("private health-check copy escaped its root".into()))?
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let verified = crate::flatten::read_regular_file_no_follow_under_root(&root, &relative)?;
    if verified != bytes {
        return Err(AppError::Transaction(
            "private 3D health-check copy failed verification".into(),
        ));
    }
    Ok(path)
}

fn remove_private_verified_script(path: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("private health-check copy has no parent".into()))?;
    if path_has_link_component(parent) {
        return Err(AppError::PathSecurity(
            "private health-check storage changed before cleanup".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if crate::security::is_link_metadata(&metadata) || !metadata.is_file() {
        return Err(AppError::PathSecurity(
            "private health-check copy is not a regular file".into(),
        ));
    }
    std::fs::remove_file(path)?;
    if path.exists() {
        return Err(AppError::Transaction(
            "private 3D health-check copy could not be removed".into(),
        ));
    }
    if let Some(directory) = parent.parent() {
        if path_has_link_component(directory) {
            return Err(AppError::PathSecurity(
                "private health-check parent changed before cleanup".into(),
            ));
        }
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
}

fn mcp_failure(error: AppError) -> WorkflowHealthResult {
    let message = redact_secrets(&error.to_string(), &[]);
    let status = if matches!(&error, AppError::UnsupportedPlatform(_)) {
        "planned_unavailable"
    } else {
        "incomplete"
    };
    WorkflowHealthResult {
        status: status.into(),
        exit_code: None,
        timed_out: message.to_ascii_lowercase().contains("timed out"),
        stdout: String::new(),
        stderr: bound_process_output(message),
    }
}

fn installed_mcp_target(
    project_root: &Path,
    lock: &InstallationLock,
) -> Result<crate::mcp::VerifiedMcpTarget, AppError> {
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
    if configured != Some(target.target.as_str()) {
        return Err(AppError::Process(
            "installed MCP configuration does not match the locked manifest command".into(),
        ));
    }
    Ok(target)
}

#[tauri::command(async)]
fn run_mcp_health_check(project_root: String) -> Result<WorkflowHealthResult, String> {
    run_blocking_command("mcp-health-check", move || {
        run_mcp_health_check_blocking(project_root)
    })
}

fn run_mcp_health_check_blocking(project_root: String) -> Result<WorkflowHealthResult, String> {
    if Platform::current() != Platform::Windows {
        return Ok(WorkflowHealthResult {
            status: "unsupported_platform".into(),
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: "the verified MCP workflow is currently supported only on Windows".into(),
        });
    }
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
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
        detected.mcp_status = if matches!(&error, AppError::UnsupportedPlatform(_)) {
            "planned_unavailable"
        } else {
            "block"
        }
        .into();
        detected.notes.push(format!(
            "MCP initialize health unavailable or failed: {}",
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
    notes: Vec<String>,
) -> Result<ReadinessReport, AppError> {
    let mut detected = crate::readiness::project_input(project_root, project_id)?;
    refresh_installed_mcp_readiness(project_root, &mut detected)?;
    let locked_ai = read_project_lock(project_root).ok();
    if detected.ai_provider == "codex" {
        let live_codex = with_codex_session(|session| session.account_read(false)).ok();
        detected.codex_authenticated = detected.codex_authenticated
            && live_codex.as_ref().is_some_and(|status| {
                status.error.is_none()
                    && status.authenticated
                    && status.auth_mode == "chatgpt"
                    && !status.usage_limited
            });
        detected.ai_authenticated = detected.codex_authenticated;
    } else {
        let endpoint = locked_ai
            .as_ref()
            .and_then(|lock| lock.ai_endpoint.clone())
            .unwrap_or_default();
        let credential_reference = ai_credential_reference(&detected.ai_provider);
        let status = ai::account_status(
            &OsCredentialStore,
            &AiProviderConfig {
                provider: detected.ai_provider.clone(),
                model: detected.ai_model.clone(),
                endpoint,
                credential_reference,
            },
        );
        detected.ai_authenticated = detected.ai_authenticated
            && status.error.is_none()
            && status.authenticated
            && !status.usage_limited;
    }
    if !detected.ai_authenticated {
        detected.ai_analysis_status = "blocked".into();
        detected.codex_analysis_status = "blocked".into();
    }
    detected.workflow_3d_state = read_project_lock(project_root)
        .map(|lock| cached_three_d_state(project_root, &lock))
        .unwrap_or(workflow_3d_state);
    detected.notes.extend(notes);
    Ok(crate::readiness::evaluate(&detected))
}

#[tauri::command(async)]
fn evaluate_readiness(input: ReadinessInput) -> Result<ReadinessReport, String> {
    let root = PathBuf::from(&input.project_root);
    if root.is_dir() {
        let root = validate_project_root(&root).map_err(command_error)?;
        evaluate_installed_readiness(
            &root,
            &input.project_id,
            input.workflow_3d_state,
            input.notes,
        )
        .map_err(command_error)
    } else {
        Ok(crate::readiness::evaluate(&input))
    }
}

#[tauri::command(async)]
fn preview_descriptors(state: Value) -> Result<Vec<GeneratedArtifact>, String> {
    require_ai_session(&state).map_err(command_error)?;
    let root = project_root_from_state(&state).map_err(command_error)?;
    let _ = codex_analysis_from_state(&state, &root).map_err(command_error)?;
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

#[tauri::command(async)]
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

#[tauri::command(async)]
fn preview_source_manifest(
    source_mode: String,
    pinned_ref: String,
) -> Result<SourceManifestPreview, String> {
    run_blocking_command("source-preview", move || {
        preview_source_manifest_blocking(source_mode, pinned_ref)
    })
}

fn preview_source_manifest_blocking(
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

fn selected_ids(state: &Value, provider: &str) -> Vec<String> {
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
    if provider != "codex" {
        selected.retain(|id| id != "codex.config");
    }
    selected
}

fn reject_codex_only_dependencies(provider: &str, selected: &[String]) -> Result<(), AppError> {
    if provider != "codex"
        && selected.iter().any(|id| {
            matches!(
                id.as_str(),
                "codex.config" | "mcp.hoi4_agent_tools" | "workflow.3d"
            )
        })
    {
        return Err(AppError::InvalidInput(
            "the selected source components require the Codex integration; choose Codex or deselect the Codex-dependent MCP and 3D components".into(),
        ));
    }
    Ok(())
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
                    let platform = component
                        .platforms
                        .iter()
                        .map(|declared| match declared {
                            ManifestPlatform::Windows => Some(Platform::Windows),
                            ManifestPlatform::Macos => Some(Platform::Macos),
                            // An all-platform command is still an action
                            // for the current installation target. The
                            // platform claim comes from the verified
                            // manifest; no alternate route is invented.
                            ManifestPlatform::All => Some(Platform::current()),
                        })
                        .next()
                        .flatten()?;
                    let target = rule.target.as_deref()?;
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
                        verified_executable_sha256: rule
                            .parameters
                            .get("executable_sha256")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        verified_executable_size: rule
                            .parameters
                            .get("executable_size")
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
    validate_project_root_or_destination(&root).map(|(root, _)| root)
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

fn adapt_subagent_for_spawn(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("subagent definition is not UTF-8: {error}")))?;
    let mut value = text.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("subagent definition is not valid TOML: {error}"))
    })?;
    if let Some(fork_context) = value.get("fork_context").and_then(toml::Value::as_bool) {
        if fork_context {
            return Err(AppError::Source(
                "subagent definition requires inherited conversation context".into(),
            ));
        }
        return Ok(bytes.to_vec());
    }
    if text.contains("fork_context=false") {
        return Ok(bytes.to_vec());
    }
    let instructions = value
        .get_mut("developer_instructions")
        .and_then(|instructions| instructions.as_str())
        .ok_or_else(|| {
            AppError::Source("subagent definition has no developer instructions".into())
        })?
        .to_string();
    value["developer_instructions"] = toml::Value::String(format!(
        "{instructions}\n\nThe parent must spawn this subagent with fork_context=false."
    ));
    let rendered = toml::to_string_pretty(&value).map_err(|error| {
        AppError::Source(format!(
            "adapted subagent definition could not be written: {error}"
        ))
    })?;
    rendered.parse::<toml::Value>().map_err(|error| {
        AppError::Source(format!("adapted subagent definition is invalid: {error}"))
    })?;
    Ok(rendered.into_bytes())
}

fn adapt_agents_for_selection(
    bytes: &[u8],
    identity: &ProjectIdentity,
    provider: &str,
    model: &str,
    super_events_selected: bool,
) -> Result<Vec<u8>, AppError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("AGENTS template is not UTF-8: {error}")))?;
    let prefix = identity
        .primary_namespace
        .as_deref()
        .or(identity.script_prefix.as_deref())
        .unwrap_or(&identity.project_id);
    let mut adapted = text
        .replace("[MOD_NAME]", &identity.display_name)
        .replace("[MOD_PREFIX]", prefix);
    adapted = strip_agents_placeholder_guide(&adapted);
    if adapted.contains("[MOD_") || adapted.contains("{{PROJECT_") || adapted.contains("<PROJECT_")
    {
        return Err(AppError::Source(
            "AGENTS template contains unresolved project placeholders".into(),
        ));
    }
    let profile = ai::profile(provider)
        .map(|profile| profile.optimization_profile)
        .unwrap_or_else(|| "provider conventions".into());
    adapted.push_str(&format!(
        "\n\n## Selected AI planning profile\n\n- Provider: `{provider}`\n- Model: `{model}`\n- Optimization profile: {profile}\n- Semantic analysis is advisory; confirm deterministic identifiers, paths, hashes, and file changes before apply.\n"
    ));
    const SUPER_EVENTS_START: &str = "<!-- HOI4_MOD_SETUP:SUPER_EVENTS:START -->";
    const SUPER_EVENTS_END: &str = "<!-- HOI4_MOD_SETUP:SUPER_EVENTS:END -->";
    match (
        adapted.find(SUPER_EVENTS_START),
        adapted.find(SUPER_EVENTS_END),
    ) {
        (Some(start), Some(end)) if start < end => {
            let block_end = end + SUPER_EVENTS_END.len();
            if super_events_selected {
                adapted.replace_range(end..block_end, "");
                adapted.replace_range(start..start + SUPER_EVENTS_START.len(), "");
            } else {
                adapted.replace_range(start..block_end, "");
            }
        }
        (None, None) if super_events_selected => {
            adapted.push_str(
                "\n## Optional Super Events workflow\n\n- Use `hoi4-super-events` for the installed runtime contract, `hoi4-super-events-planning` for the implementation brief, `hoi4-super-events-event-integration` for caller and cleanup wiring, `hoi4-super-events-text-audio-research` and `hoi4-super-events-feature-assets` for sourced presentation assets, and `hoi4-super-events-subagents` for bounded delegation. Keep registration, caller, text, quote, response, image, optional or explicitly required audio, provenance, documentation, cleanup, and the acceptance scenario aligned.\n- Spawn `hoi4_super_event_quote_researcher`, `hoi4_super_event_audio_researcher`, or `hoi4_super_event_art_researcher` with `fork_context=false` only when its narrow selected-only handoff is needed.\n",
            );
        }
        (None, None) => {}
        _ => {
            return Err(AppError::Source(
                "AGENTS template has an incomplete Super Events section marker".into(),
            ));
        }
    }
    Ok(adapted.into_bytes())
}

fn strip_agents_placeholder_guide(text: &str) -> String {
    let heading = Regex::new(r"(?m)^## Placeholder Guide[\t ]*\r?$")
        .expect("static placeholder-guide heading regex");
    let next_heading = Regex::new(r"(?m)^## [^\r\n]+").expect("static Markdown heading regex");
    let mut stripped = text.to_string();
    while let Some(section) = heading.find(&stripped) {
        let section_end = next_heading
            .find(&stripped[section.end()..])
            .map(|next| section.end() + next.start())
            .unwrap_or(stripped.len());
        stripped.replace_range(section.start()..section_end, "");
    }
    stripped
}

fn adapt_super_events_source(
    bytes: &[u8],
    identity: &ProjectIdentity,
) -> Result<Vec<u8>, AppError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Source(format!("Super Events source is not UTF-8: {error}")))?;
    let prefix = identity
        .primary_namespace
        .as_deref()
        .or(identity.script_prefix.as_deref())
        .ok_or_else(|| {
            AppError::InvalidInput(
                "a confirmed script prefix or primary namespace is required for Super Events"
                    .into(),
            )
        })?;
    let valid_prefix =
        Regex::new(r"^[a-z_][a-z0-9_]{0,63}$").expect("static HOI4 identifier regex");
    if !valid_prefix.is_match(prefix) {
        return Err(AppError::InvalidInput(
            "the Super Events namespace contains unsupported characters".into(),
        ));
    }
    let escaped_name = identity
        .display_name
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let adapted = text
        .replace("[MOD_PREFIX]", prefix)
        .replace("[MOD_NAME]", &escaped_name);
    if adapted.contains("[MOD_") {
        return Err(AppError::Source(
            "Super Events source contains unresolved project placeholders".into(),
        ));
    }
    Ok(adapted.into_bytes())
}

fn is_super_events_runtime_component(component_id: &str) -> bool {
    matches!(
        component_id,
        "workflow.super_events.runtime.interface"
            | "workflow.super_events.runtime.common"
            | "workflow.super_events.runtime.events"
            | "workflow.super_events.runtime.localisation"
    )
}

fn adapt_selected_source(
    component_id: &str,
    bytes: &[u8],
    identity: &ProjectIdentity,
    ai_provider: &str,
    ai_model: &str,
    super_events_selected: bool,
    mcp_selected: bool,
) -> Result<Vec<u8>, AppError> {
    if component_id == "core.agents" {
        adapt_agents_for_selection(
            bytes,
            identity,
            ai_provider,
            ai_model,
            super_events_selected,
        )
    } else if component_id == "codex.config" {
        adapt_codex_config_for_selection(bytes, mcp_selected)
    } else if component_id == "core.subagents" || component_id.ends_with(".subagents") {
        adapt_subagent_for_spawn(bytes)
    } else if is_super_events_runtime_component(component_id) {
        adapt_super_events_source(bytes, identity)
    } else {
        Ok(bytes.to_vec())
    }
}

fn maintenance_identity(lock: &InstallationLock, root: &Path) -> ProjectIdentity {
    let descriptor = std::fs::read(root.join("descriptor.mod"))
        .ok()
        .and_then(|bytes| crate::descriptors::parse_descriptor(&bytes).ok())
        .map(|descriptor| descriptor.fields);
    let display_name = descriptor
        .as_ref()
        .and_then(|fields| fields.get("name").cloned())
        .unwrap_or_else(|| lock.project_id.clone());
    ProjectIdentity {
        display_name,
        project_id: lock.project_id.clone(),
        author: String::new(),
        version: "0.1.0".into(),
        supported_game_version: "*".into(),
        project_root: root.to_path_buf(),
        default_branch: "main".into(),
        script_prefix: descriptor
            .as_ref()
            .and_then(|fields| fields.get("script_prefix").cloned()),
        primary_namespace: descriptor
            .as_ref()
            .and_then(|fields| fields.get("namespace").cloned()),
        descriptor_tags: Vec::new(),
        launcher_descriptor_path: None,
    }
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
    if state.get("mode").and_then(Value::as_str) == Some("new") {
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
    crate::codex::validate_folder_profile_paths(&values)
}

fn ai_provider_from_state(state: &Value) -> Result<String, AppError> {
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

fn ai_model_from_state(state: &Value) -> String {
    state
        .get("aiModel")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("default")
        .to_string()
}

fn ai_endpoint_from_state(state: &Value) -> Option<String> {
    state
        .get("aiEndpoint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn ai_optimization_profile(provider: &str) -> String {
    ai::profile(provider)
        .map(|profile| profile.optimization_profile)
        .unwrap_or_else(crate::models::default_ai_optimization_profile)
}

fn flatten_for_chat_from_state(state: &Value) -> bool {
    state
        .get("flattenForChat")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn project_readme(
    identity: &ProjectIdentity,
    description: &str,
    provider: &str,
    model: &str,
) -> Result<String, AppError> {
    if description.len() > 32 * 1024 || description.contains('\0') {
        return Err(AppError::InvalidInput(
            "project description exceeds the bounded README input limit".into(),
        ));
    }
    let optimization_profile = ai_optimization_profile(provider);
    Ok(format!(
        "# {}\n\n{}\n\n## Local development\n\nThis project was prepared by HOI4 Mod Setup for agentic development.\n\n- Project ID: `{}`\n- Supported game version: `{}`\n- Planning provider: `{}`\n- Model: `{}`\n- Optimization profile: {}\n- Workshop identity: none assigned\n",
        identity.display_name,
        description.trim(),
        identity.project_id,
        identity.supported_game_version,
        provider,
        model,
        optimization_profile,
    ))
}

fn codex_analysis_from_state(
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
    let profile = ai::profile(&provider).ok_or_else(|| {
        AppError::Credential("confirmed analysis uses an unsupported AI provider".into())
    })?;
    if provider != "codex"
        && (record.model.as_deref() != Some(ai_model_from_state(state).as_str())
            || record.optimization_profile.as_deref()
                != Some(profile.optimization_profile.as_str()))
    {
        return Err(AppError::Credential(
            "confirmed analysis does not match the selected provider, model, or optimization profile".into(),
        ));
    }
    if provider == "codex"
        && record
            .optimization_profile
            .as_deref()
            .is_some_and(|value| value != profile.optimization_profile)
    {
        return Err(AppError::Credential(
            "confirmed Codex analysis uses a different optimization profile".into(),
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
    if !codex_record_confirmed_in_session_with_endpoint(
        &record,
        Some(project_root),
        expected_scan_id,
        endpoint_fingerprint.as_deref(),
        Some(&expected_confirmation_values_sha256),
    )? {
        return Err(AppError::Credential(
            "selected-provider confirmation is no longer present in the core session; rerun semantic analysis"
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

fn codex_record_confirmed_in_session_with_endpoint(
    record: &CodexAnalysisRecord,
    expected_project_root: Option<&Path>,
    expected_scan_id: Option<Uuid>,
    expected_endpoint_fingerprint: Option<&str>,
    expected_confirmation_values_sha256: Option<&str>,
) -> Result<bool, AppError> {
    let analyses = codex_analyses()
        .lock()
        .map_err(|_| AppError::Process("Codex analysis store is unavailable".into()))?;
    Ok(analyses.get(&record.analysis_id).is_some_and(|pending| {
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
    }))
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
    require_ai_session(state)?;
    let root = project_root_from_state(state)?;
    let identity = project_identity_from_state(state, &root)?;
    let codex_analysis = codex_analysis_from_state(state, &root)?;
    let ai_provider = ai_provider_from_state(state)?;
    let ai_model = ai_model_from_state(state);
    let mesh_selected = state
        .get("meshSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    ai::validate_config(&AiProviderConfig {
        provider: ai_provider.clone(),
        model: ai_model.clone(),
        endpoint: ai_endpoint_from_state(state).unwrap_or_default(),
        credential_reference: if ai_provider == "codex" {
            None
        } else {
            ai_credential_reference(&ai_provider)
        },
    })?;
    let flatten_chat_sources = flatten_for_chat_from_state(state);
    if flatten_chat_sources && ai_provider != "codex" {
        return Err(AppError::InvalidInput(
            "flattened ChatGPT Chat sources are available only when Codex is selected".into(),
        ));
    }
    let git_setup = git_setup_from_state(state, &root)?;
    let request = source_request_from_state(state)?;
    let client = HttpSourceClient::new()?;
    let resolution = resolve_source(&client, &request)?;
    let requested = selected_ids(state, &ai_provider);
    if requested.is_empty() {
        return Err(AppError::InvalidInput(
            "select at least one manifest component".into(),
        ));
    }
    let selected = expand_components(&resolution.manifest, &requested)?;
    reject_codex_only_dependencies(&ai_provider, &selected)?;
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
    let mut download_ledger = Vec::new();
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
        let mut verified = verify_download(
            selection,
            &source_bytes,
            &resolution.identity.resolved_revision,
        )?;
        let bytes = adapt_selected_source(
            &selection.component_id,
            &source_bytes,
            &identity,
            &ai_provider,
            &ai_model,
            super_events_selected,
            mcp_selected,
        )?;
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
                id: format!("conflict-{index}"),
                path: selection.destination.clone(),
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
    if mode == "new" {
        let description = string_field(state, "description").unwrap_or_default();
        let readme = project_readme(
            &identity,
            &description,
            &ai_provider,
            &ai_model_from_state(state),
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
    if flatten_chat_sources
        && !generated
            .iter()
            .any(|artifact| artifact.destination == "README.md")
        && !root.join("README.md").is_file()
    {
        let readme = project_readme(
            &identity,
            &string_field(state, "description").unwrap_or_default(),
            &ai_provider,
            &ai_model_from_state(state),
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
        current_meshy_credential_reference()?
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
        let provider_is_codex = ai_provider == "codex";
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
                "telemetry": false
            },
            "ai": {
                "provider": ai_provider.clone(),
                "model": ai_model_from_state(state),
                "optimization_profile": ai::profile(&ai_provider)
                    .map(|profile| profile.optimization_profile)
                    .unwrap_or_else(|| "provider conventions".into())
            },
            "codex": {
                "integration": if provider_is_codex { "codex_app_server" } else { "provider_api" },
                "auth_mode": if provider_is_codex { "chatgpt" } else if provider_profile.requires_credential { "api_key" } else { "local_endpoint" },
                "auth_status": if provider_is_codex { "signed_in" } else { "configured" },
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
    if flatten_chat_sources {
        let flatten_artifacts = crate::flatten::build_artifacts(&prepared, &generated, &root)?;
        generated.extend(flatten_artifacts);
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
        let had_existing_destination = operation_local_sha.is_some();
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
    if mesh_selected && !credential_references.is_empty() {
        optional_workflows.insert("workflow.3d".into(), "selected_pending".into());
    }
    for item in &support {
        if item.state == "unsupported_platform" {
            optional_workflows.insert(item.component_id.clone(), "unsupported_platform".into());
        }
    }
    optional_workflows.insert(
        "codex.chat_flatten".into(),
        if flatten_chat_sources {
            "selected_pending".into()
        } else {
            "not_selected".into()
        },
    );
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
        ai_optimization_profile: ai_optimization_profile(&ai_provider),
        ai_provider,
        ai_model: ai_model_from_state(state),
        ai_endpoint: ai_endpoint_from_state(state),
        flatten_chat_sources,
        codex_analysis: Some(codex_analysis),
        selected_components: selected,
        wiki_required_pages: resolution.manifest.wiki.required_pages.clone(),
        wiki_metadata: Some(crate::source::wiki_install_metadata(&resolution.manifest)),
        generated_artifacts: generated,
        download_ledger,
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
            backup_root: application_data_root()?
                .join("backups")
                .display()
                .to_string(),
            staging_root: application_data_root()?
                .join("staging")
                .display()
                .to_string(),
            directories: if mode == "new" {
                selected_folder_profile(state)?
            } else {
                Vec::new()
            },
            atomic_apply_expected: true,
            project_root_mode: if root.exists() {
                ProjectRootMode::Existing
            } else {
                ProjectRootMode::CreateLeaf
            },
            project_root_parent: (!root.exists())
                .then(|| root.parent().map(|path| path.display().to_string()))
                .flatten(),
            project_root_leaf: (!root.exists())
                .then(|| {
                    root.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .flatten(),
        },
        approvals: PlanApprovals {
            dry_run_reviewed: false,
            external_actions_reviewed: false,
            git_remote_approved: false,
            push_approved: false,
        },
    };
    if plan.flatten_chat_sources {
        let mut prepared_plan = PreparedPlan {
            plan,
            prepared_files: prepared,
            merge_contexts: HashMap::new(),
            canonical_root: root,
            approved: false,
        };
        refresh_flattened_outputs(&mut prepared_plan)?;
        return Ok((prepared_plan.plan, prepared_plan.prepared_files));
    }
    Ok((plan, prepared))
}

fn is_flattened_destination(path: &str) -> bool {
    path.replace('\\', "/")
        .starts_with(&format!("{}/", crate::flatten::FLAT_DESTINATION_ROOT))
}

fn is_flattened_source_path(path: Option<&str>) -> bool {
    path.is_some_and(|value| {
        value.replace('\\', "/").starts_with(&format!(
            "generated:{}/",
            crate::flatten::FLAT_DESTINATION_ROOT
        ))
    })
}

fn flatten_operation_uses_incoming(operation: &PlanOperation) -> bool {
    if matches!(
        operation.resolution.as_deref(),
        Some(
            "review_required"
                | "reverse_merge_required"
                | "user_owned_review"
                | "keep_user_modification"
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

fn accepted_flatten_prepared_files(
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
            Some("keep" | "keep_user_modification")
        );
        let eligible = file
            .destination
            .replace('\\', "/")
            .starts_with(".agents/skills/")
            || file
                .destination
                .replace('\\', "/")
                .starts_with(".codex/agents/");
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

fn refresh_flattened_outputs(prepared_plan: &mut PreparedPlan) -> Result<(), AppError> {
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

#[tauri::command(async)]
fn build_installation_plan(state: Value) -> Result<InstallationPlan, String> {
    run_blocking_command("installation-plan", move || {
        build_installation_plan_blocking(state)
    })
}

fn build_installation_plan_blocking(state: Value) -> Result<InstallationPlan, String> {
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

fn lock_workflow_selected(lock: &InstallationLock, workflow_id: &str) -> bool {
    lock.optional_workflows
        .get(workflow_id)
        .is_some_and(|workflow| {
            !matches!(
                workflow.state.as_str(),
                "unsupported_platform" | "not_selected" | "removed"
            )
        })
        || lock.components.iter().any(|component| {
            component.id == workflow_id
                && !matches!(
                    component.state.as_str(),
                    "unsupported_platform" | "not_selected" | "removed"
                )
        })
}

fn require_maintenance_reanalysis(
    mode: &str,
    analysis_override: Option<&CodexAnalysisRecord>,
    project_root: &Path,
    expected_endpoint: Option<&str>,
) -> Result<(), AppError> {
    if mode != "update" {
        return Ok(());
    }
    let record = analysis_override.ok_or_else(|| {
        AppError::Credential(
            "update planning requires a fresh, confirmed provider reanalysis of the installed project"
                .into(),
        )
    })?;
    crate::codex::validate_confirmed_record(record)?;
    if record.analysis_purpose.as_deref() != Some("maintenance_reanalysis") {
        return Err(AppError::Credential(
            "update planning requires a maintenance-purpose provider reanalysis".into(),
        ));
    }
    let expected_endpoint_fingerprint = record
        .provider
        .as_deref()
        .filter(|provider| *provider != "codex")
        .map(|_| {
            expected_endpoint
                .ok_or_else(|| {
                    AppError::Credential(
                        "provider reanalysis endpoint is not bound to the installed profile".into(),
                    )
                })
                .map(|endpoint| sha256_bytes(endpoint.trim().as_bytes()))
        })
        .transpose()?;
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
        if expected_endpoint_fingerprint
            .as_deref()
            .is_some_and(|expected| pending.endpoint_fingerprint.as_deref() != Some(expected))
        {
            return Err(AppError::Credential(
                "update reanalysis was confirmed against a different provider endpoint".into(),
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

/// Add files for a newly selected optional component to a repair plan. The
/// predecessor lock remains the source of truth for every already-managed
/// file; only missing component ownership is introduced here. Existing bytes
/// are compared before the operation is selected so a collision becomes an
/// explicit review instead of a silent replacement.
fn append_additional_component_operations(
    operations: &mut Vec<PlanOperation>,
    selections: &[crate::source::SelectedSourceFile],
    lock: &InstallationLock,
    root: &Path,
    refresh_agents: bool,
) -> Result<(), AppError> {
    let managed_components = lock
        .components
        .iter()
        .filter(|component| {
            !matches!(
                component.state.as_str(),
                "removed" | "not_selected" | "unsupported_platform"
            )
        })
        .map(|component| component.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let managed_paths = lock
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.external))
        .collect::<std::collections::HashSet<_>>();
    for (index, selection) in selections.iter().enumerate() {
        let refresh_managed_agents = refresh_agents && selection.component_id == "core.agents";
        if managed_components.contains(selection.component_id.as_str()) && !refresh_managed_agents {
            continue;
        }
        if managed_paths.contains(&(selection.destination.as_str(), false))
            && !refresh_managed_agents
        {
            continue;
        }
        let expected_sha256 = selection.expected_sha256.as_deref().ok_or_else(|| {
            AppError::Source(format!(
                "selected source file lacks SHA-256 evidence: {}",
                selection.source_path
            ))
        })?;
        let destination = safe_join(root, &selection.destination)?;
        let local_sha256 = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        let current_locked_file = refresh_managed_agents
            .then(|| {
                lock.files.iter().find(|file| {
                    file.component_id == "core.agents"
                        && file.path == selection.destination
                        && !file.external
                })
            })
            .flatten();
        let unmodified_sha256 = current_locked_file
            .map(|file| file.installed_sha256.as_str())
            .unwrap_or(expected_sha256);
        let (action, local_state, resolution) = match local_sha256.as_deref() {
            None => (OperationAction::Create, LocalState::Absent, None),
            Some(hash) if hash == unmodified_sha256 => (
                OperationAction::Replace,
                LocalState::Unmodified,
                Some(if refresh_managed_agents {
                    "optional_guidance_refresh".into()
                } else {
                    "new_component_file".into()
                }),
            ),
            Some(_) => (
                OperationAction::Skip,
                LocalState::Modified,
                Some("review_required".into()),
            ),
        };
        operations.push(PlanOperation {
            id: format!("repair-add-{}-{index:05}", selection.component_id),
            component_id: selection.component_id.clone(),
            ownership: Some(selection.ownership),
            location_scope: Some("project".into()),
            action,
            source_path: Some(selection.source_path.clone()),
            destination: selection.destination.clone(),
            source_sha256: Some(expected_sha256.into()),
            source_size: selection.expected_size,
            platform: Some(selection.platform),
            executable: selection.executable,
            result_sha256: None,
            base_sha256: current_locked_file.map(|file| file.source_sha256.clone()),
            local_sha256,
            local_state,
            resolution,
            external: false,
            rollback: if action == OperationAction::Create {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
        });
    }
    Ok(())
}

#[tauri::command(async)]
fn build_maintenance_plan(
    mode: String,
    project_root: String,
    analysis_override: Option<CodexAnalysisRecord>,
    add_optional_components: Option<Vec<String>>,
) -> Result<InstallationPlan, String> {
    run_blocking_command("maintenance-plan", move || {
        build_maintenance_plan_blocking(
            mode,
            project_root,
            analysis_override,
            add_optional_components,
        )
    })
}

fn build_maintenance_plan_blocking(
    mode: String,
    project_root: String,
    analysis_override: Option<CodexAnalysisRecord>,
    add_optional_components: Option<Vec<String>>,
) -> Result<InstallationPlan, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let lock = read_project_lock(&root).map_err(command_error)?;
    let add_optional_components = add_optional_components.unwrap_or_default();
    let mut seen_optional_components = std::collections::HashSet::new();
    if !add_optional_components.iter().all(|id| {
        matches!(id.as_str(), "workflow.3d" | "workflow.super_events")
            && seen_optional_components.insert(id.as_str())
    }) {
        return Err("an optional workflow selection is invalid or duplicated".into());
    }
    if !add_optional_components.is_empty() && !matches!(mode.as_str(), "update" | "repair") {
        return Err("optional workflows can be added only during update or repair".into());
    }
    if mode != "remove" {
        if lock.ai_provider == "codex" {
            require_codex_chatgpt_session().map_err(command_error)?;
        } else {
            let credential_reference = ai_credential_reference(&lock.ai_provider);
            let status = ai::account_status(
                &OsCredentialStore,
                &AiProviderConfig {
                    provider: lock.ai_provider.clone(),
                    model: lock.ai_model.clone(),
                    endpoint: lock.ai_endpoint.clone().unwrap_or_default(),
                    credential_reference,
                },
            );
            if status.error.is_some() || !status.authenticated {
                return Err(format!(
                    "{} is not configured for maintenance planning",
                    lock.ai_provider
                ));
            }
        }
    }
    require_maintenance_reanalysis(
        &mode,
        analysis_override.as_ref(),
        &root,
        lock.ai_endpoint.as_deref(),
    )
    .map_err(command_error)?;
    let codex_analysis = if mode == "remove" {
        None
    } else {
        let record = analysis_override
            .clone()
            .or_else(|| lock.codex_analysis.clone())
            .ok_or_else(|| {
                "the installed project has no confirmed provider analysis; run Import review before maintenance"
                    .to_string()
        })?;
        crate::codex::validate_confirmed_record(&record).map_err(command_error)?;
        let record_provider = record.provider.as_deref().unwrap_or("codex");
        let profile = ai::profile(&lock.ai_provider).ok_or_else(|| {
            format!(
                "installed lock uses an unsupported AI provider: {}",
                lock.ai_provider
            )
        })?;
        if record_provider != lock.ai_provider
            || (lock.ai_provider != "codex"
                && (record.model.as_deref() != Some(lock.ai_model.as_str())
                    || record.optimization_profile.as_deref()
                        != Some(profile.optimization_profile.as_str())))
        {
            return Err("provider reanalysis does not match the installed provider profile".into());
        }
        if analysis_override.is_some()
            && !codex_record_confirmed_in_session_with_endpoint(
                &record,
                Some(&root),
                None,
                (lock.ai_provider != "codex")
                    .then(|| {
                        sha256_bytes(lock.ai_endpoint.as_deref().unwrap_or_default().as_bytes())
                    })
                    .as_deref(),
                None,
            )
            .map_err(command_error)?
        {
            return Err(
                "the provider reanalysis is not confirmed in the current core session; review it again"
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
    let mut wiki_metadata = lock.wiki_metadata.clone();
    let mut maintenance_mcp_manifest: Option<RemoteManifest> = None;
    let (mut operations, mut plan_source, mut source_revision) = if mode == "update" {
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
        let mut requested = lock
            .components
            .iter()
            .filter(|component| {
                component.state != "removed"
                    && manifest_component_ids.contains(&component.id.as_str())
            })
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        for component_id in &add_optional_components {
            if !requested.iter().any(|id| id == component_id) {
                requested.push(component_id.clone());
            }
        }
        let expanded =
            expand_components(&resolution.manifest, &requested).map_err(command_error)?;
        reject_codex_only_dependencies(&lock.ai_provider, &expanded).map_err(command_error)?;
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
                let desired_bytes = adapt_selected_source(
                    &selection.component_id,
                    &source_bytes,
                    &maintenance_identity(&lock, &root),
                    &lock.ai_provider,
                    &lock.ai_model,
                    maintenance_components
                        .iter()
                        .any(|id| id == "workflow.super_events"),
                    mcp_selected,
                )?;
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
                    preserved_local: false,
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
        wiki_metadata = Some(crate::source::wiki_install_metadata(&resolution.manifest));
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
    if mode == "repair" && !add_optional_components.is_empty() {
        let resolution = resolve_source(
            &client,
            &SourceRequest {
                // A repair can add only workflows declared by the same
                // immutable source revision that produced the installed lock.
                // Newly published workflows are added through Update so all
                // managed files still share one exact source revision.
                mode: SourceMode::PinnedCommit,
                requested_ref: Some(lock.source.revision.clone()),
                release: None,
            },
        )
        .map_err(command_error)?;
        if resolution.identity.manifest_sha256 != lock.source.manifest_sha256 {
            return Err(
                "the installed source manifest does not match its immutable revision evidence"
                    .into(),
            );
        }
        let requested = add_optional_components
            .iter()
            .filter(|id| {
                !lock.components.iter().any(|component| {
                    component.id.as_str() == id.as_str()
                        && !matches!(component.state.as_str(), "removed" | "not_selected")
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        if requested.is_empty() {
            return Err("the selected optional workflows are already installed".into());
        }
        let expanded =
            expand_components(&resolution.manifest, &requested).map_err(command_error)?;
        reject_codex_only_dependencies(&lock.ai_provider, &expanded).map_err(command_error)?;
        let support = crate::source::resolve_platform_support(
            &resolution.manifest,
            &expanded,
            Platform::current(),
        )
        .map_err(command_error)?;
        if support.iter().any(|item| item.state == "blocked") {
            return Err("an optional workflow has no verified route on this computer".into());
        }
        let supported = expanded
            .iter()
            .filter(|id| {
                support
                    .iter()
                    .find(|item| item.component_id == **id)
                    .is_some_and(|item| item.state == "supported")
            })
            .cloned()
            .collect::<Vec<_>>();
        let tree = client
            .fetch_tree(&resolution.identity.resolved_revision)
            .map_err(command_error)?;
        let selections = select_component_files(&resolution.manifest, &supported, &tree)
            .map_err(command_error)?;
        append_additional_component_operations(
            &mut operations,
            &selections,
            &lock,
            &root,
            requested.iter().any(|id| id == "workflow.super_events"),
        )
        .map_err(command_error)?;
        for component_id in expanded {
            if !maintenance_components.iter().any(|id| id == &component_id) {
                maintenance_components.push(component_id);
            }
        }
        plan_source.mode = lock.source.mode;
        plan_source.requested_ref = lock.source.requested_ref.clone();
        plan_source.release = lock.source.release.clone();
        source_revision = lock.source.revision.clone();
    }
    let mut prepared = Vec::new();
    let mut download_ledger = Vec::new();
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
        let source_is_generated = operation
            .source_path
            .as_deref()
            .is_some_and(|path| path.starts_with("generated:"));
        let source_bytes = if source_is_generated {
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
                    operation.source_size,
                )
                .map_err(command_error)?
        };
        let source_ledger_entry = if source_is_generated {
            None
        } else {
            let source_path = operation
                .source_path
                .clone()
                .ok_or_else(|| "remote maintenance operation has no source path".to_string())?;
            let ownership = operation.ownership.ok_or_else(|| {
                "remote maintenance operation has no ownership evidence".to_string()
            })?;
            let platform = operation.platform.ok_or_else(|| {
                "remote maintenance operation has no platform evidence".to_string()
            })?;
            if operation.source_size != Some(source_bytes.len() as u64) {
                return Err(format!(
                    "remote maintenance source size mismatch: {}",
                    operation.destination
                ));
            }
            Some(crate::models::DownloadedFile {
                operation_id: operation.id.clone(),
                source_path,
                destination: operation.destination.clone(),
                source_revision: source_revision.clone(),
                manifest_sha256: plan_source.manifest_sha256.clone(),
                sha256: expected.to_string(),
                size: source_bytes.len() as u64,
                component_id: operation.component_id.clone(),
                ownership,
                platform,
                executable: lock
                    .files
                    .iter()
                    .find(|file| {
                        file.path == operation.destination && file.external == operation.external
                    })
                    .is_some_and(|file| file.executable),
            })
        };
        let mcp_selected = lock_mcp_selected(&lock);
        let generated_source = operation
            .source_path
            .as_deref()
            .is_some_and(|path| path.starts_with("generated:"));
        let bytes = if generated_source && operation.component_id != "core.agents" {
            source_bytes
        } else {
            adapt_selected_source(
                &operation.component_id,
                &source_bytes,
                &maintenance_identity(&lock, &root),
                &lock.ai_provider,
                &lock.ai_model,
                maintenance_components
                    .iter()
                    .any(|id| id == "workflow.super_events"),
                mcp_selected,
            )
            .map_err(command_error)?
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
            && !matches!(
                operation.component_id.as_str(),
                "core.agents" | "codex.config"
            )
            && operation.result_sha256.is_none()
        {
            // Provider-adapted AGENTS and MCP-filtered Codex config have source
            // evidence for the downloaded blob and a separate result hash for
            // the selected project output. Other files must still match source
            // evidence.
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
        if let Some(entry) = source_ledger_entry {
            download_ledger.push(entry);
        }
        if operation.local_state == LocalState::Modified
            && !operation
                .source_path
                .as_deref()
                .is_some_and(|path| path.starts_with("generated:"))
        {
            if let Some(current_file) = lock.files.iter().find(|file| {
                file.path == operation.destination && file.external == operation.external
            }) {
                // A merged file's installed bytes include the prior user's
                // accepted contribution. The lock carries its hash, not a
                // reconstructible byte-for-byte ancestor, so a raw source
                // blob is not a valid three-way merge base. Keep the
                // conflict review conservative until a verified installed
                // result is available.
                let merge_base_available = current_file.ownership != Ownership::Merged;
                let kind = path_file_kind(&operation.destination);
                if merge_base_available
                    && matches!(kind, FileKind::Text | FileKind::Toml | FileKind::Json)
                {
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
                    } else if current_file.component_id == "core.agents" {
                        adapt_agents_for_selection(
                            &base_source,
                            &maintenance_identity(&lock, &root),
                            &lock.ai_provider,
                            &lock.ai_model,
                            lock_workflow_selected(&lock, "workflow.super_events"),
                        )
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
    let mut generated_artifacts = Vec::new();
    if lock.flatten_chat_sources && mode != "remove" {
        operations.retain(|operation| {
            !operation
                .destination
                .replace('\\', "/")
                .starts_with("chatgpt_project_sources/")
        });
        prepared.retain(|file| {
            !file
                .destination
                .replace('\\', "/")
                .starts_with("chatgpt_project_sources/")
        });
        let flatten_prepared = accepted_flatten_prepared_files(&prepared, &operations, &root)
            .map_err(command_error)?;
        let flat = crate::flatten::build_artifacts(&flatten_prepared, &[], &root)
            .map_err(command_error)?;
        generated_artifacts = flat.clone();
        for (index, artifact) in flat.iter().enumerate() {
            let destination_path =
                safe_join(&root, &artifact.destination).map_err(command_error)?;
            let local_sha = if destination_path.is_file() {
                Some(sha256_file(&destination_path).map_err(command_error)?)
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
            let operation_id = format!("maintenance-flat-{index:05}");
            operations.push(PlanOperation {
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
            });
            prepared.push(PreparedFile {
                operation_id,
                destination: artifact.destination.clone(),
                bytes: artifact
                    .bytes
                    .clone()
                    .unwrap_or_else(|| artifact.content.as_bytes().to_vec()),
                expected_sha256: artifact.expected_sha256.clone(),
            });
        }
    }
    let conflicts = operations
        .iter()
        .filter(|operation| {
            operation.local_state == LocalState::Modified
                || matches!(
                    operation.resolution.as_deref(),
                    Some(
                        "review_required"
                            | "merged_base_required"
                            | "reverse_merge_required"
                            | "obsolete_review"
                    )
                )
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
    let mut optional_workflows: BTreeMap<String, String> = lock
        .optional_workflows
        .iter()
        .filter(|(id, _)| id.as_str() != "workflow.lora_comfyui_interest")
        .map(|(id, workflow)| (id.clone(), workflow.state.clone()))
        .collect();
    let mut credential_references = Vec::new();
    if add_optional_components.iter().any(|id| id == "workflow.3d") {
        let mesh_reference = current_meshy_credential_reference()
            .map_err(command_error)?
            .or_else(|| {
                lock.optional_workflows
                    .get("workflow.3d")
                    .and_then(|workflow| workflow.credential_reference.clone())
                    .map(|reference| CredentialReference {
                        name: MESHY_ENVIRONMENT_NAME.into(),
                        provider: crate::credentials::provider_name(Platform::current()).into(),
                        reference,
                        provider_id: None,
                    })
            });
        if let Some(reference) = mesh_reference {
            validate_credential_reference(&reference).map_err(command_error)?;
            credential_references.push(reference);
        }
        let previous_state = lock
            .optional_workflows
            .get("workflow.3d")
            .map(|workflow| workflow.state.as_str());
        let next_state = if credential_references.is_empty() {
            "incomplete"
        } else if previous_state == Some("ready") {
            "ready"
        } else {
            "selected_pending"
        };
        optional_workflows.insert("workflow.3d".into(), next_state.into());
    }
    if add_optional_components
        .iter()
        .any(|id| id == "workflow.super_events")
    {
        optional_workflows.insert("workflow.super_events".into(), "ready".into());
    }
    let external_actions = if mode != "remove" && lock_mcp_selected(&lock) {
        let manifest = maintenance_mcp_manifest.as_ref().ok_or_else(|| {
            "the maintenance plan could not retain the locked MCP manifest evidence".to_string()
        })?;
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
        ai_provider: lock.ai_provider.clone(),
        ai_model: lock.ai_model.clone(),
        ai_endpoint: lock.ai_endpoint.clone(),
        ai_optimization_profile: lock.ai_optimization_profile.clone(),
        flatten_chat_sources: lock.flatten_chat_sources,
        codex_analysis,
        selected_components: maintenance_components,
        wiki_required_pages,
        wiki_metadata,
        generated_artifacts,
        download_ledger,
        git_setup: None,
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
                .map_err(command_error)?
                .join("backups")
                .display()
                .to_string(),
            staging_root: application_data_root()
                .map_err(command_error)?
                .join("staging")
                .display()
                .to_string(),
            directories: Vec::new(),
            atomic_apply_expected: true,
            project_root_mode: ProjectRootMode::Existing,
            project_root_parent: None,
            project_root_leaf: None,
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

#[tauri::command(async)]
fn git_online_prepare(
    project_root: String,
    action: crate::git::OnlineGitAction,
    remote_name: String,
    repository: String,
    branch: String,
) -> Result<crate::git::GitOnlinePlan, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    crate::git::prepare_online_action(&root, action, &remote_name, &repository, &branch)
        .map_err(command_error)
}

#[tauri::command(async)]
fn git_online_action(
    project_root: String,
    plan_id: String,
    confirmed: bool,
    transaction_id: Option<String>,
) -> Result<crate::git::GitOnlineResult, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let plan_id =
        Uuid::parse_str(&plan_id).map_err(|_| "invalid online Git review ID".to_string())?;
    let result =
        crate::git::execute_online_action(&root, plan_id, confirmed).map_err(command_error)?;
    crate::git::write_online_action_record(&root, transaction_id.as_deref(), &result).map_err(
        |error| {
            format!(
                "online Git completed, but its local recovery record could not be saved: {error}"
            )
        },
    )?;
    Ok(result)
}

#[tauri::command(async)]
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

#[tauri::command(async)]
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
        refresh_flattened_outputs(prepared).map_err(command_error)?;
    }
    Ok(prepared.plan.clone())
}

#[tauri::command(async)]
fn apply_installation(plan_id: String, project_root: String) -> Result<TransactionJournal, String> {
    let id = Uuid::parse_str(&plan_id).map_err(|_| "invalid installation plan ID".to_string())?;
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
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

#[tauri::command(async)]
fn rollback_installation(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    let app_root = application_data_root().map_err(command_error)?;
    let journal_path = crate::paths::transaction_root(&app_root, id)
        .transaction
        .join("journal.json");
    let mut journal = crate::transaction::read_journal(&journal_path).map_err(command_error)?;
    crate::transaction::rollback_transaction(&root, &mut journal, &journal_path)
        .map_err(command_error)?;
    Ok(journal)
}

#[tauri::command(async)]
fn read_transaction_journal(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    let app_root = application_data_root().map_err(command_error)?;
    let journal_path = crate::paths::transaction_root(&app_root, id)
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

#[tauri::command(async)]
fn find_interrupted_transaction(
    project_root: String,
) -> Result<Option<TransactionJournal>, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let app_root = application_data_root().map_err(command_error)?;
    crate::transaction::find_incomplete_transaction(&app_root, &root).map_err(command_error)
}

#[tauri::command(async)]
fn resume_installation(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    let app_root = application_data_root().map_err(command_error)?;
    let (journal, _) = resume_transaction(&root, &app_root, id).map_err(command_error)?;
    Ok(journal)
}

#[tauri::command(async)]
fn discard_installation_staging(
    project_root: String,
    transaction_id: String,
) -> Result<TransactionJournal, String> {
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let id = Uuid::parse_str(&transaction_id).map_err(|_| "invalid transaction ID".to_string())?;
    let app_root = application_data_root().map_err(command_error)?;
    discard_staging(&root, &app_root, id).map_err(command_error)
}

#[tauri::command(async)]
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
    let readiness =
        evaluate_installed_readiness(&root, &lock.project_id, workflow_3d_state, Vec::new())
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
        executable_sha256: Some(sha256_file(&executable).map_err(command_error)?),
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
    use std::cell::Cell;
    use std::sync::MutexGuard;
    use tempfile::tempdir;

    static COMMAND_TEST_STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_state_guard() -> MutexGuard<'static, ()> {
        COMMAND_TEST_STATE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn supervisor_restarts_after_interruption_and_reinitializes_after_malformed_jsonl() {
        #[derive(Debug)]
        struct FakeSession {
            alive: bool,
            initialized: bool,
        }

        let starts = Cell::new(0_u8);
        let clears = Cell::new(0_u8);
        let mut session = Some(FakeSession {
            alive: false,
            initialized: true,
        });

        let malformed = with_supervised_codex_session(
            &mut session,
            || {
                starts.set(starts.get() + 1);
                Ok(FakeSession {
                    alive: true,
                    initialized: true,
                })
            },
            |session| session.alive,
            |session| {
                assert!(session.initialized);
                Err::<(), _>(AppError::Serialization("malformed JSONL".into()))
            },
            || {
                clears.set(clears.get() + 1);
                Ok(())
            },
        );

        assert!(matches!(malformed, Err(AppError::Serialization(_))));
        assert!(session.is_none());
        assert_eq!(starts.get(), 1);
        assert_eq!(clears.get(), 2);

        let recovered = with_supervised_codex_session(
            &mut session,
            || {
                starts.set(starts.get() + 1);
                Ok(FakeSession {
                    alive: true,
                    initialized: true,
                })
            },
            |session| session.alive,
            |session| Ok(session.initialized),
            || {
                clears.set(clears.get() + 1);
                Ok(())
            },
        )
        .unwrap();

        assert!(recovered);
        assert_eq!(starts.get(), 2);
        assert_eq!(clears.get(), 2);
        assert!(session.as_ref().is_some_and(|value| value.initialized));
    }

    #[test]
    fn every_desktop_command_uses_the_async_dispatcher() {
        let source = include_str!("commands.rs");
        let synchronous_attribute = ["#[tauri::", "command]"].concat();
        assert!(
            !source.contains(&synchronous_attribute),
            "a synchronous Tauri command can freeze the desktop event loop"
        );
        assert!(source.contains("#[tauri::command(async)]"));
    }

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
    fn invalid_provider_state_fails_closed_instead_of_falling_back_to_codex() {
        let error = ai_provider_from_state(&serde_json::json!({
            "aiProvider": "unlisted-provider"
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unsupported AI provider"));

        let error = ai_provider_from_state(&serde_json::json!({
            "aiProvider": ""
        }))
        .unwrap_err();
        assert!(error.to_string().contains("cannot be empty"));
    }

    #[test]
    fn explicitly_empty_model_is_not_replaced_with_a_default_model() {
        assert_eq!(ai_model_from_state(&serde_json::json!({"aiModel": ""})), "");
        assert_eq!(ai_model_from_state(&serde_json::json!({})), "default");
    }

    #[test]
    fn generated_readme_records_the_selected_provider_model_and_profile() {
        let readme = project_readme(
            &ProjectIdentity {
                display_name: "Example Mod".into(),
                project_id: "example_mod".into(),
                author: String::new(),
                version: "0.1.0".into(),
                supported_game_version: "1.17.*".into(),
                project_root: PathBuf::from("C:/mods/example_mod"),
                default_branch: "main".into(),
                script_prefix: None,
                primary_namespace: None,
                descriptor_tags: Vec::new(),
                launcher_descriptor_path: None,
            },
            "A provider-profiled project.",
            "claude",
            "claude-sonnet",
        )
        .unwrap();

        assert!(readme.contains("Planning provider: `claude`"));
        assert!(readme.contains("Model: `claude-sonnet`"));
        assert!(readme.contains("Optimization profile: Claude Code / Anthropic conventions"));
    }

    #[test]
    fn super_events_selection_is_recorded_and_only_adds_guidance_when_selected() {
        let selected_state = generated_state(&serde_json::json!({
            "superEventsSelected": true
        }));
        let unselected_state = generated_state(&serde_json::json!({
            "superEventsSelected": false
        }));
        assert_eq!(
            selected_state
                .get("workflow.super_events")
                .map(String::as_str),
            Some("ready")
        );
        assert_eq!(
            unselected_state
                .get("workflow.super_events")
                .map(String::as_str),
            Some("not_selected")
        );

        let identity = ProjectIdentity {
            display_name: "Example Mod".into(),
            project_id: "example_mod".into(),
            author: String::new(),
            version: "0.1.0".into(),
            supported_game_version: "1.17.*".into(),
            project_root: PathBuf::from("C:/mods/example_mod"),
            default_branch: "main".into(),
            script_prefix: Some("example".into()),
            primary_namespace: Some("example".into()),
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: None,
        };
        let template = b"# [MOD_NAME]\n\nUse `[MOD_PREFIX]` for identifiers.\n\
<!-- HOI4_MOD_SETUP:SUPER_EVENTS:START -->\n\
- Optional Super Events workflow\n\
<!-- HOI4_MOD_SETUP:SUPER_EVENTS:END -->\n";
        let selected =
            adapt_agents_for_selection(template, &identity, "codex", "default", true).unwrap();
        let unselected =
            adapt_agents_for_selection(template, &identity, "codex", "default", false).unwrap();
        assert!(String::from_utf8(selected)
            .unwrap()
            .contains("Optional Super Events workflow"));
        assert!(!String::from_utf8(unselected)
            .unwrap()
            .contains("Optional Super Events workflow"));
    }

    #[test]
    fn adapted_agents_omits_the_template_only_placeholder_guide() {
        let identity = ProjectIdentity {
            display_name: "Cold War: Southeast Asia".into(),
            project_id: "cwsea".into(),
            author: String::new(),
            version: "0.1.0".into(),
            supported_game_version: "1.17.*".into(),
            project_root: PathBuf::from("C:/mods/cwsea"),
            default_branch: "main".into(),
            script_prefix: Some("cwsea".into()),
            primary_namespace: Some("cwsea".into()),
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: None,
        };
        let template = b"# [MOD_NAME]\r\n\r\n\
## Placeholder Guide\r\n\r\n\
Use this guide once before turning the template into a real `AGENTS.md` file.\r\n\r\n\
- `[MOD_NAME]`: full mod name used in prose.\r\n\
- `[MOD_PREFIX]`: short script prefix.\r\n\r\n\
---\r\n\r\n\
## 0. Required Reading Before Any Change\r\n\r\n\
Use `[MOD_PREFIX]` for identifiers.\r\n";

        let adapted =
            adapt_agents_for_selection(template, &identity, "codex", "default", false).unwrap();
        let text = String::from_utf8(adapted).unwrap();
        assert!(text.starts_with("# Cold War: Southeast Asia"));
        assert!(text.contains("## 0. Required Reading Before Any Change"));
        assert!(text.contains("Use `cwsea` for identifiers."));
        assert!(!text.contains("Placeholder Guide"));
        assert!(!text.contains("Use this guide once"));
        assert!(!text.contains("full mod name used in prose"));
    }

    #[test]
    fn super_events_runtime_is_namespace_adapted_without_unresolved_tokens() {
        let identity = ProjectIdentity {
            display_name: "Example \"Mod\"".into(),
            project_id: "example_mod".into(),
            author: String::new(),
            version: "0.1.0".into(),
            supported_game_version: "1.17.*".into(),
            project_root: PathBuf::from("C:/mods/example_mod"),
            default_branch: "main".into(),
            script_prefix: Some("example".into()),
            primary_namespace: Some("example".into()),
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: None,
        };
        let adapted = adapt_super_events_source(
            b"name = [MOD_PREFIX]_super_events\ntext = \"[MOD_NAME]\"\n",
            &identity,
        )
        .unwrap();
        let text = String::from_utf8(adapted).unwrap();
        assert!(text.contains("name = example_super_events"));
        assert!(text.contains("text = \"Example \\\"Mod\\\"\""));
        assert!(!text.contains("[MOD_"));
    }

    #[test]
    fn selected_subagents_explicitly_require_fork_context_false() {
        let source = br#"name = "worker"
description = "A bounded worker"
developer_instructions = """
Work only on the files named by the parent.
"""
"#;
        let adapted = adapt_subagent_for_spawn(source).unwrap();
        let text = String::from_utf8(adapted).unwrap();
        let value = text.parse::<toml::Value>().unwrap();
        assert!(value
            .get("developer_instructions")
            .and_then(toml::Value::as_str)
            .unwrap()
            .contains("fork_context=false"));

        let unsafe_source = br#"name = "worker"
description = "An unsafe worker"
fork_context = true
developer_instructions = "Work on the named files."
"#;
        assert!(adapt_subagent_for_spawn(unsafe_source).is_err());
    }

    #[test]
    fn non_codex_provider_rejects_a_dependency_that_expands_to_codex_config() {
        let error = reject_codex_only_dependencies(
            "claude",
            &["core.agents".into(), "codex.config".into()],
        )
        .unwrap_err();
        assert!(error.to_string().contains("require the Codex integration"));
        assert!(reject_codex_only_dependencies("claude", &["core.agents".into()]).is_ok());
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
    fn external_link_opener_accepts_only_product_reviewed_urls() {
        assert!(is_allowed_external_url(
            "https://github.com/klimPaskov/Agentic-HOI4-Modding"
        ));
        assert!(is_allowed_external_url(
            "https://github.com/klimPaskov/comfyui-hoi4-portraits"
        ));
        assert!(is_allowed_external_url(
            "https://platform.claude.com/settings/keys"
        ));
        assert!(is_allowed_external_url(
            "https://platform.kimi.ai/console/api-keys"
        ));
        assert!(is_allowed_external_url(
            "https://bigmodel.cn/usercenter/proj-mgmt/apikeys"
        ));
        assert!(is_allowed_external_url(
            "https://platform.deepseek.com/api_keys"
        ));
        assert!(!is_allowed_external_url(
            "https://github.com/klimPaskov/Agentic-HOI4-Modding/issues"
        ));
        assert!(!is_allowed_external_url("javascript:alert(1)"));
    }

    #[test]
    fn codex_process_details_are_not_exposed_to_the_renderer() {
        assert_eq!(
            codex_user_error(AppError::Process(
                "official Codex executable was not found on the reviewed PATH".into()
            )),
            "Codex is not installed or could not be found. Install or update Codex, then choose Check again."
        );
        assert_eq!(
            codex_user_error(AppError::Process(
                "Codex App Server closed during account/read".into()
            )),
            "Codex is temporarily unavailable. Close and reopen Codex, then try again."
        );
        assert_eq!(
            codex_user_error(AppError::Process(
                "Codex App Server turn/start failed (-32600): Invalid request: readOnly.access is not allowed".into()
            )),
            "Codex is temporarily unavailable. Close and reopen Codex, then try again."
        );
    }

    #[test]
    fn codex_analysis_error_categories_remain_actionable_and_sanitized() {
        assert_eq!(
            codex_user_error(AppError::Process(
                "Codex usage state could not be checked".into()
            )),
            "Codex usage could not be checked. Choose Check again to retry."
        );
        assert_eq!(
            codex_user_error(AppError::Credential(
                "Codex usage is currently limited; retry later".into()
            )),
            "Codex usage is currently limited. Your draft is unchanged; try again when usage is available."
        );
        assert_eq!(
            codex_user_error(AppError::Credential(
                "Codex evidence contains credential-shaped content sk-live-secret".into()
            )),
            "Codex blocked private-looking information. Remove secrets from the description or selected evidence, then try again."
        );
        assert_eq!(
            codex_user_error(AppError::Credential(
                "sign in with ChatGPT through the official Codex App Server before planning".into()
            )),
            "Sign in with ChatGPT before continuing."
        );
    }

    #[test]
    fn automatic_project_paths_reject_an_invalid_project_id_before_platform_lookup() {
        let error = suggest_project_paths("../outside".into()).unwrap_err();
        assert!(error.contains("project ID"));
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
    fn login_cancel_command_targets_only_the_requested_attempt() {
        let _state_guard = test_state_guard();
        if let Ok(mut session) = codex_session().lock() {
            *session = None;
        }
        let first = Arc::new(AtomicBool::new(false));
        let second = Arc::new(AtomicBool::new(false));
        {
            let mut cancellations = codex_login_cancellations().lock().unwrap();
            cancellations.clear();
            cancellations.insert("login-1".into(), first.clone());
            cancellations.insert("login-2".into(), second.clone());
        }

        codex_login_cancel("login-1".into()).unwrap();

        assert!(first.load(Ordering::SeqCst));
        assert!(!second.load(Ordering::SeqCst));
        let cancellations = codex_login_cancellations().lock().unwrap();
        assert!(!cancellations.contains_key("login-1"));
        assert!(cancellations.contains_key("login-2"));
        drop(cancellations);
        codex_login_cancellations().lock().unwrap().clear();
    }

    #[test]
    fn scan_request_ids_are_bounded_and_cancellation_sets_the_active_flag() {
        let _state_guard = test_state_guard();
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
        let _state_guard = test_state_guard();
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(PathBuf::from("C:/mods/example")),
            scan_id: Some(Uuid::new_v4()),
            entries: HashMap::from([("finding".into(), Vec::new())]),
            evidence_sha256: None,
        };

        clear_approved_scan_evidence().unwrap();

        let evidence = codex_approved_evidence().lock().unwrap();
        assert!(evidence.project_root.is_none());
        assert!(evidence.scan_id.is_none());
        assert!(evidence.entries.is_empty());
    }

    #[test]
    fn renderer_cannot_authorize_an_arbitrary_scan_excerpt() {
        let _state_guard = test_state_guard();
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        let scan_id = Uuid::new_v4();
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(root.clone()),
            scan_id: Some(scan_id),
            entries: HashMap::from([(
                "finding".into(),
                vec![("descriptor.mod".into(), sha256_bytes(b"core excerpt"))],
            )]),
            evidence_sha256: None,
        };

        let error = approve_scan_evidence(
            root.display().to_string(),
            scan_id.to_string(),
            vec![ApprovedEvidence {
                reference: "finding".into(),
                path: "descriptor.mod".into(),
                excerpt: "renderer-authored excerpt".into(),
                excerpt_sha256: sha256_bytes(b"renderer-authored excerpt"),
                confidence: Some(1.0),
            }],
        )
        .unwrap_err();
        assert!(error.contains("exact core-scanned value"));
        clear_approved_scan_evidence().unwrap();
    }

    #[test]
    fn semantic_analysis_requires_the_exact_explicitly_approved_evidence_set() {
        let _state_guard = test_state_guard();
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        let scan_id = Uuid::new_v4();
        let evidence = vec![ApprovedEvidence {
            reference: "finding".into(),
            path: "descriptor.mod".into(),
            excerpt: "core excerpt".into(),
            excerpt_sha256: sha256_bytes(b"core excerpt"),
            confidence: Some(1.0),
        }];
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(root.clone()),
            scan_id: Some(scan_id),
            entries: HashMap::from([(
                "finding".into(),
                vec![("descriptor.mod".into(), sha256_bytes(b"core excerpt"))],
            )]),
            evidence_sha256: None,
        };
        let request = CodexAnalysisRequest {
            mode: "existing_project_semantics".into(),
            brief: String::new(),
            evidence: evidence.clone(),
            constraints: serde_json::json!({}),
            analysis_purpose: Some("existing_project_import".into()),
            project_root: Some(root.display().to_string()),
            scan_id: Some(scan_id),
        };
        assert!(validate_codex_evidence_approval(&request).is_err());

        approve_scan_evidence(root.display().to_string(), scan_id.to_string(), evidence).unwrap();
        assert!(validate_codex_evidence_approval(&request).is_ok());
        clear_approved_scan_evidence().unwrap();
    }

    #[test]
    fn confirmation_digest_covers_rendered_identity_description_and_folders() {
        let base = serde_json::json!({
            "description": "A focused HOI4 overhaul",
            "folderProfile": ["common", "events"],
            "identity": {
                "displayName": "Example Mod",
                "projectId": "example_mod",
                "author": "Author",
                "version": "0.1.0",
                "supportedGameVersion": "1.17.*",
                "projectRoot": "C:/mods/example_mod",
                "defaultBranch": "main",
                "scriptPrefix": "example",
                "primaryNamespace": "example",
                "descriptorTags": ["example"],
                "launcherDescriptorPath": "C:/mods/example.mod"
            }
        });
        let mut changed = base.clone();
        changed["description"] = serde_json::json!("A different description");
        assert_ne!(
            confirmation_values_sha256(&base).unwrap(),
            confirmation_values_sha256(&changed).unwrap()
        );
        let mut unsafe_folder = base;
        unsafe_folder["folderProfile"] = serde_json::json!([".codex"]);
        assert!(confirmation_values_sha256(&unsafe_folder).is_err());
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
    fn update_maintenance_requires_a_fresh_reanalysis_record() {
        let project = tempfile::tempdir().unwrap();
        let error =
            require_maintenance_reanalysis("update", None, project.path(), None).unwrap_err();
        assert!(error
            .to_string()
            .contains("fresh, confirmed provider reanalysis"));
        assert!(require_maintenance_reanalysis("repair", None, project.path(), None).is_ok());
        assert!(require_maintenance_reanalysis(
            "update",
            Some(&CodexAnalysisRecord {
                engine: "codex_app_server".into(),
                auth_mode: "chatgpt".into(),
                provider: Some("codex".into()),
                model: None,
                optimization_profile: Some("Codex project and ChatGPT Chat".into()),
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
            None,
        )
        .is_err());
    }

    #[test]
    fn adding_a_later_three_d_workflow_preserves_a_collision_for_review() {
        let project = tempfile::tempdir().unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.components
            .iter_mut()
            .find(|component| component.id == "workflow.3d")
            .unwrap()
            .state = "not_selected".into();
        lock.optional_workflows
            .get_mut("workflow.3d")
            .unwrap()
            .state = "not_selected".into();
        std::fs::create_dir_all(project.path().join("tools")).unwrap();
        std::fs::write(project.path().join("tools").join("mesh.py"), "local").unwrap();

        let mut operations = Vec::new();
        let selected = crate::source::SelectedSourceFile {
            component_id: "workflow.3d".into(),
            source_path: "workflow/mesh.py".into(),
            destination: "tools/mesh.py".into(),
            ownership: Ownership::Managed,
            expected_sha256: Some(sha256_bytes(b"incoming")),
            expected_size: Some(8),
            executable: false,
            platform: ManifestPlatform::All,
        };

        append_additional_component_operations(
            &mut operations,
            &[selected],
            &lock,
            project.path(),
            false,
        )
        .unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].action, OperationAction::Skip);
        assert_eq!(operations[0].resolution.as_deref(), Some("review_required"));
        assert_eq!(operations[0].local_state, LocalState::Modified);
    }

    #[test]
    fn adding_super_events_later_creates_the_skill_and_refreshes_unmodified_agents() {
        let project = tempfile::tempdir().unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.components
            .retain(|component| component.id != "workflow.super_events");
        lock.files
            .retain(|file| file.component_id != "workflow.super_events");
        lock.optional_workflows.remove("workflow.super_events");
        let installed_agents = b"# Existing project guidance\n";
        std::fs::write(project.path().join("AGENTS.md"), installed_agents).unwrap();
        let agents_lock = lock
            .files
            .iter_mut()
            .find(|file| file.path == "AGENTS.md")
            .unwrap();
        agents_lock.installed_sha256 = sha256_bytes(installed_agents);
        agents_lock.source_sha256 = sha256_bytes(b"# Old source template\n");

        let selections = vec![
            crate::source::SelectedSourceFile {
                component_id: "core.agents".into(),
                source_path: "AGENTS_template.md".into(),
                destination: "AGENTS.md".into(),
                ownership: Ownership::Merged,
                expected_sha256: Some(sha256_bytes(b"# Incoming adapted guidance\n")),
                expected_size: Some(28),
                executable: false,
                platform: ManifestPlatform::All,
            },
            crate::source::SelectedSourceFile {
                component_id: "workflow.super_events".into(),
                source_path: ".agents/skills/hoi4-super-events/SKILL.md".into(),
                destination: ".agents/skills/hoi4-super-events/SKILL.md".into(),
                ownership: Ownership::Managed,
                expected_sha256: Some(sha256_bytes(b"super event skill")),
                expected_size: Some(17),
                executable: false,
                platform: ManifestPlatform::All,
            },
        ];

        let mut operations = Vec::new();
        append_additional_component_operations(
            &mut operations,
            &selections,
            &lock,
            project.path(),
            true,
        )
        .unwrap();

        assert_eq!(operations.len(), 2);
        let agents = operations
            .iter()
            .find(|operation| operation.destination == "AGENTS.md")
            .unwrap();
        assert_eq!(agents.action, OperationAction::Replace);
        assert_eq!(agents.local_state, LocalState::Unmodified);
        assert_eq!(
            agents.resolution.as_deref(),
            Some("optional_guidance_refresh")
        );
        let skill = operations
            .iter()
            .find(|operation| operation.component_id == "workflow.super_events")
            .unwrap();
        assert_eq!(skill.action, OperationAction::Create);
        assert_eq!(skill.local_state, LocalState::Absent);
    }

    #[test]
    fn update_reanalysis_is_bound_to_the_latest_scan_context() {
        let _state_guard = test_state_guard();
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
            evidence_sha256: None,
        };
        let analysis_id = Uuid::new_v4();
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: None,
            optimization_profile: Some("Codex project and ChatGPT Chat".into()),
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
                endpoint_fingerprint: None,
                confirmed_values_sha256: None,
            },
        );
        assert!(require_maintenance_reanalysis("update", Some(&record), &root, None).is_ok());
        let mut stale = record;
        stale.evidence_sha256 = Some("d".repeat(64));
        assert!(require_maintenance_reanalysis("update", Some(&stale), &root, None).is_err());
        codex_analyses().lock().unwrap().clear();
        codex_approved_evidence().lock().unwrap().entries.clear();
    }

    #[test]
    fn non_codex_confirmation_rejects_a_changed_endpoint() {
        let _state_guard = test_state_guard();
        let analysis_id = Uuid::new_v4();
        let record = CodexAnalysisRecord {
            engine: "provider_api".into(),
            auth_mode: "api_key".into(),
            provider: Some("claude".into()),
            model: Some("claude-model".into()),
            optimization_profile: Some("Claude Code / Anthropic conventions".into()),
            analysis_id,
            schema_version: "1.0.0".into(),
            input_sha256: "a".repeat(64),
            output_sha256: "b".repeat(64),
            confirmed_fields: crate::codex::REQUIRED_ANALYSIS_PROPOSAL_KEYS
                .iter()
                .map(|field| (*field).into())
                .collect(),
            confirmed_at: "2026-07-28T00:00:00Z".into(),
            account_identity_persisted: false,
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
            evidence_sha256: None,
        };
        let endpoint_a = "https://provider.example/a";
        let endpoint_b = "https://provider.example/b";
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
                record: record.clone(),
                confirmed: Some(record.clone()),
                project_root: None,
                scan_id: None,
                endpoint_fingerprint: Some(sha256_bytes(endpoint_a.as_bytes())),
                confirmed_values_sha256: None,
            },
        );
        assert!(codex_record_confirmed_in_session_with_endpoint(
            &record,
            None,
            None,
            Some(&sha256_bytes(endpoint_a.as_bytes())),
            None,
        )
        .unwrap());
        assert!(!codex_record_confirmed_in_session_with_endpoint(
            &record,
            None,
            None,
            Some(&sha256_bytes(endpoint_b.as_bytes())),
            None,
        )
        .unwrap());
        codex_analyses().lock().unwrap().remove(&analysis_id);
    }

    #[test]
    fn refresh_keeps_a_reviewed_flatten_conflict_when_non_flat_content_changes() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".agents/skills/one")).unwrap();
        std::fs::create_dir_all(root.join(".codex/agents")).unwrap();
        std::fs::create_dir_all(root.join("chatgpt_project_sources")).unwrap();
        std::fs::write(root.join(".agents/skills/one/SKILL.md"), "local skill").unwrap();
        std::fs::write(root.join(".codex/agents/worker.toml"), "local agent").unwrap();
        std::fs::write(root.join("AGENTS.md"), "local agents").unwrap();
        std::fs::write(root.join("README.md"), "local readme").unwrap();
        std::fs::write(
            root.join("chatgpt_project_sources/AGENTS.md"),
            "older flat agents",
        )
        .unwrap();

        let digest = |value: &str| sha256_bytes(value.as_bytes());
        let operation = |id: &str,
                         component_id: &str,
                         source_path: Option<&str>,
                         destination: &str,
                         source: &str,
                         local: &str| PlanOperation {
            id: id.into(),
            component_id: component_id.into(),
            ownership: Some(Ownership::Generated),
            location_scope: Some("project".into()),
            action: OperationAction::Skip,
            source_path: source_path.map(ToOwned::to_owned),
            destination: destination.into(),
            source_sha256: Some(digest(source)),
            source_size: Some(source.len() as u64),
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: None,
            base_sha256: None,
            local_sha256: Some(digest(local)),
            local_state: LocalState::Modified,
            resolution: Some("keep".into()),
            external: false,
            rollback: RollbackAction::RestoreBackup,
        };
        let prepared_file = |operation_id: &str, destination: &str, content: &str| PreparedFile {
            operation_id: operation_id.into(),
            destination: destination.into(),
            bytes: content.as_bytes().to_vec(),
            expected_sha256: digest(content),
        };

        let agents = operation(
            "generated-agents",
            "core.agents",
            Some("generated:AGENTS.md"),
            "AGENTS.md",
            "incoming agents",
            "local agents",
        );
        let readme = operation(
            "generated-readme",
            "project.readme",
            Some("generated:README.md"),
            "README.md",
            "incoming readme",
            "local readme",
        );
        let skill = operation(
            "source-skill",
            "core.skills",
            Some(".agents/skills/one/SKILL.md"),
            ".agents/skills/one/SKILL.md",
            "incoming skill",
            "local skill",
        );
        let subagent = operation(
            "source-subagent",
            "core.subagents",
            Some(".codex/agents/worker.toml"),
            ".codex/agents/worker.toml",
            "incoming agent",
            "local agent",
        );
        let mut prepared_plan = PreparedPlan {
            plan: InstallationPlan {
                schema_version: "1.0.0".into(),
                plan_id: Uuid::new_v4(),
                project_id: "example".into(),
                created_at: None,
                maintenance_mode: None,
                source: SourceIdentity {
                    repository: "github:example/source".into(),
                    mode: SourceMode::Latest,
                    resolved_revision: "a".repeat(40),
                    requested_ref: None,
                    release: None,
                    manifest_sha256: "b".repeat(64),
                    manifest_origin: "remote".into(),
                },
                ai_provider: "codex".into(),
                ai_model: "default".into(),
                ai_endpoint: None,
                ai_optimization_profile: "Codex project and ChatGPT Chat".into(),
                flatten_chat_sources: true,
                codex_analysis: None,
                selected_components: vec![
                    "core.agents".into(),
                    "core.skills".into(),
                    "core.subagents".into(),
                ],
                wiki_required_pages: Vec::new(),
                wiki_metadata: None,
                generated_artifacts: vec![
                    GeneratedArtifact {
                        component_id: "core.agents".into(),
                        destination: "AGENTS.md".into(),
                        content: "incoming agents".into(),
                        expected_sha256: digest("incoming agents"),
                        external: false,
                        bytes: None,
                    },
                    GeneratedArtifact {
                        component_id: "project.readme".into(),
                        destination: "README.md".into(),
                        content: "incoming readme".into(),
                        expected_sha256: digest("incoming readme"),
                        external: false,
                        bytes: None,
                    },
                ],
                download_ledger: Vec::new(),
                git_setup: None,
                credential_references: Vec::new(),
                optional_workflows: BTreeMap::new(),
                operations: vec![agents, readme, skill, subagent],
                conflicts: vec![
                    PlanConflict {
                        id: "conflict-agents".into(),
                        path: "AGENTS.md".into(),
                        options: vec!["keep".into(), "replace".into()],
                        selected: Some("keep".into()),
                        apply_to_identical: false,
                    },
                    PlanConflict {
                        id: "conflict-readme".into(),
                        path: "README.md".into(),
                        options: vec!["keep".into(), "replace".into()],
                        selected: Some("keep".into()),
                        apply_to_identical: false,
                    },
                    PlanConflict {
                        id: "conflict-skill".into(),
                        path: ".agents/skills/one/SKILL.md".into(),
                        options: vec!["keep".into(), "replace".into()],
                        selected: Some("keep".into()),
                        apply_to_identical: false,
                    },
                    PlanConflict {
                        id: "conflict-subagent".into(),
                        path: ".codex/agents/worker.toml".into(),
                        options: vec!["keep".into(), "replace".into()],
                        selected: Some("keep".into()),
                        apply_to_identical: false,
                    },
                ],
                external_actions: Vec::new(),
                transaction: TransactionPlanInfo {
                    stages: Vec::new(),
                    backup_root: String::new(),
                    staging_root: String::new(),
                    directories: Vec::new(),
                    atomic_apply_expected: true,
                    project_root_mode: ProjectRootMode::Existing,
                    project_root_parent: None,
                    project_root_leaf: None,
                },
                approvals: PlanApprovals {
                    dry_run_reviewed: false,
                    external_actions_reviewed: false,
                    git_remote_approved: false,
                    push_approved: false,
                },
            },
            prepared_files: vec![
                prepared_file("generated-agents", "AGENTS.md", "incoming agents"),
                prepared_file("generated-readme", "README.md", "incoming readme"),
                prepared_file(
                    "source-skill",
                    ".agents/skills/one/SKILL.md",
                    "incoming skill",
                ),
                prepared_file(
                    "source-subagent",
                    ".codex/agents/worker.toml",
                    "incoming agent",
                ),
            ],
            merge_contexts: HashMap::new(),
            canonical_root: root.clone(),
            approved: false,
        };

        refresh_flattened_outputs(&mut prepared_plan).unwrap();
        let flat_agents = "chatgpt_project_sources/AGENTS.md";
        let flat_agents_operation = prepared_plan
            .plan
            .operations
            .iter_mut()
            .find(|operation| operation.destination == flat_agents)
            .unwrap();
        assert_eq!(flat_agents_operation.local_state, LocalState::Modified);
        let flat_agents_operation_id = flat_agents_operation.id.clone();
        flat_agents_operation.action = OperationAction::Skip;
        flat_agents_operation.resolution = Some("keep".into());
        flat_agents_operation.result_sha256 = None;
        prepared_plan
            .plan
            .conflicts
            .iter_mut()
            .find(|conflict| conflict.path == flat_agents)
            .unwrap()
            .selected = Some("keep".into());
        prepared_plan
            .prepared_files
            .retain(|file| file.operation_id != flat_agents_operation_id);

        let skill_id = prepared_plan
            .plan
            .operations
            .iter()
            .find(|operation| operation.destination == ".agents/skills/one/SKILL.md")
            .unwrap()
            .id
            .clone();
        let new_skill = "new skill after review";
        let new_skill_sha = digest(new_skill);
        prepared_plan
            .plan
            .operations
            .iter_mut()
            .find(|operation| operation.id == skill_id)
            .unwrap()
            .action = OperationAction::Replace;
        let skill_operation = prepared_plan
            .plan
            .operations
            .iter_mut()
            .find(|operation| operation.id == skill_id)
            .unwrap();
        skill_operation.resolution = Some("replace".into());
        skill_operation.result_sha256 = Some(new_skill_sha.clone());
        prepared_plan
            .prepared_files
            .iter_mut()
            .find(|file| file.operation_id == skill_id)
            .unwrap()
            .bytes = new_skill.as_bytes().to_vec();
        prepared_plan
            .prepared_files
            .iter_mut()
            .find(|file| file.operation_id == skill_id)
            .unwrap()
            .expected_sha256 = new_skill_sha;

        refresh_flattened_outputs(&mut prepared_plan).unwrap();
        let preserved = prepared_plan
            .plan
            .operations
            .iter()
            .find(|operation| operation.destination == flat_agents)
            .unwrap();
        assert_eq!(preserved.action, OperationAction::Skip);
        assert_eq!(preserved.resolution.as_deref(), Some("keep"));
        assert!(!prepared_plan
            .prepared_files
            .iter()
            .any(|file| file.destination == flat_agents));
        assert_eq!(
            prepared_plan
                .plan
                .generated_artifacts
                .iter()
                .find(|artifact| artifact.destination == "chatgpt_project_sources/one.md")
                .map(|artifact| artifact.content.as_str()),
            Some(new_skill)
        );
        assert!(crate::transaction::validate_flatten_transaction_inputs(
            &prepared_plan.plan,
            &prepared_plan.prepared_files,
            &root,
        )
        .is_ok());
    }

    #[test]
    fn maintenance_flatten_excludes_review_required_incoming_bytes() {
        let source_hash = "a".repeat(64);
        let operation = PlanOperation {
            id: "review-required".into(),
            component_id: "core.skills".into(),
            ownership: Some(Ownership::Managed),
            location_scope: Some("project".into()),
            action: OperationAction::Skip,
            source_path: Some(".agents/skills/example/SKILL.md".into()),
            destination: ".agents/skills/example/SKILL.md".into(),
            source_sha256: Some(source_hash.clone()),
            source_size: Some(1),
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: None,
            base_sha256: None,
            local_sha256: Some("b".repeat(64)),
            local_state: LocalState::Modified,
            resolution: Some("review_required".into()),
            external: false,
            rollback: RollbackAction::RestoreBackup,
        };
        let prepared = PreparedFile {
            operation_id: "review-required".into(),
            destination: ".agents/skills/example/SKILL.md".into(),
            bytes: b"incoming".to_vec(),
            expected_sha256: sha256_bytes(b"incoming"),
        };

        let project = tempfile::tempdir().unwrap();
        assert!(
            accepted_flatten_prepared_files(&[prepared], &[operation], project.path())
                .unwrap()
                .is_empty()
        );
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
        let _state_guard = test_state_guard();
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

    fn seed_codex_local_state_for_test() {
        let analysis_id = Uuid::new_v4();
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
                    provider: Some("codex".into()),
                    model: None,
                    optimization_profile: Some("Codex project and ChatGPT Chat".into()),
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
                endpoint_fingerprint: None,
                confirmed_values_sha256: None,
            },
        );
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(PathBuf::from("C:/mods/example")),
            scan_id: Some(Uuid::new_v4()),
            entries: HashMap::from([("finding".into(), Vec::new())]),
            evidence_sha256: None,
        };
    }

    #[test]
    fn local_codex_state_can_be_cleared_without_starting_an_external_session() {
        let _state_guard = test_state_guard();
        seed_codex_local_state_for_test();

        clear_codex_local_state().unwrap();

        assert!(codex_analyses().lock().unwrap().is_empty());
        let evidence = codex_approved_evidence().lock().unwrap();
        assert!(evidence.project_root.is_none());
        assert!(evidence.scan_id.is_none());
        assert!(evidence.entries.is_empty());
    }

    #[test]
    fn failed_remote_logout_still_clears_all_local_session_state() {
        let _state_guard = test_state_guard();
        seed_codex_local_state_for_test();
        codex_login_cancellations()
            .lock()
            .unwrap()
            .insert("login-pending".into(), Arc::new(AtomicBool::new(false)));
        let mut session = Some("initialized App Server session");

        let result = finalize_codex_logout_state(
            &mut session,
            Err("Sign-out could not reach Codex.".into()),
            clear_codex_local_state,
            || codex_login_cancellations().lock().unwrap().clear(),
        );

        assert_eq!(result.unwrap_err(), "Sign-out could not reach Codex.");
        assert!(session.is_none());
        assert!(codex_analyses().lock().unwrap().is_empty());
        assert!(codex_login_cancellations().lock().unwrap().is_empty());
        let evidence = codex_approved_evidence().lock().unwrap();
        assert!(evidence.project_root.is_none());
        assert!(evidence.scan_id.is_none());
        assert!(evidence.entries.is_empty());
    }
}
