//! Provider-neutral semantic analysis adapters.
//!
//! The Claude account route is the default: it runs the user's own installed
//! Claude Code, which owns sign-in (see `claude_code`). Codex keeps its
//! official App Server ownership. All known hosted API providers use
//! checked-in verified defaults and an OS-vault credential, while local and
//! custom routes use explicit addresses. The
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
use hoi4_setup_core::analysis::{extract_structured_output, ANTHROPIC_VERSION};
use hoi4_setup_core::free_assistant::{
    free_assistant, FREE_AUTH_MODE, FREE_PROVIDER_ID, OPENROUTER_DEFAULT_MODEL,
    OPENROUTER_ENDPOINT, OPENROUTER_PROVIDER_ID,
};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use serde_json::Value;
use std::io::Read;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENDPOINT_LENGTH: usize = 2048;

#[derive(Debug, Clone)]
pub struct AiProviderConfig {
    pub provider: String,
    pub model: String,
    pub reasoning_effort: String,
    pub endpoint: String,
    pub credential_reference: Option<CredentialReference>,
}

/// The free assistant's profile once its relay is deployed; it is then the
/// default for a new setup.
fn free_profile() -> Option<AiProviderProfile> {
    let free = free_assistant()?;
    Some(AiProviderProfile {
        id: FREE_PROVIDER_ID.into(),
        display_name: "Free model".into(),
        protocol: "openai_compatible".into(),
        requires_credential: false,
        optimization_profile: "Free setup analysis".into(),
        default_model: Some(free.model),
        default_reasoning_effort: Some("high".into()),
        default_endpoint: Some(free.endpoint),
        account_url: None,
    })
}

pub fn provider_profiles() -> Vec<AiProviderProfile> {
    let mut profiles: Vec<AiProviderProfile> = free_profile().into_iter().collect();
    profiles.extend(account_and_hosted_profiles());
    profiles
}

