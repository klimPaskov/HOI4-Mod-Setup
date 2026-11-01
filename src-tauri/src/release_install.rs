//! Installing a verified per-platform release binary (hoi4-imagegen-mcp).
//!
//! The source manifest declares one archive per platform: its exact URL, size,
//! SHA-256, kind and the two entries to extract. This module downloads only
//! that URL, checks size and SHA-256 before it opens the archive, extracts only
//! the declared entries (refusing traversal, links, encryption and oversized
//! content anywhere in the archive), and writes them to
//! `.tools/image_generation/hoi4-imagegen/<version>/` through the rooted
//! filesystem capability. A version is never overwritten: a different version
//! is installed beside it, and a removal deletes a version only while its file
//! hashes still match what Setup recorded.

use crate::safe_fs::RootedDir;
use crate::AppError;
use hoi4_setup_core::release_binary::{
    self as route, ArchiveKind, InstalledFile, ReleaseTarget, MAX_ARCHIVE_ENTRIES,
    MAX_EXTRACTED_FILE_BYTES, MAX_EXTRACTED_TOTAL_BYTES,
};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

/// One extracted entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFile {
    /// The archive path that was declared.
    pub archive_path: String,
    pub bytes: Vec<u8>,
}

fn refused(message: impl Into<String>) -> AppError {
    AppError::Source(format!("hoi4-imagegen archive: {}", message.into()))
}

/// An archive entry name after separator normalization, or an error for an
/// absolute, drive, traversal, empty or NUL-containing name.
fn safe_archive_name(raw: &str) -> Result<String, AppError> {
    let name = raw.replace('\\', "/");
    if name.is_empty()
        || name.contains('\0')
        || name.starts_with('/')
        || name.chars().nth(1) == Some(':')
        || name.split('/').any(|part| part == "..")
        || name.len() > 1024
    {
        return Err(refused(format!("unsafe entry name {raw:?}")));
    }
    Ok(name.trim_start_matches("./").to_owned())
}

/// Extract exactly the `wanted` entries. Every entry of the archive is
/// inspected for unsafe names and links, not only the wanted ones.
pub fn extract_declared(
    bytes: &[u8],
    kind: ArchiveKind,
    wanted: &[&str],
) -> Result<Vec<ExtractedFile>, AppError> {
    let wanted_set = wanted.iter().copied().collect::<BTreeSet<_>>();
    if wanted_set.len() != wanted.len() {
        return Err(refused("the declared entries are not distinct"));
    }
    let found = match kind {
        ArchiveKind::Zip => extract_zip(bytes, &wanted_set)?,
        ArchiveKind::TarGz => extract_tar_gz(bytes, &wanted_set)?,
    };
    // Return in the declared order and require every declared entry once.
    wanted
        .iter()
        .map(|path| {
            found
                .iter()
                .find(|file| file.archive_path == *path)
                .cloned()
                .ok_or_else(|| refused(format!("the declared entry {path} is missing")))
        })
        .collect()
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| refused(format!("unreadable entry: {error}")))?;
    if bytes.len() as u64 > limit {
        return Err(refused("an entry is larger than its declared size"));
    }
    Ok(bytes)
}

fn extract_tar_gz(bytes: &[u8], wanted: &BTreeSet<&str>) -> Result<Vec<ExtractedFile>, AppError> {
    // Decompression is bounded as a whole so a small archive cannot expand
    // without limit.
    let decoder = flate2::read::GzDecoder::new(bytes).take(MAX_EXTRACTED_TOTAL_BYTES + (16 << 20));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| refused(format!("unreadable tar: {error}")))?;
    let mut found: Vec<ExtractedFile> = Vec::new();
    let mut count = 0usize;
    let mut total = 0u64;
    for entry in entries {
        let mut entry = entry.map_err(|error| refused(format!("unreadable tar entry: {error}")))?;
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(refused("too many entries"));
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file()
            || kind.is_dir()
            || kind.is_gnu_longname()
            || kind.is_pax_global_extensions()
            || kind.is_pax_local_extensions())
        {
            return Err(refused("links and special files are not allowed"));
        }
        let path = entry
            .path()
            .map_err(|error| refused(format!("unreadable entry name: {error}")))?;
        let name = safe_archive_name(&path.to_string_lossy())?;
        if !kind.is_file() {
            continue;
        }
        let size = entry.size();
        total = total.saturating_add(size);
        if size > MAX_EXTRACTED_FILE_BYTES || total > MAX_EXTRACTED_TOTAL_BYTES {
            return Err(refused("the archive is larger than the allowed size"));
        }
        if wanted.contains(name.as_str()) {
            if found.iter().any(|file| file.archive_path == name) {
                return Err(refused(format!("the declared entry {name} appears twice")));
            }
            let content = read_limited(&mut entry, size)?;
            if content.len() as u64 != size {
                return Err(refused("an entry ended before its declared size"));
            }
            found.push(ExtractedFile {
                archive_path: name,
                bytes: content,
            });
        }
    }
    Ok(found)
}

