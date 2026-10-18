//! Pure path, hashing, and redaction rules shared by every host.
//!
//! These functions inspect text only; filesystem-aware checks stay in the
//! desktop host.

use crate::AppError;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

/// Normalizes a manifest or project-relative path without following links.
/// Absolute paths, traversal, alternate data streams, device names, and empty
/// segments are rejected before the path is allowed into a plan.
pub fn normalize_relative_path(raw: &str) -> Result<String, AppError> {
    if raw.is_empty() || raw.contains('\0') || raw.len() > 4096 {
        return Err(AppError::PathSecurity(
            "empty, oversized, or NUL-containing path".into(),
        ));
    }

    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized.chars().nth(1) == Some(':')
    {
        return Err(AppError::PathSecurity(format!(
            "absolute path is not allowed: {raw}"
        )));
    }

    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if parts.len() >= 64 || part.len() > 255 {
            return Err(AppError::PathSecurity(
                "path depth or segment length exceeds the supported limit".into(),
            ));
        }
        if part == ".." {
            return Err(AppError::PathSecurity(format!(
                "path traversal is not allowed: {raw}"
            )));
        }
        if part.contains(':') {
            return Err(AppError::PathSecurity(format!(
                "alternate data stream is not allowed: {raw}"
            )));
        }
        if part.ends_with(' ') || part.ends_with('.') || is_reserved_windows_name(part) {
            return Err(AppError::PathSecurity(format!(
                "reserved path segment: {part}"
            )));
        }
        parts.push(part);
    }

    if parts.is_empty() {
        return Err(AppError::PathSecurity(format!(
            "empty relative path: {raw}"
        )));
    }
    Ok(parts.join("/"))
}

pub fn is_reserved_windows_name(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

pub fn canonical_relative_key(path: &str) -> Result<String, AppError> {
    Ok(normalize_relative_path(path)?
        .nfkd()
        .flat_map(|character| character.to_uppercase())
        .nfkc()
        .collect())
}

pub fn validate_destination_set(destinations: &[String]) -> Result<(), AppError> {
    let mut seen = HashSet::new();
    for destination in destinations {
        let key = canonical_relative_key(destination)?;
        if !seen.insert(key) {
            return Err(AppError::PathSecurity(format!(
                "duplicate destination: {destination}"
            )));
        }
    }
    Ok(())
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Serialize a value for persistence after the same secret checks that
/// `atomic_write_json` applies. Callers that write through a retained
/// `RootedDir` use these bytes instead of reopening the file by path.
pub fn persistable_json_bytes<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, AppError> {
    let json = serde_json::to_value(value)?;
    reject_secret_like_keys(&json)?;
    // Inspect decoded values, not JSON punctuation. Otherwise an already
    // redacted assignment at the end of a string consumes its closing quote
    // on a second redaction pass and falsely prevents journal persistence.
    if json_contains_credential_shaped_content(&json) {
        return Err(AppError::Credential(
            "credential-shaped content is not serializable".into(),
        ));
    }
    Ok(serde_json::to_vec_pretty(&json)?)
}

fn json_contains_credential_shaped_content(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => contains_credential_shaped_content(text),
        serde_json::Value::Array(items) => {
            items.iter().any(json_contains_credential_shaped_content)
        }
        serde_json::Value::Object(items) => items.iter().any(|(key, value)| {
            contains_credential_shaped_content(key)
                // Preserve assignment-shaped key detection (for example
                // "provider.api_key") without inspecting serialized values.
                || contains_credential_shaped_content(&format!("{key}=[FIELD_VALUE]"))
                || json_contains_credential_shaped_content(value)
        }),
        _ => false,
    }
}

/// Detect credential-shaped content while allowing the exact non-secret
/// Meshy example documented by the verified 3D workflow. A longer value that
/// merely starts with the placeholder remains detectable because the token
/// replacement requires word boundaries.
pub fn contains_credential_shaped_content(value: &str) -> bool {
    static PLACEHOLDER: OnceLock<Regex> = OnceLock::new();
    let placeholder = PLACEHOLDER.get_or_init(|| {
        Regex::new(r"\bmsy_your_actual_key_here\b")
            .expect("static documented Meshy placeholder regex")
    });
    let inspected = placeholder.replace_all(value, "[DOCUMENTED_MESHY_KEY_PLACEHOLDER]");
    redact_secrets(&inspected, &[]) != inspected
}

fn secret_redaction_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?i)(bearer\s+)[A-Za-z0-9._~+/=-]+",
            r#"(?i)(["']?(?:authorization|api[_-]?key|client[_-]?secret|private[_-]?key|access[_-]?token|refresh[_-]?token|device[_-]?code|user[_-]?code|login[_-]?token)["']?\s*:\s*["'])[^"']+(["'])"#,
            r"(?i)((?:authorization|api[_-]?key|client[_-]?secret|private[_-]?key|access[_-]?token|refresh[_-]?token|device[_-]?code|user[_-]?code|login[_-]?token)\s*[:=]\s*)[^\s,;&]+",
            r"(?i)([?&](?:code|token|access_token|refresh_token|device_code|user_code)=)[^&#\s]+",
            r"(?i)(api[_-]?key\s*[=:]\s*)[^\s,;&]+",
            r"(?i)(meshy_api_key\s*[=:]\s*)[^\s,;&]+",
            r"\bmsy_[A-Za-z0-9_-]{8,}\b",
            r"\bsk-ant-[A-Za-z0-9_-]{20,}\b",
            r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b",
            r"\bAIza[A-Za-z0-9_-]{20,}\b",
            r"\bxai-[A-Za-z0-9_-]{20,}\b",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("static secret redaction regex"))
        .collect()
    })
}

