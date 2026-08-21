use crate::models::*;
use crate::paths::cache_root;
use crate::security::{
    atomic_write, canonical_relative_key, normalize_relative_path, path_has_link_component,
    sha256_bytes, validate_manifest_destinations,
};
use crate::AppError;
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

pub const SOURCE_REPOSITORY: &str = "https://github.com/klimPaskov/Agentic-HOI4-Modding";
pub const SOURCE_OWNER: &str = "klimPaskov";
pub const SOURCE_NAME: &str = "Agentic-HOI4-Modding";
pub const MANIFEST_PATH: &str = "hoi4-mod-setup.v2.manifest.json";
const MAX_SELECTED_FILES: usize = 20_000;
const MAX_SELECTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SOURCE_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRequest {
    pub mode: SourceMode,
    #[serde(default)]
    pub requested_ref: Option<String>,
    #[serde(default)]
    pub release: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceResolution {
    pub identity: SourceIdentity,
    pub manifest: RemoteManifest,
    pub manifest_bytes: Vec<u8>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSupport {
    pub component_id: String,
    pub selected: bool,
    pub state: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub dependents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedSourceFile {
    pub component_id: String,
    pub source_path: String,
    pub destination: String,
    pub ownership: Ownership,
    pub expected_sha256: Option<String>,
    pub expected_size: Option<u64>,
    pub executable: bool,
    pub platform: ManifestPlatform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadLedger {
    pub source: SourceIdentity,
    pub selected_files: Vec<DownloadedFile>,
    pub cache_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HttpSourceClient {
    client: Client,
    api_base: String,
    owner: String,
    name: String,
    cache_root: PathBuf,
}

impl HttpSourceClient {
    pub fn new() -> Result<Self, AppError> {
        Self::with_cache_root(cache_root()?)
    }

    fn with_cache_root(cache_root: PathBuf) -> Result<Self, AppError> {
        let client = Client::builder()
            .user_agent("HOI4-Mod-Setup/0.1")
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 {
                    attempt.stop()
                } else if approved_source_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|error| AppError::Source(format!("HTTP client: {error}")))?;
        Ok(Self {
            client,
            api_base: "https://api.github.com".to_string(),
            owner: SOURCE_OWNER.to_string(),
            name: SOURCE_NAME.to_string(),
            cache_root,
        })
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, AppError> {
        self.get_bytes_limited(url, 64 * 1024 * 1024)
    }

    fn get_bytes_limited(&self, url: &str, limit: usize) -> Result<Vec<u8>, AppError> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|error| AppError::Source(format!("invalid source URL: {error}")))?;
        if !approved_source_url(&parsed) {
            return Err(AppError::Source(format!(
                "source URL is not an approved HTTPS endpoint: {url}"
            )));
        }
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .map_err(|error| AppError::Source(format!("request failed: {error}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Source(format!("source returned HTTP {status}")));
        }
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(AppError::Source(
                "source response exceeds its bounded size limit".into(),
            ));
        }
        if !approved_source_url(response.url()) {
            return Err(AppError::Source(format!(
                "source redirect ended at an unapproved endpoint: {}",
                response.url()
            )));
        }
        let mut limited = response.take(limit as u64 + 1);
        let mut bytes = Vec::new();
        limited
            .read_to_end(&mut bytes)
            .map_err(|error| AppError::Source(format!("response body: {error}")))?;
        if bytes.len() > limit {
            return Err(AppError::Source(
                "source response exceeds its bounded size limit".into(),
            ));
        }
        Ok(bytes)
    }

    fn get_json(&self, url: &str) -> Result<Value, AppError> {
        let bytes = self.get_bytes(url)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| AppError::Source(format!("invalid source JSON: {error}")))
    }

    fn repo_url(&self, suffix: &str) -> String {
        let repository = format!("{}/repos/{}/{}", self.api_base, self.owner, self.name);
        let suffix = suffix.trim_start_matches('/');
        if suffix.is_empty() {
            repository
        } else {
            format!("{repository}/{suffix}")
        }
    }

    fn encode_path_segment(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                    (*byte as char).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect()
    }

    pub fn resolve_commit(
        &self,
        request: &SourceRequest,
    ) -> Result<(String, Option<String>, Option<String>), AppError> {
        match request.mode {
            SourceMode::Latest => {
                let repository = self.get_json(&self.repo_url(""))?;
                let branch = repository
                    .get("default_branch")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        AppError::Source("GitHub response omitted default_branch".into())
                    })?
                    .to_string();
                let branch_json = self.get_json(
                    &self.repo_url(&format!("branches/{}", encode_source_path(&branch))),
                )?;
                let commit = branch_json
                    .pointer("/commit/sha")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        AppError::Source("GitHub branch response omitted commit.sha".into())
                    })?;
                validate_commit(commit)?;
                self.verify_commit_object(commit)?;
                Ok((commit.to_ascii_lowercase(), Some(branch), None))
            }
            SourceMode::PinnedCommit => {
                let requested = request.requested_ref.as_deref().ok_or_else(|| {
                    AppError::Source("pinned commit requires a requested_ref".into())
                })?;
                validate_commit(requested)?;
                self.verify_commit_object(requested)?;
                Ok((requested.to_ascii_lowercase(), None, None))
            }
            SourceMode::PinnedRelease => {
                let release = request
                    .release
                    .as_deref()
                    .or(request.requested_ref.as_deref())
                    .ok_or_else(|| {
                        AppError::Source("pinned release requires a release tag".into())
                    })?;
                if release.trim().is_empty() {
                    return Err(AppError::Source("invalid release tag".into()));
                }
                let release_json = self.get_json(&self.repo_url(&format!(
                    "releases/tags/{}",
                    Self::encode_path_segment(release)
                )))?;
                let tag = release_json
                    .get("tag_name")
                    .and_then(Value::as_str)
                    .unwrap_or(release);
                let ref_json = self.get_json(
                    &self.repo_url(&format!("git/ref/tags/{}", Self::encode_path_segment(tag))),
                )?;
                let mut object = ref_json
                    .pointer("/object/sha")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        AppError::Source("release tag did not resolve to an object".into())
                    })?
                    .to_string();
                let mut object_type = ref_json
                    .pointer("/object/type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::Source("release tag omitted its object type".into()))?
                    .to_string();
                for _ in 0..4 {
                    validate_commit(&object)?;
                    match object_type.as_str() {
                        "commit" => {
                            self.verify_commit_object(&object)?;
                            return Ok((object.to_ascii_lowercase(), None, Some(tag.to_string())));
                        }
                        "tag" => {
                            let tag_json =
                                self.get_json(&self.repo_url(&format!("git/tags/{object}")))?;
                            object = tag_json
                                .pointer("/object/sha")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    AppError::Source(
                                        "annotated release tag omitted target SHA".into(),
                                    )
                                })?
                                .to_string();
                            object_type = tag_json
                                .pointer("/object/type")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string();
                        }
                        _ => {
                            return Err(AppError::Source(
                                "release tag did not resolve to a commit object".into(),
                            ))
                        }
                    }
                }
                Err(AppError::Source(
                    "release tag has too many annotated tag indirections".into(),
                ))
            }
        }
    }

    pub fn fetch_manifest(&self, revision: &str) -> Result<Vec<u8>, AppError> {
        validate_commit(revision)?;
        self.get_bytes(&format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            self.owner,
            self.name,
            revision,
            encode_source_path(MANIFEST_PATH)
        ))
    }

    pub fn fetch_tree(&self, revision: &str) -> Result<Vec<TreeEntry>, AppError> {
        validate_commit(revision)?;
        let value = self.get_json(&self.repo_url(&format!("git/trees/{revision}?recursive=1")))?;
        if value
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(AppError::Source(
                "GitHub tree response was truncated".into(),
            ));
        }
        let entries = value
            .get("tree")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::Source("GitHub tree response omitted tree".into()))?;
        if entries.len() > 100_000 {
            return Err(AppError::Source(
                "source tree exceeds the file-count limit".into(),
            ));
        }
        entries
            .iter()
            .map(|entry| {
                let path = entry
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::Source("tree entry omitted path".into()))?;
                let kind = entry
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("blob")
                    .to_string();
                let path = normalize_relative_path(path)
                    .map_err(|error| AppError::Source(error.to_string()))?;
                Ok(TreeEntry {
                    path,
                    kind,
                    size: entry.get("size").and_then(Value::as_u64),
                })
            })
            .collect()
    }

    pub fn fetch_file(&self, revision: &str, path: &str) -> Result<Vec<u8>, AppError> {
        self.fetch_file_with_limit(revision, path, 512 * 1024 * 1024)
    }

    fn fetch_file_with_limit(
        &self,
        revision: &str,
        path: &str,
        limit: usize,
    ) -> Result<Vec<u8>, AppError> {
        validate_commit(revision)?;
        let path =
            normalize_relative_path(path).map_err(|error| AppError::Source(error.to_string()))?;
        let bytes = self.get_bytes_limited(
            &format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                self.owner,
                self.name,
                revision,
                encode_source_path(&path)
            ),
            limit,
        )?;
        if bytes.len() > limit {
            return Err(AppError::Source(format!("file exceeds size limit: {path}")));
        }
        Ok(bytes)
    }

    /// Fetch a manifest-declared blob using an immutable, hash-addressed cache.
    /// Cache entries are accepted only after both the declared size and SHA-256
    /// match; a corrupted entry is ignored and replaced by a fresh HTTPS fetch.
    pub fn fetch_verified_file(
        &self,
        revision: &str,
        path: &str,
        expected_sha256: &str,
        expected_size: Option<u64>,
    ) -> Result<Vec<u8>, AppError> {
        self.fetch_verified_file_with_download(
            revision,
            path,
            expected_sha256,
            expected_size,
            |normalized, partial_path, fetch_limit| {
                self.download_verified_blob(
                    revision,
                    normalized,
                    partial_path,
                    fetch_limit,
                    expected_size,
                    expected_sha256,
                )
            },
        )
    }

    fn fetch_verified_file_with_download<F>(
        &self,
        revision: &str,
        path: &str,
        expected_sha256: &str,
        expected_size: Option<u64>,
        download: F,
    ) -> Result<Vec<u8>, AppError>
    where
        F: FnOnce(&str, &Path, usize) -> Result<Vec<u8>, AppError>,
    {
        validate_commit(revision)?;
        crate::source::validate_sha256(expected_sha256)?;
        let normalized =
            normalize_relative_path(path).map_err(|error| AppError::Source(error.to_string()))?;
        let cache_path = self
            .cache_root
            .join("blobs")
            .join(revision)
            .join(expected_sha256);
        let fetch_limit = match expected_size {
            Some(size) if size > MAX_SOURCE_FILE_BYTES => {
                return Err(AppError::Source(format!(
                    "declared source file exceeds size limit: {normalized}"
                )))
            }
            Some(size) => size
                .checked_add(1)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| AppError::Source("declared source file size overflows".into()))?,
            None => usize::try_from(MAX_SOURCE_FILE_BYTES)
                .expect("source file limit fits on supported desktop targets"),
        };
        if let Some(bytes) =
            read_verified_cache(&cache_path, expected_sha256, expected_size, fetch_limit)?
        {
            return Ok(bytes);
        }
        let partial_path = cache_path.with_extension("part");
        let bytes = download(&normalized, &partial_path, fetch_limit)?;
        if expected_size.is_some_and(|size| bytes.len() as u64 != size)
            || sha256_bytes(&bytes) != expected_sha256
        {
            let _ = fs::remove_file(&partial_path);
            return Err(AppError::Source(format!(
                "verified download evidence does not match {normalized}"
            )));
        }
        atomic_write(&cache_path, &bytes)
            .map_err(|error| AppError::Source(format!("write verified cache: {error}")))?;
        let _ = fs::remove_file(&partial_path);
        Ok(bytes)
    }

    fn download_verified_blob(
        &self,
        revision: &str,
        path: &str,
        partial_path: &Path,
        limit: usize,
        expected_size: Option<u64>,
        expected_sha256: &str,
    ) -> Result<Vec<u8>, AppError> {
        validate_commit(revision)?;
        if path_has_link_component(partial_path) {
            return Err(AppError::PathSecurity(
                "source partial-cache path contains a symlink or junction".into(),
            ));
        }
        let mut partial = if partial_path.is_file() {
            let metadata = fs::symlink_metadata(partial_path)?;
            if crate::security::is_link_metadata(&metadata) || !metadata.is_file() {
                return Err(AppError::PathSecurity(
                    "source partial-cache entry is not a regular file".into(),
                ));
            }
            let bytes = fs::read(partial_path)
                .map_err(|error| AppError::Source(format!("read partial source cache: {error}")))?;
            if bytes.len() > limit || expected_size.is_some_and(|size| bytes.len() as u64 > size) {
                let _ = fs::remove_file(partial_path);
                Vec::new()
            } else {
                bytes
            }
        } else {
            Vec::new()
        };

        if expected_size.is_some_and(|size| partial.len() as u64 == size)
            && expected_size.is_some_and(|size| size <= MAX_SOURCE_FILE_BYTES)
            && sha256_bytes(&partial) == expected_sha256
        {
            let file_size = partial.len() as u64;
            if file_size == expected_size.unwrap_or(file_size) {
                return Ok(partial);
            }
        }

        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            self.owner,
            self.name,
            revision,
            encode_source_path(path)
        );
        let parsed = reqwest::Url::parse(&url)
            .map_err(|error| AppError::Source(format!("invalid source URL: {error}")))?;
        if !approved_source_url(&parsed) {
            return Err(AppError::Source(
                "verified source blob URL is not an approved endpoint".into(),
            ));
        }
        let offset = partial.len();
        let mut request = self
            .client
            .get(&url)
            .header("Accept", "application/octet-stream");
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let mut response = request
            .send()
            .map_err(|error| AppError::Source(format!("source blob request failed: {error}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Source(format!(
                "source blob returned HTTP {status}"
            )));
        }
        if !approved_source_url(response.url()) {
            return Err(AppError::Source(format!(
                "source redirect ended at an unapproved endpoint: {}",
                response.url()
            )));
        }
        let resumed = offset > 0
            && status == reqwest::StatusCode::PARTIAL_CONTENT
            && response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| content_range_starts_at(value, offset));
        if offset > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT && !resumed {
            return Err(AppError::Source(
                "source server returned an invalid range response; partial cache retained".into(),
            ));
        }
        if !resumed {
            partial.clear();
        }
        let remaining = limit.saturating_sub(partial.len());
        if response
            .content_length()
            .is_some_and(|size| size > remaining as u64)
        {
            return Err(AppError::Source(
                "source blob response exceeds its bounded size limit".into(),
            ));
        }
        if resumed && status != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(AppError::Source(
                "source server did not return a valid range response".into(),
            ));
        }
        let mut file = open_partial_cache(partial_path, !resumed)?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = match response.read(&mut buffer) {
                Ok(read) => read,
                Err(error) => {
                    let _ = file.sync_all();
                    return Err(AppError::Source(format!(
                        "source blob interrupted; partial cache retained: {error}"
                    )));
                }
            };
            if read == 0 {
                break;
            }
            if partial.len().saturating_add(read) > limit {
                let _ = file.sync_all();
                return Err(AppError::Source(
                    "source blob response exceeds its bounded size limit".into(),
                ));
            }
            file.write_all(&buffer[..read]).map_err(|error| {
                AppError::Source(format!("write partial source cache: {error}"))
            })?;
            partial.extend_from_slice(&buffer[..read]);
        }
        file.sync_all()
            .map_err(|error| AppError::Source(format!("sync partial source cache: {error}")))?;
        if expected_size.is_some_and(|size| partial.len() as u64 != size) {
            return Err(AppError::Source(
                "source blob ended before its declared size; partial cache retained".into(),
            ));
        }
        Ok(partial)
    }

    fn verify_commit_object(&self, revision: &str) -> Result<(), AppError> {
        validate_commit(revision)?;
        let object = self.get_json(&self.repo_url(&format!("git/commits/{revision}")))?;
        let sha = object
            .get("sha")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Source("GitHub commit response omitted sha".into()))?;
        if !sha.eq_ignore_ascii_case(revision) {
            return Err(AppError::Source(
                "GitHub commit object did not match the requested revision".into(),
            ));
        }
        Ok(())
    }
}

