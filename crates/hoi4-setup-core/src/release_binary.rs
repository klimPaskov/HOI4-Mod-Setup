//! The verified per-platform release-binary route.
//!
//! An image provider whose route kind is `release_binary` (today only
//! hoi4-imagegen-mcp) is installed from one release archive per platform. The
//! source manifest declares each archive's exact URL, size, SHA-256, kind and
//! the entries to extract. This module only parses and validates those
//! declarations and computes the project layout; it never downloads or writes.
//! The desktop host downloads exactly the declared URL, checks size and
//! SHA-256 before it opens the archive, and extracts only the declared entries.

use crate::models::Platform;
use crate::text::normalize_relative_path;
use crate::AppError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Route kind in `image_generation.providers[].route.kind`.
pub const ROUTE_KIND: &str = "release_binary";
/// The provider, component and client server id Setup manages for it.
pub const PROVIDER_ID: &str = "hoi4_imagegen";
pub const COMPONENT_ID: &str = "imagegen.hoi4_imagegen";
pub const SERVER_ID: &str = "hoi4_imagegen";
/// The only repository a release archive may come from.
pub const RELEASE_REPOSITORY: &str = "klimPaskov/hoi4-imagegen-mcp";
/// Where every version is installed, one folder per version.
pub const INSTALL_ROOT: &str = ".tools/image_generation/hoi4-imagegen";
/// Client entries of the retired GPTImage route. Setup removes a managed one
/// and replaces it with [`SERVER_ID`]; it never installs the route again.
pub const RETIRED_SERVER_IDS: &[&str] = &["hoi4_image_gptimage"];
/// Route kind of the retired GPTImage route (older source manifests).
pub const RETIRED_ROUTE_KIND: &str = "local_package";

pub const MCP_BINARY: &str = "hoi4-imagegen-mcp";
pub const CLI_BINARY: &str = "hoi4-imagegen";

/// An archive larger than this is refused before it is downloaded.
pub const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
/// One extracted file larger than this is refused.
pub const MAX_EXTRACTED_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// All extracted bytes together.
pub const MAX_EXTRACTED_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// Entries an archive may list (declared or not).
pub const MAX_ARCHIVE_ENTRIES: usize = 4096;

pub const TARGETS: [&str; 5] = [
    "x86_64-pc-windows-msvc",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarGz,
}

impl ArchiveKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "zip" => Some(Self::Zip),
            "tar.gz" => Some(Self::TarGz),
            _ => None,
        }
    }
}

/// One platform's archive, as declared by the source manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseTarget {
    pub triple: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub archive: ArchiveKind,
    /// Archive path of `hoi4-imagegen-mcp[.exe]`.
    pub mcp_entry: String,
    /// Archive path of `hoi4-imagegen[.exe]`.
    pub cli_entry: String,
}

impl ReleaseTarget {
    pub fn is_windows(&self) -> bool {
        self.triple.contains("windows")
    }

    /// The declared entries and the file name each is installed under.
    pub fn entries(&self) -> [(&str, String); 2] {
        let suffix = if self.is_windows() { ".exe" } else { "" };
        [
            (self.mcp_entry.as_str(), format!("{MCP_BINARY}{suffix}")),
            (self.cli_entry.as_str(), format!("{CLI_BINARY}{suffix}")),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseBinaryRoute {
    pub version: String,
    pub targets: BTreeMap<String, ReleaseTarget>,
}

/// The target triple of a host, or `None` when no release is built for it.
pub fn host_triple(platform: Platform, arch: &str) -> Option<&'static str> {
    match (platform, arch) {
        (Platform::Windows, "x86_64") => Some("x86_64-pc-windows-msvc"),
        (Platform::Macos, "aarch64") => Some("aarch64-apple-darwin"),
        (Platform::Macos, "x86_64") => Some("x86_64-apple-darwin"),
        (Platform::Linux, "x86_64") => Some("x86_64-unknown-linux-gnu"),
        (Platform::Linux, "aarch64") => Some("aarch64-unknown-linux-gnu"),
        _ => None,
    }
}

/// Whether a provider row is the retired GPTImage route.
pub fn is_retired_provider(provider: &Value) -> bool {
    provider.pointer("/route/kind").and_then(Value::as_str) == Some(RETIRED_ROUTE_KIND)
}

pub fn is_release_binary_provider(provider: &Value) -> bool {
    provider.pointer("/route/kind").and_then(Value::as_str) == Some(ROUTE_KIND)
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Source(format!("release binary route: {}", message.into()))
}

fn strict_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 6
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (*part == "0" || !part.starts_with('0'))
        })
}

