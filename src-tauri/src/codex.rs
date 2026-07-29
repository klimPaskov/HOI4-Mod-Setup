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
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;
use uuid::Uuid;

pub const CODEX_SCHEMA_VERSION: &str = "1.0.0";
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
const MAX_BRIEF_BYTES: usize = 32 * 1024;
const MAX_EVIDENCE_ITEMS: usize = 256;
const MAX_EVIDENCE_BYTES: usize = 512 * 1024;

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
}

impl<T: JsonlTransport> AppServerProtocol<T> {
    pub fn new(transport: T) -> Self {
        Self::with_timeout(transport, Duration::from_secs(120))
    }

    pub fn with_timeout(transport: T, request_timeout: Duration) -> Self {
        Self {
            transport,
            next_id: 1,
            notifications: Vec::new(),
            request_timeout: request_timeout.max(Duration::from_millis(100)),
        }
    }

    pub fn is_alive(&mut self) -> bool {
        self.transport.is_alive()
    }

    pub fn initialize(&mut self) -> Result<Value, AppError> {
        let result = self.request(
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
                Err(error) => {
                    status.error = Some(format!(
                        "Codex rate-limit state could not be checked: {error}"
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
            json!({
                "type": "chatgpt",
                "useHostedLoginSuccessPage": true,
                "appBrand": "chatgpt"
            })
        };
        let result = self.request("account/login/start", params)?;
        Ok(parse_login_start(&result, device_code))
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
        if login_id.trim().is_empty() || login_id.len() > 128 {
            return Err(AppError::InvalidInput("Codex login ID is invalid".into()));
        }
        let started = std::time::Instant::now();
        let mut login_completed = false;
        let mut account_updated = false;
        let mut pending = std::mem::take(&mut self.notifications);
        loop {
            if is_cancelled() {
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
                return Err(AppError::Credential(
                    "Codex login did not complete during the bounded wait".into(),
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
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.transport
            .send(&json!({"id": id, "method": method, "params": params}))?;
        loop {
            let message = self
                .transport
                .receive(self.request_timeout)?
                .ok_or_else(|| {
                    AppError::Process(format!("Codex App Server closed during {method}"))
                })?;
            if message.get("method").is_some() && message.get("id").is_none() {
                if self.notifications.len() < 128 {
                    self.notifications.push(message);
                }
                continue;
            }
            if message.get("id") != Some(&json!(id)) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let code = error
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("request_failed");
                return Err(AppError::Process(format!(
                    "Codex App Server {method} failed: {code}"
                )));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    pub fn analyze(
        &mut self,
        request: &CodexAnalysisRequest,
    ) -> Result<CodexAnalysisResult, AppError> {
        validate_analysis_request(request)?;
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
        let thread = self.request(
            "thread/start",
            json!({
                "approvalPolicy": "never",
                "sandboxPolicy": read_only_no_project_access()
            }),
        )?;
        let thread_id = thread_id(&thread)?;
        let input_sha256 = analysis_input_sha256(request)?;
        let prompt =
            analysis_prompt_for_provider(request, &input_sha256, "Codex project and ChatGPT Chat")?;
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
                "approvalPolicy": "never"
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
        let output = turn_completed
            .then(|| messages.iter().rev().find_map(structured_output))
            .flatten()
            .ok_or_else(|| {
                AppError::Serialization(
                    "Codex returned no schema-constrained analysis output".into(),
                )
            })?;
        let analysis =
            validate_analysis_output(output, &request.mode, &input_sha256, &request.evidence)?;
        let output_bytes = serde_json::to_vec(&analysis)?;
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: None,
            optimization_profile: Some("Codex project and ChatGPT Chat".into()),
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
        };
        Ok(CodexAnalysisResult { analysis, record })
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
                messages.push(message);
            }
        }
        let started = std::time::Instant::now();
        while started.elapsed() < max_wait {
            match self.transport.receive(Duration::from_millis(200)) {
                Ok(Some(message)) => {
                    let correlated = event_matches_turn(&message, thread_id, turn_id);
                    let complete = event_completes_turn(&message, thread_id, turn_id);
                    if correlated {
                        messages.push(message);
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
        "access": {
            "type": "restricted",
            "includePlatformDefaults": false,
            "readableRoots": []
        }
    })
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
    validate_analysis_output(value, &requested_mode, &input_sha256, &[]).map(|_| ())
}

pub(crate) fn analysis_prompt_for_provider(
    request: &CodexAnalysisRequest,
    input_sha256: &str,
    optimization_profile: &str,
) -> Result<String, AppError> {
    let input = serde_json::to_string(&model_visible_analysis_input(request))?;
    Ok(format!(
        "Interpret this approved HOI4 setup input using the {optimization_profile} conventions. Return only an object matching the supplied output schema. Propose values with concise reasons and evidence_refs. Evidence refs must use only the supplied approved reference IDs; never invent paths or references. Do not read files, perform filesystem writes, execute commands, make network actions, or disclose account data. Input SHA-256 must be copied exactly.\n\ninput_sha256={input_sha256}\ninput={input}"
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
    analysis_prompt_for_provider(request, input_sha256, "Codex project and ChatGPT Chat")
}

pub(crate) fn validate_analysis_output(
    value: Value,
    requested_mode: &str,
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
            && requested_mode != "new_project_identity")
        || (analysis.mode == AnalysisMode::ExistingProjectSemantics
            && requested_mode != "existing_project_semantics")
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
    if analysis
        .component_recommendations
        .iter()
        .any(|recommendation| {
            recommendation.component_id.trim().is_empty()
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
    for warning in &analysis.warnings {
        validate_output_text(warning, "Codex warning")?;
    }
    for recommendation in &analysis.component_recommendations {
        validate_output_text(&recommendation.reason, "Codex recommendation reason")?;
    }
    Ok(analysis)
}

fn validate_output_text(value: &str, label: &str) -> Result<(), AppError> {
    if redact_secrets(value, &[]) != value {
        return Err(AppError::Serialization(format!(
            "{label} contains credential-shaped content"
        )));
    }
    Ok(())
}

fn reject_sensitive_output_value(value: &Value) -> Result<(), AppError> {
    fn contains_sensitive_key(value: &Value) -> bool {
        match value {
            Value::Object(object) => object.iter().any(|(key, child)| {
                matches!(
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
                ) || contains_sensitive_key(child)
            }),
            Value::Array(values) => values.iter().any(contains_sensitive_key),
            _ => false,
        }
    }

    let serialized = serde_json::to_string(value)?;
    if contains_sensitive_key(value) || redact_secrets(&serialized, &[]) != serialized {
        return Err(AppError::Serialization(
            "Codex proposal contains account or credential-shaped content".into(),
        ));
    }
    Ok(())
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
        }
        ProposalKey::DescriptorTags => {
            let tags = value.as_array().ok_or_else(|| {
                AppError::Serialization("Codex descriptor tags proposal must be an array".into())
            })?;
            if tags.len() > 32
                || tags.iter().any(|tag| {
                    tag.as_str().is_none_or(|tag| {
                        tag.trim().is_empty()
                            || tag.len() > 64
                            || tag.contains('\0')
                            || tag.contains('\n')
                            || tag.contains('\r')
                    })
                })
            {
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

fn parse_login_start(value: &Value, device_code: bool) -> CodexLoginStart {
    let value = value.get("login").unwrap_or(value);
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
    CodexLoginStart {
        available: true,
        login_id: value
            .get("loginId")
            .or_else(|| value.get("login_id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        auth_url: raw_auth_url
            .filter(|url| is_safe_login_url(url))
            .map(ToOwned::to_owned),
        verification_url: raw_verification_url
            .filter(|url| is_safe_login_url(url))
            .map(ToOwned::to_owned),
        user_code: value
            .get("userCode")
            .or_else(|| value.get("user_code"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        device_code,
        error: invalid_url.then(|| "Codex returned an invalid HTTPS authentication URL".into()),
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
                    .unwrap_or("Codex login failed")
                    .to_string();
                Ok(LoginNotification::Failed(redact_secrets(&error, &[])))
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
    let bases = [
        Some(value),
        value.get("params"),
        value.get("params").and_then(|params| params.get("item")),
        value.get("result"),
        value.get("item"),
        value.get("turn"),
        value.get("event"),
    ];
    for base in bases.into_iter().flatten() {
        for candidate in [
            base.get("structuredOutput"),
            base.get("structured_output"),
            base.get("output"),
            base.get("text"),
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
        if let Some(content) = base.get("content").and_then(Value::as_array) {
            for item in content {
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
}

impl ProcessJsonlTransport {
    pub fn start(executable: PathBuf) -> Result<Self, AppError> {
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
        spawn_line_reader(stdout, sender);
        Ok(Self {
            child: Some(child),
            stdin,
            lines: receiver,
        })
    }
}

fn spawn_line_reader(stdout: ChildStdout, sender: Sender<Result<String, String>>) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_bounded_line(&mut reader) {
                Ok(Some(line)) => {
                    if sender.send(Ok(line)).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }
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
        self.stdin.write_all(&line)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Value>, AppError> {
        match self.lines.recv_timeout(timeout) {
            Ok(Ok(line)) => serde_json::from_str(&line).map(Some).map_err(|_| {
                AppError::Serialization("Codex App Server returned invalid JSONL".into())
            }),
            Ok(Err(error)) => Err(AppError::Process(error)),
            Err(RecvTimeoutError::Timeout) => {
                Err(AppError::Process("Codex App Server timed out".into()))
            }
            Err(RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    fn close(&mut self) {
        if let Some(mut child) = self.child.take() {
            crate::process::terminate_process_tree(&mut child);
            let _ = child.wait();
        }
    }

    fn is_alive(&mut self) -> bool {
        self.child
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
    let path = std::env::var_os("PATH")?;
    let names: &[&str] = if cfg!(target_os = "windows") {
        &["codex.exe"]
    } else {
        &["codex"]
    };
    std::env::split_paths(&path)
        .flat_map(|entry| names.iter().map(move |name| entry.join(name)))
        .find_map(|candidate| {
            let metadata = std::fs::symlink_metadata(&candidate).ok()?;
            if is_link_metadata(&metadata) || !metadata.is_file() {
                return None;
            }
            let canonical = std::fs::canonicalize(candidate).ok()?;
            if crate::security::path_has_link_component(&canonical) {
                return None;
            }
            Some(canonical)
        })
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

    struct TimeoutTransport;

    impl JsonlTransport for TimeoutTransport {
        fn send(&mut self, _value: &Value) -> Result<(), AppError> {
            Ok(())
        }

        fn receive(&mut self, _timeout: Duration) -> Result<Option<Value>, AppError> {
            Err(AppError::Process("receive timed out".into()))
        }

        fn close(&mut self) {}
    }

    fn response(id: u64, result: Value) -> Value {
        json!({"id": id, "result": result})
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
        let browser_start = browser.login_start(false).unwrap();
        assert_eq!(browser_start.login_id.as_deref(), Some("browser-1"));
        assert_eq!(browser.transport.sent[0]["params"]["type"], "chatgpt");
        assert_eq!(
            browser.transport.sent[0]["params"]["useHostedLoginSuccessPage"],
            true
        );
        assert_eq!(browser.transport.sent[0]["params"]["appBrand"], "chatgpt");

        let device_transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::from([response(
                1,
                json!({"login": {"loginId": "device-1", "verificationUrl": "https://device.example", "userCode": "ABCD-EFGH"}}),
            )]),
            alive: true,
        };
        let mut device = AppServerProtocol::new(device_transport);
        let device_start = device.login_start(true).unwrap();
        assert!(device_start.device_code);
        assert_eq!(
            device.transport.sent[0]["params"]["type"],
            "chatgptDeviceCode"
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

        protocol.logout().unwrap();

        assert_eq!(protocol.transport.sent[0]["method"], "account/logout");
        assert_eq!(protocol.transport.sent[0]["params"], json!({}));
        assert!(protocol.transport.sent[0].get("token").is_none());
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
        assert!(validate_analysis_output(value, "new_project_identity", &input, &[]).is_err());
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

        let error = validate_analysis_output(value, "new_project_identity", &input, &[])
            .unwrap_err()
            .to_string();

        assert!(error.contains("required semantic proposals"));
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

        let error = protocol
            .wait_for_login("login-cancelled", Duration::from_secs(1))
            .unwrap_err()
            .to_string();

        assert!(!error.contains("cancelled-device-secret"));
        assert!(error.contains("REDACTED"));
        assert!(protocol.transport.sent.is_empty());
    }

    #[test]
    fn login_wait_has_a_bounded_timeout_when_the_server_stays_alive() {
        let mut protocol = AppServerProtocol::new(TimeoutTransport);

        let error = protocol
            .wait_for_login("login-timeout", Duration::from_millis(5))
            .unwrap_err()
            .to_string();

        assert!(error.contains("did not complete"));
    }

    #[test]
    fn app_server_interruption_is_reported_without_fabricating_account_state() {
        let transport = FakeTransport {
            sent: Vec::new(),
            incoming: VecDeque::new(),
            alive: false,
        };
        let mut protocol = AppServerProtocol::new(transport);

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

        let error = protocol.analyze(&request).unwrap_err().to_string();

        assert!(error.contains("no schema-constrained analysis output"));
        assert_eq!(protocol.transport.sent[2]["method"], "thread/start");
        assert_eq!(protocol.transport.sent[3]["method"], "turn/start");
        assert!(protocol.transport.sent[3]["params"]
            .get("outputSchema")
            .is_some());
    }

    #[test]
    fn analysis_record_serialization_contains_no_account_or_token_metadata() {
        let scan_id = Uuid::new_v4();
        let record = CodexAnalysisRecord {
            engine: "codex_app_server".into(),
            auth_mode: "chatgpt".into(),
            provider: Some("codex".into()),
            model: None,
            optimization_profile: Some("Codex project and ChatGPT Chat".into()),
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
            constraints: json!({"platform": "windows_or_macos"}),
            analysis_purpose: Some("existing_project_import".into()),
            project_root: Some("C:/Users/private/mod".into()),
            scan_id: Some(Uuid::new_v4()),
        };
        let input_sha256 = analysis_input_sha256(&request).unwrap();
        let prompt = analysis_prompt(&request, &input_sha256).unwrap();

        assert!(prompt.contains("finding-1"));
        assert!(!prompt.contains("C:/Users/private/mod"));
        assert!(!prompt.contains("scan_id"));
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
            optimization_profile: Some("Codex project and ChatGPT Chat".into()),
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
