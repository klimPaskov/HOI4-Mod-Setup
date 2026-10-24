use crate::models::*;
use crate::paths::cache_root;
use crate::security::{
    atomic_write, normalize_relative_path, path_has_link_component, sha256_bytes,
};
use crate::AppError;
pub use hoi4_setup_core::manifest::*;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

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
pub struct DownloadLedger {
    pub source: SourceIdentity,
    pub selected_files: Vec<DownloadedFile>,
    pub cache_keys: Vec<String>,
}

/// Where a plan builder gets the workflow package: an exact commit, that
/// commit's manifest, and the manifest-declared files.
///
/// [`HttpSourceClient`] is the GitHub implementation. A server or a test can
/// supply its own. Planning never trusts an implementation for integrity:
/// [`resolve_source`] parses and validates the manifest it returns, and every
/// selected file is checked against the manifest's size and SHA-256 before it
/// is staged, whatever the implementation claims to have verified.
pub trait SourceFetcher: Send + Sync {
    /// Resolve a request to an exact commit, the branch it came from (latest
    /// mode) and the canonical release tag (pinned release mode).
    fn resolve_commit(
        &self,
        request: &SourceRequest,
    ) -> Result<(String, Option<String>, Option<String>), AppError>;

    /// The raw bytes of the manifest at an exact commit.
    fn fetch_manifest(&self, revision: &str) -> Result<Vec<u8>, AppError>;

    /// One manifest-declared file at an exact commit. The implementation must
    /// return bytes that match `expected_sha256` and `expected_size`.
    fn fetch_verified_file(
        &self,
        revision: &str,
        path: &str,
        expected_sha256: &str,
        expected_size: Option<u64>,
    ) -> Result<Vec<u8>, AppError>;

    /// Warm any cache for many files at once. Failures are ignored: the
    /// ordered fetch repeats a miss and reports it.
    fn prefetch_verified_files(&self, _revision: &str, _files: &[(String, String, Option<u64>)]) {}