fn le16(bytes: &[u8], at: usize) -> Result<u16, AppError> {
    bytes
        .get(at..at + 2)
        .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
        .ok_or_else(|| refused("truncated zip"))
}

fn le32(bytes: &[u8], at: usize) -> Result<u32, AppError> {
    bytes
        .get(at..at + 4)
        .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
        .ok_or_else(|| refused("truncated zip"))
}

fn extract_zip(bytes: &[u8], wanted: &BTreeSet<&str>) -> Result<Vec<ExtractedFile>, AppError> {
    const EOCD: u32 = 0x0605_4b50;
    const CENTRAL: u32 = 0x0201_4b50;
    const LOCAL: u32 = 0x0403_4b50;
    let search_from = bytes.len().saturating_sub(22 + 65_535);
    let eocd = (search_from..bytes.len().saturating_sub(21))
        .rev()
        .find(|at| le32(bytes, *at).is_ok_and(|signature| signature == EOCD))
        .ok_or_else(|| refused("not a zip archive"))?;
    let entries = le16(bytes, eocd + 10)? as usize;
    let directory_size = le32(bytes, eocd + 12)? as usize;
    let directory_offset = le32(bytes, eocd + 16)? as usize;
    if entries == 0xFFFF || directory_size == 0xFFFF_FFFF || directory_offset == 0xFFFF_FFFF {
        return Err(refused("zip64 archives are not supported"));
    }
    if entries > MAX_ARCHIVE_ENTRIES {
        return Err(refused("too many entries"));
    }
    if directory_offset
        .checked_add(directory_size)
        .is_none_or(|end| end > bytes.len())
    {
        return Err(refused("the zip directory is outside the archive"));
    }
    let mut at = directory_offset;
    let mut found: Vec<ExtractedFile> = Vec::new();
    let mut total = 0u64;
    for _ in 0..entries {
        if le32(bytes, at)? != CENTRAL {
            return Err(refused("corrupt zip directory"));
        }
        let made_by = le16(bytes, at + 4)?;
        let flags = le16(bytes, at + 8)?;
        let method = le16(bytes, at + 10)?;
        let crc = le32(bytes, at + 16)?;
        let compressed = le32(bytes, at + 20)? as usize;
        let size = le32(bytes, at + 24)? as u64;
        let name_length = le16(bytes, at + 28)? as usize;
        let extra_length = le16(bytes, at + 30)? as usize;
        let comment_length = le16(bytes, at + 32)? as usize;
        let attributes = le32(bytes, at + 38)?;
        let local_offset = le32(bytes, at + 42)? as usize;
        let raw_name = bytes
            .get(at + 46..at + 46 + name_length)
            .ok_or_else(|| refused("truncated zip"))?;
        let raw_name =
            std::str::from_utf8(raw_name).map_err(|_| refused("an entry name is not UTF-8"))?;
        at += 46 + name_length + extra_length + comment_length;
        let name = safe_archive_name(raw_name)?;
        // Unix-made entries carry the file mode in the high attribute bits.
        if made_by >> 8 == 3 && (attributes >> 16) & 0o170_000 == 0o120_000 {
            return Err(refused("symbolic links are not allowed"));
        }
        if flags & 1 != 0 {
            return Err(refused("encrypted entries are not allowed"));
        }
        if name.ends_with('/') {
            continue;
        }
        total = total.saturating_add(size);
        if size > MAX_EXTRACTED_FILE_BYTES || total > MAX_EXTRACTED_TOTAL_BYTES {
            return Err(refused("the archive is larger than the allowed size"));
        }
        if !wanted.contains(name.as_str()) {
            continue;
        }
        if found.iter().any(|file| file.archive_path == name) {
            return Err(refused(format!("the declared entry {name} appears twice")));
        }
        if le32(bytes, local_offset)? != LOCAL {
            return Err(refused("corrupt zip entry header"));
        }
        let data_start = local_offset
            + 30
            + le16(bytes, local_offset + 26)? as usize
            + le16(bytes, local_offset + 28)? as usize;
        let data = bytes
            .get(data_start..data_start.checked_add(compressed).unwrap_or(usize::MAX))
            .ok_or_else(|| refused("truncated zip entry"))?;
        let content = match method {
            0 => {
                if compressed as u64 != size {
                    return Err(refused("a stored entry has inconsistent sizes"));
                }
                data.to_vec()
            }
            8 => read_limited(flate2::read::DeflateDecoder::new(data), size)?,
            _ => return Err(refused("unsupported compression method")),
        };
        if content.len() as u64 != size {
            return Err(refused("an entry does not match its declared size"));
        }
        let mut checksum = flate2::Crc::new();
        checksum.update(&content);
        if checksum.sum() != crc {
            return Err(refused("an entry fails its checksum"));
        }
        found.push(ExtractedFile {
            archive_path: name,
            bytes: content,
        });
    }
    Ok(found)
}