fn approved_source_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && matches!(
            url.host_str(),
            Some("api.github.com") | Some("raw.githubusercontent.com")
        )
}

fn content_range_starts_at(value: &str, offset: usize) -> bool {
    let prefix = format!("bytes {offset}-");
    value.starts_with(&prefix)
}

fn read_verified_cache(
    path: &Path,
    expected_sha256: &str,
    expected_size: Option<u64>,
    limit: usize,
) -> Result<Option<Vec<u8>>, AppError> {
    read_verified_cache_with_hook(path, expected_sha256, expected_size, limit, || Ok(()))
}

fn read_verified_cache_with_hook<F>(
    path: &Path,
    expected_sha256: &str,
    expected_size: Option<u64>,
    limit: usize,
    after_open: F,
) -> Result<Option<Vec<u8>>, AppError>
where
    F: FnOnce() -> Result<(), AppError>,
{
    if path_has_link_component(path) {
        return Err(AppError::PathSecurity(
            "source cache path contains a symlink or junction".into(),
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(source_open_no_follow_flag());
    #[cfg(windows)]
    options.custom_flags(0x0020_0000);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::Source(format!(
                "open verified source cache: {error}"
            )))
        }
    };
    let metadata = file
        .metadata()
        .map_err(|error| AppError::Source(format!("inspect verified source cache: {error}")))?;
    if !metadata.is_file() || crate::security::is_link_metadata(&metadata) {
        return Err(AppError::PathSecurity(
            "source cache entry is not a regular file".into(),
        ));
    }
    if metadata.len() > limit as u64 || expected_size.is_some_and(|size| metadata.len() != size) {
        return Ok(None);
    }

    after_open()?;

    let read_limit = limit
        .checked_add(1)
        .ok_or_else(|| AppError::Source("verified source cache size overflows".into()))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(read_limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::Source(format!("read verified source cache: {error}")))?;
    if bytes.len() > limit
        || expected_size.is_some_and(|size| bytes.len() as u64 != size)
        || sha256_bytes(&bytes) != expected_sha256
    {
        return Ok(None);
    }
    Ok(Some(bytes))
}

