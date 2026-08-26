//! Provider-neutral semantic analysis adapters.
//!
//! Codex remains the default and keeps its official App Server ownership. All
//! known hosted providers use checked-in verified defaults and an OS-vault
//! credential, while local and custom routes use explicit addresses. The
//! project never receives a secret or a raw
//! provider response; only the schema-validated proposal record crosses into
//! the planning boundary.

use crate::codex::{
    analysis_input_sha256, analysis_prompt_for_provider, validate_analysis_output,
    AiAnalysisRequest, CodexAnalysisResult,
};
use crate::credentials::{validate_ai_provider_credential_for, CredentialStore};
use crate::models::{AiModelOption, AiProviderProfile, CredentialReference};
use crate::security::redact_secrets;
use crate::AppError;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENDPOINT_LENGTH: usize = 2048;
const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANALYSIS_SCHEMA: &str = include_str!("../../docs/schemas/codex-analysis.schema.json");

#[derive(Debug, Clone)]
pub struct AiProviderConfig {
    pub provider: String,
    pub model: String,
    pub reasoning_effort: String,
    pub endpoint: String,
    pub credential_reference: Option<CredentialReference>,
}

pub fn provider_profiles() -> Vec<AiProviderProfile> {
    [
        (
            "codex",
            "Codex",
            "codex_app_server",
            false,
            "Codex project and ChatGPT Chat",
            Some("gpt-5.6-luna"),
            Some("xhigh"),
            None,
            None,
        ),
        (
            "claude",
            "Claude",
            "anthropic_messages",
            true,
            "Claude Code / Anthropic conventions",
            Some("claude-sonnet-5"),
            Some("high"),
            Some("https://api.anthropic.com/v1/messages"),
            Some("https://platform.claude.com/settings/keys"),
        ),
        (
            "kimi",
            "Kimi",
            "openai_compatible",
            true,
            "Kimi coding conventions",
            Some("kimi-k2.6"),
            Some("high"),
            Some("https://api.moonshot.ai/v1/chat/completions"),
            Some("https://platform.kimi.ai/console/api-keys"),
        ),
        (
            "glm",
            "GLM",
            "openai_compatible",
            true,
            "GLM coding conventions",
            Some("glm-5.2"),
            Some("high"),
            Some("https://open.bigmodel.cn/api/paas/v4/chat/completions"),
            Some("https://bigmodel.cn/usercenter/proj-mgmt/apikeys"),
        ),
        (
            "deepseek",
            "DeepSeek",
            "openai_compatible",
            true,
            "DeepSeek coding conventions",
            Some("deepseek-v4-flash"),
            Some("high"),
            Some("https://api.deepseek.com/chat/completions"),
            Some("https://platform.deepseek.com/api_keys"),
        ),
        (
            "local",
            "Local model",
            "openai_compatible",
            false,
            "Local model conventions",
            None,
            Some("high"),
            None,
            None,
        ),
        (
            "custom",
            "Other provider",
            "openai_compatible",
            true,
            "User-supplied provider conventions",
            None,
            Some("high"),
            None,
            None,
        ),
    ]
    .into_iter()
    .map(
        |(
            id,
            display_name,
            protocol,
            requires_credential,
            optimization_profile,
            default_model,
            default_reasoning_effort,
            default_endpoint,
            account_url,
        )| {
            AiProviderProfile {
                id: id.into(),
                display_name: display_name.into(),
                protocol: protocol.into(),
                requires_credential,
                optimization_profile: optimization_profile.into(),
                default_model: default_model.map(str::to_owned),
                default_reasoning_effort: default_reasoning_effort.map(str::to_owned),
                default_endpoint: default_endpoint.map(str::to_owned),
                account_url: account_url.map(str::to_owned),
            }
        },
    )
    .collect()
}

pub fn profile(provider: &str) -> Option<AiProviderProfile> {
    provider_profiles()
        .into_iter()
        .find(|item| item.id == provider)
}

