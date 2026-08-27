//! Discovery and packaging for an existing project's flattened ChatGPT sources.
//!
//! This export is deliberately separate from installation transactions. It
//! reads only the already-installed, root-contained project inputs and writes
//! one user-requested archive outside the project. The archive contains the
//! same flattened names used by the optional ChatGPT client package.

use crate::security::{
    canonical_relative_key, contains_credential_shaped_content, is_link_metadata, sha256_bytes,
    validate_external_destination,
};
use crate::{flatten, AppError};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_PACKAGE_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSourceFile {
    pub id: String,
    pub source_path: String,
    pub archive_path: String,
    pub category: String,
    pub size: u64,
    pub required: bool,
    pub selected_by_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSourcesPreview {
    pub eligible: bool,
    pub message: Option<String>,
    pub project_root: String,
    pub destination_directory: String,
    pub archive_name: String,
    pub files: Vec<ChatSourceFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSourcesPackageResult {
    pub archive_path: String,
    pub included_files: Vec<String>,
    pub bytes: u64,
    pub sha256: String,
}

/// Use the selected mod directory name for archives when the project was not
/// created by HOI4 Mod Setup and therefore has no installation lock.
pub fn project_id_from_root(project_root: &Path) -> Result<String, AppError> {
    let name = project_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            AppError::InvalidInput("the project folder has no usable archive name".into())
        })?;
    crate::security::normalize_relative_path(name)
}

#[derive(Debug, Clone)]
struct Candidate {
    file: ChatSourceFile,
}

#[derive(Debug, Clone)]
struct ArchiveEntry {
    name: String,
    bytes: Vec<u8>,
}

pub fn preview(
    project_root: &Path,
    project_id: &str,
    destination_directory: String,
) -> Result<ChatSourcesPreview, AppError> {
    let candidates = discover_candidates(project_root)?;
    let has_primary_source = candidates.iter().any(|candidate| {
        matches!(
            candidate.file.category.as_str(),
            "instructions" | "readme" | "skill" | "subagent"
        )
    });
    let eligible = has_primary_source;
    let message = if eligible {
        None
    } else {
        Some("No AGENTS.md, README.md, skill, or subagent source was found in this project.".into())
    };
    Ok(ChatSourcesPreview {
        eligible,
        message,
        project_root: crate::paths::user_facing_path(project_root),
        destination_directory,
        archive_name: archive_name(project_id)?,
        files: candidates
            .into_iter()
            .map(|candidate| candidate.file)
            .collect(),
    })
}

pub fn package(
    project_root: &Path,
    project_id: &str,
    destination_directory: &Path,
    selected_file_ids: &[String],
) -> Result<ChatSourcesPackageResult, AppError> {
    let candidates = discover_candidates(project_root)?;
    if !has_primary_candidates(&candidates) {
        return Err(AppError::InvalidInput(
            "ChatGPT source packaging requires at least one AGENTS.md, README.md, skill, or subagent source".into(),
        ));
    }
    let required = candidates
        .iter()
        .filter(|candidate| candidate.file.required)
        .map(|candidate| candidate.file.id.clone())
        .collect::<BTreeSet<_>>();
    let available = candidates
        .iter()
        .map(|candidate| candidate.file.id.clone())
        .collect::<BTreeSet<_>>();
    let selected = selected_file_ids.iter().cloned().collect::<BTreeSet<_>>();
    if !required.is_subset(&selected) {
        return Err(AppError::InvalidInput(
            "required ChatGPT source files cannot be excluded".into(),
        ));
    }
    if !selected.is_subset(&available) {
        return Err(AppError::InvalidInput(
            "the selected ChatGPT source list is stale; refresh it and try again".into(),
        ));
    }

    let mut entries = BTreeMap::<String, ArchiveEntry>::new();
    for candidate in candidates {
        if !selected.contains(&candidate.file.id) {
            continue;
        }
        let bytes = flatten::read_regular_file_no_follow_under_root(
            project_root,
            &candidate.file.source_path,
        )?;
        validate_text_source(
            &candidate.file.source_path,
            &candidate.file.archive_path,
            &bytes,
        )?;
        let key = canonical_relative_key(&candidate.file.archive_path)?;
        if entries.contains_key(&key) {
            return Err(AppError::Transaction(format!(
                "ChatGPT source archive path collision: {}",
                candidate.file.archive_path
            )));
        }
        entries.insert(
            key,
            ArchiveEntry {
                name: candidate.file.archive_path,
                bytes,
            },
        );
    }
    if entries.is_empty() {
        return Err(AppError::InvalidInput(
            "select at least one ChatGPT source file".into(),
        ));
    }

    let entries = entries.into_values().collect::<Vec<_>>();
    let archive = build_zip(&entries)?;
    let destination_directory = crate::paths::validate_export_directory(destination_directory)?;
    let archive_path = next_archive_path(&destination_directory, project_id)?;
    write_new_archive(&archive_path, &archive)?;
    Ok(ChatSourcesPackageResult {
        archive_path: crate::paths::user_facing_path(&archive_path),
        included_files: entries.into_iter().map(|entry| entry.name).collect(),
        bytes: archive.len() as u64,
        sha256: sha256_bytes(&archive),
    })
}

