//! Native Linux executable trust. Distro tools use the root-owned Debian
//! package database; account-bearing clients use publisher HTTPS identities.
//! A discovered filename or a hash observed locally never establishes trust.
use crate::AppError;
use serde_json::Value;
#[cfg(target_os = "linux")]
use std::path::Path;

fn version_token(value: &str) -> Option<&str> {
    let version = value.split('-').next()?;
    let segments = version.split('.').collect::<Vec<_>>();
    (segments.len() == 3
        && segments
            .iter()
            .all(|s| !s.is_empty() && s.len() <= 6 && s.bytes().all(|b| b.is_ascii_digit())))
    .then_some(version)
}

fn publisher_digest(value: &Value) -> Result<String, AppError> {
    if value
        .pointer("/spec/data/hash/algorithm")
        .and_then(Value::as_str)
        != Some("sha256")
    {
        return Err(AppError::Process(
            "publisher identity has no supported digest".into(),
        ));
    }
    let digest = value
        .pointer("/spec/data/hash/value")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Process("publisher identity has no digest".into()))?;
    crate::source::validate_sha256(digest)?;
    Ok(digest.to_owned())
}

fn identity_matches(
    observed: &str,
    observed_size: u64,
    published: &str,
    published_size: Option<u64>,
) -> bool {
    crate::source::validate_sha256(observed).is_ok()
        && crate::source::validate_sha256(published).is_ok()
        && observed == published
        && published_size.is_none_or(|size| size == observed_size)
}

fn publisher_redirect_allowed(url: &reqwest::Url, previous: usize) -> bool {
    previous < 4
        && url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "api.github.com"
                    | "downloads.claude.ai"
            )
        )
}

fn read_publisher_body(
    reader: impl std::io::Read,
    content_length: Option<u64>,
    limit: usize,
) -> Result<Vec<u8>, AppError> {
    use std::io::Read;
    if content_length.is_some_and(|size| size > limit as u64) {
        return Err(AppError::Process(
            "publisher identity exceeded its size limit".into(),
        ));
    }
    let mut bytes = Vec::new();
    reader.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(AppError::Process(
            "publisher identity exceeded its size limit".into(),
        ));
    }
    Ok(bytes)
}

#[cfg(target_os = "linux")]
fn codex_document_digest(value: &Value) -> Result<String, AppError> {
    use base64::Engine;
    let encoded = value
        .pointer("/rekorBundle/Payload/body")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Process("unsupported official client identity format".into()))?;
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| AppError::Process("unreadable official client identity".into()))?;
    publisher_digest(&serde_json::from_slice(&body)?)
}

fn checksum_for_path(text: &str, relative: &str) -> Result<String, AppError> {
    let mut matches = text.lines().filter_map(|line| {
        let (checksum, path) = line.split_once(char::is_whitespace)?;
        (path.trim_start() == relative
            && checksum.len() == 32
            && checksum.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| checksum.to_ascii_lowercase())
    });
    let checksum = matches
        .next()
        .ok_or_else(|| AppError::Process("the executable has no distro checksum record".into()))?;
    if matches.next().is_some() {
        return Err(AppError::Process(
            "the executable has ambiguous distro checksum records".into(),
        ));
    }
    Ok(checksum)
}

#[cfg(target_os = "linux")]
fn root_owned_path(path: &Path, executable: bool) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute() || crate::security::path_has_link_component(path) {
        return Err(AppError::Process(
            "the distro trust path is not canonical and link-free".into(),
        ));
    }
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)?;
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 || metadata.file_type().is_symlink()
        {
            return Err(AppError::Process(
                "the distro trust path is not protected by root ownership".into(),
            ));
        }
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || (executable && metadata.mode() & 0o111 == 0) {
        return Err(AppError::Process(
            "the distro trust file is not a regular executable".into(),
        ));
    }
    Ok(())
}

