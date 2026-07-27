//! Versioned persistence migrations for project state, locks, and journals.
//!
//! Migrations are deliberately pure: callers read bytes, migrate in memory, and
//! persist with the transaction-safe atomic writer only after validation.

use crate::models::{InstallationLock, TransactionJournal};
use crate::AppError;
use serde_json::{json, Map, Value};

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
    insert_default(
        codex,
        "integration",
        Value::String("codex_app_server".into()),
    );
    insert_default(codex, "auth_mode", Value::String("chatgpt".into()));
    insert_default(codex, "auth_status", Value::String("signed_out".into()));
    insert_default(codex, "analysis_required", Value::Bool(true));
    insert_default(codex, "analysis_status", Value::String("blocked".into()));
    insert_default(codex, "account_values_persisted", Value::Bool(false));
    if codex.get("integration") != Some(&Value::String("codex_app_server".into()))
        || codex.get("auth_mode") != Some(&Value::String("chatgpt".into()))
        || codex.get("analysis_required") != Some(&Value::Bool(true))
    {
        return Err(AppError::Serialization(
            "project state uses an unsupported Codex integration mode".into(),
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
        if object.get("name") != Some(&Value::String("MESHY_API_KEY".into())) {
            return Err(AppError::Credential(
                "only the manifest-declared Meshy credential may be persisted by this app".into(),
            ));
        }
        let reference = object
            .get("reference")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Credential("credential reference is missing".into()))?;
        if !reference.starts_with("credential://")
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
    let value = migrate_value(value, "transaction journal", CURRENT_JOURNAL_SCHEMA)?;
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
    fn persisted_account_identity_is_rejected() {
        assert!(migrate_state(json!({
            "schema_version": "1.0.0",
            "codex": {"email": "user@example.com"}
        }))
        .is_err());
    }

    #[test]
    fn legacy_lock_gets_an_incomplete_wiki_evidence_marker() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../examples/installation-lock.example.json"
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove("wiki_required_pages");
        let lock = migrate_lock(value).unwrap();
        assert!(lock.wiki_required_pages.is_empty());
    }

    #[test]
    fn legacy_codex_bindings_are_removed_before_strict_lock_deserialization() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../examples/installation-lock.example.json"
        ))
        .unwrap();
        value["codex_analysis"]["project_root"] = json!("C:/Users/private/mod");
        value["codex_analysis"]["scan_id"] = json!("8da5758f-e9d8-4ad4-bba6-99c9bc6583ee");

        let lock = migrate_lock(value).unwrap();

        assert!(lock.codex_analysis.unwrap().project_root.is_none());
    }
}