fn open_partial_cache(path: &Path, truncate: bool) -> Result<File, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Source("partial source cache has no parent".into()))?;
    fs::create_dir_all(parent)
        .map_err(|error| AppError::Source(format!("create partial source cache: {error}")))?;
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    if truncate {
        options.truncate(true);
    } else {
        options.append(true);
    }
    #[cfg(unix)]
    options.custom_flags(source_open_no_follow_flag());
    #[cfg(windows)]
    options.custom_flags(0x0020_0000);
    let file = options
        .open(path)
        .map_err(|error| AppError::Source(format!("open partial source cache: {error}")))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || crate::security::is_link_metadata(&metadata) {
        return Err(AppError::PathSecurity(
            "partial source cache is not a regular file".into(),
        ));
    }
    Ok(file)
}

#[cfg(target_os = "macos")]
fn source_open_no_follow_flag() -> i32 {
    0x0100
}

#[cfg(all(unix, not(target_os = "macos")))]
fn source_open_no_follow_flag() -> i32 {
    0x20000
}

fn encode_source_path(path: &str) -> String {
    path.split('/')
        .map(HttpSourceClient::encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

pub fn resolve_source(
    client: &HttpSourceClient,
    request: &SourceRequest,
) -> Result<SourceResolution, AppError> {
    let (revision, branch, canonical_release) = client.resolve_commit(request)?;
    let remote_manifest_bytes = client.fetch_manifest(&revision)?;
    let (manifest_bytes, manifest, manifest_origin) =
        select_manifest_for_revision(&remote_manifest_bytes, &revision)?;
    match request.mode {
        SourceMode::Latest
            if !manifest.update_policy.latest.resolve_default_branch
                || !manifest.update_policy.latest.record_commit =>
        {
            return Err(AppError::Source(
                "latest source mode requires default-branch resolution and recorded commit evidence"
                    .into(),
            ));
        }
        SourceMode::PinnedCommit if !manifest.update_policy.pinned.allow_commit => {
            return Err(AppError::Source(
                "the remote manifest does not allow pinned commit installs".into(),
            ));
        }
        SourceMode::PinnedRelease if !manifest.update_policy.pinned.allow_release => {
            return Err(AppError::Source(
                "the remote manifest does not allow pinned release installs".into(),
            ));
        }
        _ => {}
    }
    let manifest_sha256 = sha256_bytes(&manifest_bytes);
    let mode = request.mode;
    let identity = SourceIdentity {
        repository: format!("{SOURCE_OWNER}/{SOURCE_NAME}"),
        mode,
        resolved_revision: revision,
        requested_ref: request.requested_ref.clone(),
        release: canonical_release.or_else(|| request.release.clone()),
        manifest_sha256,
        manifest_origin: manifest_origin.into(),
    };
    Ok(SourceResolution {
        identity,
        manifest,
        manifest_bytes,
        branch,
    })
}

fn select_manifest_for_revision(
    remote_manifest_bytes: &[u8],
    revision: &str,
) -> Result<(Vec<u8>, RemoteManifest, &'static str), AppError> {
    // A manifest is committed after the source snapshot it describes. Requiring
    // the manifest to contain the hash of the commit that contains the manifest
    // would be self-referential: changing the field changes the commit hash.
    // `generated_for_revision` remains required provenance, while the resolved
    // revision below binds the manifest bytes and every selected source blob.
    let remote_manifest = parse_manifest(remote_manifest_bytes, Some(revision))?;
    Ok((remote_manifest_bytes.to_vec(), remote_manifest, "remote"))
}

pub fn parse_manifest(bytes: &[u8], revision: Option<&str>) -> Result<RemoteManifest, AppError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| AppError::Source(format!("manifest JSON is invalid: {error}")))?;
    validate_manifest_schema(&value)?;
    let manifest: RemoteManifest = serde_json::from_value(value)
        .map_err(|error| AppError::Source(format!("manifest model is invalid: {error}")))?;
    validate_manifest(&manifest, revision)?;
    Ok(manifest)
}

fn validate_manifest_schema(value: &Value) -> Result<(), AppError> {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    let schema = SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../docs/schemas/remote-manifest.schema.json"
        ))
        .expect("checked-in remote manifest schema must be valid JSON")
    });
    let validator = jsonschema::draft202012::new(schema).map_err(|error| {
        AppError::Source(format!(
            "checked-in remote manifest schema cannot be compiled: {error}"
        ))
    })?;
    validator.validate(value).map_err(|error| {
        AppError::Source(format!(
            "manifest does not satisfy the authoritative JSON Schema at {}: {}",
            error.instance_path(),
            error
        ))
    })
}

pub fn wiki_install_metadata(manifest: &RemoteManifest) -> WikiInstallMetadata {
    WikiInstallMetadata {
        snapshot_marker: manifest.wiki.snapshot_marker.clone(),
        required_media_policy: manifest.wiki.required_media_policy.clone(),
        source_status: manifest.wiki.provenance.source_status.clone(),
        license_status: manifest.wiki.provenance.license_status.clone(),
        repository_license_status: manifest
            .repository
            .license_evidence
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        notes: manifest.wiki.provenance.notes.clone(),
    }
}

fn require_canonical_manifest_path(raw: &str, label: &str) -> Result<String, AppError> {
    let normalized = normalize_relative_path(raw)
        .map_err(|error| AppError::Source(format!("invalid {label} path: {error}")))?;
    let canonical_raw = raw.replace('\\', "/").trim_end_matches('/').to_owned();
    if canonical_raw != normalized {
        return Err(AppError::Source(format!(
            "{label} path is not in canonical slash-separated form: {raw}"
        )));
    }
    Ok(normalized)
}

