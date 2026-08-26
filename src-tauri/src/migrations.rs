//! Versioned persistence migrations for project state, locks, and journals.
//!
//! Migrations are deliberately pure: callers read bytes, migrate in memory, and
//! persist with the transaction-safe atomic writer only after validation.

use crate::models::{InstallationLock, TransactionJournal};
use crate::AppError;
use serde_json::{json, Map, Value};
use uuid::Uuid;

pub const CURRENT_STATE_SCHEMA: &str = "1.0.0";
pub const CURRENT_LOCK_SCHEMA: &str = "1.0.0";
pub const CURRENT_JOURNAL_SCHEMA: &str = "1.0.0";

fn major(schema: &str) -> Result<u64, AppError> {
    schema
        .split('.')
        .next()
        .ok_or_else(|| AppError::Serialization("schema version is empty".into()))?
        .parse::<u64>()
        .map_err(|_| AppError::Serialization(format!("invalid schema version: {schema}")))
}

fn migrate_value(mut value: Value, expected_kind: &str, current: &str) -> Result<Value, AppError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::Serialization(format!("{expected_kind} must be a JSON object")))?;
    let found = object
        .get("schema_version")
        .and_then(Value::as_str)
        .unwrap_or("1.0.0");
    if major(found)? != major(current)? {
        return Err(AppError::Serialization(format!(
            "unsupported {expected_kind} schema major: {found}"
        )));
    }
    // 1.0.x has no lossy migrations. Keeping the version normalized makes
    // future patch migrations explicit and keeps locks deterministic.
    object.insert("schema_version".into(), Value::String(current.into()));
    Ok(value)
}

pub fn migrate_state(value: Value) -> Result<Value, AppError> {
    let mut value = migrate_value(value, "project state", CURRENT_STATE_SCHEMA)?;
    normalize_legacy_portrait_workflow(&mut value);
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::Serialization("project state must be a JSON object".into()))?;
    let ai = object.entry("ai").or_insert_with(|| {
        json!({
            "provider": "codex",
            "model": "gpt-5.6-luna",
            "reasoning_effort": "xhigh",
            "optimization_profile": "Codex project and ChatGPT Chat"
        })
    });
    let ai = ai.as_object_mut().ok_or_else(|| {
        AppError::Serialization("project state ai integration must be an object".into())
    })?;
    reject_persisted_ai_secret(ai)?;
    insert_default(ai, "provider", Value::String("codex".into()));
    insert_default(ai, "model", Value::String("gpt-5.6-luna".into()));
    insert_default(ai, "reasoning_effort", Value::String("xhigh".into()));
    insert_default(
        ai,
        "optimization_profile",
        Value::String("Codex project and ChatGPT Chat".into()),
    );
    let provider = ai
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if !matches!(
        provider.as_str(),
        "codex" | "claude" | "kimi" | "glm" | "deepseek" | "local" | "custom"
    ) {
        return Err(AppError::InvalidInput(
            "project state uses an unsupported AI provider".into(),
        ));
    }
    if ai
        .get("model")
        .and_then(Value::as_str)
        .is_none_or(|model| model.trim().is_empty())
    {
        return Err(AppError::InvalidInput(
            "project state AI model must be a non-empty string".into(),
        ));
    }
    if ai
        .get("optimization_profile")
        .and_then(Value::as_str)
        .is_none_or(|profile| profile.trim().is_empty())
    {
        return Err(AppError::InvalidInput(
            "project state AI optimization profile must be a non-empty string".into(),
        ));
    }
    crate::ai::validate_reasoning_effort(
        ai.get("reasoning_effort")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let codex = object.entry("codex").or_insert_with(|| {
        json!({
            "integration": "codex_app_server",
            "auth_mode": "chatgpt",
            "auth_status": "signed_out",
            "analysis_required": true,
            "analysis_status": "blocked",
            "account_values_persisted": false
        })
    });
    let codex = codex.as_object_mut().ok_or_else(|| {
        AppError::Serialization("project state codex integration must be an object".into())
    })?;
    reject_persisted_codex_identity(codex)?;
    let expected_integration = if provider == "codex" {
        "codex_app_server"
    } else {
        "provider_api"
    };
    let expected_auth_mode = if provider == "codex" {
        "chatgpt"
    } else if provider == "local" {
        "local_endpoint"
    } else {
        "api_key"
    };
    if provider != "codex" {
        // Older state used the `codex` object as the generic integration
        // container. Rewrite only its non-secret routing fields when the
        // persisted provider is no longer Codex; account values remain
        // explicitly absent and auth status remains whatever was honestly
        // recorded.
        codex.insert(
            "integration".into(),
            Value::String(expected_integration.into()),
        );
        codex.insert("auth_mode".into(), Value::String(expected_auth_mode.into()));
    }
    insert_default(
        codex,
        "integration",
        Value::String(expected_integration.into()),
    );
    insert_default(codex, "auth_mode", Value::String(expected_auth_mode.into()));
    insert_default(codex, "auth_status", Value::String("signed_out".into()));
    insert_default(codex, "analysis_required", Value::Bool(true));
    insert_default(codex, "analysis_status", Value::String("blocked".into()));
    insert_default(codex, "account_values_persisted", Value::Bool(false));
    if codex.get("integration") != Some(&Value::String(expected_integration.into()))
        || codex.get("auth_mode") != Some(&Value::String(expected_auth_mode.into()))
        || codex.get("analysis_required") != Some(&Value::Bool(true))
    {
        return Err(AppError::Serialization(
            "project state uses an unsupported AI integration mode".into(),
        ));
    }
    if codex.get("account_values_persisted") != Some(&Value::Bool(false)) {
        return Err(AppError::Credential(
            "project state claims that Codex account identity was persisted".into(),
        ));
    }
    let references = object
        .entry("credential_references")
        .or_insert_with(|| Value::Array(Vec::new()));
    validate_credential_references(references)?;
    if let Some(portrait_pipeline) = object.get("portrait_pipeline") {
        if !portrait_pipeline.is_null() {
            validate_portrait_pipeline(portrait_pipeline)?;
        }
    }
    Ok(value)
}

