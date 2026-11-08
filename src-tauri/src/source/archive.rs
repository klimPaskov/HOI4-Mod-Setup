//! One-archive download of the pinned kit commit (#168).
//!
//! An install used to fetch every manifest-declared file with its own request
//! (about 1,700 of them). The archive route downloads the exact commit once as
//! `codeload.github.com/<owner>/<name>/tar.gz/<commit>`, streams it to a
//! resumable partial file, and copies only manifest-declared entries that match
//! their declared size and SHA-256 into the same hash-addressed blob cache the
//! per-file route fills. Nothing from the archive is trusted or used as a path:
//! an entry is looked up by its exact manifest path and stored under its
//! verified hash. Anything missing, mismatched or unreadable stays a cache
//! miss, and the per-file route fetches it as before.

use super::*;
use std::collections::HashSet;

/// Below this many uncached files the per-file route is cheaper than a full
/// archive of the repository.
pub(super) const ARCHIVE_MIN_MISSING: usize = 24;
/// Compressed archive bound. The kit archive is a small fraction of this.
const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
/// Total declared size of every archive entry, read or skipped.
const MAX_ARCHIVE_UNPACKED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const ARCHIVE_TIMEOUT: Duration = Duration::from_secs(1200);

/// Manifest path -> (SHA-256, declared size).
pub(super) type Wanted = HashMap<String, (String, Option<u64>)>;

pub(super) fn blob_cache_path(cache_root: &Path, revision: &str, sha256: &str) -> PathBuf {
    cache_root.join("blobs").join(revision).join(sha256)
}

pub(super) fn archive_url(owner: &str, name: &str, revision: &str) -> String {
    format!("https://codeload.github.com/{owner}/{name}/tar.gz/{revision}")
}

/// `/<owner>/<name>/tar.gz/<commit>` on codeload, with no query.
pub(super) fn approved_archive_path(url: &reqwest::Url) -> bool {
    url.query().is_none()
        && url
            .path()
            .strip_prefix(&format!("/{SOURCE_OWNER}/{SOURCE_NAME}/tar.gz/"))
            .is_some_and(|revision| validate_commit(revision).is_ok())
}

impl HttpSourceClient {
    /// Download the commit's archive once and cache every verified wanted
    /// entry. Returns how many blobs were stored. Any error leaves the
    /// per-file route to finish the job.
    pub(super) fn prefetch_from_archive(
        &self,
        revision: &str,
        wanted: &Wanted,
    ) -> Result<usize, AppError> {
        validate_commit(revision)?;
        let url = archive_url(&self.owner, &self.name, revision);
        let parsed = reqwest::Url::parse(&url)
            .map_err(|error| AppError::Source(format!("invalid source URL: {error}")))?;
        if !approved_source_url(&parsed) {
            return Err(AppError::Source(
                "source archive URL is not an approved endpoint".into(),
            ));
        }
        let client = Client::builder()
            .user_agent("HOI4-Mod-Setup/0.1")
            .connect_timeout(Duration::from_secs(15))
            .timeout(ARCHIVE_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| AppError::Source(format!("HTTP client: {error}")))?;
        let part = self
            .cache_root
            .join("archives")
            .join(format!("{revision}.tar.gz.part"));
        download_archive(&client, &url, &part, &mut std::thread::sleep)?;
        let result = File::open(&part)
            .map_err(|error| AppError::Source(format!("open source archive: {error}")))
            .and_then(|file| {
                extract_archive_to_cache(
                    std::io::BufReader::new(file),
                    &self.cache_root,
                    revision,
                    wanted,
                )
            });
        // A fully received archive is consumed either way; a bad one must not
        // be reused.
        let _ = fs::remove_file(&part);
        result
    }
}

/// Stream `url` into `part`, resuming from what is already there when the
/// server honours a range, and retrying throttles, server errors and resets.
pub(super) fn download_archive(
    client: &Client,
    url: &str,
    part: &Path,
    sleep: &mut dyn FnMut(Duration),
) -> Result<(), AppError> {
    with_retries(&SOURCE_RETRY, sleep, || archive_attempt(client, url, part))
}

