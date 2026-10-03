//! Claude account route through the user's own installed Claude Code.
//!
//! Anthropic does not permit third-party applications to offer Claude.ai
//! login or to handle Claude account tokens. This adapter therefore never
//! starts an OAuth flow of its own: it locates the user's unmodified,
//! Anthropic-signed `claude` executable, asks it for a non-secret sign-in
//! summary, starts Claude Code's own `claude auth login` flow, and runs one
//! bounded, tool-free, settings-free print-mode turn for setup analysis.
//! Claude Code owns credential storage, refresh, and sign-out. The adapter
//! reads only the `loggedIn`, `authMethod`, and `apiProvider` fields and
//! discards every other status field, so email addresses and organization
//! names never enter app state, logs, plans, or locks.

use crate::codex::{
    analysis_input_sha256, analysis_prompt_for_provider, validate_analysis_output, AiAccountStatus,
    AiAnalysisRequest, CodexAnalysisResult,
};
use crate::models::{AiModelOption, Platform};
use crate::process::{ProcessResult, ProcessSpec};
use crate::security::{redact_secrets, sha256_file};
use crate::AppError;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const PROVIDER_ID: &str = "claude_account";
pub const ENGINE: &str = "claude_code_cli";
pub const AUTH_MODE: &str = "claude_account";
/// Official Claude Haiku 4.5 model ID from Anthropic's model documentation.
pub const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
/// Official Claude Code setup page used for the install guidance link.
pub const SETUP_URL: &str = "https://code.claude.com/docs/en/setup";
/// Reviewed signing identity of the official Claude Code binary.
pub const PUBLISHER: &str = "Anthropic";

const ANALYSIS_SCHEMA: &str = include_str!("../../docs/schemas/codex-analysis.schema.json");
const STATUS_TIMEOUT_SECONDS: u64 = 30;
const LOGIN_TIMEOUT_SECONDS: u64 = 10 * 60;
const ANALYSIS_TIMEOUT_SECONDS: u64 = 5 * 60;
const MAX_STATUS_BYTES: usize = 64 * 1024;
const MAX_HELP_BYTES: usize = 256 * 1024;
const MAX_ANALYSIS_BYTES: usize = 4 * 1024 * 1024;

/// Print-mode flags this adapter depends on. A Claude Code build that does
/// not advertise every flag is reported as needing an update instead of being
/// run with weaker isolation.
const REQUIRED_PRINT_FLAGS: [&str; 9] = [
    "--print",
    "--output-format",
    "--json-schema",
    "--model",
    "--tools",
    "--strict-mcp-config",
    "--safe-mode",
    "--no-session-persistence",
    "--system-prompt",
];

/// Non-secret environment that Claude Code needs to find the user's own
/// sign-in and network configuration. Every Anthropic or Claude Code variable
/// that could override the user's login (API keys, auth tokens, alternate
/// providers, or a host session's injected state) is deliberately excluded;
/// only the user's explicit config-directory choice is preserved.
const PASSTHROUGH_ENVIRONMENT: [&str; 11] = [
    "USER",
    "LOGNAME",
    "CLAUDE_CONFIG_DIR",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
    "NODE_EXTRA_CA_CERTS",
    "ProgramData",
];

#[derive(Debug, Clone)]
struct CachedExecutable {
    path: PathBuf,
    sha256: String,
    supports_required_flags: bool,
    length: u64,
    modified: Option<std::time::SystemTime>,
}

fn file_fingerprint(path: &Path) -> Option<(u64, Option<std::time::SystemTime>)> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    metadata
        .is_file()
        .then(|| (metadata.len(), metadata.modified().ok()))
}

static CLAUDE_EXECUTABLE: OnceLock<Mutex<Option<CachedExecutable>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeSignInSummary {
    pub logged_in: bool,
    pub auth_method: String,
    pub first_party: bool,
}