fn validate_portrait_pipeline(value: &Value) -> Result<(), AppError> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Serialization("portrait_pipeline must be an object".into()))?;
    if object.keys().any(|key| {
        matches!(
            key.as_str(),
            "api_key"
                | "apiKey"
                | "secret"
                | "secret_value"
                | "secretValue"
                | "token"
                | "password"
                | "cookie"
        )
    }) {
        return Err(AppError::Credential(
            "portrait_pipeline contains a secret-shaped field".into(),
        ));
    }
    let provider = object
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Serialization("portrait_pipeline provider is missing".into()))?;
    if !matches!(provider, "cloud" | "local" | "runpod" | "disabled") {
        return Err(AppError::InvalidInput(
            "portrait_pipeline uses an unsupported provider".into(),
        ));
    }
    let enabled = object
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| AppError::Serialization("portrait_pipeline enabled is missing".into()))?;
    if enabled == (provider == "disabled") {
        return Err(AppError::InvalidInput(
            "portrait_pipeline enabled/provider state is inconsistent".into(),
        ));
    }
    let repository = object
        .get("workflow_repository")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Serialization("portrait_pipeline repository is missing".into()))?;
    if repository != crate::portraits::PORTRAIT_REPOSITORY {
        return Err(AppError::Source(
            "portrait_pipeline is bound to an unverified repository".into(),
        ));
    }
    let preferred_workflow = object
        .get("preferred_workflow")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::Serialization("portrait_pipeline preferred workflow is missing".into())
        })?;
    if !matches!(preferred_workflow, "source" | "processing_only") {
        return Err(AppError::InvalidInput(
            "portrait_pipeline supports only sourced workflows; non-sourced portraits use native ImageGen".into(),
        ));
    }
    let commit = object
        .get("workflow_commit")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Serialization("portrait_pipeline commit is missing".into()))?;
    if commit != crate::portraits::PORTRAIT_COMMIT {
        return Err(AppError::Source(
            "portrait_pipeline is bound to an unverified workflow revision".into(),
        ));
    }
    let mcp_registered = object
        .get("mcp_registered")
        .and_then(Value::as_bool)
        .ok_or_else(|| AppError::Serialization("portrait_pipeline MCP state is missing".into()))?;
    if mcp_registered != (enabled && provider == "cloud") {
        return Err(AppError::InvalidInput(
            "portrait_pipeline MCP state does not match its provider".into(),
        ));
    }
    Ok(())
}