pub fn validate_manifest(
    manifest: &RemoteManifest,
    revision: Option<&str>,
) -> Result<(), AppError> {
    let version_pattern =
        Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$").expect("static manifest version regex");
    if !version_pattern.is_match(&manifest.schema_version) {
        return Err(AppError::Source(
            "manifest schema_version must be a full semantic version".into(),
        ));
    }
    let major = manifest
        .schema_version
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| AppError::Source("manifest schema_version is invalid".into()))?;
    if major != SUPPORTED_MANIFEST_MAJOR {
        return Err(AppError::Source(format!(
            "unsupported manifest major {major}; supported major is {SUPPORTED_MANIFEST_MAJOR}"
        )));
    }
    if manifest.manifest_id.trim().is_empty()
        || !Regex::new(r"^[a-z0-9][a-z0-9._-]*$")
            .expect("static manifest ID regex")
            .is_match(&manifest.manifest_id)
    {
        return Err(AppError::Source("manifest_id is invalid".into()));
    }
    if manifest.repository.default_branch.trim().is_empty() {
        return Err(AppError::Source(
            "manifest repository default_branch is empty".into(),
        ));
    }
    if manifest.repository.provider != "github"
        || manifest.repository.owner != SOURCE_OWNER
        || manifest.repository.name != SOURCE_NAME
    {
        return Err(AppError::Source(
            "manifest repository does not match the approved source".into(),
        ));
    }
    if !manifest.update_policy.latest.resolve_default_branch
        || !manifest.update_policy.latest.record_commit
    {
        return Err(AppError::Source(
            "manifest latest policy must resolve the default branch and record the exact commit"
                .into(),
        ));
    }
    if !matches!(
        manifest.repository.license_evidence.as_deref(),
        None | Some("verified" | "declared_unverified" | "not_found" | "unknown")
    ) {
        return Err(AppError::Source(
            "manifest repository license evidence has an unsupported status".into(),
        ));
    }
    if let Some(revision) = revision {
        validate_commit(revision)?;
        match manifest.generated_for_revision.as_deref() {
            Some(declared) => {
                validate_commit(declared)?;
            }
            None => {
                return Err(AppError::Source(
                    "manifest must declare the immutable revision used to generate its evidence"
                        .into(),
                ));
            }
        }
    }
    let id_pattern = Regex::new(r"^[a-z0-9][a-z0-9._-]*$").expect("static component id regex");
    let mut components = HashMap::new();
    for component in &manifest.components {
        if component.id.len() > 128 || !id_pattern.is_match(&component.id) {
            return Err(AppError::Source(format!(
                "invalid component id: {}",
                component.id
            )));
        }
        if components.insert(component.id.clone(), component).is_some() {
            return Err(AppError::Source(format!(
                "duplicate component id: {}",
                component.id
            )));
        }
        if component.platforms.is_empty() {
            return Err(AppError::Source(format!(
                "component {} has no platform declarations",
                component.id
            )));
        }
        if component.destination.outside_project
            || component.destination.ownership == Ownership::External
        {
            return Err(AppError::Source(format!(
                "component {} declares an unsupported external destination",
                component.id
            )));
        }
        require_canonical_manifest_path(&component.source.path, "component source")?;
        require_canonical_manifest_path(&component.destination.path, "component destination")?;
        if !matches!(component.source.kind, SourceKind::Generated)
            && component.expected_files.is_empty()
        {
            return Err(AppError::Source(format!(
                "component {} has no complete file evidence",
                component.id
            )));
        }
        let mut evidence_paths = HashSet::new();
        for expected in &component.expected_files {
            require_canonical_manifest_path(&expected.path, "expected file")?;
            let evidence_key = canonical_relative_key(&expected.path)
                .map_err(|error| AppError::Source(error.to_string()))?;
            if !evidence_paths.insert(evidence_key) {
                return Err(AppError::Source(format!(
                    "component {} has duplicate file evidence: {}",
                    component.id, expected.path
                )));
            }
            let hash = expected.sha256.as_deref().ok_or_else(|| {
                AppError::Source(format!(
                    "component {} has a file without SHA-256 evidence",
                    component.id
                ))
            })?;
            validate_sha256(hash)?;
            if expected.size.is_none() {
                return Err(AppError::Source(format!(
                    "component {} has a file without size evidence",
                    component.id
                )));
            }
        }
        for environment in &component.environment {
            crate::security::validate_env_name(&environment.name)?;
            if environment.secret && environment.storage.as_deref() != Some("os_credential_vault") {
                return Err(AppError::Source(format!(
                    "secret environment {} must use the OS credential vault",
                    environment.name
                )));
            }
            if environment.secret && environment.name != crate::credentials::MESHY_ENVIRONMENT_NAME
            {
                return Err(AppError::Source(format!(
                    "unsupported secret environment {}; only {} is supported",
                    environment.name,
                    crate::credentials::MESHY_ENVIRONMENT_NAME
                )));
            }
        }
        for tool in &component.required_tools {
            if tool.version_policy.as_deref().is_some_and(|policy| {
                !matches!(
                    policy,
                    "manifest"
                        | "repository_script"
                        | "latest_at_execution"
                        | "user_managed"
                        | "any"
                )
            }) {
                return Err(AppError::Source(format!(
                    "component {} declares an unsupported tool version policy",
                    component.id
                )));
            }
        }
        for rule in &component.validation {
            if !matches!(rule.severity.as_str(), "block" | "warn" | "info")
                || !matches!(
                    rule.kind.as_str(),
                    "exists"
                        | "sha256"
                        | "json_schema"
                        | "toml_parse"
                        | "yaml_bom"
                        | "command"
                        | "directory_coverage"
                        | "custom"
                )
            {
                return Err(AppError::Source(format!(
                    "component {} declares an unsupported validation rule",
                    component.id
                )));
            }
            if rule.kind == "command" {
                for identity in ["executable", "interpreter", "runtime"] {
                    let hash_key = format!("{identity}_sha256");
                    let size_key = format!("{identity}_size");
                    let hash_value = rule.parameters.get(&hash_key);
                    let size_value = rule.parameters.get(&size_key);
                    if hash_value.is_none() != size_value.is_none() {
                        return Err(AppError::Source(format!(
                            "component {} command identity must declare both {hash_key} and {size_key}",
                            component.id
                        )));
                    }
                    if let Some(value) = hash_value {
                        let hash = value.as_str().ok_or_else(|| {
                            AppError::Source(format!(
                                "component {} command identity SHA-256 must be a string",
                                component.id
                            ))
                        })?;
                        validate_sha256(hash)?;
                    }
                    if let Some(value) = size_value {
                        if value.as_u64().is_none() {
                            return Err(AppError::Source(format!(
                                "component {} command identity size must be a non-negative integer",
                                component.id
                            )));
                        }
                    }
                }
            }
        }
        if component.id == "workflow.3d" {
            let actions = component
                .validation
                .iter()
                .filter(|rule| rule.kind == "command" && rule.id == "3d.bootstrap")
                .collect::<Vec<_>>();
            if actions.len() != 1 {
                return Err(AppError::Source(
                    "workflow.3d must declare exactly one reviewed bootstrap action".into(),
                ));
            }
            let action = actions[0];
            let arguments = action.parameters.get("arguments").and_then(Value::as_array);
            let expected_writes = action
                .parameters
                .get("expected_writes")
                .and_then(Value::as_array);
            let declared_text = |key: &str| {
                action
                    .parameters
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            };
            if action.target.as_deref() != Some(".tools/3d_pipeline/bootstrap_3d_workflow.py")
                || arguments.is_none_or(Vec::is_empty)
                || expected_writes.is_none_or(Vec::is_empty)
                || !declared_text("network_access")
                || action.parameters.get("privilege").and_then(Value::as_str)
                    != Some("current_user")
                || !declared_text("rollback_boundary")
            {
                return Err(AppError::Source(
                    "workflow.3d bootstrap action evidence is incomplete".into(),
                ));
            }
        }
        if component.id == "mcp.hoi4_agent_tools.bootstrap" {
            let actions = component
                .validation
                .iter()
                .filter(|rule| rule.kind == "command" && rule.id == "mcp.bootstrap")
                .collect::<Vec<_>>();
            if actions.len() != 1 {
                return Err(AppError::Source(
                    "the MCP bootstrap component must declare exactly one reviewed action".into(),
                ));
            }
            let action = actions[0];
            let arguments = action.parameters.get("arguments").and_then(Value::as_array);
            let expected_writes = action
                .parameters
                .get("expected_writes")
                .and_then(Value::as_array);
            let declared_text = |key: &str| {
                action
                    .parameters
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            };
            if component.source.path != ".tools/mcp/bootstrap_hoi4_agent_tools.py"
                || action.target.as_deref() != Some(".tools/mcp/bootstrap_hoi4_agent_tools.py")
                || arguments
                    .is_none_or(|values| values.as_slice() != [Value::String("--quiet".into())])
                || expected_writes.is_none_or(Vec::is_empty)
                || !declared_text("network_access")
                || action.parameters.get("privilege").and_then(Value::as_str)
                    != Some("current_user")
                || !declared_text("rollback_boundary")
            {
                return Err(AppError::Source(
                    "the MCP bootstrap reviewed action evidence is incomplete".into(),
                ));
            }
        }
        for platform in &component.platforms {
            if matches!(platform, ManifestPlatform::All)
                && (component
                    .required_tools
                    .iter()
                    .any(|tool| !tool.commands.is_empty())
                    || component
                        .validation
                        .iter()
                        .any(|rule| rule.kind == "command"))
            {
                return Err(AppError::Source(format!(
                    "command-bearing component {} must declare explicit platform routes",
                    component.id
                )));
            }
        }
    }
    for component in &manifest.components {
        for dependency in &component.dependencies {
            if !components.contains_key(dependency) {
                return Err(AppError::Source(format!(
                    "component {} depends on unknown component {dependency}",
                    component.id
                )));
            }
        }
    }
    crate::mcp::manifest_target(manifest)?;
    let component_ids: HashSet<&str> = components.keys().map(|id| id.as_str()).collect();
    for profile in &manifest.profiles {
        if profile.id.trim().is_empty() || profile.components.is_empty() {
            return Err(AppError::Source(format!("profile {} is empty", profile.id)));
        }
        for component in &profile.components {
            if !component_ids.contains(component.as_str()) {
                return Err(AppError::Source(format!(
                    "profile {} references unknown component {component}",
                    profile.id
                )));
            }
        }
    }
    if manifest
        .profiles
        .iter()
        .filter(|profile| profile.default)
        .count()
        != 1
    {
        return Err(AppError::Source(
            "manifest must declare exactly one default profile".into(),
        ));
    }
    validate_dependency_cycles(&components)?;
    validate_manifest_destinations(&manifest.components)?;
    let wiki_component = components
        .get(&manifest.wiki.component_id)
        .ok_or_else(|| AppError::Source("wiki component_id is not declared".into()))?;
    if wiki_component.category != ComponentCategory::Wiki {
        return Err(AppError::Source(
            "wiki component_id must identify a wiki component".into(),
        ));
    }
    if !matches!(
        manifest.wiki.required_media_policy.as_str(),
        "all_declared" | "referenced_only" | "none"
    ) {
        return Err(AppError::Source("unsupported wiki media policy".into()));
    }
    if !matches!(
        manifest.wiki.provenance.source_status.as_str(),
        "verified" | "repository_only" | "unknown"
    ) || !matches!(
        manifest.wiki.provenance.license_status.as_str(),
        "verified" | "not_found" | "unknown"
    ) {
        return Err(AppError::Source(
            "wiki provenance contains an unsupported status".into(),
        ));
    }
    let mut required_pages = HashSet::new();
    for page in &manifest.wiki.required_pages {
        let page = require_canonical_manifest_path(page, "required wiki page")
            .map_err(|error| AppError::Source(format!("invalid required wiki page: {error}")))?;
        if !required_pages.insert(page.clone()) {
            return Err(AppError::Source(format!(
                "duplicate required wiki page: {page}"
            )));
        }
        let expected_path = format!("paradox_wiki/{page}");
        if !wiki_component
            .expected_files
            .iter()
            .any(|file| file.path == expected_path)
        {
            return Err(AppError::Source(format!(
                "required wiki page lacks manifest evidence: {page}"
            )));
        }
    }
    if manifest.wiki.destination != "paradox_wiki/" {
        return Err(AppError::Source(
            "wiki destination must be paradox_wiki/".into(),
        ));
    }
    if manifest.wiki.required_pages.is_empty() {
        return Err(AppError::Source("wiki must declare required pages".into()));
    }
    if manifest.update_policy.rollback_retention == 0 {
        return Err(AppError::Source(
            "rollback retention must be positive".into(),
        ));
    }
    if manifest
        .signing
        .as_ref()
        .is_some_and(|policy| policy.required)
    {
        return Err(AppError::Source(
            "signed manifests are declared but signature verification is not supported by this build"
                .into(),
        ));
    }
    Ok(())
}