    /// Whether [`resolve_source`] may keep this source's manifests and
    /// resolutions in its process-wide memory. Only a source that always
    /// answers from the public repository can; an injected or local source
    /// must be asked every time.
    fn shares_process_cache(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct HttpSourceClient {
    client: Client,
    api_base: String,
    owner: String,
    name: String,
    cache_root: PathBuf,
    /// Developer-only local checkout that replaces GitHub for package testing.
    local_repository: Option<PathBuf>,
}

impl HttpSourceClient {
    pub fn new() -> Result<Self, AppError> {
        Self::with_cache_root(cache_root()?)
    }

    /// A client for the public repository that keeps its verified blobs under
    /// `cache_root`. It never consults the developer's local-checkout
    /// environment variable, so a server's source cannot be redirected by its
    /// process environment. The directory must already exist.
    pub fn for_cache_root(cache_root: PathBuf) -> Result<Self, AppError> {
        Self::build(cache_root, None)
    }

    pub(crate) fn with_cache_root(cache_root: PathBuf) -> Result<Self, AppError> {
        Self::build(cache_root, crate::local_source::configured_repository())
    }

    fn build(cache_root: PathBuf, local_repository: Option<PathBuf>) -> Result<Self, AppError> {
        let client = Client::builder()
            .user_agent("HOI4-Mod-Setup/0.1")
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            // Every request has an exact repository/route/revision. A redirect
            // must not silently substitute another path, even on the same host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| AppError::Source(format!("HTTP client: {error}")))?;
        Ok(Self {
            client,
            api_base: "https://api.github.com".to_string(),
            owner: SOURCE_OWNER.to_string(),
            name: SOURCE_NAME.to_string(),
            local_repository,
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
        if let Some(root) = &self.local_repository {
            let bytes = crate::local_source::fetch(root, &parsed)?;
            if bytes.len() > limit {
                return Err(AppError::Source(
                    "source response exceeds its bounded size limit".into(),
                ));
            }
            return Ok(bytes);
        }
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .map_err(|error| AppError::Source(format!("request failed: {error}")))?;
        let status = response.status();
        if !status.is_success() {
            let rate_limited = is_rate_limit_response(status.as_u16(), response.headers());
            let category = if rate_limited {
                "GitHub rate limit"
            } else {
                "source request"
            };
            return Err(AppError::Source(format!(
                "{category} returned HTTP {status}"
            )));
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
        match self.resolve_commit_rest(request) {
            Err(AppError::Source(message))
                if message.starts_with("GitHub rate limit") && self.local_repository.is_none() =>
            {
                self.resolve_advertised_commit(request)
            }
            result => result,
        }
    }

    // Git's read-only ref advertisement has a separate public transport quota.
    // It downloads names and object IDs only, never objects or a checkout.
    fn resolve_advertised_commit(
        &self,
        request: &SourceRequest,
    ) -> Result<(String, Option<String>, Option<String>), AppError> {
        if request.mode == SourceMode::PinnedCommit {
            let revision = request
                .requested_ref
                .as_deref()
                .ok_or_else(|| AppError::Source("pinned commit requires a requested_ref".into()))?;
            validate_commit(revision)?;
            // GitHub's commit page rejects non-commit object IDs, including trees.
            self.get_bytes_limited(
                &format!("{SOURCE_REPOSITORY}/commit/{revision}"),
                8 * 1024 * 1024,
            )?;
            return Ok((revision.to_ascii_lowercase(), None, None));
        }
        let bytes = self.get_bytes_limited(
            &format!("{SOURCE_REPOSITORY}.git/info/refs?service=git-upload-pack"),
            8 * 1024 * 1024,
        )?;
        let advertisement = parse_ref_advertisement(&bytes)?;
        match request.mode {
            SourceMode::Latest => {
                let branch_ref = advertisement.default_branch.ok_or_else(|| {
                    AppError::Source("Git source did not advertise its default branch".into())
                })?;
                let revision = advertisement.refs.get(&branch_ref).ok_or_else(|| {
                    AppError::Source("Git source omitted the default branch revision".into())
                })?;
                Ok((
                    revision.clone(),
                    Some(branch_ref["refs/heads/".len()..].to_owned()),
                    None,
                ))
            }
            SourceMode::PinnedRelease => {
                let tag = request
                    .release
                    .as_deref()
                    .or(request.requested_ref.as_deref())
                    .filter(|tag| {
                        !tag.is_empty() && tag.len() <= 256 && !tag.chars().any(char::is_control)
                    })
                    .ok_or_else(|| {
                        AppError::Source("pinned release requires a release tag".into())
                    })?;
                // An advertised tag alone does not establish that a release exists.
                self.get_bytes_limited(
                    &format!(
                        "{SOURCE_REPOSITORY}/releases/tag/{}",
                        Self::encode_path_segment(tag)
                    ),
                    8 * 1024 * 1024,
                )?;
                let tag_ref = format!("refs/tags/{tag}");
                let revision = advertisement
                    .refs
                    .get(&format!("{tag_ref}^{{}}"))
                    .or_else(|| advertisement.refs.get(&tag_ref))
                    .ok_or_else(|| AppError::Source("Git source omitted the release tag".into()))?;
                self.get_bytes_limited(
                    &format!("{SOURCE_REPOSITORY}/commit/{revision}"),
                    8 * 1024 * 1024,
                )?;
                Ok((revision.clone(), None, Some(tag.to_owned())))
            }
            SourceMode::PinnedCommit => unreachable!(),
        }
    }

    fn resolve_commit_rest(
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
                    &self.repo_url(&format!("branches/{}", Self::encode_path_segment(&branch))),
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

    /// Warm the immutable, hash-addressed cache for many declared files with a
    /// small fixed number of parallel HTTPS fetches. Every file is still
    /// verified by size and SHA-256 before it is cached. Errors are ignored
    /// here: the caller's ordered fetch repeats any miss and reports it with
    /// full context. Files are de-duplicated by hash so two workers never
    /// write the same cache entry.
    pub fn prefetch_verified_files(&self, revision: &str, files: &[(String, String, Option<u64>)]) {
        const WORKERS: usize = 8;
        let mut seen = std::collections::BTreeSet::new();
        let unique = files
            .iter()
            .filter(|(_, sha256, _)| seen.insert(sha256.clone()))
            .collect::<Vec<_>>();
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..WORKERS.min(unique.len()) {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((path, sha256, size)) = unique.get(index) else {
                        break;
                    };
                    let _ = self.fetch_verified_file(revision, path, sha256, *size);
                });
            }
        });
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
        if let Some(root) = &self.local_repository {
            let bytes = crate::local_source::fetch(root, &parsed)?;
            if bytes.len() > limit {
                return Err(AppError::Source(
                    "source blob response exceeds its bounded size limit".into(),
                ));
            }
            return Ok(bytes);
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

impl SourceFetcher for HttpSourceClient {
    fn resolve_commit(
        &self,
        request: &SourceRequest,
    ) -> Result<(String, Option<String>, Option<String>), AppError> {
        HttpSourceClient::resolve_commit(self, request)
    }

    fn fetch_manifest(&self, revision: &str) -> Result<Vec<u8>, AppError> {
        HttpSourceClient::fetch_manifest(self, revision)
    }

    fn fetch_verified_file(
        &self,
        revision: &str,
        path: &str,
        expected_sha256: &str,
        expected_size: Option<u64>,
    ) -> Result<Vec<u8>, AppError> {
        HttpSourceClient::fetch_verified_file(self, revision, path, expected_sha256, expected_size)
    }

    fn prefetch_verified_files(&self, revision: &str, files: &[(String, String, Option<u64>)]) {
        HttpSourceClient::prefetch_verified_files(self, revision, files)
    }

    fn shares_process_cache(&self) -> bool {
        self.local_repository.is_none()
    }
}

#[derive(Debug)]
struct RefAdvertisement {
    default_branch: Option<String>,
    refs: HashMap<String, String>,
}

fn parse_ref_advertisement(bytes: &[u8]) -> Result<RefAdvertisement, AppError> {
    let invalid = || AppError::Source("Git source returned an invalid ref advertisement".into());
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(invalid());
    }
    let mut offset = 0;
    let mut packets = Vec::new();
    let mut terminated = false;
    while offset < bytes.len() {
        let header = bytes.get(offset..offset + 4).ok_or_else(invalid)?;
        let length = usize::from_str_radix(std::str::from_utf8(header).map_err(|_| invalid())?, 16)
            .map_err(|_| invalid())?;
        offset += 4;
        if length == 0 {
            terminated = offset == bytes.len();
            continue;
        }
        if length < 4 || packets.len() >= 100_000 {
            return Err(invalid());
        }
        let payload = bytes.get(offset..offset + length - 4).ok_or_else(invalid)?;
        packets.push(
            std::str::from_utf8(payload)
                .map_err(|_| invalid())?
                .trim_end_matches('\n'),
        );
        offset += length - 4;
    }
    if !terminated || packets.first() != Some(&"# service=git-upload-pack") {
        return Err(invalid());
    }
    let mut result = RefAdvertisement {
        default_branch: None,
        refs: HashMap::new(),
    };
    for (index, packet) in packets.iter().skip(1).enumerate() {
        let (record, capabilities) = packet.split_once('\0').unwrap_or((packet, ""));
        if !capabilities.is_empty() {
            if index != 0 {
                return Err(invalid());
            }
            for capability in capabilities.split_ascii_whitespace() {
                if let Some(branch) = capability.strip_prefix("symref=HEAD:") {
                    if !branch.starts_with("refs/heads/") || branch.len() <= 11 {
                        return Err(invalid());
                    }
                    result.default_branch = Some(branch.to_owned());
                }
            }
        }
        let (revision, name) = record.split_once(' ').ok_or_else(invalid)?;
        validate_commit(revision)?;
        if name.is_empty()
            || name.len() > 1024
            || name.chars().any(char::is_whitespace)
            || name.chars().any(char::is_control)
            || result
                .refs
                .insert(name.to_owned(), revision.to_ascii_lowercase())
                .is_some()
        {
            return Err(invalid());
        }
    }
    if let Some(branch) = &result.default_branch {
        if result.refs.get("HEAD") != result.refs.get(branch) || !result.refs.contains_key(branch) {
            return Err(invalid());
        }
    }
    Ok(result)
}

fn is_rate_limit_response(status: u16, headers: &reqwest::header::HeaderMap) -> bool {
    status == 429
        || (status == 403
            && (headers
                .get("x-ratelimit-remaining")
                .is_some_and(|value| value == "0")
                || headers.contains_key("retry-after")))
}

fn approved_source_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && match url.host_str() {
            Some("api.github.com") => {
                let prefix = format!("/repos/{SOURCE_OWNER}/{SOURCE_NAME}");
                url.path() == prefix || url.path().starts_with(&format!("{prefix}/"))
            }
            Some("raw.githubusercontent.com") => {
                let prefix = format!("/{SOURCE_OWNER}/{SOURCE_NAME}/");
                url.query().is_none()
                    && url
                        .path()
                        .strip_prefix(&prefix)
                        .and_then(|path| path.split_once('/'))
                        .is_some_and(|(revision, path)| {
                            validate_commit(revision).is_ok() && !path.is_empty()
                        })
            }
            Some("github.com") => approved_git_metadata_path(url),
            _ => false,
        }
}

fn approved_git_metadata_path(url: &reqwest::Url) -> bool {
    let prefix = format!("/{SOURCE_OWNER}/{SOURCE_NAME}");
    if url.path() == format!("{prefix}.git/info/refs") {
        return url.query() == Some("service=git-upload-pack");
    }
    url.query().is_none()
        && url.fragment().is_none()
        && (url
            .path()
            .strip_prefix(&format!("{prefix}/commit/"))
            .is_some_and(|revision| validate_commit(revision).is_ok())
            || url
                .path()
                .strip_prefix(&format!("{prefix}/releases/tag/"))
                .is_some_and(|tag| !tag.is_empty() && !tag.contains('/')))
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

#[derive(Default)]
struct ResolutionCache {
    entries: HashMap<String, (Instant, SourceResolution)>,
}

impl ResolutionCache {
    fn get(&mut self, key: &str, now: Instant) -> Option<SourceResolution> {
        self.entries.retain(|_, (expires, _)| *expires > now);
        self.entries
            .get(key)
            .map(|(_, resolution)| resolution.clone())
    }

    fn insert(&mut self, key: String, resolution: SourceResolution, now: Instant) {
        let ttl = resolution
            .manifest
            .update_policy
            .manifest_cache_ttl_seconds
            .unwrap_or(900)
            .min(900);
        if self.entries.len() >= 8 {
            self.entries.clear();
        }
        self.entries
            .insert(key, (now + Duration::from_secs(ttl), resolution));
    }
}

pub fn resolve_source<C: SourceFetcher + ?Sized>(
    client: &C,
    request: &SourceRequest,
) -> Result<SourceResolution, AppError> {
    static CACHE: OnceLock<Mutex<ResolutionCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let cache_key = serde_json::to_string(request)?;
    if client.shares_process_cache() {
        let mut cached = cache
            .lock()
            .map_err(|_| AppError::Source("source cache is unavailable".into()))?;
        if let Some(resolution) = cached.get(&cache_key, Instant::now()) {
            return Ok(resolution);
        }
    }
    let resolution = resolve_source_uncached(client, request)?;
    if client.shares_process_cache() {
        let mut cached = cache
            .lock()
            .map_err(|_| AppError::Source("source cache is unavailable".into()))?;
        cached.insert(cache_key, resolution.clone(), Instant::now());
    }
    Ok(resolution)
}

fn resolve_source_uncached<C: SourceFetcher + ?Sized>(
    client: &C,
    request: &SourceRequest,
) -> Result<SourceResolution, AppError> {
    let (revision, branch, canonical_release) = client.resolve_commit(request)?;
    let (manifest_bytes, manifest, manifest_origin) = if client.shares_process_cache() {
        manifest_for_revision(&revision, || client.fetch_manifest(&revision))?
    } else {
        let remote_manifest_bytes = client.fetch_manifest(&revision)?;
        select_manifest_for_revision(&remote_manifest_bytes, &revision)?
    };
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

/// The verified manifest of an exact commit. Commit-addressed content never
/// changes, so a manifest is downloaded, parsed, and schema-validated once per
/// revision even when Latest, a pin, and maintenance resolve to the same
/// commit under different requests. A few recent revisions are kept.
fn manifest_for_revision(
    revision: &str,
    fetch: impl FnOnce() -> Result<Vec<u8>, AppError>,
) -> Result<(Vec<u8>, RemoteManifest, &'static str), AppError> {
    type Entry = (String, Vec<u8>, RemoteManifest);
    static MANIFESTS: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    const KEPT: usize = 4;
    let manifests = MANIFESTS.get_or_init(Default::default);
    let unavailable = || AppError::Source("source cache is unavailable".into());
    if let Some((_, bytes, manifest)) = manifests
        .lock()
        .map_err(|_| unavailable())?
        .iter()
        .find(|(known, _, _)| known == revision)
    {
        return Ok((bytes.clone(), manifest.clone(), "remote"));
    }
    let remote_manifest_bytes = fetch()?;
    let selected = select_manifest_for_revision(&remote_manifest_bytes, revision)?;
    let mut manifests = manifests.lock().map_err(|_| unavailable())?;
    manifests.retain(|(known, _, _)| known != revision);
    if manifests.len() >= KEPT {
        manifests.remove(0);
    }
    manifests.push((revision.to_owned(), selected.0.clone(), selected.1.clone()));
    Ok(selected)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::canonical_relative_key;
    use std::collections::HashSet;

    #[test]
    fn a_revision_manifest_is_fetched_and_validated_once() {
        let bytes = include_bytes!("../../docs/fixtures/format6-candidate-manifest.json").to_vec();
        let revision = "1234567890abcdef1234567890abcdef12345678";
        let mut fetches = 0;
        for _ in 0..3 {
            let (cached, manifest, origin) = manifest_for_revision(revision, || {
                fetches += 1;
                Ok(bytes.clone())
            })
            .unwrap();
            assert_eq!(cached, bytes);
            assert_eq!(manifest.schema_version, "6.0.0");
            assert_eq!(origin, "remote");
        }
        assert_eq!(fetches, 1);
        // A failed fetch is never cached.
        let other = "abcdefabcdefabcdefabcdefabcdefabcdefabcd";
        assert!(manifest_for_revision(other, || Err(AppError::Source("offline".into()))).is_err());
        let mut retried = false;
        manifest_for_revision(other, || {
            retried = true;
            Ok(bytes.clone())
        })
        .unwrap();
        assert!(retried);
    }

    #[test]
    fn latest_local_source_resolves_a_branch_with_a_slash() {
        let repository = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let git = crate::git::find_git_executable().unwrap();
        for arguments in [
            vec!["init", "-b", "fix/source-pin"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-m",
                "Source fixture",
            ],
        ] {
            let mut command = std::process::Command::new(&git);
            command
                .arg("-C")
                .arg(repository.path())
                .args(arguments)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    if cfg!(windows) { "NUL" } else { "/dev/null" },
                );
            crate::process::configure_child_no_console_window(&mut command);
            assert!(command.output().unwrap().status.success());
        }
        let mut client = HttpSourceClient::with_cache_root(cache.path().to_owned()).unwrap();
        client.local_repository = Some(repository.path().to_owned());
        let (revision, branch, _) = client
            .resolve_commit(&SourceRequest {
                mode: SourceMode::Latest,
                requested_ref: None,
                release: None,
            })
            .unwrap();
        assert_eq!(branch.as_deref(), Some("fix/source-pin"));
        validate_commit(&revision).unwrap();
    }
    use std::cell::Cell;

    fn component(id: &str, dependencies: &[&str]) -> ComponentDefinition {
        ComponentDefinition {
            id: id.to_string(),
            display_name: id.to_string(),
            description: None,
            category: ComponentCategory::Core,
            coding_environment: None,
            shared_coding_environments: vec![],
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
        assert_eq!(MANIFEST_PATH, "hoi4-mod-setup.manifest.json");
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
        let inventory: Value = serde_json::from_str(include_str!(
            "../../docs/source-audit/current_manifest_identity.json"
        ))
        .unwrap();
        let manifest_sha256 = sha256_bytes(bytes);
        assert_eq!(
            inventory["manifest_sha256"].as_str(),
            Some(manifest_sha256.as_str()),
            "source audit must identify the exact bundled manifest bytes"
        );
        assert_eq!(
            inventory["manifest_size"].as_u64(),
            Some(bytes.len() as u64)
        );
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
        assert_eq!(manifest.components.len(), 60);
        assert!(manifest
            .components
            .iter()
            .any(|component| component.id == "docs.linux"));
        for environment in crate::coding_environment::SUPPORTED {
            assert!(manifest
                .components
                .iter()
                .any(|component| component.coding_environment.as_deref() == Some(environment)));
        }
        let core_profile = manifest
            .profiles
            .iter()
            .find(|profile| profile.default)
            .expect("checked-in manifest must have a default profile");
        assert!(core_profile
            .components
            .iter()
            .any(|component| component == "docs.mcp_integration"));
        assert!(core_profile
            .components
            .iter()
            .any(|component| component == "docs.runtimes"));
        assert!(manifest.components.iter().any(|component| {
            component.id == "core.claude.instructions"
                && component.source.template_engine.as_deref() == Some("agents_adaptation_v1")
                && component.destination.path == "CLAUDE.md"
        }));
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
        assert!(!mcp
            .dependencies
            .iter()
            .any(|dependency| dependency == "codex.config"));
        assert!(mcp
            .dependencies
            .iter()
            .any(|dependency| dependency == "mcp.hoi4_agent_tools.bootstrap"));
        let mcp_health = mcp
            .validation
            .iter()
            .find(|rule| rule.id == "mcp.hoi4.health")
            .expect("current MCP health declaration must be present");
        assert_eq!(
            mcp_health.parameters["package_version"].as_str(),
            Some("4.0.3")
        );
        assert_eq!(
            mcp_health.parameters["required_tools"]
                .as_array()
                .map(Vec::len),
            Some(28)
        );
        let workflow_3d = manifest
            .components
            .iter()
            .find(|component| component.id == "workflow.3d")
            .unwrap();
        assert!(!workflow_3d
            .dependencies
            .iter()
            .any(|dependency| dependency == "codex.config"));
        for tool in ["hoi4.tech_inspect", "hoi4.tech_render"] {
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
    fn unknown_component_template_engine_fails_closed() {
        let mut manifest: RemoteManifest = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        let component = manifest
            .components
            .iter_mut()
            .find(|component| component.id == "core.claude.instructions")
            .unwrap();
        component.source.template_engine = Some("future_unreviewed_engine".into());
        let error = validate_manifest(&manifest, None).unwrap_err();
        assert!(error.to_string().contains("unsupported template engine"));
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
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
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
            ".claude/skills/hoi4-super-events/SKILL.md",
            ".claude/skills/hoi4-super-events/assets/examples/contact_sheet.png",
            ".claude/skills/hoi4-super-events/assets/examples/super_event_world_in_fury.png",
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
            ".claude/agents/hoi4-super-event-quote-researcher.md",
            ".claude/agents/hoi4-super-event-audio-researcher.md",
            ".claude/agents/hoi4-super-event-art-researcher.md",
        ] {
            assert!(paths.contains(expected), "{expected}");
        }

        // A Git tree lists each path once even when two components (such as
        // a canonical skill and its runtime copy) read the same source file.
        let mut seen_paths = HashSet::new();
        let tree = manifest
            .components
            .iter()
            .flat_map(|component| component.expected_files.iter())
            .filter(|file| seen_paths.insert(file.path.clone()))
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
                    .starts_with(".claude/skills/hoi4-super-events")
                    && file.destination.ends_with("/SKILL.md")
            })
            .collect::<Vec<_>>();
        assert_eq!(super_event_skills.len(), 1);
        assert_eq!(
            super_event_skills[0].destination,
            ".claude/skills/hoi4-super-events/SKILL.md"
        );
        assert!(selected_files.iter().any(|file| {
            file.destination == ".claude/agents/hoi4-super-event-audio-researcher.md"
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
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
        let remote_bytes = bundled_bytes.to_vec();
        let inventory: Value = serde_json::from_str(include_str!(
            "../../docs/source-audit/current_manifest_identity.json"
        ))
        .unwrap();
        let resolved_revision = inventory["resolved_commit"].as_str().unwrap();

        let (selected_bytes, manifest, origin) =
            select_manifest_for_revision(&remote_bytes, resolved_revision).unwrap();

        assert_eq!(origin, "remote");
        assert_eq!(selected_bytes, remote_bytes);
        assert_eq!(
            manifest.generated_for_revision.as_deref(),
            inventory
                .get("manifest_generated_for_revision")
                .and_then(Value::as_str)
        );
    }

    fn live_tools_manifest_value() -> (Value, String) {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
        let inventory: Value = serde_json::from_str(include_str!(
            "../../docs/source-audit/current_manifest_identity.json"
        ))
        .unwrap();
        (
            serde_json::from_slice(bytes).unwrap(),
            inventory["manifest_generated_for_revision"]
                .as_str()
                .unwrap()
                .to_string(),
        )
    }

    fn live_tools_action(value: &mut Value) -> &mut Value {
        value["components"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|component| component["id"] == "workflow.live_tools")
            .unwrap()["validation"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|rule| rule["id"] == "live.bootstrap")
            .unwrap()
    }

    #[test]
    fn live_tools_bootstrap_pins_one_reviewed_release() {
        let (value, revision) = live_tools_manifest_value();
        let manifest =
            parse_manifest(&serde_json::to_vec(&value).unwrap(), Some(&revision)).unwrap();
        let component = manifest
            .components
            .iter()
            .find(|component| component.id == "workflow.live_tools")
            .expect("the HOI4 Live Tools workflow must be published");
        let actions = component
            .validation
            .iter()
            .filter(|rule| rule.kind == "command")
            .collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        let action = actions[0];
        assert_eq!(
            action.target.as_deref(),
            Some(".tools/mcp/bootstrap_hoi4_live_tools.py")
        );
        assert_eq!(
            action.parameters["arguments"],
            serde_json::json!(["--quiet", "--register", "none"])
        );
        assert!(action.parameters["release_url"]
            .as_str()
            .unwrap()
            .starts_with("https://github.com/klimPaskov/HOI4-Live-Tools/releases/download/"));
        assert_eq!(
            action.parameters["release_sha256"].as_str().unwrap().len(),
            64
        );
        assert!(component
            .dependencies
            .iter()
            .any(|dependency| dependency == "mcp.hoi4_live.bootstrap"));
        let script = manifest
            .components
            .iter()
            .find(|component| component.id == "mcp.hoi4_live.bootstrap")
            .expect("the managed bootstrap script must be published");
        assert_eq!(
            script.destination.path,
            ".tools/mcp/bootstrap_hoi4_live_tools.py"
        );
    }

    #[test]
    fn live_tools_bootstrap_requires_complete_reviewed_action_evidence() {
        for (field, expected) in [
            (
                "rollback_boundary",
                "HOI4 Live Tools bootstrap action evidence is incomplete",
            ),
            (
                "network_access",
                "HOI4 Live Tools bootstrap action evidence is incomplete",
            ),
            ("release_sha256", "invalid lowercase SHA-256"),
            (
                "release_size",
                "HOI4 Live Tools bootstrap action evidence is incomplete",
            ),
        ] {
            let (mut value, revision) = live_tools_manifest_value();
            live_tools_action(&mut value)["parameters"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            let error = parse_manifest(&serde_json::to_vec(&value).unwrap(), Some(&revision))
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{field}: {error}");
        }
        let (mut value, revision) = live_tools_manifest_value();
        live_tools_action(&mut value)["parameters"]["arguments"] =
            serde_json::json!(["--quiet", "--anything"]);
        let error = parse_manifest(&serde_json::to_vec(&value).unwrap(), Some(&revision))
            .unwrap_err()
            .to_string();
        assert!(error.contains("HOI4 Live Tools bootstrap action evidence is incomplete"));
    }

    #[test]
    fn live_tools_workflow_without_an_installer_stays_a_skill_only_workflow() {
        let (mut value, revision) = live_tools_manifest_value();
        value.as_object_mut().unwrap().remove("live_testing");
        value["components"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|component| component["id"] == "workflow.live_tools")
            .unwrap()["validation"] = serde_json::json!([]);
        parse_manifest(&serde_json::to_vec(&value).unwrap(), Some(&revision)).unwrap();
    }

    #[test]
    fn authoritative_manifest_schema_rejects_unknown_top_level_fields() {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
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
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
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
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
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
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
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
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
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
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
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
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
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
    fn verified_download_rejects_an_unresolved_git_lfs_pointer() {
        let bytes: &[u8] = b"version https://git-lfs.github.com/spec/v1\noid sha256:0c87ae522997ef55ebb907eee55a25391e8e5dcdc52a75dec09ad52e73abbe66\nsize 11604\n";
        let selection = SelectedSourceFile {
            component_id: "wiki.snapshot".into(),
            source_path: "paradox_wiki/media/image.jpg".into(),
            destination: "paradox_wiki/media/image.jpg".into(),
            ownership: Ownership::Managed,
            expected_sha256: Some(sha256_bytes(bytes)),
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

        assert!(error.contains("unresolved Git LFS pointer"));
        assert!(error.contains("paradox_wiki/media/image.jpg"));
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
    fn prefetch_reuses_verified_cache_entries_and_tolerates_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let client = HttpSourceClient::with_cache_root(root.path().to_path_buf()).unwrap();
        let revision = "599497ea2f93612d9094461c6fde114fc87a5c0f";
        let mut files = Vec::new();
        for index in 0..20 {
            let bytes = format!("cached file {index}").into_bytes();
            let sha256 = sha256_bytes(&bytes);
            let cache_path = client.cache_root.join("blobs").join(revision).join(&sha256);
            fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
            fs::write(&cache_path, &bytes).unwrap();
            files.push((
                format!("core/{index}.txt"),
                sha256.clone(),
                Some(bytes.len() as u64),
            ));
            files.push((
                format!("copy/{index}.txt"),
                sha256,
                Some(bytes.len() as u64),
            ));
        }

        client.prefetch_verified_files(revision, &files);

        for (path, sha256, size) in &files {
            let bytes = client
                .fetch_verified_file_with_download(revision, path, sha256, *size, |_, _, _| {
                    panic!("prefetched files must be served from the verified cache")
                })
                .unwrap();
            assert_eq!(sha256_bytes(&bytes), *sha256);
        }
    }

    #[test]
    fn cold_verified_cache_commits_first_blob_and_removes_partial_file() {
        let root = tempfile::tempdir().unwrap();
        let client = HttpSourceClient::with_cache_root(root.path().to_path_buf()).unwrap();
        let revision = "599497ea2f93612d9094461c6fde114fc87a5c0f";
        let trusted = b"first verified download";
        let digest = sha256_bytes(trusted);
        let cache_path = client.cache_root.join("blobs").join(revision).join(&digest);
        assert!(!cache_path.exists());
        let returned = client
            .fetch_verified_file_with_download(
                revision,
                "core/first.txt",
                &digest,
                Some(trusted.len() as u64),
                |_, partial, _| {
                    fs::create_dir_all(partial.parent().unwrap())?;
                    fs::write(partial, trusted)?;
                    Ok(trusted.to_vec())
                },
            )
            .unwrap();
        assert_eq!(returned, trusted);
        assert_eq!(fs::read(&cache_path).unwrap(), trusted);
        assert!(!cache_path.with_extension("part").exists());
        let cached = client
            .fetch_verified_file_with_download(
                revision,
                "core/first.txt",
                &digest,
                Some(trusted.len() as u64),
                |_, _, _| panic!("the first verified blob must be reusable"),
            )
            .unwrap();
        assert_eq!(cached, trusted);
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
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
        let mut manifest: RemoteManifest = serde_json::from_slice(bytes).unwrap();
        manifest.generated_for_revision = Some("599497ea2f93612d9094461c6fde114fc87a5c0f".into());
        validate_manifest(&manifest, Some("27128a7b311d728a959afff7238a9aeeb9987f2b")).unwrap();
    }

    #[test]
    fn manifest_rejects_unsupported_provenance_update_or_signing_policy() {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
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

    fn advertisement_fixture(records: &[String]) -> Vec<u8> {
        let mut bytes = b"001e# service=git-upload-pack\n0000".to_vec();
        for record in records {
            bytes.extend_from_slice(format!("{:04x}{record}\n", record.len() + 5).as_bytes());
        }
        bytes.extend_from_slice(b"0000");
        bytes
    }

    #[test]
    fn git_advertisement_binds_default_branch_and_peeled_tags() {
        let commit = "a".repeat(40);
        let bytes = advertisement_fixture(&[
            format!("{commit} HEAD\0symref=HEAD:refs/heads/release/main object-format=sha1"),
            format!("{commit} refs/heads/release/main"),
            format!("{} refs/tags/v1.0", "b".repeat(40)),
            format!("{commit} refs/tags/v1.0^{{}}"),
        ]);
        let result = parse_ref_advertisement(&bytes).unwrap();
        assert_eq!(
            result.default_branch.as_deref(),
            Some("refs/heads/release/main")
        );
        assert_eq!(result.refs["refs/tags/v1.0^{}"], commit);
        for invalid in [
            b"not git".to_vec(),
            bytes[..bytes.len() - 4].to_vec(),
            b"ffffshort".to_vec(),
            advertisement_fixture(&[format!("{commit} HEAD\0symref=HEAD:refs/heads/missing")]),
            advertisement_fixture(&[format!("{commit} HEAD"), format!("{commit} HEAD")]),
        ] {
            assert!(parse_ref_advertisement(&invalid).is_err());
        }
    }

    #[test]
    fn git_metadata_fallback_cannot_visit_another_repository_or_service() {
        for suffix in [
            ".git/info/refs?service=git-upload-pack",
            "/releases/tag/v1.0",
            &format!("/commit/{}", "a".repeat(40)),
        ] {
            assert!(approved_source_url(
                &reqwest::Url::parse(&format!("{SOURCE_REPOSITORY}{suffix}")).unwrap()
            ));
        }
        for url in [
            "https://github.com/attacker/repository.git/info/refs?service=git-upload-pack",
            "https://github.com/klimPaskov/Agentic-HOI4-Modding.git/info/refs?service=git-receive-pack",
            "https://github.com/klimPaskov/Agentic-HOI4-Modding/commit/main",
            "https://github.com/klimPaskov/Agentic-HOI4-Modding/releases/tag/v1?redirect=elsewhere",
            "https://api.github.com/repos/attacker/Agentic-HOI4-Modding",
            "https://api.github.com/repos/klimPaskov/Agentic-HOI4-Modding-extra",
            "https://raw.githubusercontent.com/attacker/Agentic-HOI4-Modding/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/hoi4-mod-setup.manifest.json",
            "https://raw.githubusercontent.com/klimPaskov/Agentic-HOI4-Modding/main/hoi4-mod-setup.manifest.json",
        ] {
            assert!(!approved_source_url(&reqwest::Url::parse(url).unwrap()));
        }
    }

    #[test]
    fn source_http_client_never_follows_cross_path_or_cross_revision_redirects() {
        let cache = tempfile::tempdir().unwrap();
        let client = HttpSourceClient::with_cache_root(cache.path().to_path_buf()).unwrap();
        for location in [
            "https://api.github.com/repos/attacker/repository",
            "https://raw.githubusercontent.com/klimPaskov/Agentic-HOI4-Modding/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/hoi4-mod-setup.manifest.json",
            "https://raw.githubusercontent.com/klimPaskov/Agentic-HOI4-Modding/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/another-file",
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut bytes = [0; 8192];
                let _ = stream.read(&mut bytes).unwrap();
                write!(stream, "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            });
            let response = client.client.get(format!("http://{address}/exact-source")).send().unwrap();
            assert_eq!(response.status().as_u16(), 302);
            assert_eq!(response.url().path(), "/exact-source");
            server.join().unwrap();
        }
    }

    #[test]
    fn only_rate_limit_statuses_allow_the_metadata_fallback() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert!(!is_rate_limit_response(403, &headers));
        assert!(!is_rate_limit_response(404, &headers));
        assert!(is_rate_limit_response(429, &headers));
        headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
        assert!(is_rate_limit_response(403, &headers));
        assert!(!is_rate_limit_response(401, &headers));
        headers.clear();
        headers.insert("retry-after", "60".parse().unwrap());
        assert!(is_rate_limit_response(403, &headers));
    }

    #[test]
    fn source_cache_honors_expiry_request_isolation_and_manifest_revision() {
        let bytes = include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json");
        let mut resolution = SourceResolution {
            identity: SourceIdentity {
                repository: format!("{SOURCE_OWNER}/{SOURCE_NAME}"),
                mode: SourceMode::Latest,
                resolved_revision: "a".repeat(40),
                requested_ref: None,
                release: None,
                manifest_sha256: sha256_bytes(bytes),
                manifest_origin: "remote".into(),
            },
            manifest: parse_manifest(bytes, None).unwrap(),
            manifest_bytes: bytes.to_vec(),
            branch: Some("main".into()),
        };
        let now = Instant::now();
        let mut cache = ResolutionCache::default();
        resolution.manifest.update_policy.manifest_cache_ttl_seconds = Some(2);
        cache.insert("latest".into(), resolution.clone(), now);
        assert_eq!(
            cache
                .get("latest", now + Duration::from_secs(1))
                .unwrap()
                .identity
                .resolved_revision,
            "a".repeat(40)
        );
        assert!(cache.get("pinned", now).is_none());
        assert!(cache.get("latest", now + Duration::from_secs(2)).is_none());
        resolution.identity.resolved_revision = "b".repeat(40);
        resolution.manifest.update_policy.manifest_cache_ttl_seconds = Some(86400);
        cache.insert("latest".into(), resolution.clone(), now);
        assert_eq!(
            cache.get("latest", now).unwrap().identity.resolved_revision,
            "b".repeat(40)
        );
        assert!(cache
            .get("latest", now + Duration::from_secs(900))
            .is_none());
        resolution.manifest.update_policy.manifest_cache_ttl_seconds = Some(0);
        cache.insert("disabled".into(), resolution, now);
        assert!(cache.get("disabled", now).is_none());
    }

    #[test]
    fn manifest_inventory_selects_only_declared_closure_and_rejects_outside_evidence() {
        let mut manifest = parse_manifest(
            include_bytes!("../../docs/source-manifest/hoi4-mod-setup.manifest.json"),
            None,
        )
        .unwrap();
        let selected = vec!["core.skills".to_owned()];
        let files = select_manifest_files(&manifest, &selected).unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().all(|file| file.component_id == "core.skills"
            && file.expected_sha256.is_some()
            && file.expected_size.is_some()));
        let component = manifest
            .components
            .iter_mut()
            .find(|component| component.id == "core.skills")
            .unwrap();
        component.expected_files[0].path = "outside/component.txt".into();
        assert!(select_manifest_files(&manifest, &selected).is_err());
    }

    #[test]
    #[ignore = "contacts the public source; no credentials or project writes"]
    fn live_rate_limit_fallback_resolves_and_verifies_manifest() {
        let cache = tempfile::tempdir().unwrap();
        let mut client = HttpSourceClient::with_cache_root(cache.path().to_path_buf()).unwrap();
        client.local_repository = None;
        for request in [
            SourceRequest {
                mode: SourceMode::Latest,
                requested_ref: None,
                release: None,
            },
            SourceRequest {
                mode: SourceMode::PinnedRelease,
                requested_ref: None,
                release: Some("package-2026-10-13.2".into()),
            },
        ] {
            let (revision, _, _) = client.resolve_advertised_commit(&request).unwrap();
            let bytes = client.fetch_manifest(&revision).unwrap();
            let manifest = parse_manifest(&bytes, Some(&revision)).unwrap();
            let profile = manifest
                .profiles
                .iter()
                .find(|profile| profile.default)
                .unwrap();
            let selected = expand_components(&manifest, &profile.components).unwrap();
            assert!(!select_manifest_files(&manifest, &selected)
                .unwrap()
                .is_empty());
        }
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
            coding_environments: Vec::new(),
            image_generation: None,
            live_testing: None,
        }
    }

    fn format_six_fixture() -> Value {
        let mut value: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        value["schema_version"] = serde_json::json!("6.0.0");
        for environment in value["coding_environments"].as_array_mut().unwrap() {
            let id = environment["id"].as_str().unwrap().to_string();
            let mcp = crate::coding_environment::native_client(&id)
                .and_then(|client| client.mcp)
                .unwrap();
            environment["mcp_support"] = serde_json::json!("project");
            environment["mcp_registration"] = serde_json::json!({
                "component_id": mcp.component_id,
                "path": mcp.path,
                "format": mcp.format,
            });
        }
        value
    }

    fn parse_value(value: &Value) -> Result<RemoteManifest, AppError> {
        parse_manifest(&serde_json::to_vec(value).unwrap(), None)
    }

    #[test]
    fn format_six_mcp_declarations_and_shared_components_are_validated() {
        let mut legacy: Value = serde_json::from_slice(include_bytes!(
            "../../docs/source-manifest/hoi4-mod-setup.manifest.json"
        ))
        .unwrap();
        legacy["coding_environments"][0]["mcp_support"] = serde_json::json!("project");
        assert!(
            parse_value(&legacy).is_err(),
            "format 5 has no MCP declarations"
        );

        let valid = format_six_fixture();
        let manifest = parse_value(&valid).unwrap();
        assert_eq!(manifest.schema_version, "6.0.0");
        assert!(manifest
            .coding_environments
            .iter()
            .all(|environment| environment.mcp_support == Some(McpSupport::Project)));

        let shared = |environments: Value| {
            let mut value = valid.clone();
            let component = value["components"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|component| component["id"] == "runtime.codex.skills")
                .unwrap();
            component["shared_coding_environments"] = environments;
            value
        };
        assert!(parse_value(&shared(serde_json::json!(["cursor", "opencode"]))).is_ok());
        for invalid in [
            serde_json::json!(["unknown_client"]),
            serde_json::json!(["codex"]),
            serde_json::json!(["cursor", "cursor"]),
        ] {
            assert!(parse_value(&shared(invalid.clone())).is_err(), "{invalid}");
        }

        let environment = |id: &str, edit: &dyn Fn(&mut Value)| {
            let mut value = valid.clone();
            let entry = value["coding_environments"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|environment| environment["id"] == id)
                .unwrap();
            edit(entry);
            value
        };
        let invalid = [
            environment("codex", &|entry| {
                entry.as_object_mut().unwrap().remove("mcp_registration");
            }),
            environment("cursor", &|entry| {
                entry["mcp_support"] = serde_json::json!("manual")
            }),
            environment("claude_code", &|entry| {
                entry.as_object_mut().unwrap().remove("mcp_support");
            }),
            environment("claude_code", &|entry| {
                entry["mcp_registration"]["component_id"] = serde_json::json!("runtime.cursor.mcp")
            }),
            environment("claude_code", &|entry| {
                entry["mcp_registration"]["path"] = serde_json::json!(".claude/settings.json")
            }),
            environment("claude_code", &|entry| {
                entry["mcp_registration"]["path"] = serde_json::json!("../.mcp.json")
            }),
            environment("claude_code", &|entry| {
                entry["mcp_registration"]["format"] = serde_json::json!("yaml_servers")
            }),
            environment("qoder", &|entry| {
                entry["documentation_url"] = serde_json::json!("http://example.com/mcp")
            }),
        ];
        for (index, value) in invalid.iter().enumerate() {
            assert!(
                parse_value(value).is_err(),
                "invalid declaration {index} was accepted"
            );
        }
        let manual = environment("cursor", &|entry| {
            entry["mcp_support"] = serde_json::json!("manual");
            entry.as_object_mut().unwrap().remove("mcp_registration");
            entry["documentation_url"] = serde_json::json!("https://docs.cursor.com/");
        });
        assert!(parse_value(&manual).is_ok());
    }

    #[test]
    fn newer_manifest_formats_ask_for_an_app_update() {
        let mut value = format_six_fixture();
        value["schema_version"] = serde_json::json!("7.0.0");
        let error = parse_value(&value).unwrap_err().to_string();
        assert!(error.contains("needs a newer HOI4 Mod Setup"), "{error}");
    }
}
