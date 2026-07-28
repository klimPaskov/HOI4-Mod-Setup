use crate::models::{ComponentDefinition, DestinationDefinition};
use crate::AppError;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

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
        .nfc()
        .collect::<String>()
        .to_lowercase())
}

pub fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let normalized = normalize_relative_path(relative)?;
    reject_link_components(root, &normalized)?;
    let candidate = root.join(normalized.replace('/', std::path::MAIN_SEPARATOR_STR));
    if candidate.file_name().is_none() {
        return Err(AppError::PathSecurity("destination has no filename".into()));
    }

    // For a missing destination, canonicalize the nearest existing parent and
    // compare that parent. This blocks a pre-existing link from routing writes
    // outside the project while still allowing creation of new files.
    let root_canonical = fs::canonicalize(root).map_err(|error| {
        AppError::PathSecurity(format!("project root is not accessible: {error}"))
    })?;
    let mut existing = candidate.clone();
    while !existing.exists() {
        if !existing.pop() {
            return Err(AppError::PathSecurity(
                "destination has no existing parent".into(),
            ));
        }
    }
    let existing_canonical = fs::canonicalize(&existing)
        .map_err(|error| AppError::PathSecurity(format!("cannot resolve destination: {error}")))?;
    if !existing_canonical.starts_with(&root_canonical) {
        return Err(AppError::PathSecurity(format!(
            "destination escapes project root: {}",
            candidate.display()
        )));
    }
    Ok(candidate)
}

/// Validate an explicitly selected path outside the project root. External
/// destinations are never derived from a manifest path: the UI must provide
/// the absolute path and the plan records it for review. Existing link
/// components are rejected before a backup or write is attempted.
pub fn validate_external_destination(raw: &str) -> Result<PathBuf, AppError> {
    if raw.is_empty() || raw.contains('\0') || raw.len() > 4096 {
        return Err(AppError::PathSecurity(
            "external destination is empty, oversized, or contains NUL".into(),
        ));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(AppError::PathSecurity(
            "external destination must be an absolute path".into(),
        ));
    }
    if path.file_name().is_none() {
        return Err(AppError::PathSecurity(
            "external destination must name a file".into(),
        ));
    }
    let mut depth = 0usize;
    for component in path.components() {
        if let Component::Normal(value) = component {
            depth += 1;
            let value = value.to_str().ok_or_else(|| {
                AppError::PathSecurity(
                    "external destination contains a non-Unicode path segment".into(),
                )
            })?;
            if depth > 64 || value.len() > 255 {
                return Err(AppError::PathSecurity(
                    "external path depth or segment length exceeds the supported limit".into(),
                ));
            }
            if value.contains(':')
                || value.ends_with(' ')
                || value.ends_with('.')
                || is_reserved_windows_name(value)
            {
                return Err(AppError::PathSecurity(format!(
                    "reserved external path segment: {value}"
                )));
            }
        }
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::PathSecurity(
            "external destination cannot contain parent traversal".into(),
        ));
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if is_link_metadata(&metadata) {
                return Err(AppError::PathSecurity(format!(
                    "external destination contains a symlink: {}",
                    current.display()
                )));
            }
        }
    }
    let mut parent = path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("external destination has no parent".into()))?
        .to_path_buf();
    while !parent.exists() {
        if !parent.pop() {
            return Err(AppError::PathSecurity(
                "external destination has no existing parent".into(),
            ));
        }
    }
    fs::canonicalize(&parent).map_err(|error| {
        AppError::PathSecurity(format!(
            "cannot resolve external destination parent: {error}"
        ))
    })?;
    if path.exists() {
        let metadata = fs::symlink_metadata(&path)?;
        if is_link_metadata(&metadata) {
            return Err(AppError::PathSecurity(
                "external destination is a symlink".into(),
            ));
        }
        if !metadata.is_file() {
            return Err(AppError::PathSecurity(
                "external destination is not a regular file".into(),
            ));
        }
    }
    Ok(path)
}

pub fn reject_link_components(root: &Path, relative: &str) -> Result<(), AppError> {
    let normalized = normalize_relative_path(relative)?;
    let mut current = root.to_path_buf();
    for segment in normalized.split('/') {
        current.push(segment);
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if is_link_metadata(&metadata) {
                return Err(AppError::PathSecurity(format!(
                    "symlink or junction component is not allowed: {}",
                    current.display()
                )));
            }
        }
    }
    Ok(())
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