/// Downloads the bytes of one declared archive URL.
pub trait ReleaseDownloader {
    fn download(&self, url: &str, size: u64) -> Result<Vec<u8>, AppError>;
}

pub(crate) fn release_redirect_allowed(url: &reqwest::Url, previous: usize) -> bool {
    previous < 4
        && url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "objects.githubusercontent.com"
                    | "release-assets.githubusercontent.com"
            )
        )
}

/// GitHub serves a release asset by redirecting to its storage host, so this
/// client follows at most four redirects, each of which must stay on an
/// allowlisted HTTPS host.
pub struct HttpReleaseDownloader {
    client: reqwest::blocking::Client,
}

impl HttpReleaseDownloader {
    pub fn new() -> Result<Self, AppError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("HOI4-Mod-Setup/0.1")
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(600))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if release_redirect_allowed(attempt.url(), attempt.previous().len()) {
                    attempt.follow()
                } else {
                    attempt.error("the release redirect left the allowed hosts")
                }
            }))
            .build()
            .map_err(|error| AppError::Source(format!("HTTP client: {error}")))?;
        Ok(Self { client })
    }
}

impl ReleaseDownloader for HttpReleaseDownloader {
    fn download(&self, url: &str, size: u64) -> Result<Vec<u8>, AppError> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|error| AppError::Source(format!("invalid release URL: {error}")))?;
        if !release_redirect_allowed(&parsed, 0) || parsed.host_str() != Some("github.com") {
            return Err(AppError::Source(
                "the release URL is not an approved GitHub download".into(),
            ));
        }
        let response = self
            .client
            .get(url)
            .header("Accept", "application/octet-stream")
            .send()
            .map_err(|error| AppError::Source(format!("release download failed: {error}")))?;
        if !response.status().is_success() {
            return Err(AppError::Source(format!(
                "release download returned HTTP {}",
                response.status()
            )));
        }
        if !release_redirect_allowed(response.url(), 0) {
            return Err(AppError::Source(
                "the release download ended at an unapproved host".into(),
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length != size)
        {
            return Err(AppError::Source(
                "the release download size differs from the manifest".into(),
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(size + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| AppError::Source(format!("release download interrupted: {error}")))?;
        Ok(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReceipt {
    pub version: String,
    pub tree_sha256: String,
    pub files: Vec<InstalledFile>,
    /// The version was already installed and unchanged; nothing was downloaded
    /// or written.
    pub reused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveResult {
    Removed,
    AlreadyAbsent,
    ModifiedKept,
}

fn directory_name(version: &str) -> Result<String, AppError> {
    if version.split('.').count() != 3
        || !version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err(AppError::PathSecurity(
            "the hoi4-imagegen version is not a plain major.minor.patch".into(),
        ));
    }
    Ok(route::version_dir(version))
}

/// The files of an installed version and their tree hash, or `None` when the
/// version is not installed. An entry that is not a regular file makes the
/// tree unusable and is an error.
pub fn installed_tree(
    root: &Path,
    version: &str,
) -> Result<Option<(Vec<InstalledFile>, String)>, AppError> {
    let relative = directory_name(version)?;
    let project = RootedDir::open_read(root)?;
    if !project.is_directory(&relative)? {
        return Ok(None);
    }
    let directory = project.open_dir(&relative)?;
    let mut files = Vec::new();
    for name in directory.read_dir_names()? {
        let name = name
            .to_str()
            .ok_or_else(|| AppError::PathSecurity("an installed file name is not UTF-8".into()))?
            .to_owned();
        if !directory.is_regular_file(&name)? {
            return Err(AppError::PathSecurity(format!(
                "the installed entry {name} is not a regular file"
            )));
        }
        let sha256 = directory.hash_file(&name)?;
        let size = std::fs::metadata(root.join(&relative).join(&name))
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        files.push(InstalledFile { name, sha256, size });
    }
    let tree = route::tree_sha256(&files);
    Ok(Some((files, tree)))
}

/// Install one verified release. `known_tree` is the tree hash the lock
/// recorded for this version: when the installed files still match it, the
/// install is a no-op.
pub fn install_release(
    root: &Path,
    version: &str,
    target: &ReleaseTarget,
    known_tree: Option<&str>,
    downloader: &dyn ReleaseDownloader,
) -> Result<InstallReceipt, AppError> {
    let relative = directory_name(version)?;
    if let Some(known) = known_tree {
        if let Some((files, tree)) = installed_tree(root, version).ok().flatten() {
            if tree == known {
                return Ok(InstallReceipt {
                    version: version.to_owned(),
                    tree_sha256: tree,
                    files,
                    reused: true,
                });
            }
        }
    }
    // Only the declared URL is requested; its size and SHA-256 are checked
    // before the archive is opened.
    if target.url != route::expected_url(version, &target.triple, target.archive) {
        return Err(AppError::Source(
            "the release URL does not match the pinned release".into(),
        ));
    }
    let archive = downloader.download(&target.url, target.size)?;
    route::verify_download(&archive, target)?;
    let entries = target.entries();
    let wanted = entries.iter().map(|(path, _)| *path).collect::<Vec<_>>();
    let extracted = extract_declared(&archive, target.archive, &wanted)?;
    drop(archive);
    let named = entries
        .iter()
        .zip(extracted)
        .map(|((_, name), file)| (name.clone(), file.bytes))
        .collect::<Vec<_>>();
    let expected_files = named
        .iter()
        .map(|(name, bytes)| InstalledFile {
            name: name.clone(),
            sha256: route::sha256_hex(bytes),
            size: bytes.len() as u64,
        })
        .collect::<Vec<_>>();
    let expected_tree = route::tree_sha256(&expected_files);
    let project = RootedDir::open(root)?;
    if project.exists(&relative)? {
        // Never overwrite a folder Setup did not record.
        return match installed_tree(root, version)? {
            Some((files, tree)) if tree == expected_tree => Ok(InstallReceipt {
                version: version.to_owned(),
                tree_sha256: tree,
                files,
                reused: true,
            }),
            _ => Err(AppError::Transaction(format!(
                "{relative} already exists with different content. Remove that folder and run Setup again"
            ))),
        };
    }
    let installed = (|| -> Result<(), AppError> {
        let directory = project
            .ensure_dir(route::INSTALL_ROOT)?
            .create_dir(version)?;
        for (name, bytes) in &named {
            if !directory.write_atomic_noreplace(name, bytes)? {
                return Err(AppError::Transaction(format!(
                    "{name} already exists in the new install folder"
                )));
            }
            #[cfg(unix)]
            directory.set_executable(name, true)?;
        }
        Ok(())
    })();
    let verified = installed.and_then(|()| match installed_tree(root, version)? {
        Some((files, tree)) if tree == expected_tree => Ok(files),
        _ => Err(AppError::Transaction(
            "the installed hoi4-imagegen files do not match the verified archive".into(),
        )),
    });
    match verified {
        Ok(files) => Ok(InstallReceipt {
            version: version.to_owned(),
            tree_sha256: expected_tree,
            files,
            reused: false,
        }),
        Err(error) => {
            // A partial install is never left behind.
            let _ = project.remove_tree(&relative);
            Err(error)
        }
    }
}

/// Remove an installed version, but only while its files still hash to what
/// Setup recorded. A changed folder is kept.
pub fn remove_release(
    root: &Path,
    version: &str,
    expected_tree: &str,
) -> Result<RemoveResult, AppError> {
    hoi4_setup_core::manifest::validate_sha256(expected_tree)?;
    let relative = directory_name(version)?;
    let tree = match installed_tree(root, version) {
        Ok(Some((_, tree))) => tree,
        Ok(None) => return Ok(RemoveResult::AlreadyAbsent),
        Err(_) => return Ok(RemoveResult::ModifiedKept),
    };
    if tree != expected_tree {
        return Ok(RemoveResult::ModifiedKept);
    }
    let project = RootedDir::open(root)?;
    let install_root = project.open_dir(route::INSTALL_ROOT)?;
    let directory = install_root.open_dir(version)?;
    let identity = directory.identity_token()?;
    drop(directory);
    if !install_root.remove_tree_if_identity(version, &identity)? {
        return Ok(RemoveResult::ModifiedKept);
    }
    let _ = relative;
    // Drop the install root once the last version is gone.
    let _ = project.remove_dir_if_empty(route::INSTALL_ROOT);
    Ok(RemoveResult::Removed)
}

/// The version the CLI reports, for example `0.2.0` from `hoi4-imagegen 0.2.0`.
pub fn reported_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|token| token.trim_start_matches('v'))
        .find(|token| {
            let parts = token.split('.').collect::<Vec<_>>();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        })
        .map(ToOwned::to_owned)
}