/// Only audited Debian package identities are accepted. The protected OS
/// package database is the trust anchor; MD5 here is its file-change receipt,
/// while all actual spawns remain bound to a freshly checked SHA-256.
#[cfg(target_os = "linux")]
pub(crate) fn verify_distro_executable(executable: &Path, publisher: &str) -> Result<(), AppError> {
    use md5::{Digest, Md5};
    use std::io::Read;
    root_owned_path(executable, true)?;
    let name = executable
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("");
    let package = match (publisher, name) {
        ("Python Software Foundation", name)
            if name.strip_prefix("python3.").is_some_and(|minor| {
                !minor.is_empty() && minor.len() <= 3 && minor.bytes().all(|b| b.is_ascii_digit())
            }) =>
        {
            format!("{name}-minimal")
        }
        ("OpenJS Foundation", "node") => "nodejs".into(),
        ("Linux system", "xdg-open") => "xdg-utils".into(),
        ("Linux system", "gnome-terminal") => "gnome-terminal".into(),
        ("Linux system", "konsole") => "konsole".into(),
        ("Linux system", "xterm") => "xterm".into(),
        _ => {
            return Err(AppError::Process(
                "this Linux tool has no audited distro identity route".into(),
            ))
        }
    };
    if executable.parent() != Some(Path::new("/usr/bin")) {
        return Err(AppError::Process(
            "the distro executable is outside the reviewed system directory".into(),
        ));
    }
    let info = Path::new("/var/lib/dpkg/info");
    let mut matched = None;
    for suffix in ["", ":amd64", ":arm64"] {
        let file = format!("{package}{suffix}.md5sums");
        let receipt = info.join(&file);
        if !receipt.exists() {
            continue;
        }
        root_owned_path(&receipt, false)?;
        let bytes = crate::flatten::read_bounded_regular_file_no_follow_under_root(
            info,
            &file,
            8 * 1024 * 1024,
        )?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| AppError::Process("the distro receipt is unreadable".into()))?;
        let relative = executable
            .strip_prefix("/")
            .unwrap()
            .to_str()
            .ok_or_else(|| AppError::Process("the distro executable path is invalid".into()))?;
        if let Ok(checksum) = checksum_for_path(text, relative) {
            if matched.replace(checksum).is_some() {
                return Err(AppError::Process(
                    "the executable has multiple distro receipts".into(),
                ));
            }
        }
    }
    let expected = matched.ok_or_else(|| {
        AppError::Process(
            "no supported Debian/Ubuntu package receipt identifies this executable".into(),
        )
    })?;
    let mut file = std::fs::File::open(executable)?;
    let mut hasher = Md5::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    if hex::encode(hasher.finalize()) != expected {
        return Err(AppError::Process(
            "the executable differs from its protected distro checksum".into(),
        ));
    }
    root_owned_path(executable, true)
}

#[cfg(target_os = "linux")]
fn publisher_bytes(url: &str, limit: usize) -> Result<Vec<u8>, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if !publisher_redirect_allowed(attempt.url(), attempt.previous().len()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| AppError::Process("publisher identity client is unavailable".into()))?;
    let response = client.get(url).header("User-Agent", "HOI4-Mod-Setup").send()
        .and_then(reqwest::blocking::Response::error_for_status).map_err(|_| AppError::Process("publisher identity could not be fetched; reconnect and check the official client installation".into()))?;
    let content_length = response.content_length();
    read_publisher_body(response, content_length, limit)
}