pub fn validate_commit(value: &str) -> Result<(), AppError> {
    let valid = value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(AppError::Source(format!(
            "expected a full immutable commit SHA: {value}"
        )))
    }
}

pub fn validate_sha256(value: &str) -> Result<(), AppError> {
    if value.len() == 64
        && value.chars().all(|character| character.is_ascii_hexdigit())
        && value == value.to_ascii_lowercase()
    {
        Ok(())
    } else {
        Err(AppError::Source(format!(
            "invalid lowercase SHA-256: {value}"
        )))
    }
}

pub fn validate_dependency_cycles(
    components: &HashMap<String, &ComponentDefinition>,
) -> Result<(), AppError> {
    fn visit(
        id: &str,
        components: &HashMap<String, &ComponentDefinition>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), AppError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_string()) {
            return Err(AppError::Source(format!("dependency cycle includes {id}")));
        }
        let component = components
            .get(id)
            .ok_or_else(|| AppError::Source(format!("unknown component {id}")))?;
        for dependency in &component.dependencies {
            visit(dependency, components, visiting, visited)?;
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        Ok(())
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for id in components.keys() {
        visit(id, components, &mut visiting, &mut visited)?;
    }
    Ok(())
}

pub fn expand_components(
    manifest: &RemoteManifest,
    requested: &[String],
) -> Result<Vec<String>, AppError> {
    let by_id: HashMap<&str, &ComponentDefinition> = manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let mut selected = BTreeSet::new();
    fn add(
        id: &str,
        by_id: &HashMap<&str, &ComponentDefinition>,
        selected: &mut BTreeSet<String>,
    ) -> Result<(), AppError> {
        let component = by_id
            .get(id)
            .ok_or_else(|| AppError::Source(format!("unknown selected component: {id}")))?;
        if selected.insert(id.to_string()) {
            for dependency in &component.dependencies {
                add(dependency, by_id, selected)?;
            }
        }
        Ok(())
    }
    for id in requested {
        add(id, &by_id, &mut selected)?;
    }
    let mut ordered = Vec::new();
    fn append_topological(
        id: &str,
        by_id: &HashMap<&str, &ComponentDefinition>,
        selected: &BTreeSet<String>,
        appended: &mut HashSet<String>,
        ordered: &mut Vec<String>,
    ) {
        if !selected.contains(id) || !appended.insert(id.to_string()) {
            return;
        }
        for dependency in &by_id[id].dependencies {
            append_topological(dependency, by_id, selected, appended, ordered);
        }
        ordered.push(id.to_string());
    }
    let mut appended = HashSet::new();
    for id in requested {
        append_topological(id, &by_id, &selected, &mut appended, &mut ordered);
    }
    Ok(ordered)
}

/// Return the reverse dependency graph for the verified manifest. The map is
/// used to explain removal/update impact; it never grants a dependent the
/// right to bypass the forward dependency closure.
pub fn reverse_dependencies(manifest: &RemoteManifest) -> HashMap<String, Vec<String>> {
    let mut reverse = manifest
        .components
        .iter()
        .map(|component| (component.id.clone(), Vec::new()))
        .collect::<HashMap<_, _>>();
    for component in &manifest.components {
        for dependency in &component.dependencies {
            reverse
                .entry(dependency.clone())
                .or_default()
                .push(component.id.clone());
        }
    }
    for dependents in reverse.values_mut() {
        dependents.sort();
        dependents.dedup();
    }
    reverse
}

pub fn resolve_platform_support(
    manifest: &RemoteManifest,
    selected: &[String],
    platform: Platform,
) -> Result<Vec<ComponentSupport>, AppError> {
    let by_id: HashMap<&str, &ComponentDefinition> = manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let reverse = reverse_dependencies(manifest);
    selected
        .iter()
        .map(|id| {
            let component = by_id
                .get(id.as_str())
                .ok_or_else(|| AppError::Source(format!("unknown selected component: {id}")))?;
            let dependents = reverse.get(id).cloned().unwrap_or_default();
            if component
                .platforms
                .iter()
                .any(|value| value.supports(platform))
            {
                Ok(ComponentSupport {
                    component_id: id.clone(),
                    selected: true,
                    state: "supported".into(),
                    reason: None,
                    dependents,
                })
            } else if component.optional {
                Ok(ComponentSupport {
                    component_id: id.clone(),
                    selected: true,
                    state: "unsupported_platform".into(),
                    reason: Some(format!(
                        "{} has no verified {} route",
                        component.display_name,
                        platform.manifest_name()
                    )),
                    dependents,
                })
            } else {
                Ok(ComponentSupport {
                    component_id: id.clone(),
                    selected: true,
                    state: "blocked".into(),
                    reason: Some(format!(
                        "required component {} is unsupported on {}",
                        component.id,
                        platform.manifest_name()
                    )),
                    dependents,
                })
            }
        })
        .collect()
}

pub fn select_component_files(
    manifest: &RemoteManifest,
    component_ids: &[String],
    tree: &[TreeEntry],
) -> Result<Vec<SelectedSourceFile>, AppError> {
    let by_id: HashMap<&str, &ComponentDefinition> = manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let mut destinations = HashSet::new();
    let mut selected = Vec::new();
    for component_id in component_ids {
        let component = by_id
            .get(component_id.as_str())
            .ok_or_else(|| AppError::Source(format!("unknown component: {component_id}")))?;
        match component.source.kind {
            SourceKind::Generated => continue,
            SourceKind::File => {
                let source_path = normalize_relative_path(&component.source.path)
                    .map_err(|error| AppError::Source(error.to_string()))?;
                if is_gitkeep_path(&source_path) {
                    continue;
                }
                add_source_file(component, &source_path, &mut destinations, &mut selected)?;
            }
            SourceKind::Tree => {
                let prefix = normalize_relative_path(&component.source.path)
                    .map_err(|error| AppError::Source(error.to_string()))?;
                let mut matched_evidence = HashSet::new();
                for entry in tree.iter().filter(|entry| entry.kind == "blob") {
                    if entry.path != prefix && !entry.path.starts_with(&format!("{prefix}/")) {
                        continue;
                    }
                    let relative = entry
                        .path
                        .strip_prefix(&prefix)
                        .unwrap_or("")
                        .trim_start_matches('/');
                    if !component.source.include.is_empty()
                        && !component
                            .source
                            .include
                            .iter()
                            .any(|pattern| glob_matches(pattern, relative))
                    {
                        continue;
                    }
                    if component
                        .source
                        .exclude
                        .iter()
                        .any(|pattern| glob_matches(pattern, relative))
                    {
                        continue;
                    }
                    if is_gitkeep_path(&entry.path) {
                        continue;
                    }
                    if component
                        .expected_files
                        .iter()
                        .any(|file| file.path == entry.path)
                    {
                        matched_evidence.insert(entry.path.clone());
                    }
                    add_source_file(component, &entry.path, &mut destinations, &mut selected)?;
                }
                for expected in &component.expected_files {
                    let expected_path = normalize_relative_path(&expected.path)
                        .map_err(|error| AppError::Source(error.to_string()))?;
                    if is_gitkeep_path(&expected_path) {
                        continue;
                    }
                    if !matched_evidence.contains(&expected_path) {
                        return Err(AppError::Source(format!(
                            "manifest evidence is not present in the selected source tree: {}",
                            expected.path
                        )));
                    }
                }
            }
        }
    }
    if selected.len() > MAX_SELECTED_FILES {
        return Err(AppError::Source(
            "selected source file count exceeds the aggregate limit".into(),
        ));
    }
    let selected_bytes = selected
        .iter()
        .filter_map(|file| file.expected_size)
        .try_fold(0_u64, |total, size| total.checked_add(size))
        .ok_or_else(|| AppError::Source("selected source size overflows".into()))?;
    if selected_bytes > MAX_SELECTED_BYTES {
        return Err(AppError::Source(
            "selected source size exceeds the aggregate limit".into(),
        ));
    }
    Ok(selected)
}

fn is_gitkeep_path(path: &str) -> bool {
    path.rsplit('/').next() == Some(".gitkeep")
}

fn add_source_file(
    component: &ComponentDefinition,
    source_path: &str,
    destinations: &mut HashSet<String>,
    selected: &mut Vec<SelectedSourceFile>,
) -> Result<(), AppError> {
    let destination = match component.source.kind {
        SourceKind::File | SourceKind::Generated => component.destination.path.clone(),
        SourceKind::Tree => {
            let prefix = normalize_relative_path(&component.source.path)
                .map_err(|error| AppError::Source(error.to_string()))?;
            let relative = source_path
                .strip_prefix(&prefix)
                .unwrap_or("")
                .trim_start_matches('/');
            let root = component.destination.path.trim_end_matches('/');
            format!("{root}/{relative}")
        }
    };
    let destination = normalize_relative_path(&destination)
        .map_err(|error| AppError::Source(error.to_string()))?;
    let destination_key = canonical_relative_key(&destination)
        .map_err(|error| AppError::Source(error.to_string()))?;
    if !destinations.insert(destination_key) {
        return Err(AppError::Source(format!(
            "duplicate selected destination: {destination}"
        )));
    }
    let expected = component
        .expected_files
        .iter()
        .find(|file| file.path == source_path || file.path == destination);
    if !matches!(component.source.kind, SourceKind::Generated)
        && expected
            .map(|file| file.sha256.is_none() || file.size.is_none())
            .unwrap_or(true)
    {
        return Err(AppError::Source(format!(
            "selected source file lacks manifest evidence: {source_path}"
        )));
    }
    selected.push(SelectedSourceFile {
        component_id: component.id.clone(),
        source_path: source_path.to_string(),
        destination,
        ownership: component.destination.ownership,
        expected_sha256: expected.and_then(|file| file.sha256.clone()),
        expected_size: expected.and_then(|file| file.size),
        executable: false,
        platform: component
            .platforms
            .first()
            .copied()
            .unwrap_or(ManifestPlatform::All),
    });
    Ok(())
}