fn has_primary_candidates(candidates: &[Candidate]) -> bool {
    candidates.iter().any(|candidate| {
        matches!(
            candidate.file.category.as_str(),
            "instructions" | "readme" | "skill" | "subagent"
        )
    })
}

fn discover_candidates(project_root: &Path) -> Result<Vec<Candidate>, AppError> {
    let mut candidates = Vec::new();
    add_candidate(
        project_root,
        &mut candidates,
        "AGENTS.md",
        "AGENTS.md",
        "instructions",
        true,
    )?;
    add_candidate(
        project_root,
        &mut candidates,
        "README.md",
        "README.md",
        "readme",
        true,
    )?;

    let skills_root = project_root.join(".agents/skills");
    if !path_is_link(&skills_root)? && skills_root.is_dir() {
        for entry in fs::read_dir(&skills_root)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if is_link_metadata(&metadata) || !metadata.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                return Err(AppError::PathSecurity(
                    "a skill directory has a non-Unicode name".into(),
                ));
            };
            let source = format!(".agents/skills/{name}/SKILL.md");
            add_candidate(
                project_root,
                &mut candidates,
                &source,
                &format!("{name}.md"),
                "skill",
                true,
            )?;
        }
    }

    let subagents_root = project_root.join(".codex/agents");
    if !path_is_link(&subagents_root)? && subagents_root.is_dir() {
        for entry in fs::read_dir(&subagents_root)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if is_link_metadata(&metadata) || !metadata.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                return Err(AppError::PathSecurity(
                    "a subagent file has a non-Unicode name".into(),
                ));
            };
            if !name.to_ascii_lowercase().ends_with(".toml") {
                continue;
            }
            let source = format!(".codex/agents/{name}");
            add_candidate(
                project_root,
                &mut candidates,
                &source,
                name,
                "subagent",
                true,
            )?;
        }
    }

    for entry in fs::read_dir(project_root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if is_link_metadata(&metadata) || !metadata.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.eq_ignore_ascii_case("AGENTS.md") || name.eq_ignore_ascii_case("README.md") {
            continue;
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            add_candidate(
                project_root,
                &mut candidates,
                name,
                name,
                "root_markdown",
                false,
            )?;
        }
    }

    candidates.sort_by(|left, right| left.file.archive_path.cmp(&right.file.archive_path));
    validate_candidate_collisions(&candidates)?;
    if candidates.len() > MAX_PACKAGE_FILES {
        return Err(AppError::InvalidInput(
            "ChatGPT sources exceed the aggregate file limit".into(),
        ));
    }
    Ok(candidates)
}

fn add_candidate(
    project_root: &Path,
    candidates: &mut Vec<Candidate>,
    source_path: &str,
    archive_path: &str,
    category: &str,
    required: bool,
) -> Result<(), AppError> {
    let path = crate::security::safe_join(project_root, source_path)?;
    if !path.exists() {
        return Ok(());
    }
    let bytes = flatten::read_regular_file_no_follow_under_root(project_root, source_path)?;
    validate_text_source(source_path, archive_path, &bytes)?;
    candidates.push(Candidate {
        file: ChatSourceFile {
            id: source_path.into(),
            source_path: source_path.into(),
            archive_path: archive_path.into(),
            category: category.into(),
            size: bytes.len() as u64,
            required,
            selected_by_default: required,
        },
    });
    Ok(())
}

fn validate_text_source(
    source_path: &str,
    archive_path: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "ChatGPT source exceeds the {} MiB per-file limit: {archive_path}",
            MAX_FILE_BYTES / 1024 / 1024
        )));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        AppError::Serialization(format!("ChatGPT source is not valid UTF-8: {source_path}"))
    })?;
    if contains_credential_shaped_content(text) {
        return Err(AppError::Credential(format!(
            "ChatGPT source contains secret-shaped content: {archive_path}"
        )));
    }
    Ok(())
}

