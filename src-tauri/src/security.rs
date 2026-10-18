use crate::safe_fs::RootedDir;
use crate::AppError;
pub use hoi4_setup_core::manifest::validate_manifest_destinations;
pub use hoi4_setup_core::text::{
    canonical_relative_key, contains_credential_shaped_content, is_reserved_windows_name,
    normalize_relative_path, persistable_json_bytes, redact_secrets, reject_secret_like_keys,
    sha256_bytes, validate_destination_set, validate_env_name,
};
use std::fs;
use std::path::{Component, Path, PathBuf};

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
    let (root_canonical, _) = crate::paths::validate_project_root_or_destination(root)?;
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
    let root_exists = root_canonical.exists();
    let contained = if root_exists {
        existing_canonical.starts_with(&root_canonical)
    } else {
        root_canonical
            .parent()
            .is_some_and(|parent| existing_canonical == parent)
    };
    if !contained {
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

pub fn sha256_file(path: &Path) -> Result<String, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::PathSecurity("hashed file has no parent directory".into()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::PathSecurity("hashed file name is not valid UTF-8".into()))?;
    RootedDir::open_read(parent)?.hash_file(name)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Transaction("atomic write has no parent".into()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AppError::PathSecurity("atomic write file name is not valid UTF-8".into())
        })?;
    RootedDir::open_or_create(parent)?.write_atomic(name, bytes)
}

pub fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    atomic_write(path, &persistable_json_bytes(value)?)
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
                    // macOS exposes /etc, /tmp, and /var as stable operating
                    // system aliases into /private. They are outside any
                    // selected project root; allow only these exact aliases
                    // and continue rejecting links below them.
                    #[cfg(target_os = "macos")]
                    if matches!(current.to_str(), Some("/etc" | "/tmp" | "/var")) {
                        continue;
                    }
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
    fn rejects_secret_keys_across_common_serialized_spellings() {
        for key in [
            "apiKey",
            "api_key",
            "api-key",
            "clientSecret",
            "authorization",
            "accessToken",
            "refresh-token",
            "device.code",
            "private_key",
        ] {
            let value = serde_json::json!({"nested": {key: "must-not-persist"}});
            assert!(reject_secret_like_keys(&value).is_err(), "{key}");
        }
    }

    #[test]
    fn redacts_quoted_json_secret_values() {
        let field_names = ["apiKey", "clientSecret", "private_key"];
        let values = ["example-secret", "another-secret", "third-secret"];
        let input = format!(
            r#"{{"{}":"{}","{}":"{}","{}":"{}"}}"#,
            field_names[0], values[0], field_names[1], values[1], field_names[2], values[2]
        );
        let output = redact_secrets(&input, &[]);
        for value in values {
            assert!(!output.contains(value));
        }
        assert_eq!(output.matches("[REDACTED]").count(), 3);
    }

    #[test]
    fn redacts_unquoted_client_secret_assignments() {
        let secret_name = ["client", "secret"].join("_");
        let secret_value = ["synthetic", "credential", "value"].join("-");
        let input = format!("provider failed; {secret_name}={secret_value}");
        let output = redact_secrets(&input, &[]);

        assert!(!output.contains(&secret_value));
        assert!(output.contains(&format!("{secret_name}=[REDACTED]")));
        assert!(contains_credential_shaped_content(&input));
    }

    #[test]
    fn json_persistence_accepts_redacted_string_endings_but_rejects_nested_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("journal.json");
        let secret = ["private", "persistence", "value"].join("-");
        let unsafe_message = format!("setup failed: client_secret={secret}");
        let safe =
            serde_json::json!({"stages": [{"evidence": [redact_secrets(&unsafe_message, &[])]}]});
        atomic_write_json(&path, &safe).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap(),
            safe
        );
        assert!(!fs::read_to_string(&path).unwrap().contains(&secret));
        let unsafe_value = serde_json::json!({"stages": [{"evidence": [unsafe_message]}]});
        assert!(atomic_write_json(&path, &unsafe_value).is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap(),
            safe
        );
        // A serialized JSON object embedded inside a string remains untrusted.
        let nested = serde_json::json!({"message": format!(r#"{{"client_secret":"{secret}"}}"#)});
        assert!(atomic_write_json(&path, &nested).is_err());
        for key in [
            "provider.api_key",
            "config-client_secret",
            "custom_access_token",
        ] {
            let disguised = serde_json::json!({(key): "plain-value-without-a-provider-prefix"});
            assert!(atomic_write_json(&path, &disguised).is_err(), "{key}");
        }
    }

    #[test]
    fn serialized_json_allows_only_the_exact_documented_meshy_placeholder() {
        let directory = tempfile::tempdir().unwrap();
        let documented_path = directory.path().join("documented.json");
        let documented = serde_json::json!({
            "documentation": "Use msy_your_actual_key_here as an example only"
        });

        atomic_write_json(&documented_path, &documented).unwrap();
        assert!(fs::read_to_string(&documented_path)
            .unwrap()
            .contains("msy_your_actual_key_here"));

        let real_path = directory.path().join("real.json");
        let real = serde_json::json!({
            "documentation": format!("{}{}", "msy_123456", "7890abcdef")
        });
        assert!(atomic_write_json(&real_path, &real).is_err());

        let extended_path = directory.path().join("extended.json");
        let extended = serde_json::json!({
            "documentation": format!("{}{}", "msy_your_actual_key_here", "123")
        });
        assert!(atomic_write_json(&extended_path, &extended).is_err());
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

    #[test]
    fn canonical_keys_reject_multi_character_case_aliases() {
        assert_eq!(
            canonical_relative_key("straße.txt").unwrap(),
            canonical_relative_key("STRASSE.TXT").unwrap()
        );
        assert_eq!(
            canonical_relative_key("σ.txt").unwrap(),
            canonical_relative_key("ς.TXT").unwrap()
        );
    }

    proptest! {
        #[test]
        fn normalized_paths_never_contain_parent_segments(parts in prop::collection::vec("[a-zA-Z0-9_-]{1,8}", 1..8)) {
            let input = parts.join("/");
            if let Ok(normalized) = normalize_relative_path(&input) {
                prop_assert!(!normalized.split('/').any(|part| part == ".."));
                prop_assert!(!normalized.starts_with('/'));
            }
        }
    }
}