fn archive_attempt(client: &Client, url: &str, part: &Path) -> Attempt<()> {
    let retry = |error: String| Attempt::Retry {
        error: AppError::Source(error),
        retry_after: None,
    };
    if path_has_link_component(part) {
        return Attempt::Fail(AppError::PathSecurity(
            "source archive path contains a symlink or junction".into(),
        ));
    }
    let mut offset = match fs::symlink_metadata(part) {
        Ok(metadata) if metadata.is_file() && !crate::security::is_link_metadata(&metadata) => {
            metadata.len()
        }
        Ok(_) => {
            return Attempt::Fail(AppError::PathSecurity(
                "source archive partial file is not a regular file".into(),
            ))
        }
        Err(_) => 0,
    };
    if offset >= MAX_ARCHIVE_BYTES {
        let _ = fs::remove_file(part);
        offset = 0;
    }
    let mut request = client.get(url).header("Accept", "application/octet-stream");
    if offset > 0 {
        request = request.header("Range", format!("bytes={offset}-"));
    }
    let mut response = match request.send() {
        Ok(response) => response,
        Err(error) => return retry(format!("source archive request failed: {error}")),
    };
    let status = response.status();
    if offset > 0 && status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let _ = fs::remove_file(part);
        return retry("source archive partial file was stale".into());
    }
    if !status.is_success() {
        let code = status.as_u16();
        let error = AppError::Source(format!("source archive returned HTTP {status}"));
        return if is_retryable_status(code) {
            Attempt::Retry {
                error,
                retry_after: retry_after_seconds(response.headers()),
            }
        } else {
            Attempt::Fail(error)
        };
    }
    // Redirects are refused by the client, so the response is the approved URL.
    let resumed = offset > 0
        && status == reqwest::StatusCode::PARTIAL_CONTENT
        && response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| content_range_starts_at(value, offset as usize));
    if offset > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT && !resumed {
        let _ = fs::remove_file(part);
        return retry("source archive returned an invalid range response".into());
    }
    let start = if resumed { offset } else { 0 };
    let expected_total = response.content_length().map(|length| length + start);
    if expected_total.is_some_and(|total| total > MAX_ARCHIVE_BYTES) {
        return Attempt::Fail(AppError::Source(
            "source archive exceeds its bounded size limit".into(),
        ));
    }
    let mut file = match open_partial_cache(part, !resumed) {
        Ok(file) => file,
        Err(error) => return Attempt::Fail(error),
    };
    let mut written = start;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = match response.read(&mut buffer) {
            Ok(read) => read,
            Err(error) => {
                let _ = file.sync_all();
                return retry(format!("source archive interrupted: {error}"));
            }
        };
        if read == 0 {
            break;
        }
        written += read as u64;
        if written > MAX_ARCHIVE_BYTES {
            let _ = file.sync_all();
            let _ = fs::remove_file(part);
            return Attempt::Fail(AppError::Source(
                "source archive exceeds its bounded size limit".into(),
            ));
        }
        if let Err(error) = file.write_all(&buffer[..read]) {
            return Attempt::Fail(AppError::Source(format!("write source archive: {error}")));
        }
    }
    if let Err(error) = file.sync_all() {
        return Attempt::Fail(AppError::Source(format!("sync source archive: {error}")));
    }
    if expected_total.is_some_and(|total| written != total) {
        return retry("source archive ended before its declared length".into());
    }
    Attempt::Done(())
}

