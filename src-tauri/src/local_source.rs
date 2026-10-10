//! Developer route that serves the workflow package from a local Git checkout.
//!
//! Package maintainers set `HOI4_MOD_SETUP_SOURCE_REPOSITORY` to the root of a
//! local Agentic HOI4 Modding checkout to test unpublished changes end to end.
//! The source client then answers the small GitHub API and raw-content surface
//! it normally uses from that checkout's committed objects: the default branch
//! is the checkout's current branch, and every file is read from an exact
//! commit with `git show`, so manifest evidence is still verified by SHA-256.
//! Nothing here runs unless the variable names an existing Git checkout.

use crate::AppError;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SOURCE_REPOSITORY_ENV: &str = "HOI4_MOD_SETUP_SOURCE_REPOSITORY";

/// The configured local checkout, when the developer variable names one.
pub fn configured_repository() -> Option<PathBuf> {
    let value = std::env::var_os(SOURCE_REPOSITORY_ENV)?;
    let root = PathBuf::from(value);
    (root.is_absolute() && root.join(".git").exists()).then_some(root)
}

fn git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, AppError> {
    let executable = crate::git::find_git_executable()?;
    let mut command = Command::new(executable);
    command.arg("-C").arg(root).args(arguments);
    crate::process::configure_child_no_console_window(&mut command);
    let output = command
        .output()
        .map_err(|error| AppError::Source(format!("local source Git failed to start: {error}")))?;
    if !output.status.success() {
        return Err(AppError::Source(format!(
            "local source Git command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn git_text(root: &Path, arguments: &[&str]) -> Result<String, AppError> {
    Ok(String::from_utf8_lossy(&git(root, arguments)?)
        .trim()
        .to_string())
}

fn exact_commit(root: &Path, reference: &str) -> Result<String, AppError> {
    let commit = git_text(
        root,
        &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
    )?;
    crate::source::validate_commit(&commit)?;
    Ok(commit.to_ascii_lowercase())
}

fn percent_decode(value: &str) -> Result<String, AppError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes
                .get(index + 1..index + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| {
                    AppError::Source("invalid percent-encoding in source path".into())
                })?;
            decoded.push(hex);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| AppError::Source("source path is not UTF-8".into()))
}

fn tree_json(root: &Path, revision: &str) -> Result<Value, AppError> {
    let listing = git(
        root,
        &["ls-tree", "-r", "-t", "-l", "-z", "--full-tree", revision],
    )?;
    let mut entries = Vec::new();
    for record in listing
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = String::from_utf8_lossy(record);
        let Some((metadata, path)) = record.split_once('\t') else {
            continue;
        };
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 {
            continue;
        }
        let mut entry = json!({ "path": path, "type": fields[1] });
        if let Ok(size) = fields[3].parse::<u64>() {
            entry["size"] = json!(size);
        }
        entries.push(entry);
    }
    Ok(json!({ "sha": revision, "truncated": false, "tree": entries }))
}

/// Answer one GitHub API or raw-content URL from the local checkout.
pub fn fetch(root: &Path, url: &reqwest::Url) -> Result<Vec<u8>, AppError> {
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    match url.host_str() {
        Some("api.github.com") => {
            if segments.len() < 3 || segments[0] != "repos" {
                return Err(AppError::Source(format!(
                    "local source cannot answer {url}"
                )));
            }
            let rest = &segments[3..];
            let value = match rest {
                [] => {
                    let branch = git_text(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
                    json!({ "default_branch": if branch == "HEAD" { "local".to_string() } else { branch } })
                }
                ["branches", branch] => {
                    let branch = percent_decode(branch)?;
                    let reference = if branch == "local" { "HEAD".to_string() } else { branch };
                    json!({ "commit": { "sha": exact_commit(root, &reference)? } })
                }
                ["git", "commits", revision] => json!({ "sha": exact_commit(root, revision)? }),
                ["git", "trees", revision] => tree_json(root, &exact_commit(root, revision)?)?,
                _ => {
                    return Err(AppError::Source(format!(
                        "the local source supports the latest commit and pinned commits only, not {url}"
                    )))
                }
            };
            Ok(serde_json::to_vec(&value).expect("local source JSON serializes"))
        }
        Some("raw.githubusercontent.com") if segments.len() >= 4 => {
            let revision = exact_commit(root, segments[2])?;
            let path = segments[3..]
                .iter()
                .map(|segment| percent_decode(segment))
                .collect::<Result<Vec<_>, _>>()?
                .join("/");
            git(root, &["show", &format!("{revision}:{path}")])
        }
        _ => Err(AppError::Source(format!(
            "local source cannot answer {url}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decoding_restores_spaces_and_rejects_bad_escapes() {
        assert_eq!(percent_decode("a%20b").unwrap(), "a b");
        assert_eq!(percent_decode("plain").unwrap(), "plain");
        assert!(percent_decode("bad%zz").is_err());
    }

    #[test]
    fn a_relative_or_missing_checkout_is_never_used() {
        std::env::set_var(SOURCE_REPOSITORY_ENV, "relative/path");
        assert!(configured_repository().is_none());
        std::env::remove_var(SOURCE_REPOSITORY_ENV);
        assert!(configured_repository().is_none());
    }

    /// Manual check: HOI4_MOD_SETUP_SOURCE_REPOSITORY=<checkout> cargo test
    /// local_source_serves_every_declared_file -- --ignored --nocapture
    #[test]
    #[ignore = "needs a local package checkout"]
    fn local_source_serves_every_declared_file() {
        let Some(_) = configured_repository() else {
            eprintln!("{SOURCE_REPOSITORY_ENV} is not set; skipped");
            return;
        };
        let client = crate::source::HttpSourceClient::new().unwrap();
        let request = crate::source::SourceRequest {
            mode: crate::models::SourceMode::Latest,
            requested_ref: None,
            release: None,
        };
        let resolution = crate::source::resolve_source(&client, &request).unwrap();
        let revision = resolution.identity.resolved_revision.clone();
        let tree = client.fetch_tree(&revision).unwrap();
        let all = resolution
            .manifest
            .components
            .iter()
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        let files = crate::source::select_component_files(&resolution.manifest, &all, &tree);
        let files = match files {
            Ok(files) => files,
            Err(error) => panic!("selection failed: {error}"),
        };
        let mut failures = 0;
        for file in &files {
            let sha = file.expected_sha256.clone().unwrap();
            if let Err(error) =
                client.fetch_verified_file(&revision, &file.source_path, &sha, file.expected_size)
            {
                failures += 1;
                eprintln!("{}: {error}", file.source_path);
            }
        }
        eprintln!(
            "{} files, {failures} failures, revision {revision}",
            files.len()
        );
        assert_eq!(failures, 0);
    }
}