pub fn validate_manifest_destinations(components: &[ComponentDefinition]) -> Result<(), AppError> {
    let mut owners: std::collections::HashMap<String, Vec<&DestinationDefinition>> =
        std::collections::HashMap::new();
    for component in components {
        let key = canonical_relative_key(&component.destination.path)?;
        owners.entry(key).or_default().push(&component.destination);
    }
    for (key, destinations) in owners {
        // Structured merged contributions such as .codex/config.toml are the
        // only supported shared destination. Every participant must be merged.
        if destinations.len() > 1
            && destinations
                .iter()
                .any(|d| d.ownership != crate::models::Ownership::Merged)
        {
            return Err(AppError::PathSecurity(format!(
                "duplicate destination: {key}"
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

pub fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = File::open(path).map_err(|error| AppError::Transaction(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| AppError::Transaction(error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Transaction("atomic write has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        Uuid::new_v4()
    ));
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_existing_file(&temporary, path)?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| {
        AppError::Transaction(format!(
            "atomic write failed for {}: {error}",
            path.display()
        ))
    })
}

#[cfg(windows)]
fn replace_existing_file(temporary: &Path, destination: &Path) -> Result<(), std::io::Error> {
    use std::os::windows::ffi::OsStrExt;
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let temporary_wide = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if destination.exists() {
        let replaced = unsafe {
            windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
                destination_wide.as_ptr(),
                temporary_wide.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if replaced == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    } else {
        fs::rename(temporary, destination)
    }
}

#[cfg(not(windows))]
fn replace_existing_file(temporary: &Path, destination: &Path) -> Result<(), std::io::Error> {
    fs::rename(temporary, destination)
}

pub fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let json = serde_json::to_value(value)?;
    reject_secret_like_keys(&json)?;
    let bytes = serde_json::to_vec_pretty(&json)?;
    let text = String::from_utf8_lossy(&bytes);
    if redact_secrets(&text, &[]) != text {
        return Err(AppError::Credential(
            "credential-shaped content is not serializable".into(),
        ));
    }
    atomic_write(path, &bytes)
}

pub fn redact_secrets(value: &str, known_secrets: &[String]) -> String {
    let mut redacted = value.to_string();
    for secret in known_secrets.iter().filter(|secret| !secret.is_empty()) {
        redacted = redacted.replace(secret, "[REDACTED]");
    }

    let patterns = [
        Regex::new(r"(?i)(bearer\s+)[A-Za-z0-9._~+/=-]+"),
        Regex::new(
            r"(?i)((?:authorization|access[_-]?token|refresh[_-]?token|device[_-]?code|user[_-]?code|login[_-]?token)\s*[:=]\s*)[^\s,;&]+",
        ),
        Regex::new(
            r"(?i)([?&](?:code|token|access_token|refresh_token|device_code|user_code)=)[^&#\s]+",
        ),
        Regex::new(r"(?i)(api[_-]?key\s*[=:]\s*)[^\s,;&]+"),
        Regex::new(r"(?i)(meshy_api_key\s*[=:]\s*)[^\s,;&]+"),
        Regex::new(r"\bmsy_[A-Za-z0-9_-]{8,}\b"),
        // Common hosted-provider key shapes. These are intentionally bounded
        // to high-signal prefixes so ordinary model IDs and endpoint names do
        // not become credentials by guesswork.
        Regex::new(r"\bsk-ant-[A-Za-z0-9_-]{20,}\b"),
        Regex::new(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b"),
        Regex::new(r"\bAIza[A-Za-z0-9_-]{20,}\b"),
        Regex::new(r"\bxai-[A-Za-z0-9_-]{20,}\b"),
    ];
    for pattern in patterns.into_iter().flatten() {
        redacted = pattern
            .replace_all(&redacted, |captures: &regex::Captures<'_>| {
                if captures.len() > 1 {
                    format!("{}[REDACTED]", &captures[1])
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
                    let lower = key.to_ascii_lowercase();
                    if matches!(
                        lower.as_str(),
                        "secret_value"
                            | "password"
                            | "token"
                            | "api_key"
                            | "access_token"
                            | "refresh_token"
                            | "device_code"
                            | "user_code"
                            | "credential_value"
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

pub fn is_within(root: &Path, candidate: &Path) -> bool {
    match (fs::canonicalize(root), fs::canonicalize(candidate)) {
        (Ok(root), Ok(candidate)) => candidate.starts_with(root),
        _ => false,
    }
}

pub fn relative_from_root(root: &Path, candidate: &Path) -> Result<String, AppError> {
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| AppError::PathSecurity("path is outside the approved root".into()))?;
    let display = relative.to_string_lossy().replace('\\', "/");
    normalize_relative_path(&display)
}

pub fn path_has_link_component(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(value) => {
                current.push(value);
                if fs::symlink_metadata(&current)
                    .map(|metadata| is_link_metadata(&metadata))
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            Component::CurDir | Component::ParentDir => {}
        }
    }
    false
}

pub fn is_link_metadata(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        for value in ["../secret.txt", "C:/secret.txt", "/tmp/secret", "foo:bar"] {
            assert!(normalize_relative_path(value).is_err(), "{value}");
        }
    }

    #[test]
    fn redacts_known_and_shaped_secrets() {
        let output = redact_secrets(
            "Authorization: Bearer abc123 MESHY_API_KEY=apiKeySuperSecretValue",
            &["abc123".into()],
        );
        assert!(!output.contains("abc123"));
        assert!(!output.contains("apiKeySuperSecretValue"));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_supported_hosted_provider_key_shapes() {
        let keys = [
            format!("{}{}", "sk-ant-", "123456789012345678901234"),
            format!("{}{}", "sk-proj-", "123456789012345678901234"),
            format!("{}{}", "AIza", "123456789012345678901234"),
            format!("{}{}", "xai-", "123456789012345678901234"),
        ];
        let output = redact_secrets(&keys.join(" "), &[]);
        for key in &keys {
            assert!(!output.contains(key));
        }
        assert_eq!(output.matches("[REDACTED]").count(), 4);
    }

    #[test]
    fn canonical_keys_normalize_unicode_before_case_collision_checks() {
        let composed = "localé.txt";
        let decomposed = "locale\u{301}.txt";
        assert_ne!(composed, decomposed);
        assert_eq!(
            canonical_relative_key(composed).unwrap(),
            canonical_relative_key(decomposed).unwrap()
        );
    }

    proptest! {
        #[test]
        fn normalized_paths_never_contain_parent_segments(parts in prop::collection::vec("[a-zA-Z0-9_-]{1,8}", 1..8)) {
            let input = parts.join("/");
            let normalized = normalize_relative_path(&input).unwrap();
            prop_assert!(!normalized.split('/').any(|part| part == ".."));
            prop_assert!(!normalized.starts_with('/'));
        }
    }
}