pub fn validate_config(config: &AiProviderConfig) -> Result<AiProviderProfile, AppError> {
    let profile = profile(config.provider.trim()).ok_or_else(|| {
        AppError::InvalidInput(format!("unsupported AI provider: {}", config.provider))
    })?;
    if config.model.trim().is_empty() || config.model.len() > 256 {
        return Err(AppError::InvalidInput(
            "AI model must be non-empty and at most 256 bytes".into(),
        ));
    }
    if config.model.chars().any(|character| character.is_control()) {
        return Err(AppError::InvalidInput(
            "AI model contains a control character".into(),
        ));
    }
    if redact_secrets(config.model.trim(), &[]) != config.model.trim() {
        return Err(AppError::Credential(
            "AI model contains credential-shaped content".into(),
        ));
    }
    validate_reasoning_effort(&config.reasoning_effort)?;
    if config.provider == "codex" {
        validate_endpoint_for_provider(&config.provider, Some(config.endpoint.as_str()))?;
        return Ok(profile);
    }
    validate_endpoint_for_provider(&config.provider, Some(config.endpoint.as_str()))?;
    if profile.requires_credential && config.credential_reference.is_none() {
        return Err(AppError::Credential(
            "connect the selected AI provider before semantic analysis".into(),
        ));
    }
    if let Some(reference) = config.credential_reference.as_ref() {
        validate_ai_provider_credential_for(reference, &config.provider)?;
    }
    Ok(profile)
}

pub fn validate_reasoning_effort(value: &str) -> Result<(), AppError> {
    if matches!(value.trim(), "low" | "medium" | "high" | "xhigh" | "max") {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "reasoning effort must be low, medium, high, xhigh, or max".into(),
        ))
    }
}