/// The one URL a target may use. It is derived, not trusted: the manifest's
/// `url` must equal it exactly, so only github.com release downloads of the
/// pinned repository, version and platform are ever requested.
pub fn expected_url(version: &str, triple: &str, archive: ArchiveKind) -> String {
    format!(
        "https://github.com/{RELEASE_REPOSITORY}/releases/download/v{version}/hoi4-imagegen-v{version}-{triple}.{}",
        archive.extension()
    )
}

fn entry_path(value: &str, expected_name: &str) -> Result<String, AppError> {
    let normalized = normalize_relative_path(value)
        .map_err(|_| invalid(format!("unsafe archive entry path: {value}")))?;
    if normalized != value
        || normalized.rsplit('/').next() != Some(expected_name)
        || normalized.split('/').count() > 3
    {
        return Err(invalid(format!(
            "the archive entry must be a normalized path ending in {expected_name}: {value}"
        )));
    }
    Ok(normalized)
}

/// Parse and validate one platform's declaration.
pub fn parse_target(version: &str, triple: &str, value: &Value) -> Result<ReleaseTarget, AppError> {
    if !strict_version(version) {
        return Err(invalid("version must be a plain major.minor.patch"));
    }
    if !TARGETS.contains(&triple) {
        return Err(invalid(format!("unsupported target {triple}")));
    }
    let archive = value
        .get("archive")
        .and_then(Value::as_str)
        .and_then(ArchiveKind::parse)
        .ok_or_else(|| invalid(format!("{triple}: archive must be zip or tar.gz")))?;
    if (archive == ArchiveKind::Zip) != triple.contains("windows") {
        return Err(invalid(format!(
            "{triple}: Windows uses zip and every other platform uses tar.gz"
        )));
    }
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{triple}: url is missing")))?;
    if url != expected_url(version, triple, archive) {
        return Err(invalid(format!(
            "{triple}: url must be the {RELEASE_REPOSITORY} release download for v{version}"
        )));
    }
    let sha256 = value
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{triple}: sha256 is missing")))?;
    crate::manifest::validate_sha256(sha256)?;
    let size = value
        .get("size")
        .and_then(Value::as_u64)
        .filter(|size| (1..=MAX_ARCHIVE_BYTES).contains(size))
        .ok_or_else(|| invalid(format!("{triple}: size is missing or too large")))?;
    let suffix = if triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let entries = value
        .get("entries")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid(format!("{triple}: entries are missing")))?;
    if entries.len() != 2 {
        return Err(invalid(format!("{triple}: declare exactly mcp and cli")));
    }
    let mcp_entry = entry_path(
        entries.get("mcp").and_then(Value::as_str).unwrap_or(""),
        &format!("{MCP_BINARY}{suffix}"),
    )?;
    let cli_entry = entry_path(
        entries.get("cli").and_then(Value::as_str).unwrap_or(""),
        &format!("{CLI_BINARY}{suffix}"),
    )?;
    if mcp_entry == cli_entry {
        return Err(invalid(format!("{triple}: the entries must differ")));
    }
    Ok(ReleaseTarget {
        triple: triple.to_owned(),
        url: url.to_owned(),
        sha256: sha256.to_owned(),
        size,
        archive,
        mcp_entry,
        cli_entry,
    })
}