/// The checked-in model choices. Claude Code has no model-catalog command,
/// so these are labelled as built-in choices rather than a live result.
pub fn builtin_models() -> Vec<AiModelOption> {
    vec![AiModelOption {
        id: DEFAULT_MODEL.into(),
        display_name: "Claude Haiku 4.5".into(),
        default_reasoning_effort: "high".into(),
        supported_reasoning_efforts: vec!["high".into()],
    }]
}

/// Haiku 4.5 does not accept the effort parameter, so the adapter forwards an
/// effort level only for a model that is documented to support it.
fn model_supports_effort(model: &str) -> bool {
    !model.contains("haiku")
}

pub fn validate_model(model: &str) -> Result<(), AppError> {
    if model.is_empty()
        || model.len() > 128
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_'))
        || model.starts_with('-')
    {
        return Err(AppError::InvalidInput(
            "the Claude model name is invalid".into(),
        ));
    }
    Ok(())
}

fn executable_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["claude.exe"]
    } else {
        &["claude"]
    }
}

/// Candidate launcher locations, in priority order: PATH entries first, then
/// the documented native-installer launcher directory and the Homebrew
/// prefixes on macOS. npm `.cmd` shims are not executables and are skipped.
fn executable_candidates(path: Option<&std::ffi::OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let names = executable_names();
    let mut candidates = Vec::new();
    if let Some(path) = path {
        candidates.extend(
            std::env::split_paths(path)
                .filter(|entry| entry.is_absolute())
                .flat_map(|entry| names.iter().map(move |name| entry.join(name))),
        );
    }
    if let Some(home) = home {
        candidates.extend(
            names
                .iter()
                .map(|name| home.join(".local").join("bin").join(name)),
        );
    }
    #[cfg(target_os = "macos")]
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ]);
    let mut unique = Vec::new();
    for candidate in candidates {
        if !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

fn home_directory() -> Option<PathBuf> {
    let name = if cfg!(target_os = "windows") {
        "USERPROFILE"
    } else {
        "HOME"
    };
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Resolve a launcher to the real signed binary. The native installer places
/// a symbolic link in `~/.local/bin` on macOS and Homebrew links its cask, so
/// a link is followed exactly once through canonicalization; the resolved
/// target itself must be a regular file with no link component.
fn resolve_candidate(candidate: &Path) -> Option<PathBuf> {
    let metadata = std::fs::symlink_metadata(candidate).ok()?;
    if !(metadata.is_file() || metadata.file_type().is_symlink()) {
        return None;
    }
    let resolved = crate::codex::resolve_reviewed_executable_path(candidate)?;
    let resolved_metadata = std::fs::symlink_metadata(&resolved).ok()?;
    if !resolved_metadata.is_file() || crate::security::path_has_link_component(&resolved) {
        return None;
    }
    Some(resolved)
}

fn help_supports_required_flags(help: &str) -> bool {
    REQUIRED_PRINT_FLAGS.iter().all(|flag| {
        help.split(|character: char| character.is_whitespace() || character == ',')
            .any(|token| token == *flag)
    })
}

fn probe_required_flags(executable: &Path, sha256: &str) -> Result<bool, AppError> {
    let result = run_claude(
        executable,
        sha256,
        vec!["--help".into()],
        None,
        None,
        MAX_HELP_BYTES,
        STATUS_TIMEOUT_SECONDS,
        None,
    )?;
    if result.timed_out || result.stdout_truncated {
        return Err(AppError::Process(
            "Claude Code capabilities could not be read".into(),
        ));
    }
    Ok(help_supports_required_flags(&format!(
        "{}\n{}",
        result.stdout, result.stderr
    )))
}

/// Locate the user's official Claude Code. Returns the reviewed path, its
/// content hash, and whether it supports the isolated print mode. A cached
/// entry is reused while the file's size and modification time are unchanged;
/// every spawn still re-verifies the exact content hash immediately before
/// the process starts, so a replaced binary is never run.
fn find_executable() -> Result<CachedExecutable, AppError> {
    let cache = CLAUDE_EXECUTABLE.get_or_init(|| Mutex::new(None));
    if let Ok(cached) = cache.lock() {
        if let Some(cached) = cached.as_ref() {
            if file_fingerprint(&cached.path) == Some((cached.length, cached.modified)) {
                return Ok(cached.clone());
            }
        }
    }
    let path = std::env::var_os("PATH");
    let home = home_directory();
    let mut found = None;
    for candidate in executable_candidates(path.as_deref(), home.as_deref()) {
        let Some(resolved) = resolve_candidate(&candidate) else {
            continue;
        };
        if crate::process::validate_executable_publisher(&resolved, PUBLISHER).is_err() {
            continue;
        }
        let Ok(sha256) = sha256_file(&resolved) else {
            continue;
        };
        found = Some((resolved, sha256));
        break;
    }
    let (path, sha256) = found
        .ok_or_else(|| AppError::Process("official Claude Code executable was not found".into()))?;
    let (length, modified) = file_fingerprint(&path)
        .ok_or_else(|| AppError::Process("official Claude Code executable was not found".into()))?;
    let supports_required_flags = probe_required_flags(&path, &sha256)?;
    let entry = CachedExecutable {
        path,
        sha256,
        supports_required_flags,
        length,
        modified,
    };
    if let Ok(mut cached) = cache.lock() {
        *cached = Some(entry.clone());
    }
    Ok(entry)
}

/// Locate and signature-check Claude Code ahead of the first account check.
/// Failures are ignored; the account check reports them normally.
pub fn warm_executable() {
    let _ = find_executable();
}

fn forget_executable() {
    if let Some(cache) = CLAUDE_EXECUTABLE.get() {
        if let Ok(mut cached) = cache.lock() {
            *cached = None;
        }
    }
}

fn ready_executable() -> Result<CachedExecutable, AppError> {
    let executable = find_executable()?;
    if !executable.supports_required_flags {
        return Err(AppError::Process(
            "Claude Code needs an update to support isolated setup analysis".into(),
        ));
    }
    Ok(executable)
}

fn passthrough_environment() -> Vec<(String, OsString)> {
    PASSTHROUGH_ENVIRONMENT
        .iter()
        .filter_map(|name| {
            let value = std::env::var_os(name)?;
            if *name == "CLAUDE_CONFIG_DIR" && !Path::new(&value).is_absolute() {
                return None;
            }
            Some(((*name).to_owned(), value))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn run_claude(
    executable: &Path,
    sha256: &str,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    stdin: Option<&[u8]>,
    max_output_bytes: usize,
    timeout_seconds: u64,
    should_stop: Option<&mut dyn FnMut() -> bool>,
) -> Result<ProcessResult, AppError> {
    let spec = ProcessSpec {
        executable: executable.to_path_buf(),
        executable_sha256: Some(sha256.to_owned()),
        args,
        cwd,
        platform: Platform::current(),
        environment_names: Vec::new(),
        timeout_seconds,
        max_output_bytes,
    };
    let environment = passthrough_environment();
    spec.run_reviewed_tool(
        &[executable.to_path_buf()],
        &environment,
        stdin,
        should_stop,
    )
}

/// Parse `claude auth status --json`, retaining only non-identifying fields.
pub fn parse_sign_in_summary(stdout: &str) -> Result<ClaudeSignInSummary, AppError> {
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|_| {
        AppError::Protocol("Claude Code returned an unreadable sign-in status".into())
    })?;
    let logged_in = value
        .get("loggedIn")
        .and_then(Value::as_bool)
        .ok_or_else(|| AppError::Protocol("Claude Code sign-in status omitted loggedIn".into()))?;
    let auth_method = value
        .get("authMethod")
        .and_then(Value::as_str)
        .filter(|method| {
            method.len() <= 32
                && method
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
        .unwrap_or("unknown")
        .to_owned();
    let first_party = value
        .get("apiProvider")
        .and_then(Value::as_str)
        .is_none_or(|provider| provider == "firstParty");
    Ok(ClaudeSignInSummary {
        logged_in,
        auth_method,
        first_party,
    })
}

pub fn read_sign_in() -> Result<ClaudeSignInSummary, AppError> {
    let executable = ready_executable()?;
    let result = run_claude(
        &executable.path,
        &executable.sha256,
        vec!["auth".into(), "status".into(), "--json".into()],
        None,
        None,
        MAX_STATUS_BYTES,
        STATUS_TIMEOUT_SECONDS,
        None,
    )?;
    if result.timed_out || result.stdout_truncated {
        return Err(AppError::Process(
            "Claude Code sign-in status timed out".into(),
        ));
    }
    parse_sign_in_summary(&result.stdout)
}

fn status_from_error(model: &str, error: &AppError) -> AiAccountStatus {
    let message = match error {
        AppError::Process(message) if message.contains("was not found") => {
            "Claude Code is not installed. Install it, then choose Check again."
        }
        AppError::Process(message) if message.contains("needs an update") => {
            "Claude Code needs an update. Run claude update, then choose Check again."
        }
        _ => "Claude Code could not be reached. Choose Check again.",
    };
    AiAccountStatus {
        available: false,
        authenticated: false,
        provider: PROVIDER_ID.into(),
        model: model.into(),
        auth_mode: AUTH_MODE.into(),
        usage_limited: false,
        error: Some(message.into()),
    }
}

pub fn status_from_summary(model: &str, summary: &ClaudeSignInSummary) -> AiAccountStatus {
    let authenticated = summary.logged_in && summary.first_party;
    let error = if authenticated {
        None
    } else if summary.logged_in {
        Some("Claude Code is set up for another provider. Sign in with a Claude account in Claude Code to use it here.".into())
    } else {
        Some("Sign in to Claude to continue.".into())
    };
    AiAccountStatus {
        available: true,
        authenticated,
        provider: PROVIDER_ID.into(),
        model: model.into(),
        auth_mode: AUTH_MODE.into(),
        usage_limited: false,
        error,
    }
}

pub fn account_status(model: &str) -> AiAccountStatus {
    match read_sign_in() {
        Ok(summary) => status_from_summary(model, &summary),
        Err(error) => {
            if matches!(error, AppError::Process(_)) {
                forget_executable();
            }
            status_from_error(model, &error)
        }
    }
}

/// Run Claude Code's own browser sign-in until it finishes, fails, times out,
/// or the caller cancels. Standard input is closed so the app can never
/// receive or forward an authorization code; if the browser callback cannot
/// complete, the user signs in from a terminal with Claude Code directly.
pub fn run_login(should_stop: &mut dyn FnMut() -> bool) -> Result<(), AppError> {
    let executable = ready_executable()?;
    let result = run_claude(
        &executable.path,
        &executable.sha256,
        vec!["auth".into(), "login".into(), "--claudeai".into()],
        None,
        None,
        MAX_STATUS_BYTES,
        LOGIN_TIMEOUT_SECONDS,
        Some(should_stop),
    )?;
    if result.timed_out {
        return Err(AppError::Credential(
            "Claude sign-in timed out before it finished".into(),
        ));
    }
    if result.status_code != Some(0) {
        return Err(AppError::Credential("Claude sign-in did not finish".into()));
    }
    Ok(())
}

pub fn logout() -> Result<(), AppError> {
    let executable = ready_executable()?;
    let result = run_claude(
        &executable.path,
        &executable.sha256,
        vec!["auth".into(), "logout".into()],
        None,
        None,
        MAX_STATUS_BYTES,
        STATUS_TIMEOUT_SECONDS,
        None,
    )?;
    if result.timed_out || result.status_code != Some(0) {
        return Err(AppError::Process(
            "Claude Code sign-out did not finish".into(),
        ));
    }
    Ok(())
}

fn analysis_system_prompt() -> String {
    format!(
        "You are a bounded setup assistant. Use no tools. Return only one JSON object matching this exact schema. Do not disclose account data, hidden reasoning, credentials, or filesystem content.\n\noutput_schema={ANALYSIS_SCHEMA}"
    )
}

/// Build the isolated print-mode arguments. Tools, MCP servers, user and
/// project customizations, and session persistence are all disabled; the
/// prompt travels on standard input rather than the command line.
pub fn analysis_arguments(model: &str, reasoning_effort: &str) -> Result<Vec<String>, AppError> {
    validate_model(model)?;
    crate::ai::validate_reasoning_effort(reasoning_effort)?;
    let mut args = vec![
        "--print".to_owned(),
        "--output-format".into(),
        "json".into(),
        "--json-schema".into(),
        serde_json::to_string(&serde_json::from_str::<Value>(ANALYSIS_SCHEMA)?)?,
        "--model".into(),
        model.into(),
        "--tools".into(),
        String::new(),
        "--strict-mcp-config".into(),
        "--safe-mode".into(),
        "--no-session-persistence".into(),
        "--system-prompt".into(),
        analysis_system_prompt(),
    ];
    if model_supports_effort(model) {
        args.push("--effort".into());
        args.push(reasoning_effort.trim().into());
    }
    Ok(args)
}

fn classify_result_error(text: &str) -> AppError {
    let lower = text.to_ascii_lowercase();
    if lower.contains("not logged in") || lower.contains("/login") || lower.contains("oauth") {
        AppError::Credential("sign in to Claude before continuing".into())
    } else if lower.contains("limit") || lower.contains("quota") || lower.contains("credit") {
        AppError::Credential("Claude usage is currently limited".into())
    } else if lower.contains("model") {
        AppError::InvalidInput("Claude Code rejected the selected model".into())
    } else {
        AppError::Process("Claude Code could not complete the analysis".into())
    }
}

/// Extract the schema-shaped object from a print-mode JSON result. Raw result
/// text never leaves this function; failures are mapped to sanitized
/// categories so account details in an error string cannot reach the UI.
pub fn extract_analysis_output(stdout: &str) -> Result<Value, AppError> {
    let envelope: Value = serde_json::from_str(stdout.trim())
        .map_err(|_| AppError::Protocol("Claude Code returned an unreadable result".into()))?;
    let result_text = envelope.get("result").and_then(Value::as_str).unwrap_or("");
    if envelope.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Err(classify_result_error(result_text));
    }
    if let Some(structured) = envelope
        .get("structured_output")
        .filter(|value| value.is_object())
    {
        return Ok(structured.clone());
    }
    let trimmed = result_text.trim();
    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    serde_json::from_str::<Value>(without_fence)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| {
            AppError::Serialization("Claude Code returned no structured analysis object".into())
        })
}

pub fn analyze(
    request: &AiAnalysisRequest,
    optimization_profile: &str,
) -> Result<CodexAnalysisResult, AppError> {
    let executable = ready_executable()?;
    let summary = read_sign_in()?;
    if !(summary.logged_in && summary.first_party) {
        return Err(AppError::Credential(
            "sign in to Claude before continuing".into(),
        ));
    }
    let input_sha256 = analysis_input_sha256(&request.analysis)?;
    let prompt =
        analysis_prompt_for_provider(&request.analysis, &input_sha256, optimization_profile)?;
    let args = analysis_arguments(&request.model, &request.reasoning_effort)?;
    // An empty private working directory keeps project paths and any
    // directory-scoped configuration out of the Claude Code session.
    let workspace = tempfile::Builder::new()
        .prefix("hoi4-mod-setup-claude-")
        .tempdir()
        .map_err(|error| AppError::Process(format!("Claude workspace unavailable: {error}")))?;
    // A rejected proposal set gets one corrective turn; the deterministic
    // validator is never relaxed.
    let mut turn_prompt = prompt.clone();
    let mut attempt = 0;
    let analysis = loop {
        attempt += 1;
        let result = run_claude(
            &executable.path,
            &executable.sha256,
            args.clone(),
            Some(workspace.path().to_path_buf()),
            Some(turn_prompt.as_bytes()),
            MAX_ANALYSIS_BYTES,
            ANALYSIS_TIMEOUT_SECONDS,
            None,
        )?;
        if result.timed_out {
            return Err(AppError::Process("Claude Code analysis timed out".into()));
        }
        if result.stdout_truncated {
            return Err(AppError::Serialization(
                "Claude Code response exceeded the bounded response limit".into(),
            ));
        }
        let validated =
            extract_analysis_output(&redact_secrets(&result.stdout, &[])).and_then(|response| {
                validate_analysis_output(
                    response,
                    &request.analysis,
                    &input_sha256,
                    &request.analysis.evidence,
                )
            });
        match validated {
            Ok(analysis) => break analysis,
            Err(AppError::Serialization(reason)) if attempt < crate::codex::ANALYSIS_ATTEMPTS => {
                turn_prompt = format!(
                    "{prompt}

{}",
                    crate::codex::corrective_analysis_prompt(&reason, &input_sha256)
                );
            }
            Err(error) => return Err(error),
        }
    };
    let output_sha256 = crate::security::sha256_bytes(&serde_json::to_vec(&analysis)?);
    Ok(CodexAnalysisResult {
        analysis: analysis.clone(),
        record: crate::models::CodexAnalysisRecord {
            engine: ENGINE.into(),
            auth_mode: AUTH_MODE.into(),
            provider: Some(PROVIDER_ID.into()),
            model: Some(request.model.clone()),
            reasoning_effort: Some(request.reasoning_effort.clone()),
            optimization_profile: Some(optimization_profile.to_owned()),
            analysis_id: analysis.analysis_id,
            schema_version: analysis.schema_version,
            input_sha256,
            output_sha256,
            confirmed_fields: Vec::new(),
            confirmed_at: String::new(),
            account_identity_persisted: false,
            analysis_purpose: request.analysis.analysis_purpose.clone(),
            project_root: request.analysis.project_root.clone(),
            scan_id: request.analysis.scan_id,
            evidence_sha256: (!request.analysis.evidence.is_empty())
                .then(|| crate::codex::evidence_manifest_sha256(&request.analysis.evidence))
                .transpose()?,
            source_revision: None,
            source_manifest_sha256: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_summary_keeps_only_non_identifying_fields() {
        let summary = parse_sign_in_summary(
            r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","email":"person@example.com","orgName":"Example Org","subscriptionType":"max"}"#,
        )
        .unwrap();
        assert_eq!(
            summary,
            ClaudeSignInSummary {
                logged_in: true,
                auth_method: "claude.ai".into(),
                first_party: true,
            }
        );
        let status = status_from_summary(DEFAULT_MODEL, &summary);
        assert!(status.authenticated);
        let serialized = serde_json::to_string(&status).unwrap();
        assert!(!serialized.contains("example.com"));
        assert!(!serialized.contains("Example Org"));
        assert!(!serialized.contains("max"));
    }

    #[test]
    fn signed_out_and_third_party_sessions_are_not_authenticated() {
        let signed_out = parse_sign_in_summary(
            r#"{"loggedIn":false,"authMethod":"none","apiProvider":"firstParty"}"#,
        )
        .unwrap();
        let status = status_from_summary(DEFAULT_MODEL, &signed_out);
        assert!(status.available);
        assert!(!status.authenticated);
        assert_eq!(
            status.error.as_deref(),
            Some("Sign in to Claude to continue.")
        );

        let bedrock = parse_sign_in_summary(
            r#"{"loggedIn":true,"authMethod":"third_party","apiProvider":"bedrock"}"#,
        )
        .unwrap();
        assert!(!status_from_summary(DEFAULT_MODEL, &bedrock).authenticated);
        assert!(parse_sign_in_summary("not json").is_err());
        assert!(parse_sign_in_summary(r#"{"authMethod":"claude.ai"}"#).is_err());
    }

    #[test]
    fn hostile_auth_method_values_are_not_retained() {
        let summary = parse_sign_in_summary(
            r#"{"loggedIn":true,"authMethod":"person@example.com secret","apiProvider":"firstParty"}"#,
        )
        .unwrap();
        assert_eq!(summary.auth_method, "unknown");
    }

    #[test]
    fn analysis_arguments_disable_tools_settings_mcp_and_persistence() {
        let args = analysis_arguments(DEFAULT_MODEL, "high").unwrap();
        for flag in [
            "--print",
            "--strict-mcp-config",
            "--safe-mode",
            "--no-session-persistence",
        ] {
            assert!(args.iter().any(|arg| arg == flag), "missing {flag}");
        }
        let tools = args.iter().position(|arg| arg == "--tools").unwrap();
        assert_eq!(args[tools + 1], "");
        let model = args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(args[model + 1], DEFAULT_MODEL);
        let schema = args.iter().position(|arg| arg == "--json-schema").unwrap();
        let parsed: Value = serde_json::from_str(&args[schema + 1]).unwrap();
        assert!(parsed.get("properties").is_some());
        // Haiku 4.5 does not support effort, so it must not be forwarded.
        assert!(!args.iter().any(|arg| arg == "--effort"));
        assert!(!args
            .iter()
            .any(|arg| arg.contains("C:\\") || arg.starts_with('/')));
    }

    #[test]
    fn effort_is_forwarded_only_for_models_that_support_it() {
        let args = analysis_arguments("claude-sonnet-5-5", "max").unwrap();
        let effort = args.iter().position(|arg| arg == "--effort").unwrap();
        assert_eq!(args[effort + 1], "max");
        assert!(analysis_arguments(DEFAULT_MODEL, "ultra").is_err());
    }

    #[test]
    fn model_names_cannot_inject_arguments() {
        for model in [
            "",
            "--dangerously-skip-permissions",
            "haiku model",
            "a;b",
            "x\n",
        ] {
            assert!(validate_model(model).is_err(), "accepted {model:?}");
        }
        assert!(validate_model(DEFAULT_MODEL).is_ok());
    }

    #[test]
    fn print_results_map_to_sanitized_categories() {
        let structured = r#"{"type":"result","is_error":false,"result":"","structured_output":{"schema_version":"1.0.0"}}"#;
        assert_eq!(
            extract_analysis_output(structured).unwrap()["schema_version"],
            "1.0.0"
        );
        let text = r#"{"type":"result","is_error":false,"result":"```json\n{\"schema_version\":\"1.0.0\"}\n```"}"#;
        assert_eq!(
            extract_analysis_output(text).unwrap()["schema_version"],
            "1.0.0"
        );
        let signed_out =
            r#"{"type":"result","is_error":true,"result":"Not logged in · Please run /login"}"#;
        assert!(matches!(
            extract_analysis_output(signed_out),
            Err(AppError::Credential(message)) if message.contains("sign in")
        ));
        let limited = r#"{"type":"result","is_error":true,"result":"Claude usage limit reached for person@example.com"}"#;
        match extract_analysis_output(limited) {
            Err(AppError::Credential(message)) => {
                assert!(message.contains("usage is currently limited"));
                assert!(!message.contains("example.com"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            extract_analysis_output(r#"{"type":"result","is_error":false,"result":"plain text"}"#),
            Err(AppError::Serialization(_))
        ));
        assert!(matches!(
            extract_analysis_output("garbage"),
            Err(AppError::Protocol(_))
        ));
    }

    #[test]
    fn required_flag_probe_rejects_builds_without_isolation_flags() {
        let complete = REQUIRED_PRINT_FLAGS.join("\n  ");
        assert!(help_supports_required_flags(&complete));
        let without_safe_mode = REQUIRED_PRINT_FLAGS
            .iter()
            .filter(|flag| **flag != "--safe-mode")
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!help_supports_required_flags(&without_safe_mode));
        assert!(help_supports_required_flags(
            "-p, --print  Print response\n --output-format <format>\n --json-schema <schema>\n --model <model>\n --tools <tools...>\n --strict-mcp-config\n --safe-mode\n --no-session-persistence\n --system-prompt <prompt>"
        ));
    }

    #[test]
    fn launcher_candidates_include_path_and_native_installer_locations() {
        let home = std::env::temp_dir();
        let path = std::env::join_paths([home.join("bin-a")]).unwrap();
        let candidates = executable_candidates(Some(path.as_os_str()), Some(&home));
        let name = executable_names()[0];
        assert_eq!(candidates.first(), Some(&home.join("bin-a").join(name)));
        assert!(candidates.contains(&home.join(".local").join("bin").join(name)));
        let relative = std::env::join_paths(["relative-dir"]).unwrap();
        assert!(
            executable_candidates(Some(relative.as_os_str()), None).is_empty()
                || cfg!(target_os = "macos")
        );
    }

    #[test]
    fn passthrough_environment_never_forwards_override_credentials() {
        for name in PASSTHROUGH_ENVIRONMENT {
            assert!(!name.starts_with("ANTHROPIC_"), "{name}");
            assert!(!name.starts_with("CLAUDE_CODE_"), "{name}");
            assert!(!name.contains("TOKEN") && !name.contains("KEY"), "{name}");
        }
    }

    /// Opt-in probe of the user's real Claude Code. It verifies discovery,
    /// the signature, the isolation-flag probe, and signed-out or signed-in
    /// status; with `HOI4_CLAUDE_LIVE_LOGIN=1` it also starts and cancels a
    /// real sign-in, and when signed in it runs one real Haiku analysis turn.
    #[test]
    #[ignore = "requires the user's installed Claude Code; run with pnpm test:claude-live"]
    fn live_claude_code_route() {
        let executable = find_executable().expect("official Claude Code was not found");
        assert!(
            executable.supports_required_flags,
            "Claude Code lacks a required isolation flag"
        );
        let status = account_status(DEFAULT_MODEL);
        eprintln!(
            "claude_code available={} authenticated={} error={:?}",
            status.available, status.authenticated, status.error
        );
        assert!(status.available, "{:?}", status.error);
        if std::env::var("HOI4_CLAUDE_LIVE_LOGIN").as_deref() == Ok("1") {
            // The stop clock starts at the first poll after spawn, so the
            // pre-spawn identity hash does not count toward cancellation time.
            let mut first_poll: Option<std::time::Instant> = None;
            let mut stopped_at: Option<std::time::Instant> = None;
            let mut stop = || {
                let first = *first_poll.get_or_insert_with(std::time::Instant::now);
                let stop = first.elapsed() > std::time::Duration::from_secs(4);
                if stop && stopped_at.is_none() {
                    stopped_at = Some(std::time::Instant::now());
                }
                stop
            };
            let result = run_login(&mut stop);
            let returned = std::time::Instant::now();
            assert!(
                matches!(&result, Err(AppError::Process(message)) if message.contains("cancelled")),
                "unexpected sign-in result: {result:?}"
            );
            let cancel_latency =
                returned.duration_since(stopped_at.expect("cancellation was never requested"));
            eprintln!("claude_code sign-in cancellation completed in {cancel_latency:?}");
            assert!(cancel_latency < std::time::Duration::from_secs(10));
            eprintln!("claude_code sign-in start and cancellation passed");
        }
        if !status.authenticated {
            eprintln!("claude_code is signed out; the live analysis turn was skipped");
            return;
        }
        let request = AiAnalysisRequest {
            provider: PROVIDER_ID.into(),
            model: DEFAULT_MODEL.into(),
            reasoning_effort: "high".into(),
            endpoint: String::new(),
            analysis: crate::codex::CodexAnalysisRequest {
                mode: "new_project_identity".into(),
                brief: "Iron Dawn: an alternate-history mod about a surviving Austro-Hungarian federation with new focus trees and events.".into(),
                evidence: Vec::new(),
                constraints: serde_json::json!({}),
                analysis_purpose: None,
                project_root: None,
                scan_id: None,
            },
        };
        let result = analyze(&request, "Claude account setup analysis")
            .expect("live Claude analysis failed");
        assert_eq!(result.record.engine, ENGINE);
        assert_eq!(result.record.auth_mode, AUTH_MODE);
        assert!(!result.record.account_identity_persisted);
        eprintln!(
            "claude_code analysis returned {} proposals",
            result.analysis.proposals.len()
        );
    }

    #[test]
    fn builtin_model_is_haiku_4_5_and_not_labelled_live() {
        let models = builtin_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, DEFAULT_MODEL);
        assert_eq!(models[0].supported_reasoning_efforts, vec!["high"]);
    }
}