#[cfg(target_os = "linux")]
pub(crate) fn verify_account_executable(
    executable: &Path,
    publisher: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::metadata(executable)?.permissions().mode() & 0o111 == 0 {
        return Err(AppError::Process(
            "the official client is not executable".into(),
        ));
    }
    let observed = crate::security::sha256_file(executable)?;
    let mut versions = executable
        .ancestors()
        .take(8)
        .filter_map(|path| {
            path.file_name()?
                .to_str()
                .and_then(version_token)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    // npm metadata supplies only a version candidate; it is never trust proof.
    if publisher == "OpenAI" {
        for parent in executable.ancestors().take(8) {
            if parent.ends_with("node_modules/@openai/codex")
                || parent
                    .file_name()
                    .is_some_and(|name| name == "codex-linux-x64" || name == "codex-linux-arm64")
            {
                if let Ok(bytes) = crate::flatten::read_bounded_regular_file_no_follow_under_root(
                    parent,
                    "package.json",
                    64 * 1024,
                ) {
                    if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                        if let Some(version) = value
                            .get("version")
                            .and_then(Value::as_str)
                            .and_then(version_token)
                        {
                            versions.push(version.into());
                        }
                    }
                }
            }
        }
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => {
            return Err(AppError::UnsupportedPlatform(
                "no verified official Linux client architecture".into(),
            ))
        }
    };
    match publisher {
        "Anthropic" => {
            if versions.is_empty() {
                let bytes = publisher_bytes(
                    "https://downloads.claude.ai/claude-code-releases/latest",
                    128,
                )?;
                if let Some(version) = std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|s| version_token(s.trim()))
                {
                    versions.push(version.into());
                }
            }
            versions.sort();
            versions.dedup();
            versions.truncate(4);
            for version in versions {
                let bytes = publisher_bytes(
                    &format!(
                        "https://downloads.claude.ai/claude-code-releases/{version}/manifest.json"
                    ),
                    64 * 1024,
                )?;
                let value: Value = serde_json::from_slice(&bytes)?;
                for platform in [format!("linux-{arch}"), format!("linux-{arch}-musl")] {
                    let row = &value["platforms"][&platform];
                    if row["checksum"].as_str().is_some_and(|digest| {
                        identity_matches(
                            &observed,
                            std::fs::metadata(executable)
                                .map(|m| m.len())
                                .unwrap_or(u64::MAX),
                            digest,
                            row["size"].as_u64(),
                        )
                    }) && row["size"].is_u64()
                    {
                        return Ok(());
                    }
                }
            }
        }
        "OpenAI" => {
            if versions.is_empty() {
                let bytes = publisher_bytes(
                    "https://api.github.com/repos/openai/codex/releases/latest",
                    2 * 1024 * 1024,
                )?;
                let value: Value = serde_json::from_slice(&bytes)?;
                if let Some(version) = value["tag_name"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("rust-v"))
                    .and_then(version_token)
                {
                    versions.push(version.into());
                }
            }
            versions.sort();
            versions.dedup();
            versions.truncate(4);
            let target_arch = if arch == "x64" { "x86_64" } else { "aarch64" };
            for version in versions {
                for abi in ["musl", "gnu"] {
                    let url = format!("https://github.com/openai/codex/releases/download/rust-v{version}/codex-{target_arch}-unknown-linux-{abi}.sigstore");
                    let Ok(bytes) = publisher_bytes(&url, 64 * 1024) else {
                        continue;
                    };
                    let value: Value = serde_json::from_slice(&bytes)?;
                    let digest = codex_document_digest(&value)?;
                    if identity_matches(&observed, 0, &digest, None) {
                        return Ok(());
                    }
                }
            }
        }
        _ => {
            return Err(AppError::Process(
                "no official Linux client identity route for this publisher".into(),
            ))
        }
    }
    Err(AppError::Process(
        "the client bytes do not match a published official Linux identity".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_redirects_require_exact_https_origins_and_a_bound() {
        let valid = reqwest::Url::parse(
            "https://release-assets.githubusercontent.com/asset?signature=public",
        )
        .unwrap();
        assert!(publisher_redirect_allowed(&valid, 3));
        assert!(!publisher_redirect_allowed(&valid, 4));
        for url in [
            "http://github.com/asset",
            "https://github.com:8443/asset",
            "https://github.com.attacker.invalid/asset",
            "https://attacker.invalid/asset",
            "https://user@github.com/asset",
        ] {
            assert!(!publisher_redirect_allowed(
                &reqwest::Url::parse(url).unwrap(),
                0
            ));
        }
    }

    #[test]
    fn publisher_payloads_are_bounded_with_or_without_a_length_header() {
        assert_eq!(
            read_publisher_body(std::io::Cursor::new(b"1234"), Some(4), 4).unwrap(),
            b"1234"
        );
        assert!(read_publisher_body(std::io::Cursor::new(b"12345"), None, 4).is_err());
        assert!(read_publisher_body(std::io::Cursor::new(b""), Some(5), 4).is_err());
        assert!(read_publisher_body(std::io::Cursor::new(b"12345"), Some(2), 4).is_err());
    }

    #[test]
    fn publisher_identity_rejects_digest_and_size_mismatch() {
        let observed = "a".repeat(64);
        assert!(identity_matches(&observed, 123, &observed, Some(123)));
        assert!(!identity_matches(
            &observed,
            123,
            &"b".repeat(64),
            Some(123)
        ));
        assert!(!identity_matches(&observed, 123, &observed, Some(124)));
        assert!(!identity_matches(&observed, 123, "malformed", Some(123)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn codex_publisher_payload_rejects_invalid_base64_and_json() {
        use base64::Engine;
        assert!(codex_document_digest(
            &serde_json::json!({"rekorBundle":{"Payload":{"body":"not base64!"}}})
        )
        .is_err());
        let invalid_json = base64::engine::general_purpose::STANDARD.encode(b"not json");
        assert!(codex_document_digest(
            &serde_json::json!({"rekorBundle":{"Payload":{"body":invalid_json}}})
        )
        .is_err());
    }
    #[test]
    fn only_bounded_numeric_versions_can_enter_publisher_urls() {
        assert_eq!(
            version_token("0.162.1-x86_64-unknown-linux-musl"),
            Some("0.162.1")
        );
        for value in [
            "../../escape",
            "1.2",
            "1.2.3/escape",
            "9999999.1.2",
            "1.2.3?x",
        ] {
            assert!(version_token(value).is_none());
        }
    }
    #[test]
    fn distro_receipts_require_the_exact_path_and_one_well_formed_checksum() {
        let digest = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            checksum_for_path(&format!("{digest}  usr/bin/node\n"), "usr/bin/node").unwrap(),
            digest
        );
        assert!(checksum_for_path(
            &format!("{digest}  usr/bin/node-lookalike\n"),
            "usr/bin/node"
        )
        .is_err());
        assert!(checksum_for_path(
            &format!("{digest}  usr/bin/node\n{digest}  usr/bin/node\n"),
            "usr/bin/node"
        )
        .is_err());
    }
    #[test]
    fn publisher_digest_rejects_missing_or_unsupported_identity() {
        assert!(publisher_digest(&serde_json::json!({})).is_err());
        assert!(publisher_digest(&serde_json::json!({"spec":{"data":{"hash":{"algorithm":"md5","value":"a".repeat(64)}}}})).is_err());
        assert_eq!(publisher_digest(&serde_json::json!({"spec":{"data":{"hash":{"algorithm":"sha256","value":"a".repeat(64)}}}})).unwrap(), "a".repeat(64));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn a_user_owned_lookalike_cannot_claim_distro_trust() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let node = directory.path().join("node");
        std::fs::write(&node, b"untrusted").unwrap();
        std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify_distro_executable(&node, "OpenJS Foundation").is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires supported Debian/Ubuntu system Python and Node packages"]
    fn native_distro_tools_match_protected_package_receipts() {
        for (path, publisher) in [
            ("/usr/bin/python3", "Python Software Foundation"),
            ("/usr/bin/node", "OpenJS Foundation"),
        ] {
            let executable = std::fs::canonicalize(path).unwrap();
            verify_distro_executable(&executable, publisher).unwrap();
            assert!(verify_distro_executable(&executable, "unreviewed publisher").is_err());
        }
    }
}