/// Validate the non-secret endpoint policy at the plan boundary. The full
/// runtime config validator additionally checks the OS-vault reference; this
/// helper intentionally does not need to read or serialize that secret.
pub fn validate_endpoint_for_provider(
    provider: &str,
    endpoint: Option<&str>,
) -> Result<(), AppError> {
    let profile = profile(provider.trim())
        .ok_or_else(|| AppError::InvalidInput("unsupported AI provider".into()))?;
    let value = endpoint.unwrap_or_default().trim();
    if provider == "codex" {
        if !value.is_empty() {
            return Err(AppError::InvalidInput(
                "Codex uses the local App Server and does not accept a provider endpoint".into(),
            ));
        }
        return Ok(());
    }
    if value.is_empty() || value.len() > MAX_ENDPOINT_LENGTH {
        return Err(AppError::InvalidInput(
            "AI provider endpoint is required and bounded".into(),
        ));
    }
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| AppError::InvalidInput("AI provider endpoint must be a valid URL".into()))?;
    if parsed.username() != "" || parsed.password().is_some() {
        return Err(AppError::Credential(
            "provider endpoint must not contain embedded credentials".into(),
        ));
    }
    let is_loopback = parsed
        .host_str()
        .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
    if provider == "local" && (!is_loopback || parsed.scheme() != "http") {
        return Err(AppError::InvalidInput(
            "local model endpoints must use loopback HTTP".into(),
        ));
    }
    if provider != "local" && parsed.scheme() != "https" {
        return Err(AppError::InvalidInput(
            "remote AI provider endpoints must use HTTPS".into(),
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(AppError::InvalidInput(
            "AI provider endpoints must not contain a query or fragment".into(),
        ));
    }
    if redact_secrets(parsed.as_str(), &[]) != parsed.as_str() {
        return Err(AppError::Credential(
            "AI provider endpoint contains credential-shaped content".into(),
        ));
    }
    let _ = profile;
    Ok(())
}

pub fn account_status<S: CredentialStore>(
    store: &S,
    config: &AiProviderConfig,
) -> crate::codex::AiAccountStatus {
    let profile = match validate_config(config) {
        Ok(profile) => profile,
        Err(error) => {
            return crate::codex::AiAccountStatus {
                available: false,
                authenticated: false,
                provider: config.provider.clone(),
                model: config.model.clone(),
                auth_mode: "unconfigured".into(),
                usage_limited: false,
                error: Some(redact_secrets(&error.to_string(), &[])),
            }
        }
    };
    let authenticated = if profile.requires_credential {
        config
            .credential_reference
            .as_ref()
            .and_then(|reference| store.read(reference).ok())
            .is_some_and(|value| !value.trim().is_empty())
    } else {
        true
    };
    crate::codex::AiAccountStatus {
        available: true,
        authenticated,
        provider: config.provider.clone(),
        model: config.model.clone(),
        auth_mode: if profile.requires_credential {
            "api_key".into()
        } else {
            "local_or_user_configured".into()
        },
        usage_limited: false,
        error: (!authenticated).then(|| "the selected provider credential is not available".into()),
    }
}

pub fn list_models<S: CredentialStore>(
    store: &S,
    config: &AiProviderConfig,
) -> Result<Vec<AiModelOption>, AppError> {
    let profile = validate_config(config)?;
    if config.provider == "codex" {
        return Err(AppError::InvalidInput(
            "Codex models use the App Server catalog".into(),
        ));
    }
    let mut url = reqwest::Url::parse(&config.endpoint)
        .map_err(|_| AppError::InvalidInput("AI provider endpoint must be a valid URL".into()))?;
    let path = url.path().trim_end_matches('/');
    let base = path
        .strip_suffix("/chat/completions")
        .or_else(|| path.strip_suffix("/messages"))
        .or_else(|| path.strip_suffix("/responses"))
        .unwrap_or(path);
    url.set_path(&format!("{base}/models"));
    let secret = if profile.requires_credential {
        Some(
            store.read(config.credential_reference.as_ref().ok_or_else(|| {
                AppError::Credential("provider credential reference is missing".into())
            })?)?,
        )
    } else {
        None
    };
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            AppError::Process(format!("AI provider model client could not start: {error}"))
        })?;
    let mut request = client.get(url).header(ACCEPT, "application/json");
    if profile.protocol == "anthropic_messages" {
        request = request.header("anthropic-version", ANTHROPIC_VERSION);
        if let Some(secret) = secret.as_deref() {
            request = request.header("x-api-key", secret);
        }
    } else if let Some(secret) = secret.as_deref() {
        request = request.bearer_auth(secret);
    }
    let response = request
        .send()
        .map_err(|error| AppError::Process(format!("AI provider model list failed: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Process(format!(
            "AI provider model list returned HTTP {}",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    response
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::Process(format!("AI provider model list failed: {error}")))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(AppError::Serialization(
            "AI provider model list exceeded the bounded response limit".into(),
        ));
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Serialization("AI provider model list omitted data".into()))?;
    let mut models = entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.trim();
            if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
                return None;
            }
            let efforts = advertised_efforts(entry)
                .unwrap_or_else(|| supported_efforts(&config.provider, id));
            let profile_default = profile
                .default_reasoning_effort
                .clone()
                .unwrap_or_else(|| "high".into());
            let default_reasoning_effort = entry
                .get("default_reasoning_effort")
                .or_else(|| entry.get("defaultReasoningEffort"))
                .and_then(Value::as_str)
                .filter(|effort| efforts.iter().any(|candidate| candidate == effort))
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| {
                    if efforts.contains(&profile_default) {
                        profile_default
                    } else {
                        efforts.first().cloned().unwrap_or_else(|| "high".into())
                    }
                });
            Some(AiModelOption {
                id: id.to_owned(),
                display_name: entry
                    .get("display_name")
                    .or_else(|| entry.get("displayName"))
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned(),
                default_reasoning_effort,
                supported_reasoning_efforts: efforts,
            })
        })
        .take(500)
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    models.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(models)
}

fn supported_efforts(provider: &str, model: &str) -> Vec<String> {
    let levels = if provider == "deepseek" || (provider == "claude" && model.contains("sonnet-5")) {
        &["low", "medium", "high", "xhigh", "max"][..]
    } else if provider == "claude"
        && (model.contains("4-6")
            || model.contains("4.6")
            || model.contains("4-7")
            || model.contains("4-8"))
    {
        &["low", "medium", "high", "max"][..]
    } else {
        &["high"][..]
    };
    levels.iter().map(|value| (*value).to_owned()).collect()
}