fn validate_credential_references(value: &Value) -> Result<(), AppError> {
    let references = value.as_array().ok_or_else(|| {
        AppError::Credential("project credential_references must be an array".into())
    })?;
    for reference in references {
        let object = reference.as_object().ok_or_else(|| {
            AppError::Credential("project credential reference must be an object".into())
        })?;
        if object.keys().any(|key| {
            matches!(
                key.as_str(),
                "secret_value" | "secretValue" | "api_key" | "apiKey" | "token"
            )
        }) {
            return Err(AppError::Credential(
                "project credential references contain a secret-shaped field".into(),
            ));
        }
        if object.get("name").and_then(Value::as_str) != Some("MESHY_API_KEY") {
            return Err(AppError::Credential(
                "AI provider credential references are session-only and cannot be migrated into a project".into(),
            ));
        }
        if object
            .keys()
            .any(|key| key != "name" && key != "provider" && key != "reference")
        {
            return Err(AppError::Credential(
                "project credential reference contains an unsupported field".into(),
            ));
        }
        let reference = object
            .get("reference")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Credential("credential reference is missing".into()))?;
        let opaque_id = reference.strip_prefix("credential://meshy_api_key/");
        if opaque_id.is_none_or(|value| Uuid::parse_str(value).is_err())
            || reference.chars().any(char::is_whitespace)
            || reference.len() > 512
        {
            return Err(AppError::Credential(
                "credential reference is not an opaque vault reference".into(),
            ));
        }
    }
    Ok(())
}

fn reject_persisted_ai_secret(object: &Map<String, Value>) -> Result<(), AppError> {
    const FORBIDDEN_KEYS: &[&str] = &[
        "api_key",
        "apiKey",
        "secret",
        "secret_value",
        "secretValue",
        "access_token",
        "accessToken",
        "refresh_token",
        "refreshToken",
        "token",
    ];
    if object
        .keys()
        .any(|key| FORBIDDEN_KEYS.contains(&key.as_str()))
    {
        return Err(AppError::Credential(
            "project state contains an AI provider secret".into(),
        ));
    }
    Ok(())
}

fn insert_default(object: &mut Map<String, Value>, key: &str, value: Value) {
    object.entry(key).or_insert(value);
}

fn reject_persisted_codex_identity(object: &Map<String, Value>) -> Result<(), AppError> {
    const FORBIDDEN_KEYS: &[&str] = &[
        "api_key",
        "apiKey",
        "access_token",
        "accessToken",
        "refresh_token",
        "refreshToken",
        "token",
        "email",
        "account_id",
        "accountId",
        "plan_type",
        "planType",
        "usage",
        "rate_limits",
        "rateLimits",
    ];
    if object
        .keys()
        .any(|key| FORBIDDEN_KEYS.contains(&key.as_str()))
    {
        return Err(AppError::Credential(
            "project state contains Codex account or credential data and was not migrated".into(),
        ));
    }
    Ok(())
}

pub fn migrate_lock(value: Value) -> Result<InstallationLock, AppError> {
    let mut value = migrate_value(value, "installation lock", CURRENT_LOCK_SCHEMA)?;
    normalize_legacy_portrait_workflow(&mut value);
    strip_legacy_core_only_codex_bindings(&mut value);
    bind_legacy_analysis_to_lock_source(&mut value);
    // Locks written before exact manifest wiki coverage was carried forward
    // remain readable, but readiness must not silently borrow the current
    // bundled manifest for them. An empty marker is intentionally incomplete
    // until the user runs update or repair against a verified source.
    if let Some(object) = value.as_object_mut() {
        let provider = object
            .get("ai_provider")
            .and_then(Value::as_str)
            .unwrap_or("codex")
            .to_string();
        object.entry("ai_optimization_profile").or_insert_with(|| {
            Value::String(
                match provider.as_str() {
                    "claude" => "Claude Code / Anthropic conventions",
                    "kimi" => "Kimi coding conventions",
                    "glm" => "GLM coding conventions",
                    "deepseek" => "DeepSeek coding conventions",
                    "local" => "Local model conventions",
                    "custom" => "User-supplied provider conventions",
                    _ => "Codex project and ChatGPT Chat",
                }
                .into(),
            )
        });
        object.entry("ai_reasoning_effort").or_insert_with(|| {
            Value::String(if provider == "codex" { "xhigh" } else { "high" }.into())
        });
        object
            .entry("wiki_required_pages")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(portrait_pipeline) = object.get("portrait_pipeline") {
            if !portrait_pipeline.is_null() {
                validate_portrait_pipeline(portrait_pipeline)?;
            }
        }
    }
    serde_json::from_value(value).map_err(AppError::from)
}

