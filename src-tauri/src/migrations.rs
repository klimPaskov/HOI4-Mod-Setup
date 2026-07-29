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
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::Serialization("project state must be a JSON object".into()))?;
    let ai = object.entry("ai").or_insert_with(|| {
        json!({
            "provider": "codex",
            "model": "default",
            "optimization_profile": "Codex project and ChatGPT Chat"
        })
    });
    let ai = ai.as_object_mut().ok_or_else(|| {
        AppError::Serialization("project state ai integration must be an object".into())
    })?;
    reject_persisted_ai_secret(ai)?;
    insert_default(ai, "provider", Value::String("codex".into()));
    insert_default(ai, "model", Value::String("default".into()));
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
    Ok(value)
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
    strip_legacy_core_only_codex_bindings(&mut value);
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
        object
            .entry("wiki_required_pages")
            .or_insert_with(|| Value::Array(Vec::new()));
    }
    serde_json::from_value(value).map_err(AppError::from)
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

pub fn migrate_journal(value: Value) -> Result<TransactionJournal, AppError> {
    let mut value = migrate_value(value, "transaction journal", CURRENT_JOURNAL_SCHEMA)?;
    if let Some(object) = value.as_object_mut() {
        object
            .entry("transaction_kind")
            .or_insert_with(|| Value::String("installation".into()));
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