fn advertised_efforts(entry: &Value) -> Option<Vec<String>> {
    let values = entry
        .get("supported_reasoning_efforts")
        .or_else(|| entry.get("supportedReasoningEfforts"))?
        .as_array()?;
    let mut efforts = values
        .iter()
        .filter_map(|value| {
            value
                .as_str()
                .or_else(|| value.get("reasoningEffort").and_then(Value::as_str))
        })
        .filter(|effort| matches!(*effort, "low" | "medium" | "high" | "xhigh" | "max"))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    efforts.dedup();
    (!efforts.is_empty()).then_some(efforts)
}

pub fn analyze<S: CredentialStore>(
    store: &S,
    config: &AiProviderConfig,
    request: &AiAnalysisRequest,
) -> Result<CodexAnalysisResult, AppError> {
    let profile = validate_config(config)?;
    let input_sha256 = analysis_input_sha256(&request.analysis)?;
    let prompt = analysis_prompt_for_provider(
        &request.analysis,
        &input_sha256,
        &profile.optimization_profile,
    )?;
    let secret = if profile.requires_credential {
        let reference = config.credential_reference.as_ref().ok_or_else(|| {
            AppError::Credential("provider credential reference is missing".into())
        })?;
        Some(store.read(reference)?)
    } else {
        None
    };
    if secret
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(AppError::Credential(
            "provider credential store returned an empty value".into(),
        ));
    }
    let response = request_provider(
        &config.provider,
        &profile.protocol,
        &config.endpoint,
        &config.model,
        &config.reasoning_effort,
        &prompt,
        secret.as_deref(),
    )?;
    let analysis = validate_analysis_output(
        response,
        &request.analysis,
        &input_sha256,
        &request.analysis.evidence,
    )?;
    let output_sha256 = crate::security::sha256_bytes(&serde_json::to_vec(&analysis)?);
    Ok(CodexAnalysisResult {
        analysis: analysis.clone(),
        record: crate::models::CodexAnalysisRecord {
            engine: "provider_api".into(),
            auth_mode: if profile.requires_credential {
                "api_key".into()
            } else {
                "local_endpoint".into()
            },
            provider: Some(config.provider.clone()),
            model: Some(config.model.clone()),
            reasoning_effort: Some(config.reasoning_effort.clone()),
            optimization_profile: Some(profile.optimization_profile),
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

fn request_provider(
    provider: &str,
    protocol: &str,
    endpoint: &str,
    model: &str,
    reasoning_effort: &str,
    prompt: &str,
    secret: Option<&str>,
) -> Result<Value, AppError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            AppError::Process(format!("AI provider client could not start: {error}"))
        })?;
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let system_prompt = format!(
        "Return only one JSON object matching this exact schema. Do not disclose account data, hidden reasoning, credentials, or filesystem content.\n\noutput_schema={ANALYSIS_SCHEMA}"
    );
    let supports_effort = provider == "deepseek"
        || (provider == "claude" && supported_efforts(provider, model).len() > 1);
    let body = if protocol == "anthropic_messages" {
        if let Some(secret) = secret {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(secret).map_err(|_| {
                    AppError::Credential("provider credential contains invalid header bytes".into())
                })?,
            );
        }
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        let mut body = json!({
            "model": model,
            "max_tokens": 8192,
            "system": system_prompt,
            "messages": [{"role": "user", "content": prompt}]
        });
        if supports_effort {
            body["output_config"] = json!({"effort": reasoning_effort});
        }
        body
    } else {
        if let Some(secret) = secret {
            let value = format!("Bearer {secret}");
            headers.insert(
                "authorization",
                HeaderValue::from_str(&value).map_err(|_| {
                    AppError::Credential("provider credential contains invalid header bytes".into())
                })?,
            );
        }
        let mut body = json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": prompt}
            ]
        });
        if supports_effort {
            body["reasoning_effort"] = json!(reasoning_effort);
        }
        if protocol == "openai_compatible" && endpoint.contains("api.deepseek.com") {
            body["thinking"] = json!({"type": "enabled"});
        }
        body
    };
    let response = client
        .post(endpoint)
        .headers(headers)
        .json(&body)
        .send()
        .map_err(|error| AppError::Process(format!("AI provider request failed: {error}")))?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(AppError::Serialization(
            "AI provider response exceeded the bounded response limit".into(),
        ));
    }
    let mut bytes = Vec::new();
    response
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::Process(format!("AI provider response failed: {error}")))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(AppError::Serialization(
            "AI provider response exceeded the bounded response limit".into(),
        ));
    }
    if !status.is_success() {
        return Err(AppError::Process(format!(
            "AI provider returned HTTP {}",
            status.as_u16()
        )));
    }
    let envelope: Value = serde_json::from_slice(&bytes)?;
    extract_structured_output(&envelope)
}