pub fn redact_secrets(value: &str, known_secrets: &[String]) -> String {
    let mut redacted = value.to_string();
    for secret in known_secrets.iter().filter(|secret| !secret.is_empty()) {
        redacted = redacted.replace(secret, "[REDACTED]");
    }

    // Common hosted-provider key shapes use high-signal prefixes so ordinary
    // model IDs and endpoint names do not become credentials by guesswork.
    for pattern in secret_redaction_patterns() {
        redacted = pattern
            .replace_all(&redacted, |captures: &regex::Captures<'_>| {
                if captures.len() > 1 {
                    format!(
                        "{}[REDACTED]{}",
                        &captures[1],
                        captures.get(2).map_or("", |capture| capture.as_str())
                    )
                } else {
                    "[REDACTED]".to_string()
                }
            })
            .into_owned();
    }
    redacted
}

pub fn reject_secret_like_keys(value: &serde_json::Value) -> Result<(), AppError> {
    fn walk(value: &serde_json::Value, path: &str) -> Result<(), AppError> {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let canonical = key
                        .chars()
                        .filter(|character| character.is_ascii_alphanumeric())
                        .map(|character| character.to_ascii_lowercase())
                        .collect::<String>();
                    if matches!(
                        canonical.as_str(),
                        "secretvalue"
                            | "password"
                            | "passphrase"
                            | "token"
                            | "apikey"
                            | "clientsecret"
                            | "authorization"
                            | "accesstoken"
                            | "refreshtoken"
                            | "devicecode"
                            | "usercode"
                            | "credentialvalue"
                            | "privatekey"
                    ) {
                        return Err(AppError::Credential(format!(
                            "secret-like field is not serializable: {path}.{key}"
                        )));
                    }
                    walk(child, &format!("{path}.{key}"))?;
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    walk(child, &format!("{path}[{index}]"))?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(value, "$")
}

pub fn validate_env_name(name: &str) -> Result<(), AppError> {
    let valid = Regex::new(r"^[A-Z][A-Z0-9_]*$").expect("static environment name regex");
    if valid.is_match(name) {
        Ok(())
    } else {
        Err(AppError::Credential(format!(
            "invalid environment variable name: {name}"
        )))
    }
}