fn account_and_hosted_profiles() -> Vec<AiProviderProfile> {
    [
        (
            crate::claude_code::PROVIDER_ID,
            "Claude",
            "claude_code_cli",
            false,
            "Claude account setup analysis",
            Some(crate::claude_code::DEFAULT_MODEL),
            Some("medium"),
            None,
            Some(crate::claude_code::SETUP_URL),
        ),
        (
            "codex",
            "Codex",
            "codex_app_server",
            false,
            "Codex setup analysis",
            Some("gpt-5.6-luna"),
            Some("xhigh"),
            None,
            None,
        ),
        (
            "gemini_account",
            "Gemini (account, experimental)",
            "provider_cli",
            false,
            "Gemini account setup analysis",
            Some(crate::account_cli::default_model("gemini_account")),
            Some("high"),
            None,
            Some("https://geminicli.com/docs/get-started/authentication/"),
        ),
        (
            "grok_account",
            "Grok (account, experimental)",
            "provider_cli",
            false,
            "Grok account setup analysis",
            Some(crate::account_cli::default_model("grok_account")),
            Some("high"),
            None,
            Some("https://docs.x.ai/build/cli/reference"),
        ),
        (
            "claude",
            "Claude API key",
            "anthropic_messages",
            true,
            "Claude setup analysis",
            Some(crate::claude_code::DEFAULT_MODEL),
            Some("medium"),
            Some("https://api.anthropic.com/v1/messages"),
            Some("https://platform.claude.com/settings/keys"),
        ),
        (
            "kimi",
            "Kimi",
            "openai_compatible",
            true,
            "Kimi setup analysis",
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
            "GLM setup analysis",
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
            "DeepSeek setup analysis",
            Some("deepseek-flash"),
            Some("high"),
            Some("https://api.deepseek.com/chat/completions"),
            Some("https://platform.deepseek.com/api_keys"),
        ),
        (
            "gemini",
            "Gemini API key",
            "openai_compatible",
            true,
            "Gemini setup analysis",
            Some("gemini-3.8-flash"),
            Some("high"),
            Some("https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"),
            Some("https://aistudio.google.com/api-keys"),
        ),
        (
            "grok",
            "Grok API key",
            "openai_responses",
            true,
            "Grok setup analysis",
            Some("grok-4.7"),
            Some("high"),
            Some("https://api.x.ai/v1/responses"),
            Some("https://console.x.ai"),
        ),
        (
            OPENROUTER_PROVIDER_ID,
            "OpenRouter",
            "openai_compatible",
            true,
            "OpenRouter setup analysis",
            Some(OPENROUTER_DEFAULT_MODEL),
            Some("high"),
            Some(OPENROUTER_ENDPOINT),
            Some("https://openrouter.ai/settings/keys"),
        ),
        (
            "local",
            "Local model",
            "openai_compatible",
            false,
            "Local setup analysis",
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
            "Custom setup analysis",
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

/// Non-secret integration and authentication labels persisted in readiness
/// reports and project state for a provider ID.
pub fn integration_and_auth_mode(provider: &str) -> (&'static str, &'static str) {
    match provider {
        "codex" => ("codex_app_server", "chatgpt"),
        crate::claude_code::PROVIDER_ID => {
            (crate::claude_code::ENGINE, crate::claude_code::AUTH_MODE)
        }
        "local" => ("provider_api", "local_endpoint"),
        FREE_PROVIDER_ID => ("provider_api", FREE_AUTH_MODE),
        "gemini_account" | "grok_account" => ("provider_cli", "provider_account"),
        _ => ("provider_api", "api_key"),
    }
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
    if config.provider == crate::claude_code::PROVIDER_ID
        || crate::account_cli::is_account_provider(&config.provider)
    {
        crate::claude_code::validate_model(&config.model)?;
    }
    if config.provider == "codex"
        || config.provider == crate::claude_code::PROVIDER_ID
        || crate::account_cli::is_account_provider(&config.provider)
    {
        validate_endpoint_for_provider(&config.provider, Some(config.endpoint.as_str()))?;
        return Ok(profile);
    }
    validate_endpoint_for_provider(&config.provider, Some(config.endpoint.as_str()))?;
    if config.provider == FREE_PROVIDER_ID
        && profile.default_model.as_deref() != Some(config.model.trim())
    {
        return Err(AppError::InvalidInput(
            "the free assistant runs only its checked-in model".into(),
        ));
    }
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
    let provider = provider.trim();
    let profile = profile(provider)
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
    if provider == crate::claude_code::PROVIDER_ID
        || crate::account_cli::is_account_provider(provider)
    {
        if !value.is_empty() {
            return Err(AppError::InvalidInput(
                "account routes use the official provider CLI and do not accept a provider endpoint"
                    .into(),
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
    if provider != "custom" && provider != "local" {
        let expected = profile
            .default_endpoint
            .as_deref()
            .and_then(|value| reqwest::Url::parse(value).ok())
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "the selected hosted provider has no reviewed endpoint".into(),
                )
            })?;
        if parsed.origin() != expected.origin() {
            return Err(AppError::Credential(
                "known provider keys may only be sent to the provider's reviewed HTTPS origin; use the custom provider for another host".into(),
            ));
        }
        if provider == FREE_PROVIDER_ID && parsed != expected {
            return Err(AppError::InvalidInput(
                "the free assistant uses only its checked-in relay address".into(),
            ));
        }
    }
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
    if config.provider == crate::claude_code::PROVIDER_ID {
        return crate::claude_code::account_status(&config.model);
    }
    if crate::account_cli::is_account_provider(&config.provider) {
        return crate::account_cli::account_status(&config.provider, &config.model);
    }
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
        } else if config.provider == FREE_PROVIDER_ID {
            FREE_AUTH_MODE.into()
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
    if config.provider == crate::claude_code::PROVIDER_ID {
        return Ok(crate::claude_code::builtin_models());
    }
    if crate::account_cli::is_account_provider(&config.provider) {
        return Ok(crate::account_cli::builtin_models(&config.provider));
    }
    if config.provider == FREE_PROVIDER_ID {
        // The relay serves exactly one model and no catalog.
        let model = profile.default_model.unwrap_or_default();
        return Ok(vec![AiModelOption {
            id: model.clone(),
            is_default: true,
            display_name: model,
            default_reasoning_effort: "high".into(),
            supported_reasoning_efforts: vec!["high".into()],
        }]);
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
                .or_else(|| {
                    entry
                        .get("effort")
                        .and_then(|effort| effort.get("default_level"))
                })
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
                is_default: false,
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
    if provider == "claude" || provider == crate::claude_code::PROVIDER_ID {
        return crate::claude_code::supported_efforts(model);
    }
    let levels = if provider == "deepseek" {
        &["low", "high", "max"][..]
    } else if matches!(provider, "gemini" | "grok") {
        &["low", "medium", "high"][..]
    } else {
        &["high"][..]
    };
    levels.iter().map(|value| (*value).to_owned()).collect()
}

fn advertised_efforts(entry: &Value) -> Option<Vec<String>> {
    let values = entry
        .get("supported_reasoning_efforts")
        .or_else(|| entry.get("supportedReasoningEfforts"))
        .or_else(|| {
            entry
                .get("effort")
                .and_then(|effort| effort.get("supported_levels"))
        })?
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
    if config.provider == crate::claude_code::PROVIDER_ID {
        return crate::claude_code::analyze(request, &profile.optimization_profile);
    }
    if crate::account_cli::is_account_provider(&config.provider) {
        return crate::account_cli::analyze(request);
    }
    analyze_using(store, config, request, |prompt, secret| {
        request_provider(
            &config.provider,
            &profile.protocol,
            &config.endpoint,
            &config.model,
            &config.reasoning_effort,
            prompt,
            secret,
        )
    })
}

fn analyze_using<S: CredentialStore, F>(
    store: &S,
    config: &AiProviderConfig,
    request: &AiAnalysisRequest,
    mut generate: F,
) -> Result<CodexAnalysisResult, AppError>
where
    F: FnMut(&str, Option<&str>) -> Result<Value, AppError>,
{
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
    // A rejected proposal set gets one corrective request; the deterministic
    // validator is never relaxed.
    let mut turn_prompt = prompt.clone();
    let mut attempt = 0;
    let analysis = loop {
        attempt += 1;
        let validated = generate(&turn_prompt, secret.as_deref()).and_then(|response| {
            validate_analysis_output(
                response,
                &request.analysis,
                &input_sha256,
                &request.analysis.evidence,
            )
        });
        match validated {
            Ok(analysis) => break analysis,
            Err(error) => match crate::codex::correctable_output_error(&error) {
                Some(reason) if attempt < crate::codex::ANALYSIS_ATTEMPTS => {
                    turn_prompt = format!(
                        "{prompt}\n\n{}",
                        crate::codex::corrective_analysis_prompt(reason, &input_sha256)
                    );
                }
                _ => return Err(error),
            },
        }
    };
    let output_sha256 = crate::security::sha256_bytes(&serde_json::to_vec(&analysis)?);
    Ok(CodexAnalysisResult {
        analysis: analysis.clone(),
        record: crate::models::CodexAnalysisRecord {
            engine: "provider_api".into(),
            auth_mode: integration_and_auth_mode(&config.provider).1.into(),
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

fn provider_request_payload(
    provider: &str,
    protocol: &str,
    endpoint: &str,
    model: &str,
    reasoning_effort: &str,
    prompt: &str,
    secret: Option<&str>,
) -> Result<(HeaderMap, Value), AppError> {
    let supports_effort = matches!(provider, "deepseek" | "gemini" | "grok")
        || (provider == "claude" && supported_efforts(provider, model).len() > 1);
    let request = hoi4_setup_core::analysis::provider_request(
        provider,
        protocol,
        endpoint,
        model,
        reasoning_effort,
        prompt,
        supports_effort,
    )?;
    let mut headers = HeaderMap::new();
    for (name, value) in request.headers {
        headers.insert(
            name,
            HeaderValue::from_str(&value)
                .map_err(|_| AppError::Protocol("provider header is invalid".into()))?,
        );
    }
    if let Some(secret) = secret {
        let (name, value) = hoi4_setup_core::analysis::credential_header(protocol, secret);
        headers.insert(
            name,
            HeaderValue::from_str(&value).map_err(|_| {
                AppError::Credential("provider credential contains invalid header bytes".into())
            })?,
        );
    }
    Ok((headers, request.body))
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
    let (headers, body) = provider_request_payload(
        provider,
        protocol,
        endpoint,
        model,
        reasoning_effort,
        prompt,
        secret,
    )?;
    extract_structured_output(&post_provider(provider, endpoint, headers, &body)?)
}

/// Send one request to a reviewed route and return its response envelope.
fn post_provider(
    provider: &str,
    endpoint: &str,
    headers: HeaderMap,
    body: &Value,
) -> Result<Value, AppError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            AppError::Process(format!("AI provider client could not start: {error}"))
        })?;
    let response = client
        .post(endpoint)
        .headers(headers)
        .json(body)
        .send()
        .map_err(|error| AppError::Process(format!("AI provider request failed: {error}")))?;
    let status = response.status();
    // Transport-level failures are Process or Protocol errors so they are
    // never mistaken for a rejected proposal set and retried.
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(AppError::Process(
            "AI provider response exceeded the bounded response limit".into(),
        ));
    }
    let mut bytes = Vec::new();
    response
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::Process(format!("AI provider response failed: {error}")))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(AppError::Process(
            "AI provider response exceeded the bounded response limit".into(),
        ));
    }
    if !status.is_success() {
        if status.as_u16() == 429 {
            return Err(AppError::Process(
                "provider usage is limited; retry when available".into(),
            ));
        }
        if provider == FREE_PROVIDER_ID {
            // The relay holds the maintainer's key; nothing here is the
            // user's credential to fix.
            return Err(AppError::Process(
                "the free assistant is not available right now; try again later or choose another assistant".into(),
            ));
        }
        if matches!(status.as_u16(), 401 | 403) {
            return Err(AppError::Credential(
                "provider authentication was not accepted".into(),
            ));
        }
        return Err(AppError::Process(format!(
            "AI provider returned HTTP {}",
            status.as_u16()
        )));
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        AppError::Protocol("AI provider returned an unreadable response envelope".into())
    })
}

/// The magic wand on the description step: one turn that turns a rough note
/// into a fuller description. Codex drafts through its App Server session in
/// the command layer; every other route is handled here.
pub fn draft_description<S: CredentialStore>(
    store: &S,
    config: &AiProviderConfig,
    note: &str,
) -> Result<String, AppError> {
    let profile = validate_config(config)?;
    if config.provider == "codex" {
        return Err(AppError::InvalidInput(
            "Codex drafts through the official App Server".into(),
        ));
    }
    if config.provider == crate::claude_code::PROVIDER_ID {
        return crate::claude_code::draft_description(
            &config.model,
            &config.reasoning_effort,
            note,
        );
    }
    if crate::account_cli::is_account_provider(&config.provider) {
        return crate::account_cli::draft_description(
            &config.provider,
            &config.model,
            &config.reasoning_effort,
            note,
        );
    }
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
    let (headers, body) = description_payload(&profile.protocol, config, note, secret.as_deref())?;
    hoi4_setup_core::description::read_description_reply(&post_provider(
        &config.provider,
        &config.endpoint,
        headers,
        &body,
    )?)
}

fn description_payload(
    protocol: &str,
    config: &AiProviderConfig,
    note: &str,
    secret: Option<&str>,
) -> Result<(HeaderMap, Value), AppError> {
    let supports_effort = matches!(config.provider.as_str(), "deepseek" | "gemini" | "grok")
        || (config.provider == "claude"
            && supported_efforts(&config.provider, &config.model).len() > 1);
    let request = hoi4_setup_core::description::description_request(
        &config.provider,
        protocol,
        &config.endpoint,
        &config.model,
        &config.reasoning_effort,
        note,
        supports_effort,
    )?;
    let mut headers = HeaderMap::new();
    for (name, value) in request.headers {
        headers.insert(
            name,
            HeaderValue::from_str(&value)
                .map_err(|_| AppError::Protocol("provider header is invalid".into()))?,
        );
    }
    if let Some(secret) = secret {
        let (name, value) = hoi4_setup_core::analysis::credential_header(protocol, secret);
        headers.insert(
            name,
            HeaderValue::from_str(&value).map_err(|_| {
                AppError::Credential("provider credential contains invalid header bytes".into())
            })?,
        );
    }
    Ok((headers, request.body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryCredentialStore;
    use hoi4_setup_core::analysis::ANALYSIS_OUTPUT_SCHEMA as ANALYSIS_SCHEMA;
    use serde_json::json;

    #[test]
    fn provider_profiles_include_default_and_custom_routes() {
        let profiles = provider_profiles();
        // The free assistant leads once its relay is deployed; the Claude
        // account route is the first account route either way.
        let skip = usize::from(free_assistant().is_some());
        if skip == 1 {
            assert_eq!(profiles[0].id, FREE_PROVIDER_ID);
        }
        assert_eq!(
            profiles.get(skip).map(|profile| profile.id.as_str()),
            Some(crate::claude_code::PROVIDER_ID)
        );
        let claude_account = &profiles[skip];
        assert_eq!(
            claude_account.default_model.as_deref(),
            Some("claude-haiku-5-5")
        );
        assert!(!claude_account.requires_credential);
        assert!(claude_account.default_endpoint.is_none());
        assert!(profiles.iter().any(|profile| profile.id == "codex"));
        assert!(profiles.iter().any(|profile| profile.id == "claude"));
        assert!(profiles.iter().any(|profile| profile.id == "custom"));
        for (id, model, endpoint, account_url) in [
            (
                "claude",
                "claude-haiku-5-5",
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
                "deepseek-flash",
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
            supported_efforts("deepseek", "deepseek-flash"),
            vec!["low", "high", "max"]
        );
        assert_eq!(
            advertised_efforts(&json!({
                "id": "deepseek-flash",
                "effort": {"supported_levels": ["low", "high", "max"], "default_level": "high"}
            })),
            Some(vec!["low".into(), "high".into(), "max".into()])
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
            model: "deepseek-flash".into(),
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
                .is_err()
        );
        assert!(validate_endpoint_for_provider(
            "claude",
            Some("https://api.anthropic.com/v1/messages")
        )
        .is_ok());
    }

    #[test]
    fn claude_account_route_uses_no_endpoint_or_api_key() {
        assert!(validate_endpoint_for_provider(crate::claude_code::PROVIDER_ID, None).is_ok());
        assert!(validate_endpoint_for_provider(
            crate::claude_code::PROVIDER_ID,
            Some("https://api.anthropic.com/v1/messages")
        )
        .is_err());
        let config = AiProviderConfig {
            provider: crate::claude_code::PROVIDER_ID.into(),
            model: crate::claude_code::DEFAULT_MODEL.into(),
            reasoning_effort: "high".into(),
            endpoint: String::new(),
            credential_reference: None,
        };
        assert!(validate_config(&config).is_ok());
        let mut injected = config.clone();
        injected.model = "--dangerously-skip-permissions".into();
        assert!(validate_config(&injected).is_err());
        let models = list_models(&MemoryCredentialStore::default(), &config).unwrap();
        assert_eq!(models[0].id, crate::claude_code::DEFAULT_MODEL);
        assert_eq!(
            supported_efforts("claude", crate::claude_code::DEFAULT_MODEL),
            vec!["low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(
            supported_efforts("claude", "claude-haiku-4-5-20251001"),
            vec!["high"]
        );
    }

    #[test]
    fn known_provider_credentials_cannot_be_routed_to_an_unreviewed_origin() {
        assert!(validate_endpoint_for_provider(
            "deepseek",
            Some("https://api.deepseek.com/v1/chat/completions")
        )
        .is_ok());
        for endpoint in [
            "https://attacker.example/v1/chat/completions",
            "https://api.deepseek.com.attacker.example/v1/chat/completions",
            "https://api.deepseek.com:444/v1/chat/completions",
        ] {
            assert!(
                validate_endpoint_for_provider("deepseek", Some(endpoint)).is_err(),
                "accepted unreviewed origin: {endpoint}"
            );
        }
        assert!(validate_endpoint_for_provider(
            "custom",
            Some("https://custom.example/v1/chat/completions")
        )
        .is_ok());
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
        config.model = "deepseek-flash".into();
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

    #[test]
    fn reviewed_api_routes_send_native_output_schemas_without_tools() {
        for id in ["claude", "gemini", "grok"] {
            let profile = profile(id).unwrap();
            let (_, body) = provider_request_payload(
                id,
                &profile.protocol,
                profile.default_endpoint.as_deref().unwrap(),
                profile.default_model.as_deref().unwrap(),
                "high",
                "approved brief",
                Some("fixture-only-value"),
            )
            .unwrap();
            let schema = match id {
                "claude" => &body["output_config"]["format"]["schema"],
                "gemini" => &body["response_format"]["json_schema"]["schema"],
                _ => &body["text"]["format"]["schema"],
            };
            assert_eq!(schema["properties"]["proposals"]["minItems"], 10);
            assert!(body.get("tools").is_none());
            assert!(!body.to_string().contains("fixture-only-value"));
            if id == "grok" {
                assert_eq!(body["store"], false);
            }
        }
        for id in ["gemini", "grok"] {
            let profile = profile(id).unwrap();
            assert!(
                validate_endpoint_for_provider(id, profile.default_endpoint.as_deref()).is_ok()
            );
            assert!(validate_endpoint_for_provider(id, Some("https://other.example/v1")).is_err());
            let config = AiProviderConfig {
                provider: id.into(),
                model: profile.default_model.unwrap(),
                reasoning_effort: "high".into(),
                endpoint: profile.default_endpoint.unwrap(),
                credential_reference: None,
            };
            assert!(!account_status(&MemoryCredentialStore::default(), &config).authenticated);
        }
    }

    #[test]
    fn response_blocks_skip_thinking_and_reject_truncation_before_json() {
        let content = r#"{"schema_version":"1.0.0"}"#;
        let anthropic = json!({"content":[{"type":"thinking","thinking":"not output"},{"type":"text","text":content}]});
        assert_eq!(
            extract_structured_output(&anthropic).unwrap()["schema_version"],
            "1.0.0"
        );
        let responses = json!({"output":[{"type":"reasoning"},{"type":"message","content":[{"type":"output_text","text":content}]}]});
        assert_eq!(
            extract_structured_output(&responses).unwrap()["schema_version"],
            "1.0.0"
        );
        for envelope in [
            json!({"choices":[{"finish_reason":"length","message":{"content":content}}]}),
            json!({"stop_reason":"max_tokens","content":[{"text":content}]}),
            json!({"status":"incomplete","output":responses["output"]}),
            json!({"choices":[{"message":{"content":content,"refusal":"refused"}}]}),
        ] {
            let error = extract_structured_output(&envelope).unwrap_err();
            assert!(crate::codex::correctable_output_error(&error).is_none());
        }
    }

    #[test]
    fn provider_turn_retries_once_and_never_retries_private_or_transport_output() {
        let config = AiProviderConfig {
            provider: "local".into(),
            model: "fixture".into(),
            reasoning_effort: "high".into(),
            endpoint: "http://127.0.0.1:11434/v1/chat/completions".into(),
            credential_reference: None,
        };
        let request = AiAnalysisRequest {
            provider: config.provider.clone(),
            model: config.model.clone(),
            reasoning_effort: config.reasoning_effort.clone(),
            endpoint: config.endpoint.clone(),
            analysis: crate::codex::CodexAnalysisRequest {
                mode: "new_project_identity".into(),
                brief: "A historical mod".into(),
                evidence: Vec::new(),
                constraints: json!({}),
                analysis_purpose: None,
                project_root: None,
                scan_id: None,
            },
        };
        let input = analysis_input_sha256(&request.analysis).unwrap();
        let mut valid: Value = serde_json::from_str(include_str!(
            "../../docs/examples/codex-analysis.example.json"
        ))
        .unwrap();
        valid["input_sha256"] = json!(input);
        valid["analysis_id"] = json!("review");
        valid["component_recommendations"] = json!([]);
        for proposal in valid["proposals"].as_array_mut().unwrap() {
            proposal["evidence_refs"] = json!([]);
        }
        let store = MemoryCredentialStore::default();
        let mut calls = 0;
        let result = analyze_using(&store, &config, &request, |prompt, secret| {
            assert!(secret.is_none());
            calls += 1;
            if calls == 1 {
                Ok(json!({}))
            } else {
                assert!(prompt.contains("previous response was rejected"));
                Ok(valid.clone())
            }
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert!(!result.record.account_identity_persisted);
        for private in [false, true] {
            calls = 0;
            let result = analyze_using(&store, &config, &request, |_, _| {
                calls += 1;
                if private {
                    let mut output = valid.clone();
                    output["project_summary"] = json!("account_id: private");
                    Ok(output)
                } else {
                    Err(AppError::Process("request interrupted".into()))
                }
            });
            assert!(result.is_err());
            assert_eq!(calls, 1);
        }
        calls = 0;
        assert!(analyze_using(&store, &config, &request, |_, _| {
            calls += 1;
            Ok(json!({}))
        })
        .is_err());
        assert_eq!(calls, 2);
    }

    #[test]
    fn openrouter_keys_stay_on_the_reviewed_origin() {
        let profile = profile(OPENROUTER_PROVIDER_ID).unwrap();
        assert!(profile.requires_credential);
        assert_eq!(
            profile.default_endpoint.as_deref(),
            Some(OPENROUTER_ENDPOINT)
        );
        assert_eq!(
            profile.default_model.as_deref(),
            Some(OPENROUTER_DEFAULT_MODEL)
        );
        assert!(
            validate_endpoint_for_provider(OPENROUTER_PROVIDER_ID, Some(OPENROUTER_ENDPOINT))
                .is_ok()
        );
        assert!(validate_endpoint_for_provider(
            OPENROUTER_PROVIDER_ID,
            Some("https://openrouter.ai.attacker.example/api/v1/chat/completions")
        )
        .is_err());
        assert_eq!(
            integration_and_auth_mode(OPENROUTER_PROVIDER_ID),
            ("provider_api", "api_key")
        );
    }

    #[test]
    fn the_free_route_exists_only_with_a_deployed_relay() {
        match free_assistant() {
            None => {
                assert!(profile(FREE_PROVIDER_ID).is_none());
                assert!(validate_endpoint_for_provider(
                    FREE_PROVIDER_ID,
                    Some("https://relay.example/v1/chat/completions")
                )
                .is_err());
            }
            Some(free) => {
                let profile = profile(FREE_PROVIDER_ID).unwrap();
                assert!(!profile.requires_credential);
                assert!(
                    validate_endpoint_for_provider(FREE_PROVIDER_ID, Some(&free.endpoint)).is_ok()
                );
                assert!(validate_endpoint_for_provider(
                    FREE_PROVIDER_ID,
                    Some(&format!("{}x", free.endpoint))
                )
                .is_err());
                let config = AiProviderConfig {
                    provider: FREE_PROVIDER_ID.into(),
                    model: "anthropic/claude-opus".into(),
                    reasoning_effort: "high".into(),
                    endpoint: free.endpoint.clone(),
                    credential_reference: None,
                };
                assert!(validate_config(&config).is_err());
                let config = AiProviderConfig {
                    model: free.model.clone(),
                    ..config
                };
                assert!(validate_config(&config).is_ok());
                let models = list_models(&MemoryCredentialStore::default(), &config).unwrap();
                assert_eq!(models.len(), 1);
                assert_eq!(models[0].id, free.model);
            }
        }
        assert_eq!(
            integration_and_auth_mode(FREE_PROVIDER_ID),
            ("provider_api", FREE_AUTH_MODE)
        );
    }

    #[test]
    fn description_payloads_carry_the_draft_schema_and_only_the_selected_key() {
        let config = AiProviderConfig {
            provider: OPENROUTER_PROVIDER_ID.into(),
            model: OPENROUTER_DEFAULT_MODEL.into(),
            reasoning_effort: "high".into(),
            endpoint: OPENROUTER_ENDPOINT.into(),
            credential_reference: None,
        };
        let key = ["sk", "or", "v1", "payload-test"].join("-");
        let (headers, body) =
            description_payload("openai_compatible", &config, "cold war mod", Some(&key)).unwrap();
        assert_eq!(
            headers.get("authorization").unwrap(),
            &format!("Bearer {key}")
        );
        assert_eq!(
            body["response_format"]["json_schema"]["name"],
            "mod_description"
        );
        assert_eq!(body["provider"]["require_parameters"], true);
        assert!(!body.to_string().contains(&key));
        assert!(body.get("tools").is_none());
        let (headers, _) =
            description_payload("openai_compatible", &config, "cold war mod", None).unwrap();
        assert!(headers.get("authorization").is_none());
        assert!(description_payload("openai_compatible", &config, "   ", None).is_err());
    }

    #[test]
    fn drafting_with_codex_must_use_the_app_server() {
        let config = AiProviderConfig {
            provider: "codex".into(),
            model: "gpt-5.6-luna".into(),
            reasoning_effort: "high".into(),
            endpoint: String::new(),
            credential_reference: None,
        };
        assert!(draft_description(&MemoryCredentialStore::default(), &config, "a mod").is_err());
    }
}