fn extract_structured_output(envelope: &Value) -> Result<Value, AppError> {
    if envelope.get("schema_version").is_some() {
        return Ok(envelope.clone());
    }
    if let Some(content) = envelope
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
    {
        return parse_json_text(content);
    }
    if let Some(content) = envelope.pointer("/content/0/text").and_then(Value::as_str) {
        return parse_json_text(content);
    }
    Err(AppError::Serialization(
        "AI provider returned no structured analysis object".into(),
    ))
}

fn parse_json_text(text: &str) -> Result<Value, AppError> {
    let trimmed = text.trim();
    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    serde_json::from_str(without_fence).map_err(|_| {
        AppError::Serialization("AI provider returned text that is not valid JSON".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryCredentialStore;
    use serde_json::json;

    #[test]
    fn provider_profiles_include_default_and_custom_routes() {
        let profiles = provider_profiles();
        assert_eq!(
            profiles.first().map(|profile| profile.id.as_str()),
            Some("codex")
        );
        assert!(profiles.iter().any(|profile| profile.id == "claude"));
        assert!(profiles.iter().any(|profile| profile.id == "custom"));
        for (id, model, endpoint, account_url) in [
            (
                "claude",
                "claude-sonnet-5",
                "https://api.anthropic.com/v1/messages",
                "https://platform.claude.com/settings/keys",
            ),
            (
                "kimi",
                "kimi-k2.6",
                "https://api.moonshot.ai/v1/chat/completions",
                "https://platform.kimi.ai/console/api-keys",
            ),
            (
                "glm",
                "glm-5.2",
                "https://open.bigmodel.cn/api/paas/v4/chat/completions",
                "https://bigmodel.cn/usercenter/proj-mgmt/apikeys",
            ),
            (
                "deepseek",
                "deepseek-v4-flash",
                "https://api.deepseek.com/chat/completions",
                "https://platform.deepseek.com/api_keys",
            ),
        ] {
            let profile = profiles
                .iter()
                .find(|profile| profile.id == id)
                .unwrap_or_else(|| panic!("missing {id} profile"));
            assert_eq!(profile.default_model.as_deref(), Some(model));
            assert_eq!(profile.default_endpoint.as_deref(), Some(endpoint));
            assert_eq!(profile.account_url.as_deref(), Some(account_url));
        }
        let codex = profiles
            .iter()
            .find(|profile| profile.id == "codex")
            .unwrap();
        assert_eq!(codex.default_model.as_deref(), Some("gpt-5.6-luna"));
        assert_eq!(codex.default_reasoning_effort.as_deref(), Some("xhigh"));
        let deepseek = profiles
            .iter()
            .find(|profile| profile.id == "deepseek")
            .unwrap();
        assert_eq!(deepseek.default_reasoning_effort.as_deref(), Some("high"));
    }

    #[test]
    fn reasoning_effort_is_bounded_and_provider_catalogs_expose_ordered_choices() {
        for effort in ["low", "medium", "high", "xhigh", "max"] {
            assert!(validate_reasoning_effort(effort).is_ok());
        }
        assert!(validate_reasoning_effort("ultra").is_err());
        assert_eq!(
            supported_efforts("deepseek", "deepseek-v4-flash"),
            vec!["low", "medium", "high", "xhigh", "max"]
        );
    }

    #[test]
    fn provider_system_prompt_contains_the_authoritative_analysis_schema() {
        assert!(ANALYSIS_SCHEMA.contains("\"$schema\""));
        assert!(ANALYSIS_SCHEMA.contains("\"proposals\""));
        assert!(ANALYSIS_SCHEMA.contains("\"component_recommendations\""));
    }

    #[test]
    fn remote_endpoints_require_https_and_no_embedded_credentials() {
        let config = AiProviderConfig {
            provider: "deepseek".into(),
            model: "deepseek-chat".into(),
            reasoning_effort: "high".into(),
            endpoint: "http://example.invalid/v1/chat/completions".into(),
            credential_reference: None,
        };
        assert!(validate_config(&config).is_err());
        let mut config = config;
        config.endpoint = "https://user:secret@example.invalid/v1".into();
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn local_loopback_route_can_be_configured_without_a_key() {
        let config = AiProviderConfig {
            provider: "local".into(),
            model: "local-model".into(),
            reasoning_effort: "high".into(),
            endpoint: "http://127.0.0.1:11434/v1/chat/completions".into(),
            credential_reference: None,
        };
        assert!(validate_config(&config).is_ok());
        let status = account_status(&MemoryCredentialStore::default(), &config);
        assert!(status.available);
        assert!(status.authenticated);
    }

    #[test]
    fn local_and_query_routes_are_rejected_before_network_access() {
        let mut config = AiProviderConfig {
            provider: "local".into(),
            model: "local-model".into(),
            reasoning_effort: "high".into(),
            endpoint: "https://provider.example/v1/chat/completions".into(),
            credential_reference: None,
        };
        assert!(validate_config(&config).is_err());
        config.endpoint = "http://127.0.0.1:11434/v1/chat/completions?token=1".into();
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn codex_rejects_endpoints_and_hosted_profiles_require_one() {
        assert!(validate_endpoint_for_provider("codex", Some("https://example.invalid")).is_err());
        assert!(validate_endpoint_for_provider("claude", None).is_err());
        assert!(
            validate_endpoint_for_provider("claude", Some("https://api.example.invalid/v1"))
                .is_ok()
        );
    }

    #[test]
    fn provider_configuration_rejects_secret_shaped_model_and_endpoint() {
        let test_key = format!("{}{}", "sk-", "123456789012345678901234");
        let mut config = AiProviderConfig {
            provider: "deepseek".into(),
            model: test_key.clone(),
            reasoning_effort: "high".into(),
            endpoint: "https://provider.example/v1/chat/completions".into(),
            credential_reference: Some(CredentialReference {
                name: crate::credentials::AI_PROVIDER_ENVIRONMENT_NAME.into(),
                provider: crate::credentials::provider_name(crate::models::Platform::current())
                    .into(),
                reference: "credential://ai_provider_api_key/deepseek".into(),
                provider_id: Some("deepseek".into()),
            }),
        };
        assert!(validate_config(&config).is_err());
        config.model = "deepseek-chat".into();
        config.endpoint = format!("https://provider.example/v1/token/{test_key}");
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn provider_response_extractors_accept_anthropic_and_openai_shapes() {
        let analysis = json!({"schema_version": "1.0.0", "mode": "new_project_identity"});
        assert_eq!(extract_structured_output(&analysis).unwrap(), analysis);
        let encoded = serde_json::to_string(&json!({"schema_version": "1.0.0"})).unwrap();
        assert_eq!(
            extract_structured_output(
                &json!({"choices": [{"message": {"content": format!("```json\n{encoded}\n```")}}]})
            )
            .unwrap(),
            json!({"schema_version": "1.0.0"})
        );
        assert_eq!(
            extract_structured_output(&json!({"content": [{"text": encoded}]})).unwrap(),
            json!({"schema_version": "1.0.0"})
        );
    }
}