impl ReleaseBinaryRoute {
    /// Parse and validate `route.release_binary` of an image provider row.
    pub fn from_provider(provider: &Value) -> Result<Self, AppError> {
        let route = provider
            .get("route")
            .ok_or_else(|| invalid("the provider has no route"))?;
        if route.get("kind").and_then(Value::as_str) != Some(ROUTE_KIND) {
            return Err(invalid("the provider route is not a release binary route"));
        }
        let declared = route
            .get("release_binary")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("the route has no release_binary declaration"))?;
        let version = declared
            .get("version")
            .and_then(Value::as_str)
            .filter(|version| strict_version(version))
            .ok_or_else(|| invalid("version must be a plain major.minor.patch"))?;
        if route.get("package_version").and_then(Value::as_str) != Some(version) {
            return Err(invalid("package_version must equal the release version"));
        }
        if declared.get("install_root").and_then(Value::as_str) != Some(INSTALL_ROOT) {
            return Err(invalid(format!("install_root must be {INSTALL_ROOT}")));
        }
        let declared_targets = declared
            .get("targets")
            .and_then(Value::as_object)
            .filter(|targets| !targets.is_empty())
            .ok_or_else(|| invalid("no platform targets are declared"))?;
        let mut targets = BTreeMap::new();
        for (triple, value) in declared_targets {
            targets.insert(triple.clone(), parse_target(version, triple, value)?);
        }
        Ok(Self {
            version: version.to_owned(),
            targets,
        })
    }

    /// The archive for a host, or a clear unsupported-platform error.
    pub fn target_for(&self, platform: Platform, arch: &str) -> Result<&ReleaseTarget, AppError> {
        host_triple(platform, arch)
            .and_then(|triple| self.targets.get(triple))
            .ok_or_else(|| {
                AppError::UnsupportedPlatform(format!(
                    "hoi4-imagegen has no release for {} ({arch})",
                    platform.display_name()
                ))
            })
    }

    /// Project-relative folder of this version.
    pub fn version_dir(&self) -> String {
        version_dir(&self.version)
    }

    pub fn mcp_relative(&self, windows: bool) -> String {
        binary_relative(&self.version, MCP_BINARY, windows)
    }

    pub fn cli_relative(&self, windows: bool) -> String {
        binary_relative(&self.version, CLI_BINARY, windows)
    }
}

pub fn version_dir(version: &str) -> String {
    format!("{INSTALL_ROOT}/{version}")
}

pub fn binary_relative(version: &str, binary: &str, windows: bool) -> String {
    format!(
        "{}/{binary}{}",
        version_dir(version),
        if windows { ".exe" } else { "" }
    )
}

/// The client launch for a release-binary provider: the project-relative
/// server binary and `--project-root .`, run from the project root.
pub fn client_route(provider: &Value, windows: bool) -> Option<Value> {
    let route = ReleaseBinaryRoute::from_provider(provider).ok()?;
    Some(serde_json::json!({
        "command": route.mcp_relative(windows),
        "args": ["--project-root", "."],
        "cwd": ".",
    }))
}

