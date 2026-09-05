//! ChatGPT-authenticated Codex App Server integration.
//!
//! This module owns only the local JSONL process boundary and schema
//! validation. Codex owns authentication and token persistence. No method in
//! this module reads a Codex token file or accepts an API key.

use crate::models::CodexAnalysisRecord;
use crate::security::{is_link_metadata, redact_secrets, reject_secret_like_keys, sha256_bytes};
use crate::AppError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use uuid::Uuid;

#[cfg(target_os = "windows")]
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStringExt;
#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED};

pub const CODEX_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Clone)]
struct CachedCodexExecutable {
    path: PathBuf,
    sha256: String,
}

static CODEX_EXECUTABLE: OnceLock<Mutex<Option<CachedCodexExecutable>>> = OnceLock::new();
pub const REQUIRED_ANALYSIS_PROPOSAL_KEYS: &[&str] = &[
    "display_name",
    "project_id",
    "script_prefix",
    "primary_namespace",
    "project_description",
    "descriptor_tags",
    "folder_profile",
    "agents_profile",
    "localisation_convention",
    "documentation_convention",
];
const MAX_JSONL_LINE_BYTES: usize = 2 * 1024 * 1024;
const MAX_BUFFERED_NOTIFICATIONS: usize = 128;
const MAX_BUFFERED_NOTIFICATION_BYTES: usize = 2 * 1024 * 1024;
const MAX_CORRELATED_NOTIFICATIONS: usize = 256;
const MAX_CORRELATED_NOTIFICATION_BYTES: usize = 4 * 1024 * 1024;
const MAX_PROTOCOL_ERROR_CODE_CHARS: usize = 64;
const MAX_PROTOCOL_ERROR_DETAIL_CHARS: usize = 512;
const MAX_BRIEF_BYTES: usize = 32 * 1024;
const MAX_EVIDENCE_ITEMS: usize = 256;
const MAX_EVIDENCE_BYTES: usize = 512 * 1024;
const APP_SERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodexAccountStatus {
    pub available: bool,
    pub authenticated: bool,
    pub auth_mode: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan_type: Option<String>,
    pub usage_limited: bool,
    #[serde(default)]
    pub app_server_version: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiAccountStatus {
    pub available: bool,
    pub authenticated: bool,
    pub provider: String,
    pub model: String,
    pub auth_mode: String,
    pub usage_limited: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodexLoginStart {
    pub available: bool,
    #[serde(default)]
    pub login_id: Option<String>,
    #[serde(default)]
    pub auth_url: Option<String>,
    #[serde(default)]
    pub verification_url: Option<String>,
    #[serde(default)]
    pub user_code: Option<String>,
    pub device_code: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedEvidence {
    pub reference: String,
    pub path: String,
    #[serde(default)]
    pub excerpt: String,
    pub excerpt_sha256: String,
    #[serde(default)]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAnalysisRequest {
    pub mode: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub evidence: Vec<ApprovedEvidence>,
    #[serde(default)]
    pub constraints: Value,
    #[serde(default)]
    pub analysis_purpose: Option<String>,
    #[serde(default)]
    pub project_root: Option<String>,
    #[serde(default)]
    pub scan_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAnalysisRequest {
    pub provider: String,
    pub model: String,
    pub reasoning_effort: String,
    pub endpoint: String,
    #[serde(flatten)]
    pub analysis: CodexAnalysisRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAnalysisResult {
    pub analysis: CodexAnalysis,
    pub record: CodexAnalysisRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexAnalysis {
    pub schema_version: String,
    pub analysis_id: Uuid,
    pub mode: AnalysisMode,
    pub input_sha256: String,
    pub project_summary: String,
    pub proposals: Vec<CodexProposal>,
    pub component_recommendations: Vec<ComponentRecommendation>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    NewProjectIdentity,
    ExistingProjectSemantics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexProposal {
    pub key: ProposalKey,
    pub value: Value,
    pub confidence: f64,
    pub reason: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKey {
    DisplayName,
    ProjectId,
    ScriptPrefix,
    PrimaryNamespace,
    ProjectDescription,
    DescriptorTags,
    FolderProfile,
    AgentsProfile,
    LocalisationConvention,
    DocumentationConvention,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentRecommendation {
    pub component_id: String,
    pub recommendation: Recommendation,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    Required,
    Recommended,
    NotRecommended,
}

/// A small transport trait lets protocol and failure tests run without a
/// real Codex installation. The production implementation is the official
/// `codex app-server --stdio` child process below.
pub trait JsonlTransport {
    fn send(&mut self, value: &Value) -> Result<(), AppError>;
    fn receive(&mut self, timeout: Duration) -> Result<Option<Value>, AppError>;
    fn close(&mut self);
    /// Return whether the supervised child transport can still accept a
    /// request. Protocol fakes remain alive by default; the process adapter
    /// polls the actual child without reading any credential state.
    fn is_alive(&mut self) -> bool {
        true
    }
}

pub struct AppServerProtocol<T: JsonlTransport> {
    transport: T,
    next_id: u64,
    notifications: Vec<Value>,
    request_timeout: Duration,
    initialized: bool,
}

impl<T: JsonlTransport> AppServerProtocol<T> {
    pub fn new(transport: T) -> Self {
        Self::with_timeout(transport, APP_SERVER_REQUEST_TIMEOUT)
    }

    pub fn with_timeout(transport: T, request_timeout: Duration) -> Self {
        Self {
            transport,
            next_id: 1,
            notifications: Vec::new(),
            request_timeout: request_timeout.max(Duration::from_millis(100)),
            initialized: false,
        }
    }

    pub fn is_alive(&mut self) -> bool {
        self.transport.is_alive()
    }

    pub fn initialize(&mut self) -> Result<Value, AppError> {
        if self.initialized {
            return Err(AppError::Process(
                "Codex App Server protocol is already initialized".into(),
            ));
        }
        let result = self.request_unchecked(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "hoi4-mod-setup",
                    "title": "HOI4 Mod Setup",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;
        self.transport.send(&json!({"method": "initialized"}))?;
        self.initialized = true;
        Ok(result)
    }

    pub fn account_read(&mut self, refresh_token: bool) -> Result<CodexAccountStatus, AppError> {
        let result = self.request("account/read", json!({"refreshToken": refresh_token}))?;
        let mut status = parse_account_status(&result);
        if status.authenticated && status.auth_mode == "chatgpt" {
            match self.request("account/rateLimits/read", json!({})) {
                Ok(rate_limits) => {
                    status.usage_limited |= rate_limits_limited(&rate_limits);
                }
                Err(_) => {
                    return Err(AppError::Process(
                        "Codex usage state could not be checked".into(),
                    ));
                }
            }
        }
        Ok(status)
    }

    pub fn login_start(&mut self, device_code: bool) -> Result<CodexLoginStart, AppError> {
        let params = if device_code {
            json!({"type": "chatgptDeviceCode"})
        } else {
            // Managed ChatGPT authentication is owned by Codex. Keep this at
            // the version-compatible required request shape; optional
            // branding/login-page extensions vary across App Server schemas.
            json!({"type": "chatgpt"})
        };
        let result = self.request("account/login/start", params)?;
        Ok(parse_login_start(&result, device_code))
    }

    pub fn cancel_login(&mut self, login_id: &str) -> Result<(), AppError> {
        self.ensure_initialized()?;
        validate_login_id(login_id)?;
        self.request_with_timeout(
            "account/login/cancel",
            json!({"loginId": login_id}),
            Duration::from_secs(5),
        )?;
        Ok(())
    }

    /// Wait for the bounded login-completion and account-update notification
    /// pair described by the App Server protocol, then re-read the transient
    /// account status. Tokens and account identifiers never leave Codex.
    pub fn wait_for_login(
        &mut self,
        login_id: &str,
        timeout: Duration,
    ) -> Result<CodexAccountStatus, AppError> {
        self.wait_for_login_with_cancel(login_id, timeout, || false)
    }

    pub fn wait_for_login_with_cancel(
        &mut self,
        login_id: &str,
        timeout: Duration,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<CodexAccountStatus, AppError> {
        validate_login_id(login_id)?;
        let started = std::time::Instant::now();
        let mut login_completed = false;
        let mut account_updated = false;
        let mut pending = std::mem::take(&mut self.notifications);
        loop {
            if is_cancelled() {
                self.cancel_login(login_id).map_err(|error| {
                    AppError::Credential(format!(
                        "Codex login was cancelled locally, but App Server cancellation failed: {}",
                        redact_secrets(&error.to_string(), &[])
                    ))
                })?;
                return Err(AppError::Credential("Codex login was cancelled".into()));
            }
            for message in pending.drain(..) {
                match account_login_notification(&message, login_id)? {
                    LoginNotification::Completed => login_completed = true,
                    LoginNotification::Updated => account_updated = true,
                    LoginNotification::Failed(error) => {
                        return Err(AppError::Credential(error));
                    }
                    LoginNotification::Other => {}
                }
            }
            if login_completed && account_updated {
                return self.account_read(false);
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                self.cancel_login(login_id).map_err(|error| {
                    AppError::Credential(format!(
                        "Codex login timed out, but App Server cancellation failed: {}",
                        redact_secrets(&error.to_string(), &[])
                    ))
                })?;
                return Err(AppError::Credential(
                    "Codex login did not complete during the bounded wait and was cancelled".into(),
                ));
            }
            match self
                .transport
                .receive(remaining.min(Duration::from_millis(250)))
            {
                Ok(Some(message)) => pending.push(message),
                Ok(None) => {
                    return Err(AppError::Process(
                        "Codex App Server closed during login".into(),
                    ));
                }
                Err(AppError::Process(message)) if message.contains("timed out") => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub fn logout(&mut self) -> Result<(), AppError> {
        let _ = self.request("account/logout", json!({}))?;
        Ok(())
    }

    pub(crate) fn request(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        self.ensure_initialized()?;
        self.request_unchecked(method, params)
    }

    fn ensure_initialized(&self) -> Result<(), AppError> {
        if self.initialized {
            Ok(())
        } else {
            Err(AppError::Process(
                "Codex App Server must be initialized before use".into(),
            ))
        }
    }

    fn request_unchecked(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        self.request_with_timeout(method, params, self.request_timeout)
    }

    fn request_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, AppError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.transport
            .send(&json!({"id": id, "method": method, "params": params}))?;
        let started = std::time::Instant::now();
        loop {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(AppError::Process(format!(
                    "Codex App Server {method} timed out"
                )));
            }
            let message = self.transport.receive(remaining)?.ok_or_else(|| {
                AppError::Process(format!("Codex App Server closed during {method}"))
            })?;
            if message.get("method").is_some() && message.get("id").is_none() {
                push_bounded_notification(
                    &mut self.notifications,
                    message,
                    MAX_BUFFERED_NOTIFICATIONS,
                    MAX_BUFFERED_NOTIFICATION_BYTES,
                    "buffered",
                )?;
                continue;
            }
            if message.get("id") != Some(&json!(id)) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let code = safe_protocol_error_code(error.get("code"));
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(|message| {
                        bounded_redacted_protocol_text(message, MAX_PROTOCOL_ERROR_DETAIL_CHARS)
                    })
                    .filter(|message| !message.trim().is_empty());
                return Err(AppError::Process(format!(
                    "Codex App Server {method} failed ({code}){}",
                    detail
                        .map(|message| format!(": {message}"))
                        .unwrap_or_default()
                )));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    pub fn analyze(
        &mut self,
        request: &CodexAnalysisRequest,
        model: &str,
        reasoning_effort: &str,
    ) -> Result<CodexAnalysisResult, AppError> {
        validate_analysis_request(request)?;
        crate::ai::validate_reasoning_effort(reasoning_effort)?;
        if model.trim().is_empty() || model.len() > 256 || model.chars().any(char::is_control) {
            return Err(AppError::InvalidInput("Codex model is invalid".into()));
        }
        let account = self.account_read(false)?;
        if let Some(error) = account.error.as_deref() {
            return Err(AppError::Credential(error.into()));
        }
        if !account.authenticated || account.auth_mode != "chatgpt" {
            return Err(AppError::Credential(
                "ChatGPT sign-in through Codex is required before semantic analysis".into(),
            ));
        }
        if account.usage_limited {
            return Err(AppError::Credential(
                "Codex usage is currently limited; retry after the account becomes available"
                    .into(),
            ));
        }
        let analysis_directory = AnalysisWorkingDirectory::create()?;
        let thread = self.request(
            "thread/start",
            json!({
                "approvalPolicy": "never",
                "sandbox": "read-only",
                "cwd": analysis_directory.path_string()?,
                "model": model
            }),
        )?;
        let thread_id = thread_id(&thread)?;
        let input_sha256 = analysis_input_sha256(request)?;
        let prompt = analysis_prompt_for_provider(request, &input_sha256, "Codex setup analysis")?;
        let schema: Value = serde_json::from_slice(include_bytes!(
            "../../docs/schemas/codex-analysis.schema.json"
        ))?;
        let turn = self.request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "input": [{"type": "text", "text": prompt}],
                "outputSchema": schema,
                "sandboxPolicy": read_only_no_project_access(),
                "approvalPolicy": "never",
                "model": model,
                "reasoningEffort": reasoning_effort
            }),
        )?;
        let turn_id = turn_id_from_start_response(&turn);
        let mut messages = Vec::new();
        messages.extend(self.drain_notifications(
            Duration::from_secs(120),
            &thread_id,
            turn_id.as_deref(),
        )?);
        let turn_completed = messages
            .iter()
            .any(|message| event_completes_turn(message, &thread_id, turn_id.as_deref()));
        if let Some(error) = messages
            .iter()
            .find_map(|message| completed_turn_error(message, &thread_id, turn_id.as_deref()))
        {
            return Err(AppError::Process(format!(
                "Codex planning turn failed: {error}"
            )));
        }
        let output = turn_completed
            .then(|| messages.iter().rev().find_map(structured_output))
            .flatten()
            .ok_or_else(|| {
                AppError::Serialization(
                    "Codex returned no schema-constrained analysis output".into(),
                )
            })?;
        let analysis = validate_analysis_output(output, request, &input_sha256, &request.evidence)?;
        let output_bytes = serde_json::to_vec(&analysis)?;
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: Some(model.to_owned()),
            reasoning_effort: Some(reasoning_effort.to_owned()),
            optimization_profile: Some("Codex setup analysis".into()),
            analysis_id: analysis.analysis_id,
            schema_version: analysis.schema_version.clone(),
            input_sha256,
            output_sha256: sha256_bytes(&output_bytes),
            confirmed_fields: Vec::new(),
            confirmed_at: Utc::now().to_rfc3339(),
            account_identity_persisted: false,
            analysis_purpose: request.analysis_purpose.clone(),
            project_root: request.project_root.clone(),
            scan_id: request.scan_id,
            evidence_sha256: (!request.evidence.is_empty())
                .then(|| evidence_manifest_sha256(&request.evidence))
                .transpose()?,
            source_revision: None,
            source_manifest_sha256: None,
        };
        Ok(CodexAnalysisResult { analysis, record })
    }

    pub fn model_list(&mut self) -> Result<Vec<crate::models::AiModelOption>, AppError> {
        let mut cursor: Option<String> = None;
        let mut result = Vec::new();
        loop {
            let page = self.request("model/list", json!({"cursor": cursor, "limit": 100}))?;
            let data = page
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| AppError::Serialization("Codex model list omitted data".into()))?;
            for item in data {
                if item.get("hidden").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                let id = item
                    .get("model")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        AppError::Serialization("Codex model entry omitted its ID".into())
                    })?;
                let efforts = item
                    .get("supportedReasoningEfforts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.get("reasoningEffort").and_then(Value::as_str))
                    .filter(|effort| matches!(*effort, "low" | "medium" | "high" | "xhigh" | "max"))
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                let default_effort = item
                    .get("defaultReasoningEffort")
                    .and_then(Value::as_str)
                    .filter(|effort| efforts.iter().any(|candidate| candidate == effort))
                    .unwrap_or_else(|| efforts.first().map(String::as_str).unwrap_or("high"));
                result.push(crate::models::AiModelOption {
                    id: id.to_owned(),
                    display_name: item
                        .get("displayName")
                        .and_then(Value::as_str)
                        .unwrap_or(id)
                        .to_owned(),
                    default_reasoning_effort: default_effort.to_owned(),
                    supported_reasoning_efforts: if efforts.is_empty() {
                        vec!["high".into()]
                    } else {
                        efforts
                    },
                });
            }
            cursor = page
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            if cursor.is_none() || result.len() >= 500 {
                break;
            }
        }
        Ok(result)
    }

    fn drain_notifications(
        &mut self,
        max_wait: Duration,
        thread_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Vec<Value>, AppError> {
        let pending = std::mem::take(&mut self.notifications);
        let mut messages = Vec::new();
        for message in pending {
            if event_matches_turn(&message, thread_id, turn_id) {
                push_bounded_notification(
                    &mut messages,
                    message,
                    MAX_CORRELATED_NOTIFICATIONS,
                    MAX_CORRELATED_NOTIFICATION_BYTES,
                    "correlated",
                )?;
            }
        }
        let started = std::time::Instant::now();
        while started.elapsed() < max_wait {
            match self.transport.receive(Duration::from_millis(200)) {
                Ok(Some(message)) => {
                    let correlated = event_matches_turn(&message, thread_id, turn_id);
                    let complete = event_completes_turn(&message, thread_id, turn_id);
                    if correlated {
                        push_bounded_notification(
                            &mut messages,
                            message,
                            MAX_CORRELATED_NOTIFICATIONS,
                            MAX_CORRELATED_NOTIFICATION_BYTES,
                            "correlated",
                        )?;
                    }
                    if complete {
                        break;
                    }
                }
                Ok(None) => break,
                Err(AppError::Process(message)) if message.contains("timed out") => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(messages)
    }
}

fn bounded_redacted_protocol_text(value: &str, max_chars: usize) -> String {
    redact_secrets(value, &[])
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

fn safe_login_error(value: &str) -> String {
    let bounded = bounded_redacted_protocol_text(value, MAX_PROTOCOL_ERROR_DETAIL_CHARS);
    if bounded.trim().is_empty() || validate_output_text(&bounded, "Codex login error").is_err() {
        "Codex login failed".into()
    } else {
        bounded
    }
}

fn safe_protocol_error_code(value: Option<&Value>) -> String {
    let raw = match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        _ => "request_failed".into(),
    };
    let bounded = bounded_redacted_protocol_text(&raw, MAX_PROTOCOL_ERROR_CODE_CHARS);
    if bounded.trim().is_empty() {
        "request_failed".into()
    } else {
        bounded
    }
}

fn push_bounded_notification(
    notifications: &mut Vec<Value>,
    message: Value,
    max_count: usize,
    max_bytes: usize,
    kind: &str,
) -> Result<(), AppError> {
    if notifications.len() >= max_count {
        return Err(AppError::Process(format!(
            "Codex App Server exceeded the {kind} notification count limit"
        )));
    }
    let current_bytes = notifications.iter().try_fold(0_usize, |total, value| {
        serde_json::to_vec(value)
            .map(|bytes| total.saturating_add(bytes.len()))
            .map_err(AppError::from)
    })?;
    let message_bytes = serde_json::to_vec(&message)?.len();
    if current_bytes.saturating_add(message_bytes) > max_bytes {
        return Err(AppError::Process(format!(
            "Codex App Server exceeded the {kind} notification size limit"
        )));
    }
    notifications.push(message);
    Ok(())
}

fn turn_id_from_start_response(value: &Value) -> Option<String> {
    value
        .get("turnId")
        .or_else(|| value.get("turn_id"))
        .or_else(|| value.get("turn").and_then(|turn| turn.get("id")))
        .or_else(|| {
            value
                .get("result")
                .and_then(|result| result.get("turn"))
                .and_then(|turn| turn.get("id"))
        })
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// Extract only thread/turn identity fields from protocol envelopes. Do not
/// recursively treat every nested `id` as a turn ID: item IDs are unrelated
/// and would make a valid turn appear to belong to another turn.
fn event_turn_references(value: &Value) -> (Vec<String>, Vec<String>) {
    fn visit(value: &Value, threads: &mut Vec<String>, turns: &mut Vec<String>) {
        let Value::Object(object) = value else {
            return;
        };
        for key in ["threadId", "thread_id"] {
            if let Some(id) = object.get(key).and_then(Value::as_str) {
                threads.push(id.to_owned());
            }
        }
        for key in ["turnId", "turn_id"] {
            if let Some(id) = object.get(key).and_then(Value::as_str) {
                turns.push(id.to_owned());
            }
        }
        if let Some(turn) = object.get("turn") {
            if let Some(id) = turn.get("id").and_then(Value::as_str) {
                turns.push(id.to_owned());
            }
            visit(turn, threads, turns);
        }
        for key in ["params", "result", "event", "item"] {
            if let Some(child) = object.get(key) {
                visit(child, threads, turns);
            }
        }
    }

    let mut threads = Vec::new();
    let mut turns = Vec::new();
    visit(value, &mut threads, &mut turns);
    (threads, turns)
}

fn event_matches_turn(value: &Value, thread_id: &str, turn_id: Option<&str>) -> bool {
    let (thread_ids, turn_ids) = event_turn_references(value);
    if thread_ids.is_empty() || thread_ids.iter().any(|id| id != thread_id) {
        return false;
    }
    if let Some(turn_id) = turn_id {
        if turn_ids.iter().any(|id| id != turn_id) {
            return false;
        }
    }
    true
}

fn event_completes_turn(value: &Value, thread_id: &str, turn_id: Option<&str>) -> bool {
    if !event_matches_turn(value, thread_id, turn_id) {
        return false;
    }
    if value.get("method").and_then(Value::as_str) != Some("turn/completed") {
        return false;
    }
    let Some(turn_id) = turn_id else {
        return true;
    };
    let (_, turn_ids) = event_turn_references(value);
    turn_ids.iter().any(|id| id == turn_id)
}

fn completed_turn_error(value: &Value, thread_id: &str, turn_id: Option<&str>) -> Option<String> {
    if !event_completes_turn(value, thread_id, turn_id) {
        return None;
    }
    let turn = value
        .get("params")
        .and_then(|params| params.get("turn"))
        .or_else(|| value.get("turn"))?;
    if turn.get("status").and_then(Value::as_str) != Some("failed") {
        return None;
    }
    let raw = turn
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("Codex did not return a completed analysis");
    let nested = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| deepest_error_message(&value));
    let message = nested.unwrap_or_else(|| raw.to_owned());
    Some(redact_secrets(
        &message
            .chars()
            .filter(|character| !character.is_control())
            .take(512)
            .collect::<String>(),
        &[],
    ))
}

fn deepest_error_message(value: &Value) -> Option<String> {
    if let Some(error) = value.get("error") {
        if let Some(message) = deepest_error_message(error) {
            return Some(message);
        }
    }
    value
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn validate_analysis_request(request: &CodexAnalysisRequest) -> Result<(), AppError> {
    if !matches!(
        request.mode.as_str(),
        "new_project_identity" | "existing_project_semantics"
    ) {
        return Err(AppError::InvalidInput(
            "unsupported Codex analysis mode".into(),
        ));
    }
    if request.mode == "existing_project_semantics" && request.evidence.is_empty() {
        return Err(AppError::InvalidInput(
            "existing-project Codex analysis requires approved scan evidence".into(),
        ));
    }
    if let Some(purpose) = request.analysis_purpose.as_deref() {
        if !matches!(
            purpose,
            "existing_project_import" | "maintenance_reanalysis"
        ) {
            return Err(AppError::InvalidInput(
                "unsupported Codex analysis purpose".into(),
            ));
        }
        if matches!(
            purpose,
            "existing_project_import" | "maintenance_reanalysis"
        ) && (request.mode != "existing_project_semantics"
            || request.evidence.is_empty()
            || request.project_root.is_none()
            || request.scan_id.is_none())
        {
            return Err(AppError::InvalidInput(
                "existing-project analysis requires a project root, scan ID, and approved evidence"
                    .into(),
            ));
        }
    }
    if request.brief.len() > MAX_BRIEF_BYTES || request.brief.contains('\0') {
        return Err(AppError::InvalidInput(
            "Codex brief exceeds the bounded input limit".into(),
        ));
    }
    if redact_secrets(&request.brief, &[]) != request.brief {
        return Err(AppError::Credential(
            "Codex brief contains credential-shaped content".into(),
        ));
    }
    reject_secret_like_keys(&request.constraints)?;
    let constraints_text = serde_json::to_string(&request.constraints)?;
    if redact_secrets(&constraints_text, &[]) != constraints_text {
        return Err(AppError::Credential(
            "Codex constraints contain credential-shaped content".into(),
        ));
    }
    if constraints_text.len() > MAX_EVIDENCE_BYTES {
        return Err(AppError::InvalidInput(
            "Codex constraints exceed the bounded input limit".into(),
        ));
    }
    if request.evidence.len() > MAX_EVIDENCE_ITEMS {
        return Err(AppError::InvalidInput(
            "too many approved Codex evidence items".into(),
        ));
    }
    let mut total = 0usize;
    let mut references = BTreeSet::new();
    for evidence in &request.evidence {
        if evidence.reference.trim().is_empty()
            || evidence.path.trim().is_empty()
            || evidence.path.contains('\0')
            || evidence.reference.len() > 256
            || evidence.path.len() > 512
            || crate::security::normalize_relative_path(&evidence.path).is_err()
            || forbidden_evidence_path(&evidence.path)
            || forbidden_evidence_path(&evidence.reference)
            || !references.insert(evidence.reference.as_str())
        {
            return Err(AppError::PathSecurity(
                "Codex evidence path is not project-relative".into(),
            ));
        }
        if evidence.excerpt.len() > MAX_BRIEF_BYTES
            || !is_sha256(&evidence.excerpt_sha256)
            || sha256_bytes(evidence.excerpt.as_bytes()) != evidence.excerpt_sha256
        {
            return Err(AppError::InvalidInput("Codex evidence is malformed".into()));
        }
        if redact_secrets(&evidence.excerpt, &[]) != evidence.excerpt {
            return Err(AppError::Credential(
                "Codex evidence contains credential-shaped content".into(),
            ));
        }
        total = total.saturating_add(evidence.excerpt.len());
    }
    if total > MAX_EVIDENCE_BYTES {
        return Err(AppError::InvalidInput(
            "Codex evidence exceeds the bounded input limit".into(),
        ));
    }
    Ok(())
}

fn forbidden_evidence_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let segments = normalized.split('/');
    if segments.clone().any(|segment| {
        segment == ".git"
            || segment == "auth"
            || segment == "authentication"
            || segment == "credential"
            || segment == "credentials"
            || segment == "secret"
            || segment == "secrets"
            || segment == "token"
            || segment == "tokens"
            || segment == "password"
            || segment == "passwords"
            || segment == "private"
            || segment == "private_keys"
    }) {
        return true;
    }
    let file_name = normalized.rsplit('/').next().unwrap_or_default();
    let name_tokens = file_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty());
    file_name == ".env"
        || file_name.starts_with(".env.")
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
        || file_name.ends_with(".p12")
        || file_name.ends_with(".pfx")
        || name_tokens.clone().any(|token| {
            matches!(
                token,
                "auth"
                    | "authentication"
                    | "credential"
                    | "credentials"
                    | "secret"
                    | "secrets"
                    | "token"
                    | "tokens"
                    | "password"
                    | "passwords"
                    | "passwd"
                    | "apikey"
                    | "key"
                    | "access"
                    | "rsa"
                    | "certificate"
                    | "cert"
                    | "private"
                    | "privatekey"
                    | "private_key"
                    | "id_rsa"
            )
        })
}

fn read_only_no_project_access() -> Value {
    json!({
        "type": "readOnly",
        "networkAccess": false
    })
}

struct AnalysisWorkingDirectory {
    path: PathBuf,
}

impl AnalysisWorkingDirectory {
    fn create() -> Result<Self, AppError> {
        let base = fs::canonicalize(std::env::temp_dir()).map_err(|error| {
            AppError::PathSecurity(format!(
                "Codex analysis temporary directory is unavailable: {error}"
            ))
        })?;
        if crate::security::path_has_link_component(&base) {
            return Err(AppError::PathSecurity(
                "Codex analysis temporary directory contains a symlink or junction".into(),
            ));
        }
        let path = base.join(format!("hoi4-mod-setup-analysis-{}", Uuid::new_v4()));
        fs::create_dir(&path).map_err(|error| {
            AppError::Process(format!(
                "could not create the Codex analysis directory: {error}"
            ))
        })?;
        let canonical = fs::canonicalize(&path).map_err(|error| {
            AppError::PathSecurity(format!(
                "Codex analysis directory could not be resolved: {error}"
            ))
        })?;
        if !canonical.starts_with(&base) || crate::security::path_has_link_component(&canonical) {
            let _ = fs::remove_dir(&path);
            return Err(AppError::PathSecurity(
                "Codex analysis directory escaped the operating-system temporary directory".into(),
            ));
        }
        Ok(Self { path: canonical })
    }

    fn path_string(&self) -> Result<String, AppError> {
        self.path.to_str().map(ToOwned::to_owned).ok_or_else(|| {
            AppError::PathSecurity(
                "Codex analysis directory is not representable as Unicode".into(),
            )
        })
    }
}

impl Drop for AnalysisWorkingDirectory {
    fn drop(&mut self) {
        // The directory is unique, app-created, and expected to remain empty
        // because the turn is read-only. Refuse recursive deletion if that
        // invariant is ever violated.
        let _ = fs::remove_dir(&self.path);
    }
}

fn model_visible_analysis_input(request: &CodexAnalysisRequest) -> Value {
    json!({
        "mode": &request.mode,
        "brief": &request.brief,
        "evidence": &request.evidence,
        "constraints": &request.constraints,
        "analysis_purpose": &request.analysis_purpose,
    })
}

pub fn analysis_input_sha256(request: &CodexAnalysisRequest) -> Result<String, AppError> {
    validate_analysis_request(request)?;
    Ok(sha256_bytes(&serde_json::to_vec(
        &model_visible_analysis_input(request),
    )?))
}

/// Validate evidence bytes before the user-approved evidence set is added to
/// the core-owned scan binding. This keeps the bridge from authorizing paths,
/// secrets, oversized excerpts, or stale hashes by accident.
pub fn validate_analysis_evidence(evidence: &[ApprovedEvidence]) -> Result<(), AppError> {
    validate_analysis_request(&CodexAnalysisRequest {
        mode: "existing_project_semantics".into(),
        brief: String::new(),
        evidence: evidence.to_vec(),
        constraints: json!({"platform": "windows_or_macos"}),
        analysis_purpose: None,
        project_root: None,
        scan_id: None,
    })
}

pub fn evidence_manifest_sha256(evidence: &[ApprovedEvidence]) -> Result<String, AppError> {
    Ok(sha256_bytes(&serde_json::to_vec(evidence)?))
}

/// Fuzz and integration entry point for untrusted app-server JSON. It never
/// starts a process or accesses the filesystem; callers provide only the
/// returned JSON payload.
pub fn validate_analysis_payload(bytes: &[u8]) -> Result<(), AppError> {
    let value: Value = serde_json::from_slice(bytes)?;
    let requested_mode = value
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let input_sha256 = value
        .get("input_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let request = CodexAnalysisRequest {
        mode: requested_mode,
        brief: String::new(),
        evidence: Vec::new(),
        constraints: json!({}),
        analysis_purpose: None,
        project_root: None,
        scan_id: None,
    };
    validate_analysis_output(value, &request, &input_sha256, &[]).map(|_| ())
}

pub(crate) fn analysis_prompt_for_provider(
    request: &CodexAnalysisRequest,
    input_sha256: &str,
    optimization_profile: &str,
) -> Result<String, AppError> {
    let input = serde_json::to_string(&model_visible_analysis_input(request))?;
    Ok(format!(
        "Interpret this approved HOI4 setup input using the {optimization_profile} conventions. Return only an object matching the supplied output schema. Set analysis_id to a fresh RFC 4122 UUID. Return every required proposal key exactly once. descriptor_tags and folder_profile proposal values must be JSON arrays of strings; every other proposal value must be a string. Descriptor tags may use only these official categories: Alternative History, Balance, Events, Fixes, Gameplay, Graphics, Historical, Ideologies, Map, Military, National Focuses, Sound, Technologies, Translation, Utilities. Propose concise, user-facing reasons and evidence_refs. Evidence refs must use only the supplied approved reference IDs; never invent paths or references. Keep project_summary, proposal values, reasons, and warnings focused on the mod; never attribute them to the setup assistant, provider, model, or analysis process, and never mention schemas, constraints, evidence fields, operating systems, platforms, or Workshop ID rules. Return warnings only when the user must make a decision or correct something. Do not read files, perform filesystem writes, execute commands, make network actions, or disclose account data. Input SHA-256 must be copied exactly.\n\ninput_sha256={input_sha256}\ninput={input}"
    ))
}

const RESERVED_PROJECT_FOLDER_ROOTS: &[&str] = &[
    ".agents",
    ".codex",
    ".git",
    ".hoi4-mod-setup",
    "chatgpt_project_sources",
    "paradox_wiki",
];

/// Normalize selected starter folders and keep them outside application-
/// managed roots. The same validator is used for provider proposals and the
/// final renderer state so a later edit cannot bypass this boundary.
pub fn validate_folder_profile_paths(folders: &[String]) -> Result<Vec<String>, AppError> {
    let mut normalized = BTreeSet::new();
    if folders.len() > 32 {
        return Err(AppError::InvalidInput(
            "folder profile contains too many paths".into(),
        ));
    }
    for folder in folders {
        let folder = crate::security::normalize_relative_path(folder).map_err(|error| {
            AppError::InvalidInput(format!("folder profile contains an unsafe path: {error}"))
        })?;
        let root = folder.split('/').next().unwrap_or_default();
        if RESERVED_PROJECT_FOLDER_ROOTS
            .iter()
            .any(|reserved| root.eq_ignore_ascii_case(reserved))
        {
            return Err(AppError::InvalidInput(format!(
                "folder profile targets an application-managed root: {folder}"
            )));
        }
        if !normalized.insert(folder) {
            return Err(AppError::InvalidInput(
                "folder profile contains duplicate paths".into(),
            ));
        }
    }
    Ok(normalized.into_iter().collect())
}

#[cfg(test)]
fn analysis_prompt(request: &CodexAnalysisRequest, input_sha256: &str) -> Result<String, AppError> {
    analysis_prompt_for_provider(request, input_sha256, "Codex setup analysis")
}

pub(crate) fn validate_analysis_output(
    value: Value,
    request: &CodexAnalysisRequest,
    input_sha256: &str,
    evidence: &[ApprovedEvidence],
) -> Result<CodexAnalysis, AppError> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Serialization("Codex analysis output is not an object".into()))?;
    let allowed: BTreeSet<&str> = [
        "schema_version",
        "analysis_id",
        "mode",
        "input_sha256",
        "project_summary",
        "proposals",
        "component_recommendations",
        "warnings",
    ]
    .into_iter()
    .collect();
    if object.keys().any(|key| !allowed.contains(key.as_str())) {
        return Err(AppError::Serialization(
            "Codex analysis contains an unsupported field".into(),
        ));
    }
    let analysis: CodexAnalysis = serde_json::from_value(value)?;
    if analysis.schema_version != CODEX_SCHEMA_VERSION
        || analysis.input_sha256 != input_sha256
        || analysis.proposals.is_empty()
        || (analysis.mode == AnalysisMode::NewProjectIdentity
            && request.mode != "new_project_identity")
        || (analysis.mode == AnalysisMode::ExistingProjectSemantics
            && request.mode != "existing_project_semantics")
        || analysis.project_summary.trim().is_empty()
        || analysis.project_summary.chars().count() > 1200
    {
        return Err(AppError::Serialization(
            "Codex analysis failed schema validation".into(),
        ));
    }
    let mut proposal_keys = BTreeSet::new();
    let approved_references = evidence
        .iter()
        .map(|item| item.reference.as_str())
        .collect::<BTreeSet<_>>();
    for proposal in &analysis.proposals {
        let key = serde_json::to_string(&proposal.key)?;
        if !proposal_keys.insert(key) {
            return Err(AppError::Serialization(
                "Codex analysis contains duplicate proposal keys".into(),
            ));
        }
        if !(0.0..=1.0).contains(&proposal.confidence)
            || proposal.reason.chars().count() > 500
            || proposal
                .evidence_refs
                .iter()
                .any(|reference| reference.trim().is_empty())
            || proposal
                .evidence_refs
                .iter()
                .any(|reference| !approved_references.contains(reference.as_str()))
            || (!approved_references.is_empty() && proposal.evidence_refs.is_empty())
        {
            return Err(AppError::Serialization(
                "Codex proposal failed schema validation".into(),
            ));
        }
        validate_output_text(&proposal.reason, "Codex proposal reason")?;
        validate_user_facing_analysis_text(&proposal.reason, "Codex proposal reason")?;
        reject_sensitive_output_value(&proposal.value)?;
        validate_proposal_value(&proposal.key, &proposal.value)?;
    }
    let required_keys = REQUIRED_ANALYSIS_PROPOSAL_KEYS
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<BTreeSet<_>, _>>()?;
    if required_keys.iter().any(|key| !proposal_keys.contains(key)) {
        return Err(AppError::Serialization(
            "Codex analysis omitted one or more required semantic proposals".into(),
        ));
    }
    let allowed_component_ids = analysis_component_registry_ids(request)?;
    if analysis
        .component_recommendations
        .iter()
        .any(|recommendation| {
            !valid_component_recommendation_id(&recommendation.component_id)
                || !allowed_component_ids.contains(&recommendation.component_id)
                || recommendation.reason.chars().count() > 500
        })
        || analysis
            .warnings
            .iter()
            .any(|warning| warning.chars().count() > 500)
    {
        return Err(AppError::Serialization(
            "Codex recommendation failed schema validation".into(),
        ));
    }
    validate_output_text(&analysis.project_summary, "Codex project summary")?;
    validate_user_facing_analysis_text(&analysis.project_summary, "Codex project summary")?;
    for warning in &analysis.warnings {
        validate_output_text(warning, "Codex warning")?;
        validate_user_facing_analysis_text(warning, "Codex warning")?;
    }
    for recommendation in &analysis.component_recommendations {
        reject_sensitive_output_value(&Value::String(recommendation.component_id.clone()))?;
        validate_output_text(&recommendation.reason, "Codex recommendation reason")?;
        validate_user_facing_analysis_text(&recommendation.reason, "Codex recommendation reason")?;
    }
    Ok(analysis)
}

fn analysis_component_registry_ids(
    request: &CodexAnalysisRequest,
) -> Result<BTreeSet<String>, AppError> {
    let Some(registry) = request.constraints.get("component_registry") else {
        return Ok(BTreeSet::new());
    };
    let object = registry.as_object().ok_or_else(|| {
        AppError::InvalidInput("the component recommendation registry is malformed".into())
    })?;
    if object.len() != 3
        || !object.contains_key("source_revision")
        || !object.contains_key("manifest_sha256")
        || !object.contains_key("component_ids")
    {
        return Err(AppError::InvalidInput(
            "the component recommendation registry is incomplete".into(),
        ));
    }
    let revision = object
        .get("source_revision")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let manifest_sha256 = object
        .get("manifest_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if revision.len() != 40
        || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        || manifest_sha256.len() != 64
        || !manifest_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AppError::InvalidInput(
            "the component recommendation registry identity is invalid".into(),
        ));
    }
    let component_ids = object
        .get("component_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::InvalidInput("the component recommendation registry has no IDs".into())
        })?;
    if component_ids.len() > 4096 {
        return Err(AppError::InvalidInput(
            "the component recommendation registry is too large".into(),
        ));
    }
    let mut allowed = BTreeSet::new();
    for component_id in component_ids {
        let component_id = component_id.as_str().ok_or_else(|| {
            AppError::InvalidInput("the component recommendation registry has an invalid ID".into())
        })?;
        if !valid_component_recommendation_id(component_id)
            || !allowed.insert(component_id.to_string())
        {
            return Err(AppError::InvalidInput(
                "the component recommendation registry has an invalid or duplicate ID".into(),
            ));
        }
    }
    Ok(allowed)
}

fn valid_component_recommendation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && (value.as_bytes()[0].is_ascii_lowercase() || value.as_bytes()[0].is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn validate_user_facing_analysis_text(value: &str, label: &str) -> Result<(), AppError> {
    let normalized = value.to_ascii_lowercase();
    const INTERNAL_TERMS: &[&str] = &[
        "approved input",
        "constraint",
        "evidence_refs",
        "evidence refs",
        "input_sha256",
        "input sha-256",
        "output schema",
        "workshop id",
        "windows",
        "macos",
        "operating system",
        "platform",
    ];
    if INTERNAL_TERMS.iter().any(|term| normalized.contains(term)) {
        return Err(AppError::Serialization(format!(
            "{label} contains internal setup details"
        )));
    }
    validate_provider_neutral_text(value, label)?;
    Ok(())
}

fn validate_provider_neutral_text(value: &str, label: &str) -> Result<(), AppError> {
    let attribution = regex::Regex::new(
        r"(?i)(?:\b(?:generated|suggested|analy[sz]ed|prepared|written)\s+by\s+(?:codex|chatgpt|openai|claude|anthropic|kimi|moonshot|glm|zhipu|deepseek)\b|\busing\s+(?:the\s+)?(?:codex|chatgpt|openai|claude|anthropic|kimi|moonshot|glm|zhipu|deepseek)\b|\bsetup\s+(?:assistant|provider)\b|\banalysis\s+model\b)",
    )
    .is_ok_and(|pattern| pattern.is_match(value));
    if attribution {
        return Err(AppError::Serialization(format!(
            "{label} contains setup-provider attribution"
        )));
    }
    Ok(())
}

fn validate_output_text(value: &str, label: &str) -> Result<(), AppError> {
    let normalized = value.to_ascii_lowercase();
    let account_shaped = regex::Regex::new(
        r"(?i)(?:\baccount[\s_-]*id\b|\bacct_[a-z0-9_-]+\b|\bplan(?:[\s_-]*type)?\s*[:=]|\brate[\s_-]*limits?\b|\busage(?:[\s_-]*(?:limited|remaining|used))?\s*[:=]|\b[\w.+-]+@[\w.-]+\.[a-z]{2,}\b)",
    )
    .is_ok_and(|pattern| pattern.is_match(value));
    if redact_secrets(value, &[]) != value
        || account_shaped
        || normalized.contains("\"email\"")
        || normalized.contains("'email'")
    {
        return Err(AppError::Serialization(format!(
            "{label} contains account or credential-shaped content"
        )));
    }
    Ok(())
}

fn reject_sensitive_output_value(value: &Value) -> Result<(), AppError> {
    fn validate_recursive(value: &Value) -> Result<(), AppError> {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    if matches!(
                        key.to_ascii_lowercase().as_str(),
                        "email"
                            | "account_id"
                            | "accountid"
                            | "plan"
                            | "plan_type"
                            | "usage_limited"
                            | "ratelimits"
                            | "rate_limits"
                            | "access_token"
                            | "refresh_token"
                            | "token"
                    ) {
                        return Err(AppError::Serialization(
                            "Codex proposal contains account or credential-shaped content".into(),
                        ));
                    }
                    validate_recursive(child)?;
                }
            }
            Value::Array(values) => {
                for child in values {
                    validate_recursive(child)?;
                }
            }
            Value::String(value) => validate_output_text(value, "Codex proposal value")?,
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
        Ok(())
    }

    let serialized = serde_json::to_string(value)?;
    if redact_secrets(&serialized, &[]) != serialized {
        return Err(AppError::Serialization(
            "Codex proposal contains account or credential-shaped content".into(),
        ));
    }
    validate_recursive(value)
}

fn validate_proposal_value(key: &ProposalKey, value: &Value) -> Result<(), AppError> {
    match key {
        ProposalKey::DisplayName
        | ProposalKey::ScriptPrefix
        | ProposalKey::PrimaryNamespace
        | ProposalKey::AgentsProfile
        | ProposalKey::LocalisationConvention
        | ProposalKey::DocumentationConvention => {
            let value = value.as_str().ok_or_else(|| {
                AppError::Serialization("Codex proposal must use a string value".into())
            })?;
            crate::descriptors::validate_field(value, "Codex proposal")?;
            validate_provider_neutral_text(value, "Codex proposal")?;
            if value.chars().count() > 256 {
                return Err(AppError::Serialization(
                    "Codex proposal string is too long".into(),
                ));
            }
            if matches!(
                key,
                ProposalKey::ScriptPrefix | ProposalKey::PrimaryNamespace
            ) && !value.chars().enumerate().all(|(index, character)| {
                (index == 0 && (character.is_ascii_lowercase() || character == '_'))
                    || (index > 0
                        && (character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '_'))
            }) {
                return Err(AppError::Serialization(
                    "Codex identifier proposal contains unsupported characters".into(),
                ));
            }
        }
        ProposalKey::ProjectId => {
            let value = value.as_str().ok_or_else(|| {
                AppError::Serialization("Codex project ID proposal must be a string".into())
            })?;
            crate::descriptors::validate_project_id(value)?;
        }
        ProposalKey::ProjectDescription => {
            let value = value.as_str().ok_or_else(|| {
                AppError::Serialization("Codex description proposal must be a string".into())
            })?;
            if value.trim().is_empty() || value.contains('\0') || value.len() > MAX_BRIEF_BYTES {
                return Err(AppError::Serialization(
                    "Codex description proposal is empty or too long".into(),
                ));
            }
            validate_provider_neutral_text(value, "Codex description proposal")?;
        }
        ProposalKey::DescriptorTags => {
            let tags = value.as_array().ok_or_else(|| {
                AppError::Serialization("Codex descriptor tags proposal must be an array".into())
            })?;
            let tags = tags
                .iter()
                .map(|tag| tag.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>();
            if crate::descriptors::validate_descriptor_tags(&tags).is_err() {
                return Err(AppError::Serialization(
                    "Codex descriptor tags proposal is invalid".into(),
                ));
            }
        }
        ProposalKey::FolderProfile => {
            let folders = value.as_array().ok_or_else(|| {
                AppError::Serialization("Codex folder profile proposal must be an array".into())
            })?;
            let folders = folders
                .iter()
                .map(|folder| folder.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>();
            if folders.iter().any(|folder| folder.is_empty())
                || validate_folder_profile_paths(&folders).is_err()
            {
                return Err(AppError::Serialization(
                    "Codex folder profile proposal is invalid".into(),
                ));
            }
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
}

fn parse_account_status(value: &Value) -> CodexAccountStatus {
    let account = value.get("account").unwrap_or(value);
    let auth_mode = account
        .get("type")
        .or_else(|| account.get("accountType"))
        .or_else(|| account.get("authMode"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    // Current App Server account/read responses identify a signed-in
    // ChatGPT session with `account.type = "chatgpt"`; older builds also
    // exposed an explicit boolean. Preserve the boolean when present, but do
    // not require a field the current protocol no longer returns.
    let authenticated = account
        .get("authenticated")
        .or_else(|| account.get("isLoggedIn"))
        .or_else(|| account.get("loggedIn"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| auth_mode == "chatgpt");
    let usage_limited = value
        .get("usageLimited")
        .or_else(|| account.get("usageLimited"))
        .or_else(|| {
            account
                .get("rateLimits")
                .and_then(|rate| rate.get("limited"))
        })
        .and_then(Value::as_bool)
        .unwrap_or(false);
    CodexAccountStatus {
        available: true,
        authenticated,
        auth_mode,
        email: account
            .get("email")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        plan_type: account
            .get("planType")
            .or_else(|| account.get("plan"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        usage_limited,
        app_server_version: value
            .get("version")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        error: None,
    }
}

fn rate_limits_limited(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (key == "limited" && value.as_bool() == Some(true))
                || (key == "rateLimitReachedType"
                    && value.as_str().is_some_and(|value| !value.is_empty()))
                || (key == "usedPercent" && value.as_f64().is_some_and(|value| value >= 100.0))
                || rate_limits_limited(value)
        }),
        Value::Array(values) => values.iter().any(rate_limits_limited),
        _ => false,
    }
}

fn validate_login_id(login_id: &str) -> Result<(), AppError> {
    if login_id.trim().is_empty()
        || login_id.len() > 128
        || login_id.chars().any(|character| character.is_control())
    {
        return Err(AppError::InvalidInput("Codex login ID is invalid".into()));
    }
    Ok(())
}

fn parse_login_start(value: &Value, device_code: bool) -> CodexLoginStart {
    let value = value.get("login").unwrap_or(value);
    let raw_login_id = value
        .get("loginId")
        .or_else(|| value.get("login_id"))
        .and_then(Value::as_str);
    let raw_auth_url = value
        .get("authUrl")
        .or_else(|| value.get("authorizationUrl"))
        .and_then(Value::as_str);
    let raw_verification_url = value
        .get("verificationUrl")
        .or_else(|| value.get("verification_uri"))
        .and_then(Value::as_str);
    let invalid_url = raw_auth_url.is_some_and(|url| !is_safe_login_url(url))
        || raw_verification_url.is_some_and(|url| !is_safe_login_url(url));
    let invalid_login_id = raw_login_id.is_none_or(|login_id| validate_login_id(login_id).is_err());
    let raw_user_code = value
        .get("userCode")
        .or_else(|| value.get("user_code"))
        .and_then(Value::as_str);
    let invalid_mode_fields = if device_code {
        raw_verification_url.is_none_or(|url| !is_safe_login_url(url))
            || raw_user_code.is_none_or(|code| {
                code.is_empty()
                    || code.len() > 64
                    || code
                        .chars()
                        .any(|character| !(character.is_ascii_alphanumeric() || character == '-'))
            })
    } else {
        raw_auth_url.is_none_or(|url| !is_safe_login_url(url))
    };
    CodexLoginStart {
        available: true,
        login_id: raw_login_id
            .filter(|login_id| validate_login_id(login_id).is_ok())
            .map(ToOwned::to_owned),
        auth_url: raw_auth_url
            .filter(|url| is_safe_login_url(url))
            .map(ToOwned::to_owned),
        verification_url: raw_verification_url
            .filter(|url| is_safe_login_url(url))
            .map(ToOwned::to_owned),
        user_code: raw_user_code
            .filter(|code| {
                !code.is_empty()
                    && code.len() <= 64
                    && code
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
            })
            .map(ToOwned::to_owned),
        device_code,
        error: if invalid_login_id {
            Some("Codex returned an invalid login identifier".into())
        } else if invalid_url {
            Some("Codex returned an invalid HTTPS authentication URL".into())
        } else if invalid_mode_fields {
            Some("Codex returned incomplete browser or device-code login instructions".into())
        } else {
            None
        },
    }
}

pub(crate) fn is_safe_login_url(value: &str) -> bool {
    let Some(authority_and_path) = value.strip_prefix("https://") else {
        return false;
    };
    if value.len() > 2048
        || value.chars().any(char::is_whitespace)
        || value.contains('\\')
        || value.contains('@')
        || authority_and_path.is_empty()
    {
        return false;
    }
    let authority = authority_and_path
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    !authority.is_empty() && authority.contains('.') && !authority.contains(':')
}

enum LoginNotification {
    Completed,
    Updated,
    Failed(String),
    Other,
}

fn account_login_notification(
    value: &Value,
    login_id: &str,
) -> Result<LoginNotification, AppError> {
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return Ok(LoginNotification::Other);
    };
    let params = value.get("params").unwrap_or(&Value::Null);
    match method {
        "account/login/completed" => {
            let notification_login_id = params
                .get("loginId")
                .or_else(|| params.get("login_id"))
                .and_then(Value::as_str);
            if notification_login_id != Some(login_id) {
                return Ok(LoginNotification::Other);
            }
            if params.get("success").and_then(Value::as_bool) == Some(true) {
                Ok(LoginNotification::Completed)
            } else {
                let error = params
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex login failed");
                Ok(LoginNotification::Failed(safe_login_error(error)))
            }
        }
        "account/updated"
            if params
                .get("authMode")
                .or_else(|| params.get("auth_mode"))
                .and_then(Value::as_str)
                .is_some_and(|mode| mode.eq_ignore_ascii_case("chatgpt")) =>
        {
            Ok(LoginNotification::Updated)
        }
        _ => Ok(LoginNotification::Other),
    }
}

fn thread_id(value: &Value) -> Result<String, AppError> {
    value
        .get("threadId")
        .or_else(|| value.get("thread_id"))
        .or_else(|| value.get("thread").and_then(|thread| thread.get("id")))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            AppError::Serialization("Codex App Server did not return a setup thread ID".into())
        })
}

fn structured_output(value: &Value) -> Option<Value> {
    for candidate in [
        value.get("structuredOutput"),
        value.get("structured_output"),
        value.get("output"),
        value.get("text"),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.is_object() {
            return Some(candidate.clone());
        }
        if let Some(text) = candidate.as_str() {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                if parsed.is_object() {
                    return Some(parsed);
                }
            }
        }
    }
    for key in ["params", "result", "item", "turn", "event"] {
        if let Some(output) = value.get(key).and_then(structured_output) {
            return Some(output);
        }
    }
    for key in ["content", "items"] {
        if let Some(values) = value.get(key).and_then(Value::as_array) {
            for item in values {
                if let Some(output) = structured_output(item) {
                    return Some(output);
                }
            }
        }
    }
    None
}

pub fn missing_status(error: impl Into<String>) -> CodexAccountStatus {
    CodexAccountStatus {
        available: false,
        error: Some(redact_secrets(&error.into(), &[])),
        ..Default::default()
    }
}

pub struct ProcessJsonlTransport {
    child: Option<Child>,
    stdin: ChildStdin,
    lines: Receiver<Result<String, String>>,
    stdout_open: Arc<AtomicBool>,
}

impl ProcessJsonlTransport {
    pub fn start(executable: PathBuf) -> Result<Self, AppError> {
        crate::process::validate_executable_publisher(&executable, "OpenAI")?;
        Self::start_command(
            executable,
            vec!["app-server".into(), "--stdio".into()],
            None,
        )
    }

    pub fn start_command(
        executable: PathBuf,
        args: Vec<String>,
        cwd: Option<PathBuf>,
    ) -> Result<Self, AppError> {
        Self::start_command_with_path(executable, args, cwd, None)
    }

    pub fn start_command_with_path(
        executable: PathBuf,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        safe_path: Option<PathBuf>,
    ) -> Result<Self, AppError> {
        let executable_sha256 = crate::security::sha256_file(&executable)?;
        Self::start_command_with_path_and_identity(
            executable,
            args,
            cwd,
            safe_path,
            &executable_sha256,
        )
    }

    pub(crate) fn start_command_with_path_and_identity(
        executable: PathBuf,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        safe_path: Option<PathBuf>,
        executable_sha256: &str,
    ) -> Result<Self, AppError> {
        if !executable.is_absolute() {
            return Err(AppError::Process(
                "JSONL process executable must be an absolute path".into(),
            ));
        }
        if crate::security::path_has_link_component(&executable) {
            return Err(AppError::Process(
                "JSONL process executable contains a symlink or junction".into(),
            ));
        }
        if crate::security::sha256_file(&executable)? != executable_sha256 {
            return Err(AppError::Process(
                "JSONL process executable identity changed before spawn".into(),
            ));
        }
        let mut command = Command::new(executable);
        command
            .args(args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(cwd) = cwd {
            if !cwd.is_absolute() || !cwd.is_dir() {
                return Err(AppError::Process(
                    "JSONL process working directory must be an existing absolute directory".into(),
                ));
            }
            if crate::security::path_has_link_component(&cwd) {
                return Err(AppError::Process(
                    "JSONL process working directory contains a symlink or junction".into(),
                ));
            }
            command.current_dir(cwd);
        }
        for name in [
            "PATH",
            "SystemRoot",
            "SYSTEMROOT",
            "TEMP",
            "TMP",
            "TMPDIR",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "CODEX_HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "LANG",
            "LC_ALL",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if let Some(safe_path) = safe_path {
            if !safe_path.is_absolute() || !safe_path.is_dir() {
                return Err(AppError::Process(
                    "JSONL process PATH override must be an existing absolute directory".into(),
                ));
            }
            if crate::security::path_has_link_component(&safe_path) {
                return Err(AppError::Process(
                    "JSONL process PATH override contains a symlink or junction".into(),
                ));
            }
            command.env("PATH", safe_path);
        }
        crate::process::configure_child_no_console_window(&mut command);
        crate::process::configure_child_process_group(&mut command);
        let mut child = command.spawn().map_err(|error| {
            AppError::Process(format!("could not start JSONL process: {error}"))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Process("Codex App Server stdin was not created".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Process("Codex App Server stdout was not created".into()))?;
        let (sender, receiver) = mpsc::channel();
        let stdout_open = Arc::new(AtomicBool::new(true));
        spawn_line_reader(stdout, sender, Arc::clone(&stdout_open));
        Ok(Self {
            child: Some(child),
            stdin,
            lines: receiver,
            stdout_open,
        })
    }
}

fn spawn_line_reader(
    stdout: ChildStdout,
    sender: Sender<Result<String, String>>,
    stdout_open: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_bounded_line(&mut reader) {
                Ok(Some(line)) => {
                    if sender.send(Ok(line)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    break;
                }
            }
        }
        stdout_open.store(false, Ordering::Release);
    });
}

fn read_bounded_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader
            .fill_buf()
            .map_err(|error| format!("Codex App Server output could not be read: {error}"))?;
        if chunk.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        if let Some(newline) = chunk.iter().position(|byte| *byte == b'\n') {
            if bytes.len().saturating_add(newline) > MAX_JSONL_LINE_BYTES {
                return Err("Codex App Server JSONL message exceeded the size limit".into());
            }
            bytes.extend_from_slice(&chunk[..newline]);
            reader.consume(newline + 1);
            break;
        }
        if bytes.len().saturating_add(chunk.len()) > MAX_JSONL_LINE_BYTES {
            return Err("Codex App Server JSONL message exceeded the size limit".into());
        }
        bytes.extend_from_slice(chunk);
        let consumed = chunk.len();
        reader.consume(consumed);
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| format!("Codex App Server output was not valid UTF-8: {error}"))
}

impl JsonlTransport for ProcessJsonlTransport {
    fn send(&mut self, value: &Value) -> Result<(), AppError> {
        let line = serde_json::to_vec(value)?;
        if line.len() > MAX_JSONL_LINE_BYTES {
            return Err(AppError::Serialization(
                "Codex App Server request is too large".into(),
            ));
        }
        self.stdin.write_all(&line).map_err(|_| {
            AppError::Protocol("Codex App Server input stream is unavailable".into())
        })?;
        self.stdin.write_all(b"\n").map_err(|_| {
            AppError::Protocol("Codex App Server input stream is unavailable".into())
        })?;
        self.stdin.flush().map_err(|_| {
            AppError::Protocol("Codex App Server input stream is unavailable".into())
        })?;
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Value>, AppError> {
        match self.lines.recv_timeout(timeout) {
            Ok(Ok(line)) => serde_json::from_str(&line)
                .map(Some)
                .map_err(|_| AppError::Protocol("Codex App Server returned invalid JSONL".into())),
            Ok(Err(error)) => Err(AppError::Process(error)),
            Err(RecvTimeoutError::Timeout) => {
                Err(AppError::Process("Codex App Server timed out".into()))
            }
            Err(RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    fn close(&mut self) {
        if let Some(mut child) = self.child.take() {
            if matches!(child.try_wait(), Ok(None)) {
                crate::process::terminate_process_tree(&mut child);
            }
            let _ = child.wait();
        }
        self.stdout_open.store(false, Ordering::Release);
    }

    fn is_alive(&mut self) -> bool {
        self.stdout_open.load(Ordering::Acquire)
            && self
                .child
                .as_mut()
                .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }
}

impl Drop for ProcessJsonlTransport {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn find_codex_executable() -> Option<PathBuf> {
    let cache = CODEX_EXECUTABLE.get_or_init(|| Mutex::new(None));
    if let Ok(cached) = cache.lock() {
        if let Some(cached) = cached.as_ref() {
            if cached.path.is_file()
                && !crate::security::path_has_link_component(&cached.path)
                && crate::process::validate_executable_publisher(&cached.path, "OpenAI").is_ok()
                && crate::security::sha256_file(&cached.path).ok().as_deref()
                    == Some(cached.sha256.as_str())
            {
                return Some(cached.path.clone());
            }
        }
    }
    let names: &[&str] = if cfg!(target_os = "windows") {
        &["codex.exe"]
    } else {
        &["codex"]
    };
    let path = std::env::var_os("PATH");
    let local_app_data = std::env::var_os("LOCALAPPDATA");
    let mut candidates = managed_codex_executable_candidates(local_app_data.as_deref());
    candidates.extend(codex_executable_candidates(
        path.as_deref(),
        local_app_data.as_deref(),
        names,
    ));
    let found = candidates.into_iter().find_map(|candidate| {
        let metadata = std::fs::symlink_metadata(&candidate).ok()?;
        if is_link_metadata(&metadata) || !metadata.is_file() {
            return None;
        }
        let canonical = resolve_codex_executable_path(&candidate)?;
        if crate::security::path_has_link_component(&canonical) {
            return None;
        }
        crate::process::validate_executable_publisher(&canonical, "OpenAI")
            .ok()
            .map(|_| canonical)
    });
    if let Some(path) = found.as_ref() {
        if let Ok(sha256) = crate::security::sha256_file(path) {
            if let Ok(mut cached) = cache.lock() {
                *cached = Some(CachedCodexExecutable {
                    path: path.clone(),
                    sha256,
                });
            }
        }
    }
    found
}

#[cfg(target_os = "windows")]
fn managed_codex_executable_candidates(local_app_data: Option<&OsStr>) -> Vec<PathBuf> {
    let Some(local_app_data) = local_app_data else {
        return Vec::new();
    };
    let root = PathBuf::from(local_app_data)
        .join("OpenAI")
        .join("Codex")
        .join("bin");
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut candidates = entries
        .take(64)
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let directory_type = entry.file_type().ok()?;
            if !directory_type.is_dir() || directory_type.is_symlink() {
                return None;
            }
            let candidate = entry.path().join("codex.exe");
            let metadata = fs::symlink_metadata(&candidate).ok()?;
            if !metadata.is_file() || is_link_metadata(&metadata) {
                return None;
            }
            Some((metadata.modified().ok(), candidate))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    candidates
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn managed_codex_executable_candidates(_local_app_data: Option<&OsStr>) -> Vec<PathBuf> {
    Vec::new()
}

fn codex_executable_candidates(
    path: Option<&OsStr>,
    _local_app_data: Option<&OsStr>,
    names: &[&str],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = path {
        candidates.extend(
            std::env::split_paths(path)
                .flat_map(|entry| names.iter().map(move |name| entry.join(name))),
        );
    }
    #[cfg(target_os = "windows")]
    if let Some(local_app_data) = _local_app_data {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
                .join("codex.exe"),
        );
    }
    #[cfg(target_os = "macos")]
    {
        let _ = _local_app_data;
        candidates.extend([
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex"),
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
        ]);
    }
    candidates.dedup();
    candidates
}

#[cfg(not(target_os = "windows"))]
fn resolve_codex_executable_path(candidate: &std::path::Path) -> Option<PathBuf> {
    std::fs::canonicalize(candidate).ok()
}

#[cfg(target_os = "windows")]
fn resolve_codex_executable_path(candidate: &std::path::Path) -> Option<PathBuf> {
    let file = std::fs::File::open(candidate).ok()?;
    let handle = file.as_raw_handle();
    let mut buffer = vec![0_u16; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED,
            )
        };
        if length == 0 {
            return None;
        }
        if (length as usize) < buffer.len() {
            let mut path = buffer[..length as usize].to_vec();
            const VERBATIM_PREFIX: &[u16] =
                &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
            const VERBATIM_UNC_PREFIX: &[u16] = &[
                b'\\' as u16,
                b'\\' as u16,
                b'?' as u16,
                b'\\' as u16,
                b'U' as u16,
                b'N' as u16,
                b'C' as u16,
                b'\\' as u16,
            ];
            if path.starts_with(VERBATIM_UNC_PREFIX) {
                path.splice(..VERBATIM_UNC_PREFIX.len(), [b'\\' as u16, b'\\' as u16]);
            } else if path.starts_with(VERBATIM_PREFIX) {
                path.drain(..VERBATIM_PREFIX.len());
            }
            return Some(PathBuf::from(OsString::from_wide(&path)));
        }
        if buffer.len() >= 32 * 1024 {
            return None;
        }
        buffer.resize(buffer.len() * 2, 0);
    }
}

pub fn validate_confirmed_record(record: &CodexAnalysisRecord) -> Result<(), AppError> {
    let allowed_fields = [
        "display_name",
        "project_id",
        "script_prefix",
        "primary_namespace",
        "project_description",
        "descriptor_tags",
        "folder_profile",
        "agents_profile",
        "localisation_convention",
        "documentation_convention",
    ];
    let provider = record.provider.as_deref().unwrap_or("codex");
    let valid_engine = (record.engine == "codex_app_server"
        && provider == "codex"
        && record.auth_mode == "chatgpt")
        || (record.engine == "provider_api"
            && matches!(
                provider,
                "claude" | "kimi" | "glm" | "deepseek" | "local" | "custom"
            )
            && matches!(record.auth_mode.as_str(), "api_key" | "local_endpoint")
            && record
                .model
                .as_deref()
                .is_some_and(|model| !model.trim().is_empty()));
    if !valid_engine
        || record.schema_version != CODEX_SCHEMA_VERSION
        || record.account_identity_persisted
        || !is_sha256(&record.input_sha256)
        || !is_sha256(&record.output_sha256)
        || record.confirmed_fields.is_empty()
        || record
            .confirmed_fields
            .iter()
            .any(|field| !allowed_fields.contains(&field.as_str()))
        || record.confirmed_fields.len()
            != record
                .confirmed_fields
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
        || REQUIRED_ANALYSIS_PROPOSAL_KEYS.iter().any(|required| {
            !record
                .confirmed_fields
                .iter()
                .any(|field| field == required)
        })
        || record.confirmed_at.trim().is_empty()
        || record.analysis_purpose.as_deref().is_some_and(|purpose| {
            !matches!(
                purpose,
                "existing_project_import" | "maintenance_reanalysis"
            )
        })
        || record
            .evidence_sha256
            .as_deref()
            .is_some_and(|hash| !is_sha256(hash))
        || record.source_revision.as_deref().is_none_or(|revision| {
            revision.len() != 40
                || !revision
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        || record
            .source_manifest_sha256
            .as_deref()
            .is_none_or(|hash| !is_sha256(hash))
        || (provider != "codex"
            && record
                .optimization_profile
                .as_deref()
                .is_none_or(|profile| profile.trim().is_empty()))
        || record.analysis_purpose.as_deref() == Some("maintenance_reanalysis")
            && record.evidence_sha256.is_none()
    {
        return Err(AppError::Serialization(
            "Codex analysis record is incomplete or unsafe".into(),
        ));
    }
    Ok(())
}

pub fn validate_confirmed_record_for_profile(
    record: &CodexAnalysisRecord,
    provider: &str,
    model: &str,
    reasoning_effort: &str,
    optimization_profile: &str,
) -> Result<(), AppError> {
    validate_confirmed_record(record)?;
    let canonical_profile = crate::ai::profile(provider)
        .map(|profile| profile.optimization_profile)
        .ok_or_else(|| {
            AppError::InvalidInput("installed lock uses an unsupported AI provider".into())
        })?;
    crate::ai::validate_reasoning_effort(reasoning_effort)?;
    if model.trim().is_empty()
        || record.provider.as_deref().unwrap_or("codex") != provider
        || record.model.as_deref().is_some_and(|value| value != model)
        || record
            .reasoning_effort
            .as_deref()
            .is_some_and(|value| value != reasoning_effort)
        || optimization_profile != canonical_profile
        || record
            .optimization_profile
            .as_deref()
            .is_none_or(|value| value != canonical_profile)
    {
        return Err(AppError::Serialization(
            "confirmed setup analysis does not match its installed provider profile".into(),
        ));
    }
    Ok(())
}

/// Bind user confirmation to the exact schema-validated analysis returned by
/// the app-server. The renderer supplies only the selected proposal keys; all
/// immutable record fields come from the core-owned analysis result.
pub fn confirm_analysis_record(
    record: &CodexAnalysisRecord,
    analysis: &CodexAnalysis,
    confirmed_fields: &[String],
) -> Result<CodexAnalysisRecord, AppError> {
    if record.analysis_id != analysis.analysis_id
        || !record.confirmed_fields.is_empty()
        || record.input_sha256 != analysis.input_sha256
        || record.schema_version != analysis.schema_version
        || (record.engine == "codex_app_server"
            && (record.auth_mode != "chatgpt"
                || record.provider.as_deref().unwrap_or("codex") != "codex"))
        || (record.engine == "provider_api"
            && !matches!(record.auth_mode.as_str(), "api_key" | "local_endpoint"))
        || record.account_identity_persisted
        || record.output_sha256 != sha256_bytes(&serde_json::to_vec(analysis)?)
    {
        return Err(AppError::Credential(
            "Codex confirmation is not bound to the returned analysis".into(),
        ));
    }
    let available = analysis
        .proposals
        .iter()
        .map(|proposal| serde_json::to_string(&proposal.key))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let selected = confirmed_fields
        .iter()
        .map(|field| serde_json::to_string(field).unwrap_or_default())
        .collect::<BTreeSet<_>>();
    let required = REQUIRED_ANALYSIS_PROPOSAL_KEYS
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<BTreeSet<_>, _>>()?;
    if confirmed_fields.is_empty()
        || selected.len() != confirmed_fields.len()
        || selected != available
        || required.iter().any(|key| !selected.contains(key))
        || confirmed_fields
            .iter()
            .any(|field| !available.contains(&serde_json::to_string(field).unwrap_or_default()))
    {
        return Err(AppError::Credential(
            "Codex confirmation must select returned proposal keys".into(),
        ));
    }
    let mut confirmed = record.clone();
    confirmed.confirmed_fields = confirmed_fields.to_vec();
    confirmed.confirmed_at = Utc::now().to_rfc3339();
    validate_confirmed_record(&confirmed)?;
    Ok(confirmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::Cursor;

    #[test]
    fn executable_candidates_keep_path_entries_first() {
        let joined = std::env::join_paths([
            PathBuf::from("reviewed/first"),
            PathBuf::from("reviewed/second"),
        ])
        .unwrap();
        let candidates = codex_executable_candidates(
            Some(joined.as_os_str()),
            Some(OsStr::new("C:/Users/example/AppData/Local")),
            &["codex.exe"],
        );

        assert_eq!(
            candidates[0],
            PathBuf::from("reviewed/first").join("codex.exe")
        );
        assert_eq!(
            candidates[1],
            PathBuf::from("reviewed/second").join("codex.exe")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn managed_desktop_candidates_include_only_direct_regular_binaries() {
        let local = tempfile::tempdir().unwrap();
        let root = local.path().join("OpenAI").join("Codex").join("bin");
        let current = root.join("current");
        let empty = root.join("empty");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&empty).unwrap();
        fs::write(current.join("codex.exe"), b"test").unwrap();
        fs::write(root.join("codex.exe"), b"not a direct candidate").unwrap();

        let candidates = managed_codex_executable_candidates(Some(local.path().as_os_str()));

        assert_eq!(candidates, vec![current.join("codex.exe")]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn executable_candidates_include_the_verified_desktop_install_location() {
        let candidates = codex_executable_candidates(
            None,
            Some(OsStr::new("C:/Users/example/AppData/Local")),
            &["codex.exe"],
        );

        assert_eq!(
            candidates,
            vec![PathBuf::from(
                "C:/Users/example/AppData/Local/Programs/OpenAI/Codex/bin/codex.exe"
            )]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn installed_desktop_codex_junction_resolves_to_a_reviewable_binary() {
        let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let candidate = PathBuf::from(local_app_data)
            .join("Programs")
            .join("OpenAI")
            .join("Codex")
            .join("bin")
            .join("codex.exe");
        if !candidate.is_file() {
            return;
        }

        let resolved = resolve_codex_executable_path(&candidate)
            .expect("installed Codex executable should resolve through its desktop junction");
        assert!(!crate::security::path_has_link_component(&resolved));
        crate::process::validate_executable_publisher(&resolved, "OpenAI").unwrap_or_else(
            |error| {
                panic!(
                    "resolved Codex executable should retain its reviewed publisher ({}): {error}",
                    resolved.display()
                )
            },
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn executable_candidates_include_current_and_legacy_desktop_apps() {
        let candidates = codex_executable_candidates(None, None, &["codex"]);

        assert!(candidates.contains(&PathBuf::from(
            "/Applications/ChatGPT.app/Contents/Resources/codex"
        )));
        assert!(candidates.contains(&PathBuf::from(
            "/Applications/Codex.app/Contents/Resources/codex"
        )));
    }

    struct FakeTransport {
        sent: Vec<Value>,
        incoming: VecDeque<Value>,
        alive: bool,
    }

    impl JsonlTransport for FakeTransport {
        fn send(&mut self, value: &Value) -> Result<(), AppError> {
            self.sent.push(value.clone());
            Ok(())
        }
        fn receive(&mut self, _timeout: Duration) -> Result<Option<Value>, AppError> {
            Ok(self.incoming.pop_front())
        }
        fn close(&mut self) {}

        fn is_alive(&mut self) -> bool {
            self.alive
        }
    }

    struct TimeoutTransport {
        sent: Vec<Value>,
        cancellation_response_pending: bool,
    }

    impl JsonlTransport for TimeoutTransport {
        fn send(&mut self, value: &Value) -> Result<(), AppError> {
            self.cancellation_response_pending = value["method"] == "account/login/cancel";
            self.sent.push(value.clone());
            Ok(())
        }

        fn receive(&mut self, _timeout: Duration) -> Result<Option<Value>, AppError> {
            if self.cancellation_response_pending {
                self.cancellation_response_pending = false;
                Ok(Some(response(1, json!({}))))
            } else {
                Err(AppError::Process("receive timed out".into()))
            }
        }

        fn close(&mut self) {}
    }

    fn response(id: u64, result: Value) -> Value {
        json!({"id": id, "result": result})
    }

    #[cfg(target_os = "windows")]
    fn fake_jsonl_process_command(interrupt_after_request: bool) -> (PathBuf, Vec<String>) {
        let executable = PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let script = if interrupt_after_request {
            "$null = [Console]::In.ReadLine(); exit 0"
        } else {
            "$line = [Console]::In.ReadLine(); if ($null -eq $line) { exit 2 }; \
             [Console]::Out.WriteLine('{\"id\":1,\"result\":{\"version\":\"fake\"}}'); \
             [Console]::Out.Flush(); $null = [Console]::In.ReadLine(); \
             while ($true) { Start-Sleep -Milliseconds 50 }"
        };
        (
            executable,
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script.into(),
            ],
        )
    }

    #[cfg(not(target_os = "windows"))]
    fn fake_jsonl_process_command(interrupt_after_request: bool) -> (PathBuf, Vec<String>) {
        let executable = fs::canonicalize("/bin/sh").expect("system shell");
        let script = if interrupt_after_request {
            "IFS= read -r line; exit 0"
        } else {
            "IFS= read -r line || exit 2; \
             printf '%s\n' '{\"id\":1,\"result\":{\"version\":\"fake\"}}'; \
             IFS= read -r initialized || exit 3; \
             while :; do sleep 1; done"
        };
        (executable, vec!["-c".into(), script.into()])
    }

    fn valid_analysis_value(input_sha256: &str) -> Value {
        json!({
            "schema_version": CODEX_SCHEMA_VERSION,
            "analysis_id": Uuid::new_v4(),
            "mode": "new_project_identity",
            "input_sha256": input_sha256,
            "project_summary": "A focused HOI4 mod project.",
            "proposals": [
                {"key":"display_name","value":"Demo Project","confidence":1.0,"reason":"Matches the brief.","evidence_refs":[]},
                {"key":"project_id","value":"demo_project","confidence":1.0,"reason":"Valid project identifier.","evidence_refs":[]},
                {"key":"script_prefix","value":"demo","confidence":1.0,"reason":"Short script prefix.","evidence_refs":[]},
                {"key":"primary_namespace","value":"demo","confidence":1.0,"reason":"Matches the project identity.","evidence_refs":[]},
                {"key":"project_description","value":"A demo HOI4 project.","confidence":1.0,"reason":"Summarizes the brief.","evidence_refs":[]},
                {"key":"descriptor_tags","value":["Gameplay"],"confidence":1.0,"reason":"Matches the intended content.","evidence_refs":[]},
                {"key":"folder_profile","value":["common"],"confidence":1.0,"reason":"Provides the selected structure.","evidence_refs":[]},
                {"key":"agents_profile","value":"default","confidence":1.0,"reason":"Uses the standard project guidance.","evidence_refs":[]},
                {"key":"localisation_convention","value":"english","confidence":1.0,"reason":"Uses the selected language.","evidence_refs":[]},
                {"key":"documentation_convention","value":"markdown","confidence":1.0,"reason":"Uses the project documentation format.","evidence_refs":[]}
            ],
            "component_recommendations": [
                {"component_id":"core.skills","recommendation":"recommended","reason":"Adds the verified workflow skills."}
            ],
            "warnings": []
        })
    }

    fn analysis_request_with_component_registry(component_ids: &[&str]) -> CodexAnalysisRequest {
        CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "brief".into(),
            evidence: Vec::new(),
            constraints: json!({
                "component_registry": {
                    "source_revision": "a".repeat(40),
                    "manifest_sha256": "b".repeat(64),
                    "component_ids": component_ids,
                }
            }),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        }
    }

    #[test]
    fn initialize_is_first_request_and_initialized_notification_follows() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(1, json!({"version": "test"}))]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialize().unwrap();
        assert_eq!(protocol.transport.sent[0]["method"], "initialize");
        assert_eq!(protocol.transport.sent[1]["method"], "initialized");
    }

    #[test]
    fn model_list_preserves_only_advertised_models_and_reasoning_efforts() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(
                1,
                json!({
                    "data": [{
                        "id": "luna",
                        "model": "gpt-5.6-luna",
                        "displayName": "GPT-5.6 Luna",
                        "hidden": false,
                        "defaultReasoningEffort": "xhigh",
                        "supportedReasoningEfforts": [
                            {"reasoningEffort": "low", "description": "Light"},
                            {"reasoningEffort": "xhigh", "description": "Extra high"},
                            {"reasoningEffort": "max", "description": "Max"}
                        ]
                    }, {
                        "id": "hidden",
                        "model": "hidden-model",
                        "displayName": "Hidden",
                        "hidden": true,
                        "defaultReasoningEffort": "high",
                        "supportedReasoningEfforts": []
                    }],
                    "nextCursor": null
                }),
            )]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;
        let models = protocol.model_list().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5.6-luna");
        assert_eq!(models[0].default_reasoning_effort, "xhigh");
        assert_eq!(
            models[0].supported_reasoning_efforts,
            vec!["low", "xhigh", "max"]
        );
        assert_eq!(protocol.transport.sent[0]["method"], "model/list");
    }

    #[test]
    fn account_and_login_requests_are_rejected_before_initialize() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::new(),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);

        assert!(protocol.account_read(false).is_err());
        assert!(protocol.login_start(false).is_err());
        assert!(protocol.cancel_login("login-1").is_err());
        assert!(protocol.logout().is_err());
        assert!(protocol.transport.sent.is_empty());
    }

    #[test]
    fn production_protocol_uses_the_short_request_timeout() {
        let protocol = AppServerProtocol::new(FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::new(),
            alive: true,
        });
        assert_eq!(protocol.request_timeout, Duration::from_secs(15));
    }

    #[test]
    fn app_server_request_errors_keep_safe_protocol_details() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([json!({
                "id": 1,
                "error": {
                    "code": -32602,
                    "message": "Invalid params; Authorization: Bearer private-value"
                }
            })]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let error = protocol
            .request("thread/start", json!({}))
            .unwrap_err()
            .to_string();

        assert!(error.contains("-32602"));
        assert!(error.contains("Invalid params"));
        assert!(!error.contains("private-value"));
        assert!(error.contains("REDACTED"));
    }

    #[test]
    fn app_server_error_codes_are_redacted_and_bounded() {
        let private_code = format!("Bearer {}{}", "private-value", "x".repeat(256));
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([json!({
                "id": 1,
                "error": {"code": private_code, "message": "request failed"}
            })]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let error = protocol
            .request("thread/start", json!({}))
            .unwrap_err()
            .to_string();
        let code = error
            .split_once('(')
            .and_then(|(_, suffix)| suffix.split_once(')'))
            .map(|(code, _)| code)
            .expect("bounded protocol code");

        assert!(!error.contains("private-value"));
        assert!(code.contains("REDACTED"));
        assert!(code.chars().count() <= MAX_PROTOCOL_ERROR_CODE_CHARS);

        assert_eq!(
            safe_protocol_error_code(Some(&json!({"nested": "private-value"}))),
            "request_failed"
        );
    }

    #[test]
    fn production_jsonl_transport_initializes_frames_and_shuts_down() {
        let (executable, args) = fake_jsonl_process_command(false);
        let transport = ProcessJsonlTransport::start_command(executable, args, None).unwrap();
        let mut protocol = AppServerProtocol::with_timeout(transport, Duration::from_secs(30));

        let initialized = protocol.initialize().unwrap();

        assert_eq!(initialized["version"], "fake");
        assert!(protocol.is_alive());
        protocol.transport.close();
        assert!(!protocol.is_alive());
    }

    #[test]
    fn production_jsonl_transport_reports_interrupted_child_as_dead() {
        let (executable, args) = fake_jsonl_process_command(true);
        let transport = ProcessJsonlTransport::start_command(executable, args, None).unwrap();
        let mut protocol = AppServerProtocol::with_timeout(transport, Duration::from_secs(30));
        protocol.initialized = true;

        let error = protocol
            .request("account/read", json!({"refreshToken": false}))
            .unwrap_err()
            .to_string();

        assert!(error.contains("closed during account/read"), "{error}");
        assert!(!protocol.is_alive());
    }

    #[test]
    fn account_type_must_be_chatgpt_for_analysis() {
        let status =
            parse_account_status(&json!({"account": {"type": "apiKey", "authenticated": true}}));
        assert!(status.authenticated);
        assert_ne!(status.auth_mode, "chatgpt");

        let current_chatgpt = parse_account_status(&json!({"account": {"type": "chatgpt"}}));
        assert!(current_chatgpt.authenticated);

        let incomplete = parse_account_status(&json!({"account": {}}));
        assert!(!incomplete.authenticated);
    }

    #[test]
    fn rate_limit_response_marks_reached_buckets_as_limited() {
        assert!(rate_limits_limited(&json!({
            "rateLimits": {
                "primary": {"usedPercent": 100},
                "rateLimitReachedType": "primary"
            }
        })));
        assert!(!rate_limits_limited(&json!({
            "rateLimits": {"primary": {"usedPercent": 99}}
        })));
    }

    #[test]
    fn account_read_integrates_rate_limit_state_without_persisting_account_values() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([
                response(1, json!({"account": {"type": "chatgpt"}})),
                response(2, json!({"rateLimits": {"primary": {"usedPercent": 100}}})),
            ]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let status = protocol.account_read(false).unwrap();

        assert!(status.authenticated);
        assert!(status.usage_limited);
        assert_eq!(protocol.transport.sent[0]["method"], "account/read");
        assert_eq!(
            protocol.transport.sent[1]["method"],
            "account/rateLimits/read"
        );
    }

    #[test]
    fn account_read_returns_a_sanitized_process_error_for_rate_limit_failures() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([
                response(1, json!({"account": {"type": "chatgpt"}})),
                json!({
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "owner@example.com account_id=acct_private token=secret"
                    }
                }),
            ]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let error = protocol.account_read(false).unwrap_err();

        assert!(matches!(error, AppError::Process(_)));
        let safe_error = error.to_string();
        assert!(safe_error.contains("usage state could not be checked"));
        assert!(!safe_error.contains("owner@example.com"));
        assert!(!safe_error.contains("acct_private"));
        assert!(!safe_error.contains("secret"));
        assert!(!safe_error.contains("-32000"));
    }

    #[test]
    fn login_start_uses_distinct_chatgpt_browser_and_device_code_requests() {
        let browser_transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(
                1,
                json!({"login": {"loginId": "browser-1", "authUrl": "https://login.example"}}),
            )]),
            alive: true,
        };
        let mut browser = AppServerProtocol::new(browser_transport);
        browser.initialized = true;
        let browser_start = browser.login_start(false).unwrap();
        assert_eq!(browser_start.login_id.as_deref(), Some("browser-1"));
        assert_eq!(browser.transport.sent[0]["params"]["type"], "chatgpt");
        assert_eq!(
            browser.transport.sent[0]["params"],
            json!({"type": "chatgpt"})
        );

        let device_transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(
                1,
                json!({"login": {"loginId": "device-1", "verificationUrl": "https://device.example", "userCode": "ABCD-EFGH"}}),
            )]),
            alive: true,
        };
        let mut device = AppServerProtocol::new(device_transport);
        device.initialized = true;
        let device_start = device.login_start(true).unwrap();
        assert!(device_start.device_code);
        assert_eq!(
            device.transport.sent[0]["params"]["type"],
            "chatgptDeviceCode"
        );
        assert_eq!(
            device.transport.sent[0]["params"],
            json!({"type": "chatgptDeviceCode"})
        );
        assert!(device.transport.sent[0]["params"].get("apiKey").is_none());
        assert!(device.transport.sent[0]["params"].get("token").is_none());
    }

    #[test]
    fn login_urls_require_https_without_userinfo_or_ports() {
        let valid = parse_login_start(
            &json!({"loginId": "login-1", "authUrl": "https://auth.example/login"}),
            false,
        );
        assert!(valid.error.is_none());
        assert!(valid.auth_url.is_some());

        let invalid = parse_login_start(
            &json!({"loginId": "login-2", "authUrl": "http://auth.example/login"}),
            false,
        );
        assert!(invalid.auth_url.is_none());
        assert!(invalid.error.is_some());
    }

    #[test]
    fn jsonl_reader_bounds_bytes_before_constructing_a_string() {
        let mut reader = Cursor::new(b"{\"id\":1}\r\nnext".to_vec());
        assert_eq!(
            read_bounded_line(&mut reader).unwrap().as_deref(),
            Some("{\"id\":1}")
        );
        assert_eq!(
            read_bounded_line(&mut reader).unwrap().as_deref(),
            Some("next")
        );

        let mut oversized = Cursor::new(vec![b'x'; MAX_JSONL_LINE_BYTES + 1]);
        assert!(read_bounded_line(&mut oversized)
            .unwrap_err()
            .contains("size limit"));
    }

    #[test]
    fn logout_uses_the_codex_owned_account_logout_request() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(1, json!({}))]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        protocol.logout().unwrap();

        assert_eq!(protocol.transport.sent[0]["method"], "account/logout");
        assert_eq!(protocol.transport.sent[0]["params"], json!({}));
        assert!(protocol.transport.sent[0].get("token").is_none());
    }

    #[test]
    fn login_cancel_uses_the_managed_app_server_method_and_login_id() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(1, json!({}))]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        protocol.cancel_login("login-1").unwrap();

        assert_eq!(
            protocol.transport.sent[0],
            json!({
                "id": 1,
                "method": "account/login/cancel",
                "params": {"loginId": "login-1"}
            })
        );
    }

    #[test]
    fn cancelled_login_wait_notifies_app_server_before_returning() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(1, json!({}))]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let error = protocol
            .wait_for_login_with_cancel("login-1", Duration::from_secs(1), || true)
            .unwrap_err()
            .to_string();

        assert!(error.contains("cancelled"));
        assert_eq!(protocol.transport.sent[0]["method"], "account/login/cancel");
        assert_eq!(
            protocol.transport.sent[0]["params"],
            json!({"loginId": "login-1"})
        );
    }

    #[test]
    fn extra_analysis_fields_are_rejected() {
        let request = CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "brief".into(),
            evidence: vec![],
            constraints: json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        };
        let input = analysis_input_sha256(&request).unwrap();
        let value = json!({
            "schema_version": "1.0.0",
            "analysis_id": Uuid::new_v4(),
            "mode": "new_project_identity",
            "input_sha256": input,
            "project_summary": "summary",
            "proposals": [{"key":"project_id","value":"demo","confidence":1.0,"reason":"reason","evidence_refs":[]}],
            "component_recommendations": [],
            "warnings": [],
            "hidden_reasoning": "must not be accepted"
        });
        assert!(validate_analysis_output(value, &request, &input, &[]).is_err());
    }

    #[test]
    fn authoritative_schema_binds_proposal_value_types_to_their_keys() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../docs/schemas/codex-analysis.schema.json"
        ))
        .unwrap();
        let validator = jsonschema::draft202012::new(&schema).unwrap();
        let input = "a".repeat(64);
        let valid = valid_analysis_value(&input);
        assert!(validator.is_valid(&valid));

        let mut invalid_scalar = valid.clone();
        invalid_scalar["proposals"][0]["value"] = json!(["Demo Project"]);
        assert!(!validator.is_valid(&invalid_scalar));

        let mut invalid_list = valid;
        invalid_list["proposals"][5]["value"] = json!("Gameplay");
        assert!(!validator.is_valid(&invalid_list));
    }

    #[test]
    fn analysis_output_requires_the_complete_semantic_proposal_set() {
        let request = CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "brief".into(),
            evidence: vec![],
            constraints: json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        };
        let input = analysis_input_sha256(&request).unwrap();
        let value = json!({
            "schema_version": CODEX_SCHEMA_VERSION,
            "analysis_id": Uuid::new_v4(),
            "mode": "new_project_identity",
            "input_sha256": input,
            "project_summary": "summary",
            "proposals": [{"key":"project_id","value":"demo","confidence":1.0,"reason":"reason","evidence_refs":[]}],
            "component_recommendations": [],
            "warnings": []
        });

        let error = validate_analysis_output(value, &request, &input, &[])
            .unwrap_err()
            .to_string();

        assert!(error.contains("required semantic proposals"));
    }

    #[test]
    fn analysis_output_accepts_forward_compatible_component_ids_and_rejects_invalid_ids() {
        let input = "a".repeat(64);
        let request = analysis_request_with_component_registry(&[
            "core.skills",
            "workflow.future_addition",
            "2d.future_component",
        ]);
        assert!(
            validate_analysis_output(valid_analysis_value(&input), &request, &input, &[],).is_ok()
        );

        let mut future_component = valid_analysis_value(&input);
        future_component["component_recommendations"][0]["component_id"] =
            json!("workflow.future_addition");
        assert!(validate_analysis_output(future_component, &request, &input, &[]).is_ok());

        let mut digit_component = valid_analysis_value(&input);
        digit_component["component_recommendations"][0]["component_id"] =
            json!("2d.future_component");
        assert!(validate_analysis_output(digit_component, &request, &input, &[]).is_ok());

        let mut absent_component = valid_analysis_value(&input);
        absent_component["component_recommendations"][0]["component_id"] =
            json!("workflow.not_in_manifest");
        assert!(validate_analysis_output(absent_component, &request, &input, &[]).is_err());

        let mut invalid_component = valid_analysis_value(&input);
        invalid_component["component_recommendations"][0]["component_id"] =
            json!("Workflow/Future");
        assert!(validate_analysis_output(invalid_component, &request, &input, &[]).is_err());

        let mut sensitive_component = valid_analysis_value(&input);
        sensitive_component["component_recommendations"][0]["component_id"] =
            json!("account_id.private");
        let sensitive_request =
            analysis_request_with_component_registry(&["core.skills", "account_id.private"]);
        assert!(
            validate_analysis_output(sensitive_component, &sensitive_request, &input, &[],)
                .is_err()
        );

        for (pointer, value) in [
            ("/project_summary", "Contact user@example.com"),
            ("/proposals/0/reason", "plan_type: plus"),
            ("/component_recommendations/0/reason", "account_id: private"),
        ] {
            let mut account_shaped = valid_analysis_value(&input);
            *account_shaped.pointer_mut(pointer).unwrap() = json!(value);
            assert!(
                validate_analysis_output(account_shaped, &request, &input, &[]).is_err(),
                "{pointer}"
            );
        }

        for value in [
            "owner@example.com",
            "account_id: acct_private",
            "plan_type: plus",
            "usage: 98%",
            "rate-limit: primary",
        ] {
            let mut account_shaped = valid_analysis_value(&input);
            account_shaped["proposals"][4]["value"] = json!(value);
            assert!(
                validate_analysis_output(account_shaped, &request, &input, &[]).is_err(),
                "{value}"
            );
        }

        assert!(reject_sensitive_output_value(&json!({
            "nested": ["safe", {"deeper": "account_id: acct_private"}]
        }))
        .is_err());
    }

    #[test]
    fn analysis_output_rejects_unknown_descriptor_tags_and_internal_notes() {
        let input_sha256 = "a".repeat(64);
        let request = analysis_request_with_component_registry(&["core.skills"]);
        let mut invalid_tag = valid_analysis_value(&input_sha256);
        invalid_tag["proposals"][5]["value"] = json!(["Total Conversion"]);
        assert!(validate_analysis_output(invalid_tag, &request, &input_sha256, &[]).is_err());

        let mut internal_note = valid_analysis_value(&input_sha256);
        internal_note["warnings"] = json!(["The approved input contains no evidence_refs."]);
        assert!(validate_analysis_output(internal_note, &request, &input_sha256, &[],).is_err());
    }

    #[test]
    fn analysis_output_rejects_setup_provider_attribution_in_rendered_values() {
        let input = "a".repeat(64);
        let request = analysis_request_with_component_registry(&["core.skills"]);
        for value in [
            "Prepared by DeepSeek for later development.",
            "A HOI4 project using Claude.",
            "Chosen by the setup assistant.",
        ] {
            let mut attributed = valid_analysis_value(&input);
            attributed["proposals"][4]["value"] = json!(value);
            assert!(validate_analysis_output(attributed, &request, &input, &[]).is_err());
        }
    }

    #[test]
    fn evidence_paths_are_root_contained_and_hashes_are_lowercase() {
        let request = CodexAnalysisRequest {
            mode: "existing_project_semantics".into(),
            brief: String::new(),
            evidence: vec![ApprovedEvidence {
                reference: "f1".into(),
                path: "../secret.txt".into(),
                excerpt: String::new(),
                excerpt_sha256: "a".repeat(64),
                confidence: None,
            }],
            constraints: json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        };
        assert!(validate_analysis_request(&request).is_err());
    }

    #[test]
    fn secret_shaped_evidence_paths_and_references_are_rejected() {
        for (reference, path) in [
            ("f1", "config/api-key.json"),
            ("client_secret", "descriptor.mod"),
            ("f3", "private/id_rsa"),
            ("f4", ".env.production"),
        ] {
            let request = CodexAnalysisRequest {
                mode: "existing_project_semantics".into(),
                brief: String::new(),
                evidence: vec![ApprovedEvidence {
                    reference: reference.into(),
                    path: path.into(),
                    excerpt: String::new(),
                    excerpt_sha256: sha256_bytes(b""),
                    confidence: None,
                }],
                constraints: json!({}),
                analysis_purpose: None,
                project_root: None,
                scan_id: None,
            };
            assert!(
                validate_analysis_request(&request).is_err(),
                "{reference} {path}"
            );
        }
    }

    #[test]
    fn folder_profiles_cannot_target_application_managed_roots() {
        assert!(validate_folder_profile_paths(&["common".into(), "events".into()]).is_ok());
        for folder in [
            ".git",
            ".codex/prompts",
            ".agents/skills",
            ".hoi4-mod-setup",
        ] {
            assert!(
                validate_folder_profile_paths(&[folder.into()]).is_err(),
                "{folder}"
            );
        }
    }

    #[test]
    fn app_server_item_notifications_can_carry_schema_output() {
        let message = json!({
            "method": "item/completed",
            "params": {
                "item": {
                    "type": "agentMessage",
                    "content": [{"type": "output_text", "text": "{\"answer\":\"ok\"}"}]
                }
            }
        });
        assert_eq!(structured_output(&message).unwrap()["answer"], "ok");
    }

    #[test]
    fn current_turn_completion_items_can_carry_schema_output() {
        let message = json!({
            "method": "turn/completed",
            "params": {
                "threadId": "thread-1",
                "turn": {
                    "id": "turn-1",
                    "status": "completed",
                    "items": [{
                        "id": "message-1",
                        "type": "agentMessage",
                        "text": "{\"answer\":\"ok\"}"
                    }]
                }
            }
        });
        assert_eq!(structured_output(&message).unwrap()["answer"], "ok");
    }

    #[test]
    fn failed_turn_reports_the_safe_nested_provider_message() {
        let message = json!({
            "method": "turn/completed",
            "params": {
                "threadId": "thread-1",
                "turn": {
                    "id": "turn-1",
                    "status": "failed",
                    "items": [],
                    "error": {
                        "message": "{\"error\":{\"message\":\"Invalid output schema; Authorization: Bearer private-value\"}}"
                    }
                }
            }
        });

        let error = completed_turn_error(&message, "thread-1", Some("turn-1")).unwrap();

        assert!(error.contains("Invalid output schema"));
        assert!(error.contains("REDACTED"));
        assert!(!error.contains("private-value"));
    }

    #[test]
    fn unrelated_turn_notifications_do_not_match_the_active_turn() {
        let unrelated = json!({
            "method": "turn/completed",
            "params": {"threadId": "other-thread", "turnId": "other-turn"}
        });
        let related = json!({
            "method": "turn/completed",
            "params": {"threadId": "thread-1", "turnId": "turn-1"}
        });

        assert!(!event_matches_turn(&unrelated, "thread-1", Some("turn-1")));
        assert!(event_matches_turn(&related, "thread-1", Some("turn-1")));
    }

    #[test]
    fn correlated_turn_notifications_have_aggregate_count_and_size_limits() {
        let incoming = (0..=MAX_CORRELATED_NOTIFICATIONS)
            .map(|index| {
                json!({
                    "method": "item/started",
                    "params": {
                        "threadId": "thread-1",
                        "turnId": "turn-1",
                        "item": {"id": format!("item-{index}")}
                    }
                })
            })
            .collect::<VecDeque<_>>();
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming,
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);

        let error = protocol
            .drain_notifications(Duration::from_secs(30), "thread-1", Some("turn-1"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("correlated notification count limit"));

        let mut notifications = Vec::new();
        push_bounded_notification(
            &mut notifications,
            json!({"method": "item/started", "payload": "x".repeat(80)}),
            10,
            100,
            "correlated",
        )
        .unwrap_err();
        assert!(notifications.is_empty());
    }

    #[test]
    fn current_turn_wire_shape_uses_nested_turn_identity_and_waits_for_turn_completion() {
        assert_eq!(
            turn_id_from_start_response(&json!({"turn": {"id": "turn-1"}})),
            Some("turn-1".into())
        );
        let item = json!({
            "method": "item/completed",
            "params": {"item": {"threadId": "thread-1", "turnId": "turn-1"}}
        });
        let completed = json!({
            "method": "turn/completed",
            "params": {"turn": {"id": "turn-1", "threadId": "thread-1"}}
        });

        assert!(event_matches_turn(&item, "thread-1", Some("turn-1")));
        assert!(!event_completes_turn(&item, "thread-1", Some("turn-1")));
        assert!(event_completes_turn(&completed, "thread-1", Some("turn-1")));
    }

    #[test]
    fn login_wait_requires_completion_and_account_update_before_reread() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([
                json!({
                    "method": "account/login/completed",
                    "params": {"loginId": "login-1", "success": true, "error": null}
                }),
                json!({
                    "method": "account/updated",
                    "params": {"authMode": "chatgpt", "planType": "plus"}
                }),
                response(1, json!({"account": {"type": "chatgpt"}})),
                response(2, json!({"rateLimits": {"primary": {"usedPercent": 1}}})),
            ]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;
        let status = protocol
            .wait_for_login("login-1", Duration::from_secs(1))
            .unwrap();
        assert!(status.authenticated);
        assert_eq!(status.auth_mode, "chatgpt");
        assert_eq!(protocol.transport.sent[0]["method"], "account/read");
        assert_eq!(
            protocol.transport.sent[1]["method"],
            "account/rateLimits/read"
        );
    }

    #[test]
    fn login_cancellation_is_redacted_and_does_not_trigger_account_reread() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([json!({
                "method": "account/login/completed",
                "params": {
                    "loginId": "login-cancelled",
                    "success": false,
                    "error": "Authorization: Bearer cancelled-device-secret"
                }
            })]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let error = protocol
            .wait_for_login("login-cancelled", Duration::from_secs(1))
            .unwrap_err()
            .to_string();

        assert!(!error.contains("cancelled-device-secret"));
        assert!(error.contains("REDACTED"));
        assert!(protocol.transport.sent.is_empty());
    }

    #[test]
    fn login_failure_is_bounded_and_drops_account_shaped_text() {
        let oversized = "x".repeat(MAX_PROTOCOL_ERROR_DETAIL_CHARS * 2);
        let bounded = account_login_notification(
            &json!({
                "method": "account/login/completed",
                "params": {"loginId": "login-1", "success": false, "error": oversized}
            }),
            "login-1",
        )
        .unwrap();
        let LoginNotification::Failed(bounded) = bounded else {
            panic!("expected a failed login notification");
        };
        assert_eq!(bounded.chars().count(), MAX_PROTOCOL_ERROR_DETAIL_CHARS);

        let private = account_login_notification(
            &json!({
                "method": "account/login/completed",
                "params": {"loginId": "login-1", "success": false, "error": "Account user@example.com could not sign in"}
            }),
            "login-1",
        )
        .unwrap();
        let LoginNotification::Failed(private) = private else {
            panic!("expected a failed login notification");
        };
        assert_eq!(private, "Codex login failed");
    }

    #[test]
    fn login_wait_has_a_bounded_timeout_when_the_server_stays_alive() {
        let mut protocol = AppServerProtocol::new(TimeoutTransport {
            sent: Vec::new(),
            cancellation_response_pending: false,
        });
        protocol.initialized = true;

        let error = protocol
            .wait_for_login("login-timeout", Duration::from_millis(5))
            .unwrap_err()
            .to_string();

        assert!(error.contains("did not complete"));
        assert_eq!(
            protocol.transport.sent[0]["params"]["loginId"],
            "login-timeout"
        );
    }

    #[test]
    fn app_server_interruption_is_reported_without_fabricating_account_state() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::new(),
            alive: false,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let error = protocol.account_read(false).unwrap_err().to_string();

        assert!(error.contains("closed during account/read"));
        assert!(!protocol.is_alive());
    }

    #[test]
    fn analysis_without_schema_constrained_output_is_rejected() {
        let request = CodexAnalysisRequest {
            mode: "new_project_identity".into(),
            brief: "brief".into(),
            evidence: Vec::new(),
            constraints: json!({}),
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
        };
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([
                response(
                    1,
                    json!({"account": {"type": "chatgpt", "authenticated": true}}),
                ),
                response(2, json!({"rateLimits": {"primary": {"usedPercent": 1}}})),
                response(3, json!({"threadId": "thread-1"})),
                response(4, json!({"status": "started"})),
            ]),
            alive: true,
        };
        let mut protocol = AppServerProtocol::new(transport);
        protocol.initialized = true;

        let error = protocol
            .analyze(&request, "gpt-5.6-luna", "xhigh")
            .unwrap_err()
            .to_string();

        assert!(error.contains("no schema-constrained analysis output"));
        assert_eq!(protocol.transport.sent[2]["method"], "thread/start");
        assert_eq!(
            protocol.transport.sent[2]["params"]["sandbox"],
            json!("read-only")
        );
        assert!(protocol.transport.sent[2]["params"]["cwd"]
            .as_str()
            .is_some_and(|path| path.contains("hoi4-mod-setup-analysis-")));
        assert!(protocol.transport.sent[2]["params"]
            .get("runtimeWorkspaceRoots")
            .is_none());
        assert!(protocol.transport.sent[2]["params"]
            .get("sandboxPolicy")
            .is_none());
        let analysis_directory = protocol.transport.sent[2]["params"]["cwd"]
            .as_str()
            .expect("analysis directory")
            .to_owned();
        assert_eq!(protocol.transport.sent[3]["method"], "turn/start");
        assert_eq!(
            protocol.transport.sent[3]["params"]["sandboxPolicy"],
            json!({
                "type": "readOnly",
                "networkAccess": false
            })
        );
        let authoritative_schema: Value = serde_json::from_str(include_str!(
            "../../docs/schemas/codex-analysis.schema.json"
        ))
        .unwrap();
        assert_eq!(
            protocol.transport.sent[3]["params"]["outputSchema"],
            authoritative_schema
        );
        assert!(!std::path::Path::new(&analysis_directory).exists());
    }

    #[test]
    fn analysis_record_serialization_contains_no_account_or_token_metadata() {
        let scan_id = Uuid::new_v4();
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: None,
            reasoning_effort: Some("xhigh".into()),
            optimization_profile: Some("Codex setup analysis".into()),
            analysis_id: Uuid::new_v4(),
            schema_version: CODEX_SCHEMA_VERSION.into(),
            input_sha256: "a".repeat(64),
            output_sha256: "b".repeat(64),
            confirmed_fields: REQUIRED_ANALYSIS_PROPOSAL_KEYS
                .iter()
                .map(|field| (*field).into())
                .collect(),
            confirmed_at: "2026-07-26T00:00:00Z".into(),
            account_identity_persisted: false,
            analysis_purpose: None,
            project_root: Some("C:/Users/private/mod".into()),
            scan_id: Some(scan_id),
            evidence_sha256: None,
            source_revision: Some("a".repeat(40)),
            source_manifest_sha256: Some("c".repeat(64)),
        };
        let value = serde_json::to_value(record).unwrap();

        for forbidden in [
            "email",
            "account_id",
            "accountId",
            "plan_type",
            "usage_limited",
            "access_token",
            "refresh_token",
            "token",
            "project_root",
            "scan_id",
        ] {
            assert!(value.get(forbidden).is_none(), "{forbidden}");
        }
    }

    #[test]
    fn model_prompt_omits_core_only_project_binding() {
        let request = CodexAnalysisRequest {
            mode: "existing_project_semantics".into(),
            brief: "Review the approved project facts.".into(),
            evidence: vec![ApprovedEvidence {
                reference: "finding-1".into(),
                path: "common/test.txt".into(),
                excerpt: "fact".into(),
                excerpt_sha256: sha256_bytes(b"fact"),
                confidence: Some(0.9),
            }],
            constraints: json!({"project_id_pattern": "^[a-z][a-z0-9_]{1,63}$"}),
            analysis_purpose: Some("existing_project_import".into()),
            project_root: Some("C:/Users/private/mod".into()),
            scan_id: Some(Uuid::new_v4()),
        };
        let input_sha256 = analysis_input_sha256(&request).unwrap();
        let prompt = analysis_prompt(&request, &input_sha256).unwrap();

        assert!(prompt.contains("finding-1"));
        assert!(prompt.contains("fresh RFC 4122 UUID"));
        assert!(prompt.contains("descriptor_tags and folder_profile"));
        assert!(prompt.contains("Alternative History, Balance, Events"));
        assert!(prompt.contains("Return warnings only when the user must make a decision"));
        assert!(!prompt.contains("C:/Users/private/mod"));
        assert!(!prompt.contains("scan_id"));
        assert!(!prompt.contains("windows_or_macos"));
    }

    #[test]
    fn unavailable_status_redacts_credential_shaped_process_errors() {
        let status = missing_status("Bearer app-server-secret");

        assert!(status.error.as_deref().unwrap().contains("REDACTED"));
        assert!(!status
            .error
            .as_deref()
            .unwrap()
            .contains("app-server-secret"));
    }

    #[test]
    fn protocol_exposes_supervised_transport_liveness() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::new(),
            alive: false,
        };
        let mut protocol = AppServerProtocol::new(transport);
        assert!(!protocol.is_alive());
    }

    #[test]
    fn confirmation_is_bound_to_the_exact_analysis_and_proposal_keys() {
        let proposals = vec![
            (ProposalKey::DisplayName, json!("Demo Project")),
            (ProposalKey::ProjectId, json!("demo_project")),
            (ProposalKey::ScriptPrefix, json!("demo")),
            (ProposalKey::PrimaryNamespace, json!("demo")),
            (ProposalKey::ProjectDescription, json!("A demo project.")),
            (ProposalKey::DescriptorTags, json!(["Gameplay"])),
            (ProposalKey::FolderProfile, json!(["common"])),
            (ProposalKey::AgentsProfile, json!("default")),
            (ProposalKey::LocalisationConvention, json!("english")),
            (ProposalKey::DocumentationConvention, json!("markdown")),
        ];
        let analysis = CodexAnalysis {
            schema_version: CODEX_SCHEMA_VERSION.into(),
            analysis_id: Uuid::new_v4(),
            mode: AnalysisMode::NewProjectIdentity,
            input_sha256: "1".repeat(64),
            project_summary: "summary".into(),
            proposals: proposals
                .into_iter()
                .map(|(key, value)| CodexProposal {
                    key,
                    value,
                    confidence: 1.0,
                    reason: "reason".into(),
                    evidence_refs: vec![],
                })
                .collect(),
            component_recommendations: vec![],
            warnings: vec![],
        };
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: None,
            reasoning_effort: Some("xhigh".into()),
            optimization_profile: Some("Codex setup analysis".into()),
            analysis_id: analysis.analysis_id,
            schema_version: analysis.schema_version.clone(),
            input_sha256: analysis.input_sha256.clone(),
            output_sha256: sha256_bytes(&serde_json::to_vec(&analysis).unwrap()),
            confirmed_fields: vec![],
            confirmed_at: "pending".into(),
            account_identity_persisted: false,
            analysis_purpose: None,
            project_root: None,
            scan_id: None,
            evidence_sha256: None,
            source_revision: Some("a".repeat(40)),
            source_manifest_sha256: Some("c".repeat(64)),
        };
        assert!(confirm_analysis_record(&record, &analysis, &["project_id".into()]).is_err());
        let all_fields = REQUIRED_ANALYSIS_PROPOSAL_KEYS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>();
        let confirmed = confirm_analysis_record(&record, &analysis, &all_fields).unwrap();
        assert_eq!(confirmed.confirmed_fields, all_fields);
        assert!(confirm_analysis_record(&record, &analysis, &["project_root".into()]).is_err());
    }
}