pub fn verify_download(
    selection: &SelectedSourceFile,
    bytes: &[u8],
    revision: &str,
) -> Result<DownloadedFile, AppError> {
    validate_commit(revision)?;
    let expected_sha256 = selection.expected_sha256.as_deref().ok_or_else(|| {
        AppError::Source(format!(
            "download has no manifest SHA-256 evidence: {}",
            selection.source_path
        ))
    })?;
    validate_sha256(expected_sha256)?;
    let hash = sha256_bytes(bytes);
    if expected_sha256 != hash.as_str() {
        return Err(AppError::Source(format!(
            "checksum mismatch for {}: expected {}, received {}",
            selection.source_path, expected_sha256, hash
        )));
    }
    if let Some(expected_size) = selection.expected_size {
        if expected_size != bytes.len() as u64 {
            return Err(AppError::Source(format!(
                "size mismatch for {}",
                selection.source_path
            )));
        }
    }
    Ok(DownloadedFile {
        operation_id: String::new(),
        source_path: selection.source_path.clone(),
        destination: selection.destination.clone(),
        source_revision: revision.to_string(),
        manifest_sha256: String::new(),
        sha256: hash,
        size: bytes.len() as u64,
        component_id: selection.component_id.clone(),
        ownership: selection.ownership,
        platform: selection.platform,
        executable: selection.executable,
    })
}

pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let mut expression = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' if index + 1 < chars.len() && chars[index + 1] == '*' => {
                expression.push_str(".*");
                index += 2;
            }
            '*' => {
                expression.push_str("[^/]*");
                index += 1;
            }
            '?' => {
                expression.push_str("[^/]");
                index += 1;
            }
            character => {
                expression.push_str(&regex::escape(&character.to_string()));
                index += 1;
            }
        }
    }
    expression.push('$');
    Regex::new(&expression)
        .map(|regex| regex.is_match(path))
        .unwrap_or(false)
}