/// The client server id a provider registers.
pub fn server_id(provider: &Value) -> Option<String> {
    provider
        .pointer("/route/server_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Check a downloaded archive against its declaration. Run this before the
/// archive is opened.
pub fn verify_download(bytes: &[u8], target: &ReleaseTarget) -> Result<(), AppError> {
    if bytes.len() as u64 != target.size {
        return Err(AppError::Source(format!(
            "the hoi4-imagegen download is {} bytes but the manifest declares {}",
            bytes.len(),
            target.size
        )));
    }
    if sha256_hex(bytes) != target.sha256 {
        return Err(AppError::Source(
            "the hoi4-imagegen download does not match its declared SHA-256".into(),
        ));
    }
    Ok(())
}

/// One extracted, installed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

/// A stable hash of an installed version: the sorted `name`, NUL, SHA-256 and
/// newline of every file. The lock records it so a later update or removal
/// deletes only a tree that is still exactly what Setup installed.
pub fn tree_sha256(files: &[InstalledFile]) -> String {
    let mut sorted = files.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.name.cmp(&right.name));
    let mut hasher = Sha256::new();
    for file in sorted {
        hasher.update(file.name.as_bytes());
        hasher.update([0]);
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

/// Evidence the post-install action returns and the transaction records.
pub const EVIDENCE_VERSION_PREFIX: &str = "release-version:";
pub const EVIDENCE_TREE_PREFIX: &str = "release-tree-sha256:";
pub const EVIDENCE_REMOVED: &str = "release-removed";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn provider_fixture() -> Value {
        let mut targets = serde_json::Map::new();
        for triple in TARGETS {
            let windows = triple.contains("windows");
            let archive = if windows { "zip" } else { "tar.gz" };
            let folder = format!("hoi4-imagegen-v0.2.0-{triple}");
            let suffix = if windows { ".exe" } else { "" };
            targets.insert(
                triple.into(),
                json!({
                    "url": expected_url("0.2.0", triple, if windows { ArchiveKind::Zip } else { ArchiveKind::TarGz }),
                    "sha256": "a".repeat(64),
                    "size": 1234,
                    "archive": archive,
                    "entries": {
                        "mcp": format!("{folder}/hoi4-imagegen-mcp{suffix}"),
                        "cli": format!("{folder}/hoi4-imagegen{suffix}"),
                    }
                }),
            );
        }
        json!({
            "id": "hoi4_imagegen",
            "route": {
                "kind": "release_binary",
                "server_id": "hoi4_imagegen",
                "package_version": "0.2.0",
                "release_binary": {
                    "version": "0.2.0",
                    "install_root": INSTALL_ROOT,
                    "targets": targets,
                }
            }
        })
    }

    #[test]
    fn parses_every_declared_platform() {
        let route = ReleaseBinaryRoute::from_provider(&provider_fixture()).unwrap();
        assert_eq!(route.targets.len(), 5);
        let windows = route.target_for(Platform::Windows, "x86_64").unwrap();
        assert!(windows.url.ends_with("-x86_64-pc-windows-msvc.zip"));
        assert_eq!(windows.archive, ArchiveKind::Zip);
        let mac = route.target_for(Platform::Macos, "aarch64").unwrap();
        assert_eq!(mac.triple, "aarch64-apple-darwin");
        assert_eq!(mac.archive, ArchiveKind::TarGz);
        assert_eq!(
            route.target_for(Platform::Linux, "aarch64").unwrap().triple,
            "aarch64-unknown-linux-gnu"
        );
        assert_eq!(
            route.mcp_relative(true),
            ".tools/image_generation/hoi4-imagegen/0.2.0/hoi4-imagegen-mcp.exe"
        );
        assert_eq!(
            route.cli_relative(false),
            ".tools/image_generation/hoi4-imagegen/0.2.0/hoi4-imagegen"
        );
    }

    #[test]
    fn an_unbuilt_platform_is_unsupported_not_guessed() {
        let route = ReleaseBinaryRoute::from_provider(&provider_fixture()).unwrap();
        assert!(matches!(
            route.target_for(Platform::Windows, "aarch64"),
            Err(AppError::UnsupportedPlatform(_))
        ));
        let mut partial = provider_fixture();
        partial["route"]["release_binary"]["targets"]
            .as_object_mut()
            .unwrap()
            .remove("x86_64-apple-darwin");
        let route = ReleaseBinaryRoute::from_provider(&partial).unwrap();
        assert!(route.target_for(Platform::Macos, "x86_64").is_err());
        assert!(route.target_for(Platform::Macos, "aarch64").is_ok());
    }

    #[test]
    fn rejects_urls_outside_the_pinned_release_download() {
        for bad in [
            "http://github.com/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.0/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip",
            "https://evil.example/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.0/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip",
            "https://github.com/other/hoi4-imagegen-mcp/releases/download/v0.2.0/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip",
            "https://github.com/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.1/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip",
            "https://github.com/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.0/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip?x=1",
            "https://user@github.com/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.0/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip",
            "https://github.com:444/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.0/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip",
        ] {
            let mut provider = provider_fixture();
            provider["route"]["release_binary"]["targets"]["x86_64-pc-windows-msvc"]["url"] =
                json!(bad);
            assert!(ReleaseBinaryRoute::from_provider(&provider).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_bad_hashes_sizes_entries_and_archive_kinds() {
        let cases: Vec<(&str, Value)> = vec![
            ("sha256", json!("ABC")),
            ("sha256", json!("A".repeat(64))),
            ("size", json!(0)),
            ("size", json!(MAX_ARCHIVE_BYTES + 1)),
            ("archive", json!("zip")),
            ("archive", json!("tar")),
            (
                "entries",
                json!({"mcp": "../hoi4-imagegen-mcp", "cli": "x/hoi4-imagegen"}),
            ),
            (
                "entries",
                json!({"mcp": "/abs/hoi4-imagegen-mcp", "cli": "x/hoi4-imagegen"}),
            ),
            (
                "entries",
                json!({"mcp": "x/other-binary", "cli": "x/hoi4-imagegen"}),
            ),
            (
                "entries",
                json!({"mcp": "x\\hoi4-imagegen-mcp", "cli": "x/hoi4-imagegen"}),
            ),
            ("entries", json!({"mcp": "x/hoi4-imagegen-mcp"})),
        ];
        for (field, value) in cases {
            let mut provider = provider_fixture();
            provider["route"]["release_binary"]["targets"]["aarch64-apple-darwin"][field] =
                value.clone();
            assert!(
                ReleaseBinaryRoute::from_provider(&provider).is_err(),
                "{field} {value}"
            );
        }
        let mut provider = provider_fixture();
        provider["route"]["release_binary"]["install_root"] = json!(".tools/elsewhere");
        assert!(ReleaseBinaryRoute::from_provider(&provider).is_err());
        let mut provider = provider_fixture();
        provider["route"]["package_version"] = json!("0.3.0");
        assert!(ReleaseBinaryRoute::from_provider(&provider).is_err());
        let mut provider = provider_fixture();
        provider["route"]["release_binary"]["version"] = json!("0.2");
        assert!(ReleaseBinaryRoute::from_provider(&provider).is_err());
    }

    #[test]
    fn download_verification_checks_size_before_hash() {
        let route = ReleaseBinaryRoute::from_provider(&provider_fixture()).unwrap();
        let mut target = route.targets["x86_64-unknown-linux-gnu"].clone();
        let bytes = b"archive bytes".to_vec();
        target.size = bytes.len() as u64;
        target.sha256 = sha256_hex(&bytes);
        verify_download(&bytes, &target).unwrap();
        let mut changed = bytes.clone();
        changed[0] ^= 1;
        assert!(verify_download(&changed, &target).is_err());
        assert!(verify_download(&bytes[1..], &target).is_err());
    }

    #[test]
    fn tree_hash_is_order_independent_and_content_bound() {
        let a = InstalledFile {
            name: "hoi4-imagegen".into(),
            sha256: "1".repeat(64),
            size: 1,
        };
        let b = InstalledFile {
            name: "hoi4-imagegen-mcp".into(),
            sha256: "2".repeat(64),
            size: 2,
        };
        assert_eq!(
            tree_sha256(&[a.clone(), b.clone()]),
            tree_sha256(&[b.clone(), a.clone()])
        );
        let mut changed = b.clone();
        changed.sha256 = "3".repeat(64);
        assert_ne!(tree_sha256(&[a.clone(), b]), tree_sha256(&[a, changed]));
    }

    #[test]
    fn client_route_is_project_relative_with_the_project_root_argument() {
        let route = client_route(&provider_fixture(), true).unwrap();
        assert_eq!(
            route["command"],
            ".tools/image_generation/hoi4-imagegen/0.2.0/hoi4-imagegen-mcp.exe"
        );
        assert_eq!(route["args"], json!(["--project-root", "."]));
        assert_eq!(route["cwd"], ".");
        let unix = client_route(&provider_fixture(), false).unwrap();
        assert_eq!(
            unix["command"],
            ".tools/image_generation/hoi4-imagegen/0.2.0/hoi4-imagegen-mcp"
        );
    }
}