fn validate_candidate_collisions(candidates: &[Candidate]) -> Result<(), AppError> {
    let mut seen = BTreeMap::<String, String>::new();
    for candidate in candidates {
        let key = canonical_relative_key(&candidate.file.archive_path)?;
        if let Some(existing) = seen.insert(key, candidate.file.source_path.clone()) {
            if existing != candidate.file.source_path {
                // Optional root Markdown can legitimately have the same name
                // as a flattened skill. The package command rejects the
                // collision only when both entries are selected.
                continue;
            }
        }
    }
    Ok(())
}

fn path_is_link(path: &Path) -> Result<bool, AppError> {
    Ok(fs::symlink_metadata(path)
        .map(|metadata| is_link_metadata(&metadata))
        .unwrap_or(false))
}

fn archive_name(project_id: &str) -> Result<String, AppError> {
    let normalized = crate::security::normalize_relative_path(project_id)?;
    if normalized.contains('/') {
        return Err(AppError::InvalidInput(
            "project ID is not a valid archive name".into(),
        ));
    }
    Ok(format!("{normalized}-chatgpt-project-sources.zip"))
}

fn next_archive_path(destination_directory: &Path, project_id: &str) -> Result<PathBuf, AppError> {
    let base = archive_name(project_id)?;
    for index in 0..1000_u32 {
        let filename = if index == 0 {
            base.clone()
        } else {
            format!("{}-{index}.zip", base.trim_end_matches(".zip"))
        };
        let candidate = destination_directory.join(filename);
        validate_external_destination(&candidate.to_string_lossy())?;
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::Transaction(
        "could not find an unused ChatGPT source archive name".into(),
    ))
}

fn write_new_archive(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Transaction("ChatGPT source archive has no parent".into()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "archive destination already exists",
            ));
        }
        fs::rename(&temporary, path)?;
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| {
        AppError::Transaction(format!(
            "could not write ChatGPT source archive {}: {error}",
            path.display()
        ))
    })
}