pub fn cache_key(source: &SourceIdentity, source_path: &str, sha256: &str) -> String {
    format!(
        "{}/{}/{}",
        source.resolved_revision,
        source_path.replace('/', "_"),
        sha256
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn component(id: &str, dependencies: &[&str]) -> ComponentDefinition {
        ComponentDefinition {
            id: id.to_string(),
            display_name: id.to_string(),
            description: None,
            category: ComponentCategory::Core,
            optional: false,
            platforms: vec![ManifestPlatform::All],
            source: SourceDefinition {
                kind: SourceKind::File,
                path: format!("{id}.txt"),
                include: vec![],
                exclude: vec![],
                template_engine: None,
            },
            destination: DestinationDefinition {
                path: format!("{id}.txt"),
                ownership: Ownership::Managed,
                outside_project: false,
            },
            dependencies: dependencies.iter().map(|value| value.to_string()).collect(),
            required_tools: vec![],
            environment: vec![],
            expected_files: vec![],
            conflicts: vec![],
            conflict_policy: ConflictPolicy::ReplaceIfUnmodified,
            validation: vec![],
            update: UpdatePolicy {
                strategy: UpdateStrategy::ReplaceIfUnmodified,
                remove_obsolete: false,
                preserve_local_additions: true,
            },
            capabilities: vec![],
            notes: vec![],
        }
    }

    #[test]
    fn checked_in_manifest_matches_the_supported_source_contract() {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json");
        let inventory: Value = serde_json::from_str(include_str!(
            "../../docs/source-audit/live_repository_inventory.json"
        ))
        .unwrap();
        let source_snapshot = inventory["manifest_generated_for_revision"]
            .as_str()
            .unwrap();
        let manifest = parse_manifest(bytes, Some(source_snapshot)).unwrap();
        assert_eq!(manifest.repository.owner, SOURCE_OWNER);
        assert_eq!(
            manifest.generated_for_revision.as_deref(),
            Some(source_snapshot)
        );
        assert!(manifest.profiles.iter().any(|profile| profile.default));
        assert_eq!(manifest.wiki.destination, "paradox_wiki/");
        assert_eq!(manifest.components.len(), 26);
        let core_profile = manifest
            .profiles
            .iter()
            .find(|profile| profile.default)
            .expect("checked-in manifest must have a default profile");
        assert!(core_profile
            .components
            .iter()
            .any(|component| component == "docs.mcp_integration"));
        assert!(manifest
            .components
            .iter()
            .any(|component| component.id == "workflow.super_events"));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.super_events.runtime.interface" }));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.super_events.runtime.gfx" }));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.portraits.core" }));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.portraits.router" }));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.portraits.cloud" }));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.portraits.local" }));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.portraits.runpod" }));
        assert!(manifest
            .components
            .iter()
            .any(|component| { component.id == "workflow.portraits.subagent" }));
        for provider in ["cloud", "local", "runpod"] {
            let component = manifest
                .components
                .iter()
                .find(|component| component.id == format!("workflow.portraits.{provider}"))
                .expect("portrait provider component must be declared");
            assert!(component
                .dependencies
                .iter()
                .any(|dependency| dependency == "workflow.portraits.router"));
        }
        let runpod = expand_components(&manifest, &["workflow.portraits.runpod".to_string()])
            .expect("portrait provider dependency closure must resolve");
        assert!(runpod.iter().any(|id| id == "workflow.portraits.router"));
        let mcp = manifest
            .components
            .iter()
            .find(|component| component.id == "mcp.hoi4_agent_tools")
            .expect("MCP component must be declared");
        for tool in ["hoi4.tech_inspect", "hoi4.tech_render", "hoi4.tech_compare"] {
            assert!(mcp
                .capabilities
                .iter()
                .any(|capability| capability.contains(tool)));
        }
        let three_d = manifest
            .components
            .iter()
            .find(|component| component.id == "workflow.3d")
            .expect("3D component must be declared");
        assert!(three_d.required_tools.iter().any(|tool| tool.id == "uv"));
        let bootstrap = three_d
            .validation
            .iter()
            .find(|rule| rule.id == "3d.bootstrap")
            .expect("3D bootstrap action must be declared");
        assert_eq!(
            bootstrap
                .parameters
                .get("privilege")
                .and_then(Value::as_str),
            Some("current_user")
        );
        assert!(bootstrap.parameters.contains_key("arguments"));
        assert!(bootstrap.parameters.contains_key("network_access"));
        assert!(bootstrap.parameters.contains_key("expected_writes"));
        assert!(bootstrap.parameters.contains_key("rollback_boundary"));
        assert!(manifest
            .components
            .iter()
            .all(|component| component.id != "workflow.lora_comfyui_interest"));
    }

    #[test]
    fn selected_sources_omit_gitkeep_markers() {
        let mut tree_component = component("skills", &[]);
        tree_component.source = SourceDefinition {
            kind: SourceKind::Tree,
            path: "package".into(),
            include: vec!["**".into()],
            exclude: vec![],
            template_engine: None,
        };
        tree_component.destination.path = ".agents/skills/".into();
        tree_component.expected_files = vec![
            ExpectedFile {
                path: "package/.gitkeep".into(),
                sha256: Some("0".repeat(64)),
                size: Some(0),
            },
            ExpectedFile {
                path: "package/hoi4-events/SKILL.md".into(),
                sha256: Some("1".repeat(64)),
                size: Some(12),
            },
        ];
        let manifest = minimal_manifest(vec![tree_component]);
        let tree = vec![
            TreeEntry {
                path: "package/.gitkeep".into(),
                kind: "blob".into(),
                size: Some(0),
            },
            TreeEntry {
                path: "package/hoi4-events/SKILL.md".into(),
                kind: "blob".into(),
                size: Some(12),
            },
        ];

        let selected = select_component_files(&manifest, &["skills".into()], &tree).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].destination,
            ".agents/skills/hoi4-events/SKILL.md"
        );
        assert!(selected
            .iter()
            .all(|file| !file.destination.ends_with("/.gitkeep")));
    }

    #[test]
    fn super_events_package_is_complete_and_opt_in() {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json");
        let source_snapshot = "3562696c57905e6ed50f377a4ab46d5ac5ca8766";
        let manifest = parse_manifest(bytes, Some(source_snapshot)).unwrap();
        let core_profile = manifest
            .profiles
            .iter()
            .find(|profile| profile.default)
            .expect("checked-in manifest must have a default profile");
        let core = expand_components(&manifest, &core_profile.components).unwrap();
        assert!(!core
            .iter()
            .any(|id| id.starts_with("workflow.super_events")));
        let core_paths = manifest
            .components
            .iter()
            .filter(|component| core.contains(&component.id))
            .flat_map(|component| component.expected_files.iter())
            .map(|file| file.path.to_ascii_lowercase())
            .collect::<Vec<_>>();
        assert!(core_paths
            .iter()
            .all(|path| !path.contains("hoi4-super-events") && !path.contains("hoi4_super_event")));

        let selected = expand_components(&manifest, &["workflow.super_events".into()]).unwrap();
        for expected in [
            "workflow.super_events.runtime.interface",
            "workflow.super_events.runtime.common",
            "workflow.super_events.runtime.events",
            "workflow.super_events.runtime.localisation",
            "workflow.super_events.runtime.gfx",
            "workflow.super_events.runtime.docs",
            "workflow.super_events.subagents",
            "workflow.super_events",
        ] {
            assert!(selected.iter().any(|id| id == expected), "{expected}");
        }

        let paths = manifest
            .components
            .iter()
            .filter(|component| selected.contains(&component.id))
            .flat_map(|component| component.expected_files.iter())
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        for expected in [
            ".agents/skills/hoi4-super-events/SKILL.md",
            ".agents/skills/hoi4-super-events/assets/examples/contact_sheet.png",
            ".agents/skills/hoi4-super-events/assets/examples/super_event_world_in_fury.png",
            "interface/hoi4ms_super_events.gui",
            "interface/hoi4ms_super_events.gfx",
            "common/scripted_guis/hoi4ms_super_events.txt",
            "common/scripted_effects/hoi4ms_super_events.txt",
            "common/scripted_localisation/hoi4ms_super_events.txt",
            "events/hoi4ms_super_event_examples.txt",
            "localisation/english/hoi4ms_super_events_l_english.yml",
            "gfx/super_events/super_event_template.psd",
            "gfx/super_events/super_event_image_template_457x328.psd",
            "docs/super_events/README.md",
            ".codex/agents/hoi4_super_event_quote_researcher.toml",
            ".codex/agents/hoi4_super_event_audio_researcher.toml",
            ".codex/agents/hoi4_super_event_art_researcher.toml",
        ] {
            assert!(paths.contains(expected), "{expected}");
        }

        let tree = manifest
            .components
            .iter()
            .flat_map(|component| component.expected_files.iter())
            .map(|file| TreeEntry {
                path: file.path.clone(),
                kind: "blob".into(),
                size: file.size,
            })
            .collect::<Vec<_>>();
        let selected_files = select_component_files(&manifest, &selected, &tree).unwrap();
        let selected_destinations = selected_files
            .iter()
            .map(|file| canonical_relative_key(&file.destination).unwrap())
            .collect::<HashSet<_>>();
        assert_eq!(selected_destinations.len(), selected_files.len());
        let super_event_skills = selected_files
            .iter()
            .filter(|file| {
                file.destination
                    .starts_with(".agents/skills/hoi4-super-events")
                    && file.destination.ends_with("/SKILL.md")
            })
            .collect::<Vec<_>>();
        assert_eq!(super_event_skills.len(), 1);
        assert_eq!(
            super_event_skills[0].destination,
            ".agents/skills/hoi4-super-events/SKILL.md"
        );
        assert!(selected_files.iter().any(|file| {
            file.destination == ".codex/agents/hoi4_super_event_audio_researcher.toml"
        }));
    }

    #[test]
    fn repository_metadata_url_has_no_trailing_slash() {
        let client = HttpSourceClient::with_cache_root(PathBuf::from("unused-test-cache")).unwrap();
        assert_eq!(
            client.repo_url(""),
            "https://api.github.com/repos/klimPaskov/Agentic-HOI4-Modding"
        );
        assert_eq!(
            client.repo_url("/branches/main"),
            "https://api.github.com/repos/klimPaskov/Agentic-HOI4-Modding/branches/main"
        );
    }

    #[test]
    fn published_manifest_is_consumed_at_the_resolved_revision() {
        let bundled_bytes =
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json");
        let remote_bytes = bundled_bytes.to_vec();
        let resolved_revision = "8791946cffeb98e113ebf0686c3d46f735fa3eeb";

        let (selected_bytes, manifest, origin) =
            select_manifest_for_revision(&remote_bytes, resolved_revision).unwrap();

        assert_eq!(origin, "remote");
        assert_eq!(selected_bytes, remote_bytes);
        let inventory: Value = serde_json::from_str(include_str!(
            "../../docs/source-audit/live_repository_inventory.json"
        ))
        .unwrap();
        assert_eq!(
            manifest.generated_for_revision.as_deref(),
            inventory
                .get("manifest_generated_for_revision")
                .and_then(Value::as_str)
        );
    }

    #[test]
    fn authoritative_manifest_schema_rejects_unknown_top_level_fields() {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json"
        ))
        .unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unreviewed_extension".into(), Value::Bool(true));

        let error = parse_manifest(
            &serde_json::to_vec(&value).unwrap(),
            Some("27128a7b311d728a959afff7238a9aeeb9987f2b"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("authoritative JSON Schema"));
        assert!(error.contains("unreviewed_extension"));
    }

    #[test]
    fn authoritative_manifest_schema_rejects_unknown_nested_policy_fields() {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json"
        ))
        .unwrap();
        value["update_policy"]["latest"]["allow_branch_lock"] = Value::Bool(true);

        let error = parse_manifest(
            &serde_json::to_vec(&value).unwrap(),
            Some("27128a7b311d728a959afff7238a9aeeb9987f2b"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("authoritative JSON Schema"));
        assert!(error.contains("allow_branch_lock"));
    }

    #[test]
    fn authoritative_manifest_schema_accepts_verified_command_identity_fields() {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json"
        ))
        .unwrap();
        let component = value["components"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|component| component["id"] == "mcp.hoi4_agent_tools")
            .unwrap();
        let parameters = component["validation"][0]["parameters"]
            .as_object_mut()
            .unwrap();
        parameters.insert("executable_sha256".into(), Value::String("a".repeat(64)));
        parameters.insert("executable_size".into(), Value::from(1));
        parameters.insert("interpreter_sha256".into(), Value::String("b".repeat(64)));
        parameters.insert("interpreter_size".into(), Value::from(2));
        parameters.insert("runtime_sha256".into(), Value::String("c".repeat(64)));
        parameters.insert("runtime_size".into(), Value::from(3));

        parse_manifest(
            &serde_json::to_vec(&value).unwrap(),
            Some("27128a7b311d728a959afff7238a9aeeb9987f2b"),
        )
        .expect("the schema must admit every command identity field enforced by Rust");
    }

    #[test]
    fn authoritative_manifest_schema_rejects_unknown_validation_parameters() {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json"
        ))
        .unwrap();
        let component = value["components"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|component| component["id"] == "mcp.hoi4_agent_tools")
            .unwrap();
        component["validation"][0]["parameters"]["unreviewed_identity"] = Value::Bool(true);

        let error = parse_manifest(
            &serde_json::to_vec(&value).unwrap(),
            Some("27128a7b311d728a959afff7238a9aeeb9987f2b"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("authoritative JSON Schema"));
        assert!(error.contains("unreviewed_identity"));
    }

    #[test]
    fn three_d_bootstrap_requires_complete_reviewed_action_evidence() {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json"
        ))
        .unwrap();
        let component = value["components"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|component| component["id"] == "workflow.3d")
            .unwrap();
        let action = component["validation"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|rule| rule["id"] == "3d.bootstrap")
            .unwrap();
        action["parameters"]
            .as_object_mut()
            .unwrap()
            .remove("rollback_boundary");

        let error = parse_manifest(
            &serde_json::to_vec(&value).unwrap(),
            Some("3562696c57905e6ed50f377a4ab46d5ac5ca8766"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("workflow.3d bootstrap action evidence is incomplete"));
    }

    #[test]
    fn manifest_requires_exactly_one_default_profile() {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json"
        ))
        .unwrap();
        let mut second = value["profiles"][0].clone();
        second["id"] = Value::String("second-default".into());
        second["default"] = Value::Bool(true);
        value["profiles"].as_array_mut().unwrap().push(second);

        let error = parse_manifest(
            &serde_json::to_vec(&value).unwrap(),
            Some("3562696c57905e6ed50f377a4ab46d5ac5ca8766"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("exactly one default profile"));
    }

    #[test]
    fn core_profile_keeps_windows_only_mcp_components_nonblocking_on_macos() {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json");
        let manifest =
            parse_manifest(bytes, Some("32a85b3db800dcf2920c65a6093caaf1b9b81c8b")).unwrap();
        let profile = manifest
            .profiles
            .iter()
            .find(|profile| profile.default)
            .expect("checked-in manifest must have a default profile");
        let selected = expand_components(&manifest, &profile.components).unwrap();
        let macos = resolve_platform_support(&manifest, &selected, Platform::Macos).unwrap();
        let mcp = macos
            .iter()
            .find(|item| item.component_id == "mcp.hoi4_agent_tools")
            .expect("core profile must include the declared MCP component");

        assert_eq!(mcp.state, "unsupported_platform");
        assert_eq!(
            macos
                .iter()
                .find(|item| item.component_id == "mcp.hoi4_agent_tools.bootstrap")
                .expect("MCP bootstrap dependency must remain visible")
                .state,
            "unsupported_platform"
        );
        assert!(!macos.iter().any(|item| item.state == "blocked"));
        assert!(macos
            .iter()
            .filter(|item| {
                item.component_id != "mcp.hoi4_agent_tools"
                    && item.component_id != "mcp.hoi4_agent_tools.bootstrap"
            })
            .all(|item| item.state == "supported"));
    }

    #[test]
    fn dependency_expansion_is_topological() {
        let manifest = minimal_manifest(vec![component("root", &["base"]), component("base", &[])]);
        let selected = expand_components(&manifest, &["root".into()]).unwrap();
        assert_eq!(selected, vec!["base", "root"]);
    }

    #[test]
    fn reverse_dependency_evidence_lists_dependents_deterministically() {
        let manifest = minimal_manifest(vec![
            component("root", &["base"]),
            component("other", &["base"]),
            component("base", &[]),
        ]);
        let reverse = reverse_dependencies(&manifest);
        assert_eq!(
            reverse.get("base"),
            Some(&vec!["other".into(), "root".into()])
        );
        assert!(reverse.get("root").is_some_and(Vec::is_empty));
    }

    #[test]
    fn dependency_cycles_are_rejected() {
        let manifest = minimal_manifest(vec![component("a", &["b"]), component("b", &["a"])]);
        let components = manifest
            .components
            .iter()
            .map(|component| (component.id.clone(), component))
            .collect::<HashMap<_, _>>();
        assert!(validate_dependency_cycles(&components).is_err());
    }

    #[test]
    fn missing_file_evidence_is_rejected() {
        let manifest = minimal_manifest(vec![component("a", &[])]);
        assert!(validate_manifest(&manifest, None).is_err());
    }

    #[test]
    fn download_without_manifest_hash_evidence_is_rejected() {
        let selection = SelectedSourceFile {
            component_id: "core".into(),
            source_path: "file.txt".into(),
            destination: "file.txt".into(),
            ownership: Ownership::Managed,
            expected_sha256: None,
            expected_size: None,
            executable: false,
            platform: ManifestPlatform::All,
        };
        assert!(verify_download(
            &selection,
            b"incoming",
            "599497ea2f93612d9094461c6fde114fc87a5c0f"
        )
        .is_err());
    }

    #[test]
    fn verified_download_rejects_wrong_sha256() {
        let bytes = b"incoming";
        let selection = SelectedSourceFile {
            component_id: "core".into(),
            source_path: "file.txt".into(),
            destination: "file.txt".into(),
            ownership: Ownership::Managed,
            expected_sha256: Some(sha256_bytes(b"different bytes")),
            expected_size: Some(bytes.len() as u64),
            executable: false,
            platform: ManifestPlatform::All,
        };

        let error = verify_download(
            &selection,
            bytes,
            "599497ea2f93612d9094461c6fde114fc87a5c0f",
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("checksum mismatch"));
    }

    #[test]
    fn verified_download_rejects_wrong_size() {
        let bytes = b"incoming";
        let selection = SelectedSourceFile {
            component_id: "core".into(),
            source_path: "file.txt".into(),
            destination: "file.txt".into(),
            ownership: Ownership::Managed,
            expected_sha256: Some(sha256_bytes(bytes)),
            expected_size: Some(bytes.len() as u64 + 1),
            executable: false,
            platform: ManifestPlatform::All,
        };

        let error = verify_download(
            &selection,
            bytes,
            "599497ea2f93612d9094461c6fde114fc87a5c0f",
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("size mismatch"));
    }

    #[test]
    fn verified_cache_returns_bytes_from_the_same_open_when_path_is_replaced() {
        let root = tempfile::tempdir().unwrap();
        let cache_path = root.path().join("immutable-cache-entry");
        let trusted = b"trusted immutable bytes";
        let replacement = vec![b'x'; trusted.len()];
        fs::write(&cache_path, trusted).unwrap();

        let returned = read_verified_cache_with_hook(
            &cache_path,
            &sha256_bytes(trusted),
            Some(trusted.len() as u64),
            trusted.len() + 1,
            || atomic_write(&cache_path, &replacement),
        )
        .unwrap()
        .unwrap();

        assert_eq!(returned, trusted);
        assert_eq!(fs::read(&cache_path).unwrap(), replacement);
    }

    #[test]
    fn corrupt_immutable_cache_is_replaced_by_verified_download() {
        let root = tempfile::tempdir().unwrap();
        let client = HttpSourceClient::with_cache_root(root.path().to_path_buf()).unwrap();
        let revision = "599497ea2f93612d9094461c6fde114fc87a5c0f";
        let trusted = b"trusted immutable bytes";
        let expected_sha256 = sha256_bytes(trusted);
        let cache_path = client
            .cache_root
            .join("blobs")
            .join(revision)
            .join(&expected_sha256);
        fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        fs::write(&cache_path, vec![b'x'; trusted.len()]).unwrap();
        let download_count = Cell::new(0);

        let returned = client
            .fetch_verified_file_with_download(
                revision,
                "core/file.txt",
                &expected_sha256,
                Some(trusted.len() as u64),
                |_, _, _| {
                    download_count.set(download_count.get() + 1);
                    Ok(trusted.to_vec())
                },
            )
            .unwrap();

        assert_eq!(returned, trusted);
        assert_eq!(download_count.get(), 1);
        assert_eq!(fs::read(&cache_path).unwrap(), trusted);

        let cached = client
            .fetch_verified_file_with_download(
                revision,
                "core/file.txt",
                &expected_sha256,
                Some(trusted.len() as u64),
                |_, _, _| panic!("verified replacement must be served from cache"),
            )
            .unwrap();
        assert_eq!(cached, trusted);
    }

    #[test]
    fn immutable_resolution_requires_manifest_revision_evidence() {
        let manifest = minimal_manifest(vec![component("a", &[])]);
        assert!(
            validate_manifest(&manifest, Some("599497ea2f93612d9094461c6fde114fc87a5c0f")).is_err()
        );
    }

    #[test]
    fn manifest_generation_revision_may_precede_publication_revision() {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json");
        let mut manifest: RemoteManifest = serde_json::from_slice(bytes).unwrap();
        manifest.generated_for_revision = Some("599497ea2f93612d9094461c6fde114fc87a5c0f".into());
        validate_manifest(&manifest, Some("27128a7b311d728a959afff7238a9aeeb9987f2b")).unwrap();
    }

    #[test]
    fn manifest_rejects_unsupported_provenance_update_or_signing_policy() {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.v2.manifest.json");
        let revision = "27128a7b311d728a959afff7238a9aeeb9987f2b";

        let mut provenance: RemoteManifest = serde_json::from_slice(bytes).unwrap();
        provenance.wiki.provenance.source_status = "invented".into();
        assert!(validate_manifest(&provenance, Some(revision)).is_err());

        let mut update: RemoteManifest = serde_json::from_slice(bytes).unwrap();
        update.update_policy.latest.record_commit = false;
        assert!(validate_manifest(&update, Some(revision)).is_err());

        let mut signing: RemoteManifest = serde_json::from_slice(bytes).unwrap();
        signing.signing = Some(SigningPolicy {
            required: true,
            algorithm: Some("unknown".into()),
            public_key_id: Some("unknown".into()),
        });
        assert!(validate_manifest(&signing, Some(revision)).is_err());
    }

    #[test]
    fn glob_selection_does_not_include_excluded_files() {
        assert!(glob_matches("hoi4-*/**", "hoi4-events/SKILL.md"));
        assert!(!glob_matches("hoi4-*/**", "vendor/SKILL.md"));
    }

    #[test]
    fn release_tag_is_encoded_as_one_api_path_segment() {
        assert_eq!(
            HttpSourceClient::encode_path_segment("v1/release"),
            "v1%2Frelease"
        );
    }

    #[test]
    fn range_resume_requires_the_requested_start_offset() {
        assert!(content_range_starts_at("bytes 12-99/100", 12));
        assert!(!content_range_starts_at("bytes 11-99/100", 12));
        assert!(!content_range_starts_at("12-99/100", 12));
    }

    fn minimal_manifest(components: Vec<ComponentDefinition>) -> RemoteManifest {
        RemoteManifest {
            schema_version: "2.0.0".into(),
            manifest_id: "test".into(),
            generated_for_revision: None,
            repository: RepositoryDescriptor {
                provider: "github".into(),
                owner: SOURCE_OWNER.into(),
                name: SOURCE_NAME.into(),
                default_branch: "main".into(),
                web_url: None,
                api_base: None,
                license_evidence: None,
            },
            components,
            profiles: vec![],
            wiki: WikiDefinition {
                component_id: "wiki".into(),
                destination: "paradox_wiki/".into(),
                snapshot_marker: None,
                required_pages: vec!["Data structures - Hearts of Iron 4 Wiki.md".into()],
                required_media_policy: "all_declared".into(),
                provenance: WikiProvenance {
                    source_status: "repository_only".into(),
                    license_status: "not_found".into(),
                    notes: vec![],
                },
            },
            update_policy: ManifestUpdatePolicy {
                latest: LatestPolicy {
                    resolve_default_branch: true,
                    record_commit: true,
                },
                pinned: PinnedPolicy {
                    allow_commit: true,
                    allow_release: true,
                },
                rollback_retention: 3,
                manifest_cache_ttl_seconds: None,
            },
            signing: None,
        }
    }
}
