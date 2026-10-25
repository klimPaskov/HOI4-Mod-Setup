//! The only filesystem, network, process, and credential boundary exposed to React.

use crate::ai::{self, AiProviderConfig};
#[cfg(test)]
use crate::codex::CodexAnalysis;
use crate::codex::{
    find_codex_executable, missing_status, AiAccountStatus, AiAnalysisRequest, AppServerProtocol,
    ApprovedEvidence, CodexAccountStatus, CodexAnalysisRequest, CodexAnalysisResult,
    CodexLoginStart, ProcessJsonlTransport,
};
use crate::coding_environment;
use crate::credentials::{
    discover_meshy_credential_reference, image_generation_credential_reference, provider_name,
    save_ai_provider_key, save_image_generation_key, save_meshy_key,
    validate_ai_provider_credential_for, validate_credential_reference,
    validate_image_generation_credential_for, CredentialStore, OsCredentialStore,
    ScopedSecretEnvironment, MESHY_ENVIRONMENT_NAME,
};
use crate::descriptors::generated_artifacts as render_generated_artifacts;
use crate::merge::FileKind;
use crate::models::{CredentialReference, *};
use crate::paths::{
    application_data_root, hoi4_user_mod_directory, validate_project_root,
    validate_project_root_or_destination,
};
use crate::readiness::ReadinessInput;
use crate::safe_fs::RootedDir;
#[cfg(test)]
use crate::scanner::discover_launcher_descriptor_with_check;
use crate::scanner::{
    discover_launcher_descriptor, scan_project_with_progress as scan_project_files, ScanOptions,
    ScanProgress,
};
use crate::security::{
    canonical_relative_key, path_has_link_component, redact_secrets, safe_join, sha256_bytes,
    sha256_file, validate_external_destination,
};
use crate::service::builder::{build_plan_with, PlanHost};
use crate::service::evidence::{
    ai_endpoint_from_state, ai_model_from_state, ai_provider_from_state,
    ai_reasoning_effort_from_state, approve_evidence, bind_component_registry, confirm_analysis,
    confirmation_values_sha256, confirmed_analysis_for_plan, plan_root_identity,
    record_completed_scan, record_confirmed_in_session, retain_analysis,
    same_persisted_codex_record, same_project_root, validate_approved_project_root_identity,
    validate_evidence_request, ApprovedScanEvidence, PendingCodexAnalysis,
};
#[cfg(test)]
use crate::service::evidence::{confirmation_values_from_state, scan_evidence_entries};
use crate::service::inputs::{
    ai_credential_reference, image_client_config_artifacts_with_launch,
    image_client_config_ownership, image_generation_config_artifact,
    image_generation_credential_field_supported, image_generation_guidance_artifact,
    image_generation_selection_from_state, is_image_generation_client_config_path,
    manifest_external_actions, merge_existing_image_client_config, path_file_kind,
    portrait_component_ids, portrait_pipeline_from_state, project_conventions_from_state,
    project_conventions_from_value, project_identity_from_state, project_root_from_state,
    source_request_from_state, validate_analysis_source_binding, LauncherPolicy,
};
#[cfg(test)]
use crate::service::inputs::{
    ai_optimization_profile, generated_state, project_readme, requested_components_for_selection,
    seed_manifest_optional_states, selected_ids, transaction_directories,
};
use crate::service::journal_bound_to_root;
#[cfg(test)]
use crate::service::plans::refresh_flattened_outputs;
use crate::service::plans::{
    accepted_flatten_prepared_files, approve_prepared_plan, resolve_conflict,
    store_prepared_plan_in, MergeContext, PreparedPlan,
};
use crate::service::runtime::{mcp_client_launch as launch_with_node, NodeRuntime};
use crate::source::{
    expand_components, resolve_source, select_manifest_files, verify_download, HttpSourceClient,
    SourceFetcher, SourceRequest,
};
use crate::transaction::{
    discard_staging, resume_transaction_with_options, run_transaction, PostInstallActionOutcome,
    TransactionOptions,
};
use crate::AppError;
use hoi4_setup_core::adapt::*;
use hoi4_setup_core::free_assistant::OPENROUTER_PROVIDER_ID;
use hoi4_setup_core::image_generation::*;
use hoi4_setup_core::manifest::{add_supported_profile_components, default_profile_component_ids};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;
use uuid::Uuid;

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
    profiles: Vec<Profile>,
    coding_environments: Vec<CodingEnvironmentDefinition>,
    /// Declared clients this app version or platform cannot install.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unavailable_coding_environments: Vec<UnavailableCodingEnvironment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_generation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    live_testing: Option<Value>,
    /// Set when a developer serves the package from a local checkout.
    local_source: Option<String>,
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
static CLAUDE_LOGIN_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();
static CODEX_LOGIN_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();
static SCAN_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
static SCAN_GENERATION: OnceLock<Mutex<Uuid>> = OnceLock::new();
static MESHY_CREDENTIAL_REFERENCE: OnceLock<Mutex<Option<CredentialReference>>> = OnceLock::new();
static THREE_D_HEALTH: OnceLock<Mutex<HashMap<String, CachedThreeDHealth>>> = OnceLock::new();
static READY_PROJECTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn prepared_plans() -> &'static Mutex<std::collections::HashMap<Uuid, PreparedPlan>> {
    PREPARED_PLANS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn ready_projects() -> &'static Mutex<HashMap<String, String>> {
    READY_PROJECTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn codex_session() -> &'static Mutex<Option<AppServerProtocol<ProcessJsonlTransport>>> {
    CODEX_SESSION.get_or_init(|| Mutex::new(None))
}

fn scan_cancellations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    SCAN_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn scan_generation() -> &'static Mutex<Uuid> {
    SCAN_GENERATION.get_or_init(|| Mutex::new(Uuid::nil()))
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

fn claude_login_cancellations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    CLAUDE_LOGIN_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn claude_logins_in_flight() -> &'static Mutex<std::collections::HashSet<String>> {
    static IN_FLIGHT: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

fn cancel_all_claude_logins() {
    if let Ok(mut cancellations) = claude_login_cancellations().lock() {
        for cancellation in cancellations.values() {
            cancellation.store(true, Ordering::SeqCst);
        }
        cancellations.clear();
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

#[cfg(test)]
thread_local! {
    static ISOLATE_MESHY_VAULT_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn current_meshy_credential_reference() -> Result<Option<CredentialReference>, AppError> {
    #[cfg(test)]
    if ISOLATE_MESHY_VAULT_FOR_TEST.with(std::cell::Cell::get) {
        return Ok(None);
    }
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
    // Ready is session-health evidence, not a durable assertion about a
    // credential that may have been deleted or changed while the app was not
    // running. Without a matching cache entry the optional workflow must be
    // checked again and remains non-blocking but incomplete.
    "incomplete".into()
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
            let server_version = crate::codex::codex_executable_version(&executable)?;
            let transport = ProcessJsonlTransport::start(executable)?;
            let mut protocol =
                AppServerProtocol::with_expected_server_version(transport, server_version);
            protocol.initialize()?;
            Ok(protocol)
        },
        AppServerProtocol::is_alive,
        callback,
        clear_codex_analysis_state,
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
    if matches!(&result, Err(AppError::Process(_) | AppError::Protocol(_))) {
        *session = None;
        clear_local_state().map_err(AppError::Process)?;
    }
    result
}

fn clear_codex_local_state() -> Result<(), String> {
    clear_codex_analysis_state()?;
    clear_approved_scan_evidence()?;
    Ok(())
}

fn clear_codex_analysis_state() -> Result<(), String> {
    codex_analyses()
        .lock()
        .map_err(|_| "Codex analysis store is unavailable".to_string())?
        .clear();
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
            reasoning_effort: ai_reasoning_effort_from_state(state),
            endpoint: ai_endpoint_from_state(state).unwrap_or_default(),
            credential_reference: ai_credential_reference(&provider),
        },
    );
    if status.error.is_some() || !status.available || !status.authenticated || status.usage_limited
    {
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
    mut plan: InstallationPlan,
    prepared_files: Vec<crate::models::PreparedFile>,
    merge_contexts: HashMap<String, MergeContext>,
    canonical_root: PathBuf,
) -> Result<InstallationPlan, AppError> {
    // A plan that installs the package's check tooling also carries the
    // reviewed step that prepares it (the desktop runs it after the apply).
    let install = crate::paths::game_install();
    crate::agent_setup::add_to_plan(
        &mut plan,
        install.detected.then_some(install.game_dir.as_str()),
    );
    let mut plans = prepared_plans()
        .lock()
        .map_err(|_| AppError::Transaction("prepared plan store is unavailable".into()))?;
    store_prepared_plan_in(
        &mut plans,
        plan,
        prepared_files,
        merge_contexts,
        canonical_root,
    )
}

fn command_error(error: AppError) -> String {
    error.to_string()
}

fn provider_analysis_user_error(error: AppError) -> String {
    match error {
        AppError::UnsafeProviderOutput(_) => {
            "The setup assistant returned private-looking account data. The response was discarded. Your draft and scan are unchanged; try the review again.".into()
        }
        AppError::InvalidInput(_) => {
            "The selected provider settings or returned proposal were invalid. Review the setup assistant settings and try again."
                .into()
        }
        AppError::PathSecurity(_) => {
            "The response contained an unsafe project value. Review the proposal and try again."
                .into()
        }
        AppError::Source(_) => {
            "The verified setup source could not be loaded. Check the connection and try again."
                .into()
        }
        AppError::Scan(_) => {
            "The approved project scan could not be used for analysis. Scan the project again and retry."
                .into()
        }
        AppError::Merge(_) | AppError::Transaction(_) | AppError::Io(_) => {
            "The analysis could not be prepared safely. Your draft and scan remain unchanged; try again."
                .into()
        }
        AppError::Credential(message) if message.contains("changed after analysis") => {
            "The setup account changed while the review was finishing. Run the review again.".into()
        }
        AppError::Credential(message) if message.contains("usage") && message.contains("limited") => {
            "The selected provider's usage is currently limited. Your draft and scan are unchanged; try again when usage is available.".into()
        }
        AppError::Credential(_) => {
            "The selected provider credential needs attention. Reconnect the setup assistant and try again."
                .into()
        }
        AppError::Process(message) if message.contains("usage is limited") => {
            "The selected provider's usage is currently limited. Your draft and scan are unchanged; try again when usage is available.".into()
        }
        AppError::Process(_) => {
            "The selected provider is temporarily unavailable. Check its connection and try again."
                .into()
        }
        AppError::Protocol(_) => {
            "The selected provider returned an invalid protocol response. Check its connection and try again."
                .into()
        }
        AppError::Serialization(_) => {
            "The response did not match the required proposal format. Try the analysis again."
                .into()
        }
        AppError::UnsupportedPlatform(_) => {
            "The selected provider is not supported on this platform. Choose another setup assistant."
                .into()
        }
    }
}

/// Renderer-facing plan errors. Validation messages that ask the user to
/// change a choice stay readable; source, filesystem, transaction, and tool
/// failures are replaced with a next step so internal paths, URLs, and OS
/// error text never reach the screen.
/// Refuse a plan whose MCP setup would have to replace an installed HOI4
/// Agent Tools package that a running MCP client has loaded. npm cannot
/// replace loaded native libraries, so the failure would otherwise surface
/// only at readiness, after every project file was applied.
fn ensure_mcp_package_replaceable(plan: &InstallationPlan) -> Result<(), AppError> {
    let Ok(target) = crate::mcp::reviewed_plan_target(&plan.external_actions) else {
        return Ok(());
    };
    if crate::mcp::loaded_native_files_blocking_replacement(&target)?.is_empty() {
        return Ok(());
    }
    Err(AppError::InvalidInput(format!(
        "{MCP_IN_USE_MESSAGE} Nothing was changed."
    )))
}

const MCP_IN_USE_MESSAGE: &str = "This setup needs to reinstall HOI4 Agent Tools at the version it uses, but an app connected to the HOI4 MCP is using the installed copy. Close Codex, Claude Code, Cursor, or any other app using the HOI4 MCP, then prepare the changes again. For an older project, Check for updates moves it to the current version instead.";

/// The current bytes of a generated file that still matches its installed
/// hash. They are exactly the generated content, so a lock written before
/// generated content was kept for updated files can still be repaired.
fn healthy_generated_bytes(
    lock: &InstallationLock,
    root: &Path,
    operation: &PlanOperation,
) -> Option<Vec<u8>> {
    let locked = lock
        .files
        .iter()
        .find(|file| file.path == operation.destination && file.external == operation.external)?;
    let path = if operation.external {
        crate::security::validate_external_destination(&operation.destination).ok()?
    } else {
        safe_join(root, &operation.destination).ok()?
    };
    let bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
        path.parent()?,
        path.file_name()?.to_str()?,
        64 * 1024 * 1024,
    )
    .ok()?;
    (sha256_bytes(&bytes) == locked.installed_sha256).then_some(bytes)
}

fn planning_command_error(error: AppError) -> String {
    crate::paths::record_diagnostic("plan", &error.to_string());
    let message = planning_user_message(error);
    match crate::paths::diagnostics_log_path() {
        Some(path) => format!("{message} Technical details: {}", path.display()),
        None => message,
    }
}

fn planning_user_message(error: AppError) -> String {
    match error {
        AppError::Credential(message) if message.starts_with("sign in with ChatGPT") => {
            "Sign in with ChatGPT before continuing.".into()
        }
        AppError::Credential(message) if message.contains("different provider endpoint") => {
            "This project was set up with a different assistant address than the one connected now. Connect the same address on the first screen, run the review again, and confirm it. Nothing was changed."
                .into()
        }
        // Confirmation and session-binding checks share the credential
        // category with real sign-in failures, but the user fixes them by
        // reviewing again, not by reconnecting.
        AppError::Credential(message)
            if !message.starts_with("connect the selected")
                && ["reanalysis", "confirm", "semantic review", "core session"]
                    .iter()
                    .any(|needle| message.contains(needle)) =>
        {
            "The confirmed review no longer matches this session. Run the review again, confirm it, and try again. Nothing was changed."
                .into()
        }
        provider_error @ (AppError::Credential(_)
        | AppError::UnsafeProviderOutput(_)
        | AppError::Protocol(_)
        | AppError::Serialization(_)) => provider_analysis_user_error(provider_error),
        AppError::Source(message) => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("git lfs pointer") {
                "The setup source contains a placeholder instead of a real file. Nothing was changed; try again after the source is fixed."
                    .into()
            } else if lower.contains("os error 206") || lower.contains("filename or extension is too long") {
                "A file path is too long for Windows. Move the project to a shorter folder or turn on Windows long paths, then try again. Nothing was changed."
                    .into()
            } else if lower.contains("source blob response exceeds its bounded size limit") {
                // A file larger than its manifest declares: the published
                // manifest and files of this version disagree. A commit made
                // before the source's manifest refresh does this.
                "The files of this setup version do not match its published manifest, so it was refused. Nothing was changed. Choose Latest, a release, or a newer commit."
                    .into()
            } else if lower.contains("checksum") || lower.contains("sha-256") {
                "The setup source did not pass verification. Nothing was changed; try again later."
                    .into()
            } else {
                "The setup source could not be downloaded or prepared. Check your connection and try again; nothing was changed."
                    .into()
            }
        }
        AppError::Transaction(_) | AppError::Io(_) | AppError::Merge(_) => {
            "The changes could not be prepared safely. Nothing was changed; try again.".into()
        }
        AppError::PathSecurity(_) => {
            "A project path changed or is not safe to use. Check the project folder and try again."
                .into()
        }
        AppError::Process(_) => {
            "A required tool could not run while preparing the changes. Try again.".into()
        }
        AppError::Scan(_) => {
            "The project scan is out of date. Scan the project again before preparing changes."
                .into()
        }
        AppError::UnsupportedPlatform(_) => {
            "A selected component is not supported on this computer. Deselect it and try again."
                .into()
        }
        AppError::InvalidInput(message) => {
            let message = crate::security::redact_secrets(&message, &[]);
            let mut characters = message.chars();
            match characters.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), characters.as_str()),
                None => "The selected options are not valid. Review them and try again.".into(),
            }
        }
    }
}

async fn run_blocking_command<T, F>(name: &'static str, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| format!("{name} stopped unexpectedly"))?
}

/// Renderer-facing messages for the Claude account route. Claude Code's raw
/// output never reaches the UI; only these sanitized categories do.
fn claude_user_error(error: AppError) -> String {
    match error {
        AppError::Process(message) if message.contains("was not found") => {
            "Claude Code is not installed. Install it, then choose Check again.".into()
        }
        AppError::Process(message) if message.contains("needs an update") => {
            "Claude Code needs an update. Run claude update, then choose Check again.".into()
        }
        AppError::Process(message) if message.contains("cancelled") => {
            "Claude sign-in was cancelled.".into()
        }
        AppError::Process(message) if message.contains("timed out") => {
            "Claude Code took too long to respond. Your draft is unchanged; try again.".into()
        }
        AppError::Process(_) | AppError::Protocol(_) => {
            "Claude Code could not complete this request. Your draft is unchanged; try again."
                .into()
        }
        AppError::Credential(message) => {
            let category = message.to_ascii_lowercase();
            if category.contains("usage") && category.contains("limited") {
                "Claude usage is currently limited. Your draft is unchanged; try again when usage is available.".into()
            } else if category.contains("timed out") {
                "Claude sign-in timed out. Choose Sign in to Claude to try again.".into()
            } else if category.contains("credential-shaped") {
                "Private-looking information was blocked. Remove secrets from the description or selected evidence, then try again.".into()
            } else if category.contains("did not finish") {
                "Claude sign-in did not finish. Try again, or sign in from a terminal with claude auth login and choose Check again.".into()
            } else {
                "Sign in to Claude before continuing.".into()
            }
        }
        AppError::Serialization(_) => {
            "Claude returned a response that did not match the required proposal format. Try the analysis again."
                .into()
        }
        AppError::InvalidInput(message) if message.contains("model") => {
            "Claude Code did not accept the selected model. Choose another model and try again."
                .into()
        }
        AppError::UnsupportedPlatform(_) => {
            "Claude sign-in is not available on this computer.".into()
        }
        other => provider_analysis_user_error(other),
    }
}

fn codex_user_error(error: AppError) -> String {
    match error {
        error @ AppError::UnsafeProviderOutput(_) => provider_analysis_user_error(error),
        AppError::Process(message) if message.contains("model is not supported") || message.contains("model is not available") => {
            "The selected model is not available for this Codex account. Update Codex and choose an available model, then try the review again.".into()
        }
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
        AppError::Protocol(_) => {
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
            "Codex returned a response that did not match the required proposal format. Try the analysis again."
                .into()
        }
        AppError::UnsupportedPlatform(_) => {
            "Codex sign-in is not available on this computer.".into()
        }
        _ => "Codex could not complete this request. Try again.".into(),
    }
}

pub fn run() {
    // Verify the setup-assistant clients' signatures in the background while
    // the first screen renders. Platform signature checks of these large
    // binaries take tens of seconds; a remembered result makes the first
    // account check fast. Nothing is started or read from an account here.
    std::thread::spawn(|| {
        crate::claude_code::warm_executable();
        let _ = find_codex_executable();
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                cancel_all_codex_logins();
                cancel_all_claude_logins();
                crate::account_cli::shutdown();
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
            ai_model_list,
            ai_account_read,
            store_ai_provider_credential,
            openrouter_sign_in,
            openrouter_sign_in_cancel,
            draft_mod_description,
            remove_ai_provider_credential,
            store_image_generation_credential,
            image_generation_credential_status,
            remove_image_generation_credential,
            claude_login_start,
            claude_login_wait,
            claude_login_cancel,
            claude_logout,
            provider_account_login,
            provider_account_cancel,
            provider_account_login_wait,
            provider_account_pending,
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
            read_diagnostics,
            pick_context_documents,
            pick_context_images,
            forget_context_attachment,
            scan_project,
            cancel_scan,
            store_meshy_credential,
            meshy_credential_status,
            remove_meshy_credential,
            run_3d_health_check,
            inspect_local_portrait_provider,
            install_local_portrait_workflows,
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
            check_installation_requirements,
            resolve_installation_conflict,
            apply_installation,
            run_agent_setup,
            rollback_installation,
            read_transaction_journal,
            find_interrupted_transaction,
            resume_installation,
            discard_installation_staging,
            open_in_codex,
            open_in_coding_environment,
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
        "supported_platforms": ["windows", "macos", "linux"],
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
async fn ai_model_list(provider: String, endpoint: String) -> Result<Vec<AiModelOption>, String> {
    run_blocking_command("provider-model-list", move || {
        if provider == "codex" {
            return with_codex_session(AppServerProtocol::model_list)
                .map_err(provider_analysis_user_error);
        }
        let profile = ai::profile(&provider)
            .ok_or_else(|| AppError::InvalidInput("unsupported AI provider".into()))
            .map_err(provider_analysis_user_error)?;
        ai::list_models(
            &OsCredentialStore,
            &AiProviderConfig {
                provider: provider.clone(),
                model: profile.default_model.unwrap_or_else(|| "model-list".into()),
                reasoning_effort: profile
                    .default_reasoning_effort
                    .unwrap_or_else(|| "high".into()),
                endpoint,
                credential_reference: ai_credential_reference(&provider),
            },
        )
        .map_err(provider_analysis_user_error)
    })
    .await
}

#[tauri::command(async)]
async fn ai_account_read(
    provider: String,
    model: String,
    reasoning_effort: String,
    endpoint: String,
) -> AiAccountStatus {
    let fallback_provider = provider.clone();
    let fallback_model = model.clone();
    run_blocking_command("provider-account-read", move || {
        let credential_reference = ai_credential_reference(&provider);
        Ok(ai::account_status(
            &OsCredentialStore,
            &AiProviderConfig {
                provider,
                model,
                reasoning_effort,
                endpoint,
                credential_reference,
            },
        ))
    })
    .await
    .unwrap_or_else(|_| AiAccountStatus {
        available: false,
        authenticated: false,
        provider: fallback_provider,
        model: fallback_model,
        auth_mode: "unconfigured".into(),
        usage_limited: false,
        error: Some("The selected provider status could not be read. Choose Check again.".into()),
    })
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

fn openrouter_sign_in_cancellation() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    static CANCELLATION: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();
    CANCELLATION.get_or_init(|| Mutex::new(None))
}

/// Sign in with OpenRouter in the system browser and keep the returned key in
/// the OS vault as the OpenRouter credential.
#[tauri::command(async)]
async fn openrouter_sign_in() -> Result<bool, String> {
    run_blocking_command("openrouter-sign-in", || {
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut slot = openrouter_sign_in_cancellation()
                .lock()
                .map_err(|_| "OpenRouter sign-in is unavailable".to_string())?;
            if let Some(previous) = slot.replace(cancelled.clone()) {
                previous.store(true, Ordering::SeqCst);
            }
        }
        let outcome = (|| {
            let pending = crate::openrouter::PendingSignIn::start().map_err(command_error)?;
            open_url_in_system_browser(pending.url.clone())?;
            let key = pending.finish(&cancelled).map_err(command_error)?;
            let reference = save_ai_provider_key(&OsCredentialStore, OPENROUTER_PROVIDER_ID, &key)
                .map_err(command_error)?;
            validate_ai_provider_credential_for(&reference, OPENROUTER_PROVIDER_ID)
                .map_err(command_error)?;
            Ok(true)
        })();
        if let Ok(mut slot) = openrouter_sign_in_cancellation().lock() {
            if slot
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &cancelled))
            {
                *slot = None;
            }
        }
        outcome
    })
    .await
}

#[tauri::command(async)]
fn openrouter_sign_in_cancel() -> Result<(), String> {
    if let Some(cancelled) = openrouter_sign_in_cancellation()
        .lock()
        .map_err(|_| "OpenRouter sign-in is unavailable".to_string())?
        .take()
    {
        cancelled.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// The magic wand on the description step. The note goes only to the
/// selected setup assistant; the draft returns to the description field for
/// the user to review.
#[tauri::command(async)]
async fn draft_mod_description(
    provider: String,
    model: String,
    reasoning_effort: String,
    endpoint: String,
    note: String,
) -> Result<String, String> {
    run_blocking_command("description-draft", move || {
        if provider == "codex" {
            return with_codex_session(|session| {
                session.draft_description(&note, &model, &reasoning_effort)
            })
            .map_err(codex_user_error);
        }
        let credential_reference = ai_credential_reference(&provider);
        if let Some(reference) = credential_reference.as_ref() {
            validate_ai_provider_credential_for(reference, provider.trim())
                .map_err(provider_analysis_user_error)?;
        }
        ai::draft_description(
            &OsCredentialStore,
            &AiProviderConfig {
                provider: provider.clone(),
                model,
                reasoning_effort,
                endpoint,
                credential_reference,
            },
            &note,
        )
        .map_err(|error| {
            if provider == crate::claude_code::PROVIDER_ID {
                claude_user_error(error)
            } else {
                provider_analysis_user_error(error)
            }
        })
    })
    .await
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
fn store_image_generation_credential(
    provider: String,
    field: String,
    value: String,
) -> Result<CredentialReference, String> {
    let provider = provider.trim();
    let field = field.trim();
    if !image_generation_credential_field_supported(provider, field) {
        return Err("That image provider credential field is not supported.".into());
    }
    save_image_generation_key(&OsCredentialStore, provider, field, &value).map_err(command_error)
}

#[tauri::command(async)]
fn image_generation_credential_status(
    provider: String,
    field: String,
) -> Result<Option<CredentialReference>, String> {
    let provider = provider.trim();
    let field = field.trim();
    if !image_generation_credential_field_supported(provider, field) {
        return Err("That image provider credential field is not supported.".into());
    }
    let reference =
        image_generation_credential_reference(provider, field).map_err(command_error)?;
    validate_image_generation_credential_for(&reference, provider, field).map_err(command_error)?;
    Ok(OsCredentialStore.read(&reference).ok().map(|_| reference))
}

#[tauri::command(async)]
fn remove_image_generation_credential(provider: String, field: String) -> Result<bool, String> {
    let provider = provider.trim();
    let field = field.trim();
    if !image_generation_credential_field_supported(provider, field) {
        return Err("That image provider credential field is not supported.".into());
    }
    let reference =
        image_generation_credential_reference(provider, field).map_err(command_error)?;
    validate_image_generation_credential_for(&reference, provider, field).map_err(command_error)?;
    OsCredentialStore
        .delete(&reference)
        .map_err(command_error)?;
    Ok(true)
}

#[tauri::command(async)]
fn claude_login_start() -> Result<String, String> {
    cancel_all_claude_logins();
    crate::claude_code::invalidate_session();
    clear_codex_local_state()?;
    let login_id = Uuid::new_v4().to_string();
    claude_login_cancellations()
        .lock()
        .map_err(|_| "Claude sign-in store is unavailable".to_string())?
        .insert(login_id.clone(), Arc::new(AtomicBool::new(false)));
    Ok(login_id)
}

/// Run Claude Code's own browser sign-in for one registered attempt and
/// return the refreshed non-secret status. The renderer never receives a
/// sign-in URL, code, or token: Claude Code opens the browser itself.
#[tauri::command(async)]
async fn claude_login_wait(login_id: String, model: String) -> Result<AiAccountStatus, String> {
    run_blocking_command("claude-login-wait", move || {
        let cancellation = claude_login_cancellations()
            .lock()
            .map_err(|_| "Claude sign-in store is unavailable".to_string())?
            .get(&login_id)
            .cloned()
            .ok_or_else(|| "Claude sign-in is not active or was already cancelled".to_string())?;
        // Each attempt is waited on once, so one ID never starts two
        // `claude auth login` processes.
        if !claude_logins_in_flight()
            .lock()
            .map_err(|_| "Claude sign-in store is unavailable".to_string())?
            .insert(login_id.clone())
        {
            return Err("Claude sign-in is already in progress.".into());
        }
        let mut should_stop = || cancellation.load(Ordering::SeqCst);
        let result = crate::claude_code::run_login(&mut should_stop);
        if let Ok(mut in_flight) = claude_logins_in_flight().lock() {
            in_flight.remove(&login_id);
        }
        if let Ok(mut cancellations) = claude_login_cancellations().lock() {
            cancellations.remove(&login_id);
        }
        // A cancel that arrives after Claude Code finished signing in does
        // not turn a completed sign-in into a cancellation.
        if result.is_err() && cancellation.load(Ordering::SeqCst) {
            return Err("Claude sign-in was cancelled.".into());
        }
        result.map_err(claude_user_error)?;
        Ok(crate::claude_code::account_status(&model))
    })
    .await
}

#[tauri::command(async)]
fn claude_login_cancel(login_id: String) -> Result<(), String> {
    let cancellation = claude_login_cancellations()
        .lock()
        .map_err(|_| "Claude sign-in store is unavailable".to_string())?
        .get(&login_id)
        .cloned()
        .ok_or_else(|| "Claude sign-in is not active or was already cancelled".to_string())?;
    cancellation.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command(async)]
async fn claude_logout() -> Result<(), String> {
    run_blocking_command("claude-logout", || {
        cancel_all_claude_logins();
        crate::claude_code::invalidate_session();
        let result = crate::claude_code::logout().map_err(claude_user_error);
        if let Ok(mut projects) = ready_projects().lock() {
            projects.clear();
        }
        clear_codex_local_state()?;
        result
    })
    .await
}

#[tauri::command(async)]
async fn provider_account_login(provider: String, logout: bool) -> Result<String, String> {
    run_blocking_command("provider-account-login", move || {
        let attempt = crate::account_cli::reserve_login(&provider, logout)
            .map_err(provider_analysis_user_error)?;
        clear_codex_local_state()?;
        Ok(attempt)
    })
    .await
}

#[tauri::command(async)]
async fn provider_account_cancel(provider: String, attempt_id: String) -> Result<(), String> {
    run_blocking_command("provider-account-cancel", move || {
        crate::account_cli::cancel_login(&provider, &attempt_id)
            .map_err(provider_analysis_user_error)
    })
    .await
}

#[tauri::command(async)]
async fn provider_account_login_wait(
    provider: String,
    attempt_id: String,
) -> Result<AiAccountStatus, String> {
    run_blocking_command("provider-account-login-wait", move || {
        crate::account_cli::wait_login(&provider, &attempt_id).map_err(provider_analysis_user_error)
    })
    .await
}

#[tauri::command(async)]
async fn provider_account_pending(provider: String) -> Result<Option<String>, String> {
    run_blocking_command("provider-account-pending", move || {
        crate::account_cli::pending_login_id(&provider).map_err(provider_analysis_user_error)
    })
    .await
}

#[tauri::command(async)]
async fn codex_account_read() -> CodexAccountStatus {
    run_blocking_command("codex-account-read", || {
        Ok(
            match with_codex_session(|session| session.account_read(false)) {
                Ok(status) => status,
                Err(error) => missing_status(codex_user_error(error)),
            },
        )
    })
    .await
    .unwrap_or_else(missing_status)
}

#[tauri::command(async)]
async fn codex_login_start(mode: String) -> CodexLoginStart {
    run_blocking_command("codex-login-start", move || {
        Ok(codex_login_start_blocking(mode))
    })
    .await
    .unwrap_or_else(|error| CodexLoginStart {
        available: false,
        error: Some(error),
        ..Default::default()
    })
}

fn codex_login_start_blocking(mode: String) -> CodexLoginStart {
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
async fn codex_login_wait(login_id: String) -> Result<CodexAccountStatus, String> {
    run_blocking_command("codex-login-wait", move || {
        codex_login_wait_blocking(login_id)
    })
    .await
}

fn codex_login_wait_blocking(login_id: String) -> Result<CodexAccountStatus, String> {
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
    const ALLOWED_URLS: &[&str] = &[
        "https://github.com/klimPaskov/Agentic-HOI4-Modding",
        "https://github.com/klimPaskov/HOI4-Mod-Setup",
        "https://github.com/klimPaskov/comfyui-hoi4-portraits",
        "https://chatgpt.com",
        "https://github.com/smerss/Clausewitz-MCP",
        "https://github.com/klimPaskov/hoi4-mod-setup/blob/main/docs/LIVE_TESTING_LINUX.md",
        "https://github.com/klimPaskov/Agentic-HOI4-Modding/blob/main/.claude/skills/hoi4-live-playtest/references/setup.md",
        "https://github.com/klimPaskov/Agentic-HOI4-Modding/blob/main/.claude/skills/hoi4-live-playtest/references/hoi4-live-tools.md",
        "https://developers.openai.com/api/docs/guides/image-generation/",
        "https://ai.google.dev/gemini-api/docs/image-generation",
        "https://fal.ai/pricing",
        "https://replicate.com/docs",
        "https://bfl.ai/models/flux-3-image",
        "https://platform.stability.ai/docs/getting-started/stable-image",
        "https://developer.ideogram.ai/",
        "https://www.recraft.ai/docs",
        "https://docs.leonardo.ai/docs/connect-to-leonardoai-mcp",
        "https://developer.adobe.com/firefly-services/docs/firefly-api/",
        "https://docs.x.ai/developers/model-capabilities/imagine",
        "https://www.together.ai/models/flux-2-pro",
        "https://openrouter.ai/docs/guides/features/images",
        "https://docs.comfy.org/",
        "https://github.com/AUTOMATIC1111/stable-diffusion-webui/wiki/API",
        "https://github.com/lllyasviel/stable-diffusion-webui-forge",
        "https://github.com/openai/codex/blob/main/codex-rs/skills/src/assets/samples/imagegen/SKILL.md",
        "https://cloud.google.com/vertex-ai/generative-ai/docs/models/imagen/4-0-generate-001",
        "https://learn.microsoft.com/en-us/azure/foundry/foundry-models/how-to/use-foundry-models-mai-image",
        "https://docs.aws.amazon.com/bedrock/latest/userguide/stable-image-services.html",
        "https://huggingface.co/docs/inference-providers/index",
        "https://github.com/midjourney/docs/blob/main/data/faq.txt",
        "https://github.com/klimPaskov/HOI4-Live-Tools",
        "https://klimpaskov.github.io/Agentic-HOI4-Modding/",
    ];
    ALLOWED_URLS.contains(&url)
        || coding_environment::NATIVE_CLIENTS
            .iter()
            .filter_map(|client| client.documentation_url)
            .any(|documentation| documentation == url)
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
async fn codex_logout() -> Result<(), String> {
    run_blocking_command("codex-logout", codex_logout_blocking).await
}

fn codex_logout_blocking() -> Result<(), String> {
    cancel_all_codex_logins();
    // A prior App Server interruption may have cleared the supervised
    // process while Codex-owned authentication remains persisted. Start and
    // initialize a fresh App Server when needed so sign-out always attempts
    // the official account/logout method.
    let logout_result = with_codex_session(|protocol| protocol.logout()).map_err(codex_user_error);
    complete_codex_logout(logout_result)
}

fn complete_codex_logout(logout_result: Result<(), String>) -> Result<(), String> {
    if let Ok(mut projects) = ready_projects().lock() {
        projects.clear();
    }
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

fn analysis_source_request(request: &CodexAnalysisRequest) -> Result<SourceRequest, AppError> {
    let source = request.constraints.get("source");
    let mode = match source
        .and_then(|value| value.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("latest")
    {
        "latest" => SourceMode::Latest,
        "pinned_commit" => SourceMode::PinnedCommit,
        "pinned_release" => SourceMode::PinnedRelease,
        _ => {
            return Err(AppError::InvalidInput(
                "semantic analysis declares an unsupported source mode".into(),
            ))
        }
    };
    let selected_ref = source
        .and_then(|value| value.get("selected_ref"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(SourceRequest {
        mode,
        requested_ref: (mode == SourceMode::PinnedCommit)
            .then(|| selected_ref.clone())
            .flatten(),
        release: (mode == SourceMode::PinnedRelease)
            .then_some(selected_ref)
            .flatten(),
    })
}

#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSourceSelection {
    mode: SourceMode,
    #[serde(default)]
    selected_ref: Option<String>,
}

impl UpdateSourceSelection {
    fn request(&self) -> Result<SourceRequest, AppError> {
        let selected = self.selected_ref.as_deref().unwrap_or("").trim();
        match self.mode {
            SourceMode::Latest if !selected.is_empty() => {
                return Err(AppError::InvalidInput(
                    "Latest cannot include a pinned reference".into(),
                ));
            }
            SourceMode::PinnedCommit => crate::source::validate_commit(selected)?,
            SourceMode::PinnedRelease
                if selected.is_empty()
                    || selected.len() > 256
                    || selected.chars().any(char::is_control) =>
            {
                return Err(AppError::InvalidInput("Choose a valid release tag".into()));
            }
            _ => {}
        }
        Ok(SourceRequest {
            mode: self.mode,
            requested_ref: (self.mode == SourceMode::PinnedCommit).then(|| selected.to_owned()),
            release: (self.mode == SourceMode::PinnedRelease).then(|| selected.to_owned()),
        })
    }
}

fn maintenance_source_request(
    lock: &InstallationLock,
    selection: Option<&UpdateSourceSelection>,
) -> Result<SourceRequest, AppError> {
    if let Some(selection) = selection {
        return selection.request();
    }
    // Older callers retain their recorded policy until they explicitly retarget.
    Ok(SourceRequest {
        mode: lock.source.mode,
        requested_ref: lock.source.requested_ref.clone(),
        release: lock.source.release.clone(),
    })
}

fn validate_update_release_identity(
    lock: &InstallationLock,
    request: &SourceRequest,
    resolved: &SourceIdentity,
) -> Result<(), AppError> {
    let same_release = request.mode == SourceMode::PinnedRelease
        && lock.source.mode == SourceMode::PinnedRelease
        && request.release.as_ref().or(request.requested_ref.as_ref())
            == lock
                .source
                .release
                .as_ref()
                .or(lock.source.requested_ref.as_ref());
    if same_release && resolved.resolved_revision != lock.source.revision {
        return Err(AppError::InvalidInput(
            "the pinned release resolved to a different commit; choose a new release explicitly"
                .into(),
        ));
    }
    Ok(())
}

fn source_request_for_analysis(request: &CodexAnalysisRequest) -> Result<SourceRequest, AppError> {
    if request.analysis_purpose.as_deref() != Some("maintenance_reanalysis") {
        return analysis_source_request(request);
    }
    let project_root = request
        .project_root
        .as_deref()
        .ok_or_else(|| AppError::PathSecurity("maintenance analysis has no project root".into()))?;
    let lock = read_project_lock(Path::new(project_root))?;
    let selection = request
        .constraints
        .get("update_source")
        .map(|value| {
            serde_json::from_value::<UpdateSourceSelection>(value.clone())
                .map_err(|_| AppError::InvalidInput("Invalid update source selection".into()))
        })
        .transpose()?;
    maintenance_source_request(&lock, selection.as_ref())
}

fn bind_analysis_component_registry(
    request: &mut CodexAnalysisRequest,
) -> Result<SourceIdentity, AppError> {
    let source_request = source_request_for_analysis(request)?;
    let client = HttpSourceClient::new()?;
    let resolution = resolve_source(&client, &source_request)?;
    bind_component_registry(request, &resolution)?;
    Ok(resolution.identity)
}

fn bind_analysis_record_to_source(record: &mut CodexAnalysisRecord, source: &SourceIdentity) {
    record.source_revision = Some(source.resolved_revision.clone());
    record.source_manifest_sha256 = Some(source.manifest_sha256.clone());
}

fn prepare_analysis_command_input<F>(
    request: &mut CodexAnalysisRequest,
    bind_registry: F,
) -> Result<SourceIdentity, String>
where
    F: FnOnce(&mut CodexAnalysisRequest) -> Result<SourceIdentity, AppError>,
{
    (|| {
        if let Some(project_root) = request.project_root.as_deref() {
            request.project_root = Some(
                validate_project_root(Path::new(project_root))?
                    .display()
                    .to_string(),
            );
        }
        validate_codex_evidence_approval(request)?;
        bind_registry(request)
    })()
    .map_err(provider_analysis_user_error)
}

#[tauri::command(async)]
async fn codex_analyze(
    request: CodexAnalysisRequest,
    model: String,
    reasoning_effort: String,
) -> Result<CodexAnalysisResult, String> {
    run_blocking_command("codex-analysis", move || {
        codex_analyze_blocking(request, model, reasoning_effort)
    })
    .await
}

fn codex_analyze_blocking(
    request: CodexAnalysisRequest,
    model: String,
    reasoning_effort: String,
) -> Result<CodexAnalysisResult, String> {
    let mut request = request;
    let source = prepare_analysis_command_input(&mut request, bind_analysis_component_registry)?;
    let mut result =
        with_codex_session(|session| session.analyze(&request, &model, &reasoning_effort))
            .map_err(codex_user_error)?;
    bind_analysis_record_to_source(&mut result.record, &source);
    let mut analyses = codex_analyses()
        .lock()
        .map_err(|_| "Codex analysis store is unavailable".to_string())?;
    retain_analysis(
        &mut analyses,
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
async fn ai_analyze(request: AiAnalysisRequest) -> Result<CodexAnalysisResult, String> {
    run_blocking_command("provider-analysis", move || ai_analyze_blocking(request)).await
}

fn ai_analyze_blocking(mut request: AiAnalysisRequest) -> Result<CodexAnalysisResult, String> {
    if request.provider == "codex" {
        return Err("Codex analysis must use the official Codex App Server command".into());
    }
    let account_generation = provider_session_generation(&request.provider);
    let credential_reference = ai_credential_reference(&request.provider);
    if let Some(reference) = credential_reference.as_ref() {
        validate_ai_provider_credential_for(reference, request.provider.trim())
            .map_err(provider_analysis_user_error)?;
    }
    let source =
        prepare_analysis_command_input(&mut request.analysis, bind_analysis_component_registry)?;
    let mut result = ai::analyze(
        &OsCredentialStore,
        &AiProviderConfig {
            provider: request.provider.clone(),
            model: request.model.clone(),
            reasoning_effort: request.reasoning_effort.clone(),
            endpoint: request.endpoint.clone(),
            credential_reference,
        },
        &request,
    )
    .map_err(|error| {
        // The internal reason is bounded and never carries model content; it
        // is what a user can share when every attempt is rejected.
        crate::paths::record_diagnostic("analysis", &error.to_string());
        if request.provider == crate::claude_code::PROVIDER_ID {
            claude_user_error(error)
        } else {
            provider_analysis_user_error(error)
        }
    })?;
    bind_analysis_record_to_source(&mut result.record, &source);
    retain_provider_analysis(&result, &request, account_generation)?;
    Ok(result)
}

fn retain_provider_analysis(
    result: &CodexAnalysisResult,
    request: &AiAnalysisRequest,
    account_generation: Option<u64>,
) -> Result<(), String> {
    let mut analyses = codex_analyses()
        .lock()
        .map_err(|_| "AI analysis store is unavailable".to_string())?;
    require_provider_session_generation(&request.provider, account_generation)
        .map_err(provider_analysis_user_error)?;
    retain_analysis(
        &mut analyses,
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
    Ok(())
}

fn provider_session_generation(provider: &str) -> Option<u64> {
    if provider == crate::claude_code::PROVIDER_ID {
        Some(crate::claude_code::session_generation())
    } else if crate::account_cli::is_account_provider(provider) {
        Some(crate::account_cli::session_generation())
    } else {
        None
    }
}

fn require_provider_session_generation(
    provider: &str,
    generation: Option<u64>,
) -> Result<(), AppError> {
    if provider_session_generation(provider) != generation {
        return Err(AppError::Credential(
            "the setup account changed after analysis; run the review again".into(),
        ));
    }
    Ok(())
}

#[tauri::command(async)]
fn approve_scan_evidence(
    project_root: String,
    scan_id: String,
    evidence: Vec<ApprovedEvidence>,
) -> Result<(), String> {
    let project_root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let scan_id = Uuid::parse_str(&scan_id).map_err(|_| "scan ID is invalid".to_string())?;
    let mut approved = codex_approved_evidence()
        .lock()
        .map_err(|_| "approved scan evidence store is unavailable".to_string())?;
    approve_evidence(&mut approved, &project_root, scan_id, &evidence)
}

fn validate_codex_evidence_approval(request: &CodexAnalysisRequest) -> Result<(), AppError> {
    if request.evidence.is_empty() {
        // No evidence is judged against no approval; the store is not read.
        return validate_evidence_request(
            &ApprovedScanEvidence::default(),
            request,
            &crate::context::validate_context_evidence,
        );
    }
    let approved = codex_approved_evidence().lock().map_err(|_| {
        AppError::Serialization("approved scan evidence store is unavailable".into())
    })?;
    validate_evidence_request(
        &approved,
        request,
        &crate::context::validate_context_evidence,
    )
}

#[tauri::command(async)]
fn confirm_codex_analysis(
    record: CodexAnalysisRecord,
    confirmed_fields: Vec<String>,
    confirmed_values: Value,
) -> Result<CodexAnalysisRecord, String> {
    let mut analyses = codex_analyses()
        .lock()
        .map_err(|_| "Codex analysis store is unavailable".to_string())?;
    confirm_analysis(&mut analyses, &record, &confirmed_fields, &confirmed_values)
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
            let discovery = discover_launcher_descriptor(&canonical);
            let launcher_descriptor_path = discovery
                .as_ref()
                .ok()
                .and_then(|path| path.as_ref())
                .map(|path| crate::paths::user_facing_path(path));
            Ok(FolderSelection {
                path: Some(crate::paths::user_facing_path(&canonical)),
                launcher_descriptor_path,
                // Discovery explains itself; the internal error category
                // prefix would only make a review item read like a crash.
                error: discovery.err().map(|error| match error {
                    AppError::Scan(message) => message,
                    other => other.to_string(),
                }),
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

/// Result of adding context files the user picked.
#[derive(Debug, Clone, serde::Serialize)]
struct ContextPick<T> {
    added: Vec<T>,
    refused: Vec<String>,
    cancelled: bool,
}

fn picked_paths(
    selected: Option<Vec<tauri_plugin_dialog::FilePath>>,
) -> Result<Option<Vec<PathBuf>>, String> {
    let Some(selected) = selected else {
        return Ok(None);
    };
    selected
        .into_iter()
        .map(|file| {
            file.into_path()
                .map_err(|error| format!("selected file is not a local path: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// Add documents that describe the mod. Reading happens only for files the
/// user picks in the native dialog.
#[tauri::command(async)]
async fn pick_context_documents(
    app: tauri::AppHandle,
) -> Result<ContextPick<crate::context::ContextDocument>, String> {
    use tauri_plugin_dialog::DialogExt;

    let selected = app
        .dialog()
        .file()
        .set_title("Add documents that describe the mod")
        .add_filter("Documents and scripts", crate::context::DOCUMENT_EXTENSIONS)
        .blocking_pick_files();
    let Some(paths) = picked_paths(selected)? else {
        return Ok(ContextPick {
            added: Vec::new(),
            refused: Vec::new(),
            cancelled: true,
        });
    };
    run_blocking_command("context-documents", move || {
        let (added, refused) = crate::context::add_documents(&paths);
        Ok(ContextPick {
            added,
            refused,
            cancelled: false,
        })
    })
    .await
}

/// Add images for the thumbnail or visual direction.
#[tauri::command(async)]
async fn pick_context_images(
    app: tauri::AppHandle,
) -> Result<ContextPick<crate::context::ContextImage>, String> {
    use tauri_plugin_dialog::DialogExt;

    let selected = app
        .dialog()
        .file()
        .set_title("Add images for the thumbnail or visual direction")
        .add_filter("Images", crate::context::IMAGE_EXTENSIONS)
        .blocking_pick_files();
    let Some(paths) = picked_paths(selected)? else {
        return Ok(ContextPick {
            added: Vec::new(),
            refused: Vec::new(),
            cancelled: true,
        });
    };
    run_blocking_command("context-images", move || {
        let (added, refused) = crate::context::add_images(&paths);
        Ok(ContextPick {
            added,
            refused,
            cancelled: false,
        })
    })
    .await
}

/// The recent diagnostics log, with the app version, for the user to copy.
#[tauri::command(async)]
fn read_diagnostics(app: tauri::AppHandle) -> Result<String, String> {
    let recent = crate::paths::recent_diagnostics(32 * 1024);
    Ok(format!(
        "HOI4 Mod Setup {} on {}\n{}",
        app.package_info().version,
        Platform::current().display_name(),
        if recent.trim().is_empty() {
            "No diagnostics have been recorded."
        } else {
            recent.as_str()
        }
    ))
}

#[tauri::command(async)]
fn forget_context_attachment(id: String) -> Result<(), String> {
    crate::context::forget(&id).map_err(command_error)
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
async fn scan_project(
    app: tauri::AppHandle,
    root: String,
    request_id: String,
    launcher_descriptor_path: Option<String>,
) -> Result<ScanResult, String> {
    run_blocking_command("project-scan", move || {
        scan_project_blocking(app, root, request_id, launcher_descriptor_path)
    })
    .await
}

fn scan_project_blocking(
    app: tauri::AppHandle,
    root: String,
    request_id: String,
    launcher_descriptor_path: Option<String>,
) -> Result<ScanResult, String> {
    validate_scan_request_id(&request_id)?;
    // A new scan invalidates every prior evidence approval, including when
    // validation or the worker fails before producing a result.
    let generation = Uuid::new_v4();
    let mut active_generation = scan_generation()
        .lock()
        .map_err(|_| "scan generation store is unavailable".to_string())?;
    let mut approved = codex_approved_evidence()
        .lock()
        .map_err(|_| "approved scan evidence store is unavailable".to_string())?;
    *active_generation = generation;
    *approved = ApprovedScanEvidence::default();
    drop(approved);
    drop(active_generation);
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
    let scan_attempt = (|| {
        let scanned_root_identity = RootedDir::open_read(Path::new(&root))
            .and_then(|directory| directory.identity_token())
            .map_err(command_error)?;
        let root = validate_project_root(Path::new(&root)).map_err(command_error)?;
        let validated_root_identity = RootedDir::open_read(&root)
            .and_then(|directory| directory.identity_token())
            .map_err(command_error)?;
        if validated_root_identity != scanned_root_identity {
            return Err("project root changed while the read-only scan was starting".into());
        }
        let launcher_descriptor = launcher_descriptor_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(validate_external_destination)
            .transpose()
            .map_err(command_error)?;
        let event_request_id = request_id.clone();
        let event_app = app.clone();
        let result = scan_project_files(
            &root,
            &ScanOptions {
                approved_external_descriptor: launcher_descriptor,
                cancel_flag: Some(cancellation.clone()),
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
        )
        .map_err(command_error)?;
        let completed_root_identity = RootedDir::open_read(&root)
            .and_then(|directory| directory.identity_token())
            .map_err(command_error)?;
        if completed_root_identity != scanned_root_identity {
            return Err("project root changed during the read-only scan".into());
        }
        Ok::<_, String>((root, scanned_root_identity, result))
    })();
    if let Ok(mut active) = scan_cancellations().lock() {
        active.remove(&request_id);
    }
    let (root, project_root_identity, result) = scan_attempt?;
    let active_generation = scan_generation()
        .lock()
        .map_err(|_| "scan generation store is unavailable".to_string())?;
    let mut approved = codex_approved_evidence()
        .lock()
        .map_err(|_| "approved scan evidence store is unavailable".to_string())?;
    if *active_generation != generation {
        return Err("scan result was superseded by a newer project scan".into());
    }
    record_completed_scan(&mut approved, root, project_root_identity, &result)?;
    Ok(result)
}

#[cfg(test)]
fn approve_launcher_descriptor_for_scan(
    root: &Path,
    requested: Option<&str>,
) -> Result<Option<PathBuf>, AppError> {
    approve_launcher_descriptor_for_scan_with_check(root, requested, || false)
}

#[cfg(test)]
fn approve_launcher_descriptor_for_scan_with_check(
    root: &Path,
    requested: Option<&str>,
    mut should_stop: impl FnMut() -> bool,
) -> Result<Option<PathBuf>, AppError> {
    let Some(requested) = requested.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let requested = validate_external_destination(requested)?
        .canonicalize()
        .map_err(AppError::from)?;
    let discovered =
        discover_launcher_descriptor_with_check(root, &mut should_stop)?.ok_or_else(|| {
            AppError::InvalidInput(
                "the requested launcher descriptor is not the current bounded candidate".into(),
            )
        })?;
    let discovered = validate_external_destination(&discovered.to_string_lossy())?
        .canonicalize()
        .map_err(AppError::from)?;
    if requested != discovered {
        return Err(AppError::InvalidInput(
            "the requested launcher descriptor is not the current bounded candidate".into(),
        ));
    }
    Ok(Some(requested))
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
async fn run_3d_health_check(project_root: String) -> Result<WorkflowHealthResult, String> {
    let result =
        tauri::async_runtime::spawn_blocking(move || run_3d_health_check_blocking(project_root))
            .await;
    Ok(match result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => three_d_failure(error),
        Err(_) => three_d_failure("the 3D setup check stopped unexpectedly".into()),
    })
}

#[tauri::command(async)]
async fn inspect_local_portrait_provider(
    configured_root: Option<String>,
    server_url: String,
) -> Result<crate::portraits::LocalPortraitDiscovery, String> {
    run_blocking_command("portrait-local-discovery", move || {
        Ok(crate::portraits::discover_local(
            configured_root.as_deref(),
            &server_url,
        ))
    })
    .await
}

#[tauri::command(async)]
async fn install_local_portrait_workflows(
    comfyui_root: String,
) -> Result<crate::portraits::LocalPortraitInstallResult, String> {
    run_blocking_command("portrait-local-install", move || {
        crate::portraits::install_current_workflows(&comfyui_root).map_err(command_error)
    })
    .await
}

fn three_d_failure(error: String) -> WorkflowHealthResult {
    WorkflowHealthResult {
        status: "incomplete".into(),
        exit_code: None,
        timed_out: error.to_ascii_lowercase().contains("timed out"),
        stdout: String::new(),
        stderr: bound_process_output(redact_secrets(&error, &[])),
    }
}

fn run_reviewed_post_install_actions(
    root: &Path,
    plan: &InstallationPlan,
    component_id: &str,
) -> Result<PostInstallActionOutcome, AppError> {
    // A removal sets nothing up. Its only reviewed action is the cleanup of
    // the GPTImage dependency tree that Setup itself installed.
    if plan.maintenance_mode.as_deref() == Some("remove")
        && !(component_id == IMAGEGEN_GPTIMAGE_COMPONENT_ID
            && plan
                .external_actions
                .iter()
                .any(|action| action.id == "external.imagegen.gptimage.cleanup"))
    {
        return Err(AppError::Transaction(
            "managed removal cannot run a post-install action".into(),
        ));
    }
    if component_id == crate::mcp::COMPONENT_ID {
        return run_reviewed_mcp_bootstrap(root, plan);
    }
    if component_id == LIVE_TOOLS_COMPONENT_ID {
        return run_reviewed_live_tools_bootstrap(root, plan);
    }
    if component_id == IMAGEGEN_GPTIMAGE_COMPONENT_ID {
        if plan
            .external_actions
            .iter()
            .any(|action| action.id == "external.imagegen.gptimage.cleanup")
        {
            return run_reviewed_gptimage_cleanup(root, plan);
        }
        return run_reviewed_gptimage_bootstrap(root, plan);
    }
    if component_id != "workflow.3d" {
        return Err(AppError::Transaction(format!(
            "unknown reviewed post-install component: {component_id}"
        )));
    }
    if Platform::current() != Platform::Windows {
        return Ok(PostInstallActionOutcome {
            component_id: "workflow.3d".into(),
            state: "unsupported_platform".into(),
            evidence: "the verified source declares no non-Windows 3D bootstrap route".into(),
        });
    }
    let actions = plan
        .external_actions
        .iter()
        .filter(|action| action.component_id == "workflow.3d")
        .collect::<Vec<_>>();
    if actions.len() != 1 {
        return Err(AppError::Transaction(
            "the reviewed plan does not contain exactly one 3D bootstrap action".into(),
        ));
    }
    let action = actions[0];
    let expected_arguments = [
        ".tools/3d_pipeline/bootstrap_3d_workflow.py",
        "--project-root",
        "<project_root>",
        "--verify-reviewed-config",
        "--quiet",
    ];
    if action.command_source != "repository_script"
        || action.platform != Platform::Windows
        || action.arguments
            != expected_arguments
                .into_iter()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        || action.network_access == "not_declared"
        || action.expected_writes.is_empty()
        || action.privilege != "current_user"
        || action.rollback_boundary == "not_declared_by_source"
    {
        return Err(AppError::Transaction(
            "the reviewed 3D bootstrap action is incomplete or changed".into(),
        ));
    }
    let target = expected_arguments[0];
    let operation = plan
        .operations
        .iter()
        .find(|operation| {
            !operation.external
                && operation.component_id == "workflow.3d"
                && operation.destination.eq_ignore_ascii_case(target)
        })
        .ok_or_else(|| {
            AppError::Transaction(
                "the reviewed 3D bootstrap has no managed operation evidence".into(),
            )
        })?;
    let script_bytes = crate::flatten::read_regular_file_no_follow_under_root(root, target)?;
    let expected_hash = operation
        .result_sha256
        .as_deref()
        .or(operation.source_sha256.as_deref())
        .ok_or_else(|| {
            AppError::Transaction("the reviewed 3D bootstrap has no checksum evidence".into())
        })?;
    if sha256_bytes(&script_bytes) != expected_hash
        || operation
            .source_size
            .is_some_and(|expected| expected != script_bytes.len() as u64)
    {
        return Err(AppError::PathSecurity(
            "the reviewed 3D bootstrap changed before execution".into(),
        ));
    }
    let Some(reference) = plan
        .credential_references
        .iter()
        .find(|reference| reference.name == MESHY_ENVIRONMENT_NAME)
    else {
        return Ok(PostInstallActionOutcome {
            component_id: "workflow.3d".into(),
            state: "incomplete".into(),
            evidence: "MESHY_API_KEY was not available in the OS credential vault".into(),
        });
    };
    validate_credential_reference(reference)?;
    let environment = match ScopedSecretEnvironment::from_credential(
        &OsCredentialStore,
        reference,
        MESHY_ENVIRONMENT_NAME,
    ) {
        Ok(environment) => environment,
        Err(_) => {
            return Ok(PostInstallActionOutcome {
                component_id: "workflow.3d".into(),
                state: "incomplete".into(),
                evidence: "MESHY_API_KEY could not be loaded from the OS credential vault".into(),
            });
        }
    };
    let python = match crate::process::find_path_executable(&["python.exe", "python"]) {
        Ok(python) => python,
        Err(_) => {
            return Ok(PostInstallActionOutcome {
                component_id: "workflow.3d".into(),
                state: "incomplete".into(),
                evidence: "Python is unavailable for the source-declared 3D bootstrap".into(),
            });
        }
    };
    if crate::process::validate_executable_publisher(&python, "Python Software Foundation").is_err()
    {
        return Ok(PostInstallActionOutcome {
            component_id: "workflow.3d".into(),
            state: "incomplete".into(),
            evidence: "Python publisher validation did not pass".into(),
        });
    }
    let private_script = create_private_verified_script(&script_bytes)?;
    let spec = crate::process::ProcessSpec {
        executable: python.clone(),
        executable_sha256: Some(sha256_file(&python)?),
        args: vec![
            "-I".into(),
            private_script.display().to_string(),
            "--project-root".into(),
            root.display().to_string(),
            "--verify-reviewed-config".into(),
            "--quiet".into(),
        ],
        cwd: Some(root.to_path_buf()),
        platform: Platform::Windows,
        environment_names: vec![MESHY_ENVIRONMENT_NAME.into()],
        timeout_seconds: 30 * 60,
        max_output_bytes: 2 * 1024 * 1024,
    };
    let run_result = spec.run(&[python], Some(&environment));
    let cleanup_result = remove_private_verified_script(&private_script);
    let result = run_result;
    cleanup_result?;
    let result = match result {
        Ok(result) => result,
        Err(_) => {
            return Ok(PostInstallActionOutcome {
                component_id: "workflow.3d".into(),
                state: "incomplete".into(),
                evidence: "the source-declared 3D bootstrap could not be started".into(),
            });
        }
    };
    let state = if result.status_code == Some(0) && !result.timed_out {
        "ready"
    } else {
        "incomplete"
    };
    Ok(PostInstallActionOutcome {
        component_id: "workflow.3d".into(),
        state: state.into(),
        evidence: format!(
            "bootstrap exit={} timed_out={}",
            result
                .status_code
                .map_or_else(|| "none".into(), |code| code.to_string()),
            result.timed_out
        ),
    })
}

const IMAGEGEN_GPTIMAGE_COMPONENT_ID: &str = "imagegen.gptimage";
const IMAGEGEN_GPTIMAGE_BOOTSTRAP: &str = ".tools/image_generation/providers/gptimage/bootstrap.py";
const IMAGEGEN_GPTIMAGE_PACKAGE_ROOT: &str = ".tools/image_generation/providers/gptimage";
const IMAGEGEN_GPTIMAGE_NODE_MODULES: &str =
    ".tools/image_generation/providers/gptimage/node_modules";

fn image_provider_selected(selection: Option<&ImageGenerationSelection>, id: &str) -> bool {
    selection.is_some_and(|selection| {
        selection.primary_provider == id
            || selection
                .additional_providers
                .iter()
                .any(|candidate| candidate == id)
    })
}

/// Whether a plan installs this image provider. Removal only reverses what is
/// installed, so it never plans a provider bootstrap; a removal that carried
/// one would stop being a pure removal and wrongly ask for a provider analysis.
fn provider_bootstrap_selected(
    mode: &str,
    selection: Option<&ImageGenerationSelection>,
    id: &str,
) -> bool {
    mode != "remove" && image_provider_selected(selection, id)
}

fn reviewed_gptimage_cleanup_action() -> ExternalAction {
    ExternalAction {
        id: "external.imagegen.gptimage.cleanup".into(),
        component_id: IMAGEGEN_GPTIMAGE_COMPONENT_ID.into(),
        platform: Platform::current(),
        command_source: "app_managed_cleanup".into(),
        executable: Some("HOI4 Mod Setup".into()),
        arguments: vec![IMAGEGEN_GPTIMAGE_NODE_MODULES.into()],
        working_directory: Some("<project_root>".into()),
        environment_names: Vec::new(),
        network_access: "none".into(),
        expected_writes: vec![format!("{IMAGEGEN_GPTIMAGE_NODE_MODULES}/**")],
        privilege: "current_user".into(),
        rollback_boundary: "Removes only the exact dependency tree that Setup recorded after GPTImage installation. A changed tree is kept.".into(),
        display_command: Some("Remove the GPTImage server dependencies from this project if they match Setup's saved integrity hash.".into()),
        risk: "medium".into(),
        requires_approval: true,
        contains_secret: false,
        verified_executable_sha256: None,
        verified_executable_size: None,
        verified_interpreter_sha256: None,
        verified_interpreter_size: None,
        verified_runtime_sha256: None,
        verified_runtime_size: None,
        verified_package_symlinks: Default::default(),
        verified_package_name: None,
        verified_package_version: None,
        verified_package_integrity: None,
        verified_package_tree_sha256: None,
        verified_package_file_count: None,
        verified_runtime_entry: None,
        required_tool_names: Vec::new(),
    }
}

fn gptimage_bootstrap_outcome(
    state: &str,
    evidence: impl Into<String>,
) -> PostInstallActionOutcome {
    PostInstallActionOutcome {
        component_id: IMAGEGEN_GPTIMAGE_COMPONENT_ID.into(),
        state: state.into(),
        evidence: evidence.into(),
    }
}

fn expected_gptimage_file_hash(
    plan: &InstallationPlan,
    root: &Path,
    relative: &str,
) -> Result<String, AppError> {
    if let Some(operation) = plan.operations.iter().find(|operation| {
        operation.component_id == IMAGEGEN_GPTIMAGE_COMPONENT_ID
            && operation.destination == relative
    }) {
        return operation
            .result_sha256
            .clone()
            .or_else(|| operation.source_sha256.clone())
            .ok_or_else(|| {
                AppError::Transaction("the reviewed GPTImage file has no checksum".into())
            });
    }
    let lock = read_project_lock(root)?;
    lock.files
        .iter()
        .find(|file| file.component_id == IMAGEGEN_GPTIMAGE_COMPONENT_ID && file.path == relative)
        .map(|file| file.installed_sha256.clone())
        .ok_or_else(|| {
            AppError::Transaction("the installed GPTImage file has no lock evidence".into())
        })
}

fn run_reviewed_gptimage_bootstrap(
    root: &Path,
    plan: &InstallationPlan,
) -> Result<PostInstallActionOutcome, AppError> {
    let actions = plan
        .external_actions
        .iter()
        .filter(|action| action.component_id == IMAGEGEN_GPTIMAGE_COMPONENT_ID)
        .collect::<Vec<_>>();
    if actions.len() != 1 {
        return Err(AppError::Transaction(
            "the reviewed GPTImage setup action is missing or duplicated".into(),
        ));
    }
    let action = actions[0];
    let reviewed_arguments = &action.arguments;
    let expected_tree_sha256 = reviewed_arguments
        .get(4)
        .filter(|value| Regex::new(r"^[0-9a-f]{64}$").unwrap().is_match(value))
        .ok_or_else(|| {
            AppError::Transaction(
                "the reviewed GPTImage source-tree hash is missing or invalid".into(),
            )
        })?;
    let expected_lock_sha256 = reviewed_arguments
        .get(6)
        .filter(|value| Regex::new(r"^[0-9a-f]{64}$").unwrap().is_match(value))
        .ok_or_else(|| {
            AppError::Transaction(
                "the reviewed GPTImage lockfile hash is missing or invalid".into(),
            )
        })?;
    if action.command_source != "repository_script"
        || action.platform != Platform::current()
        || action.arguments.len() != 7
        || action.arguments.first().map(String::as_str) != Some(IMAGEGEN_GPTIMAGE_BOOTSTRAP)
        || action.arguments.get(1).map(String::as_str) != Some("--package-root")
        || action.arguments.get(2).map(String::as_str)
            != Some("<project_root>/.tools/image_generation/providers/gptimage")
        || action.arguments.get(3).map(String::as_str) != Some("--expected-tree-sha256")
        || action.arguments.get(5).map(String::as_str) != Some("--expected-lock-sha256")
        || action.network_access == "not_declared"
        || action.expected_writes.is_empty()
        || action.privilege != "current_user"
        || action.rollback_boundary == "not_declared_by_source"
        || action.verified_package_name.as_deref() != Some("gptimage")
        || action.verified_package_version.as_deref() != Some("0.1.0")
    {
        return Err(AppError::Transaction(
            "the reviewed GPTImage setup action is incomplete or changed".into(),
        ));
    }
    if !action
        .expected_writes
        .iter()
        .any(|path| path == ".tools/image_generation/providers/gptimage/node_modules/**")
        || !action
            .network_access
            .to_ascii_lowercase()
            .contains("npm registry")
        || !action
            .rollback_boundary
            .to_ascii_lowercase()
            .contains("node_modules")
    {
        return Err(AppError::Transaction(
            "the reviewed GPTImage network, write, or rollback boundary is incomplete".into(),
        ));
    }
    let package_root = safe_join(root, IMAGEGEN_GPTIMAGE_PACKAGE_ROOT)?;
    let package_json_path = format!("{IMAGEGEN_GPTIMAGE_PACKAGE_ROOT}/package.json");
    let package_json =
        crate::flatten::read_regular_file_no_follow_under_root(root, &package_json_path)?;
    if sha256_bytes(&package_json) != expected_gptimage_file_hash(plan, root, &package_json_path)? {
        return Err(AppError::PathSecurity(
            "the installed GPTImage package metadata changed after review".into(),
        ));
    }
    let package: Value = serde_json::from_slice(&package_json).map_err(|_| {
        AppError::Source("the installed GPTImage package metadata is invalid".into())
    })?;
    if package.get("name").and_then(Value::as_str) != Some("gptimage")
        || package.get("version").and_then(Value::as_str) != Some("0.1.0")
    {
        return Err(AppError::PathSecurity(
            "the installed GPTImage package does not match its reviewed version".into(),
        ));
    }
    let package_lock_path = format!("{IMAGEGEN_GPTIMAGE_PACKAGE_ROOT}/package-lock.json");
    let expected_lock_hash = expected_gptimage_file_hash(plan, root, &package_lock_path)?;
    if expected_lock_sha256 != &expected_lock_hash {
        return Err(AppError::Transaction(
            "the reviewed GPTImage lockfile hash differs from the selected package file".into(),
        ));
    }
    let lock_bytes =
        crate::flatten::read_regular_file_no_follow_under_root(root, &package_lock_path)?;
    if sha256_bytes(&lock_bytes) != expected_lock_hash {
        return Err(AppError::PathSecurity(
            "the installed GPTImage package lock changed after review".into(),
        ));
    }
    let script_bytes =
        crate::flatten::read_regular_file_no_follow_under_root(root, IMAGEGEN_GPTIMAGE_BOOTSTRAP)?;
    let expected_script_hash =
        expected_gptimage_file_hash(plan, root, IMAGEGEN_GPTIMAGE_BOOTSTRAP)?;
    if sha256_bytes(&script_bytes) != expected_script_hash {
        return Err(AppError::PathSecurity(
            "the GPTImage bootstrap script changed after review".into(),
        ));
    }
    let python = match crate::process::find_path_executable(&[
        "python.exe",
        "python3.exe",
        "python",
        "python3",
    ]) {
        Ok(path) => path,
        Err(_) => {
            return Ok(gptimage_bootstrap_outcome(
                "incomplete",
                "Python 3 is unavailable for GPTImage setup",
            ))
        }
    };
    if matches!(Platform::current(), Platform::Windows | Platform::Linux)
        && crate::process::validate_executable_publisher(&python, "Python Software Foundation")
            .is_err()
    {
        return Ok(gptimage_bootstrap_outcome(
            "incomplete",
            "Python publisher validation did not pass for GPTImage setup",
        ));
    }
    let node_modules_path = package_root.join("node_modules");
    let previous_tree_hash = plan
        .image_generation
        .as_ref()
        .and_then(|selection| selection.installed_package_tree_sha256.as_deref());
    let previous_lock_hash = plan
        .image_generation
        .as_ref()
        .and_then(|selection| selection.installed_package_lock_sha256.as_deref());
    if node_modules_path.is_dir() && previous_lock_hash == Some(expected_lock_hash.as_str()) {
        if let (Some(expected), Ok(actual)) = (
            previous_tree_hash,
            crate::mcp::package_tree_sha256(&node_modules_path),
        ) {
            if expected == actual {
                return Ok(gptimage_bootstrap_outcome(
                    "ready",
                    format!(
                        "package-tree-sha256:{actual};package-lock-sha256:{expected_lock_hash}"
                    ),
                ));
            }
        }
    }
    let node = match crate::process::find_path_executable(&["node.exe", "node"]) {
        Ok(path) => path,
        Err(_) => {
            return Ok(gptimage_bootstrap_outcome(
                "incomplete",
                "Node.js LTS is unavailable for GPTImage setup",
            ))
        }
    };
    if matches!(Platform::current(), Platform::Windows | Platform::Linux)
        && crate::process::validate_executable_publisher(&node, "OpenJS Foundation").is_err()
    {
        return Ok(gptimage_bootstrap_outcome(
            "incomplete",
            "Node.js publisher validation did not pass for GPTImage setup",
        ));
    }
    let node_version = crate::process::ProcessSpec {
        executable: node.clone(),
        executable_sha256: Some(sha256_file(&node)?),
        args: vec!["--version".into()],
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: Vec::new(),
        timeout_seconds: 10,
        max_output_bytes: 4096,
    }
    .run(std::slice::from_ref(&node), None);
    let major = node_version.ok().and_then(|result| {
        result
            .stdout
            .trim()
            .strip_prefix('v')
            .and_then(|version| version.split('.').next())
            .and_then(|value| value.parse::<u64>().ok())
    });
    if major.is_none_or(|major| major < 20) {
        return Ok(gptimage_bootstrap_outcome(
            "incomplete",
            "Node.js 20 or newer is required for GPTImage setup",
        ));
    }
    let private_script = create_private_verified_script(&script_bytes)?;
    let spec = crate::process::ProcessSpec {
        executable: python.clone(),
        executable_sha256: Some(sha256_file(&python)?),
        args: vec![
            "-I".into(),
            private_script.display().to_string(),
            "--package-root".into(),
            package_root.display().to_string(),
            "--expected-tree-sha256".into(),
            expected_tree_sha256.clone(),
            "--expected-lock-sha256".into(),
            expected_lock_sha256.clone(),
        ],
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: Vec::new(),
        timeout_seconds: 12 * 60,
        max_output_bytes: 128 * 1024,
    };
    let run = spec.run(&[python], None);
    remove_private_verified_script(&private_script)?;
    Ok(match run {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => {
            match crate::mcp::package_tree_sha256(&node_modules_path) {
                Ok(hash) => gptimage_bootstrap_outcome(
                    "ready",
                    format!("package-tree-sha256:{hash};package-lock-sha256:{expected_lock_hash}"),
                ),
                Err(error) => gptimage_bootstrap_outcome(
                    "incomplete",
                    format!(
                        "the installed GPTImage dependencies could not be verified: {}",
                        redact_secrets(&error.to_string(), &[])
                    ),
                ),
            }
        }
        failure => gptimage_bootstrap_outcome(
            "incomplete",
            bootstrap_failure_details("GPTImage setup", failure),
        ),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GptimageCleanupResult {
    Removed,
    AlreadyAbsent,
    ModifiedKept,
}

fn remove_gptimage_dependencies_if_unchanged(
    root: &Path,
    expected_tree: &str,
) -> Result<GptimageCleanupResult, AppError> {
    crate::source::validate_sha256(expected_tree).map_err(|_| {
        AppError::Transaction("the installed GPTImage dependency hash is invalid".into())
    })?;
    let node_modules = safe_join(root, IMAGEGEN_GPTIMAGE_NODE_MODULES)?;
    let metadata = match std::fs::symlink_metadata(&node_modules) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GptimageCleanupResult::AlreadyAbsent)
        }
        Err(error) => {
            return Err(AppError::Transaction(format!(
                "could not inspect GPTImage dependencies: {error}"
            )))
        }
    };
    if crate::security::is_link_metadata(&metadata) || !metadata.is_dir() {
        return Ok(GptimageCleanupResult::ModifiedKept);
    }
    let opened = match RootedDir::open_read(&node_modules) {
        Ok(opened) => opened,
        Err(_) => return Ok(GptimageCleanupResult::ModifiedKept),
    };
    let identity = opened.identity_token()?;
    let actual_tree = match crate::mcp::package_tree_sha256(&node_modules) {
        Ok(hash) => hash,
        Err(_) => return Ok(GptimageCleanupResult::ModifiedKept),
    };
    let reopened = match RootedDir::open_read(&node_modules) {
        Ok(reopened) => reopened,
        Err(_) => return Ok(GptimageCleanupResult::ModifiedKept),
    };
    if reopened.identity_token()? != identity || actual_tree != expected_tree {
        return Ok(GptimageCleanupResult::ModifiedKept);
    }
    let package_root = safe_join(root, IMAGEGEN_GPTIMAGE_PACKAGE_ROOT)?;
    let package_directory = RootedDir::open(&package_root)?;
    if !package_directory.remove_tree_if_identity("node_modules", &identity)? {
        return Ok(GptimageCleanupResult::ModifiedKept);
    }
    Ok(GptimageCleanupResult::Removed)
}

fn run_reviewed_gptimage_cleanup(
    root: &Path,
    plan: &InstallationPlan,
) -> Result<PostInstallActionOutcome, AppError> {
    let actions = plan
        .external_actions
        .iter()
        .filter(|action| action.id == "external.imagegen.gptimage.cleanup")
        .collect::<Vec<_>>();
    if actions.len() != 1 {
        return Err(AppError::Transaction(
            "the reviewed GPTImage cleanup action is missing or duplicated".into(),
        ));
    }
    let action = actions[0];
    if action.component_id != IMAGEGEN_GPTIMAGE_COMPONENT_ID
        || action.command_source != "app_managed_cleanup"
        || action.platform != Platform::current()
        || action.arguments.as_slice() != [IMAGEGEN_GPTIMAGE_NODE_MODULES]
        || action.network_access != "none"
        || action.privilege != "current_user"
        || action.expected_writes.as_slice() != [format!("{IMAGEGEN_GPTIMAGE_NODE_MODULES}/**")]
    {
        return Err(AppError::Transaction(
            "the reviewed GPTImage cleanup action is incomplete or changed".into(),
        ));
    }
    let Some(expected_tree) = plan
        .image_generation
        .as_ref()
        .and_then(|selection| selection.installed_package_tree_sha256.as_deref())
    else {
        return Ok(gptimage_bootstrap_outcome(
            "not_selected",
            "no Setup-managed GPTImage dependency tree was recorded; nothing was removed",
        ));
    };
    crate::source::validate_sha256(expected_tree).map_err(|_| {
        AppError::Transaction("the installed GPTImage dependency hash is invalid".into())
    })?;
    match remove_gptimage_dependencies_if_unchanged(root, expected_tree)? {
        GptimageCleanupResult::Removed => Ok(gptimage_bootstrap_outcome(
            "not_selected",
            "package-tree-removed",
        )),
        GptimageCleanupResult::AlreadyAbsent => Ok(gptimage_bootstrap_outcome(
            "not_selected",
            "GPTImage dependencies were already absent",
        )),
        GptimageCleanupResult::ModifiedKept => Ok(gptimage_bootstrap_outcome(
            "not_selected",
            "GPTImage dependencies changed after Setup installed them; the modified tree was kept",
        )),
    }
}

/// Why the MCP bootstrap cannot use a Python on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpPython {
    Missing,
    PublisherNotVerified,
}

/// The Python the source-declared MCP bootstrap runs with. The bootstrap and
/// the requirement check before installation share this lookup so a check that
/// passes means the bootstrap finds the same interpreter.
fn mcp_bootstrap_python() -> Result<PathBuf, McpPython> {
    let python = crate::process::find_path_executable(&["python3", "python.exe", "python"])
        .map_err(|_| McpPython::Missing)?;
    crate::process::validate_executable_publisher(&python, "Python Software Foundation")
        .map_err(|_| McpPython::PublisherNotVerified)?;
    Ok(python)
}

/// A tool the reviewed plan needs on this computer. It is checked before any
/// project file changes, so a missing tool is fixed on the review screen
/// instead of stopping the installation after files were written.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
struct SetupRequirement {
    id: String,
    label: String,
    /// `ready` or `missing`.
    status: String,
    /// What was found and, when missing, the safe next step.
    detail: String,
}

const PYTHON_REQUIREMENT_ID: &str = "python.agent_tools_bootstrap";

fn python_requirement(check: Result<PathBuf, McpPython>) -> SetupRequirement {
    let (status, detail) = match check {
        Ok(_) => ("ready", "Python from python.org was found.".to_string()),
        Err(McpPython::Missing) => (
            "missing",
            "Python 3.9 or newer was not found. Install it from python.org and tick \u{201c}Add python.exe to PATH\u{201d}, then choose Check again.".to_string(),
        ),
        Err(McpPython::PublisherNotVerified) => (
            "missing",
            "The Python on this computer is not the python.org build, which HOI4 Agent Tools requires. Install Python from python.org, then choose Check again.".to_string(),
        ),
    };
    SetupRequirement {
        id: PYTHON_REQUIREMENT_ID.into(),
        label: "Python for HOI4 Agent Tools".into(),
        status: status.into(),
        detail,
    }
}

/// Tools the plan's post-install setup runs need. Nothing here changes the
/// project; it only reports what the installation would fail on.
fn setup_requirements(plan: &InstallationPlan) -> Vec<SetupRequirement> {
    let mut requirements = Vec::new();
    let mcp_runs = plan.maintenance_mode.as_deref() != Some("remove")
        && matches!(Platform::current(), Platform::Windows | Platform::Linux)
        && plan
            .external_actions
            .iter()
            .any(|action| action.component_id == "mcp.hoi4_agent_tools.bootstrap")
        && plan
            .optional_workflows
            .get(crate::mcp::COMPONENT_ID)
            .is_some_and(|state| {
                matches!(state.as_str(), "selected_pending" | "incomplete" | "ready")
            });
    if mcp_runs {
        requirements.push(python_requirement(mcp_bootstrap_python()));
    }
    requirements
}

fn ensure_setup_requirements_met(plan: &InstallationPlan) -> Result<(), String> {
    let missing = setup_requirements(plan)
        .into_iter()
        .filter(|requirement| requirement.status != "ready")
        .map(|requirement| format!("{}: {}", requirement.label, requirement.detail))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Setup needs something that is missing on this computer. {} Nothing was changed.",
            missing.join(" ")
        ))
    }
}

#[tauri::command(async)]
fn check_installation_requirements(plan_id: String) -> Result<Vec<SetupRequirement>, String> {
    let id = Uuid::parse_str(&plan_id).map_err(|_| "invalid installation plan ID".to_string())?;
    let plan = prepared_plans()
        .lock()
        .map_err(|_| "prepared plan store is unavailable".to_string())?
        .get(&id)
        .map(|prepared| prepared.plan.clone())
        .ok_or_else(|| "installation plan is not available in the core session".to_string())?;
    Ok(setup_requirements(&plan))
}

fn run_reviewed_mcp_bootstrap(
    root: &Path,
    plan: &InstallationPlan,
) -> Result<PostInstallActionOutcome, AppError> {
    if !matches!(Platform::current(), Platform::Windows | Platform::Linux) {
        return Ok(PostInstallActionOutcome {
            component_id: crate::mcp::COMPONENT_ID.into(),
            state: "unsupported_platform".into(),
            evidence: "the verified source declares no non-Windows HOI4 Agent Tools route".into(),
        });
    }
    let actions = plan
        .external_actions
        .iter()
        .filter(|action| action.component_id == "mcp.hoi4_agent_tools.bootstrap")
        .collect::<Vec<_>>();
    if actions.len() != 1 {
        return Err(AppError::Transaction(
            "the reviewed plan does not contain exactly one MCP bootstrap action".into(),
        ));
    }
    let action = actions[0];
    let expected_arguments = [".tools/mcp/bootstrap_hoi4_agent_tools.py", "--quiet"];
    if action.command_source != "repository_script"
        || action.platform != Platform::current()
        || action.arguments
            != expected_arguments
                .into_iter()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        || action.network_access == "not_declared"
        || action.expected_writes.is_empty()
        || action.privilege != "current_user"
        || action.rollback_boundary == "not_declared_by_source"
    {
        return Err(AppError::Transaction(
            "the reviewed MCP bootstrap action is incomplete or changed".into(),
        ));
    }
    let target = expected_arguments[0];
    let operation = plan
        .operations
        .iter()
        .find(|operation| {
            !operation.external
                && operation.component_id == "mcp.hoi4_agent_tools.bootstrap"
                && operation.destination.eq_ignore_ascii_case(target)
        })
        .ok_or_else(|| {
            AppError::Transaction(
                "the reviewed MCP bootstrap has no managed operation evidence".into(),
            )
        })?;
    let script_bytes = crate::flatten::read_regular_file_no_follow_under_root(root, target)?;
    let expected_hash = operation
        .result_sha256
        .as_deref()
        .or(operation.source_sha256.as_deref())
        .ok_or_else(|| {
            AppError::Transaction("the reviewed MCP bootstrap has no checksum evidence".into())
        })?;
    if sha256_bytes(&script_bytes) != expected_hash
        || operation
            .source_size
            .is_some_and(|expected| expected != script_bytes.len() as u64)
    {
        return Err(AppError::PathSecurity(
            "the reviewed MCP bootstrap changed before execution".into(),
        ));
    }
    let python = match mcp_bootstrap_python() {
        Ok(python) => python,
        Err(McpPython::Missing) => {
            return Ok(PostInstallActionOutcome {
                component_id: crate::mcp::COMPONENT_ID.into(),
                state: "incomplete".into(),
                evidence: "Python is unavailable for the source-declared MCP bootstrap".into(),
            });
        }
        Err(McpPython::PublisherNotVerified) => {
            return Ok(PostInstallActionOutcome {
                component_id: crate::mcp::COMPONENT_ID.into(),
                state: "incomplete".into(),
                evidence: "Python publisher validation did not pass for the MCP bootstrap".into(),
            });
        }
    };
    let private_script = create_private_verified_script(&script_bytes)?;
    let spec = crate::process::ProcessSpec {
        executable: python.clone(),
        executable_sha256: Some(sha256_file(&python)?),
        args: vec![
            "-I".into(),
            private_script.display().to_string(),
            "--quiet".into(),
        ],
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: vec![],
        timeout_seconds: 30 * 60,
        max_output_bytes: 2 * 1024 * 1024,
    };
    let run_result = spec.run(&[python], None);
    let cleanup_result = remove_private_verified_script(&private_script);
    cleanup_result?;
    let result = match run_result {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => result,
        failure => {
            return Ok(PostInstallActionOutcome {
                component_id: crate::mcp::COMPONENT_ID.into(),
                state: "incomplete".into(),
                evidence: mcp_bootstrap_failure_details(failure),
            });
        }
    };
    let health = crate::mcp::reviewed_plan_target(&plan.external_actions)
        .and_then(|target| crate::mcp::initialize_health(root, &target));
    Ok(match health {
        Ok(evidence) => PostInstallActionOutcome {
            component_id: crate::mcp::COMPONENT_ID.into(),
            state: "ready".into(),
            evidence: format!(
                "package bootstrap exit={} verified_tools={}",
                result.status_code.unwrap_or_default(),
                evidence.required_tools.len()
            ),
        },
        Err(error) => PostInstallActionOutcome {
            component_id: crate::mcp::COMPONENT_ID.into(),
            state: "incomplete".into(),
            evidence: format!(
                "package installed but MCP health failed: {}",
                redact_secrets(&error.to_string(), &[])
            ),
        },
    })
}

// The optional HOI4 Live Tools workflow. Its reviewed action runs the managed
// bootstrap script, which installs the release the package pins for the
// current user and registers it in the user-level client configurations.
use crate::mcp::LIVE_TOOLS_COMPONENT_ID;
const LIVE_TOOLS_SCRIPT_COMPONENT_ID: &str = "mcp.hoi4_live.bootstrap";
const LIVE_TOOLS_SCRIPT: &str = ".tools/mcp/bootstrap_hoi4_live_tools.py";

fn live_tools_outcome(state: &str, evidence: impl Into<String>) -> PostInstallActionOutcome {
    PostInstallActionOutcome {
        component_id: LIVE_TOOLS_COMPONENT_ID.into(),
        state: state.into(),
        evidence: evidence.into(),
    }
}

fn run_reviewed_live_tools_bootstrap(
    root: &Path,
    plan: &InstallationPlan,
) -> Result<PostInstallActionOutcome, AppError> {
    if Platform::current() != Platform::Windows {
        return Ok(live_tools_outcome(
            "unsupported_platform",
            "the verified source declares no non-Windows HOI4 Live Tools route",
        ));
    }
    let actions = plan
        .external_actions
        .iter()
        .filter(|action| action.component_id == LIVE_TOOLS_COMPONENT_ID)
        .collect::<Vec<_>>();
    if actions.len() != 1 {
        return Err(AppError::Transaction(
            "the reviewed plan does not contain exactly one HOI4 Live Tools bootstrap action"
                .into(),
        ));
    }
    let action = actions[0];
    let expected_arguments = [LIVE_TOOLS_SCRIPT, "--quiet", "--register", "none"];
    if action.command_source != "repository_script"
        || action.platform != Platform::Windows
        || action.arguments
            != expected_arguments
                .into_iter()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        || action.network_access == "not_declared"
        || action.expected_writes.is_empty()
        || action.privilege != "current_user"
        || action.rollback_boundary == "not_declared_by_source"
    {
        return Err(AppError::Transaction(
            "the reviewed HOI4 Live Tools bootstrap action is incomplete or changed".into(),
        ));
    }
    let operation = plan
        .operations
        .iter()
        .find(|operation| {
            !operation.external
                && operation.component_id == LIVE_TOOLS_SCRIPT_COMPONENT_ID
                && operation
                    .destination
                    .eq_ignore_ascii_case(LIVE_TOOLS_SCRIPT)
        })
        .ok_or_else(|| {
            AppError::Transaction(
                "the reviewed HOI4 Live Tools bootstrap has no managed operation evidence".into(),
            )
        })?;
    let script_bytes =
        crate::flatten::read_regular_file_no_follow_under_root(root, LIVE_TOOLS_SCRIPT)?;
    let expected_hash = operation
        .result_sha256
        .as_deref()
        .or(operation.source_sha256.as_deref())
        .ok_or_else(|| {
            AppError::Transaction(
                "the reviewed HOI4 Live Tools bootstrap has no checksum evidence".into(),
            )
        })?;
    if sha256_bytes(&script_bytes) != expected_hash
        || operation
            .source_size
            .is_some_and(|expected| expected != script_bytes.len() as u64)
    {
        return Err(AppError::PathSecurity(
            "the reviewed HOI4 Live Tools bootstrap changed before execution".into(),
        ));
    }
    let python = match crate::process::find_path_executable(&["python.exe", "python"]) {
        Ok(python) => python,
        Err(_) => {
            return Ok(live_tools_outcome(
                "incomplete",
                "Python 3.9 or newer is unavailable for the HOI4 Live Tools installer",
            ));
        }
    };
    if crate::process::validate_executable_publisher(&python, "Python Software Foundation").is_err()
    {
        return Ok(live_tools_outcome(
            "incomplete",
            "Python publisher validation did not pass for the HOI4 Live Tools installer",
        ));
    }
    let private_script = create_private_verified_script(&script_bytes)?;
    let spec = crate::process::ProcessSpec {
        executable: python.clone(),
        executable_sha256: Some(sha256_file(&python)?),
        args: vec![
            "-I".into(),
            private_script.display().to_string(),
            "--quiet".into(),
            "--register".into(),
            "none".into(),
        ],
        cwd: Some(root.to_path_buf()),
        platform: Platform::Windows,
        environment_names: vec![],
        timeout_seconds: 30 * 60,
        max_output_bytes: 2 * 1024 * 1024,
    };
    let run_result = spec.run(&[python], None);
    remove_private_verified_script(&private_script)?;
    Ok(match run_result {
        Ok(result) if result.status_code == Some(0) && !result.timed_out => live_tools_outcome(
            "ready",
            "HOI4 Live Tools runtime installed and verified; project registration is managed by Setup",
        ),
        failure => live_tools_outcome(
            "incomplete",
            bootstrap_failure_details("HOI4 Live Tools setup", failure),
        ),
    })
}

fn mcp_bootstrap_failure_details(
    result: Result<crate::process::ProcessResult, AppError>,
) -> String {
    bootstrap_failure_details("MCP package setup", result)
}

fn bootstrap_failure_details(
    label: &str,
    result: Result<crate::process::ProcessResult, AppError>,
) -> String {
    let detail = match result {
        Err(error) => format!("{label} could not start: {error}"),
        Ok(result) => {
            let output = if result.stderr.trim().is_empty() {
                result.stdout.trim()
            } else {
                result.stderr.trim()
            };
            format!(
                "{label} failed (exit={}, timed_out={}): {}",
                result
                    .status_code
                    .map_or_else(|| "none".into(), |code| code.to_string()),
                result.timed_out,
                if output.is_empty() {
                    "the setup process returned no diagnostic output"
                } else {
                    output
                }
            )
        }
    };
    let mut detail = redact_secrets(&detail, &[]);
    const MAX_BYTES: usize = 2 * 1024;
    if detail.len() > MAX_BYTES {
        // Leave room to complete a redaction marker if truncation splits it.
        let mut end = MAX_BYTES - 32;
        while !detail.is_char_boundary(end) {
            end -= 1;
        }
        detail.truncate(end);
        detail.push_str(" ...");
        detail = redact_secrets(&detail, &[]);
    }
    detail
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
        args: vec![
            private_script.display().to_string(),
            "--project-root".into(),
            root.display().to_string(),
            "--verify-reviewed-config".into(),
            "--quiet".into(),
        ],
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
    let target = crate::mcp::manifest_target_for_platform(&manifest, Platform::current())?;
    let expected = mcp_client_launch(&target, Platform::current())?;
    let mut environments = vec![lock.primary_coding_environment.as_str()];
    environments.extend(
        lock.additional_coding_environments
            .iter()
            .map(String::as_str),
    );
    environments.sort();
    environments.dedup();
    for environment in environments {
        // Clients whose MCP servers live only in user or IDE settings are a
        // manual step reported by readiness; there is no project route to
        // verify for them.
        if matches!(
            coding_environment::mcp_support(&manifest, environment),
            Some(McpSupport::Manual | McpSupport::None)
        ) {
            continue;
        }
        let destination = coding_environment::native_client(environment)
            .and_then(|client| client.mcp)
            .map(|mcp| mcp.path)
            .ok_or_else(|| {
                AppError::Process(
                    "a selected coding client has no verified MCP registration route".into(),
                )
            })?;
        let bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
            project_root,
            destination,
            1024 * 1024,
        )?;
        if crate::mcp::client_registration(&bytes, destination)? != Some(expected.clone()) {
            return Err(AppError::Process("a selected coding client's MCP registration differs from the locked manifest launch".into()));
        }
    }
    Ok(target)
}

fn mcp_client_launch(
    target: &crate::mcp::VerifiedMcpTarget,
    platform: Platform,
) -> Result<(String, Vec<String>), AppError> {
    // The desktop's Linux route is the system Node and the per-user npm
    // prefix; a server supplies its own `NodeRuntime` through the service.
    let node = (platform == Platform::Linux)
        .then(NodeRuntime::linux_system)
        .transpose()?;
    launch_with_node(target, platform, node.as_ref())
}

#[tauri::command(async)]
async fn run_mcp_health_check(project_root: String) -> Result<WorkflowHealthResult, String> {
    run_blocking_command("mcp-health-check", move || {
        run_mcp_health_check_blocking(project_root)
    })
    .await
}

fn run_mcp_health_check_blocking(project_root: String) -> Result<WorkflowHealthResult, String> {
    if !matches!(Platform::current(), Platform::Windows | Platform::Linux) {
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
        || !matches!(Platform::current(), Platform::Windows | Platform::Linux)
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
        let report = evaluate_installed_readiness(
            &root,
            &input.project_id,
            input.workflow_3d_state,
            input.notes,
        )
        .map_err(command_error)?;
        if report.open_in_codex.enabled {
            let lock_hash = sha256_file(&crate::paths::lock_path(&root)).map_err(command_error)?;
            if let Ok(mut ready) = ready_projects().lock() {
                ready.insert(root.display().to_string(), lock_hash);
            }
        } else if let Ok(mut ready) = ready_projects().lock() {
            ready.remove(&root.display().to_string());
        }
        Ok(report)
    } else {
        Ok(crate::readiness::evaluate(&input))
    }
}

#[tauri::command(async)]
async fn preview_descriptors(state: Value) -> Result<Vec<GeneratedArtifact>, String> {
    run_blocking_command("descriptor-preview", move || {
        preview_descriptors_blocking(state)
    })
    .await
}

fn preview_descriptors_blocking(state: Value) -> Result<Vec<GeneratedArtifact>, String> {
    require_ai_session(&state).map_err(provider_analysis_user_error)?;
    let root = project_root_from_state(&state).map_err(provider_analysis_user_error)?;
    let _ = codex_analysis_from_state(&state, &root).map_err(provider_analysis_user_error)?;
    let identity =
        project_identity_from_state(&state, &root).map_err(provider_analysis_user_error)?;
    render_generated_artifacts(&identity).map_err(provider_analysis_user_error)
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

fn conflict_preview_kind(path: &str, kind: FileKind) -> FileKind {
    // Descriptor text can be reviewed without enabling automatic text merging.
    if matches!(kind, FileKind::Binary)
        && Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mod"))
    {
        FileKind::Text
    } else {
        kind
    }
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
    let kind = conflict_preview_kind(&operation.destination, kind);
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

#[tauri::command(async)]
async fn preview_source_manifest(
    source_mode: String,
    pinned_ref: String,
) -> Result<SourceManifestPreview, String> {
    run_blocking_command("source-preview", move || {
        preview_source_manifest_blocking(source_mode, pinned_ref)
    })
    .await
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
    let coding_environments = crate::coding_environment::definitions(&resolution.manifest);
    let unavailable_coding_environments = crate::coding_environment::unavailable_definitions(
        &resolution.manifest,
        Platform::current(),
    );
    Ok(SourceManifestPreview {
        schema_version: resolution.manifest.schema_version.clone(),
        manifest_id: resolution.manifest.manifest_id.clone(),
        source: resolution.identity,
        repository: resolution.manifest.repository.clone(),
        components: resolution.manifest.components,
        profiles: resolution.manifest.profiles,
        coding_environments,
        unavailable_coding_environments,
        image_generation: resolution.manifest.image_generation.clone(),
        live_testing: resolution.manifest.live_testing.clone(),
        local_source: crate::local_source::configured_repository()
            .map(|root| root.display().to_string()),
    })
}

fn image_generation_state_for_maintenance(
    supplied: Option<&Value>,
    lock: &InstallationLock,
    environments: &CodingEnvironmentSelection,
) -> Value {
    let mut state = serde_json::Map::new();
    state.insert(
        "primaryCodingEnvironment".into(),
        Value::String(environments.primary.clone()),
    );
    state.insert(
        "additionalCodingEnvironments".into(),
        serde_json::json!(environments.additional),
    );
    if let Some(value) = supplied {
        state.insert("imageGeneration".into(), value.clone());
    } else if let Some(value) = lock
        .image_generation
        .as_ref()
        .and_then(|value| serde_json::to_value(value).ok())
    {
        state.insert("imageGeneration".into(), value);
    }
    Value::Object(state)
}

fn apply_image_generation_component_selection(
    requested: &mut Vec<String>,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
) -> Result<(), AppError> {
    requested.retain(|id| !id.starts_with("imagegen."));
    for provider in image_generation_provider_rows(manifest, selection)? {
        if let Some(component_id) = provider
            .pointer("/route/component_id")
            .and_then(Value::as_str)
        {
            if !manifest
                .components
                .iter()
                .any(|component| component.id == component_id)
            {
                return Err(AppError::Source(format!("image provider component is missing from the verified manifest: {component_id}")));
            }
            requested.push(component_id.to_owned());
        }
    }
    requested.sort();
    requested.dedup();
    Ok(())
}

fn newly_defaulted_component_ids(
    previous_manifest: &RemoteManifest,
    current_manifest: &RemoteManifest,
    lock: &InstallationLock,
) -> Result<Vec<String>, AppError> {
    let previous_defaults = default_profile_component_ids(previous_manifest)?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let declined_optional = previous_manifest
        .components
        .iter()
        .filter(|component| {
            component.optional
                && lock
                    .optional_workflows
                    .get(&component.id)
                    .is_some_and(|workflow| workflow.state == "not_selected")
        })
        .map(|component| component.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    Ok(default_profile_component_ids(current_manifest)?
        .into_iter()
        .filter(|id| !previous_defaults.contains(id) && !declined_optional.contains(id.as_str()))
        .collect())
}

fn validate_optional_component_ids(
    manifest: &RemoteManifest,
    component_ids: &[String],
) -> Result<(), AppError> {
    for component_id in component_ids {
        let component = manifest
            .components
            .iter()
            .find(|component| component.id == *component_id)
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "the selected optional component is not declared by the verified manifest: {component_id}"
                ))
            })?;
        if !component.optional {
            return Err(AppError::InvalidInput(format!(
                "the selected component is not optional: {component_id}"
            )));
        }
    }
    Ok(())
}

fn adapt_codex_config_for_selection(
    bytes: &[u8],
    mcp_selected: bool,
    three_d_selected: bool,
    super_events_selected: bool,
    portrait: &PortraitPipelineConfig,
) -> Result<Vec<u8>, AppError> {
    let launcher = if three_d_selected {
        Some(
            std::env::current_exe()
                .map_err(|error| {
                    AppError::Process(format!(
                        "the app-owned Meshy launcher path is unavailable: {error}"
                    ))
                })?
                .display()
                .to_string(),
        )
    } else {
        None
    };
    hoi4_setup_core::adapt::adapt_codex_config(
        bytes,
        mcp_selected,
        launcher.as_deref(),
        super_events_selected,
        portrait,
    )
}

fn persisted_project_conventions(project_root: &Path) -> ProjectConventions {
    const STATE_PATH: &str = ".hoi4-mod-setup/state.json";
    let Ok(bytes) = crate::flatten::read_bounded_regular_file_no_follow_under_root(
        project_root,
        STATE_PATH,
        1024 * 1024,
    ) else {
        return ProjectConventions::default();
    };
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|state| project_conventions_from_state(&state).ok())
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn adapt_selected_source(
    component_id: &str,
    destination: &str,
    bytes: &[u8],
    identity: &ProjectIdentity,
    conventions: &ProjectConventions,
    ai_provider: &str,
    ai_model: &str,
    ai_reasoning_effort: &str,
    super_events_selected: bool,
    mcp_selected: bool,
    three_d_selected: bool,
    portrait: &PortraitPipelineConfig,
    selected_components: &[String],
) -> Result<Vec<u8>, AppError> {
    crate::service::builder::adapt_selected_source(
        &DesktopHost::detect(),
        component_id,
        destination,
        bytes,
        identity,
        conventions,
        ai_provider,
        ai_model,
        ai_reasoning_effort,
        super_events_selected,
        mcp_selected,
        three_d_selected,
        portrait,
        selected_components,
    )
}

fn image_client_config_source_destinations(
    manifest: &RemoteManifest,
    coding_environments: &CodingEnvironmentSelection,
    platform: Platform,
) -> Vec<String> {
    let selected = coding_environment::selected_environment_ids(coding_environments);
    manifest
        .components
        .iter()
        .filter(|component| {
            selected
                .iter()
                .any(|environment| coding_environment::serves_environment(component, environment))
                && component
                    .platforms
                    .iter()
                    .any(|declared| declared.supports(platform))
        })
        .map(|component| component.destination.path.clone())
        .collect()
}

fn image_client_config_artifacts(
    root: &Path,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    already_selected_destinations: &[String],
    live_selection: &LiveTestingSelection,
) -> Result<Vec<GeneratedArtifact>, AppError> {
    image_client_config_artifacts_for_platform(
        root,
        manifest,
        selection,
        coding_environments,
        already_selected_destinations,
        live_selection,
        Platform::current(),
    )
}

fn image_client_config_artifacts_for_platform(
    root: &Path,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    already_selected_destinations: &[String],
    live_selection: &LiveTestingSelection,
    platform: Platform,
) -> Result<Vec<GeneratedArtifact>, AppError> {
    image_client_config_artifacts_with_launch(
        root,
        manifest,
        selection,
        coding_environments,
        already_selected_destinations,
        live_selection,
        platform,
        &|target| mcp_client_launch(target, platform),
    )
}

fn append_maintenance_image_client_config_operations(
    artifacts: &[GeneratedArtifact],
    operations: &mut Vec<PlanOperation>,
    prepared: &mut Vec<PreparedFile>,
    lock: &InstallationLock,
    root: &Path,
) -> Result<(), AppError> {
    for (index, artifact) in artifacts.iter().enumerate() {
        let path = safe_join(root, &artifact.destination)?;
        let local_sha = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if crate::security::is_link_metadata(&metadata) || !metadata.is_file() {
                    return Err(AppError::PathSecurity(format!(
                        "coding client configuration is not a regular file: {}",
                        artifact.destination
                    )));
                }
                Some(sha256_file(&path)?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(AppError::Transaction(format!(
                    "could not inspect coding client configuration {}: {error}",
                    artifact.destination
                )))
            }
        };
        let previous = lock.files.iter().find(|file| {
            file.path == artifact.destination
                && file.component_id == artifact.component_id
                && !file.external
        });
        if local_sha.as_deref() == Some(artifact.expected_sha256.as_str()) {
            continue;
        }
        let locally_unmodified = match (&local_sha, previous) {
            (None, _) => false,
            (Some(hash), Some(previous)) => {
                !previous.preserved_local && hash == &previous.installed_sha256
            }
            _ => false,
        };
        let local_state = if local_sha.is_none() {
            LocalState::Absent
        } else if locally_unmodified {
            LocalState::Unmodified
        } else {
            LocalState::Modified
        };
        let action = if local_state == LocalState::Modified {
            OperationAction::Skip
        } else {
            OperationAction::Generate
        };
        let operation_id = format!(
            "maintenance-image-client-config-{}-{index:03}",
            artifact.component_id.replace('.', "-")
        );
        operations.retain(|operation| {
            operation.external || operation.destination != artifact.destination
        });
        prepared.retain(|file| file.destination != artifact.destination);
        operations.push(PlanOperation {
            id: operation_id.clone(),
            component_id: artifact.component_id.clone(),
            ownership: Some(image_client_config_ownership(
                local_sha.is_some(),
                previous.map(|file| file.ownership),
            )),
            location_scope: Some("project".into()),
            action,
            source_path: Some(format!("generated:{}", artifact.destination)),
            destination: artifact.destination.clone(),
            source_sha256: Some(artifact.expected_sha256.clone()),
            source_size: Some(artifact.content.len() as u64),
            platform: Some(ManifestPlatform::All),
            executable: false,
            result_sha256: (action == OperationAction::Generate)
                .then_some(artifact.expected_sha256.clone()),
            base_sha256: previous.map(|file| file.source_sha256.clone()),
            local_sha256: local_sha,
            local_state,
            resolution: (local_state == LocalState::Modified).then_some("review_required".into()),
            external: false,
            rollback: if local_state == LocalState::Absent {
                RollbackAction::RemoveCreated
            } else {
                RollbackAction::RestoreBackup
            },
            external_parent_identity: None,
        });
        prepared.push(PreparedFile {
            operation_id,
            destination: artifact.destination.clone(),
            bytes: artifact.content.as_bytes().to_vec(),
            expected_sha256: artifact.expected_sha256.clone(),
        });
    }
    Ok(())
}

fn adapt_image_generation_client_config(
    bytes: &[u8],
    destination: &str,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    project_root: &Path,
    live_selection: &LiveTestingSelection,
) -> Result<Vec<u8>, AppError> {
    adapt_image_generation_client_config_for_platform(
        bytes,
        destination,
        manifest,
        selection,
        coding_environments,
        project_root,
        live_selection,
        Platform::current(),
    )
}

#[allow(clippy::too_many_arguments)]
fn adapt_image_generation_client_config_for_platform(
    bytes: &[u8],
    destination: &str,
    manifest: &RemoteManifest,
    selection: &ImageGenerationSelection,
    coding_environments: &CodingEnvironmentSelection,
    _project_root: &Path,
    live_selection: &LiveTestingSelection,
    platform: Platform,
) -> Result<Vec<u8>, AppError> {
    adapt_image_generation_client_config_with_launch(
        bytes,
        destination,
        manifest,
        selection,
        coding_environments,
        live_selection,
        platform,
        cfg!(windows),
        &|target| mcp_client_launch(target, platform),
    )
}

#[allow(clippy::too_many_arguments)]
fn adapt_agents_for_selection(
    bytes: &[u8],
    identity: &ProjectIdentity,
    _provider: &str,
    _model: &str,
    _reasoning_effort: &str,
    conventions: &ProjectConventions,
    super_events_selected: bool,
    portrait_enabled: bool,
) -> Result<Vec<u8>, AppError> {
    hoi4_setup_core::adapt::adapt_agents_for_selection(
        bytes,
        identity,
        &desktop_game_folders(),
        conventions,
        super_events_selected,
        portrait_enabled,
    )
}

fn desktop_game_folders() -> GameFolders {
    let install = crate::paths::game_install();
    GameFolders {
        game_dir: install.game_dir.clone(),
        workshop_dir: install.workshop_dir.clone(),
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
        // Prefer metadata written by current versions. The descriptor fallback
        // reads legacy installs once so repair can migrate away from the old,
        // unsupported descriptor keys without losing the user's convention.
        script_prefix: lock.script_prefix.clone().or_else(|| {
            descriptor
                .as_ref()
                .and_then(|fields| fields.get("script_prefix").cloned())
        }),
        primary_namespace: lock.primary_namespace.clone().or_else(|| {
            descriptor
                .as_ref()
                .and_then(|fields| fields.get("namespace").cloned())
        }),
        descriptor_tags: Vec::new(),
        launcher_descriptor_path: None,
    }
}

fn codex_analysis_from_state(
    state: &Value,
    project_root: &Path,
) -> Result<CodexAnalysisRecord, AppError> {
    let analyses = codex_analyses()
        .lock()
        .map_err(|_| AppError::Process("Codex analysis store is unavailable".into()))?;
    confirmed_analysis_for_plan(&analyses, state, project_root)
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
    Ok(record_confirmed_in_session(
        &analyses,
        record,
        expected_project_root,
        expected_scan_id,
        expected_endpoint_fingerprint,
        expected_confirmation_values_sha256,
    ))
}

/// The desktop's answers to the plan builder: its process-wide analysis and
/// scan stores, its credential vault and launcher, its application-data
/// folders, the game it detected, its attachment store and its source cache.
struct DesktopHost {
    folders: GameFolders,
    /// Replaces the public repository; tests only.
    source: Option<Arc<dyn SourceFetcher>>,
}

impl DesktopHost {
    fn detect() -> Self {
        Self {
            folders: desktop_game_folders(),
            source: None,
        }
    }
}

impl PlanHost for DesktopHost {
    fn project_root(&self, state: &Value) -> Result<PathBuf, AppError> {
        project_root_from_state(state)
    }

    fn confirmed_analysis(
        &self,
        state: &Value,
        root: &Path,
    ) -> Result<CodexAnalysisRecord, AppError> {
        codex_analysis_from_state(state, root)
    }

    fn project_root_identity(
        &self,
        root: &Path,
        root_exists: bool,
        analysis: &CodexAnalysisRecord,
    ) -> Result<String, AppError> {
        if root_exists {
            let approved = codex_approved_evidence().lock().map_err(|_| {
                AppError::Serialization("approved scan evidence store is unavailable".into())
            })?;
            plan_root_identity(&approved, root, true, analysis.scan_id)
        } else {
            // A root to be created binds its parent; no scan is involved.
            plan_root_identity(&ApprovedScanEvidence::default(), root, false, None)
        }
    }

    fn launcher_policy(&self) -> LauncherPolicy {
        LauncherPolicy::Required
    }

    fn game_folders(&self) -> GameFolders {
        self.folders.clone()
    }

    fn three_d_launcher(&self) -> Result<String, AppError> {
        Ok(std::env::current_exe()
            .map_err(|error| {
                AppError::Process(format!(
                    "the app-owned Meshy launcher path is unavailable: {error}"
                ))
            })?
            .display()
            .to_string())
    }

    fn meshy_credential_reference(&self) -> Result<Option<CredentialReference>, AppError> {
        current_meshy_credential_reference()
    }

    fn context_thumbnail(&self, state: &Value) -> Result<Option<Vec<u8>>, AppError> {
        crate::context::thumbnail_from_state(state)
    }

    fn context_copies(&self, state: &Value) -> Result<Vec<crate::context::ContextFile>, AppError> {
        crate::context::copies_from_state(state)
    }

    fn mcp_launch(
        &self,
        target: &crate::mcp::VerifiedMcpTarget,
        platform: Platform,
    ) -> Result<(String, Vec<String>), AppError> {
        mcp_client_launch(target, platform)
    }

    fn transaction_roots(&self) -> Result<(PathBuf, PathBuf), AppError> {
        Ok((
            application_data_root()?.join("backups"),
            application_data_root()?.join("staging"),
        ))
    }

    fn source(&self) -> Result<Arc<dyn SourceFetcher>, AppError> {
        match &self.source {
            Some(source) => Ok(source.clone()),
            None => Ok(Arc::new(HttpSourceClient::new()?)),
        }
    }

    fn check_plan(&self, plan: &InstallationPlan) -> Result<(), AppError> {
        ensure_mcp_package_replaceable(plan)
    }
}

fn build_plan(state: &Value) -> Result<(InstallationPlan, Vec<PreparedFile>), AppError> {
    build_plan_with(state, &DesktopHost::detect())
}

#[tauri::command(async)]
async fn build_installation_plan(state: Value) -> Result<InstallationPlan, String> {
    run_blocking_command("installation-plan", move || {
        build_installation_plan_blocking(state)
    })
    .await
}

fn build_installation_plan_blocking(state: Value) -> Result<InstallationPlan, String> {
    require_ai_session(&state).map_err(provider_analysis_user_error)?;
    build_plan(&state)
        .and_then(|(plan, prepared_files)| {
            store_prepared_plan(
                plan,
                prepared_files,
                HashMap::new(),
                project_root_from_state(&state)?,
            )
        })
        .map_err(planning_command_error)
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
            && matches!(Platform::current(), Platform::Windows | Platform::Linux)
    })
}

fn component_ids_include_mcp(component_ids: &[String]) -> bool {
    component_ids.iter().any(|component_id| {
        component_id == "mcp.hoi4_agent_tools"
            && matches!(Platform::current(), Platform::Windows | Platform::Linux)
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
    expected_confirmation_values_sha256: Option<&str>,
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
    let expected_confirmation_values_sha256 =
        expected_confirmation_values_sha256.ok_or_else(|| {
            AppError::Credential(
                "update planning requires the exact values confirmed in the semantic review".into(),
            )
        })?;
    crate::codex::validate_confirmed_record(record)?;
    if record.analysis_purpose.as_deref() != Some("maintenance_reanalysis") {
        return Err(AppError::Credential(
            "update planning requires a maintenance-purpose provider reanalysis".into(),
        ));
    }
    // A profile without a custom endpoint (the Claude account route, and API
    // providers on their default endpoint) records none in the lock; the
    // analysis bound the same empty endpoint when it ran.
    let expected_endpoint_fingerprint = record
        .provider
        .as_deref()
        .filter(|provider| *provider != "codex")
        .map(|_| sha256_bytes(expected_endpoint.unwrap_or_default().trim().as_bytes()));
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
        if pending.confirmed_values_sha256.as_deref() != Some(expected_confirmation_values_sha256) {
            return Err(AppError::Credential(
                "update review values changed after semantic confirmation".into(),
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
    identity: &ProjectIdentity,
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
    // Refresh the seeded maintenance operation; overlapping component batches
    // must not create a second write to the same destination.
    let mut planned_paths = BTreeMap::new();
    for (index, operation) in operations.iter().enumerate().filter(|(_, op)| !op.external) {
        if planned_paths
            .insert(canonical_relative_key(&operation.destination)?, index)
            .is_some()
        {
            return Err(AppError::Source(
                "maintenance plan already contains a duplicate destination".into(),
            ));
        }
    }
    for selection in selections {
        let refresh_managed_agents = refresh_agents
            && (selection.component_id == "core.agents"
                || selection.component_id == "core.claude.instructions"
                || is_image_generation_client_config_path(&selection.destination)
                || ((selection.component_id == "core.skills"
                    || selection.component_id == "core.subagents")
                    && is_image_generation_guidance_path(&selection.destination)));
        let adapted_destination = adapt_super_events_destination(
            &selection.component_id,
            &selection.destination,
            identity,
        )?;
        if managed_components.contains(selection.component_id.as_str()) && !refresh_managed_agents {
            continue;
        }
        if managed_paths.contains(&(adapted_destination.as_str(), false)) && !refresh_managed_agents
        {
            continue;
        }
        let expected_sha256 = selection.expected_sha256.as_deref().ok_or_else(|| {
            AppError::Source(format!(
                "selected source file lacks SHA-256 evidence: {}",
                selection.source_path
            ))
        })?;
        let destination_key = canonical_relative_key(&adapted_destination)?;
        let existing_index = planned_paths.get(&destination_key).copied();
        if let Some(index) = existing_index {
            let operation = &operations[index];
            if operation.destination != adapted_destination
                || operation.component_id != selection.component_id
                || operation.ownership != Some(selection.ownership)
                || operation.location_scope.as_deref() != Some("project")
                || matches!(
                    operation.action,
                    OperationAction::DeleteManaged
                        | OperationAction::Rename
                        | OperationAction::Merge
                )
                || matches!(
                    operation.resolution.as_deref(),
                    Some("keep" | "skip" | "replace" | "merge" | "rename")
                )
            {
                return Err(AppError::Source(
                    "maintenance selection conflicts with an already planned destination".into(),
                ));
            }
            if !refresh_managed_agents
                && (operation.source_path.as_deref() != Some(selection.source_path.as_str())
                    || operation.source_sha256.as_deref() != Some(expected_sha256)
                    || operation.source_size != selection.expected_size
                    || operation.platform != Some(selection.platform)
                    || operation.executable != selection.executable)
            {
                return Err(AppError::Source(
                    "repeated maintenance selection has different source evidence".into(),
                ));
            }
        }
        if existing_index.is_some() && !refresh_managed_agents {
            continue;
        }
        let destination = safe_join(root, &adapted_destination)?;
        let local_sha256 = if destination.is_file() {
            Some(sha256_file(&destination)?)
        } else {
            None
        };
        let current_locked_file = refresh_managed_agents
            .then(|| {
                lock.files.iter().find(|file| {
                    file.component_id == selection.component_id
                        && file.path == adapted_destination
                        && !file.external
                })
            })
            .flatten();
        let unmodified_sha256 = current_locked_file
            .map(|file| file.installed_sha256.as_str())
            .unwrap_or(expected_sha256);
        let (action, local_state, resolution) = match local_sha256.as_deref() {
            None if current_locked_file.is_some_and(|file| file.preserved_local) => (
                OperationAction::Skip,
                LocalState::Unknown,
                Some("review_required".into()),
            ),
            None => (OperationAction::Create, LocalState::Absent, None),
            Some(hash)
                if hash == unmodified_sha256
                    && !current_locked_file.is_some_and(|file| file.preserved_local) =>
            {
                (
                    OperationAction::Replace,
                    LocalState::Unmodified,
                    Some(if refresh_managed_agents {
                        "optional_guidance_refresh".into()
                    } else {
                        "new_component_file".into()
                    }),
                )
            }
            Some(_) => (
                OperationAction::Skip,
                LocalState::Modified,
                Some("review_required".into()),
            ),
        };
        let rollback = if local_sha256.is_none() {
            RollbackAction::RemoveCreated
        } else {
            RollbackAction::RestoreBackup
        };
        let operation = PlanOperation {
            id: existing_index
                .map(|index| operations[index].id.clone())
                .unwrap_or_else(|| {
                    format!(
                        "repair-add-{}-{}",
                        selection.component_id,
                        sha256_bytes(destination_key.as_bytes())
                    )
                }),
            component_id: selection.component_id.clone(),
            ownership: Some(selection.ownership),
            location_scope: Some("project".into()),
            action,
            source_path: Some(selection.source_path.clone()),
            destination: adapted_destination,
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
            rollback,
            external_parent_identity: None,
        };
        if let Some(index) = existing_index {
            operations[index] = operation;
        } else {
            planned_paths.insert(destination_key, operations.len());
            operations.push(operation);
        }
    }
    Ok(())
}

fn is_image_generation_guidance_path(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    path.ends_with("claude.md")
        || path.ends_with("agents.md")
        || path.ends_with("/hoi4-live-testing/skill.md")
        || path.ends_with("/hoi4-live-tools/skill.md")
        || path.ends_with("/hoi4-debug/skill.md")
        || path.ends_with("/hoi4-feature-assets/skill.md")
        || path.ends_with("/hoi4-feature-assets/references/asset-source-rules.md")
        || [
            "hoi4-generated-feature-art.md",
            "hoi4-icon-artist.md",
            "hoi4-portrait-creator.md",
        ]
        .iter()
        .any(|name| path.ends_with(name))
}

fn validate_generated_maintenance_content(
    destination: &str,
    source: &[u8],
    adapted: &[u8],
    expected_sha256: &str,
) -> Result<(), AppError> {
    // A client config is recomposed from its verified generated baseline and
    // current user settings. Other generated artifacts retain their exact hash.
    let verified = if is_image_generation_client_config_path(destination) {
        source
    } else {
        adapted
    };
    if sha256_bytes(verified) != expected_sha256 {
        return Err(AppError::Transaction(format!(
            "maintenance generated checksum mismatch: {destination}"
        )));
    }
    Ok(())
}

fn prepare_preserved_generated_skip(
    lock: &InstallationLock,
    operation: &mut PlanOperation,
) -> bool {
    let preserve = operation.action == OperationAction::Skip
        && operation.local_sha256.is_some()
        && lock.files.iter().any(|file| {
            file.preserved_local
                && file.path == operation.destination
                && file.external == operation.external
                && file.source_path.starts_with("generated:")
                && operation.source_path.as_deref() == Some(file.source_path.as_str())
                && operation.source_sha256.as_deref() == Some(file.source_sha256.as_str())
                && file.generated_content.is_none()
                && file.generated_bytes.is_none()
        });
    if preserve && operation.local_state == LocalState::Unmodified {
        // The lock's prior Keep decision still applies to identical local bytes.
        operation.resolution = Some("keep".into());
    }
    preserve
}

fn maintenance_conflicts(
    operations: &[PlanOperation],
    prepared: &[PreparedFile],
    merge_contexts: &HashMap<String, MergeContext>,
) -> Vec<PlanConflict> {
    operations
        .iter()
        .filter(|operation| {
            // An edit the user already made, with nothing new from the
            // source, is kept without asking again.
            operation.resolution.as_deref() != Some("keep_reviewed_edit")
                && (operation.local_state == LocalState::Modified
                    || matches!(
                        operation.resolution.as_deref(),
                        Some(
                            "review_required"
                                | "merged_base_required"
                                | "reverse_merge_required"
                                | "obsolete_review"
                        )
                    ))
        })
        .map(|operation| {
            let no_incoming = !prepared
                .iter()
                .any(|file| file.operation_id == operation.id);
            let preserve_only = no_incoming
                || matches!(
                    operation.resolution.as_deref(),
                    Some("reverse_merge_required" | "obsolete_review")
                );
            let mut options = if preserve_only {
                vec!["keep".into(), "skip".into()]
            } else {
                vec![
                    "keep".into(),
                    "replace".into(),
                    "rename".into(),
                    "skip".into(),
                ]
            };
            if !preserve_only && merge_contexts.contains_key(&operation.id) {
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
        .collect()
}

#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
async fn build_maintenance_plan(
    mode: String,
    project_root: String,
    analysis_override: Option<CodexAnalysisRecord>,
    add_optional_components: Option<Vec<String>>,
    portrait_pipeline: Option<Value>,
    primary_coding_environment: Option<String>,
    additional_coding_environments: Option<Vec<String>>,
    analysis_confirmation_values: Option<Value>,
    image_generation: Option<Value>,
    live_testing: Option<Value>,
    update_source: Option<UpdateSourceSelection>,
) -> Result<InstallationPlan, String> {
    run_maintenance_command(move || {
        build_maintenance_plan_blocking(
            mode,
            project_root,
            analysis_override,
            add_optional_components,
            portrait_pipeline,
            primary_coding_environment,
            additional_coding_environments,
            analysis_confirmation_values,
            image_generation,
            live_testing,
            update_source,
        )
    })
    .await
}

async fn run_maintenance_command<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    run_blocking_command("maintenance-plan", move || {
        work().map_err(planning_command_error)
    })
    .await
}

#[allow(clippy::too_many_arguments)]
fn build_maintenance_plan_blocking(
    mode: String,
    project_root: String,
    analysis_override: Option<CodexAnalysisRecord>,
    add_optional_components: Option<Vec<String>>,
    portrait_pipeline_value: Option<Value>,
    primary_coding_environment: Option<String>,
    additional_coding_environments: Option<Vec<String>>,
    analysis_confirmation_values: Option<Value>,
    image_generation_value: Option<Value>,
    live_testing_value: Option<Value>,
    update_source: Option<UpdateSourceSelection>,
) -> Result<InstallationPlan, AppError> {
    if update_source.is_some() && mode != "update" {
        return Err(AppError::InvalidInput(
            "Only Update can change the source policy".into(),
        ));
    }
    let root =
        validate_project_root_or_destination(Path::new(&project_root)).map(|(root, _)| root)?;
    let conventions = analysis_confirmation_values
        .as_ref()
        .map(|value| project_conventions_from_value(value.get("conventions")))
        .transpose()?
        .unwrap_or_else(|| persisted_project_conventions(&root));
    let confirmation_values_sha256 = analysis_confirmation_values
        .as_ref()
        .map(confirmation_values_sha256)
        .transpose()?;
    let lock = read_project_lock(&root)?;
    let update_source_request = maintenance_source_request(&lock, update_source.as_ref())?;
    let image_generation_input = image_generation_value;
    let mut selected_live_testing = Some(
        lock.live_testing
            .clone()
            .unwrap_or_else(|| crate::live_testing::legacy_selection(&lock.components)),
    );
    let coding_environment_selection = if mode == "remove" {
        CodingEnvironmentSelection {
            primary: lock.primary_coding_environment.clone(),
            additional: lock.additional_coding_environments.clone(),
        }
    } else {
        let mut value = serde_json::Map::new();
        if let Some(primary) = primary_coding_environment {
            value.insert("primaryCodingEnvironment".into(), Value::String(primary));
        } else {
            value.insert(
                "primaryCodingEnvironment".into(),
                Value::String(lock.primary_coding_environment.clone()),
            );
        }
        if let Some(additional) = additional_coding_environments {
            value.insert(
                "additionalCodingEnvironments".into(),
                Value::Array(additional.into_iter().map(Value::String).collect()),
            );
        } else {
            value.insert(
                "additionalCodingEnvironments".into(),
                Value::Array(
                    lock.additional_coding_environments
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        coding_environment::selection_from_value(&Value::Object(value))?
    };
    let portrait_pipeline = if let Some(value) = portrait_pipeline_value {
        portrait_pipeline_from_state(&serde_json::json!({"portraitPipeline": value}))?
    } else {
        lock.portrait_pipeline
            .clone()
            .unwrap_or_else(|| PortraitPipelineConfig {
                enabled: false,
                provider: "disabled".into(),
                provider_status: "not_selected".into(),
                workflow_repository: "https://github.com/klimPaskov/comfyui-hoi4-portraits".into(),
                workflow_branch: "codex/portrait-pipeline".into(),
                workflow_commit: "b47222a77f2f6454704530865aa1441fad48bdd3".into(),
                preferred_workflow: "source".into(),
                local_comfyui_root: String::new(),
                local_server_url: "http://127.0.0.1:8188".into(),
                runpod_url: String::new(),
                runpod_workspace: "/workspace/comfyui-hoi4-portraits".into(),
                mcp_registered: false,
            })
    };
    let add_optional_components = add_optional_components.unwrap_or_default();
    let mut seen_optional_components = std::collections::HashSet::new();
    if !add_optional_components.iter().all(|id| {
        !id.is_empty()
            && id.len() <= 128
            && id.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
            && seen_optional_components.insert(id.as_str())
    }) {
        return Err(AppError::InvalidInput(
            "an optional workflow selection is invalid or duplicated".into(),
        ));
    }
    if !add_optional_components.is_empty() && !matches!(mode.as_str(), "update" | "repair") {
        return Err(AppError::InvalidInput(
            "optional workflows can be added only during update or repair".into(),
        ));
    }
    if !portrait_pipeline.enabled
        && add_optional_components
            .iter()
            .any(|id| id.starts_with("workflow.portraits"))
    {
        return Err(AppError::InvalidInput(
            "portrait components cannot be added while portrait production is disabled".into(),
        ));
    }
    if mode != "remove" {
        if lock.ai_provider == "codex" {
            require_codex_chatgpt_session()?;
        } else {
            let credential_reference = ai_credential_reference(&lock.ai_provider);
            let status = ai::account_status(
                &OsCredentialStore,
                &AiProviderConfig {
                    provider: lock.ai_provider.clone(),
                    model: lock.ai_model.clone(),
                    reasoning_effort: lock.ai_reasoning_effort.clone(),
                    endpoint: lock.ai_endpoint.clone().unwrap_or_default(),
                    credential_reference,
                },
            );
            if status.error.is_some() || !status.authenticated {
                return Err(AppError::Credential(format!(
                    "{} is not configured for maintenance planning",
                    lock.ai_provider
                )));
            }
        }
    }
    require_maintenance_reanalysis(
        &mode,
        analysis_override.as_ref(),
        &root,
        lock.ai_endpoint.as_deref(),
        confirmation_values_sha256.as_deref(),
    )?;
    let codex_analysis = if mode == "remove" {
        None
    } else {
        let record = analysis_override
            .clone()
            .or_else(|| lock.codex_analysis.clone())
            .ok_or_else(|| {
                AppError::Credential("the installed project has no confirmed provider analysis; run Import review before maintenance"
                    .into())
        })?;
        crate::codex::validate_confirmed_record(&record)?;
        let record_provider = record.provider.as_deref().unwrap_or("codex");
        let profile = ai::profile(&lock.ai_provider).ok_or_else(|| {
            AppError::Credential(format!(
                "installed lock uses an unsupported AI provider: {}",
                lock.ai_provider
            ))
        })?;
        if record_provider != lock.ai_provider
            || (lock.ai_provider != "codex"
                && (record.model.as_deref() != Some(lock.ai_model.as_str())
                    || record.optimization_profile.as_deref()
                        != Some(profile.optimization_profile.as_str())))
        {
            return Err(AppError::Credential(
                "provider reanalysis does not match the installed provider profile".into(),
            ));
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
                confirmation_values_sha256.as_deref(),
            )?
        {
            return Err(AppError::Credential(
                "the provider reanalysis is not confirmed in the current core session; review it again"
                    .into(),
            ));
        }
        Some(record)
    };
    let client = HttpSourceClient::new()?;
    let mut maintenance_components = lock
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect::<Vec<_>>();
    let mut wiki_required_pages = lock.wiki_required_pages.clone();
    let mut wiki_metadata = lock.wiki_metadata.clone();
    let mut maintenance_mcp_manifest: Option<RemoteManifest> = None;
    let mut image_generation_manifest: Option<RemoteManifest> = None;
    let mut selected_image_generation = lock.image_generation.clone();
    if !portrait_pipeline.enabled {
        maintenance_components.retain(|id| !id.starts_with("workflow.portraits"));
    }
    let (mut operations, mut plan_source, mut source_revision) = if mode == "update" {
        let resolution = resolve_source(&client, &update_source_request)?;
        validate_update_release_identity(&lock, &update_source_request, &resolution.identity)?;
        if let Some(record) = codex_analysis.as_ref() {
            validate_analysis_source_binding(record, &resolution.identity)?;
        }
        // Every selected MCP action, including standalone Live Tools, uses the target manifest.
        maintenance_mcp_manifest = Some(resolution.manifest.clone());
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
        let incoming_environment_ids =
            coding_environment::all_environment_component_ids(&resolution.manifest);
        requested.retain(|id| !incoming_environment_ids.contains(id));
        let live_state = serde_json::json!({"live_testing": live_testing_value.clone().unwrap_or_else(|| serde_json::to_value(selected_live_testing.as_ref().unwrap()).unwrap())});
        let mut live_requested = requested.clone();
        if live_testing_value.is_some()
            && !live_requested
                .iter()
                .any(|id| id == "workflow.live_testing")
        {
            live_requested.push("workflow.live_testing".into());
        }
        let live_selection = crate::live_testing::from_state(
            &live_state,
            &resolution.manifest,
            &live_requested,
            Platform::current(),
        )?;
        crate::live_testing::components(&mut requested, &live_selection, &resolution.manifest)?;
        selected_live_testing = Some(live_selection);
        let image_state = image_generation_state_for_maintenance(
            image_generation_input.as_ref(),
            &lock,
            &coding_environment_selection,
        );
        let mut desired_image = image_generation_selection_from_state(
            &image_state,
            &resolution.manifest,
            &coding_environment_selection,
        )?;
        if let Some(installed_image) = lock.image_generation.as_ref() {
            desired_image.installed_package_tree_sha256 =
                installed_image.installed_package_tree_sha256.clone();
            desired_image.installed_package_lock_sha256 =
                installed_image.installed_package_lock_sha256.clone();
        }
        selected_image_generation = Some(desired_image);
        apply_image_generation_component_selection(
            &mut requested,
            &resolution.manifest,
            selected_image_generation
                .as_ref()
                .expect("selection was set"),
        )?;
        image_generation_manifest = Some(resolution.manifest.clone());
        if resolution
            .manifest
            .components
            .iter()
            .any(|component| component.id == "mcp.hoi4_agent_tools.bootstrap")
            && !requested
                .iter()
                .any(|id| id == "mcp.hoi4_agent_tools.bootstrap")
        {
            requested.push("mcp.hoi4_agent_tools.bootstrap".into());
        }
        if resolution
            .manifest
            .components
            .iter()
            .any(|component| component.id == "mcp.hoi4_agent_tools")
            && !requested.iter().any(|id| id == "mcp.hoi4_agent_tools")
        {
            requested.push("mcp.hoi4_agent_tools".into());
        }
        requested.retain(|id| !id.starts_with("workflow.portraits"));
        if portrait_pipeline.enabled {
            if let Some(ids) = portrait_component_ids(&portrait_pipeline.provider) {
                for component_id in ids {
                    if !requested.iter().any(|id| id == component_id) {
                        requested.push(component_id.into());
                    }
                }
            }
        }
        for component_id in &add_optional_components {
            if !requested.iter().any(|id| id == component_id) {
                requested.push(component_id.clone());
            }
        }
        validate_optional_component_ids(&resolution.manifest, &add_optional_components)?;
        let previous_manifest = resolve_installed_manifest(&lock)?;
        let new_defaults =
            newly_defaulted_component_ids(&previous_manifest, &resolution.manifest, &lock)?;
        add_supported_profile_components(
            &resolution.manifest,
            &mut requested,
            &new_defaults,
            Platform::current(),
        )?;
        let active_component_ids = expand_components(&resolution.manifest, &requested)?
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        for component_id in coding_environment::component_ids(
            &resolution.manifest,
            &coding_environment_selection,
            &active_component_ids,
        )? {
            if !requested.iter().any(|id| id == &component_id) {
                requested.push(component_id);
            }
        }
        let expanded = expand_components(&resolution.manifest, &requested)?;
        maintenance_components = expanded.clone();
        let support = crate::source::resolve_platform_support(
            &resolution.manifest,
            &expanded,
            Platform::current(),
        )?;
        if support.iter().any(|item| item.state == "blocked") {
            return Err(AppError::UnsupportedPlatform(
                "the current source has an unsupported required component".into(),
            ));
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
        let selections = select_manifest_files(&resolution.manifest, &supported)?;
        // Re-adapt the incoming Codex configuration against the desired
        // maintenance closure.  The predecessor lock may not contain Codex
        // yet (for example when a project switches from Claude to Codex), so
        // using only the old lock would silently omit its MCP route.
        let mcp_selected = component_ids_include_mcp(&maintenance_components);
        let project_identity = maintenance_identity(&lock, &root);
        if mcp_selected {
            maintenance_mcp_manifest = Some(resolution.manifest.clone());
        }
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
        let incoming = selections
            .iter()
            .map(|selection| {
                let destination = adapt_super_events_destination(
                    &selection.component_id,
                    &selection.destination,
                    &project_identity,
                )?;
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
                    &destination,
                    &source_bytes,
                    &project_identity,
                    &conventions,
                    &lock.ai_provider,
                    &lock.ai_model,
                    &lock.ai_reasoning_effort,
                    maintenance_components
                        .iter()
                        .any(|id| id == "workflow.super_events"),
                    mcp_selected,
                    maintenance_components.iter().any(|id| id == "workflow.3d"),
                    &portrait_pipeline,
                    &maintenance_components,
                )?;
                let desired_bytes = if is_image_generation_client_config_path(&destination) {
                    merge_existing_image_client_config(&root, &destination, &desired_bytes)?
                } else {
                    desired_bytes
                };
                let desired_bytes =
                    if let Some(image_selection) = selected_image_generation.as_ref() {
                        let guided = adapt_image_generation_guidance(
                            &desired_bytes,
                            &resolution.manifest,
                            image_selection,
                        )?;
                        adapt_image_generation_client_config(
                            &guided,
                            &destination,
                            &resolution.manifest,
                            image_selection,
                            &coding_environment_selection,
                            &root,
                            selected_live_testing.as_ref().unwrap(),
                        )?
                    } else {
                        desired_bytes
                    };
                Ok(LockedFile {
                    path: destination,
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
                    installer_created: false,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
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
        let operations = crate::transaction::update_operations(&lock, &incoming, &root)?;
        wiki_required_pages = resolution.manifest.wiki.required_pages.clone();
        wiki_metadata = Some(crate::source::wiki_install_metadata(&resolution.manifest));
        (
            operations,
            resolution.identity.clone(),
            resolution.identity.resolved_revision,
        )
    } else {
        if mode != "remove" && lock_mcp_selected(&lock) {
            maintenance_mcp_manifest = Some(resolve_installed_manifest(&lock)?);
        }
        let operations = match mode.as_str() {
            "repair" => {
                let mut operations = crate::transaction::repair_operations(&lock, &root)?;
                if !portrait_pipeline.enabled {
                    let portrait_lock = InstallationLock {
                        files: lock
                            .files
                            .iter()
                            .filter(|file| file.component_id.starts_with("workflow.portraits"))
                            .cloned()
                            .collect(),
                        ..lock.clone()
                    };
                    let mut portrait_removals =
                        crate::transaction::managed_removal_operations(&portrait_lock, &root)?;
                    for (index, operation) in portrait_removals.iter_mut().enumerate() {
                        operation.id = format!("repair-portrait-remove-{index:05}");
                    }
                    operations.retain(|operation| {
                        !operation.component_id.starts_with("workflow.portraits")
                    });
                    operations.extend(portrait_removals);
                }
                Ok(operations)
            }
            "reinstall" => crate::transaction::reinstall_operations(&lock, &root),
            "remove" => crate::transaction::managed_removal_operations(&lock, &root),
            _ => {
                return Err(AppError::InvalidInput(
                    "unsupported maintenance mode".into(),
                ))
            }
        }?;
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
    if matches!(mode.as_str(), "repair" | "reinstall") {
        let locked_selection = CodingEnvironmentSelection {
            primary: lock.primary_coding_environment.clone(),
            additional: lock.additional_coding_environments.clone(),
        };
        if coding_environment_selection != locked_selection
            || image_generation_input.is_some()
            || live_testing_value.is_some()
            || !lock
                .components
                .iter()
                .any(|component| component.id == "mcp.hoi4_agent_tools.bootstrap")
            || !lock
                .components
                .iter()
                .any(|component| component.id == "mcp.hoi4_agent_tools")
        {
            // Repair and reinstall are allowed to change the native client
            // closure without switching the immutable source revision. The
            // same pinned manifest supplies additions and the lock supplies
            // the base used to decide whether a deselected file may be
            // removed.
            let resolution = resolve_source(
                &client,
                &SourceRequest {
                    mode: SourceMode::PinnedCommit,
                    requested_ref: Some(lock.source.revision.clone()),
                    release: None,
                },
            )?;
            if resolution.identity.manifest_sha256 != lock.source.manifest_sha256 {
                return Err(AppError::Source(
                    "the installed source manifest does not match its immutable revision evidence"
                        .into(),
                ));
            }
            let manifest_environment_ids =
                coding_environment::all_environment_component_ids(&resolution.manifest);
            let mut active_requested = lock
                .components
                .iter()
                .filter(|component| {
                    !manifest_environment_ids.contains(&component.id)
                        && !matches!(
                            component.state.as_str(),
                            "removed" | "not_selected" | "unsupported_platform"
                        )
                })
                .map(|component| component.id.clone())
                .collect::<Vec<_>>();
            let live_state = serde_json::json!({"live_testing": live_testing_value.clone().unwrap_or_else(|| serde_json::to_value(selected_live_testing.as_ref().unwrap()).unwrap())});
            let mut live_requested = active_requested.clone();
            if live_testing_value.is_some()
                && !live_requested
                    .iter()
                    .any(|id| id == "workflow.live_testing")
            {
                live_requested.push("workflow.live_testing".into());
            }
            let live_selection = crate::live_testing::from_state(
                &live_state,
                &resolution.manifest,
                &live_requested,
                Platform::current(),
            )?;
            crate::live_testing::components(
                &mut active_requested,
                &live_selection,
                &resolution.manifest,
            )?;
            selected_live_testing = Some(live_selection);
            let image_state = image_generation_state_for_maintenance(
                image_generation_input.as_ref(),
                &lock,
                &coding_environment_selection,
            );
            let mut desired_image = image_generation_selection_from_state(
                &image_state,
                &resolution.manifest,
                &coding_environment_selection,
            )?;
            if let Some(installed_image) = lock.image_generation.as_ref() {
                desired_image.installed_package_tree_sha256 =
                    installed_image.installed_package_tree_sha256.clone();
                desired_image.installed_package_lock_sha256 =
                    installed_image.installed_package_lock_sha256.clone();
            }
            let mut desired_image_ids = Vec::new();
            for provider in image_generation_provider_rows(&resolution.manifest, &desired_image)? {
                if let Some(component_id) = provider
                    .pointer("/route/component_id")
                    .and_then(Value::as_str)
                {
                    desired_image_ids.push(component_id.to_owned());
                }
            }
            let live_ids = active_requested
                .iter()
                .filter(|id| {
                    matches!(
                        id.as_str(),
                        "workflow.live_testing" | "workflow.live_tools" | "mcp.hoi4_live.bootstrap"
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            desired_image_ids.extend(
                expand_components(&resolution.manifest, &live_ids)?
                    .into_iter()
                    .filter(|id| {
                        matches!(
                            id.as_str(),
                            "workflow.live_testing"
                                | "workflow.live_tools"
                                | "mcp.hoi4_live.bootstrap"
                        )
                    }),
            );
            let current_image_ids = lock
                .components
                .iter()
                .filter(|component| {
                    (component.id.starts_with("imagegen.")
                        || matches!(
                            component.id.as_str(),
                            "workflow.live_testing"
                                | "workflow.live_tools"
                                | "mcp.hoi4_live.bootstrap"
                        ))
                        && !matches!(
                            component.state.as_str(),
                            "removed" | "not_selected" | "unsupported_platform"
                        )
                })
                .map(|component| component.id.clone())
                .collect::<std::collections::HashSet<_>>();
            let desired_image_set = desired_image_ids
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            let add_image_ids = desired_image_set
                .difference(&current_image_ids)
                .cloned()
                .collect::<Vec<_>>();
            let remove_image_ids = current_image_ids
                .difference(&desired_image_set)
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            let image_selection_changed = lock.image_generation.as_ref() != Some(&desired_image)
                || lock.live_testing != selected_live_testing;
            if !add_image_ids.is_empty() {
                let expanded = expand_components(&resolution.manifest, &add_image_ids)?;
                let support = crate::source::resolve_platform_support(
                    &resolution.manifest,
                    &expanded,
                    Platform::current(),
                )?;
                if support.iter().any(|item| item.state == "blocked") {
                    return Err(AppError::UnsupportedPlatform(
                        "a selected image provider has no verified route on this computer".into(),
                    ));
                }
                let supported = expanded
                    .into_iter()
                    .filter(|id| {
                        support.iter().any(|item| {
                            item.component_id == id.as_str() && item.state == "supported"
                        })
                    })
                    .collect::<Vec<_>>();
                let selections = select_manifest_files(&resolution.manifest, &supported)?;
                append_additional_component_operations(
                    &mut operations,
                    &selections,
                    &lock,
                    &root,
                    &maintenance_identity(&lock, &root),
                    false,
                )?;
            }
            if !remove_image_ids.is_empty() {
                operations.retain(|operation| !remove_image_ids.contains(&operation.component_id));
                let obsolete = InstallationLock {
                    components: lock
                        .components
                        .iter()
                        .filter(|component| remove_image_ids.contains(&component.id))
                        .cloned()
                        .collect(),
                    files: lock
                        .files
                        .iter()
                        .filter(|file| remove_image_ids.contains(&file.component_id))
                        .cloned()
                        .collect(),
                    ..lock.clone()
                };
                operations.extend(crate::transaction::managed_removal_operations(
                    &obsolete, &root,
                )?);
            }
            maintenance_components.retain(|id| {
                !id.starts_with("imagegen.")
                    && !matches!(
                        id.as_str(),
                        "workflow.live_testing" | "workflow.live_tools" | "mcp.hoi4_live.bootstrap"
                    )
            });
            maintenance_components.extend(desired_image_ids.clone());
            selected_image_generation = Some(desired_image);
            image_generation_manifest = Some(resolution.manifest.clone());
            if image_selection_changed {
                let selected_environments =
                    coding_environment::selected_environment_ids(&coding_environment_selection);
                let refresh_ids = resolution
                    .manifest
                    .components
                    .iter()
                    .filter(|component| {
                        [
                            "core.claude.instructions",
                            "core.agents",
                            "core.skills",
                            "core.subagents",
                            "codex.config",
                        ]
                        .contains(&component.id.as_str())
                            || selected_environments.iter().any(|environment| {
                                coding_environment::serves_environment(component, environment)
                            })
                    })
                    .map(|component| component.id.clone())
                    .collect::<Vec<_>>();
                if !refresh_ids.is_empty() {
                    let expanded = expand_components(&resolution.manifest, &refresh_ids)?;
                    let selections = select_manifest_files(&resolution.manifest, &expanded)?
                        .into_iter()
                        .filter(|selection| {
                            is_image_generation_guidance_path(&selection.destination)
                                || is_image_generation_client_config_path(&selection.destination)
                        })
                        .collect::<Vec<_>>();
                    if !selections.is_empty() {
                        append_additional_component_operations(
                            &mut operations,
                            &selections,
                            &lock,
                            &root,
                            &maintenance_identity(&lock, &root),
                            true,
                        )?;
                    }
                }
            }
            for component_id in &add_optional_components {
                if !active_requested.iter().any(|id| id == component_id) {
                    active_requested.push(component_id.clone());
                }
            }
            let active_component_ids = expand_components(&resolution.manifest, &active_requested)?
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            let mut desired_environment_ids = coding_environment::component_ids(
                &resolution.manifest,
                &coding_environment_selection,
                &active_component_ids,
            )?;
            // The tools server is shared by every selected native client.  It
            // is deliberately retained outside the primary/additional
            // environment closure so switching clients never unregisters the
            // provider-neutral MCP route.
            if resolution
                .manifest
                .components
                .iter()
                .any(|component| component.id == "mcp.hoi4_agent_tools")
                && !desired_environment_ids
                    .iter()
                    .any(|id| id == "mcp.hoi4_agent_tools")
            {
                desired_environment_ids.push("mcp.hoi4_agent_tools".into());
            }
            let active_locked_environment_ids = lock
                .components
                .iter()
                .filter(|component| {
                    manifest_environment_ids.contains(&component.id)
                        && !matches!(
                            component.state.as_str(),
                            "removed" | "not_selected" | "unsupported_platform"
                        )
                })
                .map(|component| component.id.clone())
                .collect::<std::collections::HashSet<_>>();
            let desired_set = desired_environment_ids
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            let mut add_ids = desired_environment_ids
                .iter()
                .filter(|id| !active_locked_environment_ids.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            if resolution
                .manifest
                .components
                .iter()
                .any(|component| component.id == "mcp.hoi4_agent_tools.bootstrap")
                && !lock
                    .components
                    .iter()
                    .any(|component| component.id == "mcp.hoi4_agent_tools.bootstrap")
            {
                add_ids.push("mcp.hoi4_agent_tools.bootstrap".into());
            }
            if resolution
                .manifest
                .components
                .iter()
                .any(|component| component.id == "mcp.hoi4_agent_tools")
                && !lock
                    .components
                    .iter()
                    .any(|component| component.id == "mcp.hoi4_agent_tools")
                && !add_ids.iter().any(|id| id == "mcp.hoi4_agent_tools")
            {
                add_ids.push("mcp.hoi4_agent_tools".into());
            }
            let remove_ids = active_locked_environment_ids
                .iter()
                .filter(|id| !desired_set.contains(*id))
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            if !add_ids.is_empty() {
                let expanded = expand_components(&resolution.manifest, &add_ids)?;
                let support = crate::source::resolve_platform_support(
                    &resolution.manifest,
                    &expanded,
                    Platform::current(),
                )?;
                if support.iter().any(|item| item.state == "blocked") {
                    return Err(AppError::UnsupportedPlatform(
                        "a selected coding environment has no verified route on this computer"
                            .into(),
                    ));
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
                let selections = select_manifest_files(&resolution.manifest, &supported)?;
                append_additional_component_operations(
                    &mut operations,
                    &selections,
                    &lock,
                    &root,
                    &maintenance_identity(&lock, &root),
                    false,
                )?;
            }
            if !remove_ids.is_empty() {
                operations.retain(|operation| !remove_ids.contains(&operation.component_id));
                let obsolete_lock = InstallationLock {
                    components: lock
                        .components
                        .iter()
                        .filter(|component| remove_ids.contains(&component.id))
                        .cloned()
                        .collect(),
                    files: lock
                        .files
                        .iter()
                        .filter(|file| remove_ids.contains(&file.component_id))
                        .cloned()
                        .collect(),
                    ..lock.clone()
                };
                let mut removals =
                    crate::transaction::managed_removal_operations(&obsolete_lock, &root)?;
                for (index, operation) in removals.iter_mut().enumerate() {
                    operation.id = format!("maintenance-environment-remove-{index:05}");
                }
                operations.extend(removals);
            }
            maintenance_components.retain(|id| !remove_ids.contains(id));
            for component_id in desired_environment_ids {
                if !maintenance_components.iter().any(|id| id == &component_id) {
                    maintenance_components.push(component_id);
                }
            }
        }
    }
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
        )?;
        if resolution.identity.manifest_sha256 != lock.source.manifest_sha256 {
            return Err(AppError::Source(
                "the installed source manifest does not match its immutable revision evidence"
                    .into(),
            ));
        }
        validate_optional_component_ids(&resolution.manifest, &add_optional_components)?;
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
            return Err(AppError::InvalidInput(
                "the selected optional workflows are already installed".into(),
            ));
        }
        let expanded = expand_components(&resolution.manifest, &requested)?;
        if portrait_pipeline.enabled {
            let keep_portrait = expanded
                .iter()
                .filter(|id| id.starts_with("workflow.portraits"))
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            let obsolete_portrait_lock = InstallationLock {
                files: lock
                    .files
                    .iter()
                    .filter(|file| {
                        file.component_id.starts_with("workflow.portraits")
                            && !keep_portrait.contains(&file.component_id)
                    })
                    .cloned()
                    .collect(),
                ..lock.clone()
            };
            if !obsolete_portrait_lock.files.is_empty() {
                let mut removals =
                    crate::transaction::managed_removal_operations(&obsolete_portrait_lock, &root)?;
                for (index, operation) in removals.iter_mut().enumerate() {
                    operation.id = format!("repair-portrait-provider-remove-{index:05}");
                }
                operations.retain(|operation| {
                    !operation.component_id.starts_with("workflow.portraits")
                        || keep_portrait.contains(&operation.component_id)
                });
                operations.extend(removals);
            }
        }
        let support = crate::source::resolve_platform_support(
            &resolution.manifest,
            &expanded,
            Platform::current(),
        )?;
        if support.iter().any(|item| item.state == "blocked") {
            return Err(AppError::UnsupportedPlatform(
                "an optional workflow has no verified route on this computer".into(),
            ));
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
        let selections = select_manifest_files(&resolution.manifest, &supported)?;
        append_additional_component_operations(
            &mut operations,
            &selections,
            &lock,
            &root,
            &maintenance_identity(&lock, &root),
            requested.iter().any(|id| id == "workflow.super_events"),
        )?;
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
        // A deselected client's MCP file loses only the installer's routes.
        // Its bytes come from the unchanged live file, not from the source.
        if operation.resolution.as_deref() == Some(crate::transaction::MCP_ROUTE_REMOVAL) {
            prepared.push(crate::transaction::prepare_mcp_route_removal(
                &root,
                operation,
                &crate::transaction::installer_mcp_server_ids(&lock),
            )?);
            continue;
        }
        if operation.action == OperationAction::DeleteManaged
            || operation.source_sha256.is_none()
            || (mode == "remove" && operation.action == OperationAction::Skip)
        {
            continue;
        }
        // A kept generated file has no reconstructible incoming baseline.
        // Preserve its source provenance, but never offer its local bytes as a replacement.
        if prepare_preserved_generated_skip(&lock, operation) {
            continue;
        }
        let expected = operation.source_sha256.as_deref().ok_or_else(|| {
            AppError::Source("maintenance operation has no expected source hash".into())
        })?;
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
                .or_else(|| healthy_generated_bytes(&lock, &root, operation))
                .ok_or_else(|| {
                    AppError::Transaction(format!(
                        "generated maintenance artifact has no recorded content: {}",
                        operation.destination
                    ))
                })?
        } else {
            client.fetch_verified_file(
                &source_revision,
                operation.source_path.as_deref().unwrap_or(""),
                expected,
                operation.source_size,
            )?
        };
        let source_ledger_entry = if source_is_generated {
            None
        } else {
            let source_path = operation.source_path.clone().ok_or_else(|| {
                AppError::Source("remote maintenance operation has no source path".into())
            })?;
            let ownership = operation.ownership.ok_or_else(|| {
                AppError::Source("remote maintenance operation has no ownership evidence".into())
            })?;
            let platform = operation.platform.ok_or_else(|| {
                AppError::Source("remote maintenance operation has no platform evidence".into())
            })?;
            if operation.source_size != Some(source_bytes.len() as u64) {
                return Err(AppError::Source(format!(
                    "remote maintenance source size mismatch: {}",
                    operation.destination
                )));
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
        // `maintenance_components` already reflects additions/removals made
        // by this plan.  Use it for adaptation so a newly selected Codex
        // package receives its MCP route and a deselected one loses it.
        let mcp_selected = component_ids_include_mcp(&maintenance_components);
        let generated_source = operation
            .source_path
            .as_deref()
            .is_some_and(|path| path.starts_with("generated:"));
        let bytes = if generated_source && operation.component_id != "core.agents" {
            source_bytes.clone()
        } else {
            adapt_selected_source(
                &operation.component_id,
                &operation.destination,
                &source_bytes,
                &maintenance_identity(&lock, &root),
                &conventions,
                &lock.ai_provider,
                &lock.ai_model,
                &lock.ai_reasoning_effort,
                maintenance_components
                    .iter()
                    .any(|id| id == "workflow.super_events"),
                mcp_selected,
                maintenance_components.iter().any(|id| id == "workflow.3d"),
                &portrait_pipeline,
                &maintenance_components,
            )?
        };
        let bytes = if is_image_generation_client_config_path(&operation.destination) {
            merge_existing_image_client_config(&root, &operation.destination, &bytes)?
        } else {
            bytes
        };
        let bytes = if let (Some(manifest), Some(selection)) = (
            image_generation_manifest.as_ref(),
            selected_image_generation.as_ref(),
        ) {
            let guided = adapt_image_generation_guidance(&bytes, manifest, selection)?;
            adapt_image_generation_client_config(
                &guided,
                &operation.destination,
                manifest,
                selection,
                &coding_environment_selection,
                &root,
                selected_live_testing.as_ref().unwrap(),
            )?
        } else {
            bytes
        };
        let actual = sha256_bytes(&bytes);
        if generated_source {
            validate_generated_maintenance_content(
                &operation.destination,
                &source_bytes,
                &bytes,
                expected,
            )?;
        }
        // A remote source was verified against its manifest SHA-256 when it
        // was fetched. Adaptation then legitimately changes many files (the
        // project instructions, Codex configuration, subagent TOML, and any
        // file with optional Super Events or portrait sections), and the
        // adapted bytes become this operation's result hash, including for
        // a skipped file: its prepared bytes are what Use new would install,
        // while a skip itself is verified against the unchanged local hash.
        if is_image_generation_client_config_path(&operation.destination)
            && operation.action == OperationAction::Skip
            && operation.local_state == LocalState::Unmodified
            && operation.resolution.is_none()
            && operation.local_sha256.as_deref() != Some(actual.as_str())
        {
            operation.action = if generated_source {
                OperationAction::Generate
            } else {
                OperationAction::Replace
            };
            operation.rollback = RollbackAction::RestoreBackup;
        }
        operation.result_sha256 = Some(actual.clone());
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
                    let base_source = client.fetch_verified_file(
                        &current_file.source_revision,
                        &current_file.source_path,
                        &current_file.source_sha256,
                        None,
                    )?;
                    let base = if current_file.component_id == "codex.config" {
                        adapt_codex_config_for_selection(
                            &base_source,
                            mcp_selected,
                            maintenance_components.iter().any(|id| id == "workflow.3d"),
                            maintenance_components
                                .iter()
                                .any(|id| id == "workflow.super_events"),
                            &portrait_pipeline,
                        )?
                    } else if current_file.component_id == "core.agents" {
                        adapt_agents_for_selection(
                            &base_source,
                            &maintenance_identity(&lock, &root),
                            &lock.ai_provider,
                            &lock.ai_model,
                            &lock.ai_reasoning_effort,
                            &conventions,
                            lock_workflow_selected(&lock, "workflow.super_events"),
                            portrait_pipeline.enabled,
                        )?
                    } else {
                        base_source
                    };
                    let local_path = if current_file.external {
                        validate_external_destination(&current_file.path)
                    } else {
                        safe_join(&root, &current_file.path)
                    }?;
                    let local = std::fs::read(local_path)?;
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
    if mode != "remove" {
        if let (Some(manifest), Some(selection)) = (
            image_generation_manifest.as_ref(),
            selected_image_generation.as_ref(),
        ) {
            let client_config_destinations = image_client_config_source_destinations(
                manifest,
                &coding_environment_selection,
                Platform::current(),
            );
            let client_config_artifacts = image_client_config_artifacts(
                &root,
                manifest,
                selection,
                &coding_environment_selection,
                &client_config_destinations,
                selected_live_testing.as_ref().unwrap(),
            )?;
            append_maintenance_image_client_config_operations(
                &client_config_artifacts,
                &mut operations,
                &mut prepared,
                &lock,
                &root,
            )?;
            generated_artifacts.extend(client_config_artifacts);
            if let Some(guidance) = image_generation_guidance_artifact(manifest, selection)? {
                append_maintenance_image_client_config_operations(
                    std::slice::from_ref(&guidance),
                    &mut operations,
                    &mut prepared,
                    &lock,
                    &root,
                )?;
                generated_artifacts.push(guidance);
            } else if let Some(previous) = lock
                .files
                .iter()
                .find(|file| file.component_id == "image_generation.guidance" && !file.external)
            {
                let obsolete = InstallationLock {
                    files: vec![previous.clone()],
                    ..lock.clone()
                };
                operations.retain(|operation| {
                    operation.external || operation.destination != previous.path
                });
                prepared.retain(|file| file.destination != previous.path);
                operations.extend(crate::transaction::managed_removal_operations(
                    &obsolete, &root,
                )?);
            }
            if let Some(artifact) = image_generation_config_artifact(manifest, selection)? {
                append_maintenance_image_client_config_operations(
                    std::slice::from_ref(&artifact),
                    &mut operations,
                    &mut prepared,
                    &lock,
                    &root,
                )?;
                generated_artifacts.push(artifact);
            } else if let Some(previous) = lock.files.iter().find(|file| {
                file.path == ".hoi4-setup/image_generation.json"
                    && file.component_id == "image_generation.config"
                    && !file.external
            }) {
                operations.retain(|operation| {
                    operation.external || operation.destination != previous.path
                });
                prepared.retain(|file| file.destination != previous.path);
                let path = safe_join(&root, &previous.path)?;
                let local_sha = if path.is_file() {
                    Some(sha256_file(&path)?)
                } else {
                    None
                };
                let local_state = match local_sha.as_deref() {
                    None => LocalState::Absent,
                    Some(hash) if hash == previous.installed_sha256 => LocalState::Unmodified,
                    Some(_) => LocalState::Modified,
                };
                if local_state != LocalState::Absent {
                    let action = if local_state == LocalState::Unmodified {
                        OperationAction::DeleteManaged
                    } else {
                        OperationAction::Skip
                    };
                    operations.push(PlanOperation {
                        id: "maintenance-image-generation-config-remove".into(),
                        component_id: "image_generation.config".into(),
                        ownership: Some(Ownership::Generated),
                        location_scope: Some("project".into()),
                        action,
                        source_path: Some(format!("generated:{}", previous.path)),
                        destination: previous.path.clone(),
                        source_sha256: Some(previous.source_sha256.clone()),
                        source_size: previous.installed_size,
                        platform: Some(previous.platform.unwrap_or(ManifestPlatform::All)),
                        executable: false,
                        result_sha256: None,
                        base_sha256: Some(previous.source_sha256.clone()),
                        local_sha256: local_sha,
                        local_state,
                        resolution: (local_state == LocalState::Modified)
                            .then_some("review_required".into()),
                        external: false,
                        rollback: RollbackAction::RestoreBackup,
                        external_parent_identity: None,
                    });
                }
            }
        }
    }
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
        let flatten_prepared = accepted_flatten_prepared_files(&prepared, &operations, &root)?;
        let flat = crate::flatten::build_artifacts(&flatten_prepared, &[], &root)?;
        generated_artifacts.extend(flat.clone());
        for (index, artifact) in flat.iter().enumerate() {
            let destination_path = safe_join(&root, &artifact.destination)?;
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
                external_parent_identity: None,
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
    let conflicts = maintenance_conflicts(&operations, &prepared, &merge_contexts);
    let mut optional_workflows: BTreeMap<String, String> = lock
        .optional_workflows
        .iter()
        .filter(|(id, _)| id.as_str() != "workflow.lora_comfyui_interest")
        .map(|(id, workflow)| (id.clone(), workflow.state.clone()))
        .collect();
    let mut credential_references = Vec::new();
    if add_optional_components.iter().any(|id| id == "workflow.3d") {
        let mesh_reference = current_meshy_credential_reference()?.or_else(|| {
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
            validate_credential_reference(&reference)?;
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
    if let Some(manifest) = maintenance_mcp_manifest.as_ref() {
        for component in manifest.components.iter().filter(|component| {
            component.optional && maintenance_components.iter().any(|id| id == &component.id)
        }) {
            let component_id = &component.id;
            if matches!(
                component_id.as_str(),
                "workflow.3d" | "workflow.super_events"
            ) || component_id.starts_with("workflow.portraits")
            {
                continue;
            }
            let has_action =
                manifest_external_actions(manifest, std::slice::from_ref(component_id))
                    .iter()
                    .any(|action| action.component_id == *component_id);
            optional_workflows.insert(
                component_id.clone(),
                if has_action {
                    "selected_pending"
                } else {
                    "ready"
                }
                .into(),
            );
        }
    }
    optional_workflows.insert(
        "workflow.portraits".into(),
        if !portrait_pipeline.enabled {
            "not_selected".into()
        } else if portrait_pipeline.provider_status == "ready" {
            "ready".into()
        } else {
            "incomplete".into()
        },
    );
    let mcp_selected = mode != "remove" && component_ids_include_mcp(&maintenance_components);
    let mut external_actions = if mcp_selected {
        // Update already resolved the target manifest above. Repair and
        // reinstall may add Codex to a lock that did not contain it, so load
        // the immutable locked manifest lazily for that transition as well.
        let manifest = if let Some(manifest) = maintenance_mcp_manifest.as_ref() {
            manifest.clone()
        } else {
            resolve_installed_manifest(&lock)?
        };
        let actions = manifest_external_actions(&manifest, &maintenance_components);
        if !actions
            .iter()
            .any(|action| action.component_id == crate::mcp::COMPONENT_ID)
        {
            return Err(AppError::Source(
                "the locked MCP manifest does not declare one reviewed health action".into(),
            ));
        }
        actions
    } else if mode != "remove"
        && Platform::current() == Platform::Windows
        && maintenance_components
            .iter()
            .any(|id| id == LIVE_TOOLS_COMPONENT_ID)
    {
        // HOI4 Live Tools carries its own reviewed bootstrap action, so it
        // is planned even when the HOI4 Agent Tools MCP is not selected.
        let manifest = if let Some(manifest) = maintenance_mcp_manifest.as_ref() {
            manifest.clone()
        } else {
            resolve_installed_manifest(&lock)?
        };
        manifest_external_actions(&manifest, &[LIVE_TOOLS_COMPONENT_ID.to_string()])
    } else {
        Vec::new()
    };
    let gptimage_selected =
        provider_bootstrap_selected(&mode, selected_image_generation.as_ref(), "gptimage");
    if gptimage_selected
        && !external_actions
            .iter()
            .any(|action| action.id.starts_with("external.imagegen.gptimage."))
    {
        let manifest = if let Some(manifest) = image_generation_manifest.as_ref() {
            manifest.clone()
        } else {
            resolve_installed_manifest(&lock)?
        };
        external_actions.extend(manifest_external_actions(
            &manifest,
            &[IMAGEGEN_GPTIMAGE_COMPONENT_ID.into()],
        ));
    }
    let gptimage_installed_with_receipt = lock.components.iter().any(|component| {
        component.id == IMAGEGEN_GPTIMAGE_COMPONENT_ID
            && !matches!(
                component.state.as_str(),
                "removed" | "not_selected" | "unsupported_platform"
            )
    }) && lock
        .image_generation
        .as_ref()
        .and_then(|selection| selection.installed_package_tree_sha256.as_ref())
        .is_some();
    let gptimage_cleanup_required =
        (mode == "remove" || !gptimage_selected) && gptimage_installed_with_receipt;
    if gptimage_cleanup_required {
        external_actions.push(reviewed_gptimage_cleanup_action());
    }
    optional_workflows.insert(
        IMAGEGEN_GPTIMAGE_COMPONENT_ID.into(),
        if gptimage_selected {
            "selected_pending"
        } else {
            "not_selected"
        }
        .into(),
    );
    let external_actions_reviewed = external_actions.is_empty();
    if let Some(record) = codex_analysis.as_ref() {
        validate_analysis_source_binding(record, &plan_source)?;
    }
    let project_root_identity = {
        let approved = codex_approved_evidence()
            .lock()
            .map_err(|_| AppError::Scan("approved scan evidence store is unavailable".into()))?;
        if approved
            .project_root
            .as_ref()
            .is_none_or(|approved_root| !same_project_root(approved_root, &root))
        {
            return Err(AppError::Scan(
                "maintenance planning requires a fresh scan of this project".into(),
            ));
        }
        let expected_scan = (mode == "update")
            .then(|| analysis_override.as_ref().and_then(|record| record.scan_id))
            .flatten();
        validate_approved_project_root_identity(&root, &approved, expected_scan)?
    };
    let mut plan = InstallationPlan {
        schema_version: crate::migrations::CURRENT_PLAN_SCHEMA.into(),
        plan_id: Uuid::new_v4(),
        project_id: lock.project_id.clone(),
        script_prefix: lock.script_prefix.clone(),
        primary_namespace: lock.primary_namespace.clone(),
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        maintenance_mode: Some(mode.clone()),
        source: plan_source,
        ai_provider: lock.ai_provider.clone(),
        ai_model: lock.ai_model.clone(),
        ai_reasoning_effort: lock.ai_reasoning_effort.clone(),
        ai_endpoint: lock.ai_endpoint.clone(),
        ai_optimization_profile: lock.ai_optimization_profile.clone(),
        flatten_chat_sources: lock.flatten_chat_sources,
        codex_analysis,
        selected_components: maintenance_components,
        primary_coding_environment: coding_environment_selection.primary.clone(),
        additional_coding_environments: coding_environment_selection.additional.clone(),
        wiki_required_pages,
        wiki_metadata,
        generated_artifacts,
        download_ledger,
        git_setup: None,
        credential_references,
        optional_workflows,
        portrait_pipeline: Some(portrait_pipeline),
        image_generation: selected_image_generation,
        live_testing: selected_live_testing,
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
            directories: Vec::new(),
            atomic_apply_expected: true,
            project_root_mode: ProjectRootMode::Existing,
            project_root_parent: None,
            project_root_leaf: None,
            project_root_identity: Some(project_root_identity),
        },
        approvals: PlanApprovals {
            dry_run_reviewed: false,
            external_actions_reviewed,
            git_remote_approved: false,
            push_approved: false,
        },
    };
    crate::transaction::bind_plan_external_parents(&mut plan)?;
    ensure_mcp_package_replaceable(&plan)?;
    store_prepared_plan(plan, prepared, merge_contexts, root)
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
    approve_prepared_plan(prepared);
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
    resolve_conflict(prepared, &path, &choice)
}

/// What the development tools step did, for the Ready screen.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
struct AgentSetupResult {
    /// `ready`, `incomplete`, or `not_needed`.
    state: String,
    detail: String,
}

fn bounded_tool_output(result: &crate::process::ProcessResult) -> String {
    let mut text = String::new();
    text.push_str(result.stdout.trim());
    if !result.stderr.trim().is_empty() {
        text.push('\n');
        text.push_str(result.stderr.trim());
    }
    let redacted = redact_secrets(&text, &[]);
    let tail: String = redacted
        .chars()
        .rev()
        .take(1200)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    tail.trim().to_string()
}

/// Run the package's `check.py setup` once, after an apply, as the reviewed
/// plan describes it, and record its effect in the lock. Never fails the
/// installation: the project is usable without it, and the Ready screen shows
/// what is left.
fn run_agent_setup_blocking(
    plan: &InstallationPlan,
    root: &Path,
) -> Result<AgentSetupResult, AppError> {
    let Some(action) = crate::agent_setup::reviewed_action(plan)? else {
        return Ok(AgentSetupResult {
            state: "not_needed".into(),
            detail: String::new(),
        });
    };
    crate::agent_setup::verify_script(root, plan)?;
    let lock = read_project_lock(root)?;
    let record = |changes: &crate::agent_setup::Changes, state: &str, detail: &str| {
        crate::agent_setup::amend_lock(
            root,
            changes,
            state,
            (!detail.is_empty()).then(|| detail.to_string()),
        )
    };
    // The Microsoft Store's python.exe is a launcher stub, not an interpreter.
    let python = crate::process::find_path_executable(&["python3", "python.exe", "python"])
        .ok()
        .filter(|path| !path.to_string_lossy().contains("WindowsApps"));
    let Some(python) = python else {
        let detail = "Python 3.9 or newer was not found, so the development tools were not prepared. Install it from python.org, then run Repair.";
        record(&Default::default(), "incomplete", detail)?;
        return Ok(AgentSetupResult {
            state: "incomplete".into(),
            detail: detail.into(),
        });
    };
    let before = crate::agent_setup::snapshot(root, &lock)?;
    let mut args = vec!["-B".to_string()];
    args.extend(action.arguments.iter().cloned());
    let spec = crate::process::ProcessSpec {
        executable: python.clone(),
        executable_sha256: Some(sha256_file(&python)?),
        args,
        cwd: Some(root.to_path_buf()),
        platform: Platform::current(),
        environment_names: vec![],
        timeout_seconds: 20 * 60,
        max_output_bytes: 2 * 1024 * 1024,
    };
    let run = spec.run(&[python], None);
    let changes = crate::agent_setup::changes_since(root, &before)?;
    let (state, detail) = match &run {
        _ if !changes.unexpected.is_empty() => (
            "incomplete",
            format!(
                "The setup command changed files it must not: {}.",
                changes.unexpected.join(", ")
            ),
        ),
        Ok(result) if result.status_code == Some(0) && !result.timed_out => {
            ("ready", String::new())
        }
        Ok(result) if result.timed_out => (
            "incomplete",
            "The setup command took too long and was stopped; run Repair to try again.".into(),
        ),
        Ok(result) => ("incomplete", bounded_tool_output(result)),
        Err(error) => (
            "incomplete",
            redact_secrets(&format!("The setup command could not run: {error}"), &[]),
        ),
    };
    record(&changes, state, &detail)?;
    Ok(AgentSetupResult {
        state: state.into(),
        detail,
    })
}

#[tauri::command(async)]
fn run_agent_setup(plan_id: String, project_root: String) -> Result<AgentSetupResult, String> {
    let id = Uuid::parse_str(&plan_id).map_err(|_| "invalid installation plan ID".to_string())?;
    let root = validate_project_root_or_destination(Path::new(&project_root))
        .map(|(root, _)| root)
        .map_err(command_error)?;
    let plan = {
        let plans = prepared_plans()
            .lock()
            .map_err(|_| "prepared plan store is unavailable".to_string())?;
        let prepared = plans
            .get(&id)
            .ok_or_else(|| "installation plan is not available in the core session".to_string())?;
        if prepared.canonical_root != root {
            return Err("installation root changed after review".into());
        }
        prepared.plan.clone()
    };
    run_agent_setup_blocking(&plan, &root).map_err(command_error)
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
    // An MCP client may have started after the review; refuse before any
    // project file changes rather than failing at readiness.
    ensure_mcp_package_replaceable(&approved_plan).map_err(planning_command_error)?;
    // A missing tool the setup checks run is reported while nothing has been
    // written, so a first install never needs an undo for a missing prerequisite.
    ensure_setup_requirements_met(&approved_plan)?;
    let (journal, _) = run_transaction(
        &root,
        &approved_plan,
        &prepared,
        &TransactionOptions {
            post_install_action_runner: Some(run_reviewed_post_install_actions),
            ..Default::default()
        },
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
    let (journal, _) = resume_transaction_with_options(
        &root,
        &app_root,
        id,
        Some(run_reviewed_post_install_actions),
    )
    .map_err(command_error)?;
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
    let lock = read_project_lock(&root).map_err(command_error)?;
    let workflow_3d_state = lock
        .optional_workflows
        .get("workflow.3d")
        .map(|workflow| workflow.state.clone())
        .unwrap_or_else(|| "not_selected".into());
    let readiness =
        evaluate_installed_readiness(&root, &lock.project_id, workflow_3d_state, Vec::new())
            .map_err(command_error)?;
    if !readiness.open_in_codex.enabled {
        return Err(format!(
            "Codex remains disabled until its installed integration and readiness checks pass: {}",
            readiness.open_in_codex.blocking_check_ids.join(", ")
        ));
    }
    let Some(executable) = find_codex_executable() else {
        return Ok(manual_open_in_codex_result(&root));
    };
    if Platform::current() == Platform::Linux {
        return Ok(match launch_in_terminal(&root, &executable) {
            Ok(()) => OpenInCodexResult {
                opened: false,
                message: "Project terminal opened. Run codex to start Codex in this project."
                    .into(),
            },
            Err(_) => OpenInCodexResult {
                opened: false,
                message: format!("Open a terminal in {} and run codex.", root.display()),
            },
        });
    }
    let spec = crate::process::ProcessSpec {
        executable: executable.clone(),
        executable_sha256: Some(
            crate::process::verified_executable_sha256(&executable, "OpenAI")
                .map_err(command_error)?,
        ),
        args: codex_app_args(&root),
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

/// Open the prepared project in the selected coding environment. Codex keeps
/// its verified app opener; Claude Code starts in a new terminal window in the
/// project folder; other environments get a clear manual instruction.
#[tauri::command(async)]
fn open_in_coding_environment(
    project_root: String,
    environment: String,
) -> Result<OpenInCodexResult, String> {
    if environment == crate::coding_environment::CODEX {
        return open_in_codex(project_root);
    }
    if !crate::coding_environment::is_supported(&environment) {
        return Err(format!("unsupported coding environment: {environment}"));
    }
    let root = validate_project_root(Path::new(&project_root)).map_err(command_error)?;
    let lock = read_project_lock(&root).map_err(command_error)?;
    let workflow_3d_state = lock
        .optional_workflows
        .get("workflow.3d")
        .map(|workflow| workflow.state.clone())
        .unwrap_or_else(|| "not_selected".into());
    let readiness =
        evaluate_installed_readiness(&root, &lock.project_id, workflow_3d_state, Vec::new())
            .map_err(command_error)?;
    if !readiness.core_ready {
        return Err("Resolve the blocking readiness checks before opening the project.".into());
    }
    if environment == crate::coding_environment::CLAUDE_CODE {
        if let Ok(executable) = crate::claude_code::verified_executable_path() {
            if launch_in_terminal(&root, &executable).is_ok() {
                return Ok(OpenInCodexResult {
                    opened: Platform::current() != Platform::Linux,
                    message: if Platform::current() == Platform::Linux {
                        "Project terminal opened. Run claude to start Claude Code in this project."
                    } else {
                        "Claude Code started in a new terminal window in the project folder."
                    }
                    .into(),
                });
            }
        }
        return Ok(OpenInCodexResult {
            opened: false,
            message: format!(
                "Open a terminal in {} and run claude to start Claude Code in this project.",
                root.display()
            ),
        });
    }
    // Other clients are never launched: their executables have no verified
    // identity here. The message names the client and its documented command.
    let message = match coding_environment::native_client(&environment) {
        Some(client) => match client.open_command {
            Some(command) => format!(
                "Open a terminal in {} and run {command} to start {}.",
                root.display(),
                client.display_name
            ),
            None => format!(
                "Open this folder in {}: {}",
                client.display_name,
                root.display()
            ),
        },
        None => format!(
            "Open this folder in your coding environment: {}",
            root.display()
        ),
    };
    Ok(OpenInCodexResult {
        opened: false,
        message,
    })
}

/// Start an interactive command-line tool in a new terminal window whose
/// working directory is the project folder.
fn launch_in_terminal(root: &Path, executable: &Path) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let system_root = std::env::var_os("SystemRoot")
            .ok_or_else(|| AppError::Process("SystemRoot is unavailable".into()))?;
        let cmd = PathBuf::from(system_root).join("System32").join("cmd.exe");
        let line = format!(
            "/d /c start \"Claude Code\" /d \"{}\" \"{}\"",
            root.display(),
            executable.display()
        );
        if line.contains('%') || line.contains('^') {
            return Err(AppError::Process(
                "the project path cannot be passed to a terminal safely".into(),
            ));
        }
        let mut command = std::process::Command::new(cmd);
        command.raw_arg(line).current_dir(root);
        crate::process::configure_child_no_console_window(&mut command);
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| AppError::Process(format!("the terminal could not start: {error}")))
    }
    #[cfg(target_os = "macos")]
    {
        let quote =
            |value: &Path| format!("'{}'", value.display().to_string().replace('\'', r"'\''"));
        let shell = format!("cd {} && {}", quote(root), quote(executable));
        let script = format!(
            "tell application \"Terminal\" to do script \"{}\"",
            shell.replace('\\', "\\\\").replace('"', "\\\"")
        );
        std::process::Command::new("/usr/bin/osascript")
            .args([
                "-e",
                &script,
                "-e",
                "tell application \"Terminal\" to activate",
            ])
            .spawn()
            .map(|_| ())
            .map_err(|error| AppError::Process(format!("Terminal could not start: {error}")))
    }
    #[cfg(target_os = "linux")]
    {
        let _ = executable;
        for (name, arguments) in [
            (
                "gnome-terminal",
                vec!["--working-directory".into(), root.display().to_string()],
            ),
            (
                "konsole",
                vec!["--workdir".into(), root.display().to_string()],
            ),
            ("xterm", vec![]),
        ] {
            let Ok(terminal) = std::fs::canonicalize(Path::new("/usr/bin").join(name)) else {
                continue;
            };
            let Ok(hash) = crate::process::verified_executable_sha256(&terminal, "Linux system")
            else {
                continue;
            };
            return crate::process::ProcessSpec {
                executable: terminal.clone(),
                executable_sha256: Some(hash),
                args: arguments,
                cwd: Some(root.to_path_buf()),
                platform: Platform::Linux,
                environment_names: vec![],
                timeout_seconds: 10,
                max_output_bytes: 4096,
            }
            .spawn_detached(&[terminal]);
        }
        Err(AppError::Process(
            "a verified desktop terminal is unavailable; open a terminal in the project folder"
                .into(),
        ))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (root, executable);
        Err(AppError::UnsupportedPlatform(
            "opening a terminal is supported on Windows and macOS".into(),
        ))
    }
}

fn codex_app_args(root: &Path) -> Vec<String> {
    vec!["app".into(), root.display().to_string()]
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
#[path = "commands_lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::plans::existing_project_file_is_kept_by_default;
    use std::cell::Cell;
    use std::sync::MutexGuard;
    use tempfile::tempdir;

    static COMMAND_TEST_STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn app_info_lists_all_supported_desktop_platforms() {
        assert_eq!(
            app_info()["supported_platforms"],
            serde_json::json!(["windows", "macos", "linux"])
        );
    }

    #[test]
    fn descriptor_conflicts_preview_text_without_changing_merge_classification() {
        for path in ["descriptor.mod", "launcher/MyMod.MOD"] {
            let merge_kind = path_file_kind(path);
            assert!(matches!(merge_kind, FileKind::Binary));
            let preview_kind = conflict_preview_kind(path, merge_kind);
            assert!(matches!(preview_kind, FileKind::Text));
            let descriptor = b"name=\"Example Mod\"\nreplace_path=\"common/ideas\"\n";
            assert_eq!(
                preview_text(preview_kind, Some(descriptor)).0.as_deref(),
                Some(std::str::from_utf8(descriptor).unwrap())
            );
            assert_eq!(preview_text(preview_kind, Some(&[0xff])).0, None);
            assert_eq!(preview_text(preview_kind, None).0, None);
        }
        for kind in [FileKind::Symlink, FileKind::Directory] {
            assert!(!matches!(
                conflict_preview_kind("descriptor.mod", kind),
                FileKind::Text
            ));
        }
        assert!(matches!(
            conflict_preview_kind("thumbnail.png", FileKind::Binary),
            FileKind::Binary
        ));
    }

    fn bootstrap_action(component_id: &str) -> ExternalAction {
        serde_json::from_value(serde_json::json!({
            "id": format!("external.{component_id}"),
            "component_id": component_id,
            "platform": Platform::current().manifest_name(),
            "command_source": "repository_script",
            "risk": "medium",
            "requires_approval": true,
        }))
        .unwrap()
    }

    fn requirement_plan(mcp_state: Option<&str>, bootstrap: bool) -> InstallationPlan {
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        plan.maintenance_mode = None;
        plan.external_actions = if bootstrap {
            vec![bootstrap_action("mcp.hoi4_agent_tools.bootstrap")]
        } else {
            Vec::new()
        };
        plan.optional_workflows.clear();
        if let Some(state) = mcp_state {
            plan.optional_workflows
                .insert(crate::mcp::COMPONENT_ID.into(), state.into());
        }
        plan
    }

    #[test]
    fn an_existing_mods_own_project_files_are_kept_by_default() {
        let dir = tempdir().unwrap();
        // A real Workshop descriptor: no picture line, extra Workshop fields.
        let descriptor = dir.path().join("descriptor.mod");
        std::fs::write(
            &descriptor,
            "version=\"1.2\"\ntags={\n\t\"Military\"\n}\nname=\"Great War Redux\"\nsupported_version=\"1.12.*\"\nremote_file_id=\"2924972770\"\n",
        )
        .unwrap();
        assert!(existing_project_file_is_kept_by_default(
            "project.descriptor",
            &descriptor,
            false
        ));
        // A descriptor without a name is not the mod's own usable file.
        std::fs::write(&descriptor, "version=\"1.2\"\n").unwrap();
        assert!(!existing_project_file_is_kept_by_default(
            "project.descriptor",
            &descriptor,
            false
        ));
        // A valid thumbnail is kept unless the user chose a new one in the app.
        let thumbnail = dir.path().join("thumbnail.png");
        std::fs::write(
            &thumbnail,
            crate::descriptors::placeholder_thumbnail_png().unwrap(),
        )
        .unwrap();
        assert!(existing_project_file_is_kept_by_default(
            "project.thumbnail",
            &thumbnail,
            false
        ));
        assert!(!existing_project_file_is_kept_by_default(
            "project.thumbnail",
            &thumbnail,
            true
        ));
        std::fs::write(&thumbnail, b"not a png").unwrap();
        assert!(!existing_project_file_is_kept_by_default(
            "project.thumbnail",
            &thumbnail,
            false
        ));
        // A README is kept; other generated files are never kept this way.
        let readme = dir.path().join("README.md");
        std::fs::write(&readme, "# My mod\n").unwrap();
        assert!(existing_project_file_is_kept_by_default(
            "project.readme",
            &readme,
            false
        ));
        assert!(!existing_project_file_is_kept_by_default(
            "git.ignore",
            &readme,
            false
        ));
    }

    #[test]
    #[ignore = "timing probe: set HOI4_TIME_PROJECT to an installed project (and APPDATA to its profile)"]
    fn time_installed_readiness() {
        let Some(root) = std::env::var_os("HOI4_TIME_PROJECT") else {
            return;
        };
        let root = PathBuf::from(root);
        let started = std::time::Instant::now();
        let mut detected = crate::readiness::project_input(&root, "probe").unwrap();
        eprintln!("project_input: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        refresh_installed_mcp_readiness(&root, &mut detected).unwrap();
        eprintln!(
            "mcp refresh: {:?} -> {}",
            started.elapsed(),
            detected.mcp_status
        );
        let started = std::time::Instant::now();
        let report = crate::readiness::evaluate(&detected);
        eprintln!(
            "evaluate: {:?} core_ready={}",
            started.elapsed(),
            report.core_ready
        );
    }

    #[test]
    fn a_changed_assistant_address_is_named_in_the_planning_message() {
        let message = planning_user_message(AppError::Credential(
            "update reanalysis was confirmed against a different provider endpoint".into(),
        ));
        assert!(message.contains("different assistant address"));
        assert!(message.contains("Nothing was changed."));
    }

    #[test]
    fn a_pinned_version_whose_manifest_disagrees_with_its_files_is_explained() {
        let message = planning_user_message(AppError::Source(
            "source blob response exceeds its bounded size limit".into(),
        ));
        assert!(message.contains("do not match its published manifest"));
        assert!(message.contains("Choose Latest, a release, or a newer commit"));
        assert!(!message.contains("connection"));
    }

    #[test]
    fn a_missing_python_is_reported_with_the_safe_next_step_before_install() {
        let missing = python_requirement(Err(McpPython::Missing));
        assert_eq!(missing.status, "missing");
        assert!(missing.detail.contains("python.org"));
        assert!(missing.detail.contains("Check again"));
        let unverified = python_requirement(Err(McpPython::PublisherNotVerified));
        assert_eq!(unverified.status, "missing");
        assert!(unverified.detail.contains("python.org build"));
        let found = python_requirement(Ok(PathBuf::from("python.exe")));
        assert_eq!(found.status, "ready");
        assert_eq!(found.id, PYTHON_REQUIREMENT_ID);
    }

    #[test]
    fn setup_requirements_cover_only_plans_that_run_the_mcp_bootstrap() {
        if !matches!(Platform::current(), Platform::Windows | Platform::Linux) {
            return;
        }
        // The bootstrap runs for a selected, pending MCP route.
        for state in ["selected_pending", "incomplete", "ready"] {
            let plan = requirement_plan(Some(state), true);
            assert_eq!(
                setup_requirements(&plan)
                    .iter()
                    .map(|requirement| requirement.id.as_str())
                    .collect::<Vec<_>>(),
                vec![PYTHON_REQUIREMENT_ID],
                "{state}"
            );
        }
        // No bootstrap action, an unselected route, or a removal needs no Python.
        assert!(setup_requirements(&requirement_plan(Some("selected_pending"), false)).is_empty());
        assert!(setup_requirements(&requirement_plan(Some("not_selected"), true)).is_empty());
        assert!(setup_requirements(&requirement_plan(None, true)).is_empty());
        let mut removal = requirement_plan(Some("selected_pending"), true);
        removal.maintenance_mode = Some("remove".into());
        assert!(setup_requirements(&removal).is_empty());
    }

    #[test]
    fn installing_without_a_required_tool_stops_before_any_change() {
        if !matches!(Platform::current(), Platform::Windows | Platform::Linux) {
            return;
        }
        let plan = requirement_plan(Some("selected_pending"), true);
        let outcome = ensure_setup_requirements_met(&plan);
        // Whether this computer has python.org Python decides the outcome; the
        // message, when it fails, names the fix and states nothing changed.
        match (mcp_bootstrap_python(), outcome) {
            (Ok(_), Ok(())) => {}
            (Err(_), Err(message)) => {
                assert!(message.contains("Python for HOI4 Agent Tools"));
                assert!(message.contains("Check again"));
                assert!(message.ends_with("Nothing was changed."));
            }
            (found, outcome) => panic!(
                "requirement check disagrees with the bootstrap lookup: {found:?} {outcome:?}"
            ),
        }
    }

    pub(super) fn test_state_guard() -> MutexGuard<'static, ()> {
        COMMAND_TEST_STATE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn super_events_test_identity() -> ProjectIdentity {
        ProjectIdentity {
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
        }
    }

    fn image_generation_test_manifest() -> RemoteManifest {
        let environments = serde_json::json!([
            "claude_code",
            "codex",
            "cursor",
            "qoder",
            "opencode",
            "gemini_cli",
            "antigravity",
            "grok_build",
            "github_copilot",
            "qwen_code"
        ]);
        let route = serde_json::json!({
            "kind": "stdio_adapter", "auth_method": "api_key_vault", "server_id": "hoi4_image_openai_api",
            "url": null, "component_id": "imagegen.runtime", "runtime_entry": ".tools/image_generation/runtime/server.py",
            "api_protocol": "openai_images", "api_base_url": "https://api.openai.com/v1", "default_model": "gpt-image-2",
            "supported_models": ["gpt-image-2"],
            "tool_names": {"generate": "generate_image"}, "tool_arguments": {"prompt": "prompt", "output_path": null},
            "credential_fields": [{"id": "api_key", "display_name": "OpenAI API key", "environment_name": "OPENAI_API_KEY"}],
            "requires_primary_coding_environment": null,
        });
        let capabilities = serde_json::json!({
            "reference_images": {"supported": true, "max_count": 4, "notes": "Pass reference files by path."},
            "image_edits": true,
            "transparent_background": {"native": true, "notes": "Request transparency and verify the PNG alpha."},
            "size_limits": {"accepts_dimensions": true, "maximum_width_px": 3840, "maximum_height_px": 3840, "maximum_pixels": 8_294_400, "notes": "Use model-supported dimensions."},
            "one_image_per_call": true,
            "alpha_verification": "Check the saved alpha.",
            "fallback_if_no_alpha": "Use background removal and check the saved alpha.",
            "fallback_if_no_edits": "Make a new image from the reference.",
        });
        RemoteManifest {
            schema_version: "4.0.0".into(),
            manifest_id: "image-test".into(),
            generated_for_revision: None,
            repository: RepositoryDescriptor {
                provider: "github".into(),
                owner: "owner".into(),
                name: "repo".into(),
                default_branch: "main".into(),
                web_url: None,
                api_base: None,
                license_evidence: None,
            },
            components: Vec::new(),
            profiles: Vec::new(),
            wiki: WikiDefinition {
                component_id: "wiki.snapshot".into(),
                destination: "paradox_wiki/".into(),
                snapshot_marker: None,
                required_pages: vec![],
                required_media_policy: "none".into(),
                provenance: WikiProvenance {
                    source_status: "unknown".into(),
                    license_status: "unknown".into(),
                    notes: vec![],
                },
            },
            update_policy: ManifestUpdatePolicy {
                latest: LatestPolicy {
                    resolve_default_branch: true,
                    record_commit: true,
                },
                pinned: PinnedPolicy {
                    allow_commit: true,
                    allow_release: true,
                },
                rollback_retention: 1,
                manifest_cache_ttl_seconds: None,
            },
            signing: None,
            coding_environments: Vec::new(),
            live_testing: None,
            image_generation: Some(serde_json::json!({
                "default_provider": "openai_api",
                "providers": [{
                    "id": "openai_api", "display_name": "OpenAI Images API", "description": "Images with a saved API key.",
                    "availability": "supported", "requirements": ["An API key."], "good_for": ["Reference edits"],
                    "cost_summary": "Pay per image.", "models": ["GPT Image"], "supported_coding_environments": environments,
                    "route": route, "capabilities": capabilities,
                }, {
                    "id": "none", "display_name": "None", "description": "Manual art.",
                    "availability": "supported", "requirements": [], "good_for": [],
                    "cost_summary": "No provider charge.", "models": [],
                    "supported_coding_environments": environments,
                    "route": {"kind": "manual", "auth_method": "none"},
                    "capabilities": {}
                }]
            })),
        }
    }

    #[test]
    fn image_selection_is_manifest_bound_and_project_config_contains_only_vault_references() {
        let manifest = image_generation_test_manifest();
        let environments = CodingEnvironmentSelection {
            primary: "cursor".into(),
            additional: vec!["codex".into()],
        };
        let reference = image_generation_credential_reference("openai_api", "api_key").unwrap();
        let value = serde_json::json!({"imageGeneration": {
            "primary_provider": "openai_api", "additional_providers": [], "route_settings": {},
            "credential_references": {"openai_api.api_key": reference.reference}
        }});
        let selection =
            image_generation_selection_from_state(&value, &manifest, &environments).unwrap();
        let artifact = image_generation_config_artifact(&manifest, &selection)
            .unwrap()
            .unwrap();
        assert_eq!(artifact.destination, ".hoi4-setup/image_generation.json");
        assert!(artifact
            .content
            .contains("credential://image_generation_api_key/openai_api_api_key"));
        assert!(!artifact.content.contains("actual-secret-value"));
        assert!(artifact.content.contains("openai_images"));

        let selected_model = serde_json::json!({"imageGeneration": {
            "primary_provider": "openai_api", "additional_providers": [],
            "route_settings": {"openai_api": {"model": "gpt-image-2"}},
            "credential_references": {"openai_api.api_key": reference.reference}
        }});
        let model_selection =
            image_generation_selection_from_state(&selected_model, &manifest, &environments)
                .unwrap();
        assert_eq!(
            model_selection.route_settings["openai_api"]["model"],
            "gpt-image-2"
        );
        let mut aws_manifest = manifest.clone();
        let aws_provider = aws_manifest
            .image_generation
            .as_mut()
            .and_then(Value::as_object_mut)
            .and_then(|catalog| catalog.get_mut("providers"))
            .and_then(Value::as_array_mut)
            .and_then(|providers| providers.first_mut())
            .unwrap();
        aws_provider["id"] = Value::String("aws_bedrock".into());
        aws_provider["route"]["api_protocol"] = Value::String("aws_bedrock".into());
        aws_provider["route"]["supported_models"] =
            serde_json::json!(["stability.stable-image-ultra-v1:1"]);
        aws_provider["route"]["default_model"] =
            Value::String("stability.stable-image-ultra-v1:1".into());
        aws_provider["models"] = serde_json::json!(["Stable Image Ultra"]);
        let aws_settings = serde_json::json!({"imageGeneration": {
            "primary_provider": "aws_bedrock", "additional_providers": [],
            "route_settings": {"aws_bedrock": {"region": "eu-west-1", "model": "stability.stable-image-ultra-v1:1"}},
            "credential_references": {}
        }});
        let aws_selection =
            image_generation_selection_from_state(&aws_settings, &aws_manifest, &environments)
                .unwrap();
        assert_eq!(
            aws_selection.route_settings["aws_bedrock"]["region"],
            "eu-west-1"
        );
        assert_eq!(
            aws_selection.route_settings["aws_bedrock"]["model"],
            "stability.stable-image-ultra-v1:1"
        );
        let arbitrary_model = serde_json::json!({"imageGeneration": {
            "primary_provider": "openai_api", "additional_providers": [],
            "route_settings": {"openai_api": {"model": "arbitrary-model"}}, "credential_references": {}
        }});
        assert!(
            image_generation_selection_from_state(&arbitrary_model, &manifest, &environments)
                .is_err()
        );

        let invalid = serde_json::json!({"imageGeneration": {
            "primary_provider": "not-in-manifest", "additional_providers": [], "route_settings": {}, "credential_references": {}
        }});
        assert!(image_generation_selection_from_state(&invalid, &manifest, &environments).is_err());
    }

    #[test]
    fn gptimage_cleanup_removes_only_the_recorded_dependency_tree() {
        let project = tempdir().unwrap();
        let package_root = project.path().join(IMAGEGEN_GPTIMAGE_PACKAGE_ROOT);
        let node_modules = package_root.join("node_modules");
        std::fs::create_dir_all(node_modules.join("sample")).unwrap();
        std::fs::write(
            node_modules.join("sample/package.json"),
            b"{\"name\":\"sample\"}",
        )
        .unwrap();
        let expected = crate::mcp::package_tree_sha256(&node_modules).unwrap();

        assert_eq!(
            remove_gptimage_dependencies_if_unchanged(project.path(), &expected).unwrap(),
            GptimageCleanupResult::Removed
        );
        assert!(!node_modules.exists());

        std::fs::create_dir_all(node_modules.join("sample")).unwrap();
        std::fs::write(node_modules.join("sample/package.json"), b"installed tree").unwrap();
        let recorded = crate::mcp::package_tree_sha256(&node_modules).unwrap();
        std::fs::write(node_modules.join("sample/local-change.txt"), b"keep me").unwrap();
        assert_eq!(
            remove_gptimage_dependencies_if_unchanged(project.path(), &recorded).unwrap(),
            GptimageCleanupResult::ModifiedKept
        );
        assert!(node_modules.join("sample/local-change.txt").exists());

        assert_eq!(
            remove_gptimage_dependencies_if_unchanged(project.path(), &recorded).unwrap(),
            GptimageCleanupResult::ModifiedKept
        );
    }

    #[test]
    fn historical_pre_provider_skills_remain_reproducible_without_weakening_current_ceiling() {
        let legacy: RemoteManifest =
            serde_json::from_slice(include_bytes!("../../docs/fixtures/v0.3.5-manifest.json"))
                .unwrap();
        assert!(legacy.image_generation.is_none());
        let current: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        assert!(current.image_generation.is_some());
        let bytes = vec![b'x'; SKILL_BYTE_LIMIT + 1];
        let image = ImageGenerationSelection {
            primary_provider: "none".into(),
            ..Default::default()
        };
        let environments = CodingEnvironmentSelection {
            primary: "codex".into(),
            additional: vec![],
        };
        let live = LiveTestingSelection {
            provider: "none".into(),
            ..Default::default()
        };
        let destination = ".agents/skills/hoi4-decisions-missions/SKILL.md";
        let reproduced = adapt_image_generation_client_config(
            &bytes,
            destination,
            &legacy,
            &image,
            &environments,
            Path::new("C:/mods/example"),
            &live,
        )
        .unwrap();
        assert_eq!(reproduced, bytes);
        let rejected = adapt_image_generation_client_config(
            &bytes,
            destination,
            &current,
            &image,
            &environments,
            Path::new("C:/mods/example"),
            &live,
        )
        .unwrap_err();
        assert!(rejected.to_string().contains("58000-byte limit"));
        assert_eq!(
            adapt_image_generation_client_config(
                &bytes[..SKILL_BYTE_LIMIT],
                destination,
                &current,
                &image,
                &environments,
                Path::new("C:/mods/example"),
                &live
            )
            .unwrap()
            .len(),
            SKILL_BYTE_LIMIT
        );
    }

    #[test]
    fn many_image_providers_use_a_bounded_skill_and_complete_managed_reference() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let providers = manifest.image_generation.as_ref().unwrap()["providers"]
            .as_array()
            .unwrap();
        let image = ImageGenerationSelection {
            primary_provider: "openai_api".into(),
            additional_providers: providers
                .iter()
                .filter_map(|row| row["id"].as_str())
                .filter(|id| *id != "openai_api" && *id != "none")
                .map(str::to_owned)
                .collect(),
            ..Default::default()
        };
        let skill = format!("---\nname: bounded\ndescription: fixture\n---\n{}\n<!-- HOI4_MOD_SETUP:IMAGE_GENERATION_PROVIDER:START -->\nold\n<!-- HOI4_MOD_SETUP:IMAGE_GENERATION_PROVIDER:END -->\n", "x".repeat(56_800));
        let adapted = adapt_image_generation_guidance(skill.as_bytes(), &manifest, &image).unwrap();
        assert!(adapted.len() <= SKILL_BYTE_LIMIT);
        assert!(String::from_utf8(adapted)
            .unwrap()
            .contains(IMAGE_GUIDANCE_REFERENCE));
        let reference = image_generation_guidance_artifact(&manifest, &image)
            .unwrap()
            .unwrap();
        for id in std::iter::once(&image.primary_provider).chain(image.additional_providers.iter())
        {
            assert!(reference.content.contains(&format!("`{id}`")), "{id}");
        }
        assert!(reference.content.contains("one image per call"));
        assert!(reference.content.contains("alpha below 255"));
        assert!(reference.content.contains("If unavailable:"));
        assert!(reference.content.contains("output_path:"));
        let root = tempfile::tempdir().unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.files.clear();
        let mut operations = vec![];
        let mut prepared = vec![];
        append_maintenance_image_client_config_operations(
            std::slice::from_ref(&reference),
            &mut operations,
            &mut prepared,
            &lock,
            root.path(),
        )
        .unwrap();
        assert_eq!(operations[0].action, OperationAction::Generate);
        assert_eq!(operations[0].component_id, "image_generation.guidance");
        assert_eq!(operations[0].rollback, RollbackAction::RemoveCreated);
        assert_eq!(prepared[0].bytes, reference.content.as_bytes());
        let mut baseline: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let mut file = baseline.files[0].clone();
        file.path = IMAGE_GUIDANCE_REFERENCE.into();
        file.component_id = reference.component_id.clone();
        file.external = false;
        file.ownership = Ownership::Generated;
        file.source_path = format!("generated:{IMAGE_GUIDANCE_REFERENCE}");
        file.source_sha256 = reference.expected_sha256.clone();
        file.installed_sha256 = reference.expected_sha256.clone();
        file.source_size = Some(reference.content.len() as u64);
        file.installed_size = file.source_size;
        file.generated_content = Some(reference.content.clone());
        baseline.files = vec![file];
        std::fs::create_dir_all(root.path().join(".tools/image_generation")).unwrap();
        std::fs::write(
            root.path().join(IMAGE_GUIDANCE_REFERENCE),
            reference.content.as_bytes(),
        )
        .unwrap();
        let replacement = image_generation_guidance_artifact(
            &manifest,
            &ImageGenerationSelection {
                primary_provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        append_maintenance_image_client_config_operations(
            std::slice::from_ref(&replacement),
            &mut operations,
            &mut prepared,
            &baseline,
            root.path(),
        )
        .unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].action, OperationAction::Generate);
        assert_eq!(operations[0].rollback, RollbackAction::RestoreBackup);
        assert_eq!(prepared[0].bytes, replacement.content.as_bytes());
        let removal =
            crate::transaction::managed_removal_operations(&baseline, root.path()).unwrap();
        assert_eq!(removal[0].action, OperationAction::DeleteManaged);
        std::fs::write(
            root.path().join(IMAGE_GUIDANCE_REFERENCE),
            "user modification",
        )
        .unwrap();
        let preservation =
            crate::transaction::managed_removal_operations(&baseline, root.path()).unwrap();
        assert_eq!(preservation[0].action, OperationAction::Skip);
        assert_eq!(preservation[0].local_state, LocalState::Modified);
    }

    #[test]
    fn preserved_generated_repair_does_not_invent_incoming_descriptor_bytes() {
        let root = tempfile::tempdir().unwrap();
        let local = b"name=\"Workshop mod\"\nsupported_version=\"1.19.*\"\n";
        let incoming =
            b"name=\"Workshop mod\"\nsupported_version=\"1.19.*\"\npicture=\"thumbnail.png\"\n";
        std::fs::write(root.path().join("descriptor.mod"), local).unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let mut file = lock.files[0].clone();
        file.path = "descriptor.mod".into();
        file.external = false;
        file.ownership = Ownership::Generated;
        file.source_path = "generated:descriptor.mod".into();
        file.source_sha256 = sha256_bytes(incoming);
        file.installed_sha256 = sha256_bytes(local);
        file.preserved_local = true;
        file.generated_content = None;
        file.generated_bytes = None;
        lock.files = vec![file];

        for mut operations in [
            crate::transaction::reinstall_operations(&lock, root.path()).unwrap(),
            crate::transaction::update_operations(&lock, &lock.files, root.path()).unwrap(),
        ] {
            assert!(prepare_preserved_generated_skip(&lock, &mut operations[0]));
            assert_eq!(operations[0].resolution.as_deref(), Some("keep"));
            assert_eq!(operations[0].source_sha256, Some(sha256_bytes(incoming)));
            assert!(maintenance_conflicts(&operations, &[], &HashMap::new()).is_empty());
        }

        let mut operations = crate::transaction::repair_operations(&lock, root.path()).unwrap();
        assert!(prepare_preserved_generated_skip(&lock, &mut operations[0]));
        let operation = &operations[0];
        assert_eq!(operation.action, OperationAction::Skip);
        assert_eq!(operation.resolution.as_deref(), Some("keep"));
        assert_eq!(operation.source_sha256, Some(sha256_bytes(incoming)));
        assert_eq!(operation.local_sha256, Some(sha256_bytes(local)));
        assert!(maintenance_conflicts(&operations, &[], &HashMap::new()).is_empty());
        assert!(validate_generated_maintenance_content(
            "descriptor.mod",
            local,
            local,
            &sha256_bytes(incoming)
        )
        .is_err());

        std::fs::write(root.path().join("descriptor.mod"), b"user edited again").unwrap();
        for mut operations in [
            crate::transaction::reinstall_operations(&lock, root.path()).unwrap(),
            crate::transaction::update_operations(&lock, &lock.files, root.path()).unwrap(),
        ] {
            assert!(prepare_preserved_generated_skip(&lock, &mut operations[0]));
            let conflicts = maintenance_conflicts(&operations, &[], &HashMap::new());
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].options, ["keep", "skip"]);
            assert_eq!(conflicts[0].selected, None);
        }
        let mut operations = crate::transaction::repair_operations(&lock, root.path()).unwrap();
        assert!(prepare_preserved_generated_skip(&lock, &mut operations[0]));
        let conflicts = maintenance_conflicts(&operations, &[], &HashMap::new());
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].options, ["keep", "skip"]);
        assert_eq!(conflicts[0].selected, None);
        let mut text_operation = operations[0].clone();
        text_operation.destination = "README.md".into();
        let prepared = vec![PreparedFile {
            operation_id: text_operation.id.clone(),
            destination: text_operation.destination.clone(),
            bytes: b"verified incoming".to_vec(),
            expected_sha256: sha256_bytes(b"verified incoming"),
        }];
        let merge_contexts = HashMap::from([(
            text_operation.id.clone(),
            MergeContext {
                kind: FileKind::Text,
                base: b"base".to_vec(),
                local: b"local".to_vec(),
                incoming: b"verified incoming".to_vec(),
            },
        )]);
        let verified = maintenance_conflicts(&[text_operation], &prepared, &merge_contexts);
        assert_eq!(
            verified[0].options,
            ["keep", "replace", "merge", "rename", "skip"]
        );
        assert_eq!(
            std::fs::read(root.path().join("descriptor.mod")).unwrap(),
            b"user edited again"
        );

        lock.files[0].generated_content = Some(String::from_utf8(incoming.to_vec()).unwrap());
        assert!(!prepare_preserved_generated_skip(&lock, &mut operations[0]));
        lock.files[0].generated_content = None;
        lock.files[0].preserved_local = false;
        assert!(!prepare_preserved_generated_skip(&lock, &mut operations[0]));
        lock.files[0].preserved_local = true;
        std::fs::remove_file(root.path().join("descriptor.mod")).unwrap();
        let mut missing = crate::transaction::repair_operations(&lock, root.path()).unwrap();
        assert!(!prepare_preserved_generated_skip(&lock, &mut missing[0]));
    }

    #[test]
    fn generated_client_recomposition_verifies_baseline_and_replaces_old_repair_operation() {
        for platform in [Platform::Windows, Platform::Macos] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join(".codex")).unwrap();
            let destination = ".codex/config.toml";
            let baseline = b"custom = 42\n[mcp_servers.unrelated]\ncommand='safe'\n[mcp_servers.hoi4-live]\ncommand='old'\n";
            std::fs::write(root.path().join(destination), baseline).unwrap();
            let manifest = image_generation_test_manifest();
            let image = ImageGenerationSelection {
                primary_provider: "openai_api".into(),
                ..Default::default()
            };
            let environments = CodingEnvironmentSelection {
                primary: "codex".into(),
                additional: vec![],
            };
            let live = LiveTestingSelection {
                linux_ready: true,
                ssh_host: "hoi4-linux".into(),
                ..Default::default()
            };
            let artifacts = image_client_config_artifacts_for_platform(
                root.path(),
                &manifest,
                &image,
                &environments,
                &[],
                &live,
                platform,
            )
            .unwrap();
            let artifact = artifacts
                .iter()
                .find(|artifact| artifact.destination == destination)
                .unwrap();
            let incoming = artifact.content.as_bytes();
            assert!(artifact.content.contains("unrelated"));
            assert!(artifact.content.contains("custom = 42"));
            assert!(artifact.content.contains("clausewitz_linux"));
            assert!(artifact.content.contains("hoi4_image_openai_api"));
            assert!(!artifact.content.contains("hoi4-live"));
            validate_generated_maintenance_content(
                destination,
                baseline,
                incoming,
                &sha256_bytes(baseline),
            )
            .unwrap();
            assert!(validate_generated_maintenance_content(
                destination,
                b"corrupt",
                incoming,
                &sha256_bytes(baseline)
            )
            .is_err());
            assert!(validate_generated_maintenance_content(
                "README.md",
                baseline,
                incoming,
                &sha256_bytes(baseline)
            )
            .is_err());
            let mut lock: InstallationLock = serde_json::from_str(include_str!(
                "../../docs/examples/installation-lock.example.json"
            ))
            .unwrap();
            let mut file = lock.files[0].clone();
            file.path = destination.into();
            file.external = false;
            file.component_id = artifact.component_id.clone();
            file.source_path = format!("generated:{destination}");
            file.source_sha256 = sha256_bytes(baseline);
            file.source_size = Some(baseline.len() as u64);
            file.installed_sha256 = sha256_bytes(baseline);
            file.installed_size = Some(baseline.len() as u64);
            file.ownership = Ownership::Merged;
            file.generated_content = Some(String::from_utf8(baseline.to_vec()).unwrap());
            lock.files = vec![file];
            let mut operations = crate::transaction::repair_operations(&lock, root.path()).unwrap();
            assert_eq!(operations.len(), 1);
            let mut prepared = vec![PreparedFile {
                operation_id: operations[0].id.clone(),
                destination: destination.into(),
                bytes: baseline.to_vec(),
                expected_sha256: sha256_bytes(baseline),
            }];
            append_maintenance_image_client_config_operations(
                &artifacts,
                &mut operations,
                &mut prepared,
                &lock,
                root.path(),
            )
            .unwrap();
            assert_eq!(
                operations
                    .iter()
                    .filter(|operation| operation.destination == destination)
                    .count(),
                1
            );
            assert_eq!(
                prepared
                    .iter()
                    .filter(|file| file.destination == destination)
                    .count(),
                1
            );
            assert_eq!(operations[0].action, OperationAction::Generate);
            assert_eq!(operations[0].rollback, RollbackAction::RestoreBackup);
            assert_eq!(prepared[0].bytes, incoming);
            lock.files[0].preserved_local = true;
            append_maintenance_image_client_config_operations(
                &artifacts,
                &mut operations,
                &mut prepared,
                &lock,
                root.path(),
            )
            .unwrap();
            assert_eq!(operations.len(), 1);
            assert_eq!(operations[0].action, OperationAction::Skip);
            assert_eq!(operations[0].resolution.as_deref(), Some("review_required"));
        }
    }

    #[test]
    fn maintenance_live_config_changes_are_reviewed_and_reversible() {
        for platform in [Platform::Windows, Platform::Macos] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join(".codex")).unwrap();
            let destination = ".codex/config.toml";
            std::fs::write(root.path().join(destination), "custom = 42\n").unwrap();
            let manifest = image_generation_test_manifest();
            let image = ImageGenerationSelection {
                primary_provider: "openai_api".into(),
                ..Default::default()
            };
            let environments = CodingEnvironmentSelection {
                primary: "codex".into(),
                additional: vec![],
            };
            let mut lock: InstallationLock = serde_json::from_str(include_str!(
                "../../docs/examples/installation-lock.example.json"
            ))
            .unwrap();
            lock.files.clear();
            for live in [
                LiveTestingSelection {
                    linux_ready: true,
                    ssh_host: "hoi4-linux".into(),
                    ..Default::default()
                },
                LiveTestingSelection {
                    provider: "none".into(),
                    ..Default::default()
                },
                LiveTestingSelection {
                    linux_ready: true,
                    ssh_host: "hoi4-linux".into(),
                    ..Default::default()
                },
            ] {
                let artifacts = image_client_config_artifacts_for_platform(
                    root.path(),
                    &manifest,
                    &image,
                    &environments,
                    &[],
                    &live,
                    platform,
                )
                .unwrap();
                let mut operations = vec![];
                let mut prepared = vec![];
                append_maintenance_image_client_config_operations(
                    &artifacts,
                    &mut operations,
                    &mut prepared,
                    &lock,
                    root.path(),
                )
                .unwrap();
                let operation = operations
                    .iter()
                    .find(|op| op.destination == destination)
                    .unwrap();
                assert_eq!(operation.action, OperationAction::Skip);
                assert_eq!(operation.resolution.as_deref(), Some("review_required"));
                assert_eq!(operation.rollback, RollbackAction::RestoreBackup);
                let incoming = prepared
                    .iter()
                    .find(|file| file.destination == destination)
                    .unwrap();
                let text = std::str::from_utf8(&incoming.bytes).unwrap();
                assert!(text.contains("custom = 42"));
                assert_eq!(text.contains("clausewitz_linux"), live.provider != "none");
                std::fs::write(root.path().join(destination), &incoming.bytes).unwrap();
            }
        }
    }

    #[test]
    fn unsupported_platform_live_registration_remains_rejected() {
        let manifest = image_generation_test_manifest();
        let image = ImageGenerationSelection {
            primary_provider: "none".into(),
            ..Default::default()
        };
        let environments = CodingEnvironmentSelection {
            primary: "claude_code".into(),
            additional: vec![],
        };
        let live = LiveTestingSelection {
            linux_ready: true,
            ssh_host: "hoi4-linux".into(),
            ..Default::default()
        };
        let error = adapt_image_generation_client_config_for_platform(
            b"{}",
            ".mcp.json",
            &manifest,
            &image,
            &environments,
            Path::new("C:/mods/example"),
            &live,
            Platform::Unsupported,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("supports Windows, macOS and Linux only"));
    }

    #[test]
    fn live_routes_are_recomposed_only_for_selected_coding_clients() {
        for platform in [Platform::Windows, Platform::Macos] {
            let manifest = image_generation_test_manifest();
            let selection = ImageGenerationSelection {
                primary_provider: "openai_api".into(),
                ..Default::default()
            };
            let live = LiveTestingSelection {
                linux_ready: true,
                ssh_host: "hoi4-linux".into(),
                ..Default::default()
            };
            for (destination, environment) in [
                (".mcp.json", "claude_code"),
                (".cursor/mcp.json", "cursor"),
                (".qoder/mcp.json", "qoder"),
                ("opencode.json", "opencode"),
                (".codex/config.toml", "codex"),
            ] {
                let source = if destination.ends_with(".toml") {
                    b"[mcp_servers.unrelated]\ncommand='safe'\n[mcp_servers.hoi4-live]\ncommand='old'\n"
                    .to_vec()
                } else {
                    let key = if destination == "opencode.json" {
                        "mcp"
                    } else {
                        "mcpServers"
                    };
                    serde_json::to_vec(&serde_json::json!({key:{"unrelated":{"command":"safe"},"hoi4-live":{"command":"old"}},"custom":42})).unwrap()
                };
                for selected in [true, false] {
                    let environments = CodingEnvironmentSelection {
                        primary: if selected {
                            environment
                        } else if environment == "codex" {
                            "claude_code"
                        } else {
                            "codex"
                        }
                        .into(),
                        additional: vec![],
                    };
                    let bytes = adapt_image_generation_client_config_for_platform(
                        &source,
                        destination,
                        &manifest,
                        &selection,
                        &environments,
                        Path::new("C:/mods/example"),
                        &live,
                        platform,
                    )
                    .unwrap();
                    let text = String::from_utf8(bytes).unwrap();
                    assert_eq!(text.contains("clausewitz_linux"), selected, "{destination}");
                    assert!(!text.contains("hoi4-live"));
                    assert!(text.contains("unrelated"));
                }
            }
        }
    }

    #[test]
    fn image_provider_adaptation_replaces_markers_and_removes_old_client_routes() {
        let mut manifest = image_generation_test_manifest();
        let providers = manifest
            .image_generation
            .as_mut()
            .and_then(Value::as_object_mut)
            .and_then(|catalog| catalog.get_mut("providers"))
            .and_then(Value::as_array_mut)
            .unwrap();
        let mut old_provider = providers[0].clone();
        old_provider["id"] = Value::String("old_provider".into());
        old_provider["route"]["server_id"] = Value::String("hoi4_image_old_provider".into());
        providers.push(old_provider);
        let selection = ImageGenerationSelection {
            primary_provider: "openai_api".into(),
            additional_providers: vec![],
            route_settings: BTreeMap::new(),
            credential_references: BTreeMap::new(),
            installed_package_tree_sha256: None,
            installed_package_lock_sha256: None,
        };
        let guided = adapt_image_generation_guidance(b"Before\n<!-- HOI4_MOD_SETUP:IMAGE_GENERATION_PROVIDER:START -->\nplaceholder\n<!-- HOI4_MOD_SETUP:IMAGE_GENERATION_PROVIDER:END -->\nAfter", &manifest, &selection).unwrap();
        let guided = String::from_utf8(guided).unwrap();
        assert!(guided.contains("Tools: generate: `generate_image`"));
        assert!(guided.contains("output_path: not supported by this tool"));
        assert!(guided.contains("Maximum image area: 8294400 pixels."));
        assert!(guided.contains("one image per call"));
        assert!(guided.contains("at least one pixel with alpha below 255"));
        assert!(!guided.contains("placeholder"));

        let environments = CodingEnvironmentSelection {
            primary: "claude_code".into(),
            additional: vec![],
        };
        let config = br#"{"mcpServers":{"hoi4_image_old_provider":{"command":"old"},"hoi4_image_custom":{"command":"keep"},"hoi4_agent_tools":{"command":"hoi4"}}}"#;
        let adapted = adapt_image_generation_client_config(
            config,
            ".mcp.json",
            &manifest,
            &selection,
            &environments,
            Path::new("C:/mods/example"),
            &LiveTestingSelection {
                provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let adapted: Value = serde_json::from_slice(&adapted).unwrap();
        assert!(adapted["mcpServers"].get("hoi4_image_openai_api").is_some());
        assert!(adapted["mcpServers"]
            .get("hoi4_image_old_provider")
            .is_none());
        assert!(adapted["mcpServers"].get("hoi4_image_custom").is_some());
        assert_eq!(
            adapted["mcpServers"].get("hoi4_agent_tools").is_some(),
            Platform::current() != Platform::Linux,
            "this fixture declares no native Linux HOI4 Agent Tools route"
        );
        assert_eq!(adapted["mcpServers"]["hoi4_image_openai_api"]["cwd"], ".");

        let codex = b"[mcp_servers.hoi4_image_old_provider]\ncommand = \"old\"\n";
        let adapted = adapt_image_generation_client_config(
            codex,
            ".codex/config.toml",
            &manifest,
            &selection,
            &CodingEnvironmentSelection {
                primary: "codex".into(),
                additional: vec![],
            },
            Path::new("C:/mods/example"),
            &LiveTestingSelection {
                provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let adapted: toml::Value = std::str::from_utf8(&adapted).unwrap().parse().unwrap();
        assert!(adapted["mcp_servers"]
            .get("hoi4_image_openai_api")
            .is_some());
        assert!(adapted["mcp_servers"]
            .get("hoi4_image_old_provider")
            .is_none());
        assert_eq!(
            adapted["mcp_servers"]["hoi4_image_openai_api"]["cwd"].as_str(),
            Some(".")
        );

        for (environment, destination) in
            [("cursor", ".cursor/mcp.json"), ("qoder", ".qoder/mcp.json")]
        {
            let existing = br#"{"mcpServers":{"other":{"command":"keep"},"hoi4_image_old_provider":{"command":"old"}}}"#;
            let adapted = adapt_image_generation_client_config(
                existing,
                destination,
                &manifest,
                &selection,
                &CodingEnvironmentSelection {
                    primary: environment.into(),
                    additional: vec![],
                },
                Path::new("C:/mods/example"),
                &LiveTestingSelection {
                    provider: "none".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let adapted: Value = serde_json::from_slice(&adapted).unwrap();
            assert_eq!(adapted["mcpServers"]["other"]["command"], "keep");
            assert!(adapted["mcpServers"]
                .get("hoi4_image_old_provider")
                .is_none());
            assert_eq!(adapted["mcpServers"]["hoi4_image_openai_api"]["cwd"], ".");
        }

        let opencode = br#"{"theme":"dark","mcp":{"other":{"type":"local","command":["existing"]},"hoi4_image_old_provider":{"type":"local","command":["old"]},"hoi4_image_custom":{"type":"local","command":["keep"]}}}"#;
        let adapted = adapt_image_generation_client_config(
            opencode,
            "opencode.json",
            &manifest,
            &selection,
            &CodingEnvironmentSelection {
                primary: "opencode".into(),
                additional: vec![],
            },
            Path::new("C:/mods/example"),
            &LiveTestingSelection {
                provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let adapted: Value = serde_json::from_slice(&adapted).unwrap();
        assert_eq!(adapted["theme"], "dark");
        assert!(adapted["mcp"].get("hoi4_image_openai_api").is_some());
        assert!(adapted["mcp"]["hoi4_image_openai_api"]["command"].is_array());
        assert_eq!(adapted["mcp"]["hoi4_image_openai_api"]["type"], "local");
        assert_eq!(adapted["mcp"]["hoi4_image_openai_api"]["enabled"], true);

        let none = LiveTestingSelection {
            provider: "none".into(),
            ..Default::default()
        };
        let adapt_for = |environment: &str, destination: &str, bytes: &[u8]| {
            adapt_image_generation_client_config(
                bytes,
                destination,
                &manifest,
                &selection,
                &CodingEnvironmentSelection {
                    primary: environment.into(),
                    additional: vec![],
                },
                Path::new("C:/mods/example"),
                &none,
            )
            .unwrap()
        };
        let gemini: Value = serde_json::from_slice(&adapt_for(
            "gemini_cli",
            ".gemini/settings.json",
            br#"{"context":{"fileName":["GEMINI.md"]},"mcpServers":{"hoi4_image_old_provider":{"command":"old"}}}"#,
        ))
        .unwrap();
        assert_eq!(gemini["context"]["fileName"][0], "GEMINI.md");
        assert_eq!(gemini["mcpServers"]["hoi4_image_openai_api"]["cwd"], ".");
        assert!(gemini["mcpServers"]
            .get("hoi4_image_old_provider")
            .is_none());
        let vscode: Value = serde_json::from_slice(&adapt_for(
            "github_copilot",
            ".vscode/mcp.json",
            b"{\n  // user comment\n  \"servers\": {\"user\": {\"type\": \"stdio\", \"command\": \"keep\"}},\n}",
        ))
        .unwrap();
        assert_eq!(vscode["servers"]["user"]["command"], "keep");
        let route = &vscode["servers"]["hoi4_image_openai_api"];
        assert_eq!(route["type"], "stdio");
        assert_eq!(route["cwd"], "${workspaceFolder}");
        let grok: toml::Value = std::str::from_utf8(&adapt_for(
            "grok_build",
            ".grok/config.toml",
            b"[mcp_servers.hoi4_image_old_provider]\ncommand = \"old\"\n",
        ))
        .unwrap()
        .parse()
        .unwrap();
        let route = &grok["mcp_servers"]["hoi4_image_openai_api"];
        assert_eq!(route["cwd"].as_str(), Some("."));
        assert!(route.get("enabled").is_none(), "enabled is a Codex key");
        assert!(grok["mcp_servers"].get("hoi4_image_old_provider").is_none());
        // Kiro documents no working directory, so its image routes are never
        // adapted. (Linux may still re-render the file for the shared route.)
        let kiro = br#"{"mcpServers":{"hoi4_image_old_provider":{"command":"old"}}}"#;
        let kiro_adapted: Value =
            serde_json::from_slice(&adapt_for("kiro", ".kiro/settings/mcp.json", kiro)).unwrap();
        assert_eq!(kiro_adapted, serde_json::from_slice::<Value>(kiro).unwrap());
        assert_eq!(adapted["mcp"]["hoi4_image_openai_api"]["cwd"], ".");
        assert_eq!(adapted["mcp"]["other"]["command"][0], "existing");
        assert!(adapted["mcp"].get("hoi4_image_old_provider").is_none());
        assert!(adapted["mcp"].get("hoi4_image_custom").is_some());

        let no_provider = ImageGenerationSelection {
            primary_provider: "none".into(),
            additional_providers: vec![],
            route_settings: BTreeMap::new(),
            credential_references: BTreeMap::new(),
            installed_package_tree_sha256: None,
            installed_package_lock_sha256: None,
        };
        let adapted = adapt_image_generation_client_config(
            br#"{"mcpServers":{"user_tool":{"command":"keep"},"hoi4_image_openai_api":{"command":"old"},"hoi4_image_old_provider":{"command":"older"}}}"#,
            ".mcp.json",
            &manifest,
            &no_provider,
            &environments,
            Path::new("C:/mods/example"),
        &LiveTestingSelection { provider: "none".into(), ..Default::default() })
        .unwrap();
        let adapted: Value = serde_json::from_slice(&adapted).unwrap();
        assert_eq!(adapted["mcpServers"]["user_tool"]["command"], "keep");
        assert!(adapted["mcpServers"].get("hoi4_image_openai_api").is_none());
        assert!(adapted["mcpServers"]
            .get("hoi4_image_old_provider")
            .is_none());
    }

    #[test]
    fn selected_image_clients_get_reviewable_config_artifacts_on_windows_and_macos() {
        let mut manifest = image_generation_test_manifest();
        let client_components = [
            ("claude_code", ".mcp.json", "windows"),
            ("codex", ".codex/config.toml", "all"),
            ("cursor", ".cursor/mcp.json", "windows"),
            ("qoder", ".qoder/mcp.json", "windows"),
            ("opencode", "opencode.json", "all"),
        ];
        manifest.components = client_components
            .iter()
            .map(|(environment, path, platform)| {
                serde_json::from_value(serde_json::json!({
                    "id": format!("runtime.{environment}.config"),
                    "display_name": format!("{environment} config"),
                    "category": "environment",
                    "coding_environment": environment,
                    "optional": true,
                    "platforms": [platform],
                    "source": { "kind": "file", "path": path },
                    "destination": { "path": path, "ownership": "merged" },
                    "dependencies": [],
                    "conflict_policy": "replace_if_unmodified",
                    "validation": [],
                    "update": { "strategy": "replace_if_unmodified", "remove_obsolete": false }
                }))
                .unwrap()
            })
            .collect();
        let environments = CodingEnvironmentSelection {
            primary: "claude_code".into(),
            additional: vec![
                "codex".into(),
                "cursor".into(),
                "qoder".into(),
                "opencode".into(),
            ],
        };
        let selection = ImageGenerationSelection {
            primary_provider: "openai_api".into(),
            additional_providers: vec![],
            route_settings: BTreeMap::new(),
            credential_references: BTreeMap::new(),
            installed_package_tree_sha256: None,
            installed_package_lock_sha256: None,
        };
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join(".mcp.json"),
            br#"{"mcpServers":{"user_tool":{"command":"keep"}}}"#,
        )
        .unwrap();
        let windows_paths =
            image_client_config_source_destinations(&manifest, &environments, Platform::Windows);
        let windows_artifacts = image_client_config_artifacts(
            root.path(),
            &manifest,
            &selection,
            &environments,
            &windows_paths,
            &LiveTestingSelection {
                provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(windows_artifacts.is_empty());

        let macos_paths =
            image_client_config_source_destinations(&manifest, &environments, Platform::Macos);
        assert_eq!(macos_paths, vec![".codex/config.toml", "opencode.json"]);
        let macos_artifacts = image_client_config_artifacts(
            root.path(),
            &manifest,
            &selection,
            &environments,
            &macos_paths,
            &LiveTestingSelection {
                provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            macos_artifacts
                .iter()
                .map(|artifact| artifact.destination.as_str())
                .collect::<Vec<_>>(),
            vec![".mcp.json", ".cursor/mcp.json", ".qoder/mcp.json"]
        );
        for artifact in macos_artifacts {
            assert!(artifact.content.contains("hoi4_image_openai_api"));
            if artifact.destination == ".mcp.json" {
                let config: Value = serde_json::from_str(&artifact.content).unwrap();
                assert_eq!(config["mcpServers"]["user_tool"]["command"], "keep");
            }
        }

        let operation_root = tempdir().unwrap();
        let operation_artifacts = image_client_config_artifacts(
            operation_root.path(),
            &manifest,
            &selection,
            &environments,
            &macos_paths,
            &LiveTestingSelection {
                provider: "none".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let mut operations = Vec::new();
        let mut prepared = Vec::new();
        append_maintenance_image_client_config_operations(
            &operation_artifacts,
            &mut operations,
            &mut prepared,
            &lock,
            operation_root.path(),
        )
        .unwrap();
        assert_eq!(operations.len(), 3);
        assert_eq!(prepared.len(), 3);
        assert!(operations.iter().all(|operation| {
            operation.action == OperationAction::Generate && !operation.external
        }));
        assert!(operations
            .iter()
            .all(|operation| operation.ownership == Some(Ownership::Generated)));

        let existing_config_operations = {
            let mut operations = Vec::new();
            let mut prepared = Vec::new();
            let existing_artifacts = image_client_config_artifacts(
                root.path(),
                &manifest,
                &selection,
                &environments,
                &macos_paths,
                &LiveTestingSelection {
                    provider: "none".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            append_maintenance_image_client_config_operations(
                &existing_artifacts,
                &mut operations,
                &mut prepared,
                &lock,
                root.path(),
            )
            .unwrap();
            operations
        };
        assert_eq!(
            existing_config_operations
                .iter()
                .find(|operation| operation.destination == ".mcp.json")
                .unwrap()
                .ownership,
            Some(Ownership::Merged)
        );

        assert_eq!(
            image_client_config_ownership(false, None),
            Ownership::Generated
        );
        assert_eq!(image_client_config_ownership(true, None), Ownership::Merged);
        assert_eq!(
            image_client_config_ownership(true, Some(Ownership::Generated)),
            Ownership::Generated
        );
        assert_eq!(
            image_client_config_ownership(true, Some(Ownership::Merged)),
            Ownership::Merged
        );
    }

    #[test]
    fn app_removal_preserves_existing_client_config_and_deletes_app_created_config() {
        let root = tempdir().unwrap();
        let preexisting_path = ".mcp.json";
        let preexisting_bytes = br#"{"mcpServers":{"user_tool":{"command":"keep"},"hoi4_image_openai_api":{"command":"image"}}}"#;
        std::fs::write(root.path().join(preexisting_path), preexisting_bytes).unwrap();
        let generated_path = ".cursor/mcp.json";
        let generated_bytes = br#"{"mcpServers":{"hoi4_image_openai_api":{"command":"image"}}}"#;
        std::fs::create_dir_all(root.path().join(".cursor")).unwrap();
        std::fs::write(root.path().join(generated_path), generated_bytes).unwrap();

        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.files = vec![
            LockedFile {
                path: preexisting_path.into(),
                location_scope: Some("project".into()),
                component_id: "image_generation.client_config.claude_code".into(),
                source_path: format!("generated:{preexisting_path}"),
                source_revision: lock.source.revision.clone(),
                source_sha256: sha256_bytes(preexisting_bytes),
                source_size: Some(preexisting_bytes.len() as u64),
                base_sha256: None,
                installed_sha256: sha256_bytes(preexisting_bytes),
                installed_size: Some(preexisting_bytes.len() as u64),
                ownership: image_client_config_ownership(true, None),
                preserved_local: false,
                external: false,
                generated_content: Some(String::from_utf8(preexisting_bytes.to_vec()).unwrap()),
                generated_bytes: None,
                executable: false,
                platform: Some(ManifestPlatform::All),
                installer_created: false,
            },
            LockedFile {
                path: generated_path.into(),
                location_scope: Some("project".into()),
                component_id: "image_generation.client_config.cursor".into(),
                source_path: format!("generated:{generated_path}"),
                source_revision: lock.source.revision.clone(),
                source_sha256: sha256_bytes(generated_bytes),
                source_size: Some(generated_bytes.len() as u64),
                base_sha256: None,
                installed_sha256: sha256_bytes(generated_bytes),
                installed_size: Some(generated_bytes.len() as u64),
                ownership: image_client_config_ownership(false, None),
                preserved_local: false,
                external: false,
                generated_content: Some(String::from_utf8(generated_bytes.to_vec()).unwrap()),
                generated_bytes: None,
                executable: false,
                platform: Some(ManifestPlatform::All),
                installer_created: false,
            },
        ];
        let operations =
            crate::transaction::managed_removal_operations(&lock, root.path()).unwrap();
        let preexisting = operations
            .iter()
            .find(|operation| operation.destination == preexisting_path)
            .unwrap();
        assert_eq!(preexisting.action, OperationAction::Skip);
        assert_eq!(
            preexisting.resolution.as_deref(),
            Some("reverse_merge_required")
        );
        let generated = operations
            .iter()
            .find(|operation| operation.destination == generated_path)
            .unwrap();
        assert_eq!(generated.action, OperationAction::DeleteManaged);
        assert!(std::fs::read_to_string(root.path().join(preexisting_path))
            .unwrap()
            .contains("user_tool"));
    }

    #[test]
    fn three_d_failures_return_a_redacted_non_blocking_result() {
        let result = three_d_failure("Bearer private-health-token timed out".into());
        assert_eq!(result.status, "incomplete");
        assert!(result.timed_out);
        assert!(result.stderr.contains("REDACTED"));
        assert!(!result.stderr.contains("private-health-token"));
    }

    #[test]
    fn mcp_bootstrap_failure_keeps_the_cause_without_secrets_or_unbounded_output() {
        let output = |stderr: String| crate::process::ProcessResult {
            status_code: Some(1),
            stdout: String::new(),
            stderr,
            timed_out: false,
            stdout_truncated: false,
            stderr_truncated: false,
        };
        let message = mcp_bootstrap_failure_details(Ok(output(
            "The installed MCP package tree does not match the reviewed release. client_secret=private-test-value".into(),
        )));
        assert!(message.contains("package tree does not match"));
        assert!(message.contains("exit=1"));
        assert!(!message.contains("private-test-value"));
        assert!(message.contains("REDACTED"));
        let message = mcp_bootstrap_failure_details(Ok(output(
            "ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¾ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¾ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â§ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¾ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¾ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¾Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¾ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢"
                .repeat(3000),
        )));
        assert!(message.len() <= 2048);
        assert!(message.ends_with("..."));
        for padding in 1880..2048 {
            let message = mcp_bootstrap_failure_details(Ok(output(format!(
                "{} client_secret=private-test-value {}",
                "x".repeat(padding),
                "y".repeat(100)
            ))));
            assert!(message.len() <= 2048);
            assert!(!crate::security::contains_credential_shaped_content(
                &message
            ));
        }
        let message = mcp_bootstrap_failure_details(Err(AppError::Process(
            "publisher check failed; Bearer private-test-token".into(),
        )));
        assert!(message.contains("publisher check failed"));
        assert!(!message.contains("private-test-token"));
    }

    #[test]
    fn removal_never_plans_a_provider_bootstrap() {
        let selection = ImageGenerationSelection {
            primary_provider: "gptimage".into(),
            ..Default::default()
        };
        assert!(provider_bootstrap_selected(
            "repair",
            Some(&selection),
            "gptimage"
        ));
        assert!(!provider_bootstrap_selected(
            "remove",
            Some(&selection),
            "gptimage"
        ));
        assert!(!provider_bootstrap_selected("update", None, "gptimage"));
    }

    #[test]
    fn managed_removal_never_restarts_the_three_d_bootstrap() {
        let project = tempdir().unwrap();
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        plan.maintenance_mode = Some("remove".into());
        plan.optional_workflows
            .insert("workflow.3d".into(), "ready".into());
        plan.external_actions.clear();

        let error =
            run_reviewed_post_install_actions(project.path(), &plan, "workflow.3d").unwrap_err();
        assert!(error.to_string().contains("managed removal"));
    }

    #[test]
    fn managed_removal_may_only_run_the_reviewed_image_cleanup() {
        let project = tempdir().unwrap();
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        plan.maintenance_mode = Some("remove".into());
        plan.external_actions = vec![reviewed_gptimage_cleanup_action()];
        for component in [
            crate::mcp::COMPONENT_ID,
            LIVE_TOOLS_COMPONENT_ID,
            "workflow.3d",
        ] {
            let error =
                run_reviewed_post_install_actions(project.path(), &plan, component).unwrap_err();
            assert!(error.to_string().contains("managed removal"), "{component}");
        }
        // The cleanup itself is allowed to start; with no installed tree it
        // fails later for its own reason, never for being a removal.
        let outcome = run_reviewed_post_install_actions(
            project.path(),
            &plan,
            IMAGEGEN_GPTIMAGE_COMPONENT_ID,
        );
        if let Err(error) = outcome {
            assert!(!error.to_string().contains("managed removal cannot run"));
        }
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
                Err::<(), _>(AppError::Protocol("malformed JSONL".into()))
            },
            || {
                clears.set(clears.get() + 1);
                Ok(())
            },
        );

        assert!(matches!(malformed, Err(AppError::Protocol(_))));
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
    fn rejected_codex_proposal_keeps_the_initialized_session_and_approved_scan() {
        #[derive(Debug)]
        struct FakeSession;

        let starts = Cell::new(0_u8);
        let clears = Cell::new(0_u8);
        let mut session = Some(FakeSession);
        let result = with_supervised_codex_session(
            &mut session,
            || {
                starts.set(starts.get() + 1);
                Ok(FakeSession)
            },
            |_| true,
            |_| Err::<(), _>(AppError::Serialization("proposal rejected".into())),
            || {
                clears.set(clears.get() + 1);
                Ok(())
            },
        );

        assert!(matches!(result, Err(AppError::Serialization(_))));
        assert!(session.is_some());
        assert_eq!(starts.get(), 0);
        assert_eq!(clears.get(), 0);
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
        assert!(source.contains("run_blocking_command(\"descriptor-preview\""));
        assert!(
            source.contains("require_ai_session(&state).map_err(provider_analysis_user_error)?")
        );
    }

    #[test]
    fn blocking_command_work_uses_the_async_runtime_blocking_pool() {
        let source = include_str!("commands.rs");
        let start = source
            .find("async fn run_blocking_command")
            .expect("blocking command helper is present");
        let helper = &source[start..];
        let end = helper.find("\n}").expect("blocking helper body is closed");
        let helper = &helper[..end];
        assert!(helper.contains("tauri::async_runtime::spawn_blocking(work)"));
        assert!(!helper.contains(".join()"));
    }

    #[test]
    fn every_plan_builder_binds_external_destination_parents() {
        // Plan construction needs a resolved source, so this guards the
        // wiring; the binding itself is tested in the transaction module.
        for (source, builder) in [
            (include_str!("service/builder.rs"), "fn build_plan_with("),
            (
                include_str!("commands.rs"),
                "fn build_maintenance_plan_blocking(",
            ),
        ] {
            let start = source.find(builder).expect("plan builder is present");
            let body = &source[start..];
            let body = &body[..body.find("\n}\n").expect("plan builder body is closed")];
            assert!(
                body.contains("crate::transaction::bind_plan_external_parents(&mut plan)"),
                "{builder} must bind external destination parents before the plan is reviewed"
            );
        }
    }

    #[test]
    fn project_identity_reads_nested_descriptor_fields() {
        let root = tempdir().unwrap();
        let root_path = PathBuf::from(crate::paths::user_facing_path(
            &root.path().canonicalize().unwrap(),
        ));
        let project_root = root_path.join("project");
        let launcher = root_path.join("cold_war_curtain.mod");
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
                "descriptorTags": ["Gameplay", "Graphics"],
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
            vec!["Gameplay".to_string(), "Graphics".to_string()]
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
    fn portrait_provider_routes_reject_unverified_revisions_and_non_loopback_urls() {
        let error = portrait_pipeline_from_state(&serde_json::json!({
            "portraitPipeline": {
                "enabled": true,
                "provider": "local",
                "workflowCommit": "a".repeat(40),
                "localServerUrl": "http://127.0.0.1:8188"
            }
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("current verified upstream revision"));

        let error = portrait_pipeline_from_state(&serde_json::json!({
            "portraitPipeline": {
                "enabled": true,
                "provider": "local",
                "localServerUrl": "http://127.0.0.1.evil:8188"
            }
        }))
        .unwrap_err();
        assert!(error.to_string().contains("loopback"));

        let error = portrait_pipeline_from_state(&serde_json::json!({
            "portraitPipeline": {
                "enabled": true,
                "provider": "runpod",
                "runpodUrl": "https://pod.example/?token=secret"
            }
        }))
        .unwrap_err();
        assert!(error.to_string().contains("query data"));
    }

    #[test]
    fn explicitly_empty_model_is_not_replaced_with_a_default_model() {
        assert_eq!(ai_model_from_state(&serde_json::json!({"aiModel": ""})), "");
        assert_eq!(ai_model_from_state(&serde_json::json!({})), "default");
    }

    #[test]
    fn generated_readme_does_not_assign_the_setup_assistant_as_the_development_client() {
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
            "high",
        )
        .unwrap();

        assert!(readme.contains("provider-neutral agentic development"));
        assert!(readme.contains("does not select or restrict the development client"));
        assert!(!readme.contains("Planning provider"));
        assert!(!readme.contains("claude-sonnet"));
        assert!(!readme.contains("Claude setup analysis"));
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
        let selected = adapt_agents_for_selection(
            template,
            &identity,
            "codex",
            "default",
            "xhigh",
            &ProjectConventions::default(),
            true,
            false,
        )
        .unwrap();
        let unselected = adapt_agents_for_selection(
            template,
            &identity,
            "codex",
            "default",
            "xhigh",
            &ProjectConventions::default(),
            false,
            false,
        )
        .unwrap();
        let selected = String::from_utf8(selected).unwrap();
        assert!(selected.contains("Optional Super Events workflow"));
        assert!(!selected.contains("Selected AI planning profile"));
        assert!(!selected.contains("Reasoning effort"));
        assert!(!String::from_utf8(unselected)
            .unwrap()
            .contains("Optional Super Events workflow"));
    }

    #[test]
    fn canonical_claude_instructions_are_adapted_like_agents_instructions() {
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
        let template = b"# Project Instructions\n\nThis describes `[MOD_NAME]`.\n\n## Placeholder Guide\n\nReplace `[MOD_PREFIX]` before use.\n\n## 0. Required Reading\n\nGame files live in `[HOI4_GAME_DIR]` and Workshop mods in `[STEAM_WORKSHOP_DIR]`.\n\n<!-- HOI4_MOD_SETUP:COMPONENT:workflow.live_tools:START -->\nLive testing guidance.\n<!-- HOI4_MOD_SETUP:COMPONENT:workflow.live_tools:END -->\n<!-- HOI4_MOD_SETUP:COMPONENT:workflow.super_events:START -->\nSuper Events guidance.\n<!-- HOI4_MOD_SETUP:COMPONENT:workflow.super_events:END -->\nUse `[MOD_PREFIX]` identifiers.\n";
        let adapt = |selected: &[String]| {
            String::from_utf8(
                adapt_selected_source(
                    "core.claude.instructions",
                    "CLAUDE.md",
                    template,
                    &identity,
                    &ProjectConventions::default(),
                    "deepseek",
                    "deepseek-flash",
                    "high",
                    selected.iter().any(|id| id == "workflow.super_events"),
                    false,
                    false,
                    &test_portrait_config("disabled", false),
                    selected,
                )
                .unwrap(),
            )
            .unwrap()
        };
        let plain = adapt(&[]);
        assert!(plain.starts_with("# Project Instructions\n\nThis describes `Example Mod`."));
        assert!(plain.contains("Use `example` identifiers."));
        assert!(!plain.contains("Placeholder Guide"));
        assert!(!plain.contains("[MOD_"));
        assert!(!plain.contains("[HOI4_GAME_DIR]") && !plain.contains("[STEAM_WORKSHOP_DIR]"));
        assert!(plain.contains("steamapps/common/Hearts of Iron IV"));
        assert!(!plain.contains("Live testing guidance"));
        assert!(!plain.contains("Super Events guidance"));
        assert!(!plain.contains("HOI4_MOD_SETUP"));
        assert!(!plain.contains("Optional Super Events workflow"));
        assert!(!plain.contains("DeepSeek") && !plain.contains("deepseek-flash"));

        let selected = adapt(&["workflow.live_tools".into(), "workflow.super_events".into()]);
        assert!(selected.contains("Live testing guidance.\nSuper Events guidance.\nUse `example`"));
        assert!(!selected.contains("HOI4_MOD_SETUP"));
        assert!(!selected.contains("Optional Super Events workflow"));
    }

    #[test]
    fn unbalanced_component_blocks_are_rejected() {
        let error = adapt_component_blocks(
            b"<!-- HOI4_MOD_SETUP:COMPONENT:workflow.live_tools:START -->\ntext\n".to_vec(),
            &[],
        )
        .unwrap_err();
        assert!(error.to_string().contains("unterminated"));
        let error = adapt_component_blocks(
            b"<!-- HOI4_MOD_SETUP:COMPONENT:a:START -->\n<!-- HOI4_MOD_SETUP:COMPONENT:b:END -->\n"
                .to_vec(),
            &[],
        )
        .unwrap_err();
        assert!(error.to_string().contains("unbalanced"));
        let untouched = b"no markers here".to_vec();
        assert_eq!(
            adapt_component_blocks(untouched.clone(), &[]).unwrap(),
            untouched
        );
    }

    #[test]
    fn confirmed_project_conventions_are_rendered_provider_neutrally() {
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
        let conventions = ProjectConventions {
            agents_profile: "events-first".into(),
            localisation_convention: "English keys use snake_case".into(),
            documentation_convention: "Markdown under docs/".into(),
        };
        let adapted = adapt_agents_for_selection(
            b"# [MOD_NAME]\n\nUse `[MOD_PREFIX]` for identifiers.\n",
            &identity,
            "deepseek",
            "deepseek-flash",
            "high",
            &conventions,
            false,
            false,
        )
        .unwrap();
        let adapted = String::from_utf8(adapted).unwrap();
        assert!(adapted.contains("## Confirmed project conventions"));
        assert!(adapted.contains("`events-first`"));
        assert!(adapted.contains("`English keys use snake_case`"));
        assert!(adapted.contains("`Markdown under docs/`"));
        assert!(!adapted.contains("DeepSeek"));
        assert!(!adapted.contains("deepseek-flash"));
        assert!(!adapted.contains("reasoning effort"));
    }

    #[test]
    fn every_setup_creates_the_private_work_directory_without_duplicates() {
        let new_state = serde_json::json!({
            "folderProfile": ["common", ".tmp"]
        });
        assert_eq!(
            transaction_directories(&new_state, "new").unwrap(),
            vec![".tmp".to_string(), "common".to_string()]
        );
        assert_eq!(
            transaction_directories(&serde_json::json!({}), "existing").unwrap(),
            vec![".tmp".to_string()]
        );
    }

    #[test]
    fn ordinary_skill_keeps_super_events_guidance_only_when_selected() {
        let source = b"# Ordinary skill\n\nAlways present.\n\n<!-- HOI4_MOD_SETUP_SUPER_EVENTS_START -->\n## Super Events integration\n\nSelected only.\n<!-- HOI4_MOD_SETUP_SUPER_EVENTS_END -->\n";
        let selected = adapt_optional_super_events_skill(source, true).unwrap();
        let selected = String::from_utf8(selected).unwrap();
        assert!(selected.contains("Selected only."));
        assert!(!selected.contains("HOI4_MOD_SETUP_SUPER_EVENTS"));

        let unselected = adapt_optional_super_events_skill(source, false).unwrap();
        let unselected = String::from_utf8(unselected).unwrap();
        assert!(unselected.contains("Always present."));
        assert!(!unselected.contains("Super Events integration"));
        assert!(!unselected.contains("Selected only."));
    }

    pub(super) fn test_portrait_config(provider: &str, enabled: bool) -> PortraitPipelineConfig {
        PortraitPipelineConfig {
            enabled,
            provider: provider.into(),
            provider_status: if enabled {
                "needs_authorization".into()
            } else {
                "not_selected".into()
            },
            workflow_repository: crate::portraits::PORTRAIT_REPOSITORY.into(),
            workflow_branch: crate::portraits::PORTRAIT_BRANCH.into(),
            workflow_commit: crate::portraits::PORTRAIT_COMMIT.into(),
            preferred_workflow: "source".into(),
            local_comfyui_root: String::new(),
            local_server_url: "http://127.0.0.1:8188".into(),
            runpod_url: String::new(),
            runpod_workspace: "/workspace/comfyui-hoi4-portraits".into(),
            mcp_registered: enabled && provider == "cloud",
        }
    }

    #[test]
    fn disabled_portrait_markers_and_cloud_mcp_are_removed() {
        let source = b"# Shared guidance\n<!-- HOI4_MOD_SETUP_PORTRAITS_START -->\nComfyUI provider guidance\n<!-- HOI4_MOD_SETUP_PORTRAITS_END -->\n";
        let disabled =
            String::from_utf8(adapt_optional_portrait_section(source, false).unwrap()).unwrap();
        assert!(disabled.contains("Shared guidance"));
        assert!(!disabled.to_ascii_lowercase().contains("comfyui"));
        assert!(!disabled.contains("HOI4_MOD_SETUP_PORTRAITS"));

        let config = br#"[mcp_servers.comfy_cloud_portraits]
url = "https://cloud.comfy.org/mcp"

[portrait_pipeline]
enabled = true
provider = "cloud"
mcp_server = "comfy_cloud_portraits"
"#;
        let local = String::from_utf8(
            adapt_codex_config_for_selection(
                config,
                false,
                false,
                false,
                &test_portrait_config("local", true),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!local.contains("comfy_cloud_portraits"));
        assert!(!local.contains("cloud.comfy.org"));
    }

    #[test]
    fn unselected_super_events_agents_are_removed_from_codex_config() {
        let config = br#"[agents]
max_threads = 8

[agents.hoi4_repo_explorer]
config_file = "agents/hoi4_repo_explorer.toml"

[agents.hoi4_super_event_art_researcher]
config_file = "agents/hoi4_super_event_art_researcher.toml"
"#;
        let core = String::from_utf8(
            adapt_codex_config_for_selection(
                config,
                false,
                false,
                false,
                &test_portrait_config("disabled", false),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(core.contains("hoi4_repo_explorer"));
        assert!(!core.contains("hoi4_super_event_art_researcher"));

        let selected = String::from_utf8(
            adapt_codex_config_for_selection(
                config,
                false,
                false,
                true,
                &test_portrait_config("disabled", false),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(selected.contains("hoi4_super_event_art_researcher"));
    }

    #[test]
    fn binary_sources_without_portrait_markers_are_kept_unchanged() {
        let identity = super_events_test_identity();
        let portrait = test_portrait_config("disabled", false);
        let source = [0_u8, 0xff, 0xfe, 0_u8, 0x80];

        let adapted = adapt_selected_source(
            "workflow.portraits.core",
            ".agents/skills/hoi4-portrait-production/SKILL.md",
            &source,
            &identity,
            &ProjectConventions::default(),
            "codex",
            "default",
            "xhigh",
            false,
            false,
            false,
            &portrait,
            &[],
        )
        .unwrap();

        assert_eq!(adapted, source);
    }

    #[test]
    fn default_profile_additions_are_adopted_without_hardcoded_component_ids() {
        let mut manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();
        let mut future = manifest
            .components
            .iter()
            .find(|component| component.id == "core.skills")
            .unwrap()
            .clone();
        future.id = "core.future_skills".into();
        future.display_name = "Future skills".into();
        manifest.components.push(future);
        manifest
            .profiles
            .iter_mut()
            .find(|profile| profile.default)
            .unwrap()
            .components
            .push("core.future_skills".into());
        let mut requested = vec!["core.agents".into()];

        let defaults = default_profile_component_ids(&manifest).unwrap();
        add_supported_profile_components(&manifest, &mut requested, &defaults, Platform::Windows)
            .unwrap();

        assert!(requested.iter().any(|id| id == "core.future_skills"));
    }

    #[test]
    fn fresh_planning_records_unknown_optional_components_as_not_selected() {
        let mut manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();
        let mut future = manifest
            .components
            .iter()
            .find(|component| component.id == "core.skills")
            .unwrap()
            .clone();
        future.id = "workflow.future_tools".into();
        future.display_name = "Future tools".into();
        future.optional = true;
        manifest.components.push(future);
        let mut states = BTreeMap::new();

        seed_manifest_optional_states(&mut states, &manifest);

        assert_eq!(
            states.get("workflow.future_tools").map(String::as_str),
            Some("not_selected")
        );
    }

    #[test]
    fn a_declined_optional_component_is_not_silently_adopted_when_it_becomes_default() {
        let mut previous_manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();
        let mut future = previous_manifest
            .components
            .iter()
            .find(|component| component.id == "core.skills")
            .unwrap()
            .clone();
        future.id = "workflow.future_tools".into();
        future.display_name = "Future tools".into();
        future.optional = true;
        previous_manifest.components.push(future);
        let mut current_manifest = previous_manifest.clone();
        current_manifest
            .profiles
            .iter_mut()
            .find(|profile| profile.default)
            .unwrap()
            .components
            .push("workflow.future_tools".into());
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.optional_workflows.insert(
            "workflow.future_tools".into(),
            OptionalWorkflowLock {
                state: "not_selected".into(),
                reason: None,
                credential_reference: None,
            },
        );

        let additions =
            newly_defaulted_component_ids(&previous_manifest, &current_manifest, &lock).unwrap();

        assert!(!additions.iter().any(|id| id == "workflow.future_tools"));
    }

    #[test]
    fn arbitrary_manifest_subagent_destination_gets_safe_spawn_adaptation() {
        let identity = super_events_test_identity();
        let source = br#"developer_instructions = "Review one bounded surface."
"#;
        let adapted = adapt_selected_source(
            "future.helper_profile",
            ".codex/agents/future_helper.toml",
            source,
            &identity,
            &ProjectConventions::default(),
            "codex",
            "default",
            "xhigh",
            false,
            false,
            false,
            &test_portrait_config("disabled", false),
            &[],
        )
        .unwrap();

        assert!(String::from_utf8(adapted)
            .unwrap()
            .contains("fork_context=false"));
    }

    #[test]
    fn codex_mcp_registration_keeps_the_source_idle_exit_environment() {
        let config = b"[mcp_servers.hoi4_agent_tools]\ncommand = \"hoi4-agent-tools.cmd\"\nenv = { HOI4_AGENT_STDIO_IDLE_EXIT_MINUTES = \"30\" }\n";
        let adapted = adapt_codex_config_for_selection(
            config,
            true,
            false,
            false,
            &test_portrait_config("disabled", false),
        )
        .unwrap();
        let value: toml::Value = String::from_utf8(adapted).unwrap().parse().unwrap();
        assert_eq!(
            value["mcp_servers"]["hoi4_agent_tools"]["env"]["HOI4_AGENT_STDIO_IDLE_EXIT_MINUTES"]
                .as_str(),
            Some("30"),
            "Codex keeps a stdio server per finished subagent unless it exits when idle"
        );
    }

    #[test]
    fn native_mcp_retargeting_preserves_custom_routes_and_source_environment_for_every_client() {
        let launch = ("/usr/bin/node".to_string(), vec!["/home/example/.local/share/hoi4-mod-setup/npm/lib/node_modules/hoi4-agent-tools/dist/bin/stdio.js".to_string()]);
        let json_routes = coding_environment::NATIVE_CLIENTS
            .iter()
            .filter_map(|client| client.mcp)
            .filter(|mcp| !mcp.format.is_toml())
            .collect::<Vec<_>>();
        assert!(json_routes.len() >= 10);
        for mcp in json_routes {
            let path = mcp.path;
            let key = mcp.format.servers_key();
            let bytes = serde_json::to_vec(&serde_json::json!({(key):{"hoi4_agent_tools":{"command":"hoi4-agent-tools.cmd","args":["old"],"env":{"HOI4_AGENT_STDIO_IDLE_EXIT_MINUTES":"30"}},"custom":{"command":"keep-me"}}})).unwrap();
            let value: Value = serde_json::from_slice(
                &retarget_client_config(&bytes, path, Some(&launch)).unwrap(),
            )
            .unwrap();
            assert_eq!(value[key]["custom"]["command"], "keep-me");
            assert_eq!(
                value[key]["hoi4_agent_tools"]["env"]["HOI4_AGENT_STDIO_IDLE_EXIT_MINUTES"],
                "30"
            );
            if mcp.format == crate::native_mcp::McpFormat::LocalCommandArray {
                assert_eq!(
                    value[key]["hoi4_agent_tools"]["command"],
                    serde_json::json!([launch.0, launch.1[0]])
                );
            } else {
                assert_eq!(value[key]["hoi4_agent_tools"]["command"], launch.0);
                assert_eq!(
                    value[key]["hoi4_agent_tools"]["args"],
                    serde_json::json!(launch.1)
                );
            }
        }
        let source = b"[mcp_servers.hoi4_agent_tools]\ncommand = 'hoi4-agent-tools.cmd'\nenv = { HOI4_AGENT_STDIO_IDLE_EXIT_MINUTES = '30' }\n[mcp_servers.custom]\ncommand = 'keep-me'\n";
        let value: toml::Value = String::from_utf8(
            retarget_client_config(source, ".codex/config.toml", Some(&launch)).unwrap(),
        )
        .unwrap()
        .parse()
        .unwrap();
        assert_eq!(
            value["mcp_servers"]["hoi4_agent_tools"]["command"].as_str(),
            Some(launch.0.as_str())
        );
        assert_eq!(
            value["mcp_servers"]["custom"]["command"].as_str(),
            Some("keep-me")
        );
        let removed: toml::Value =
            String::from_utf8(retarget_client_config(source, ".codex/config.toml", None).unwrap())
                .unwrap()
                .parse()
                .unwrap();
        assert!(removed["mcp_servers"].get("hoi4_agent_tools").is_none());
        assert_eq!(
            removed["mcp_servers"]["custom"]["command"].as_str(),
            Some("keep-me")
        );
        // Grok Build shares the TOML route shape; an unregistered or legacy
        // path is never rewritten.
        let grok: toml::Value = String::from_utf8(
            retarget_client_config(source, ".grok/config.toml", Some(&launch)).unwrap(),
        )
        .unwrap()
        .parse()
        .unwrap();
        assert_eq!(
            grok["mcp_servers"]["hoi4_agent_tools"]["command"].as_str(),
            Some(launch.0.as_str())
        );
        let legacy = br#"{"mcpServers":{"hoi4_agent_tools":{"command":["hoi4-agent-tools.cmd"]}}}"#;
        assert_eq!(
            retarget_client_config(legacy, ".opencode/mcp.json", Some(&launch)).unwrap(),
            legacy.to_vec()
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_client_registrations_bind_the_reviewed_user_prefix_before_bootstrap() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let target =
            crate::mcp::manifest_target_for_platform(&manifest, Platform::Windows).unwrap();
        // Planning must not depend on the bootstrap's wrapper or Node already
        // existing. It captures the exact source-owned current-user route.
        let launch = mcp_client_launch(&target, Platform::Windows).unwrap();
        assert!(Path::new(&launch.0).is_absolute());
        assert_eq!(
            launch.0,
            crate::paths::user_facing_path(
                &crate::paths::windows_npm_prefix()
                    .unwrap()
                    .join(&target.target)
            )
        );
        assert!(launch.1.is_empty());
        for destination in [
            ".mcp.json",
            ".cursor/mcp.json",
            ".qoder/mcp.json",
            "opencode.json",
            ".codex/config.toml",
        ] {
            let source = if destination == ".codex/config.toml" {
                b"[mcp_servers.hoi4_agent_tools]\ncommand='hoi4-agent-tools.cmd'\nargs=[]\n"
                    .to_vec()
            } else if destination == "opencode.json" {
                serde_json::to_vec(&serde_json::json!({"mcp":{"hoi4_agent_tools":{"type":"local","command":["hoi4-agent-tools.cmd"]}}})).unwrap()
            } else {
                serde_json::to_vec(&serde_json::json!({"mcpServers":{"hoi4_agent_tools":{"command":"hoi4-agent-tools.cmd","args":[]}}})).unwrap()
            };
            let adapted = retarget_client_config(&source, destination, Some(&launch)).unwrap();
            assert_eq!(
                crate::mcp::client_registration(&adapted, destination).unwrap(),
                Some(launch.clone()),
                "{destination}"
            );
        }
    }

    #[test]
    fn selected_3d_workflow_materializes_the_reviewed_mcp_routes() {
        let config = b"[mcp_servers.hoi4_agent_tools]\ncommand = \"hoi4-agent-tools.cmd\"\n";
        let adapted = String::from_utf8(
            adapt_codex_config_for_selection(
                config,
                true,
                true,
                false,
                &test_portrait_config("disabled", false),
            )
            .unwrap(),
        )
        .unwrap();

        for route in ["meshy", "blender_hoi4", "blender_lab"] {
            assert!(adapted.contains(&format!("[mcp_servers.{route}]")));
        }
        assert!(adapted.contains("run_blender_hoi4_adapter.cmd"));
        assert!(adapted.contains("MESHY_API_KEY"));
        assert!(adapted.contains("--run-verified-meshy-mcp"));
        assert!(!adapted.contains("run_meshy_mcp.cmd"));
        assert!(!adapted.contains("run_verified_meshy.py"));
    }

    #[test]
    fn published_3d_bootstrap_action_preserves_reviewed_external_boundaries() {
        let manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();

        let actions = manifest_external_actions(&manifest, &["workflow.3d".into()]);
        let action = actions
            .iter()
            .find(|action| action.component_id == "workflow.3d")
            .expect("3D bootstrap action must be plan-visible");

        assert_eq!(
            action.arguments,
            vec![
                ".tools/3d_pipeline/bootstrap_3d_workflow.py",
                "--project-root",
                "<project_root>",
                "--verify-reviewed-config",
                "--quiet",
            ]
        );
        assert_ne!(action.network_access, "not_declared");
        assert!(!action.expected_writes.is_empty());
        assert_eq!(action.privilege, "current_user");
        assert_ne!(action.rollback_boundary, "not_declared_by_source");
    }

    #[test]
    fn published_live_tools_action_preserves_reviewed_external_boundaries() {
        let manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();

        let actions = manifest_external_actions(&manifest, &[LIVE_TOOLS_COMPONENT_ID.into()]);
        let action = actions
            .iter()
            .find(|action| action.component_id == LIVE_TOOLS_COMPONENT_ID)
            .expect("the HOI4 Live Tools bootstrap action must be plan-visible");

        assert_eq!(
            action.arguments,
            vec![LIVE_TOOLS_SCRIPT, "--quiet", "--register", "none"]
        );
        assert_ne!(action.network_access, "not_declared");
        assert!(!action.expected_writes.is_empty());
        assert_eq!(action.privilege, "current_user");
        assert_ne!(action.rollback_boundary, "not_declared_by_source");
    }

    #[cfg(windows)]
    #[test]
    fn live_tools_bootstrap_needs_exactly_one_reviewed_action() {
        let project = tempdir().unwrap();
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        plan.maintenance_mode = None;
        plan.external_actions.clear();

        let error =
            run_reviewed_post_install_actions(project.path(), &plan, LIVE_TOOLS_COMPONENT_ID)
                .unwrap_err();
        assert!(error.to_string().contains("exactly one HOI4 Live Tools"));
    }

    #[test]
    fn managed_removal_never_restarts_the_live_tools_bootstrap() {
        let project = tempdir().unwrap();
        let mut plan: InstallationPlan = serde_json::from_str(include_str!(
            "../../docs/examples/installation-plan.example.json"
        ))
        .unwrap();
        plan.maintenance_mode = Some("remove".into());
        plan.external_actions.clear();

        let error =
            run_reviewed_post_install_actions(project.path(), &plan, LIVE_TOOLS_COMPONENT_ID)
                .unwrap_err();
        assert!(error.to_string().contains("managed removal"));
    }

    #[test]
    fn published_mcp_actions_bind_the_automatic_bootstrap_and_all_technology_routes() {
        let manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();
        let actions = manifest_external_actions(
            &manifest,
            &[
                "mcp.hoi4_agent_tools.bootstrap".into(),
                crate::mcp::COMPONENT_ID.into(),
            ],
        );
        let bootstrap = actions
            .iter()
            .find(|action| action.component_id == "mcp.hoi4_agent_tools.bootstrap")
            .expect("the automatic package bootstrap must be reviewable");
        assert_eq!(
            bootstrap.arguments,
            vec![".tools/mcp/bootstrap_hoi4_agent_tools.py", "--quiet"]
        );
        assert_ne!(bootstrap.network_access, "not_declared");
        assert!(!bootstrap.expected_writes.is_empty());
        assert_eq!(bootstrap.privilege, "current_user");

        let target = crate::mcp::reviewed_plan_target(&actions).unwrap();
        assert_eq!(target.package_version, "4.0.3");
        assert_eq!(target.required_tools.len(), 28);
        for route in [
            "hoi4.tech_inspect",
            "hoi4.tech_render",
            "hoi4.probability_analyze",
            "hoi4.job_inspect",
            "hoi4.job_cancel",
        ] {
            assert!(target.required_tools.iter().any(|tool| tool == route));
        }
    }

    #[test]
    fn updated_manifest_mcp_identity_reaches_review_without_app_version_constants() {
        let mut manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();
        let rule = manifest
            .components
            .iter_mut()
            .find(|component| component.id == crate::mcp::COMPONENT_ID)
            .unwrap()
            .validation
            .iter_mut()
            .find(|rule| rule.id == crate::mcp::HEALTH_RULE_ID)
            .unwrap();
        rule.parameters
            .insert("package_version".into(), serde_json::json!("99.1.0"));
        rule.parameters.insert(
            "package_integrity".into(),
            serde_json::json!(format!("sha512-{}", "A".repeat(86) + "==")),
        );
        rule.parameters.insert(
            "package_tree_sha256".into(),
            serde_json::json!("d".repeat(64)),
        );
        if let Some(pins) = rule
            .parameters
            .get_mut("platform_pins")
            .and_then(Value::as_object_mut)
        {
            for pin in pins.values_mut() {
                pin["package_tree_sha256"] = serde_json::json!("d".repeat(64));
            }
        }
        let actions = manifest_external_actions(&manifest, &[crate::mcp::COMPONENT_ID.into()]);
        let target = crate::mcp::reviewed_plan_target(&actions).unwrap();
        assert_eq!(target.package_version, "99.1.0");
        assert_eq!(target.package_tree_sha256, "d".repeat(64));
        assert_eq!(
            target.package_integrity,
            format!("sha512-{}", "A".repeat(86) + "==")
        );
        let mut missing = actions;
        missing[0].verified_package_tree_sha256 = None;
        assert!(crate::mcp::reviewed_plan_target(&missing).is_err());
    }

    #[test]
    fn enabled_portrait_selection_adds_the_complete_component_closure() {
        let enabled = selected_ids(
            &serde_json::json!({
                "selectedComponents": ["core.skills"],
                "portraitPipeline": {
                    "enabled": true,
                    "provider": "cloud"
                }
            }),
            "codex",
        );
        for component in [
            "workflow.portraits.core",
            "workflow.portraits.cloud",
            "workflow.portraits.subagent",
            "workflow.portraits.config",
        ] {
            assert!(enabled.iter().any(|id| id == component));
        }
        assert!(!enabled.iter().any(|id| id == "workflow.portraits.local"));
        assert!(!enabled.iter().any(|id| id == "workflow.portraits.runpod"));

        for provider in ["local", "runpod"] {
            let selected = selected_ids(
                &serde_json::json!({
                    "selectedComponents": [],
                    "portraitPipeline": {"enabled": true, "provider": provider}
                }),
                "codex",
            );
            assert!(selected
                .iter()
                .any(|id| id == &format!("workflow.portraits.{provider}")));
            for other in ["cloud", "local", "runpod"] {
                if other != provider {
                    assert!(!selected
                        .iter()
                        .any(|id| id == &format!("workflow.portraits.{other}")));
                }
            }
        }

        let disabled = selected_ids(
            &serde_json::json!({
                "selectedComponents": ["workflow.portraits.core", "workflow.portraits.config"],
                "portraitPipeline": {
                    "enabled": false,
                    "provider": "disabled"
                }
            }),
            "codex",
        );
        assert!(disabled
            .iter()
            .all(|id| !id.starts_with("workflow.portraits")));
    }

    #[test]
    fn incomplete_optional_skill_marker_fails_closed() {
        let error = adapt_optional_super_events_skill(
            b"<!-- HOI4_MOD_SETUP_SUPER_EVENTS_START -->\nmissing end",
            true,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("incomplete optional Super Events section"));
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

        let adapted = adapt_agents_for_selection(
            template,
            &identity,
            "codex",
            "default",
            "xhigh",
            &ProjectConventions::default(),
            false,
            false,
        )
        .unwrap();
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
            script_prefix: Some("example_script".into()),
            primary_namespace: Some("example_namespace".into()),
            descriptor_tags: Vec::new(),
            launcher_descriptor_path: None,
        };
        let adapted = adapt_super_events_source(
            b"name = [MOD_PREFIX]_super_events\ntext = \"[MOD_NAME]\"\n",
            &identity,
        )
        .unwrap();
        let text = String::from_utf8(adapted).unwrap();
        assert!(text.contains("name = example_script_super_events"));
        assert!(text.contains("text = \"Example \\\"Mod\\\"\""));
        assert!(!text.contains("[MOD_"));
        assert_eq!(
            adapt_super_events_destination(
                "workflow.super_events.runtime.interface",
                "interface/hoi4ms_super_events.gui",
                &identity,
            )
            .unwrap(),
            "interface/example_script_super_events.gui"
        );
        assert_eq!(
            adapt_super_events_destination(
                "workflow.super_events.runtime.events",
                "events/hoi4ms_super_event_examples.txt",
                &identity,
            )
            .unwrap(),
            "events/example_script_super_event_examples.txt"
        );
        assert_eq!(
            adapt_super_events_destination(
                "workflow.super_events.runtime.gfx",
                "gfx/super_events/super_event_bg.dds",
                &identity,
            )
            .unwrap(),
            "gfx/super_events/super_event_bg.dds"
        );
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
    fn setup_provider_does_not_filter_development_components() {
        let selected = selected_ids(
            &serde_json::json!({
                "selectedComponents": ["core.agents", "codex.config", "mcp.hoi4_agent_tools"]
            }),
            "deepseek",
        );
        assert!(selected.iter().any(|id| id == "codex.config"));
        assert!(selected.iter().any(|id| id == "mcp.hoi4_agent_tools"));
    }

    #[test]
    fn shared_mcp_route_is_requested_for_every_coding_environment() {
        let manifest = crate::source::parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();
        for primary in crate::coding_environment::SUPPORTED {
            let state = serde_json::json!({
                "primaryCodingEnvironment": primary,
                "additionalCodingEnvironments": [],
                "selectedComponents": []
            });
            let (requested, selection) =
                requested_components_for_selection(&manifest, &state, "deepseek").unwrap();
            assert_eq!(selection.primary, primary);
            assert!(
                requested.iter().any(|id| id == "mcp.hoi4_agent_tools"),
                "shared MCP package missing for {primary}"
            );
            assert!(
                requested
                    .iter()
                    .any(|id| id == "mcp.hoi4_agent_tools.bootstrap"),
                "shared MCP bootstrap missing for {primary}"
            );
            // Linux is a compile/test host, not an app platform. Exercise both
            // supported platform policies explicitly instead of assuming the
            // host can select a desktop default profile.
            for platform in [Platform::Windows, Platform::Macos] {
                let mut supported_request = requested.clone();
                add_supported_profile_components(
                    &manifest,
                    &mut supported_request,
                    &default_profile_component_ids(&manifest).unwrap(),
                    platform,
                )
                .unwrap();
                assert!(
                    supported_request.iter().any(|id| id == "docs.runtimes"),
                    "new source-declared core default missing for {primary} on {platform:?}"
                );
            }
        }
        let state = serde_json::json!({
            "primaryCodingEnvironment": "cursor",
            "additionalCodingEnvironments": ["codex", "claude_code"],
            "selectedComponents": []
        });
        let (requested, _) =
            requested_components_for_selection(&manifest, &state, "codex").unwrap();
        for id in [
            "codex.config",
            "runtime.cursor",
            "runtime.claude",
            "runtime.claude.mcp",
        ] {
            assert!(
                requested.iter().any(|candidate| candidate == id),
                "missing {id}"
            );
        }
        assert!(requested.iter().any(|id| id == "mcp.hoi4_agent_tools"));
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
    fn codex_opener_launches_the_desktop_app_for_the_project() {
        let root = tempdir().unwrap();
        assert_eq!(
            codex_app_args(root.path()),
            vec!["app".to_string(), root.path().display().to_string()]
        );
    }

    #[test]
    fn login_url_opener_rejects_untrusted_urls_before_process_resolution() {
        let error = open_codex_login_url("javascript:alert(1)".into()).unwrap_err();
        assert!(error.contains("invalid HTTPS authentication URL"));
    }

    #[test]
    fn published_provider_documentation_and_live_guides_are_exactly_allowlisted() {
        let manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        for catalog in [
            manifest.image_generation.as_ref(),
            manifest.live_testing.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            for row in catalog["providers"].as_array().unwrap() {
                if let Some(url) = row["documentation_url"].as_str() {
                    assert!(is_allowed_external_url(url), "{url}");
                    assert!(!is_allowed_external_url(&format!("{url}?unreviewed=1")));
                    assert!(!is_allowed_external_url(&format!("{url}#unreviewed")));
                }
            }
        }
        for url in ["https://github.com/smerss/Clausewitz-MCP", "https://github.com/klimPaskov/hoi4-mod-setup/blob/main/docs/LIVE_TESTING_LINUX.md", "https://github.com/klimPaskov/Agentic-HOI4-Modding/blob/main/.claude/skills/hoi4-live-playtest/references/setup.md", "https://github.com/klimPaskov/Agentic-HOI4-Modding/blob/main/.claude/skills/hoi4-live-playtest/references/hoi4-live-tools.md"] {
            assert!(is_allowed_external_url(url));
        }
        for url in [
            "javascript:alert(1)",
            "https://github.com.evil.example/smerss/Clausewitz-MCP",
            "https://github.com/other/Clausewitz-MCP",
            "https://user:password@github.com/smerss/Clausewitz-MCP",
            "https://github.com/smerss/Clausewitz-MCP/issues",
            "http://github.com/smerss/Clausewitz-MCP",
        ] {
            assert!(!is_allowed_external_url(url));
        }
    }

    #[test]
    fn external_link_opener_accepts_only_product_reviewed_urls() {
        assert!(is_allowed_external_url(
            "https://github.com/klimPaskov/Agentic-HOI4-Modding"
        ));
        assert!(is_allowed_external_url(
            "https://github.com/klimPaskov/HOI4-Mod-Setup"
        ));
        assert!(is_allowed_external_url("https://chatgpt.com"));
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
    fn private_provider_output_is_discarded_without_blame_on_the_users_credentials() {
        for mapper in [
            provider_analysis_user_error,
            codex_user_error,
            claude_user_error,
        ] {
            let message = mapper(AppError::UnsafeProviderOutput(
                "private-output-sentinel".into(),
            ));
            assert!(message.contains("discarded"));
            assert!(!message.contains("private-output-sentinel"));
            assert!(!message.contains("Reconnect"));
        }
        for error in [
            AppError::Process("provider usage is limited; retry when available".into()),
            AppError::Credential("Gemini usage is currently limited".into()),
        ] {
            let message = provider_analysis_user_error(error);
            assert!(message.contains("usage is currently limited"));
            assert!(!message.contains("Reconnect"));
        }
    }

    #[test]
    fn logout_between_adapter_completion_and_storage_cannot_restore_a_pending_analysis() {
        let _guard = test_state_guard();
        clear_codex_local_state().unwrap();
        let analysis: CodexAnalysis = serde_json::from_str(include_str!(
            "../../docs/examples/codex-analysis.example.json"
        ))
        .unwrap();
        let lock: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        let mut record: CodexAnalysisRecord =
            serde_json::from_value(lock["codex_analysis"].clone()).unwrap();
        record.analysis_id = analysis.analysis_id;
        let completed = CodexAnalysisResult { analysis, record };
        let request = AiAnalysisRequest {
            provider: "claude_account".into(),
            model: crate::claude_code::DEFAULT_MODEL.into(),
            reasoning_effort: "medium".into(),
            endpoint: String::new(),
            analysis: CodexAnalysisRequest {
                mode: "new_project_identity".into(),
                brief: "A mod".into(),
                evidence: Vec::new(),
                constraints: serde_json::json!({}),
                analysis_purpose: None,
                project_root: None,
                scan_id: None,
            },
        };
        let before_logout = provider_session_generation(&request.provider);
        // The adapter has already completed; sign-out occurs before the store takes its lock.
        crate::claude_code::invalidate_session();
        clear_codex_local_state().unwrap();
        assert!(retain_provider_analysis(&completed, &request, before_logout).is_err());
        assert!(codex_analyses().lock().unwrap().is_empty());
        retain_provider_analysis(
            &completed,
            &request,
            provider_session_generation(&request.provider),
        )
        .unwrap();
        crate::claude_code::invalidate_session();
        clear_codex_local_state().unwrap();
        assert!(codex_analyses().lock().unwrap().is_empty());
    }

    #[test]
    fn analysis_preparation_maps_typed_io_failures_at_the_command_boundary() {
        let mut request = CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "A test mod".into(),
            evidence: vec![],
            constraints: serde_json::json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        };
        let sentinel = "private-source-sentinel C:\\private\\cache (os error 2)";
        let error = prepare_analysis_command_input(&mut request, |_| {
            Err(AppError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                sentinel,
            )))
        })
        .unwrap_err();
        assert!(!error.contains(sentinel));
        assert!(!error.contains("os error"));
        assert!(!error.contains("private"));

        let source = include_str!("commands.rs");
        for command in ["fn codex_analyze_blocking(", "fn ai_analyze_blocking("] {
            let body = &source[source.find(command).unwrap()..];
            let body = &body[..body.find("\n}\n").unwrap()];
            assert!(body.contains("prepare_analysis_command_input("));
            assert!(!body.contains("map_err(command_error)"));
        }
    }

    #[test]
    fn maintenance_command_hides_filesystem_details_on_real_lock_read_failure() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("private-source-sentinel");
        let error = tauri::async_runtime::block_on(build_maintenance_plan(
            "repair".into(),
            root.display().to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ))
        .unwrap_err();
        assert!(error.contains("could not be prepared safely"));
        assert!(!error.contains("private-source-sentinel"));
        assert!(!error.contains("os error"));
        assert!(!root.exists());
        let injected = tauri::async_runtime::block_on(run_maintenance_command::<(), _>(|| {
            Err(AppError::Io(std::io::Error::other("private-io-sentinel")))
        }))
        .unwrap_err();
        assert!(!injected.contains("private-io-sentinel"));
    }

    #[test]
    fn async_maintenance_boundary_preserves_authentication_usage_and_review_categories() {
        let signed_out = tauri::async_runtime::block_on(run_maintenance_command(|| {
            validate_codex_planning_account(&CodexAccountStatus::default())
        }))
        .unwrap_err();
        assert!(signed_out.starts_with("Sign in with ChatGPT"));

        let limited = tauri::async_runtime::block_on(run_maintenance_command(|| {
            validate_codex_planning_account(&CodexAccountStatus {
                available: true,
                authenticated: true,
                auth_mode: "chatgpt".into(),
                usage_limited: true,
                ..Default::default()
            })
        }))
        .unwrap_err();
        assert!(limited.contains("usage is currently limited"));
        assert!(!limited.contains("Sign in"));

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let stale_review = tauri::async_runtime::block_on(run_maintenance_command(move || {
            require_maintenance_reanalysis("update", None, &root, None, None)
        }))
        .unwrap_err();
        assert!(stale_review.starts_with("The confirmed review no longer matches"));
        assert!(!stale_review.contains("Reconnect"));

        let source = include_str!("commands.rs");
        let start = source.find("async fn build_maintenance_plan(").unwrap();
        let command = &source[start..];
        let command = &command[..command.find("\n}\n").unwrap()];
        assert!(command.contains("run_maintenance_command(move ||"));
        let start = source.find("fn build_maintenance_plan_blocking(").unwrap();
        let body = &source[start..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("Result<InstallationPlan, AppError>"));
        for mapper in [
            "command_error",
            "planning_command_error",
            "provider_analysis_user_error",
        ] {
            assert!(!body.contains(&format!("map_err({mapper})")));
        }
    }

    #[test]
    fn async_maintenance_boundary_hides_actual_file_errors_and_invalid_environment_values() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let error = tauri::async_runtime::block_on(run_maintenance_command(move || {
            crate::flatten::read_bounded_regular_file_no_follow_under_root(
                &root,
                "private-file-sentinel.md",
                1024,
            )
        }))
        .unwrap_err();
        assert!(!error.contains("private-file-sentinel"));
        assert!(!error.contains("os error"));
        assert!(!error.contains("flattened file"));

        for primary_invalid in [true, false] {
            let error = tauri::async_runtime::block_on(run_maintenance_command(move || {
                let invalid = "private-value-sentinel".repeat(1000);
                coding_environment::selection_from_value(&if primary_invalid {
                    serde_json::json!({"primaryCodingEnvironment": invalid})
                } else {
                    serde_json::json!({"additionalCodingEnvironments": [invalid]})
                })
            }))
            .unwrap_err();
            assert!(error.starts_with("Select"));
            assert!(!error.contains("private-value-sentinel"));
            assert!(error.split(" Technical details:").next().unwrap().len() < 100);
        }

        let source = include_str!("flatten.rs");
        let start = source
            .find("pub(crate) fn read_bounded_with_check<F>(")
            .unwrap();
        let body = &source[start..];
        let body = &body[..body.find("\n    }\n").unwrap()];
        assert!(body.contains("open_regular_file_under_root(self, &normalized)?"));
        assert!(body.contains("file.read(&mut chunk[..chunk_len])?"));
        assert!(!body.contains("map_err("));
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
        let codex_schema_error = codex_user_error(AppError::Serialization(
            "Codex proposal must use a string value".into(),
        ));
        assert!(codex_schema_error.contains("required proposal format"));
        assert!(!codex_schema_error.contains("serialization"));
        assert!(!codex_schema_error.contains("must use a string value"));

        let provider_schema_error = provider_analysis_user_error(AppError::Serialization(
            "Codex proposal must use a string value".into(),
        ));
        assert!(provider_schema_error.contains("required proposal format"));
        assert!(!provider_schema_error.contains("serialization"));
        assert!(!provider_schema_error.contains("must use a string value"));

        let sentinel = "provider-private-sentinel C:\\private\\provider.log";
        let provider_errors = [
            AppError::InvalidInput(sentinel.into()),
            AppError::PathSecurity(sentinel.into()),
            AppError::Source(sentinel.into()),
            AppError::Scan(sentinel.into()),
            AppError::Merge(sentinel.into()),
            AppError::Transaction(sentinel.into()),
            AppError::Io(std::io::Error::other(sentinel)),
            AppError::Credential(sentinel.into()),
            AppError::Process(sentinel.into()),
            AppError::Protocol(sentinel.into()),
            AppError::Serialization(sentinel.into()),
            AppError::UnsupportedPlatform(sentinel.into()),
        ];
        for error in provider_errors {
            let message = provider_analysis_user_error(error);
            assert!(!message.contains(sentinel));
            assert!(!message.contains("provider-private-sentinel"));
            assert!(!message.contains("private\\provider.log"));
            assert!(message.len() <= 160);
        }

        for error in [
            AppError::Credential(sentinel.into()),
            AppError::Protocol(sentinel.into()),
            AppError::Serialization(sentinel.into()),
        ] {
            let message = planning_user_message(error);
            assert!(!message.contains(sentinel));
            assert!(!message.contains("provider-private-sentinel"));
            assert!(message.len() <= 160);
        }

        for error in [
            AppError::Source(format!("write verified cache: {sentinel}")),
            AppError::Transaction(format!("directory durability flush failed: {sentinel}")),
            AppError::Io(std::io::Error::other(sentinel)),
            AppError::PathSecurity(sentinel.into()),
            AppError::Process(sentinel.into()),
            AppError::Merge(sentinel.into()),
        ] {
            let message = planning_user_message(error);
            assert!(!message.contains("provider-private-sentinel"));
            assert!(!message.contains("private"));
            assert!(message.len() <= 160);
        }
        let selected = planning_command_error(AppError::InvalidInput(
            "select at least one manifest component".into(),
        ));
        assert!(selected.starts_with("Select at least one manifest component"));
        if cfg!(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux"
        )) {
            assert!(selected.contains("Technical details:"));
            assert!(selected.contains("diagnostics.log"));
        } else {
            assert!(!selected.contains("Technical details:"));
        }
        assert!(!selected.contains("provider-private-sentinel"));

        let source = include_str!("commands.rs");
        let handler = source
            .split("async fn ai_model_list(")
            .nth(1)
            .unwrap()
            .split("#[tauri::command")
            .next()
            .unwrap();
        let handler = handler.split_whitespace().collect::<String>();
        assert!(handler.contains("with_codex_session(AppServerProtocol::model_list).map_err(provider_analysis_user_error)"));
        assert!(
            source.contains("validate_confirmed_record(&record).map_err(planning_command_error)?")
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
            project_root_identity: None,
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
    fn approved_scan_identity_rejects_a_replacement_at_the_same_path() {
        let _state_guard = test_state_guard();
        let project = tempfile::tempdir().unwrap();
        let root = project.path().join("mod");
        std::fs::create_dir(&root).unwrap();
        let scan_id = Uuid::new_v4();
        let identity = RootedDir::open_read(&root)
            .unwrap()
            .identity_token()
            .unwrap();
        let approved = ApprovedScanEvidence {
            project_root: Some(root.clone()),
            project_root_identity: Some(identity),
            scan_id: Some(scan_id),
            entries: HashMap::new(),
            evidence_sha256: None,
        };

        std::fs::rename(&root, project.path().join("mod-reviewed")).unwrap();
        std::fs::create_dir(&root).unwrap();

        assert!(validate_approved_project_root_identity(&root, &approved, Some(scan_id)).is_err());
    }

    #[test]
    fn renderer_cannot_authorize_an_arbitrary_external_launcher_file() {
        let parent = tempfile::tempdir().unwrap();
        let parent_path = PathBuf::from(crate::paths::user_facing_path(
            &parent.path().canonicalize().unwrap(),
        ));
        let project = parent_path.join("example");
        std::fs::create_dir(&project).unwrap();
        let declared = crate::paths::user_facing_path(&project.canonicalize().unwrap());
        let declared = if cfg!(target_os = "windows") {
            declared.replace('\\', "/")
        } else {
            declared
        };
        let candidate = parent_path.join("example.mod");
        std::fs::write(
            &candidate,
            format!("name=\"Example\"\npath=\"{declared}\"\n"),
        )
        .unwrap();
        let arbitrary = parent_path.join("arbitrary.mod");
        std::fs::write(&arbitrary, "name=\"Private\"\n").unwrap();

        assert!(
            approve_launcher_descriptor_for_scan(&project, Some(&arbitrary.to_string_lossy()),)
                .is_err()
        );
        assert_eq!(
            approve_launcher_descriptor_for_scan(&project, Some(&candidate.to_string_lossy()),)
                .unwrap()
                .unwrap(),
            candidate.canonicalize().unwrap()
        );
        assert!(approve_launcher_descriptor_for_scan(&project, None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn user_added_documents_are_bound_by_the_context_store_not_the_scan_approval() {
        let _state_guard = test_state_guard();
        let _context_guard = crate::context::test_guard();
        let folder = tempfile::tempdir().unwrap();
        let notes = folder.path().join("pitch.md");
        std::fs::write(
            &notes,
            format!("# Pitch {}\nA divided Europe.\n", Uuid::new_v4()),
        )
        .unwrap();
        let (documents, refused) = crate::context::add_documents(&[notes]);
        assert!(refused.is_empty(), "{refused:?}");
        let document = documents[0].clone();
        let context_evidence = ApprovedEvidence {
            reference: document.reference.clone(),
            path: document.evidence_path.clone(),
            excerpt: document.excerpt.clone(),
            excerpt_sha256: document.excerpt_sha256.clone(),
            confidence: None,
        };
        let request =
            |evidence: Vec<ApprovedEvidence>, purpose: Option<&str>| CodexAnalysisRequest {
                mode: "new_project_identity".into(),
                brief: "A divided Europe.".into(),
                evidence,
                constraints: serde_json::json!({}),
                analysis_purpose: purpose.map(str::to_owned),
                project_root: None,
                scan_id: None,
            };
        // A new mod has no scan: a bound document alone is accepted.
        validate_codex_evidence_approval(&request(vec![context_evidence.clone()], None)).unwrap();
        let mut tampered = context_evidence.clone();
        tampered.excerpt_sha256 = sha256_bytes(b"other text");
        assert!(validate_codex_evidence_approval(&request(vec![tampered], None)).is_err());
        let mut renamed = context_evidence.clone();
        renamed.path = "@context/99-other.md".into();
        assert!(validate_codex_evidence_approval(&request(vec![renamed], None)).is_err());
        // Maintenance still needs approved scan evidence.
        assert!(validate_codex_evidence_approval(&request(
            vec![context_evidence.clone()],
            Some("maintenance_reanalysis")
        ))
        .is_err());
        crate::context::forget(&document.id).unwrap();
        assert!(validate_codex_evidence_approval(&request(vec![context_evidence], None)).is_err());
    }

    #[test]
    fn renderer_cannot_authorize_an_arbitrary_scan_excerpt() {
        let _state_guard = test_state_guard();
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        let scan_id = Uuid::new_v4();
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(root.clone()),
            project_root_identity: Some(
                RootedDir::open_read(&root)
                    .unwrap()
                    .identity_token()
                    .unwrap(),
            ),
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
    fn local_only_portrait_routes_are_never_approved_as_semantic_evidence() {
        let _state_guard = test_state_guard();
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        let scan_id = Uuid::new_v4();
        let excerpt = r#"{"local_root":"C:/ComfyUI"}"#;
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(root.clone()),
            project_root_identity: Some(
                RootedDir::open_read(&root)
                    .unwrap()
                    .identity_token()
                    .unwrap(),
            ),
            scan_id: Some(scan_id),
            entries: HashMap::from([(
                crate::scanner::LOCAL_ONLY_PORTRAIT_ROUTES_FINDING.into(),
                vec![(
                    ".hoi4-mod-setup/install.lock.json".into(),
                    sha256_bytes(excerpt.as_bytes()),
                )],
            )]),
            evidence_sha256: None,
        };

        let error = approve_scan_evidence(
            root.display().to_string(),
            scan_id.to_string(),
            vec![ApprovedEvidence {
                reference: crate::scanner::LOCAL_ONLY_PORTRAIT_ROUTES_FINDING.into(),
                path: ".hoi4-mod-setup/install.lock.json".into(),
                excerpt: excerpt.into(),
                excerpt_sha256: sha256_bytes(excerpt.as_bytes()),
                confidence: Some(1.0),
            }],
        )
        .unwrap_err();
        assert!(error.contains("stay on this computer"));
        assert!(codex_approved_evidence()
            .lock()
            .unwrap()
            .evidence_sha256
            .is_none());
        clear_approved_scan_evidence().unwrap();
    }

    #[test]
    fn approved_launcher_scan_crosses_the_semantic_bridge_without_authorizing_other_paths() {
        let _state_guard = test_state_guard();
        for (linked_worktree, malformed_launcher) in [(false, false), (true, false), (false, true)]
        {
            assert_launcher_scan_bridge(linked_worktree, malformed_launcher);
        }
    }

    fn assert_launcher_scan_bridge(linked_worktree: bool, malformed_launcher: bool) {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("example");
        std::fs::create_dir(&project).unwrap();
        let root = project.canonicalize().unwrap();
        let declared = crate::paths::user_facing_path(&root).replace('\\', "/");
        let launcher = parent.path().join("example.mod");
        std::fs::write(project.join("descriptor.mod"), "name=\"Example\"\n").unwrap();
        std::fs::write(
            &launcher,
            if malformed_launcher {
                format!("path=\"{declared}\"\n")
            } else {
                format!("name=\"Example\"\npath=\"{declared}\"\n")
            },
        )
        .unwrap();
        if linked_worktree {
            std::fs::write(project.join(".git"), "gitdir: ../unopened-worktree\n").unwrap();
        }
        let result = crate::scanner::scan_project(
            &project,
            &ScanOptions {
                approved_external_descriptor: Some(launcher.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!result.partial);
        if malformed_launcher {
            assert!(result.conflicts.iter().any(|conflict| {
                conflict.id == "conflict.launcher.malformed" && conflict.severity == "block"
            }));
        }
        let finding = result
            .findings
            .iter()
            .find(|finding| finding.id == "descriptor.launcher")
            .unwrap();
        assert_eq!(
            finding.value["path"],
            launcher.canonicalize().unwrap().display().to_string()
        );
        let mut evidence = result
            .findings
            .iter()
            .map(|finding| {
                let excerpt =
                    finding
                        .model_excerpt
                        .clone()
                        .unwrap_or_else(|| match &finding.value {
                            Value::String(text) => text.clone(),
                            value => serde_json::to_string(value).unwrap(),
                        });
                ApprovedEvidence {
                    reference: finding.id.clone(),
                    path: finding.evidence[0].path.clone(),
                    excerpt_sha256: sha256_bytes(excerpt.as_bytes()),
                    excerpt,
                    confidence: Some(finding.confidence),
                }
            })
            .collect::<Vec<_>>();
        if linked_worktree {
            assert!(result
                .conflicts
                .iter()
                .any(|conflict| conflict.id == "conflict.git.worktree"));
        }
        evidence.extend(result.conflicts.iter().map(|conflict| {
            let excerpt = conflict.details.clone().unwrap_or_default();
            ApprovedEvidence {
                reference: conflict.id.clone(),
                path: conflict.path.clone(),
                excerpt_sha256: sha256_bytes(excerpt.as_bytes()),
                excerpt,
                confidence: None,
            }
        }));
        for item in &evidence {
            assert!(
                crate::codex::validate_analysis_evidence(std::slice::from_ref(item)).is_ok(),
                "invalid evidence {} at {}",
                item.reference,
                item.path
            );
        }
        *codex_approved_evidence().lock().unwrap() = ApprovedScanEvidence {
            project_root: Some(root.clone()),
            project_root_identity: Some(
                RootedDir::open_read(&root)
                    .unwrap()
                    .identity_token()
                    .unwrap(),
            ),
            scan_id: Some(result.scan_id),
            entries: scan_evidence_entries(&result).unwrap(),
            evidence_sha256: None,
        };
        approve_scan_evidence(
            root.display().to_string(),
            result.scan_id.to_string(),
            evidence.clone(),
        )
        .unwrap();
        let mut request = CodexAnalysisRequest {
            mode: "existing_project_semantics".into(),
            brief: String::new(),
            evidence,
            constraints: serde_json::json!({}),
            analysis_purpose: Some("existing_project_import".into()),
            project_root: Some(root.display().to_string()),
            scan_id: Some(result.scan_id),
        };
        crate::codex::validate_analysis_evidence(&request.evidence).unwrap();
        validate_codex_evidence_approval(&request).unwrap();
        let approved_evidence = request.evidence.clone();
        request.evidence[0].excerpt.push('x');
        assert!(crate::codex::validate_analysis_evidence(&request.evidence).is_err());
        assert!(validate_codex_evidence_approval(&request).is_err());
        request.evidence[0].excerpt_sha256 = sha256_bytes(request.evidence[0].excerpt.as_bytes());
        crate::codex::validate_analysis_evidence(&request.evidence).unwrap();
        assert!(validate_codex_evidence_approval(&request).is_err());
        request.evidence = approved_evidence;
        request.scan_id = Some(Uuid::new_v4());
        assert!(validate_codex_evidence_approval(&request).is_err());
        request.scan_id = Some(result.scan_id);
        request.project_root = Some(parent.path().display().to_string());
        assert!(validate_codex_evidence_approval(&request).is_err());
        request.project_root = Some(root.display().to_string());
        let external = request
            .evidence
            .iter_mut()
            .find(|item| item.reference == "descriptor.launcher")
            .unwrap();
        external.path = parent.path().join("unapproved.mod").display().to_string();
        assert!(crate::codex::validate_analysis_evidence(&request.evidence).is_err());
        assert!(validate_codex_evidence_approval(&request).is_err());
        request
            .evidence
            .iter_mut()
            .find(|item| item.reference == "descriptor.launcher")
            .unwrap()
            .path = "unapproved.mod".into();
        crate::codex::validate_analysis_evidence(&request.evidence).unwrap();
        assert!(validate_codex_evidence_approval(&request).is_err());
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
            project_root_identity: Some(
                RootedDir::open_read(&root)
                    .unwrap()
                    .identity_token()
                    .unwrap(),
            ),
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
            "conventions": {
                "agents_profile": "default",
                "localisation_convention": "english",
                "documentation_convention": "markdown"
            },
            "componentRecommendations": [],
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
        let mut changed_convention = base.clone();
        changed_convention["conventions"]["documentation_convention"] =
            serde_json::json!("plain markdown");
        assert_ne!(
            confirmation_values_sha256(&base).unwrap(),
            confirmation_values_sha256(&changed_convention).unwrap()
        );
        let mut changed_recommendation = base.clone();
        changed_recommendation["componentRecommendations"] = serde_json::json!([{
            "component_id": "core.skills",
            "recommendation": "recommended",
            "reason": "Repeated workflows were requested."
        }]);
        assert_ne!(
            confirmation_values_sha256(&base).unwrap(),
            confirmation_values_sha256(&changed_recommendation).unwrap()
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
            require_maintenance_reanalysis("update", None, project.path(), None, None).unwrap_err();
        assert!(error
            .to_string()
            .contains("fresh, confirmed provider reanalysis"));
        assert!(require_maintenance_reanalysis("repair", None, project.path(), None, None).is_ok());
        assert!(require_maintenance_reanalysis(
            "update",
            Some(&CodexAnalysisRecord {
                engine: "codex_app_server".into(),
                auth_mode: "chatgpt".into(),
                provider: Some("codex".into()),
                model: None,
                reasoning_effort: None,
                optimization_profile: Some("Codex setup analysis".into()),
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
                source_revision: Some("a".repeat(40)),
                source_manifest_sha256: Some("d".repeat(64)),
            }),
            project.path(),
            None,
            None,
        )
        .is_err());
    }

    #[test]
    fn maintenance_reanalysis_uses_the_installed_pinned_source() {
        let project = tempfile::tempdir().unwrap();
        let lock_path = project.path().join(".hoi4-mod-setup/install.lock.json");
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let mut lock: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock["source"]["mode"] = serde_json::json!("pinned_release");
        lock["source"]["requested_ref"] = serde_json::json!("v0.2.12");
        lock["source"]["release"] = serde_json::json!("v0.2.12");
        std::fs::write(&lock_path, serde_json::to_vec(&lock).unwrap()).unwrap();
        let request = CodexAnalysisRequest {
            mode: "existing_project_semantics".into(),
            brief: String::new(),
            evidence: Vec::new(),
            constraints: serde_json::json!({}),
            analysis_purpose: Some("maintenance_reanalysis".into()),
            project_root: Some(project.path().display().to_string()),
            scan_id: None,
        };

        let source = source_request_for_analysis(&request).unwrap();
        assert_eq!(source.mode, SourceMode::PinnedRelease);
        assert_eq!(source.requested_ref.as_deref(), Some("v0.2.12"));
        assert_eq!(source.release.as_deref(), Some("v0.2.12"));

        lock["source"]["mode"] = serde_json::json!("latest");
        lock["source"]["requested_ref"] = Value::Null;
        lock["source"]["release"] = Value::Null;
        std::fs::write(&lock_path, serde_json::to_vec(&lock).unwrap()).unwrap();
        let mut renderer_pinned = request;
        renderer_pinned.constraints = serde_json::json!({
            "source": { "mode": "pinned_commit", "selected_ref": "f".repeat(40) }
        });
        let latest = source_request_for_analysis(&renderer_pinned).unwrap();
        assert_eq!(latest.mode, SourceMode::Latest);
        assert!(latest.requested_ref.is_none());
        assert!(latest.release.is_none());
    }

    #[test]
    fn explicit_update_targets_replace_pins_without_mutating_the_installed_lock() {
        let project = tempfile::tempdir().unwrap();
        let lock_path = project.path().join(".hoi4-mod-setup/install.lock.json");
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.source.mode = SourceMode::PinnedRelease;
        lock.source.release = Some("old-release".into());
        let bytes = serde_json::to_vec(&lock).unwrap();
        std::fs::write(&lock_path, &bytes).unwrap();
        for value in [
            serde_json::json!({"mode": "latest"}),
            serde_json::json!({"mode": "pinned_commit", "selected_ref": "f".repeat(40)}),
            serde_json::json!({"mode": "pinned_release", "selected_ref": "new-release"}),
        ] {
            let selection: UpdateSourceSelection = serde_json::from_value(value.clone()).unwrap();
            let plan_request = maintenance_source_request(&lock, Some(&selection)).unwrap();
            let request = CodexAnalysisRequest {
                mode: "existing_project_semantics".into(),
                brief: String::new(),
                evidence: Vec::new(),
                constraints: serde_json::json!({"update_source": value}),
                analysis_purpose: Some("maintenance_reanalysis".into()),
                project_root: Some(project.path().display().to_string()),
                scan_id: None,
            };
            let analysis_request = source_request_for_analysis(&request).unwrap();
            assert_eq!(
                serde_json::to_value(plan_request).unwrap(),
                serde_json::to_value(analysis_request).unwrap()
            );
            assert_eq!(std::fs::read(&lock_path).unwrap(), bytes);
        }
    }

    #[test]
    fn update_source_rejects_incomplete_or_ambiguous_pins() {
        for value in [
            serde_json::json!({"mode": "latest", "selected_ref": "old"}),
            serde_json::json!({"mode": "pinned_commit", "selected_ref": "main"}),
            serde_json::json!({"mode": "pinned_release"}),
            serde_json::json!({"mode": "pinned_release", "selected_ref": "a\nb"}),
            serde_json::json!({"mode": "pinned_release", "selected_ref": "a".repeat(257)}),
        ] {
            let selection: UpdateSourceSelection = serde_json::from_value(value).unwrap();
            assert!(selection.request().is_err());
        }
        assert!(serde_json::from_value::<UpdateSourceSelection>(
            serde_json::json!({"mode": "latest", "repository": "other"})
        )
        .is_err());
    }

    #[test]
    fn update_keeps_same_release_immutable_but_allows_explicit_retargeting() {
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.source.mode = SourceMode::PinnedRelease;
        lock.source.release = Some("old".into());
        let resolved = SourceIdentity {
            repository: lock.source.repository.clone(),
            mode: SourceMode::PinnedRelease,
            resolved_revision: "f".repeat(40),
            requested_ref: None,
            release: Some("old".into()),
            manifest_sha256: "a".repeat(64),
            manifest_origin: "remote".into(),
        };
        let same = maintenance_source_request(&lock, None).unwrap();
        assert!(validate_update_release_identity(&lock, &same, &resolved).is_err());
        for selection in [
            UpdateSourceSelection {
                mode: SourceMode::Latest,
                selected_ref: None,
            },
            UpdateSourceSelection {
                mode: SourceMode::PinnedRelease,
                selected_ref: Some("new".into()),
            },
            UpdateSourceSelection {
                mode: SourceMode::PinnedCommit,
                selected_ref: Some("f".repeat(40)),
            },
        ] {
            assert!(validate_update_release_identity(
                &lock,
                &selection.request().unwrap(),
                &resolved
            )
            .is_ok());
        }
    }

    #[test]
    fn repair_cannot_receive_an_update_source_override() {
        let error = tauri::async_runtime::block_on(build_maintenance_plan(
            "repair".into(),
            "unopened-project".into(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(UpdateSourceSelection {
                mode: SourceMode::Latest,
                selected_ref: None,
            }),
        ))
        .unwrap_err();
        assert!(!error.contains("unopened-project"));
        assert!(error.contains("Only Update"));
    }

    #[test]
    fn guidance_refresh_reconciles_real_maintenance_operations_without_losing_local_review() {
        for mode in ["repair", "reinstall"] {
            for (path, component) in [
                ("CLAUDE.md", "core.claude.instructions"),
                ("AGENTS.md", "core.agents"),
            ] {
                for state in [
                    "healthy",
                    "missing",
                    "modified",
                    "preserved",
                    "preserved_missing",
                ] {
                    let project = tempfile::tempdir().unwrap();
                    let mut lock: InstallationLock = serde_json::from_str(include_str!(
                        "../../docs/examples/installation-lock.example.json"
                    ))
                    .unwrap();
                    lock.files.retain(|file| file.path == "AGENTS.md");
                    let file = &mut lock.files[0];
                    file.path = path.into();
                    file.component_id = component.into();
                    file.source_path = format!("{path}.template");
                    file.source_sha256 = sha256_bytes(b"source guidance");
                    file.source_size = Some(15);
                    file.installed_sha256 = sha256_bytes(b"adapted guidance");
                    file.preserved_local = matches!(state, "preserved" | "preserved_missing");
                    file.platform = Some(ManifestPlatform::All);
                    let selection = crate::source::SelectedSourceFile {
                        component_id: component.into(),
                        source_path: file.source_path.clone(),
                        destination: path.into(),
                        ownership: file.ownership,
                        expected_sha256: Some(file.source_sha256.clone()),
                        expected_size: file.source_size,
                        executable: false,
                        platform: ManifestPlatform::All,
                    };
                    if !matches!(state, "missing" | "preserved_missing") {
                        std::fs::write(
                            project.path().join(path),
                            if state == "modified" {
                                b"user changes".as_slice()
                            } else {
                                b"adapted guidance".as_slice()
                            },
                        )
                        .unwrap();
                    }
                    let before = std::fs::read(project.path().join(path)).ok();
                    let mut operations = if mode == "repair" {
                        crate::transaction::repair_operations(&lock, project.path())
                    } else {
                        crate::transaction::reinstall_operations(&lock, project.path())
                    }
                    .unwrap();
                    let original_id = operations[0].id.clone();
                    for _ in 0..2 {
                        append_additional_component_operations(
                            &mut operations,
                            std::slice::from_ref(&selection),
                            &lock,
                            project.path(),
                            &super_events_test_identity(),
                            true,
                        )
                        .unwrap();
                        assert_eq!(operations.len(), 1, "{mode} {path} {state}");
                        assert_eq!(operations[0].id, original_id);
                    }
                    let operation = &operations[0];
                    let expected = match state {
                        "healthy" => OperationAction::Replace,
                        "missing" => OperationAction::Create,
                        _ => OperationAction::Skip,
                    };
                    assert_eq!(operation.action, expected, "{mode} {path} {state}");
                    assert_eq!(operation.source_sha256, selection.expected_sha256);
                    assert_eq!(operation.ownership, Some(selection.ownership));
                    if matches!(state, "modified" | "preserved" | "preserved_missing") {
                        assert_eq!(
                            operation.local_state,
                            if state == "preserved_missing" {
                                LocalState::Unknown
                            } else {
                                LocalState::Modified
                            }
                        );
                        assert_eq!(operation.resolution.as_deref(), Some("review_required"));
                        assert_eq!(
                            maintenance_conflicts(&operations, &[], &HashMap::new()).len(),
                            1
                        );
                    }
                    assert_eq!(
                        operation.rollback,
                        if matches!(state, "missing" | "preserved_missing") {
                            RollbackAction::RemoveCreated
                        } else {
                            RollbackAction::RestoreBackup
                        }
                    );
                    assert_eq!(std::fs::read(project.path().join(path)).ok(), before);
                    let mut plan: InstallationPlan = serde_json::from_str(include_str!(
                        "../../docs/examples/installation-plan.example.json"
                    ))
                    .unwrap();
                    plan.operations = operations;
                    plan.generated_artifacts.clear();
                    plan.conflicts.clear();
                    plan.git_setup = None;
                    let record = plan.codex_analysis.as_mut().unwrap();
                    record.provider = Some(plan.ai_provider.clone());
                    record.model = Some(plan.ai_model.clone());
                    record.reasoning_effort = Some(plan.ai_reasoning_effort.clone());
                    record.optimization_profile = Some(plan.ai_optimization_profile.clone());
                    plan.approvals.dry_run_reviewed = true;
                    plan.approvals.external_actions_reviewed = true;
                    crate::transaction::validate_plan(&plan).unwrap();
                    if state == "preserved_missing" {
                        plan.operations[0].action = OperationAction::Replace;
                        plan.operations[0].resolution = Some("replace".into());
                        crate::transaction::validate_plan(&plan).unwrap();
                        assert_eq!(plan.operations[0].rollback, RollbackAction::RemoveCreated);
                    }
                    let mut duplicate = plan.operations[0].clone();
                    duplicate.id.push_str("-duplicate");
                    plan.operations.push(duplicate);
                    assert!(crate::transaction::validate_plan(&plan)
                        .unwrap_err()
                        .to_string()
                        .contains("duplicate plan destination"));
                }
            }
        }
    }

    #[test]
    fn maintenance_dependency_overlap_is_idempotent_but_conflicting_evidence_is_rejected() {
        let project = tempfile::tempdir().unwrap();
        let mut lock: InstallationLock = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        lock.files.clear();
        lock.components.clear();
        let selection = crate::source::SelectedSourceFile {
            component_id: "workflow.example".into(),
            source_path: "example.md".into(),
            destination: "example.md".into(),
            ownership: Ownership::Managed,
            expected_sha256: Some(sha256_bytes(b"incoming")),
            expected_size: Some(8),
            executable: false,
            platform: ManifestPlatform::All,
        };
        let mut operations = Vec::new();
        for _ in 0..2 {
            append_additional_component_operations(
                &mut operations,
                std::slice::from_ref(&selection),
                &lock,
                project.path(),
                &super_events_test_identity(),
                false,
            )
            .unwrap();
        }
        assert_eq!(operations.len(), 1);
        for mutation in [
            "owner",
            "ownership",
            "scope",
            "hash",
            "case",
            "decision",
            "delete",
        ] {
            let mut conflicting = operations.clone();
            match mutation {
                "owner" => conflicting[0].component_id = "other.owner".into(),
                "ownership" => conflicting[0].ownership = Some(Ownership::Merged),
                "scope" => conflicting[0].location_scope = Some("application_data".into()),
                "hash" => conflicting[0].source_sha256 = Some("f".repeat(64)),
                "case" => conflicting[0].destination = "EXAMPLE.md".into(),
                "decision" => conflicting[0].resolution = Some("keep".into()),
                "delete" => conflicting[0].action = OperationAction::DeleteManaged,
                _ => unreachable!(),
            }
            let before = serde_json::to_value(&conflicting).unwrap();
            assert!(append_additional_component_operations(
                &mut conflicting,
                std::slice::from_ref(&selection),
                &lock,
                project.path(),
                &super_events_test_identity(),
                false
            )
            .is_err());
            assert_eq!(serde_json::to_value(&conflicting).unwrap(), before);
        }
        let mut other_selection = selection.clone();
        other_selection.source_path = "other.md".into();
        other_selection.destination = "other.md".into();
        append_additional_component_operations(
            &mut operations,
            &[other_selection],
            &lock,
            project.path(),
            &super_events_test_identity(),
            false,
        )
        .unwrap();
        assert_eq!(operations.len(), 2);
        assert_ne!(operations[0].id, operations[1].id);
        assert!(!project.path().join("example.md").exists());
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
            &super_events_test_identity(),
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
            &super_events_test_identity(),
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
            project_root_identity: Some(
                RootedDir::open_read(&root)
                    .unwrap()
                    .identity_token()
                    .unwrap(),
            ),
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
            reasoning_effort: None,
            optimization_profile: Some("Codex setup analysis".into()),
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
            source_revision: Some("a".repeat(40)),
            source_manifest_sha256: Some("d".repeat(64)),
        };
        let confirmation_values = serde_json::json!({
            "description": "A focused project.",
            "folderProfile": [],
            "identity": {
                "displayName": "Project",
                "projectId": "project",
                "author": "Author",
                "version": "0.1.0",
                "supportedGameVersion": "1.17.*",
                "projectRoot": root.display().to_string(),
                "defaultBranch": "main"
            },
            "conventions": {
                "agents_profile": "default",
                "localisation_convention": "english",
                "documentation_convention": "markdown"
            },
            "componentRecommendations": []
        });
        let confirmation_hash = confirmation_values_sha256(&confirmation_values).unwrap();
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
                confirmed_values_sha256: Some(confirmation_hash.clone()),
            },
        );
        assert!(require_maintenance_reanalysis(
            "update",
            Some(&record),
            &root,
            None,
            Some(&confirmation_hash)
        )
        .is_ok());

        // The Claude account route records no endpoint in the lock and bound
        // the empty endpoint when its analysis ran.
        let mut claude = record.clone();
        claude.provider = Some("claude_account".into());
        claude.engine = "claude_code_cli".into();
        claude.auth_mode = "claude_account".into();
        claude.model = Some(crate::claude_code::DEFAULT_MODEL.into());
        {
            let mut analyses = codex_analyses().lock().unwrap();
            let pending = analyses.get_mut(&analysis_id).unwrap();
            pending.record = claude.clone();
            pending.confirmed = Some(claude.clone());
            pending.endpoint_fingerprint = Some(sha256_bytes(b""));
        }
        assert!(require_maintenance_reanalysis(
            "update",
            Some(&claude),
            &root,
            None,
            Some(&confirmation_hash)
        )
        .is_ok());
        assert!(require_maintenance_reanalysis(
            "update",
            Some(&claude),
            &root,
            Some("https://other.example/v1"),
            Some(&confirmation_hash)
        )
        .is_err());
        {
            let mut analyses = codex_analyses().lock().unwrap();
            let pending = analyses.get_mut(&analysis_id).unwrap();
            pending.record = record.clone();
            pending.confirmed = Some(record.clone());
            pending.endpoint_fingerprint = None;
        }

        let mut stale = record;
        stale.evidence_sha256 = Some("d".repeat(64));
        assert!(require_maintenance_reanalysis(
            "update",
            Some(&stale),
            &root,
            None,
            Some(&confirmation_hash)
        )
        .is_err());
        codex_analyses().lock().unwrap().clear();
        codex_approved_evidence().lock().unwrap().entries.clear();
    }

    #[test]
    fn import_record_from_the_renderer_regains_its_core_scan_binding() {
        let _state_guard = test_state_guard();
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        let scan_id = Uuid::new_v4();
        let analysis_id = Uuid::new_v4();
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: Some("gpt-5.6-luna".into()),
            reasoning_effort: Some("xhigh".into()),
            optimization_profile: Some("Codex setup analysis".into()),
            analysis_id,
            schema_version: "1.0.0".into(),
            input_sha256: "a".repeat(64),
            output_sha256: "b".repeat(64),
            confirmed_fields: crate::codex::REQUIRED_ANALYSIS_PROPOSAL_KEYS
                .iter()
                .map(|field| (*field).into())
                .collect(),
            confirmed_at: "2026-10-03T00:00:00Z".into(),
            account_identity_persisted: false,
            analysis_purpose: Some("existing_project_import".into()),
            project_root: Some(root.display().to_string()),
            scan_id: Some(scan_id),
            evidence_sha256: Some("c".repeat(64)),
            source_revision: Some("a".repeat(40)),
            source_manifest_sha256: Some("d".repeat(64)),
        };
        let state = serde_json::json!({
            "aiProvider": "codex",
            "aiModel": "gpt-5.6-luna",
            "aiReasoningEffort": "xhigh",
            "scanContext": {"scanId": scan_id.to_string()},
            // The renderer receives the record without core-only bindings.
            "codexAnalysisRecord": serde_json::to_value(&record).unwrap(),
            "description": "A focused project.",
            "folderProfile": [],
            "identity": {
                "displayName": "Project",
                "projectId": "project",
                "author": "Author",
                "version": "0.1.0",
                "supportedGameVersion": "1.19.*",
                "projectRoot": root.display().to_string(),
                "defaultBranch": "main"
            },
            "conventions": {
                "agents_profile": "default",
                "localisation_convention": "english",
                "documentation_convention": "markdown"
            },
        });
        assert!(state["codexAnalysisRecord"].get("scan_id").is_none());
        let confirmation_hash =
            confirmation_values_sha256(&confirmation_values_from_state(&state).unwrap()).unwrap();
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
                confirmed_values_sha256: Some(confirmation_hash),
            },
        );

        let restored = codex_analysis_from_state(&state, &root).unwrap();

        assert_eq!(restored.scan_id, Some(scan_id));
        assert!(restored
            .project_root
            .as_deref()
            .is_some_and(|value| same_project_root(Path::new(value), &root)));
        codex_analyses().lock().unwrap().remove(&analysis_id);
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
            reasoning_effort: Some("high".into()),
            optimization_profile: Some("Claude setup analysis".into()),
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
            source_revision: Some("a".repeat(40)),
            source_manifest_sha256: Some("d".repeat(64)),
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
            external_parent_identity: None,
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
                script_prefix: Some("example".into()),
                primary_namespace: Some("example".into()),
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
                ai_model: "gpt-5.6-luna".into(),
                ai_reasoning_effort: "xhigh".into(),
                ai_endpoint: None,
                ai_optimization_profile: "Codex setup analysis".into(),
                primary_coding_environment: "codex".into(),
                additional_coding_environments: vec![],
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
                portrait_pipeline: None,
                image_generation: None,
                live_testing: None,
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
                    project_root_identity: None,
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
            external_parent_identity: None,
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
        assert_eq!(normalized_three_d_state("ready", None, false), "incomplete");
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
                    reasoning_effort: None,
                    optimization_profile: Some("Codex setup analysis".into()),
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
                    source_revision: Some("a".repeat(40)),
                    source_manifest_sha256: Some("d".repeat(64)),
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
            project_root_identity: None,
            scan_id: Some(Uuid::new_v4()),
            entries: HashMap::from([("finding".into(), Vec::new())]),
            evidence_sha256: None,
        };
    }

    #[test]
    fn interrupted_codex_session_clears_proposals_but_preserves_approved_scan_evidence() {
        let _state_guard = test_state_guard();
        seed_codex_local_state_for_test();

        clear_codex_analysis_state().unwrap();

        assert!(codex_analyses().lock().unwrap().is_empty());
        let evidence = codex_approved_evidence().lock().unwrap();
        assert_eq!(
            evidence.project_root,
            Some(PathBuf::from("C:/mods/example"))
        );
        assert!(evidence.scan_id.is_some());
        assert!(evidence.entries.contains_key("finding"));
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

    #[test]
    fn logout_clears_process_local_ready_project_evidence() {
        let _state_guard = test_state_guard();
        ready_projects()
            .lock()
            .unwrap()
            .insert("C:/mods/example".into(), "lock-sha256".into());

        let result = complete_codex_logout(Err("Sign-out could not reach Codex.".into()));

        assert_eq!(result.unwrap_err(), "Sign-out could not reach Codex.");
        assert!(ready_projects().lock().unwrap().is_empty());
    }
}