fn build_zip(entries: &[ArchiveEntry]) -> Result<Vec<u8>, AppError> {
    let total_input = entries.iter().try_fold(0_u64, |total, entry| {
        total.checked_add(entry.bytes.len() as u64)
    });
    if total_input.is_none_or(|total| total > MAX_TOTAL_BYTES) {
        return Err(AppError::InvalidInput(
            "ChatGPT sources exceed the aggregate size limit".into(),
        ));
    }
    let mut archive = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let name = entry.name.as_bytes();
        if name.len() > u16::MAX as usize {
            return Err(AppError::InvalidInput(
                "ChatGPT source archive path is too long".into(),
            ));
        }
        let offset = archive.len() as u32;
        let crc = crc32(&entry.bytes);
        write_u32(&mut archive, 0x0403_4b50);
        write_u16(&mut archive, 20);
        write_u16(&mut archive, 0x0800);
        write_u16(&mut archive, 0);
        write_u16(&mut archive, 0);
        write_u16(&mut archive, 0);
        write_u32(&mut archive, crc);
        write_u32(&mut archive, entry.bytes.len() as u32);
        write_u32(&mut archive, entry.bytes.len() as u32);
        write_u16(&mut archive, name.len() as u16);
        write_u16(&mut archive, 0);
        archive.extend_from_slice(name);
        archive.extend_from_slice(&entry.bytes);

        write_u32(&mut central, 0x0201_4b50);
        write_u16(&mut central, 20);
        write_u16(&mut central, 20);
        write_u16(&mut central, 0x0800);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u32(&mut central, crc);
        write_u32(&mut central, entry.bytes.len() as u32);
        write_u32(&mut central, entry.bytes.len() as u32);
        write_u16(&mut central, name.len() as u16);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u32(&mut central, 0);
        write_u32(&mut central, offset);
        central.extend_from_slice(name);
    }
    let central_offset = archive.len() as u32;
    archive.extend_from_slice(&central);
    if entries.len() > u16::MAX as usize || central.len() > u32::MAX as usize {
        return Err(AppError::InvalidInput(
            "ChatGPT source archive is too large".into(),
        ));
    }
    write_u32(&mut archive, 0x0605_4b50);
    write_u16(&mut archive, 0);
    write_u16(&mut archive, 0);
    write_u16(&mut archive, entries.len() as u16);
    write_u16(&mut archive, entries.len() as u16);
    write_u32(&mut archive, central.len() as u32);
    write_u32(&mut archive, central_offset);
    write_u16(&mut archive, 0);
    Ok(archive)
}

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn project() -> tempfile::TempDir {
        let project = tempdir().unwrap();
        fs::create_dir_all(project.path().join(".agents/skills/events")).unwrap();
        fs::create_dir_all(project.path().join(".codex/agents")).unwrap();
        fs::write(project.path().join("AGENTS.md"), "instructions").unwrap();
        fs::write(project.path().join("README.md"), "readme").unwrap();
        fs::write(
            project.path().join(".agents/skills/events/SKILL.md"),
            "events skill",
        )
        .unwrap();
        fs::write(
            project.path().join(".codex/agents/researcher.toml"),
            "name = 'researcher'",
        )
        .unwrap();
        fs::write(project.path().join("NOTES.md"), "optional notes").unwrap();
        project
    }

    #[test]
    fn preview_selects_installed_flattenable_inputs_and_disables_root_notes() {
        let project = project();
        let preview = preview(
            project.path(),
            "sample_mod",
            project.path().to_string_lossy().into(),
        )
        .unwrap();
        assert!(preview.eligible);
        assert_eq!(
            preview
                .files
                .iter()
                .filter(|file| file.selected_by_default)
                .count(),
            4
        );
        assert!(preview
            .files
            .iter()
            .any(|file| file.archive_path == "events.md"));
        assert!(preview
            .files
            .iter()
            .any(|file| file.archive_path == "researcher.toml"));
        assert!(preview
            .files
            .iter()
            .any(|file| file.archive_path == "NOTES.md" && !file.selected_by_default));
    }

    #[test]
    fn package_requires_required_files_and_keeps_project_unchanged() {
        let project = project();
        let destination = tempdir().unwrap();
        let preview = preview(
            project.path(),
            "sample_mod",
            destination.path().to_string_lossy().into(),
        )
        .unwrap();
        let mut selected = preview
            .files
            .iter()
            .filter(|file| file.selected_by_default)
            .map(|file| file.id.clone())
            .collect::<Vec<_>>();
        selected.push("NOTES.md".into());
        let result = package(project.path(), "sample_mod", destination.path(), &selected).unwrap();
        assert!(Path::new(&result.archive_path).is_file());
        assert!(result.included_files.contains(&"events.md".into()));
        assert!(result.included_files.contains(&"NOTES.md".into()));
        assert!(project.path().join("AGENTS.md").is_file());
        let archive = fs::read(&result.archive_path).unwrap();
        assert!(archive.starts_with(b"PK\x03\x04"));
        assert!(archive
            .windows(b"NOTES.md".len())
            .any(|window| window == b"NOTES.md"));
        assert_eq!(
            &result.sha256,
            &sha256_bytes(&fs::read(&result.archive_path).unwrap())
        );
    }

    #[test]
    fn partial_existing_project_without_lock_can_package_available_sources() {
        let project = tempdir().unwrap();
        fs::create_dir_all(project.path().join(".agents/skills/events")).unwrap();
        fs::create_dir_all(project.path().join(".codex/agents")).unwrap();
        fs::write(
            project.path().join(".agents/skills/events/SKILL.md"),
            "events skill",
        )
        .unwrap();
        fs::write(
            project.path().join(".codex/agents/researcher.toml"),
            "name = 'researcher'",
        )
        .unwrap();
        fs::write(project.path().join("NOTES.md"), "optional notes").unwrap();
        let destination = tempdir().unwrap();

        let preview = preview(
            project.path(),
            &project_id_from_root(project.path()).unwrap(),
            destination.path().to_string_lossy().into(),
        )
        .unwrap();
        assert!(preview.eligible);
        assert_eq!(
            preview
                .files
                .iter()
                .filter(|file| file.selected_by_default)
                .count(),
            2
        );
        let selected = preview
            .files
            .iter()
            .filter(|file| file.selected_by_default)
            .map(|file| file.id.clone())
            .collect::<Vec<_>>();
        let result = package(
            project.path(),
            &project_id_from_root(project.path()).unwrap(),
            destination.path(),
            &selected,
        )
        .unwrap();
        assert!(result
            .archive_path
            .ends_with("-chatgpt-project-sources.zip"));
        assert!(result.included_files.contains(&"events.md".into()));
        assert!(result.included_files.contains(&"researcher.toml".into()));
        assert!(project
            .path()
            .join(".agents/skills/events/SKILL.md")
            .is_file());
    }

    #[test]
    fn project_id_from_root_rejects_roots_without_a_leaf_name() {
        assert!(project_id_from_root(Path::new("C:\\")).is_err());
    }

    #[test]
    fn package_rejects_secret_shaped_content() {
        let project = project();
        fs::write(
            project.path().join("AGENTS.md"),
            "MESHY_API_KEY=real-secret",
        )
        .unwrap();
        assert!(preview(project.path(), "sample_mod", String::new()).is_err());
    }
}