fn bind_legacy_analysis_to_lock_source(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let source_revision = object
        .get("source")
        .and_then(Value::as_object)
        .and_then(|source| source.get("revision"))
        .and_then(Value::as_str)
        .filter(|revision| {
            revision.len() == 40
                && revision
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .map(ToOwned::to_owned);
    let source_manifest_sha256 = object
        .get("source")
        .and_then(Value::as_object)
        .and_then(|source| source.get("manifest_sha256"))
        .and_then(Value::as_str)
        .filter(|sha256| {
            sha256.len() == 64
                && sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .map(ToOwned::to_owned);
    let Some(record) = object
        .get_mut("codex_analysis")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if record.get("source_revision").is_none_or(Value::is_null) {
        if let Some(revision) = source_revision {
            record.insert("source_revision".into(), Value::String(revision));
        }
    }
    if record
        .get("source_manifest_sha256")
        .is_none_or(Value::is_null)
    {
        if let Some(sha256) = source_manifest_sha256 {
            record.insert("source_manifest_sha256".into(), Value::String(sha256));
        }
    }
}

fn strip_legacy_core_only_codex_bindings(value: &mut Value) {
    if let Some(record) = value
        .as_object_mut()
        .and_then(|object| object.get_mut("codex_analysis"))
        .and_then(Value::as_object_mut)
    {
        record.remove("project_root");
        record.remove("scan_id");
    }
}

fn normalize_legacy_portrait_workflow(value: &mut Value) {
    let Some(pipeline) = value
        .as_object_mut()
        .and_then(|object| object.get_mut("portrait_pipeline"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if pipeline.get("preferred_workflow").and_then(Value::as_str) == Some("text_to_image") {
        // The old route generated non-sourced portraits through ComfyUI. It
        // cannot be mapped to a sourced workflow without changing the asset
        // classification, so disable the optional provider and leave native
        // ImageGen to the parent portrait workflow.
        pipeline.insert("enabled".into(), Value::Bool(false));
        pipeline.insert("provider".into(), Value::String("disabled".into()));
        pipeline.insert(
            "provider_status".into(),
            Value::String("not_selected".into()),
        );
        pipeline.insert("preferred_workflow".into(), Value::String("source".into()));
        pipeline.insert("mcp_registered".into(), Value::Bool(false));
    }
}

pub fn migrate_journal(value: Value) -> Result<TransactionJournal, AppError> {
    let mut value = migrate_value(value, "transaction journal", CURRENT_JOURNAL_SCHEMA)?;
    if let Some(object) = value.as_object_mut() {
        object
            .entry("transaction_kind")
            .or_insert_with(|| Value::String("installation".into()));
        object.entry("project_root_lifecycle").or_insert_with(|| {
            serde_json::json!({
                "mode": "existing",
                "canonical_parent": null,
                "leaf": null,
                "checkpoint": "not_required",
                "created_by_transaction": false,
                "observed_exists": true,
                "cleanup_result": null
            })
        });
    }
    serde_json::from_value(value).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn patch_version_is_normalized_without_dropping_fields() {
        let migrated =
            migrate_state(json!({"schema_version": "1.0.0", "project_id": "demo"})).unwrap();
        assert_eq!(migrated["schema_version"], "1.0.0");
        assert_eq!(migrated["project_id"], "demo");
    }

    #[test]
    fn future_major_is_rejected() {
        assert!(migrate_state(json!({"schema_version": "2.0.0"})).is_err());
    }

    #[test]
    fn missing_codex_state_defaults_to_signed_out_without_identity() {
        let migrated = migrate_state(json!({
            "schema_version": "1.0.0",
            "project_id": "demo"
        }))
        .unwrap();
        assert_eq!(migrated["codex"]["integration"], "codex_app_server");
        assert_eq!(migrated["codex"]["auth_mode"], "chatgpt");
        assert_eq!(migrated["codex"]["auth_status"], "signed_out");
        assert_eq!(migrated["codex"]["account_values_persisted"], false);
    }

    #[test]
    fn non_codex_state_migrates_to_provider_api_without_claiming_codex_auth() {
        let migrated = migrate_state(json!({
            "schema_version": "1.0.0",
            "ai": {
                "provider": "claude",
                "model": "claude-model",
                "optimization_profile": "Claude Code / Anthropic conventions"
            },
            "codex": {
                "integration": "codex_app_server",
                "auth_mode": "chatgpt",
                "auth_status": "signed_out",
                "analysis_required": true,
                "analysis_status": "blocked",
                "account_values_persisted": false
            }
        }))
        .unwrap();
        assert_eq!(migrated["codex"]["integration"], "provider_api");
        assert_eq!(migrated["codex"]["auth_mode"], "api_key");
        assert_eq!(migrated["codex"]["auth_status"], "signed_out");
        assert_eq!(migrated["codex"]["analysis_status"], "blocked");
    }

    #[test]
    fn local_state_migrates_to_loopback_provider_mode() {
        let migrated = migrate_state(json!({
            "schema_version": "1.0.0",
            "ai": {
                "provider": "local",
                "model": "local-model",
                "optimization_profile": "Local model conventions"
            }
        }))
        .unwrap();
        assert_eq!(migrated["codex"]["integration"], "provider_api");
        assert_eq!(migrated["codex"]["auth_mode"], "local_endpoint");
    }

    #[test]
    fn persisted_account_identity_is_rejected() {
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "codex": {"email": "user@example.com"}
        }))
        .is_err());
    }

    #[test]
    fn malformed_ai_selection_is_rejected_during_migration() {
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "ai": {
                "provider": "claude",
                "model": "",
                "optimization_profile": "Claude Code / Anthropic conventions"
            }
        }))
        .is_err());
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "ai": {
                "provider": "claude",
                "model": "claude-3-7-sonnet",
                "optimization_profile": ""
            }
        }))
        .is_err());
    }

    fn portrait_state(provider: &str, enabled: bool, mcp_registered: bool) -> Value {
        json!({
            "enabled": enabled,
            "provider": provider,
            "provider_status": if enabled { "needs_authorization" } else { "not_selected" },
            "workflow_repository": crate::portraits::PORTRAIT_REPOSITORY,
            "workflow_branch": crate::portraits::PORTRAIT_BRANCH,
            "workflow_commit": crate::portraits::PORTRAIT_COMMIT,
            "preferred_workflow": "source",
            "local_comfyui_root": "",
            "local_server_url": "http://127.0.0.1:8188",
            "runpod_url": "",
            "runpod_workspace": "/workspace/comfyui-hoi4-portraits",
            "mcp_registered": mcp_registered
        })
    }

    #[test]
    fn portrait_state_migration_accepts_verified_disabled_configuration() {
        let migrated = migrate_state(json!({
            "schema_version": "1.0.0",
            "portrait_pipeline": portrait_state("disabled", false, false)
        }))
        .unwrap();
        assert_eq!(migrated["portrait_pipeline"]["provider"], "disabled");
        assert_eq!(
            migrated["portrait_pipeline"]["workflow_commit"],
            crate::portraits::PORTRAIT_COMMIT
        );
    }

    #[test]
    fn portrait_state_migration_rejects_secrets_and_unverified_revision() {
        let mut secret = portrait_state("cloud", true, true);
        secret["api_key"] = json!("never-persist");
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "portrait_pipeline": secret
        }))
        .is_err());

        let mut unverified = portrait_state("cloud", true, true);
        unverified["workflow_commit"] = json!("a".repeat(40));
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "portrait_pipeline": unverified
        }))
        .is_err());
    }

    #[test]
    fn portrait_state_migration_rejects_inconsistent_cloud_mcp_state() {
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "portrait_pipeline": portrait_state("local", true, true)
        }))
        .is_err());
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "portrait_pipeline": Value::Null
        }))
        .is_ok());
    }

    #[test]
    fn legacy_text_to_image_portrait_state_is_disabled_instead_of_rerouted() {
        let mut state = portrait_state("cloud", true, true);
        state["preferred_workflow"] = json!("text_to_image");
        let migrated = migrate_state(json!({
            "schema_version": "1.0.0",
            "portrait_pipeline": state
        }))
        .unwrap();
        assert_eq!(migrated["portrait_pipeline"]["enabled"], false);
        assert_eq!(migrated["portrait_pipeline"]["provider"], "disabled");
        assert_eq!(
            migrated["portrait_pipeline"]["preferred_workflow"],
            "source"
        );
        assert_eq!(migrated["portrait_pipeline"]["mcp_registered"], false);
    }

    #[test]
    fn project_ai_provider_reference_cannot_be_migrated_or_smuggled_extra_fields() {
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "credential_references": [{
                "name": "AI_PROVIDER_API_KEY",
                "provider": "windows_credential_manager",
                "reference": "credential://ai_provider_api_key/claude"
            }]
        }))
        .is_err());
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "credential_references": [{
                "name": "MESHY_API_KEY",
                "provider": "windows_credential_manager",
                "reference": "credential://meshy_api_key/00000000-0000-0000-0000-000000000000",
                "provider_id": "claude"
            }]
        }))
        .is_err());
    }

    #[test]
    fn project_meshy_reference_must_use_the_opaque_meshy_namespace() {
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "credential_references": [{
                "name": "MESHY_API_KEY",
                "provider": "windows_credential_manager",
                "reference": "credential://other/00000000-0000-0000-0000-000000000000"
            }]
        }))
        .is_err());
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "credential_references": [{
                "name": "MESHY_API_KEY",
                "provider": "windows_credential_manager",
                "reference": "credential://meshy_api_key/not-a-uuid"
            }]
        }))
        .is_err());
    }

    #[test]
    fn legacy_lock_gets_an_incomplete_wiki_evidence_marker() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove("wiki_required_pages");
        let lock = migrate_lock(value).unwrap();
        assert!(lock.wiki_required_pages.is_empty());
    }

    #[test]
    fn legacy_lock_without_wiki_provenance_remains_readable_but_incomplete() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove("wiki_metadata");
        let lock = migrate_lock(value).unwrap();
        assert!(lock.wiki_metadata.is_none());
    }

    #[test]
    fn legacy_lock_gets_the_provider_optimization_profile() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        value["ai_provider"] = json!("claude");
        value
            .as_object_mut()
            .unwrap()
            .remove("ai_optimization_profile");
        let lock = migrate_lock(value).unwrap();
        assert_eq!(
            lock.ai_optimization_profile,
            "Claude Code / Anthropic conventions"
        );
    }

    #[test]
    fn legacy_lock_analysis_inherits_exact_existing_lock_source_evidence() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        value["codex_analysis"]
            .as_object_mut()
            .unwrap()
            .remove("source_revision");
        value["codex_analysis"]["source_manifest_sha256"] = Value::Null;
        value["codex_analysis"]["auth_mode"] = json!("chatgpt");

        let lock = migrate_lock(value).unwrap();
        let record = lock.codex_analysis.unwrap();
        assert_eq!(
            record.source_revision.as_deref(),
            Some(lock.source.revision.as_str())
        );
        assert_eq!(
            record.source_manifest_sha256.as_deref(),
            Some(lock.source.manifest_sha256.as_str())
        );
        crate::codex::validate_confirmed_record(&record).unwrap();
    }

    #[test]
    fn legacy_lock_analysis_does_not_invent_missing_source_evidence() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        value["source"]["manifest_sha256"] = json!("legacy-unknown");
        value["codex_analysis"]
            .as_object_mut()
            .unwrap()
            .remove("source_manifest_sha256");

        let lock = migrate_lock(value).unwrap();
        let record = lock.codex_analysis.unwrap();
        assert!(record.source_manifest_sha256.is_none());
        assert!(crate::codex::validate_confirmed_record(&record).is_err());
    }

    #[test]
    fn legacy_codex_bindings_are_removed_before_strict_lock_deserialization() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../docs/examples/installation-lock.example.json"
        ))
        .unwrap();
        value["codex_analysis"]["project_root"] = json!("C:/Users/private/mod");
        value["codex_analysis"]["scan_id"] = json!("8da5758f-e9d8-4ad4-bba6-99c9bc6583ee");

        let lock = migrate_lock(value).unwrap();

        assert!(lock.codex_analysis.unwrap().project_root.is_none());
    }

    #[test]
    fn legacy_journal_defaults_to_an_installation_transaction() {
        let value: Value = serde_json::from_str(include_str!(
            "../../docs/examples/transaction-journal.example.json"
        ))
        .unwrap();
        let mut legacy = value;
        legacy.as_object_mut().unwrap().remove("transaction_kind");
        let journal = migrate_journal(legacy).unwrap();
        assert_eq!(journal.transaction_kind, "installation");
        assert!(journal.parent_transaction_id.is_none());
    }
}