/// Copy every wanted entry whose size and SHA-256 match into the blob cache.
/// Links, special files, unlisted paths and mismatched content are skipped
/// without error; only an unreadable archive or an over-large one fails.
pub(super) fn extract_archive_to_cache<R: Read>(
    reader: R,
    cache_root: &Path,
    revision: &str,
    wanted: &Wanted,
) -> Result<usize, AppError> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(reader));
    let unreadable =
        |error: std::io::Error| AppError::Source(format!("source archive is unreadable: {error}"));
    let mut stored = HashSet::new();
    let mut declared_total = 0_u64;
    for entry in archive.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        declared_total = declared_total.saturating_add(entry.size());
        if declared_total > MAX_ARCHIVE_UNPACKED_BYTES {
            return Err(AppError::Source(
                "source archive exceeds its unpacked size limit".into(),
            ));
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        // The first component is the archive's own `<name>-<commit>` folder.
        let Some(rest) = std::str::from_utf8(&entry.path_bytes())
            .ok()
            .and_then(|path| path.split_once('/').map(|(_, rest)| rest.to_owned()))
        else {
            continue;
        };
        let Some((sha256, size)) = wanted.get(&rest) else {
            continue;
        };
        let length = entry.size();
        if size.is_some_and(|size| length != size) || length > MAX_SOURCE_FILE_BYTES {
            continue;
        }
        let mut bytes = Vec::with_capacity(length.min(64 * 1024 * 1024) as usize);
        (&mut entry)
            .take(length + 1)
            .read_to_end(&mut bytes)
            .map_err(unreadable)?;
        if bytes.len() as u64 != length || &sha256_bytes(&bytes) != sha256 {
            continue;
        }
        if stored.contains(sha256) {
            continue;
        }
        if atomic_write(&blob_cache_path(cache_root, revision, sha256), &bytes).is_ok() {
            stored.insert(sha256.clone());
        }
    }
    Ok(stored.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    fn tar_gz(entries: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data, kind) in entries {
            let mut header = tar::Header::new_gnu();
            // Written as raw bytes so a hostile name reaches the reader intact.
            header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
            header.set_size(if *kind == tar::EntryType::Regular {
                data.len() as u64
            } else {
                0
            });
            header.set_entry_type(*kind);
            header.set_mode(0o644);
            if *kind == tar::EntryType::Symlink {
                header.as_old_mut().linkname[..data.len()].copy_from_slice(data);
            }
            header.set_cksum();
            let body: &[u8] = if *kind == tar::EntryType::Regular {
                data
            } else {
                &[]
            };
            builder.append(&header, body).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap()
    }

    fn wanted(items: &[(&str, &[u8])]) -> Wanted {
        items
            .iter()
            .map(|(path, data)| {
                (
                    (*path).to_owned(),
                    (sha256_bytes(data), Some(data.len() as u64)),
                )
            })
            .collect()
    }

    #[test]
    fn archive_entries_are_cached_only_when_size_and_hash_match() {
        let good: &[u8] = b"print('ok')\n";
        let other: &[u8] = b"declared elsewhere";
        let archive = tar_gz(&[
            ("kit-0123", b"", tar::EntryType::Directory),
            ("kit-0123/.claude/a.md", good, tar::EntryType::Regular),
            // Same path, content different from the manifest's hash.
            ("kit-0123/b.md", b"tampered", tar::EntryType::Regular),
            // Declared with another size.
            (
                "kit-0123/c.md",
                b"longer than declared",
                tar::EntryType::Regular,
            ),
            // Not declared by the manifest.
            ("kit-0123/extra.md", b"unlisted", tar::EntryType::Regular),
            // A link at a declared path is never followed or stored.
            ("kit-0123/link.md", b"a.md", tar::EntryType::Symlink),
            // Hostile names are only ever lookup keys; nothing is extracted.
            ("kit-0123/../../evil.md", other, tar::EntryType::Regular),
        ]);
        let mut manifest = wanted(&[(".claude/a.md", good), ("b.md", b"expected b")]);
        manifest.insert("c.md".into(), (sha256_bytes(b"short"), Some(5)));
        manifest.insert("link.md".into(), (sha256_bytes(b"a.md"), Some(4)));
        manifest.insert("../evil.md".into(), (sha256_bytes(other), None));
        let cache = tempfile::tempdir().unwrap();
        let stored =
            extract_archive_to_cache(archive.as_slice(), cache.path(), REVISION, &manifest)
                .unwrap();
        assert_eq!(stored, 1);
        let blobs = cache.path().join("blobs").join(REVISION);
        let names = fs::read_dir(&blobs)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![sha256_bytes(good)]);
        assert_eq!(fs::read(blobs.join(sha256_bytes(good))).unwrap(), good);
        // Nothing was written next to or above the cache.
        assert!(!cache.path().join("evil.md").exists());
        assert!(!cache.path().parent().unwrap().join("evil.md").exists());
    }

    #[test]
    fn two_paths_with_one_hash_store_one_blob() {
        let data: &[u8] = b"shared bytes";
        let archive = tar_gz(&[
            ("kit/one.txt", data, tar::EntryType::Regular),
            ("kit/two.txt", data, tar::EntryType::Regular),
        ]);
        let cache = tempfile::tempdir().unwrap();
        let manifest = wanted(&[("one.txt", data), ("two.txt", data)]);
        let stored =
            extract_archive_to_cache(archive.as_slice(), cache.path(), REVISION, &manifest)
                .unwrap();
        assert_eq!(stored, 1);
    }

    #[test]
    fn a_truncated_archive_is_an_error_not_a_partial_success() {
        let data: &[u8] = &[7_u8; 200_000];
        let archive = tar_gz(&[("kit/big.bin", data, tar::EntryType::Regular)]);
        let cache = tempfile::tempdir().unwrap();
        let manifest = wanted(&[("big.bin", data)]);
        let result = extract_archive_to_cache(
            &archive[..archive.len() / 2],
            cache.path(),
            REVISION,
            &manifest,
        );
        assert!(result.is_err());
        // The unverified half-entry never reached the cache.
        assert!(!cache
            .path()
            .join("blobs")
            .join(REVISION)
            .join(sha256_bytes(data))
            .exists());
    }

    #[test]
    fn only_the_fixed_repository_commit_archive_is_approved() {
        let url = |text: &str| reqwest::Url::parse(text).unwrap();
        let good = archive_url(SOURCE_OWNER, SOURCE_NAME, REVISION);
        assert!(approved_source_url(&url(&good)));
        for bad in [
            format!("https://codeload.github.com/attacker/{SOURCE_NAME}/tar.gz/{REVISION}"),
            format!("https://codeload.github.com/{SOURCE_OWNER}/{SOURCE_NAME}/tar.gz/main"),
            format!("https://codeload.github.com/{SOURCE_OWNER}/{SOURCE_NAME}/zip/{REVISION}"),
            format!("https://codeload.github.com/{SOURCE_OWNER}/{SOURCE_NAME}/tar.gz/{REVISION}?x=1"),
            format!("http://codeload.github.com/{SOURCE_OWNER}/{SOURCE_NAME}/tar.gz/{REVISION}"),
            format!("https://codeload.github.com.evil.invalid/{SOURCE_OWNER}/{SOURCE_NAME}/tar.gz/{REVISION}"),
            format!("https://user@codeload.github.com/{SOURCE_OWNER}/{SOURCE_NAME}/tar.gz/{REVISION}"),
        ] {
            assert!(!approved_source_url(&url(&bad)), "{bad}");
        }
    }

    /// A tiny HTTP server: the first request sends half the body and drops the
    /// connection, the second must ask for the rest with a Range header.
    #[test]
    fn an_interrupted_archive_download_resumes_from_the_partial_file() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;
        let body: Vec<u8> = (0..200_000_u32).map(|n| (n % 251) as u8).collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let served = body.clone();
        let ranges = std::sync::Arc::new(Mutex::new(Vec::<Option<String>>::new()));
        let seen = ranges.clone();
        let server = std::thread::spawn(move || {
            for (index, stream) in listener.incoming().take(2).enumerate() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut range = None;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: ") {
                        range = Some(value.trim().to_owned());
                    }
                }
                seen.lock().unwrap().push(range.clone());
                if index == 0 {
                    let half = served.len() / 2;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        served.len()
                    )
                    .unwrap();
                    stream.write_all(&served[..half]).unwrap();
                } else {
                    let start = range
                        .unwrap()
                        .strip_prefix("bytes=")
                        .unwrap()
                        .trim_end_matches('-')
                        .parse::<usize>()
                        .unwrap();
                    write!(
                        stream,
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nConnection: close\r\n\r\n",
                        served.len() - start,
                        served.len() - 1,
                        served.len()
                    )
                    .unwrap();
                    stream.write_all(&served[start..]).unwrap();
                }
            }
        });
        let cache = tempfile::tempdir().unwrap();
        let part = cache.path().join("archives").join("x.tar.gz.part");
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut sleeps = 0;
        download_archive(&client, &format!("http://{address}/a"), &part, &mut |_| {
            sleeps += 1
        })
        .unwrap();
        server.join().unwrap();
        assert_eq!(fs::read(&part).unwrap(), body);
        assert_eq!(sleeps, 1);
        let ranges = ranges.lock().unwrap();
        assert_eq!(ranges[0], None);
        assert_eq!(ranges[1].as_deref(), Some("bytes=100000-"));
    }

    /// Manual measurement against the live repository (network):
    /// cargo test live_archive_vs_single_files -- --ignored --nocapture
    #[test]
    #[ignore = "downloads the live kit twice"]
    fn live_archive_vs_single_files() {
        let request = SourceRequest {
            mode: crate::models::SourceMode::Latest,
            requested_ref: None,
            release: None,
        };
        let probe = tempfile::tempdir().unwrap();
        let client = HttpSourceClient::for_cache_root(probe.path().to_path_buf()).unwrap();
        let resolution = resolve_source(&client, &request).unwrap();
        let revision = resolution.identity.resolved_revision.clone();
        let tree = client.fetch_tree(&revision).unwrap();
        let all = resolution
            .manifest
            .components
            .iter()
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        let files = select_component_files(&resolution.manifest, &all, &tree).unwrap();
        let list = files
            .iter()
            .map(|file| {
                (
                    file.source_path.clone(),
                    file.expected_sha256.clone().unwrap(),
                    file.expected_size,
                )
            })
            .collect::<Vec<_>>();
        let unique = list
            .iter()
            .map(|(_, sha, _)| sha.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len();

        // Archive route, cold cache.
        let cache = tempfile::tempdir().unwrap();
        let client = HttpSourceClient::for_cache_root(cache.path().to_path_buf()).unwrap();
        let wanted: Wanted = list
            .iter()
            .map(|(path, sha, size)| (path.clone(), (sha.clone(), *size)))
            .collect();
        let started = Instant::now();
        let stored = client.prefetch_from_archive(&revision, &wanted).unwrap();
        let archive_seconds = started.elapsed().as_secs_f64();
        let missing = list
            .iter()
            .filter(|(_, sha, _)| !blob_cache_path(cache.path(), &revision, sha).is_file())
            .count();

        // Per-file route, cold cache (the previous behaviour).
        let cache = tempfile::tempdir().unwrap();
        let client = HttpSourceClient::for_cache_root(cache.path().to_path_buf()).unwrap();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let mut seen = std::collections::BTreeSet::new();
        let single = list
            .iter()
            .filter(|(_, sha, _)| seen.insert(sha.clone()))
            .collect::<Vec<_>>();
        let started = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((path, sha, size)) = single.get(index) else {
                        break;
                    };
                    let _ = client.fetch_verified_file(&revision, path, sha, *size);
                });
            }
        });
        let single_seconds = started.elapsed().as_secs_f64();
        eprintln!(
            "revision {revision}: {} declared paths, {unique} unique files
             archive: 1 request, {archive_seconds:.1}s, {stored} blobs stored, {missing} left for the per-file route
             single files: {unique} requests, {single_seconds:.1}s",
            list.len()
        );
    }
}
